//! O que a aplicação segura: [`Conexao`], [`Envio`] e [`Recebe`].
//!
//! Nenhum destes tipos toca no `Rtc`: eles falam com a tarefa da [`crate::Ponta`]
//! por comandos, e a tarefa é a única que muta o str0m (a regra de uma mutação
//! seguida de esvaziar o `poll_output`, da documentação do str0m).
//!
//! # Contrapressão
//!
//! - **Saída:** cada canal tem [`crate::CREDITO_DE_SAIDA`] bytes de crédito.
//!   `write_all` espera crédito, e ele volta quando a mensagem entra no SCTP.
//!   Por baixo, o escalonador da ponta só põe volume no SCTP enquanto a
//!   associação tem pouco esperando (R4 da revisão: a voz não espera atrás de
//!   um anexo).
//! - **Entrada:** cada canal guarda até [`crate::MAX_FILA_DE_ENTRADA`] bytes não
//!   lidos. Quem estoura é cortado ([`crate::CORTE_FILA`]) e contado.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use str0m::channel::ChannelId;
use tokio::sync::{mpsc, oneshot, watch, OwnedSemaphorePermit, Semaphore};

/// Por que uma conexão acabou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// O par fechou, com o código que mandou no corte do `ctl`.
    Fechada {
        /// O código do par.
        codigo: u32,
    },
    /// Caiu: ocioso, DTLS, ICE, ou o par sumiu sem dizer.
    Caiu(String),
    /// Este lado fechou.
    Local,
    /// Quem abre um canal bateu no teto de [`crate::MAX_CANAIS`] canais vivos
    /// por conexão. **Só a abertura é recusada**: a conexão segue de pé. É o
    /// que o servidor traduz em `RecusaDeTunel::CanaisEsgotados`.
    CanaisEsgotados,
}

impl std::fmt::Display for Motivo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fechada { codigo } => write!(f, "o par fechou a conexão (código {codigo})"),
            Self::Caiu(porque) => write!(f, "a conexão caiu: {porque}"),
            Self::Local => write!(f, "a conexão foi fechada deste lado"),
            Self::CanaisEsgotados => {
                write!(f, "o teto de {} canais por conexão", crate::MAX_CANAIS)
            }
        }
    }
}

impl std::error::Error for Motivo {}

/// Por que um canal parou.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FalhaDoCanal {
    /// O fluxo acabou (`fim`) antes do que se pedia, ou já tinha acabado.
    #[error("o fluxo terminou")]
    Fim,
    /// O par cortou o fluxo, com um código.
    #[error("o par cortou o fluxo (código {0})")]
    Cortado(u32),
    /// A conexão, ou o canal, caiu.
    #[error("{0}")]
    Caiu(Motivo),
}

/// Por que um datagrama (de voz, ou do `j` do túnel) não saiu. Nunca espera:
/// voz ou jogo atrasado não serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Recusa {
    /// Acima do teto do canal: [`crate::MAX_VOZ`] na voz, [`crate::MAX_DADOS`]
    /// no `j`. No `j` é contado em `Contadores::tunel_grande_demais`.
    #[error("datagrama acima do teto do canal")]
    Grande,
    /// A fila da tarefa está cheia (a de voz, ou a do `j`: [`crate::FILA_DE_JOGO`]).
    /// No `j` é contado em `Contadores::tunel_descartado`.
    #[error("a fila de datagramas está cheia")]
    Cheia,
    /// A conexão acabou.
    #[error("a conexão acabou")]
    Fechada,
}

/// O que a conexão contou do caminho.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Leitura {
    /// Bytes UDP que saíram para o par.
    pub bytes_enviados: u64,
    /// Datagramas UDP que saíram para o par.
    pub pacotes_enviados: u64,
    /// Bytes UDP que chegaram do par.
    pub bytes_recebidos: u64,
    /// O ida e volta que o ICE mediu, quando mediu (`None` antes do primeiro
    /// check respondido). Desde 26/10/2026 os dois lados checam: o anfitrião é
    /// ICE completo e controlado, e não mais ICE-lite.
    pub ida_e_volta: Option<Duration>,
    /// Pacotes perdidos no caminho. **Sempre `None`**: o str0m 0.24.1 não expõe as
    /// retransmissões nem as perdas do SCTP (`stats.rs` só as conta para RTP, e
    /// este produto não manda RTP). `None` é «não sei», e nunca «zero».
    pub pacotes_perdidos: Option<u64>,
    /// Eventos de congestionamento (cortes de `cwnd`). **Sempre `None`**, pelo
    /// mesmo motivo: o `cwnd` do `sctp-proto` não sai do str0m.
    pub eventos_de_congestionamento: Option<u64>,
    /// A fração de perda na saída que o str0m calcula (`PeerStats::egress_loss_fraction`).
    /// Vem dos relatórios RTCP, então sem mídia RTP fica em `None`.
    pub fracao_de_perda_na_saida: Option<f32>,
    /// A fração de perda na **entrada** (`PeerStats::ingress_loss_fraction`): do
    /// que o par mandou por RTP, quanto não chegou aqui, desde a estatística
    /// anterior. É a perda **antes** da retransmissão — o que o caminho tirou, e
    /// não o que faltou no quadro. Sem mídia RTP chegando fica em `None`.
    pub fracao_de_perda_na_entrada: Option<f32>,
}

/// Os contadores de uma conexão: o que foi recusado ou descartado, para quem investiga.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Contadores {
    /// Canais que o par abriu e foram fechados ao chegar: label ou protocolo
    /// desconhecido, ou acima do teto.
    pub canais_recusados: u64,
    /// Canais cortados por estourar a fila de entrada.
    pub cortes_por_fila: u64,
    /// Mensagens que não eram envelope, ou fora do lugar.
    pub violacoes: u64,
    /// Datagramas de voz que chegaram e não couberam na fila, ou eram grandes demais.
    pub voz_descartada: u64,
    /// Datagramas de voz que não saíram (SCTP sem espaço).
    pub voz_nao_saiu: u64,
    /// Quadros de mídia que o `Rtc` não aceitou (a mídia não estava ligada,
    /// ou a conexão não estava pronta).
    pub midia_nao_saiu: u64,
    /// Pacotes ou quadros de mídia que o `str0m` recusou ao empacotar ou
    /// desempacotar: largados, sem derrubar a conexão.
    pub midia_malformada: u64,
    /// O maior volume (canais "u" e "b") que o escalonador já deixou no SCTP na
    /// frente da voz, em bytes. Fica abaixo de `LIMIAR_DE_VOLUME` mais uma
    /// mensagem: é a garantia do R4, medida.
    pub maior_volume_no_sctp: u64,
    /// Datagramas `j` do túnel que não couberam: a fila de saída cheia, o SCTP
    /// sem espaço, ou a fila de entrada cheia. O jogo repete o que perde; o
    /// número serve a quem investiga «o jogo engasga».
    pub tunel_descartado: u64,
    /// Datagramas `j` acima de [`crate::MAX_DADOS`], na saída (recusados) ou na
    /// entrada (descartados).
    pub tunel_grande_demais: u64,
}

#[derive(Default)]
pub(crate) struct ContadoresVivos {
    pub(crate) canais_recusados: AtomicU64,
    pub(crate) cortes_por_fila: AtomicU64,
    pub(crate) violacoes: AtomicU64,
    pub(crate) voz_descartada: AtomicU64,
    pub(crate) voz_nao_saiu: AtomicU64,
    pub(crate) midia_nao_saiu: AtomicU64,
    pub(crate) midia_malformada: AtomicU64,
    pub(crate) maior_volume_no_sctp: AtomicU64,
    pub(crate) tunel_descartado: AtomicU64,
    pub(crate) tunel_grande_demais: AtomicU64,
}

/// O que a tarefa e os punhos de uma conexão dividem.
pub(crate) struct Compartilhado {
    pub(crate) id: usize,
    pub(crate) remoto: Mutex<SocketAddr>,
    pub(crate) impressao_local: [u8; 32],
    pub(crate) impressao_par: OnceLock<[u8; 32]>,
    pub(crate) apresentacao: OnceLock<Vec<u8>>,
    pub(crate) credencial: String,
    pub(crate) motivo: watch::Sender<Option<Motivo>>,
    pub(crate) bytes_enviados: AtomicU64,
    pub(crate) pacotes_enviados: AtomicU64,
    pub(crate) bytes_recebidos: AtomicU64,
    pub(crate) rtt_us: AtomicU64,
    pub(crate) perda_na_saida: Mutex<Option<f32>>,
    pub(crate) perda_na_entrada: Mutex<Option<f32>>,
    pub(crate) voz_na_fila: AtomicUsize,
    /// Os quadros de mídia que esperam a tarefa ([`crate::FILA_DE_MIDIA`]).
    pub(crate) midia_na_fila: AtomicUsize,
    /// A estimativa de banda do RTP na saída, em bps. Zero: nenhuma ainda.
    pub(crate) estimativa_bps: AtomicU64,
    /// As filas de entrada vivas (cada `Recebe` que ainda não chegou ao fim nem
    /// foi largado). Um canal que o par terminou sai do SCTP e de `canais`, mas a
    /// fila dele (até [`crate::MAX_FILA_DE_ENTRADA`]) continua guardada até quem
    /// lê a esvaziar: o teto de canais conta estas também.
    pub(crate) filas_vivas: AtomicUsize,
    /// A soma, na última revisão, do que os canais de volume seguram (fila não
    /// lida mais crédito concedido e não usado). Só a [`crate::teste`] lê.
    pub(crate) credito_em_aberto: AtomicU64,
    /// Só a [`crate::teste`]: a próxima mensagem de dados para nesta barreira
    /// entre a soma na fila e o envio, para um teste montar a corrida com quem
    /// larga o `Recebe`. Desarmada, custa uma leitura atômica.
    pub(crate) pausa_de_teste: crate::teste::PausaDeTeste,
    pub(crate) contadores: ContadoresVivos,
}

impl Compartilhado {
    pub(crate) fn motivo_atual(&self) -> Motivo {
        self.motivo
            .borrow()
            .clone()
            .unwrap_or_else(|| Motivo::Caiu("a conexão acabou".into()))
    }
}

/// O que um punho pede à tarefa da ponta.
pub(crate) enum Comando {
    Escrever {
        par: usize,
        canal: ChannelId,
        mensagem: Vec<u8>,
        credito: Option<OwnedSemaphorePermit>,
        depois: Depois,
    },
    Voz {
        par: usize,
        bytes: Vec<u8>,
    },
    Parar {
        par: usize,
        canal: ChannelId,
        codigo: u32,
    },
    /// Declara a mídia RTP do par (`Conexao::ligar_midia`).
    LigarMidia {
        par: usize,
        fila: mpsc::Sender<crate::EventoDeMidia>,
    },
    /// Um quadro de mídia: sai sem esperar.
    Midia {
        par: usize,
        faixa: u8,
        trilha: crate::Trilha,
        tempo: u64,
        bytes: Arc<[u8]>,
    },
    /// Pede um quadro-chave ao par.
    PedirChave {
        par: usize,
        faixa: u8,
    },
    /// Recomeça a estimativa de banda de um valor.
    SemearEstimativa {
        par: usize,
        bps: u64,
    },
    /// Até onde a estimativa de banda deve procurar.
    TaxaDaMidia {
        par: usize,
        desejada_bps: u64,
    },
    /// Quem lê liberou janela: o novo limite absoluto de dados do canal.
    /// Quem lê consumiu (ou largou) parte de uma fila: a tarefa revê o
    /// crédito da conexão.
    Consumiu {
        par: usize,
    },
    /// A aplicação pegou o `Recebe` de um canal que o par abriu: só agora ele
    /// recebe crédito de dados.
    Aceito {
        par: usize,
        canal: ChannelId,
    },
    /// Só para teste (`teste::reter_credito`): segura os anúncios deste lado.
    ReterCredito {
        par: usize,
        reter: bool,
    },
    Abrir {
        par: usize,
        natureza: Natureza,
        resposta: crate::conexao::RespostaDeAbrir,
    },
    /// Quem segurava o `j` largou: o canal fecha, sem envelope (um corte de
    /// `u`/`b` seria lixo para quem lê datagramas crus).
    FecharJogo {
        par: usize,
        canal: ChannelId,
    },
    /// Um datagrama do `j`: sai sem esperar, e o que não couber é contado.
    Jogo {
        par: usize,
        canal: ChannelId,
        bytes: Vec<u8>,
    },
    Fechar {
        par: usize,
        codigo: u32,
    },
    Discar {
        discagem: crate::Discagem,
        resposta: oneshot::Sender<Result<Conexao, crate::FalhaAoDiscar>>,
    },
    /// Um `CHAMADO` que passou pelo teto de furos (`Ponta::chamado`).
    Chamado {
        credencial: String,
        candidatos: Vec<std::net::SocketAddr>,
    },
    Anfitriao(Option<Box<crate::ConfigDoAnfitriao>>),
    Desligar,
}

/// O que um `Comando::Abrir` abre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Natureza {
    Uni,
    Bi,
    /// O `t`: bidirecional como o `b`, mas entregue na fila do túnel.
    Tunel,
    /// O `j`: datagramas, sem envelope, sem ordem e sem retransmissão.
    Jogo,
}

/// O que a abertura de um canal devolve a quem pediu.
pub(crate) enum Aberto {
    Fluxo(Envio, Option<Recebe>),
    Jogo(Datagramas),
}

/// Quem pediu um canal espera os punhos dele, ou o motivo.
pub(crate) type RespostaDeAbrir = oneshot::Sender<Result<Aberto, Motivo>>;

/// O que acontece com o canal depois que esta mensagem entra no SCTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depois {
    Nada,
    Fim,
    Corte,
}

/// O que chega à fila de leitura de um canal.
#[derive(Debug)]
pub(crate) enum Entrada {
    Dados(Vec<u8>),
    Fim,
    Corte(u32),
}

/// O estado de um canal que os dois punhos (e a tarefa) leem.
pub(crate) struct EstadoDoCanal {
    pub(crate) parado: Mutex<Option<FalhaDoCanal>>,
    /// Como terminou a entrega deste sentido: `Ok` quando quem lê confirmou o
    /// fim, `Err` com o corte ou a queda. É o `stopped()` do QUIC.
    entrega: watch::Sender<Option<Result<(), FalhaDoCanal>>>,
}

impl Default for EstadoDoCanal {
    fn default() -> Self {
        Self {
            parado: Mutex::new(None),
            entrega: watch::Sender::new(None),
        }
    }
}

impl EstadoDoCanal {
    pub(crate) fn parar(&self, falha: FalhaDoCanal) {
        if let Ok(mut parado) = self.parado.lock() {
            if parado.is_none() {
                *parado = Some(falha.clone());
            }
        }
        self.entrega.send_if_modified(|entrega| {
            if entrega.is_none() {
                *entrega = Some(Err(falha));
                true
            } else {
                false
            }
        });
    }

    /// Quem lê confirmou o fim deste sentido.
    pub(crate) fn entregue(&self) {
        self.entrega.send_if_modified(|entrega| {
            if entrega.is_none() {
                *entrega = Some(Ok(()));
                true
            } else {
                false
            }
        });
    }

    fn falha(&self) -> Option<FalhaDoCanal> {
        self.parado.lock().ok().and_then(|parado| parado.clone())
    }
}

/// Fecha a conexão quando o último punho dela cai, como o QUIC fazia.
struct Guarda {
    cmd: mpsc::UnboundedSender<Comando>,
    par: usize,
}

impl Drop for Guarda {
    fn drop(&mut self) {
        let _ = self.cmd.send(Comando::Fechar {
            par: self.par,
            codigo: 0,
        });
    }
}

struct Filas {
    uni: tokio::sync::Mutex<mpsc::Receiver<Recebe>>,
    bi: tokio::sync::Mutex<mpsc::Receiver<(Envio, Recebe)>>,
    voz: tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>,
    tunel: tokio::sync::Mutex<mpsc::Receiver<CanalDeTunel>>,
    controle: Mutex<Option<(Envio, Recebe)>>,
}

/// Uma conexão WebRTC: o `ctl`, a voz e os canais extras.
#[derive(Clone)]
pub struct Conexao {
    comp: Arc<Compartilhado>,
    cmd: mpsc::UnboundedSender<Comando>,
    filas: Arc<Filas>,
    _guarda: Arc<Guarda>,
}

/// As pontas das filas que a tarefa enche.
pub(crate) struct LadoDaTarefa {
    pub(crate) uni: mpsc::Sender<Recebe>,
    pub(crate) bi: mpsc::Sender<(Envio, Recebe)>,
    pub(crate) voz: mpsc::Sender<Vec<u8>>,
    pub(crate) tunel: mpsc::Sender<CanalDeTunel>,
}

impl Conexao {
    pub(crate) fn nova(
        comp: Arc<Compartilhado>,
        cmd: mpsc::UnboundedSender<Comando>,
        controle: (Envio, Recebe),
    ) -> (Self, LadoDaTarefa) {
        let (uni_tx, uni_rx) = mpsc::channel(crate::MAX_CANAIS);
        let (bi_tx, bi_rx) = mpsc::channel(crate::MAX_CANAIS);
        let (voz_tx, voz_rx) = mpsc::channel(crate::FILA_DE_VOZ);
        let (tunel_tx, tunel_rx) = mpsc::channel(crate::MAX_CANAIS);
        let filas = Arc::new(Filas {
            uni: tokio::sync::Mutex::new(uni_rx),
            bi: tokio::sync::Mutex::new(bi_rx),
            voz: tokio::sync::Mutex::new(voz_rx),
            tunel: tokio::sync::Mutex::new(tunel_rx),
            controle: Mutex::new(Some(controle)),
        });
        let guarda = Arc::new(Guarda {
            cmd: cmd.clone(),
            par: comp.id,
        });
        (
            Self {
                comp,
                cmd,
                filas,
                _guarda: guarda,
            },
            LadoDaTarefa {
                uni: uni_tx,
                bi: bi_tx,
                voz: voz_tx,
                tunel: tunel_tx,
            },
        )
    }

    /// O `ctl`: o fluxo de controle. Entregue uma vez só; a segunda chamada é `None`.
    #[must_use]
    pub fn controle(&self) -> Option<(Envio, Recebe)> {
        self.filas.controle.lock().ok().and_then(|mut c| c.take())
    }

    /// O próximo canal "u" que o par abriu.
    ///
    /// # Errors
    ///
    /// O [`Motivo`] de a conexão ter acabado.
    pub async fn accept_uni(&self) -> Result<Recebe, Motivo> {
        let mut fila = self.filas.uni.lock().await;
        let recebe = fila.recv().await.ok_or_else(|| self.comp.motivo_atual())?;
        self.aceitou(recebe.canal);
        Ok(recebe)
    }

    /// O canal que o par abriu passou à aplicação: só agora ele recebe crédito.
    fn aceitou(&self, canal: ChannelId) {
        let _ = self.cmd.send(Comando::Aceito {
            par: self.comp.id,
            canal,
        });
    }

    /// O próximo canal "b" que o par abriu.
    ///
    /// # Errors
    ///
    /// O [`Motivo`] de a conexão ter acabado.
    pub async fn accept_bi(&self) -> Result<(Envio, Recebe), Motivo> {
        let mut fila = self.filas.bi.lock().await;
        let (envio, recebe) = fila.recv().await.ok_or_else(|| self.comp.motivo_atual())?;
        self.aceitou(recebe.canal);
        Ok((envio, recebe))
    }

    async fn abrir(&self, natureza: Natureza) -> Result<Aberto, Motivo> {
        let (resposta, espera) = oneshot::channel();
        self.cmd
            .send(Comando::Abrir {
                par: self.comp.id,
                natureza,
                resposta,
            })
            .map_err(|_| self.comp.motivo_atual())?;
        espera.await.map_err(|_| self.comp.motivo_atual())?
    }

    /// Abre um canal "u" para o par.
    ///
    /// # Errors
    ///
    /// A conexão que acabou, ou o teto de canais.
    pub async fn open_uni(&self) -> Result<Envio, Motivo> {
        match self.abrir(Natureza::Uni).await? {
            Aberto::Fluxo(envio, _) => Ok(envio),
            // Inalcançável: `Natureza::Uni` nunca abre um `j`; falha em vez de entrar em pânico.
            Aberto::Jogo(_) => Err(Motivo::Caiu("o canal unidirecional abriu como jogo".into())),
        }
    }

    /// Abre um canal "b" com o par.
    ///
    /// # Errors
    ///
    /// A conexão que acabou, ou o teto de canais.
    pub async fn open_bi(&self) -> Result<(Envio, Recebe), Motivo> {
        match self.abrir(Natureza::Bi).await? {
            Aberto::Fluxo(envio, Some(recebe)) => Ok((envio, recebe)),
            _ => Err(Motivo::Caiu(
                "o canal bidirecional abriu sem leitura".into(),
            )),
        }
    }

    /// Abre um canal de túnel para o par: o `t` (ordenado e confiável) ou o `j`
    /// (sem ordem e sem retransmissão).
    ///
    /// **Só chame para um par que negociou a linha 12.** Um par 11 não conhece
    /// os rótulos e fecha o canal ao chegar; o enlace não sabe a versão
    /// negociada, e quem decide é o chamador.
    ///
    /// # Errors
    ///
    /// [`Motivo::CanaisEsgotados`] no teto de [`crate::MAX_CANAIS`] canais (a
    /// conexão segue de pé), ou o motivo de a conexão ter acabado.
    pub async fn abrir_tunel(&self, c: Confiabilidade) -> Result<CanalDeTunel, Motivo> {
        let natureza = match c {
            Confiabilidade::Ordenada => Natureza::Tunel,
            Confiabilidade::SemOrdem => Natureza::Jogo,
        };
        match self.abrir(natureza).await? {
            Aberto::Fluxo(envio, Some(recebe)) => Ok(CanalDeTunel::Ordenado(envio, recebe)),
            Aberto::Jogo(datagramas) => Ok(CanalDeTunel::SemOrdem(datagramas)),
            Aberto::Fluxo(_, None) => {
                Err(Motivo::Caiu("o canal de túnel abriu sem leitura".into()))
            }
        }
    }

    /// O próximo canal de túnel (`t` ou `j`) que o par abriu.
    ///
    /// # Errors
    ///
    /// O [`Motivo`] de a conexão ter acabado.
    pub async fn aceitar_tunel(&self) -> Result<CanalDeTunel, Motivo> {
        let mut fila = self.filas.tunel.lock().await;
        let canal = fila.recv().await.ok_or_else(|| self.comp.motivo_atual())?;
        match &canal {
            CanalDeTunel::Ordenado(_, recebe) => self.aceitou(recebe.canal),
            CanalDeTunel::SemOrdem(datagramas) => self.aceitou(datagramas.canal),
        }
        Ok(canal)
    }

    /// O próximo datagrama de voz.
    ///
    /// # Errors
    ///
    /// O [`Motivo`] de a conexão ter acabado.
    pub async fn read_datagram(&self) -> Result<Vec<u8>, Motivo> {
        let mut fila = self.filas.voz.lock().await;
        fila.recv().await.ok_or_else(|| self.comp.motivo_atual())
    }

    /// Manda um datagrama de voz. Nunca espera.
    ///
    /// # Errors
    ///
    /// [`Recusa`]: grande demais, fila cheia, ou a conexão acabou.
    pub fn send_datagram(&self, bytes: Vec<u8>) -> Result<(), Recusa> {
        if bytes.len() > crate::MAX_VOZ {
            return Err(Recusa::Grande);
        }
        if self.comp.motivo.borrow().is_some() {
            return Err(Recusa::Fechada);
        }
        if self.comp.voz_na_fila.fetch_add(1, Ordering::Relaxed) >= crate::FILA_DE_VOZ {
            self.comp.voz_na_fila.fetch_sub(1, Ordering::Relaxed);
            return Err(Recusa::Cheia);
        }
        self.cmd
            .send(Comando::Voz {
                par: self.comp.id,
                bytes,
            })
            .map_err(|_| Recusa::Fechada)
    }

    /// Liga a mídia RTP desta conexão (vídeo e som, ADR 0067) e devolve o
    /// punho dela. **As duas pontas têm de ligar**: quem liga sozinho manda
    /// pacotes que o par não espera. Chamar de novo troca a fila de recepção:
    /// o punho anterior para de receber.
    #[must_use]
    pub fn ligar_midia(&self) -> crate::Midia {
        let (tx, rx) = mpsc::channel(crate::FILA_DE_RECEPCAO);
        let _ = self.cmd.send(Comando::LigarMidia {
            par: self.comp.id,
            fila: tx,
        });
        crate::Midia {
            remetente: crate::RemetenteDeMidia {
                comp: Arc::clone(&self.comp),
                cmd: self.cmd.clone(),
            },
            fila: rx,
        }
    }

    /// O endereço do par, como o ICE o escolheu.
    #[must_use]
    pub fn remote_address(&self) -> SocketAddr {
        self.comp
            .remoto
            .lock()
            .map(|r| *r)
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)))
    }

    /// Um número desta conexão, único no processo enquanto ela vive.
    #[must_use]
    pub fn stable_id(&self) -> usize {
        self.comp.id
    }

    /// Os contadores do caminho.
    #[must_use]
    pub fn leitura(&self) -> Leitura {
        let rtt = self.comp.rtt_us.load(Ordering::Relaxed);
        Leitura {
            bytes_enviados: self.comp.bytes_enviados.load(Ordering::Relaxed),
            pacotes_enviados: self.comp.pacotes_enviados.load(Ordering::Relaxed),
            bytes_recebidos: self.comp.bytes_recebidos.load(Ordering::Relaxed),
            ida_e_volta: (rtt > 0).then(|| Duration::from_micros(rtt)),
            pacotes_perdidos: None,
            eventos_de_congestionamento: None,
            fracao_de_perda_na_saida: self.comp.perda_na_saida.lock().ok().and_then(|p| *p),
            fracao_de_perda_na_entrada: self.comp.perda_na_entrada.lock().ok().and_then(|p| *p),
        }
    }

    /// O ida e volta, ou zero quando ninguém mediu.
    #[must_use]
    pub fn rtt(&self) -> Duration {
        self.leitura().ida_e_volta.unwrap_or_default()
    }

    /// A impressão do certificado DTLS deste lado.
    #[must_use]
    pub fn impressao_dtls_local(&self) -> [u8; 32] {
        self.comp.impressao_local
    }

    /// A impressão do certificado DTLS que o par apresentou.
    #[must_use]
    pub fn impressao_dtls_do_par(&self) -> [u8; 32] {
        self.comp.impressao_par.get().copied().unwrap_or([0; 32])
    }

    /// No cliente, o corpo da apresentação (`0x03`) que o anfitrião mandou.
    #[must_use]
    pub fn apresentacao(&self) -> &[u8] {
        self.comp.apresentacao.get().map_or(&[], Vec::as_slice)
    }

    /// O `U` desta conexão.
    #[must_use]
    pub fn credencial(&self) -> &str {
        &self.comp.credencial
    }

    /// Fecha: manda `corte(codigo)` no `ctl` e depois derruba DTLS e SCTP.
    ///
    /// O motivo em texto vai só ao registro deste lado: o fio leva o código.
    pub fn close(&self, codigo: u32, motivo: &[u8]) {
        tracing::debug!(
            conexao = self.comp.id,
            codigo,
            motivo = %String::from_utf8_lossy(motivo),
            "fechando a conexão"
        );
        let _ = self.cmd.send(Comando::Fechar {
            par: self.comp.id,
            codigo,
        });
    }

    pub(crate) fn pausa_de_teste(&self) -> &crate::teste::PausaDeTeste {
        &self.comp.pausa_de_teste
    }

    pub(crate) fn credito_em_aberto(&self) -> u64 {
        self.comp.credito_em_aberto.load(Ordering::Relaxed)
    }

    /// Segura (ou solta) os anúncios de crédito (`0x04`) que este lado manda.
    /// Só a [`crate::teste`] usa: é como um par atrasa ou nega o crédito.
    pub(crate) fn reter_credito(&self, reter: bool) {
        let _ = self.cmd.send(Comando::ReterCredito {
            par: self.comp.id,
            reter,
        });
    }

    /// Espera a conexão acabar, por qualquer lado.
    pub async fn closed(&self) -> Motivo {
        let mut motivo = self.comp.motivo.subscribe();
        let visto = match motivo.wait_for(Option::is_some).await {
            Ok(visto) => visto.clone(),
            Err(_) => None,
        };
        visto.unwrap_or_else(|| self.comp.motivo_atual())
    }

    /// Se a conexão já acabou.
    #[must_use]
    pub fn acabou(&self) -> Option<Motivo> {
        self.comp.motivo.borrow().clone()
    }

    /// O que esta conexão recusou ou descartou.
    #[must_use]
    pub fn contadores(&self) -> Contadores {
        let c = &self.comp.contadores;
        Contadores {
            canais_recusados: c.canais_recusados.load(Ordering::Relaxed),
            cortes_por_fila: c.cortes_por_fila.load(Ordering::Relaxed),
            violacoes: c.violacoes.load(Ordering::Relaxed),
            voz_descartada: c.voz_descartada.load(Ordering::Relaxed),
            voz_nao_saiu: c.voz_nao_saiu.load(Ordering::Relaxed),
            midia_nao_saiu: c.midia_nao_saiu.load(Ordering::Relaxed),
            midia_malformada: c.midia_malformada.load(Ordering::Relaxed),
            maior_volume_no_sctp: c.maior_volume_no_sctp.load(Ordering::Relaxed),
            tunel_descartado: c.tunel_descartado.load(Ordering::Relaxed),
            tunel_grande_demais: c.tunel_grande_demais.load(Ordering::Relaxed),
        }
    }
}

impl std::fmt::Debug for Conexao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conexao")
            .field("id", &self.comp.id)
            .field("remoto", &self.remote_address())
            .finish_non_exhaustive()
    }
}

/// Se o túnel pede ordem e retransmissão (o TCP do jogo) ou não (o UDP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confiabilidade {
    /// Ordenado e confiável: o canal `t`.
    Ordenada,
    /// Sem ordem e sem retransmissão: o canal `j`.
    SemOrdem,
}

/// Um canal de túnel, do tipo que o `abrir_tunel` pediu ou que o par abriu.
#[derive(Debug)]
pub enum CanalDeTunel {
    /// O `t`: os mesmos `Envio` e `Recebe` (e o mesmo crédito) dos canais `u` e `b`.
    Ordenado(Envio, Recebe),
    /// O `j`: datagramas.
    SemOrdem(Datagramas),
}

/// O que se leu da configuração de um canal `j` no DCEP. Só a [`crate::teste`] lê.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConfigLida {
    pub(crate) ordenado: bool,
    pub(crate) sem_retransmissao: bool,
}

/// O canal `j`: datagramas sem ordem e sem retransmissão, cada um até
/// [`crate::MAX_DADOS`] bytes, sem envelope (a mensagem SCTP é o datagrama).
///
/// Nada aqui espera: o jogo que atrasa é pior que o que perde.
pub struct Datagramas {
    pub(crate) cmd: mpsc::UnboundedSender<Comando>,
    pub(crate) comp: Arc<Compartilhado>,
    pub(crate) canal: ChannelId,
    pub(crate) estado: Arc<EstadoDoCanal>,
    /// Datagramas já entregues à tarefa e ainda não escritos no SCTP.
    pub(crate) na_fila: Arc<AtomicUsize>,
    pub(crate) entrada: mpsc::Receiver<Vec<u8>>,
    pub(crate) config: ConfigLida,
}

impl std::fmt::Debug for Datagramas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Datagramas")
            .field("conexao", &self.comp.id)
            .field("canal", &self.canal)
            .finish_non_exhaustive()
    }
}

impl Datagramas {
    /// Só a [`crate::teste`]: o que o DCEP trouxe da configuração do canal.
    pub(crate) fn config_lida(&self) -> ConfigLida {
        self.config
    }

    /// Manda um datagrama. Nunca espera.
    ///
    /// # Errors
    ///
    /// [`Recusa::Grande`] acima de [`crate::MAX_DADOS`] (conta
    /// `tunel_grande_demais`); [`Recusa::Cheia`] com [`crate::FILA_DE_JOGO`]
    /// datagramas esperando (conta `tunel_descartado`); [`Recusa::Fechada`] se
    /// o canal ou a conexão acabou.
    pub fn mandar(&self, bytes: &[u8]) -> Result<(), Recusa> {
        if self.comp.motivo.borrow().is_some() || self.estado.falha().is_some() {
            return Err(Recusa::Fechada);
        }
        let contadores = &self.comp.contadores;
        if bytes.len() > crate::MAX_DADOS {
            contadores
                .tunel_grande_demais
                .fetch_add(1, Ordering::Relaxed);
            return Err(Recusa::Grande);
        }
        if self.na_fila.fetch_add(1, Ordering::Relaxed) >= crate::FILA_DE_JOGO {
            self.na_fila.fetch_sub(1, Ordering::Relaxed);
            contadores.tunel_descartado.fetch_add(1, Ordering::Relaxed);
            return Err(Recusa::Cheia);
        }
        self.cmd
            .send(Comando::Jogo {
                par: self.comp.id,
                canal: self.canal,
                bytes: bytes.to_vec(),
            })
            .map_err(|_| {
                self.na_fila.fetch_sub(1, Ordering::Relaxed);
                Recusa::Fechada
            })
    }

    /// O próximo datagrama, ou `None` quando o canal acabou.
    pub async fn receber(&mut self) -> Option<Vec<u8>> {
        self.entrada.recv().await
    }
}

impl Drop for Datagramas {
    fn drop(&mut self) {
        // Largar o `j` o fecha: a tarefa tira o canal do mapa e libera a vaga.
        let _ = self.cmd.send(Comando::FecharJogo {
            par: self.comp.id,
            canal: self.canal,
        });
    }
}

/// O lado de escrever de um canal. Também `tokio::io::AsyncWrite`: o
/// `poll_shutdown` é o [`Envio::finish`].
pub struct Envio {
    cmd: mpsc::UnboundedSender<Comando>,
    par: usize,
    canal: ChannelId,
    credito: Arc<Semaphore>,
    sondado: tokio_util::sync::PollSemaphore,
    estado: Arc<EstadoDoCanal>,
    comp: Arc<Compartilhado>,
    terminado: bool,
}

impl tokio::io::AsyncWrite for Envio {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if let Err(falha) = self.conferir() {
            return Poll::Ready(Err(std::io::Error::other(falha)));
        }
        let quanto = buf.len().min(crate::MAX_DADOS);
        let permissoes = u32::try_from(quanto + 1).unwrap_or(u32::MAX);
        let credito = match self.sondado.poll_acquire_many(cx, permissoes) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Some(credito)) => credito,
            Poll::Ready(None) => {
                let falha = self
                    .estado
                    .falha()
                    .unwrap_or_else(|| FalhaDoCanal::Caiu(self.comp.motivo_atual()));
                return Poll::Ready(Err(std::io::Error::other(falha)));
            }
        };
        let pedaco = buf.get(..quanto).unwrap_or_default();
        match self.enviar(crate::envelope::dados(pedaco), Some(credito), Depois::Nada) {
            Ok(()) => Poll::Ready(Ok(quanto)),
            Err(falha) => Poll::Ready(Err(std::io::Error::other(falha))),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.terminado {
            return std::task::Poll::Ready(Ok(()));
        }
        std::task::Poll::Ready(self.finish().map_err(std::io::Error::other))
    }
}

impl std::fmt::Debug for Envio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Envio")
            .field("conexao", &self.par)
            .field("canal", &self.canal)
            .finish_non_exhaustive()
    }
}

impl Envio {
    pub(crate) fn novo(
        cmd: mpsc::UnboundedSender<Comando>,
        comp: Arc<Compartilhado>,
        canal: ChannelId,
        credito: Arc<Semaphore>,
        estado: Arc<EstadoDoCanal>,
    ) -> Self {
        Self {
            cmd,
            par: comp.id,
            canal,
            sondado: tokio_util::sync::PollSemaphore::new(Arc::clone(&credito)),
            credito,
            estado,
            comp,
            terminado: false,
        }
    }

    fn conferir(&self) -> Result<(), FalhaDoCanal> {
        if self.terminado {
            return Err(FalhaDoCanal::Fim);
        }
        if let Some(falha) = self.estado.falha() {
            return Err(falha);
        }
        Ok(())
    }

    fn enviar(
        &self,
        mensagem: Vec<u8>,
        credito: Option<OwnedSemaphorePermit>,
        depois: Depois,
    ) -> Result<(), FalhaDoCanal> {
        self.cmd
            .send(Comando::Escrever {
                par: self.par,
                canal: self.canal,
                mensagem,
                credito,
                depois,
            })
            .map_err(|_| FalhaDoCanal::Caiu(self.comp.motivo_atual()))
    }

    /// Escreve todos os bytes, em mensagens de até [`crate::MAX_DADOS`].
    ///
    /// Espera crédito quando o canal já tem [`crate::CREDITO_DE_SAIDA`] bytes a caminho.
    ///
    /// # Errors
    ///
    /// O canal que acabou, foi cortado pelo par, ou a conexão que caiu.
    pub async fn write_all(&mut self, bytes: &[u8]) -> Result<(), FalhaDoCanal> {
        for pedaco in bytes.chunks(crate::MAX_DADOS) {
            self.conferir()?;
            let quanto = u32::try_from(pedaco.len() + 1).unwrap_or(u32::MAX);
            let credito = Arc::clone(&self.credito)
                .acquire_many_owned(quanto)
                .await
                .map_err(|_| {
                    self.estado
                        .falha()
                        .unwrap_or(FalhaDoCanal::Caiu(self.comp.motivo_atual()))
                })?;
            self.conferir()?;
            self.enviar(crate::envelope::dados(pedaco), Some(credito), Depois::Nada)?;
        }
        Ok(())
    }

    /// Termina o fluxo neste sentido (o `fim`). Não espera.
    ///
    /// # Errors
    ///
    /// O canal que já tinha acabado.
    pub fn finish(&mut self) -> Result<(), FalhaDoCanal> {
        self.conferir()?;
        self.terminado = true;
        self.enviar(crate::envelope::fim(), None, Depois::Fim)
    }

    /// Corta o fluxo com um código. Não espera.
    ///
    /// # Errors
    ///
    /// O canal que já tinha acabado.
    pub fn reset(&mut self, codigo: u32) -> Result<(), FalhaDoCanal> {
        if self.terminado {
            return Err(FalhaDoCanal::Fim);
        }
        self.terminado = true;
        self.enviar(crate::envelope::corte(codigo), None, Depois::Corte)
    }

    /// Espera quem lê confirmar o fim (o `stopped()` do QUIC).
    ///
    /// Depois do [`Envio::finish`]: `Ok(())` quando quem lê recebeu tudo e
    /// devolveu o `fim`; `Err(Cortado(c))` quando ele cortou, mesmo depois do
    /// último bloco; `Err(Caiu)` quando a conexão caiu antes.
    ///
    /// # Errors
    ///
    /// O corte de quem lê, ou a queda.
    pub async fn entregue(&mut self) -> Result<(), FalhaDoCanal> {
        let mut entrega = self.estado.entrega.subscribe();
        let visto = match entrega.wait_for(Option::is_some).await {
            Ok(visto) => visto.clone(),
            Err(_) => None,
        };
        visto.unwrap_or_else(|| Err(FalhaDoCanal::Caiu(self.comp.motivo_atual())))
    }

    /// Mantido pela forma de quem chama. O escalonador da ponta já põe o `ctl`
    /// e a voz na frente, e reveza os canais de volume entre si.
    ///
    /// # Errors
    ///
    /// Nunca, hoje.
    pub fn set_priority(&self, _prioridade: i32) -> Result<(), FalhaDoCanal> {
        Ok(())
    }
}

impl Drop for Envio {
    fn drop(&mut self) {
        // Como o QUIC: largar o fluxo sem terminar é terminá-lo.
        if !self.terminado && self.estado.falha().is_none() {
            self.terminado = true;
            let _ = self.enviar(crate::envelope::fim(), None, Depois::Fim);
        }
    }
}

/// O lado de ler de um canal.
pub struct Recebe {
    entrada: mpsc::UnboundedReceiver<Entrada>,
    na_fila: Arc<AtomicUsize>,
    atual: Vec<u8>,
    posicao: usize,
    fim: Option<Result<(), FalhaDoCanal>>,
    cmd: mpsc::UnboundedSender<Comando>,
    par: usize,
    canal: ChannelId,
    comp: Arc<Compartilhado>,
    /// Bytes de dados já tirados da fila por quem lê.
    consumidos: u64,
    /// O que estava consumido no último aviso à tarefa.
    avisado: u64,
    /// Ocupa uma vaga em [`Compartilhado::filas_vivas`] até o fim ou o `drop`.
    vaga: bool,
}

impl std::fmt::Debug for Recebe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recebe")
            .field("conexao", &self.par)
            .field("canal", &self.canal)
            .finish_non_exhaustive()
    }
}

impl Recebe {
    pub(crate) fn novo(
        entrada: mpsc::UnboundedReceiver<Entrada>,
        na_fila: Arc<AtomicUsize>,
        cmd: mpsc::UnboundedSender<Comando>,
        comp: Arc<Compartilhado>,
        canal: ChannelId,
    ) -> Self {
        comp.filas_vivas.fetch_add(1, Ordering::Relaxed);
        Self {
            entrada,
            na_fila,
            atual: Vec::new(),
            posicao: 0,
            fim: None,
            cmd,
            par: comp.id,
            canal,
            comp,
            consumidos: 0,
            avisado: 0,
            vaga: true,
        }
    }

    /// Quem lê desistiu (`stop` ou `drop`): o que estava na fila é jogado fora,
    /// e sai da conta do crédito agregado na hora.
    ///
    /// **Subtrai o que de fato drenou, e nunca zera.** A tarefa do enlace soma
    /// `N` antes de pôr `N` bytes na fila e, se a fila já fechou, desfaz com
    /// `-N`. Cada `+N` tem exatamente um `-N` depois dele: o de quem tirou a
    /// mensagem (a leitura, ou este dreno) ou o desfazer da tarefa, quando a
    /// mensagem nunca entrou. Um `store(0)` no meio de um `+N` e o desfazer dele
    /// dava a volta no contador (revisão do crédito integrado, P1), e o
    /// agregado ficava cheio para sempre. Sem trava e sem depender de ordenação.
    fn descartar_fila(&mut self) {
        self.entrada.close();
        while let Ok(entrada) = self.entrada.try_recv() {
            if let Entrada::Dados(bytes) = entrada {
                self.na_fila.fetch_sub(bytes.len(), Ordering::Relaxed);
            }
        }
        self.atual = Vec::new();
        self.posicao = 0;
    }

    /// A fila acabou (fim, corte ou queda já lidos, ou largada): solta a vaga.
    fn soltar_vaga(&mut self) {
        if std::mem::take(&mut self.vaga) {
            self.comp.filas_vivas.fetch_sub(1, Ordering::Relaxed);
            // A fila esvaziou ou foi largada: o que ela segurava volta ao agregado.
            let _ = self.cmd.send(Comando::Consumiu { par: self.par });
        }
    }

    /// Conta o que saiu da fila e avisa a tarefa a cada pedaço lido (até um
    /// quarto da janela), para ela rever o crédito (`0x04`) da conexão: o deste
    /// canal e o dos que esperam vaga no agregado.
    fn consumiu(&mut self, quanto: usize) {
        self.consumidos += quanto as u64;
        let passo = (crate::MAX_FILA_DE_ENTRADA / 16) as u64;
        if self.consumidos >= self.avisado + passo {
            self.avisado = self.consumidos;
            let _ = self.cmd.send(Comando::Consumiu { par: self.par });
        }
    }

    /// Lê até `buf.len()` bytes. `Ok(None)` é o fim do fluxo.
    ///
    /// # Errors
    ///
    /// O par que cortou, ou a conexão que caiu.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, FalhaDoCanal> {
        std::future::poll_fn(|cx| self.poll_ler(cx, buf)).await
    }

    fn poll_ler(
        &mut self,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<Result<Option<usize>, FalhaDoCanal>> {
        use std::task::Poll;
        if buf.is_empty() {
            return Poll::Ready(Ok(Some(0)));
        }
        loop {
            if let Some(resto) = self.atual.get(self.posicao..) {
                if !resto.is_empty() {
                    let n = resto.len().min(buf.len());
                    if let (Some(destino), Some(origem)) = (buf.get_mut(..n), resto.get(..n)) {
                        destino.copy_from_slice(origem);
                    }
                    self.posicao += n;
                    return Poll::Ready(Ok(Some(n)));
                }
            }
            if let Some(fim) = self.fim.clone() {
                self.soltar_vaga();
                return Poll::Ready(fim.map(|()| None));
            }
            match self.entrada.poll_recv(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Entrada::Dados(bytes))) => {
                    self.na_fila.fetch_sub(bytes.len(), Ordering::Relaxed);
                    self.consumiu(bytes.len());
                    self.atual = bytes;
                    self.posicao = 0;
                }
                Poll::Ready(Some(Entrada::Fim)) => self.fim = Some(Ok(())),
                Poll::Ready(Some(Entrada::Corte(codigo))) => {
                    self.fim = Some(Err(FalhaDoCanal::Cortado(codigo)));
                }
                Poll::Ready(None) => {
                    self.fim = Some(Err(FalhaDoCanal::Caiu(self.comp.motivo_atual())));
                }
            }
        }
    }

    /// Lê exatamente `buf.len()` bytes.
    ///
    /// # Errors
    ///
    /// [`FalhaDoCanal::Fim`] se o fluxo acabou antes; o corte ou a queda.
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), FalhaDoCanal> {
        let mut lidos = 0;
        while lidos < buf.len() {
            let Some(resto) = buf.get_mut(lidos..) else {
                break;
            };
            match self.read(resto).await? {
                Some(n) => lidos += n,
                None => return Err(FalhaDoCanal::Fim),
            }
        }
        Ok(())
    }

    /// Lê até o fim, com um teto. Passar do teto é erro, e nada é guardado além dele.
    ///
    /// # Errors
    ///
    /// O teto, o corte ou a queda.
    pub async fn read_to_end(&mut self, teto: usize) -> Result<Vec<u8>, FalhaDoCanal> {
        let mut saida = Vec::new();
        let mut pedaco = vec![0_u8; 16 * 1024];
        while let Some(n) = self.read(&mut pedaco).await? {
            if saida.len() + n > teto {
                return Err(FalhaDoCanal::Cortado(crate::CORTE_FILA));
            }
            saida.extend_from_slice(pedaco.get(..n).unwrap_or_default());
        }
        Ok(saida)
    }

    /// Diz ao par que este lado não lê mais (manda `corte(codigo)`).
    ///
    /// # Errors
    ///
    /// A conexão que caiu.
    pub fn stop(&mut self, codigo: u32) -> Result<(), FalhaDoCanal> {
        if self.fim.is_none() {
            self.fim = Some(Err(FalhaDoCanal::Cortado(codigo)));
        }
        self.descartar_fila();
        self.cmd
            .send(Comando::Parar {
                par: self.par,
                canal: self.canal,
                codigo,
            })
            .map_err(|_| FalhaDoCanal::Caiu(self.comp.motivo_atual()))
    }
}

impl tokio::io::AsyncRead for Recebe {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::task::Poll;
        let destino = buf.initialize_unfilled();
        match self.poll_ler(cx, destino) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(Some(n))) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(falha)) => Poll::Ready(Err(std::io::Error::other(falha))),
        }
    }
}

impl Drop for Recebe {
    fn drop(&mut self) {
        // A fila sai da conta **antes** do aviso à tarefa: ela revê o agregado
        // ao receber o aviso, e não pode ver a fila de quem já foi largado.
        self.descartar_fila();
        self.soltar_vaga();
        // Como o QUIC: largar a leitura no meio é parar o fluxo.
        if self.fim.is_none() {
            let _ = self.cmd.send(Comando::Parar {
                par: self.par,
                canal: self.canal,
                codigo: 0,
            });
        }
    }
}
