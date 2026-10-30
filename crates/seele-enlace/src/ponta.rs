//! A [`Ponta`]: um socket UDP, uma tarefa, e todos os `Rtc` que passam por ele.
//!
//! # A demultiplexação (RFC 7983)
//!
//! Pelo primeiro byte: STUN (0 a 3), DTLS (20 a 63), e o resto vai para
//! [`Ponta::mensagens_enc`] — os datagramas de texto do ponto de encontro, que
//! dividem a porta 8383 com o WebRTC. Um STUN ou um DTLS vai ao `Rtc` que o
//! aceita (`Rtc::accepts`, que é como o str0m separa vários pares num socket).
//!
//! # O anfitrião
//!
//! ICE completo e **controlado**, e DTLS passivo (até 26/10/2026, ICE-lite: um
//! anfitrião lite não checa, e atrás de um NAT de casa que muda a porta conforme
//! o destino quem liga nunca aprendia a porta certa; medido no 5G, 14178 para o
//! encontro e 14224 para o relay). O par nasce de dois jeitos, pelos mesmos
//! portões: um `CHAMADO` do ENC/3 ([`Ponta::chamado`]), que já traz o U e os
//! candidatos de quem liga, e o ICE deste lado os checa; ou um STUN Binding com
//! USERNAME `U:U` que nenhum `Rtc` aceita. Um U que já tem par vai a ele, venha o
//! Binding antes ou depois do `CHAMADO`. Os portões, nesta ordem:
//!
//! 1. o MESSAGE-INTEGRITY confere com `U` (um pacote forjado sem `U` não gasta
//!    nada; no `CHAMADO`, que chega pelo ponto, a credencial só precisa ter a forma
//!    de um U, e o que limita é o teto do furo, [`FUROS_POR_MINUTO`]);
//! 2. o IP não está esperando depois de falhas seguidas ([`FALHAS_ANTES_DA_ESPERA`]);
//! 3. o teto por IP ([`crate::MAX_PENDENTES_POR_IP`]);
//! 4. o balde de criações por IP: [`CRIACOES_POR_IP`] apertos **que não
//!    terminaram** em [`JANELA_DE_CRIACAO`] (um aperto que termina devolve a vaga);
//! 5. `admitir(ip)`, o balde de quem hospeda;
//! 6. o teto total ([`crate::MAX_PENDENTES`]). **Cheio, o pendente mais velho
//!    sai** e o novo entra (R3 da revisão). Recusar o novo deixaria quem
//!    forja origens trancar a porta por 10 s, de novo e de novo.
//!
//! Cada recusa tem um contador próprio em [`ContadoresDaPonta`] (`recusa_*`) e
//! um `tracing`: do outro lado ela é só silêncio.
//!
//! Um pendente que não começa o DTLS em 2 s sai (R3, (c)): o par forjado nunca
//! recebe a resposta do Binding, então nunca manda DTLS. O par do `CHAMADO` nasce
//! antes de quem liga checar, e os 2 s dele contam do primeiro Binding de quem
//! liga; sem nenhum, ele sai no prazo do aperto ([`crate::APERTO`]) e vira lápide.
//!
//! Responder o primeiro Binding sem estado e só criar o `Rtc` com o DTLS (R3,
//! (a)) não foi feito. O str0m só nomeia o caminho de envio ao ver um Binding
//! com USE-CANDIDATE, e um `Rtc` criado no primeiro DTLS ficaria sem caminho até
//! o próximo check do par, que pode tardar segundos.
//!
//! # Quem liga
//!
//! ICE controlling, DTLS ativo e SCTP de cliente. A descrição remota é
//! sintetizada (D2): as credenciais `U:U`, os alvos como candidatos host, e a
//! impressão do pino. Não há SDP em lugar nenhum.
//!
//! # O escalonador (R4)
//!
//! A cada volta, nesta ordem: o `ctl` e os cortes saem na hora; a voz sai na
//! hora em que chega; e os canais de volume ("u" e "b") só escrevem no SCTP
//! enquanto a associação inteira tem menos de [`LIMIAR_DE_VOLUME`] esperando,
//! revezando entre si. Assim um anexo de 32 MiB nunca põe mais de 64 KiB na
//! frente de um datagrama de voz.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::config::Fingerprint;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceCreds, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot, watch, Semaphore};

use crate::conexao::{
    Comando, Compartilhado, Conexao, ContadoresVivos, Depois, Entrada, Envio, EstadoDoCanal,
    LadoDaTarefa, Recebe,
};
use crate::{CertificadoDtls, Motivo};

/// Os canais de volume só escrevem no SCTP com a associação abaixo disto.
pub const LIMIAR_DE_VOLUME: usize = 64 * 1024;
/// Quanto o anfitrião espera o primeiro DTLS depois de o ICE nomear um caminho.
pub const ESPERA_PELO_DTLS: Duration = Duration::from_secs(2);
/// Binding Indications por candidato num furo (R2).
pub const INDICACOES_POR_CANDIDATO: usize = 5;
/// Candidatos por furo (R2).
pub const CANDIDATOS_POR_FURO: usize = 3;
/// Furos por minuto num anfitrião (R2).
pub const FUROS_POR_MINUTO: usize = 30;

/// Por quanto tempo o U de uma conexão que terminou não cria outra (a lápide).
///
/// O defeito que isto conserta, achado pelo spike C: depois de uma impressão
/// adulterada o navegador manda um alerta fatal, e o próximo Binding de
/// consentimento com o mesmo U recriava o `Rtc` — um laço de recriação.
///
/// 30 s, o valor validado no spike C: com a lápide, o navegador desiste sozinho
/// em ~15 s (ICE `failed`) e para de mandar consentimento.
pub const LAPIDE: Duration = Duration::from_secs(30);
/// Criações de par por IP numa [`JANELA_DE_CRIACAO`] (o balde do spike C,
/// dimensionado para três sessões de uma casa atrás de um NAT, com reconexões).
pub const CRIACOES_POR_IP: usize = 8;
/// A janela do [`CRIACOES_POR_IP`].
pub const JANELA_DE_CRIACAO: Duration = Duration::from_secs(10);
/// Lápides guardadas; cheio, a mais velha sai.
pub const MAX_LAPIDES: usize = 4096;
/// Falhas seguidas de um IP antes de ele esperar para tentar um U novo.
pub const FALHAS_ANTES_DA_ESPERA: u32 = 3;
/// A espera mais longa de um IP que falha.
pub const ESPERA_MAXIMA: Duration = Duration::from_secs(30);

/// A espera depois de `falhas` falhas seguidas: 1 s, 2 s, 4 s… até [`ESPERA_MAXIMA`].
fn espera_por_falhas(falhas: u32) -> Duration {
    let expoente = falhas.saturating_sub(FALHAS_ANTES_DA_ESPERA).min(5);
    Duration::from_secs(1_u64 << expoente).min(ESPERA_MAXIMA)
}

/// O U de um Binding request com USERNAME `U:U`, sem conferir a integridade.
fn binding_com_u(bytes: &[u8]) -> Option<String> {
    let mensagem = str0m::ice::StunMessage::parse(bytes).ok()?;
    if !mensagem.is_binding_request() {
        return None;
    }
    crate::credencial::de_usuario(mensagem.username()?).map(str::to_owned)
}

static PROXIMO_ID: AtomicUsize = AtomicUsize::new(1);

/// Como um anfitrião se apresenta e quem ele deixa entrar.
pub struct ConfigDoAnfitriao {
    /// O certificado DTLS: no `seeled`, o longo do banco.
    pub certificado: CertificadoDtls,
    /// O corpo da `0x03`, até [`crate::MAX_APRESENTACAO`] bytes. Vazio no `seeled`.
    pub apresentacao: Vec<u8>,
    /// O balde por IP, consultado antes de criar o `Rtc`.
    pub admitir: Arc<dyn Fn(IpAddr) -> bool + Send + Sync>,
}

impl ConfigDoAnfitriao {
    /// Um anfitrião que deixa todo mundo tentar e se apresenta vazio.
    #[must_use]
    pub fn novo(certificado: CertificadoDtls) -> Self {
        Self {
            certificado,
            apresentacao: Vec::new(),
            admitir: Arc::new(|_| true),
        }
    }
}

impl std::fmt::Debug for ConfigDoAnfitriao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigDoAnfitriao")
            .field("certificado", &self.certificado)
            .field("apresentacao", &self.apresentacao.len())
            .finish_non_exhaustive()
    }
}

/// Contra o quê quem liga confere o certificado do anfitrião.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pino {
    /// A impressão do convite, ou a guardada: o DTLS cai se for outra.
    Impressao([u8; 32]),
    /// Primeiro contato sem impressão (TOFU, só no nativo): a conexão abre e
    /// quem chamou decide por [`Conexao::impressao_dtls_do_par`] antes do `Hello`.
    Qualquer,
}

/// Uma discagem.
#[derive(Debug, Clone)]
pub struct Discagem {
    /// Os endereços do anfitrião, tentados juntos pelo ICE.
    pub alvos: Vec<SocketAddr>,
    /// O pino.
    pub pino: Pino,
    /// O `U` já sorteado (o do `CHAMA`); `None` sorteia um.
    pub credencial: Option<String>,
    /// O prazo inteiro, do primeiro check à apresentação.
    pub prazo: Duration,
    /// O certificado DTLS deste lado; `None` gera um efêmero.
    pub certificado: Option<CertificadoDtls>,
}

impl Discagem {
    /// Uma discagem com o prazo de [`crate::APERTO`] e um `U` novo.
    #[must_use]
    pub fn nova(alvos: Vec<SocketAddr>, pino: Pino) -> Self {
        Self {
            alvos,
            pino,
            credencial: None,
            prazo: crate::APERTO,
            certificado: None,
        }
    }
}

/// Por que uma discagem não virou conexão.
#[derive(Debug, thiserror::Error)]
pub enum FalhaAoDiscar {
    /// Nenhum alvo utilizável.
    #[error("nenhum endereço para discar")]
    SemAlvos,
    /// Nenhum pacote voltou de nenhum alvo no prazo.
    #[error("nada respondeu nesses endereços")]
    SemResposta,
    /// Quem respondeu apresentou outro certificado.
    #[error("quem atendeu não é o servidor esperado (impressão diferente)")]
    ImpressaoDiferente {
        /// A impressão que veio, quando o DTLS chegou a mostrá-la.
        vista: Option<[u8; 32]>,
    },
    /// O par falou algo que o enlace não aceita.
    #[error("o par violou o enlace: {0}")]
    Protocolo(String),
    /// Alguém respondeu, e o aperto não terminou no prazo.
    #[error("o aperto não terminou no prazo")]
    Prazo,
    /// A ponta foi fechada.
    #[error("a ponta foi fechada")]
    PontaFechada,
    /// Erro do sistema.
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// Os contadores de uma ponta.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContadoresDaPonta {
    /// Pendentes despejados para dar lugar a um novo (R3).
    pub pendentes_despejados: u64,
    /// Pendentes que não começaram o DTLS a tempo.
    pub pendentes_sem_dtls: u64,
    /// Bindings recusados por integridade, teto por IP ou balde: a soma dos
    /// quatro contadores `recusa_*` abaixo, que dizem qual portão recusou.
    pub bindings_recusados: u64,
    /// … porque o MESSAGE-INTEGRITY não conferia com o U.
    pub recusa_pela_integridade: u64,
    /// … porque o IP gastou o balde de criações ([`CRIACOES_POR_IP`] apertos
    /// que não terminaram em [`JANELA_DE_CRIACAO`]).
    pub recusa_pelo_balde_de_criacao: u64,
    /// … porque o IP já tinha [`crate::MAX_PENDENTES_POR_IP`] pendentes.
    pub recusa_pelo_teto_por_ip: u64,
    /// … porque quem hospeda não admitiu o endereço (`admitir(ip)`).
    pub recusa_de_quem_hospeda: u64,
    /// Bindings com o U de uma conexão que terminou há pouco (a lápide).
    pub bindings_em_lapide: u64,
    /// Pares de anfitrião criados desde que a ponta abriu.
    pub pares_criados: u64,
    /// Pendentes aposentados por um aperto novo da mesma origem.
    pub pendentes_superados: u64,
    /// Bindings com U novo de uma origem que já tem uma sessão ativa (descartados).
    pub bindings_com_sessao_ativa: u64,
    /// Pânicos do str0m/dimpl contidos: cada um descartou um par, e só ele.
    pub panicos_contidos: u64,
    /// Registros DTLS de época ≥ 1 descartados antes do ClientHello da sessão
    /// (de uma sessão anterior da mesma origem, ou forjados).
    pub dtls_de_outra_sessao: u64,
    /// Bindings de um IP que está esperando depois de falhas seguidas.
    pub bindings_em_espera: u64,
    /// Furos recusados pelo teto por minuto ou por candidato inválido.
    pub furos_recusados: u64,
    /// Datagramas que não saíram (socket cheio).
    pub datagramas_perdidos_na_saida: u64,
    /// Pares de anfitrião que nasceram de um `CHAMADO` (e não de um Binding).
    pub pares_do_chamado: u64,
    /// `CHAMADO`s com o U de uma conexão que terminou há pouco (a lápide).
    pub chamados_em_lapide: u64,
}

#[derive(Default)]
struct ContadoresDaPontaVivos {
    pendentes_despejados: std::sync::atomic::AtomicU64,
    pendentes_sem_dtls: std::sync::atomic::AtomicU64,
    bindings_recusados: std::sync::atomic::AtomicU64,
    recusa_pela_integridade: std::sync::atomic::AtomicU64,
    recusa_pelo_balde_de_criacao: std::sync::atomic::AtomicU64,
    recusa_pelo_teto_por_ip: std::sync::atomic::AtomicU64,
    recusa_de_quem_hospeda: std::sync::atomic::AtomicU64,
    bindings_em_lapide: std::sync::atomic::AtomicU64,
    pares_criados: std::sync::atomic::AtomicU64,
    pendentes_superados: std::sync::atomic::AtomicU64,
    bindings_com_sessao_ativa: std::sync::atomic::AtomicU64,
    panicos_contidos: std::sync::atomic::AtomicU64,
    dtls_de_outra_sessao: std::sync::atomic::AtomicU64,
    bindings_em_espera: std::sync::atomic::AtomicU64,
    furos_recusados: std::sync::atomic::AtomicU64,
    datagramas_perdidos_na_saida: std::sync::atomic::AtomicU64,
    pares_do_chamado: std::sync::atomic::AtomicU64,
    chamados_em_lapide: std::sync::atomic::AtomicU64,
}

struct Interno {
    cmd: mpsc::UnboundedSender<Comando>,
    local: SocketAddr,
    /// O descritor de envio. `fechar()` o solta, para o socket voltar ao sistema
    /// sem esperar a última cópia da `Ponta` cair (pedido do E).
    socket: Mutex<Option<Arc<std::net::UdpSocket>>>,
    /// Vira `true` quando a tarefa da ponta saiu e soltou os descritores dela.
    parada: watch::Receiver<bool>,
    chegadas: tokio::sync::Mutex<mpsc::Receiver<Conexao>>,
    enc: Mutex<Option<FilaDoEnc>>,
    vivas: Arc<AtomicUsize>,
    furos: Mutex<VecDeque<Instant>>,
    contadores: Arc<ContadoresDaPontaVivos>,
}

impl Drop for Interno {
    fn drop(&mut self) {
        let _ = self.cmd.send(Comando::Desligar);
    }
}

/// Um socket UDP e as conexões WebRTC que passam por ele. Sem estado global:
/// cada ponta tem a sua tarefa, os seus `Rtc` e o seu provedor de cripto.
#[derive(Clone)]
pub struct Ponta {
    interno: Arc<Interno>,
}

impl std::fmt::Debug for Ponta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ponta")
            .field("local", &self.interno.local)
            .field("vivas", &self.conexoes_vivas())
            .finish()
    }
}

impl Ponta {
    /// Abre um socket em `escuta` e a tarefa dele.
    ///
    /// # Errors
    ///
    /// O socket que não abre.
    pub async fn abrir(
        escuta: SocketAddr,
        anfitriao: Option<ConfigDoAnfitriao>,
    ) -> io::Result<Self> {
        Self::de_socket(std::net::UdpSocket::bind(escuta)?, anfitriao)
    }

    /// A ponta de quem liga: `[::]:0` em pilha dupla (o `IPV6_V6ONLY` escrito à
    /// mão, porque o padrão muda por sistema), com recuo para `0.0.0.0:0`.
    ///
    /// # Errors
    ///
    /// Nenhum dos dois sockets abre.
    pub async fn abrir_cliente() -> io::Result<Self> {
        let duplo = (|| {
            let socket = socket2::Socket::new(
                socket2::Domain::IPV6,
                socket2::Type::DGRAM,
                Some(socket2::Protocol::UDP),
            )?;
            socket.set_only_v6(false)?;
            socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)).into())?;
            io::Result::Ok(std::net::UdpSocket::from(socket))
        })();
        match duplo {
            Ok(socket) => Self::de_socket(socket, None),
            Err(erro) => {
                tracing::debug!(%erro, "sem pilha dupla: a ponta de quem liga fica em IPv4");
                Self::abrir(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)), None).await
            }
        }
    }

    /// Arma (ou, com `None`, desarma) o modo anfitrião nesta ponta: o par de
    /// tela atende na mesma porta que fala com o servidor. Desarmar não
    /// derruba quem já entrou.
    pub fn passar_a_atender(&self, config: Option<ConfigDoAnfitriao>) {
        let _ = self
            .interno
            .cmd
            .send(Comando::Anfitriao(config.map(Box::new)));
    }

    /// Adota um socket já aberto (o de `alcance::abrir_escuta`, com o
    /// `IPV6_V6ONLY` escrito à mão). Precisa de um runtime Tokio.
    ///
    /// # Errors
    ///
    /// O socket que não vira não bloqueante.
    pub fn de_socket(
        socket: std::net::UdpSocket,
        anfitriao: Option<ConfigDoAnfitriao>,
    ) -> io::Result<Self> {
        Self::de_socket_pedindo(socket, anfitriao, BUFFER_DE_RECEPCAO)
    }

    /// O [`Ponta::de_socket`] pedindo `recepcao` bytes de buffer: o teste pede
    /// pouco de propósito, para ver o enlace andar com o que o sistema der.
    fn de_socket_pedindo(
        socket: std::net::UdpSocket,
        anfitriao: Option<ConfigDoAnfitriao>,
        recepcao: usize,
    ) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        ajustar_buffers(&socket, recepcao);
        // Um segundo descritor do mesmo socket, só para mandar. O `try_send_to`
        // do Tokio depende da prontidão registrada, e o primeiro envio de um
        // socket recém-registrado voltava `WouldBlock` (medido: a primeira
        // Indication de um furo se perdia). O `send_to` do descritor não
        // bloqueante só falha com o buffer do sistema cheio.
        let saida = Arc::new(socket.try_clone()?);
        Self::com_socket(UdpSocket::from_std(socket)?, saida, anfitriao)
    }

    fn com_socket(
        socket: UdpSocket,
        saida: Arc<std::net::UdpSocket>,
        anfitriao: Option<ConfigDoAnfitriao>,
    ) -> io::Result<Self> {
        let local = socket.local_addr()?;
        let socket = Arc::new(socket);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (chegadas_tx, chegadas_rx) = mpsc::channel(16);
        let (enc_tx, enc_rx) = mpsc::channel(64);
        let vivas = Arc::new(AtomicUsize::new(0));
        let contadores = Arc::new(ContadoresDaPontaVivos::default());
        let (parada_tx, parada_rx) = watch::channel(false);
        let motor = Motor {
            socket,
            saida: Arc::clone(&saida),
            local,
            anfitriao,
            pares: HashMap::new(),
            credenciais: HashMap::new(),
            donos: HashMap::new(),
            lapides: HashMap::new(),
            ordem_das_lapides: VecDeque::new(),
            falhas_por_ip: HashMap::new(),
            criacoes_por_ip: HashMap::new(),
            cmd: cmd_tx.downgrade(),
            chegadas: chegadas_tx,
            enc: enc_tx,
            vivas: Arc::clone(&vivas),
            contadores: Arc::clone(&contadores),
        };
        tokio::spawn(async move {
            motor.rodar(cmd_rx).await;
            // O `motor` (com o socket de leitura e o de envio) já caiu aqui.
            let _ = parada_tx.send(true);
        });
        Ok(Self {
            interno: Arc::new(Interno {
                cmd: cmd_tx,
                local,
                socket: Mutex::new(Some(saida)),
                parada: parada_rx,
                chegadas: tokio::sync::Mutex::new(chegadas_rx),
                enc: Mutex::new(Some(enc_rx)),
                vivas,
                furos: Mutex::new(VecDeque::new()),
                contadores,
            }),
        })
    }

    /// O endereço do socket.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.interno.local
    }

    /// Anfitrião: a próxima conexão que chegou e terminou o aperto.
    pub async fn aceitar(&self) -> Option<Conexao> {
        self.interno.chegadas.lock().await.recv().await
    }

    /// Disca para um anfitrião.
    ///
    /// # Errors
    ///
    /// [`FalhaAoDiscar`], com o motivo que se sabe.
    pub async fn discar(&self, discagem: Discagem) -> Result<Conexao, FalhaAoDiscar> {
        let (resposta, espera) = oneshot::channel();
        self.interno
            .cmd
            .send(Comando::Discar { discagem, resposta })
            .map_err(|_| FalhaAoDiscar::PontaFechada)?;
        espera.await.map_err(|_| FalhaAoDiscar::PontaFechada)?
    }

    /// Os datagramas que não são STUN nem DTLS (o ENC/3). Entregue uma vez.
    #[must_use]
    pub fn mensagens_enc(&self) -> Option<mpsc::Receiver<(SocketAddr, Vec<u8>)>> {
        self.interno.enc.lock().ok().and_then(|mut enc| enc.take())
    }

    /// Manda um datagrama cru por este socket (o ENC/3).
    ///
    /// # Errors
    ///
    /// O socket que não aceitou.
    pub fn enviar_bruto(&self, destino: SocketAddr, bytes: &[u8]) -> io::Result<()> {
        let destino = para_o_socket(destino, self.interno.local);
        self.saida()?.send_to(bytes, destino).map(|_| ())
    }

    fn saida(&self) -> io::Result<Arc<std::net::UdpSocket>> {
        self.interno
            .socket
            .lock()
            .ok()
            .and_then(|socket| socket.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "a ponta foi fechada"))
    }

    /// Fura o NAT deste socket para quem liga (D7): Binding Indications a até
    /// [`CANDIDATOS_POR_FURO`] candidatos, [`INDICACOES_POR_CANDIDATO`] cada,
    /// espaçadas por 200 ms. Os candidatos vêm de quem liga e ninguém os
    /// autentica, então: só os que `serve_de_candidato` aceita, e no máximo
    /// [`FUROS_POR_MINUTO`] furos por minuto (R2). Devolve quantos candidatos
    /// foram furados.
    pub fn furar(&self, candidatos: &[SocketAddr]) -> usize {
        self.furar_com(candidatos, seele_proto::encontro::serve_de_candidato)
    }

    /// O [`Ponta::furar`] com o filtro dado: o teste usa um que aceita loopback.
    fn furar_com(&self, candidatos: &[SocketAddr], serve: fn(SocketAddr) -> bool) -> usize {
        let validos: Vec<SocketAddr> = candidatos
            .iter()
            .copied()
            .filter(|c| serve(*c))
            .take(CANDIDATOS_POR_FURO)
            .collect();
        if validos.is_empty() {
            self.interno
                .contadores
                .furos_recusados
                .fetch_add(1, Ordering::Relaxed);
            return 0;
        }
        let agora = Instant::now();
        {
            let Ok(mut furos) = self.interno.furos.lock() else {
                return 0;
            };
            while furos
                .front()
                .is_some_and(|t| agora.duration_since(*t) > Duration::from_secs(60))
            {
                furos.pop_front();
            }
            if furos.len() >= FUROS_POR_MINUTO {
                self.interno
                    .contadores
                    .furos_recusados
                    .fetch_add(1, Ordering::Relaxed);
                return 0;
            }
            furos.push_back(agora);
        }
        let Ok(socket) = self.saida() else {
            return 0;
        };
        // Fraca: um furo em curso não segura o socket depois de `fechar()`.
        let socket = Arc::downgrade(&socket);
        let local = self.interno.local;
        let quantos = validos.len();
        tokio::spawn(async move {
            for _ in 0..INDICACOES_POR_CANDIDATO {
                let Some(socket) = socket.upgrade() else {
                    return;
                };
                for destino in &validos {
                    let _ = socket.send_to(&indicacao(), para_o_socket(*destino, local));
                }
                drop(socket);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
        quantos
    }

    /// Atende um `CHAMADO` (D7, ENC/3): fura como [`Ponta::furar`] e, se o furo
    /// passou pelos tetos dele, entrega `credencial` e os candidatos à tarefa da
    /// ponta, que cria (ou reaproveita, pelo U) o par de anfitrião **já com os
    /// candidatos**, e o ICE completo deste lado os checa.
    ///
    /// A fila até a tarefa é limitada pelo mesmo teto do furo: no máximo
    /// [`FUROS_POR_MINUTO`] `CHAMADO`s por minuto, de [`CANDIDATOS_POR_FURO`]
    /// candidatos cada. Uma credencial fora da forma só fura. Devolve quantos
    /// candidatos foram furados.
    pub fn chamado(&self, credencial: &str, candidatos: &[SocketAddr]) -> usize {
        self.chamado_com(
            credencial,
            candidatos,
            seele_proto::encontro::serve_de_candidato,
        )
    }

    /// O [`Ponta::chamado`] com o filtro dado: o teste usa um que aceita loopback.
    fn chamado_com(
        &self,
        credencial: &str,
        candidatos: &[SocketAddr],
        serve: fn(SocketAddr) -> bool,
    ) -> usize {
        let furados = self.furar_com(candidatos, serve);
        if furados == 0 || !crate::credencial_valida(credencial) {
            return furados;
        }
        let validos: Vec<SocketAddr> = candidatos
            .iter()
            .copied()
            .filter(|c| serve(*c))
            .take(CANDIDATOS_POR_FURO)
            .collect();
        let _ = self.interno.cmd.send(Comando::Chamado {
            credencial: credencial.to_owned(),
            candidatos: validos,
        });
        furados
    }

    /// Derruba todas as conexões, para a tarefa e solta o socket. Depois de
    /// [`Ponta::fechada`], a porta voltou ao sistema, mesmo com outras cópias
    /// desta `Ponta` vivas.
    pub fn fechar(&self) {
        let _ = self.interno.cmd.send(Comando::Desligar);
        if let Ok(mut socket) = self.interno.socket.lock() {
            socket.take();
        }
    }

    /// Espera a tarefa da ponta sair e soltar os descritores: depois disto, a
    /// porta pode ser aberta de novo.
    pub async fn fechada(&self) {
        let mut parada = self.interno.parada.clone();
        let _ = parada.wait_for(|parou| *parou).await;
    }

    /// O buffer de recepção que o sistema deu a este socket, em bytes.
    #[must_use]
    pub fn buffer_de_recepcao(&self) -> Option<usize> {
        let socket = self.saida().ok()?;
        socket2::SockRef::from(socket.as_ref())
            .recv_buffer_size()
            .ok()
    }

    /// Quantas conexões a tarefa tem agora, inclusive as que estão no aperto.
    #[must_use]
    pub fn conexoes_vivas(&self) -> usize {
        self.interno.vivas.load(Ordering::Relaxed)
    }

    /// Os contadores da ponta.
    #[must_use]
    pub fn contadores(&self) -> ContadoresDaPonta {
        let c = &self.interno.contadores;
        ContadoresDaPonta {
            pendentes_despejados: c.pendentes_despejados.load(Ordering::Relaxed),
            pendentes_sem_dtls: c.pendentes_sem_dtls.load(Ordering::Relaxed),
            bindings_recusados: c.bindings_recusados.load(Ordering::Relaxed),
            recusa_pela_integridade: c.recusa_pela_integridade.load(Ordering::Relaxed),
            recusa_pelo_balde_de_criacao: c.recusa_pelo_balde_de_criacao.load(Ordering::Relaxed),
            recusa_pelo_teto_por_ip: c.recusa_pelo_teto_por_ip.load(Ordering::Relaxed),
            recusa_de_quem_hospeda: c.recusa_de_quem_hospeda.load(Ordering::Relaxed),
            bindings_em_lapide: c.bindings_em_lapide.load(Ordering::Relaxed),
            pares_criados: c.pares_criados.load(Ordering::Relaxed),
            pendentes_superados: c.pendentes_superados.load(Ordering::Relaxed),
            bindings_com_sessao_ativa: c.bindings_com_sessao_ativa.load(Ordering::Relaxed),
            panicos_contidos: c.panicos_contidos.load(Ordering::Relaxed),
            dtls_de_outra_sessao: c.dtls_de_outra_sessao.load(Ordering::Relaxed),
            bindings_em_espera: c.bindings_em_espera.load(Ordering::Relaxed),
            furos_recusados: c.furos_recusados.load(Ordering::Relaxed),
            datagramas_perdidos_na_saida: c.datagramas_perdidos_na_saida.load(Ordering::Relaxed),
            pares_do_chamado: c.pares_do_chamado.load(Ordering::Relaxed),
            chamados_em_lapide: c.chamados_em_lapide.load(Ordering::Relaxed),
        }
    }
}

/// Roda uma mutação do `Rtc` contendo um pânico do str0m ou do dimpl.
///
/// **Medido em 24/10**: um registro DTLS forjado de época 1 que chega a um par
/// ainda no aperto bate num `assert_eq!(seq_current.epoch, 0)` do dimpl 0.7.6
/// (`dtls12/engine.rs:381`), e o pânico derrubava a tarefa da ponta inteira, com
/// todas as conexões dela. A documentação do str0m recomenda `catch_unwind` («to
/// safely discard a faulty `Rtc` instance»), e o `Rtc` é `UnwindSafe`. O pânico
/// vira erro, e o erro descarta só aquele par.
fn blindar<T>(
    panicos: &std::sync::atomic::AtomicU64,
    mutacao: impl FnOnce() -> Result<T, str0m::RtcError>,
) -> Result<T, str0m::RtcError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(mutacao)) {
        Ok(resultado) => resultado,
        Err(_) => {
            panicos.fetch_add(1, Ordering::Relaxed);
            Err(str0m::RtcError::Io(io::Error::other(
                "o str0m entrou em pânico com um pacote do par, e o par foi descartado",
            )))
        }
    }
}

/// O buffer de recepção que a ponta pede ao sistema.
///
/// R4 na subida (medido em 24/10 com o Chromium e o WebKit mandando 4 canais de
/// volume a ~300 Mbit/s em loopback): com o buffer padrão do macOS, o
/// `netstat -s -p udp` contava milhares de «dropped due to full socket buffers»
/// por rajada, e a voz (sem retransmissão) perdia 6–9 % nas janelas em que a
/// rajada começa. O SCTP retransmite o volume; a voz, não.
pub const BUFFER_DE_RECEPCAO: usize = 4 * 1024 * 1024;
/// O buffer de envio que a ponta pede ao sistema.
pub const BUFFER_DE_ENVIO: usize = 1024 * 1024;

/// Pede ao sistema `recepcao` bytes de buffer de recepção e [`BUFFER_DE_ENVIO`]
/// de envio, e devolve o de recepção que ele **concedeu** (lido de volta).
///
/// O sistema pode dar menos: o teto é dele (`kern.ipc.maxsockbuf` no macOS,
/// `net.core.rmem_max` no Linux, que costuma ficar bem abaixo de 4 MiB). Nada
/// falha por isso, mas **o registro avisa**: com menos que o pedido, uma rajada
/// de volume pode transbordar o socket e perder voz, e o enlace não promete voz
/// sem perda nessas condições (`docs/evidencias/r4-subida-2026-10-24.md`). O
/// Linux devolve o dobro do concedido (a contabilidade dele), e por isso a
/// comparação é com o pedido, e não com a igualdade.
fn ajustar_buffers(socket: &std::net::UdpSocket, recepcao: usize) -> Option<usize> {
    let referencia = socket2::SockRef::from(socket);
    if let Err(erro) = referencia.set_recv_buffer_size(recepcao) {
        tracing::warn!(%erro, pedido = recepcao, "o sistema recusou o buffer de recepção pedido");
    }
    if let Err(erro) = referencia.set_send_buffer_size(BUFFER_DE_ENVIO) {
        tracing::debug!(%erro, "o sistema recusou o buffer de envio pedido");
    }
    let concedido = referencia.recv_buffer_size().ok();
    match concedido {
        Some(dado) if dado >= recepcao => tracing::debug!(
            recepcao = dado,
            envio = ?referencia.send_buffer_size().ok(),
            "buffers do socket da ponta"
        ),
        dado => tracing::warn!(
            pedido = recepcao,
            concedido = ?dado,
            "o sistema deu menos buffer de recepção que o pedido: uma rajada de volume \
             pode transbordar o socket e perder voz (no Linux, o teto é net.core.rmem_max)"
        ),
    }
    concedido
}

/// Uma STUN Binding Indication (RFC 8489): 20 bytes, sem atributo. Só abre o
/// mapeamento do NAT; quem recebe a ignora.
fn indicacao() -> [u8; 20] {
    let mut pacote = [0_u8; 20];
    let transacao: [u8; 12] = rand::random();
    pacote[..2].copy_from_slice(&0x0011_u16.to_be_bytes());
    pacote[4..8].copy_from_slice(&0x2112_A442_u32.to_be_bytes());
    pacote[8..].copy_from_slice(&transacao);
    pacote
}

/// Um endereço como o str0m o vê: IPv4 mapeado em IPv6 vira IPv4.
fn do_socket(endereco: SocketAddr) -> SocketAddr {
    match endereco {
        SocketAddr::V6(seis) => match seis.ip().to_ipv4_mapped() {
            Some(quatro) => SocketAddr::from((quatro, seis.port())),
            None => endereco,
        },
        SocketAddr::V4(_) => endereco,
    }
}

/// O inverso, para mandar por um socket IPv6 de pilha dupla.
fn para_o_socket(endereco: SocketAddr, local: SocketAddr) -> SocketAddr {
    match (endereco, local) {
        (SocketAddr::V4(quatro), SocketAddr::V6(_)) => {
            SocketAddr::from((quatro.ip().to_ipv6_mapped(), quatro.port()))
        }
        _ => endereco,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Papel {
    Anfitriao,
    Cliente,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tipo {
    Ctl,
    Voz,
    /// Um "u" que este lado abriu: só escreve.
    UniSaida,
    /// Um "u" que o par abriu: só lê.
    UniEntrada,
    Bi,
}

impl Tipo {
    const fn de_volume(self) -> bool {
        matches!(self, Self::UniSaida | Self::UniEntrada | Self::Bi)
    }
}

struct Pendente {
    mensagem: Vec<u8>,
    _credito: Option<tokio::sync::OwnedSemaphorePermit>,
    depois: Depois,
}

struct Canal {
    tipo: Tipo,
    aberto: bool,
    estado: Arc<EstadoDoCanal>,
    credito: Arc<Semaphore>,
    entrada: Option<mpsc::UnboundedSender<Entrada>>,
    na_fila: Arc<AtomicUsize>,
    pendentes: VecDeque<Pendente>,
    fim_local: bool,
    fim_remoto: bool,
    /// Fechar o canal quando o SCTP terminar de mandar o que tem dele.
    fechar: bool,
    /// Quem abriu localmente espera o `ChannelOpen`.
    abrindo: Option<RespostaDeAbrir>,
    /// Bytes de dados (corpos de `0x00`) que este lado já pôs no SCTP.
    enviados: u64,
    /// O limite absoluto que o par anunciou (`0x04`). Começa em 0: na linha 2 o
    /// crédito é obrigatório, e nenhum byte de dados sai antes do primeiro
    /// anúncio (só fim, corte, apresentação e crédito, que nunca esperam).
    limite_do_par: u64,
    /// O limite que este lado anunciou ao par (ou vai anunciar), quando ele lê
    /// este canal. Começa em 0: nada é concedido antes de [`rever_credito`].
    limite_para_o_par: u64,
    /// Bytes de dados que o par já mandou neste canal.
    recebidos: u64,
    /// A aplicação já pegou o `Recebe` (`accept_*`, `controle`, ou abriu o
    /// canal). Antes disso o canal não recebe crédito de dados: um canal que o
    /// par abre antes de a sessão existir (antes da prova D5, que o servidor
    /// confere no `ctl`) fica na fila de aceite sem guardar nenhum byte.
    aceito: bool,
    /// Um crédito à espera de espaço no SCTP. Ele não entra na fila dos dados:
    /// um crédito atrás de dados parados pelo crédito do par travaria os dois.
    credito_a_mandar: Option<u64>,
    /// Este lado já anunciou crédito neste canal.
    anunciou: bool,
    /// O corte deste lado já entrou na fila (fechar não manda dois).
    cortou: bool,
    /// O corte deste lado já saiu para o SCTP.
    corte_saiu: bool,
}

/// Quem pediu um canal espera os punhos dele, ou o motivo.
pub(crate) type RespostaDeAbrir = oneshot::Sender<Result<(Envio, Option<Recebe>), Motivo>>;
/// O que não é STUN nem DTLS, com a origem.
type FilaDoEnc = mpsc::Receiver<(SocketAddr, Vec<u8>)>;

impl Canal {
    fn novo(tipo: Tipo) -> Self {
        Self {
            tipo,
            aberto: false,
            estado: Arc::new(EstadoDoCanal::default()),
            credito: Arc::new(Semaphore::new(crate::CREDITO_DE_SAIDA)),
            entrada: None,
            na_fila: Arc::new(AtomicUsize::new(0)),
            pendentes: VecDeque::new(),
            fim_local: false,
            fim_remoto: false,
            fechar: false,
            abrindo: None,
            enviados: 0,
            limite_do_par: 0,
            limite_para_o_par: 0,
            recebidos: 0,
            aceito: false,
            credito_a_mandar: None,
            anunciou: false,
            cortou: false,
            corte_saiu: false,
        }
    }

    /// Se este lado lê este canal (e por isso anuncia crédito nele).
    const fn le(&self) -> bool {
        matches!(self.tipo, Tipo::Ctl | Tipo::UniEntrada | Tipo::Bi)
    }

    fn morrer(&mut self, falha: crate::FalhaDoCanal) {
        self.estado.parar(falha);
        self.credito.close();
        self.entrada = None;
        self.pendentes.clear();
    }
}

struct Par {
    rtc: Rtc,
    papel: Papel,
    comp: Arc<Compartilhado>,
    ip: IpAddr,
    /// A origem do Binding que criou o par (o anfitrião) ou o primeiro alvo (quem liga).
    origem: SocketAddr,
    /// Aposentado por um aperto novo da mesma origem: não conta como falha do IP.
    superado: bool,
    /// O anfitrião já viu o ClientHello (época 0) desta sessão.
    viu_client_hello: bool,
    /// O par já anunciou crédito (`0x04`) em algum canal: ele fala a
    /// contrapressão. Quem liga sempre anuncia (obrigatório na linha 2); o
    /// anfitrião anuncia a partir do primeiro `0x04` do par, e por isso a
    /// apresentação continua sendo a primeira mensagem do `ctl` dele.
    fala_credito: bool,
    /// O código do fechamento pedido (`close`), para a despedida no fio.
    codigo_de_fechar: Option<u32>,
    /// Os anúncios de crédito estão retidos (só pela [`crate::teste`]).
    credito_retido: bool,
    /// As filas de canais que já fecharam no SCTP e ainda têm bytes que a
    /// aplicação não leu: contam no crédito agregado até esvaziar.
    filas_orfas: Vec<Arc<AtomicUsize>>,
    canais: HashMap<ChannelId, Canal>,
    ctl: ChannelId,
    voz: ChannelId,
    conectado: bool,
    pronto: bool,
    dtls_visto: bool,
    algum_pacote: bool,
    falhas_de_envio: u64,
    criado: Instant,
    prazo: Instant,
    ultimo_rx: Instant,
    ultimo_tx: Instant,
    /// Desde quando o anfitrião espera o primeiro DTLS ([`ESPERA_PELO_DTLS`]).
    /// Só começa depois de o ICE nomear um caminho. Um Binding inicial ainda
    /// pode ser apenas uma checagem; até a nomeação vale o prazo geral do aperto.
    espera_do_dtls_desde: Option<Instant>,
    /// Os candidatos que os `CHAMADO`s deram a este par (anfitrião).
    remotos_do_chamado: Vec<SocketAddr>,
    proximo: Instant,
    fechando: Option<Instant>,
    motivo_remoto: Option<Motivo>,
    espera: Option<oneshot::Sender<Result<Conexao, crate::FalhaAoDiscar>>>,
    lado: Option<LadoDaTarefa>,
    revezamento: usize,
    apresentacao: Vec<u8>,
}

struct Motor {
    socket: Arc<UdpSocket>,
    saida: Arc<std::net::UdpSocket>,
    local: SocketAddr,
    anfitriao: Option<ConfigDoAnfitriao>,
    pares: HashMap<usize, Par>,
    /// O U de cada conexão viva: um Binding vai pelo U, e um U vivo nunca cria outra.
    credenciais: HashMap<String, usize>,
    /// O par do último Binding de cada origem: é para ele que o DTLS dela vai.
    donos: HashMap<SocketAddr, usize>,
    lapides: HashMap<String, Instant>,
    ordem_das_lapides: VecDeque<(String, Instant)>,
    falhas_por_ip: HashMap<IpAddr, (u32, Instant)>,
    criacoes_por_ip: HashMap<IpAddr, VecDeque<Instant>>,
    cmd: mpsc::WeakUnboundedSender<Comando>,
    chegadas: mpsc::Sender<Conexao>,
    enc: mpsc::Sender<(SocketAddr, Vec<u8>)>,
    vivas: Arc<AtomicUsize>,
    contadores: Arc<ContadoresDaPontaVivos>,
}

const IMPRESSAO_DE_ENCHIMENTO: [u8; 32] = [0; 32];

fn impressao_str0m(bytes: [u8; 32]) -> Fingerprint {
    Fingerprint {
        hash_func: "sha-256".into(),
        bytes: bytes.to_vec(),
    }
}

fn configurar_canais(rtc: &mut Rtc) -> (ChannelId, ChannelId) {
    let mut api = rtc.direct_api();
    let ctl = api.create_data_channel(ChannelConfig {
        label: crate::LABEL_CTL.into(),
        ordered: true,
        reliability: Reliability::Reliable,
        negotiated: Some(0),
        protocol: String::new(),
    });
    let voz = api.create_data_channel(ChannelConfig {
        label: crate::LABEL_VOZ.into(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        negotiated: Some(1),
        protocol: String::new(),
    });
    (ctl, voz)
}

impl Motor {
    async fn rodar(mut self, mut comandos: mpsc::UnboundedReceiver<Comando>) {
        let mut buf = vec![0_u8; 2048];
        loop {
            let agora = Instant::now();
            self.tique(agora);
            let acordar = self.proximo_acordar(agora);
            tokio::select! {
                lido = self.socket.recv_from(&mut buf) => {
                    match lido {
                        Ok((n, origem)) => {
                            if let Some(bytes) = buf.get(..n) {
                                self.chegou(Instant::now(), do_socket(origem), bytes);
                            }
                            // Ler em lote (64 `try_recv_from` por volta) foi
                            // medido no R4 da subida e não mudou nem a perda nem
                            // o atraso da voz: ficou de fora. O que resolveu foi
                            // o buffer de recepção (`BUFFER_DE_RECEPCAO`).
                        }
                        Err(erro) => {
                            // No Windows, um ICMP de porta fechada volta como erro
                            // de leitura (WSAECONNRESET): não é o socket que morreu.
                            tracing::trace!(%erro, "leitura do socket falhou");
                        }
                    }
                }
                comando = comandos.recv() => {
                    match comando {
                        Some(Comando::Desligar) | None => {
                            self.desligar();
                            return;
                        }
                        Some(comando) => self.comando(Instant::now(), comando),
                    }
                }
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(acordar)) => {}
            }
        }
    }

    fn desligar(&mut self) {
        let ids: Vec<usize> = self.pares.keys().copied().collect();
        for id in ids {
            // A ponta que desliga se despede de cada par pronto com o corte no
            // `ctl` (o código que o `close` pediu, ou 0): quem estava dentro lê
            // «fechada», e não «caiu». O QUIC mandava `CONNECTION_CLOSE` a todos.
            self.despedir_no_fio(id);
            self.encerrar(id, Motivo::Local, None);
        }
    }

    fn proximo_acordar(&self, agora: Instant) -> Instant {
        let mut acordar = agora + Duration::from_secs(1);
        for par in self.pares.values() {
            acordar = acordar.min(par.proximo).min(par.prazo);
            if let Some(fechando) = par.fechando {
                acordar = acordar.min(fechando);
            }
            if par.pronto {
                acordar = acordar
                    .min(par.ultimo_rx + crate::OCIOSO)
                    .min(par.ultimo_tx + crate::KEEPALIVE);
            }
        }
        acordar.max(agora + Duration::from_millis(1))
    }

    fn virtual_local(&self, ipv4: bool) -> SocketAddr {
        let porta = self.local.port();
        let ip = self.local.ip();
        if !ip.is_unspecified() && ip.is_ipv4() == ipv4 {
            return SocketAddr::new(ip, porta);
        }
        if ipv4 {
            SocketAddr::from((Ipv4Addr::LOCALHOST, porta))
        } else {
            SocketAddr::from((Ipv6Addr::LOCALHOST, porta))
        }
    }

    // ---- entrada da rede --------------------------------------------------

    fn chegou(&mut self, agora: Instant, origem: SocketAddr, bytes: &[u8]) {
        let Some(&primeiro) = bytes.first() else {
            return;
        };
        let e_stun = primeiro <= 3;
        let e_dtls = (20..=63).contains(&primeiro);
        if !e_stun && !e_dtls {
            let _ = self.enc.try_send((origem, bytes.to_vec()));
            return;
        }
        let destino = self.virtual_local(origem.is_ipv4());
        let Ok(recebido) = Receive::new(Protocol::Udp, origem, destino, bytes) else {
            return;
        };
        let entrada = Input::Receive(agora, recebido);

        // **Um Binding com USERNAME `U:U` vai pelo U, e nunca pela origem.** O
        // `Rtc::accepts` do str0m aceita qualquer pacote vindo do endereço que ele
        // nomeou; um Binding com outro U do mesmo socket (quem reconecta da mesma
        // porta) cairia no `Rtc` velho. E um U que já tem conexão nunca cria outra.
        if e_stun {
            if let Some(u) = binding_com_u(bytes) {
                let ativa = self.ativa_da_origem(origem);
                if let Some(&id) = self.credenciais.get(&u) {
                    if ativa.is_none_or(|ativa| ativa == id) {
                        self.donos.insert(origem, id);
                    }
                    self.entregar(id, agora, entrada, bytes.len(), None);
                    return;
                }
                if self.anfitriao.is_none() {
                    return;
                }
                // **Uma sessão ativa é dona da sua 5-tupla.** A associação DTLS é por
                // origem: com uma sessão pronta vinda de `origem`, um U novo da mesma
                // origem não cria par nenhum. O U não é segredo, e um par novo
                // disputaria o DTLS da sessão (quem forja a origem a derrubaria). Quem
                // redisca da mesma porta com a sessão antiga ainda viva fica sem
                // resposta até ela acabar: fechada pelo corte, ou pelo ocioso de 20 s.
                if ativa.is_some() {
                    self.contadores
                        .bindings_com_sessao_ativa
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                if self.em_lapide(agora, &u) {
                    self.contadores
                        .bindings_em_lapide
                        .fetch_add(1, Ordering::Relaxed);
                    // Quem continua batendo com um U enterrado o mantém enterrado:
                    // a lápide conta do último Binding, e não da morte do par.
                    self.enterrar(agora, u);
                    return;
                }
                if let Some(id) = self.talvez_criar(agora, origem, bytes, u) {
                    self.donos.insert(origem, id);
                    self.entregar(id, agora, entrada, bytes.len(), None);
                }
                return;
            }
        }
        // **O DTLS de uma origem vai ao par do último Binding dela.** Vários pares
        // pendentes podem ter a mesma origem como candidato (quem redisca da mesma
        // porta, depois de recusar um candidato pelo pino), e o `Rtc::accepts`
        // aceitaria o DTLS em qualquer um deles: o par velho comia o aperto do
        // novo, que morria sem DTLS (medido: a corrida do `seele-core`). O
        // `accepts` continua sendo o recuo, para quem só manda DTLS.
        //
        // E a ordem é: a sessão ativa da origem, se há uma; senão o par do último
        // Binding. Um par pendente nunca recebe o DTLS de uma sessão ativa.
        let preferido = self
            .ativa_da_origem(origem)
            .or_else(|| self.donos.get(&origem).copied())
            .filter(|id| {
                self.pares
                    .get(id)
                    .is_some_and(|par| par.rtc.accepts(&entrada))
            });
        let dono = preferido.or_else(|| {
            self.pares
                .iter()
                .find(|(_, par)| par.rtc.accepts(&entrada))
                .map(|(id, _)| *id)
        });
        if let Some(id) = dono {
            if e_dtls && !self.dtls_e_desta_sessao(id, bytes) {
                self.contadores
                    .dtls_de_outra_sessao
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            self.entregar(id, agora, entrada, bytes.len(), e_dtls.then_some(origem));
        }
    }

    /// **Um registro DTLS de outra sessão da mesma 5-tupla nunca entra num par
    /// que ainda espera o aperto.** Com a mesma origem, o UDP não separa a sessão
    /// velha da nova; o DTLS separa pela época. Antes de ver o primeiro registro de
    /// aperto (conteúdo 22, época 0) do par nesta sessão — o ClientHello, no
    /// anfitrião; o ServerHello, em quem liga —, nenhum registro de época ≥ 1 pode
    /// ser desta sessão: só a anterior (o `close_notify` dela, um dado atrasado) os
    /// teria. Medido: o `close_notify` atrasado da sessão velha fechava o aperto
    /// da nova, dos dois lados, de vez em quando, em
    /// `fechar_e_rediscar_da_mesma_porta_na_hora_sempre_abre`.
    fn dtls_e_desta_sessao(&mut self, id: usize, bytes: &[u8]) -> bool {
        let Some(par) = self.pares.get_mut(&id) else {
            return false;
        };
        if par.conectado || par.viu_client_hello {
            return true;
        }
        let (Some(&tipo), Some(epoca)) = (bytes.first(), bytes.get(3..5)) else {
            return false;
        };
        if epoca != [0, 0] {
            return false;
        }
        if tipo == 22 {
            par.viu_client_hello = true;
        }
        true
    }

    /// Põe o corte de fechamento no SCTP **agora**, na frente do que ainda não
    /// saiu do `ctl`: é a última coisa que o par recebe antes do DTLS fechar.
    /// Sem efeito se o par não está pronto, ou se o corte já saiu.
    fn despedir_no_fio(&mut self, id: usize) {
        {
            let Some(par) = self.pares.get_mut(&id) else {
                return;
            };
            if !par.pronto {
                return;
            }
            let codigo = par.codigo_de_fechar.unwrap_or(0);
            let ctl = par.ctl;
            let Some(c) = par.canais.get_mut(&ctl) else {
                return;
            };
            if c.corte_saiu {
                return;
            }
            c.pendentes.clear();
            c.pendentes.push_front(Pendente {
                mensagem: crate::envelope::corte(codigo),
                _credito: None,
                depois: Depois::Corte,
            });
            c.cortou = true;
            c.fim_local = true;
        }
        self.escrever(id);
    }

    /// A sessão pronta (DTLS fechado, ctl e voz abertos) que veio de `origem`.
    fn ativa_da_origem(&self, origem: SocketAddr) -> Option<usize> {
        self.pares
            .iter()
            .find(|(_, par)| {
                par.pronto
                    && par.fechando.is_none()
                    && par.comp.remoto.lock().is_ok_and(|remoto| *remoto == origem)
            })
            .map(|(id, _)| *id)
    }

    /// Se o U está em lápide: a conexão dele terminou há menos de [`LAPIDE`].
    fn em_lapide(&mut self, agora: Instant, u: &str) -> bool {
        while let Some((mais_velho, quando)) = self.ordem_das_lapides.front() {
            if agora.duration_since(*quando) < LAPIDE {
                break;
            }
            if self.lapides.get(mais_velho) == Some(quando) {
                self.lapides.remove(mais_velho);
            }
            self.ordem_das_lapides.pop_front();
        }
        self.lapides
            .get(u)
            .is_some_and(|quando| agora.duration_since(*quando) < LAPIDE)
    }

    fn enterrar(&mut self, agora: Instant, u: String) {
        self.lapides.insert(u.clone(), agora);
        self.ordem_das_lapides.push_back((u, agora));
        // A fila de ordem cresce a cada Binding que renova uma lápide; o teto
        // dela é o que impede quem bate rápido de crescer a memória.
        while self.lapides.len() > MAX_LAPIDES || self.ordem_das_lapides.len() > 4 * MAX_LAPIDES {
            let Some((mais_velho, quando)) = self.ordem_das_lapides.pop_front() else {
                break;
            };
            if self.lapides.get(&mais_velho) == Some(&quando) {
                self.lapides.remove(&mais_velho);
            }
        }
    }

    /// Uma recusa de Binding: o contador do motivo e a soma.
    fn recusar(&self, motivo: &std::sync::atomic::AtomicU64) {
        motivo.fetch_add(1, Ordering::Relaxed);
        self.contadores
            .bindings_recusados
            .fetch_add(1, Ordering::Relaxed);
    }

    /// O balde de criações por IP: no máximo [`CRIACOES_POR_IP`] apertos **que
    /// não terminaram** em [`JANELA_DE_CRIACAO`]. Só consulta (a vaga se gasta em
    /// [`Self::gastar_uma_criacao`], quando o par nasce); cheio, devolve quantas
    /// há e a idade da mais velha.
    ///
    /// Um aperto que termina devolve a vaga ([`Self::devolver_a_criacao`]). O
    /// laço que isto para é o de criações que nunca terminam: U novos da mesma
    /// porta sem DTLS, ou o navegador que recria o par depois de um alerta
    /// fatal (o spike C). Apertos completos de um mesmo endereço — uma casa ou
    /// um escritório atrás de um NAT — quem limita é quem hospeda (`admitir`, o
    /// balde do servidor por endereço, `seele-server/src/taxa.rs`: 30 de rajada
    /// e 30 por minuto). Medido em 25/10/2026: contando também os completos, a
    /// nona pessoa do mesmo IP em 10 s ficava sem resposta, e o servidor
    /// promete trinta.
    fn cabe_uma_criacao(
        &mut self,
        agora: Instant,
        ip: IpAddr,
    ) -> Result<(), (usize, Option<Duration>)> {
        if self.criacoes_por_ip.len() >= MAX_LAPIDES && !self.criacoes_por_ip.contains_key(&ip) {
            self.criacoes_por_ip.retain(|_, vezes| {
                vezes
                    .back()
                    .is_some_and(|t| agora.duration_since(*t) < JANELA_DE_CRIACAO)
            });
            if self.criacoes_por_ip.len() >= MAX_LAPIDES {
                return Err((0, None));
            }
        }
        let vezes = self.criacoes_por_ip.entry(ip).or_default();
        while vezes
            .front()
            .is_some_and(|t| agora.duration_since(*t) >= JANELA_DE_CRIACAO)
        {
            vezes.pop_front();
        }
        if vezes.len() >= CRIACOES_POR_IP {
            return Err((vezes.len(), vezes.front().map(|t| agora.duration_since(*t))));
        }
        Ok(())
    }

    /// Gasta a vaga de um par que nasceu agora (depois de [`Self::cabe_uma_criacao`]).
    fn gastar_uma_criacao(&mut self, criado: Instant, ip: IpAddr) {
        self.criacoes_por_ip
            .entry(ip)
            .or_default()
            .push_back(criado);
    }

    /// Devolve a vaga de um par que terminou o aperto.
    fn devolver_a_criacao(&mut self, criado: Instant, ip: IpAddr) {
        if let Some(vezes) = self.criacoes_por_ip.get_mut(&ip) {
            if let Some(posicao) = vezes.iter().position(|t| *t == criado) {
                vezes.remove(posicao);
            }
            if vezes.is_empty() {
                self.criacoes_por_ip.remove(&ip);
            }
        }
    }

    /// Se este IP está esperando depois de falhas seguidas.
    fn ip_em_espera(&self, agora: Instant, ip: IpAddr) -> bool {
        self.falhas_por_ip.get(&ip).is_some_and(|(falhas, ultima)| {
            *falhas >= FALHAS_ANTES_DA_ESPERA && agora < *ultima + espera_por_falhas(*falhas)
        })
    }

    fn falhou_o_ip(&mut self, agora: Instant, ip: IpAddr) {
        if self.falhas_por_ip.len() >= MAX_LAPIDES && !self.falhas_por_ip.contains_key(&ip) {
            // Cheio: esquece os que já passaram da espera mais longa.
            self.falhas_por_ip
                .retain(|_, (_, ultima)| agora.duration_since(*ultima) < ESPERA_MAXIMA);
            if self.falhas_por_ip.len() >= MAX_LAPIDES {
                return;
            }
        }
        let entrada = self.falhas_por_ip.entry(ip).or_insert((0, agora));
        if agora.duration_since(entrada.1) > LAPIDE {
            entrada.0 = 0;
        }
        entrada.0 = entrada.0.saturating_add(1);
        entrada.1 = agora;
    }

    fn entregar(
        &mut self,
        id: usize,
        agora: Instant,
        entrada: Input<'_>,
        tamanho: usize,
        dtls_de: Option<SocketAddr>,
    ) {
        let resultado = {
            let Some(par) = self.pares.get_mut(&id) else {
                return;
            };
            par.ultimo_rx = agora;
            par.algum_pacote = true;
            if let Some(origem) = dtls_de {
                par.dtls_visto = true;
                // O DTLS só anda pelo par que o ICE nomeou: a origem dele é o
                // candidato que venceu, já canônica. É o que `remote_address()`
                // devolve assim que o `discar` termina, e não o primeiro alvo.
                if let Ok(mut remoto) = par.comp.remoto.lock() {
                    if *remoto != origem {
                        *remoto = origem;
                    }
                }
            }
            par.comp
                .bytes_recebidos
                .fetch_add(tamanho as u64, Ordering::Relaxed);
            blindar(&self.contadores.panicos_contidos, || {
                par.rtc.handle_input(entrada)
            })
        };
        if let Err(erro) = resultado {
            self.falhou(id, &erro);
            return;
        }
        self.drenar(id);
        self.escrever(id);
    }

    /// Um Binding que ninguém aceitou: um par novo, se passar pelos portões.
    fn talvez_criar(
        &mut self,
        agora: Instant,
        origem: SocketAddr,
        bytes: &[u8],
        u: String,
    ) -> Option<usize> {
        let mensagem = str0m::ice::StunMessage::parse(bytes).ok()?;
        let hmac = |chave: &[u8], partes: &[&[u8]]| crate::cripto::hmac_sha1(chave, partes);
        // Cada portão recusa com um motivo próprio, num contador e num
        // `tracing`: uma recusa aqui é silêncio do outro lado («nada
        // respondeu»), e o motivo só existe deste lado.
        if !mensagem.verify(u.as_bytes(), hmac) {
            self.recusar(&self.contadores.recusa_pela_integridade);
            tracing::debug!(%origem, "Binding recusado: o MESSAGE-INTEGRITY não confere com o U");
            return None;
        }
        let id = self.criar_anfitriao(agora, origem, u, false)?;
        tracing::debug!(conexao = id, %origem, "um par novo bateu");
        // **Um aperto novo da mesma origem aposenta os pendentes dela.** São os
        // pares que os checks de discagens abandonadas criaram (o ICE de quem
        // liga checa todos os alvos e fica com um): eles nunca vão terminar, e
        // só disputariam o DTLS do novo. Os prontos ficam.
        let superados: Vec<usize> = self
            .pares
            .iter()
            .filter(|(outro, par)| {
                // Pendente ou abandonado: sem DTLS nenhum, ou com o aperto parado
                // há mais de um segundo. Um aperto com DTLS andando não é superado.
                let parado =
                    !par.dtls_visto || agora.duration_since(par.ultimo_rx) > Duration::from_secs(1);
                **outro != id
                    && par.papel == Papel::Anfitriao
                    && !par.pronto
                    && par.origem == origem
                    && parado
            })
            .map(|(outro, _)| *outro)
            .collect();
        for superado in superados {
            if let Some(par) = self.pares.get_mut(&superado) {
                par.superado = true;
            }
            self.contadores
                .pendentes_superados
                .fetch_add(1, Ordering::Relaxed);
            self.encerrar(
                superado,
                Motivo::Caiu("superado por um aperto novo da mesma origem".into()),
                None,
            );
        }
        Some(id)
    }

    /// Um `CHAMADO` (D7) que a ponta aceitou: o par nasce **agora**, com os
    /// candidatos de quem liga, e o ICE completo deste lado checa cada um.
    ///
    /// É o que abre o NAT de casa que muda a porta conforme o destino (medido em
    /// 26/10/2026: 14178 para o encontro, 14224 para o relay de quem liga): a
    /// checagem sai pela porta que o NAT abre para quem liga, e quem liga aprende
    /// dela o candidato peer-reflexive (RFC 8445 7.3.1.3) e checa de volta por ali.
    /// As checagens são do str0m, com o MESSAGE-INTEGRITY da credencial U.
    ///
    /// Se o U já tem par (o Binding direto chegou antes, ou o `CHAMADO` veio
    /// repetido), os candidatos entram nele, e nenhum par novo nasce. Um U em
    /// lápide não nasce de novo. O resto são os mesmos portões do Binding.
    fn chamado(&mut self, agora: Instant, u: String, candidatos: Vec<SocketAddr>) {
        if self.anfitriao.is_none() || !crate::credencial_valida(&u) {
            return;
        }
        let candidatos: Vec<SocketAddr> = candidatos
            .into_iter()
            .map(do_socket)
            .filter(|c| self.local.is_ipv6() || c.is_ipv4())
            .take(CANDIDATOS_POR_FURO)
            .collect();
        let Some(&primeiro) = candidatos.first() else {
            return;
        };
        let id = if let Some(&id) = self.credenciais.get(&u) {
            id
        } else {
            if self.em_lapide(agora, &u) {
                self.contadores
                    .chamados_em_lapide
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            let Some(id) = self.criar_anfitriao(agora, primeiro, u, true) else {
                return;
            };
            self.contadores
                .pares_do_chamado
                .fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                conexao = id,
                candidatos = candidatos.len(),
                "degrau 4: o par do CHAMADO nasceu; o ICE deste lado checa os candidatos de quem liga"
            );
            id
        };
        self.juntar_candidatos(id, agora, &candidatos);
    }

    /// Os candidatos de um `CHAMADO` entram no par (só um anfitrião ainda no
    /// aperto, sem repetir, e no máximo [`CANDIDATOS_POR_FURO`] por par).
    fn juntar_candidatos(&mut self, id: usize, agora: Instant, candidatos: &[SocketAddr]) {
        let resultado = {
            let Some(par) = self.pares.get_mut(&id) else {
                return;
            };
            if par.papel != Papel::Anfitriao || par.pronto {
                return;
            }
            for candidato in candidatos {
                if par.remotos_do_chamado.len() >= CANDIDATOS_POR_FURO
                    || par.remotos_do_chamado.contains(candidato)
                {
                    continue;
                }
                if let Ok(remoto) = Candidate::host(*candidato, "udp") {
                    par.rtc.add_remote_candidate(remoto);
                    par.remotos_do_chamado.push(*candidato);
                }
            }
            blindar(&self.contadores.panicos_contidos, || {
                par.rtc.handle_input(Input::Timeout(agora))
            })
        };
        if let Err(erro) = resultado {
            self.falhou(id, &erro);
            return;
        }
        self.drenar(id);
    }

    /// Os portões e o par de anfitrião: o caminho comum do Binding que ninguém
    /// aceitou e do `CHAMADO`. `origem` é de onde o Binding veio, ou o primeiro
    /// candidato do `CHAMADO`.
    fn criar_anfitriao(
        &mut self,
        agora: Instant,
        origem: SocketAddr,
        u: String,
        pelo_chamado: bool,
    ) -> Option<usize> {
        let ip = origem.ip();
        if self.ip_em_espera(agora, ip) {
            self.contadores
                .bindings_em_espera
                .fetch_add(1, Ordering::Relaxed);
            tracing::debug!(%ip, "Binding recusado: o IP espera depois de falhas seguidas");
            return None;
        }
        let pendentes_do_ip = self
            .pares
            .values()
            .filter(|p| !p.pronto && p.papel == Papel::Anfitriao && p.ip == ip)
            .count();
        if pendentes_do_ip >= crate::MAX_PENDENTES_POR_IP {
            self.recusar(&self.contadores.recusa_pelo_teto_por_ip);
            tracing::info!(%ip, pendentes_do_ip, "Binding recusado: o teto de pendentes por IP");
            return None;
        }
        if let Err((no_balde, mais_velha)) = self.cabe_uma_criacao(agora, ip) {
            self.recusar(&self.contadores.recusa_pelo_balde_de_criacao);
            tracing::info!(
                %ip,
                no_balde,
                idade_da_mais_velha = ?mais_velha,
                "Binding recusado: o balde de criações do IP (apertos que não terminaram) está cheio"
            );
            return None;
        }
        let pendentes: Vec<(usize, Instant)> = self
            .pares
            .iter()
            .filter(|(_, p)| !p.pronto && p.papel == Papel::Anfitriao)
            .map(|(id, p)| (*id, p.criado))
            .collect();
        let (admitido, certificado, apresentacao) = {
            let anfitriao = self.anfitriao.as_ref()?;
            (
                (anfitriao.admitir)(ip),
                anfitriao.certificado.clone(),
                anfitriao.apresentacao.clone(),
            )
        };
        if !admitido {
            self.recusar(&self.contadores.recusa_de_quem_hospeda);
            tracing::info!(%ip, "aperto recusado antes de criar o par: quem hospeda não admitiu este endereço agora (o balde dele está vazio, ou ocupado)");
            return None;
        }
        if pendentes.len() >= crate::MAX_PENDENTES {
            if let Some((mais_velho, _)) = pendentes.iter().min_by_key(|(_, criado)| *criado) {
                self.contadores
                    .pendentes_despejados
                    .fetch_add(1, Ordering::Relaxed);
                self.encerrar(
                    *mais_velho,
                    Motivo::Caiu("despejado: o teto de apertos pendentes encheu".into()),
                    None,
                );
            }
        }
        // **ICE completo e controlado** (e não mais ICE-lite): o anfitrião checa
        // os candidatos que conhece, aprende o peer-reflexive de quem liga e
        // responde. Quem liga (o nativo ou a página) segue controlador e nomeia.
        let mut config = Rtc::builder()
            .set_crypto_provider(crate::cripto::provedor())
            .set_local_ice_credentials(IceCreds {
                ufrag: u.clone(),
                pass: u.clone(),
            })
            .set_dtls_cert(certificado.para_o_str0m())
            .set_fingerprint_verification(false)
            .set_sctp_max_buffered_amount(crate::BUFFER_DO_SCTP)
            .set_stats_interval(Some(Duration::from_secs(1)));
        if pelo_chamado {
            // Como na discagem (R2): as checagens se repetem pelo aperto inteiro,
            // para cobrir quem liga que ainda não tem a descrição deste lado.
            config.set_initial_stun_rto(Duration::from_millis(250));
            config.set_max_stun_rto(Duration::from_millis(1000));
            let tentativas = usize::try_from(crate::APERTO.as_millis() / 500)
                .unwrap_or(20)
                .max(4);
            config.set_max_stun_retransmits(tentativas);
        }
        let mut rtc = config.build(agora);
        {
            let mut api = rtc.direct_api();
            api.set_ice_controlling(false);
            api.set_remote_ice_credentials(IceCreds {
                ufrag: u.clone(),
                pass: u.clone(),
            });
            api.set_remote_fingerprint(impressao_str0m(IMPRESSAO_DE_ENCHIMENTO));
            if api.start_dtls(false).is_err() {
                return None;
            }
            api.start_sctp(false);
        }
        let (ctl, voz) = configurar_canais(&mut rtc);
        for ipv4 in [true, false] {
            if let Ok(candidato) = Candidate::host(self.virtual_local(ipv4), "udp") {
                rtc.add_local_candidate(candidato);
            }
        }
        let id = self.inserir(
            rtc,
            Papel::Anfitriao,
            origem,
            u,
            certificado.impressao(),
            ctl,
            voz,
            agora,
            crate::APERTO,
            None,
        );
        if let Some(par) = self.pares.get_mut(&id) {
            par.apresentacao = apresentacao;
            let (criado, ip) = (par.criado, par.ip);
            self.gastar_uma_criacao(criado, ip);
        }
        self.contadores
            .pares_criados
            .fetch_add(1, Ordering::Relaxed);
        Some(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn inserir(
        &mut self,
        rtc: Rtc,
        papel: Papel,
        remoto: SocketAddr,
        credencial: String,
        impressao_local: [u8; 32],
        ctl: ChannelId,
        voz: ChannelId,
        agora: Instant,
        prazo: Duration,
        espera: Option<oneshot::Sender<Result<Conexao, crate::FalhaAoDiscar>>>,
    ) -> usize {
        let id = PROXIMO_ID.fetch_add(1, Ordering::Relaxed);
        self.credenciais.insert(credencial.clone(), id);
        let (motivo, _) = watch::channel(None);
        let comp = Arc::new(Compartilhado {
            id,
            remoto: Mutex::new(remoto),
            impressao_local,
            impressao_par: OnceLock::new(),
            apresentacao: OnceLock::new(),
            credencial,
            motivo,
            bytes_enviados: Default::default(),
            pacotes_enviados: Default::default(),
            bytes_recebidos: Default::default(),
            rtt_us: Default::default(),
            perda_na_saida: Mutex::new(None),
            voz_na_fila: Default::default(),
            filas_vivas: Default::default(),
            credito_em_aberto: Default::default(),
            pausa_de_teste: Default::default(),
            contadores: ContadoresVivos::default(),
        });
        let mut canais = HashMap::new();
        canais.insert(ctl, Canal::novo(Tipo::Ctl));
        canais.insert(voz, Canal::novo(Tipo::Voz));
        self.pares.insert(
            id,
            Par {
                rtc,
                papel,
                comp,
                ip: remoto.ip(),
                origem: remoto,
                superado: false,
                viu_client_hello: false,
                fala_credito: false,
                codigo_de_fechar: None,
                credito_retido: false,
                filas_orfas: Vec::new(),
                canais,
                ctl,
                voz,
                conectado: false,
                pronto: false,
                dtls_visto: false,
                algum_pacote: false,
                falhas_de_envio: 0,
                criado: agora,
                prazo: agora + prazo,
                ultimo_rx: agora,
                ultimo_tx: agora,
                espera_do_dtls_desde: None,
                remotos_do_chamado: Vec::new(),
                proximo: agora,
                fechando: None,
                motivo_remoto: None,
                espera,
                lado: None,
                revezamento: 0,
                apresentacao: Vec::new(),
            },
        );
        self.vivas.store(self.pares.len(), Ordering::Relaxed);
        id
    }

    // ---- comandos ---------------------------------------------------------

    fn comando(&mut self, agora: Instant, comando: Comando) {
        match comando {
            Comando::Escrever {
                par,
                canal,
                mensagem,
                credito,
                depois,
            } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    if let Some(c) = p.canais.get_mut(&canal) {
                        if !c.fim_local
                            && c.estado.parado.lock().map(|e| e.is_none()).unwrap_or(false)
                        {
                            if depois != Depois::Nada {
                                c.fim_local = true;
                            }
                            let pendente = Pendente {
                                mensagem,
                                _credito: credito,
                                depois,
                            };
                            if depois == Depois::Corte {
                                // O corte passa na frente do que ainda não saiu: o
                                // fluxo acabou, e mandar o resto seria desperdício.
                                c.pendentes.clear();
                            }
                            c.pendentes.push_back(pendente);
                        }
                    }
                }
                self.escrever(par);
            }
            Comando::Voz { par, bytes } => {
                let Some(p) = self.pares.get_mut(&par) else {
                    return;
                };
                p.comp.voz_na_fila.fetch_sub(1, Ordering::Relaxed);
                if !p.pronto {
                    return;
                }
                let voz = p.voz;
                let escrito = p
                    .rtc
                    .channel(voz)
                    .is_some_and(|mut canal| canal.write(true, &bytes).unwrap_or(false));
                if !escrito {
                    p.comp
                        .contadores
                        .voz_nao_saiu
                        .fetch_add(1, Ordering::Relaxed);
                }
                self.drenar(par);
            }
            Comando::Parar { par, canal, codigo } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    let e_ctl = canal == p.ctl;
                    if let Some(c) = p.canais.get_mut(&canal) {
                        c.entrada = None;
                        if !c.fim_local {
                            c.fim_local = true;
                            c.pendentes.clear();
                            c.pendentes.push_back(Pendente {
                                mensagem: crate::envelope::corte(codigo),
                                _credito: None,
                                depois: Depois::Corte,
                            });
                        }
                    }
                    if e_ctl && p.fechando.is_none() {
                        p.fechando = Some(agora + Duration::from_millis(300));
                    }
                    rever_credito(p);
                }
                self.escrever(par);
            }
            Comando::Abrir {
                par,
                bidirecional,
                resposta,
            } => self.abrir_canal(par, bidirecional, resposta),
            Comando::Fechar { par, codigo } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    if p.fechando.is_none() {
                        p.fechando = Some(agora + Duration::from_millis(300));
                        p.codigo_de_fechar = Some(codigo);
                        let ctl = p.ctl;
                        if let Some(c) = p.canais.get_mut(&ctl) {
                            // **Mesmo com o `ctl` já terminado** (`finish`): toda
                            // despedida do servidor termina o `ctl` e depois fecha, e
                            // sem o corte o par via «caiu» em vez de «fechada».
                            // O lado do par continua aberto, e ele aceita o corte
                            // depois do fim. Atrás do que ainda está na fila (a
                            // despedida, um `Disconnecting`); se a fila não andar até
                            // o prazo, `despedir_no_fio` o manda na frente.
                            if !c.cortou {
                                c.cortou = true;
                                c.fim_local = true;
                                c.pendentes.push_back(Pendente {
                                    mensagem: crate::envelope::corte(codigo),
                                    _credito: None,
                                    depois: Depois::Corte,
                                });
                            }
                        }
                    }
                }
                self.escrever(par);
            }
            Comando::ReterCredito { par, reter } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    p.credito_retido = reter;
                }
                self.escrever(par);
            }
            Comando::Consumiu { par } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    rever_credito(p);
                }
                self.escrever(par);
            }
            Comando::Aceito { par, canal } => {
                if let Some(p) = self.pares.get_mut(&par) {
                    if let Some(c) = p.canais.get_mut(&canal) {
                        c.aceito = true;
                    }
                    rever_credito(p);
                }
                self.escrever(par);
            }
            Comando::Discar { discagem, resposta } => self.discar(agora, discagem, resposta),
            Comando::Chamado {
                credencial,
                candidatos,
            } => self.chamado(agora, credencial, candidatos),
            Comando::Anfitriao(config) => {
                // Desarmar não derruba quem já entrou: só para de criar pares novos.
                self.anfitriao = config.map(|c| *c);
            }
            Comando::Desligar => self.desligar(),
        }
    }

    fn discar(
        &mut self,
        agora: Instant,
        discagem: crate::Discagem,
        resposta: oneshot::Sender<Result<Conexao, crate::FalhaAoDiscar>>,
    ) {
        let alvos: Vec<SocketAddr> = discagem
            .alvos
            .iter()
            .map(|a| do_socket(*a))
            .filter(|a| !a.ip().is_unspecified() && a.port() != 0)
            .filter(|a| self.local.is_ipv6() || a.is_ipv4())
            .collect();
        let Some(&primeiro) = alvos.first() else {
            let _ = resposta.send(Err(crate::FalhaAoDiscar::SemAlvos));
            return;
        };
        let u = match discagem.credencial {
            Some(u) if crate::credencial_valida(&u) => u,
            Some(_) => {
                let _ = resposta.send(Err(crate::FalhaAoDiscar::Protocolo(
                    "credencial com forma errada".into(),
                )));
                return;
            }
            None => crate::sortear_credencial(),
        };
        if self.credenciais.contains_key(&u) {
            let _ = resposta.send(Err(crate::FalhaAoDiscar::Protocolo(
                "esta credencial já está numa conexão viva".into(),
            )));
            return;
        }
        let certificado = match discagem
            .certificado
            .map_or_else(CertificadoDtls::efemero, Ok)
        {
            Ok(c) => c,
            Err(erro) => {
                let _ = resposta.send(Err(crate::FalhaAoDiscar::Protocolo(erro.to_string())));
                return;
            }
        };
        let conferir = matches!(discagem.pino, crate::Pino::Impressao(_));
        let mut config = Rtc::builder()
            .set_crypto_provider(crate::cripto::provedor())
            .set_local_ice_credentials(IceCreds {
                ufrag: u.clone(),
                pass: u.clone(),
            })
            .set_dtls_cert(certificado.para_o_str0m())
            .set_fingerprint_verification(conferir)
            .set_sctp_max_buffered_amount(crate::BUFFER_DO_SCTP)
            .set_stats_interval(Some(Duration::from_secs(1)));
        // R2: os checks se repetem pelo prazo inteiro, para cobrir a corrida
        // entre o furo do anfitrião e o primeiro check.
        config.set_initial_stun_rto(Duration::from_millis(250));
        config.set_max_stun_rto(Duration::from_millis(1000));
        let tentativas = usize::try_from(discagem.prazo.as_millis() / 500)
            .unwrap_or(20)
            .max(4);
        config.set_max_stun_retransmits(tentativas);
        let mut rtc = config.build(agora);
        {
            let mut api = rtc.direct_api();
            api.set_ice_controlling(true);
            api.set_remote_ice_credentials(IceCreds {
                ufrag: u.clone(),
                pass: u.clone(),
            });
            let esperada = match discagem.pino {
                crate::Pino::Impressao(fp) => fp,
                crate::Pino::Qualquer => IMPRESSAO_DE_ENCHIMENTO,
            };
            api.set_remote_fingerprint(impressao_str0m(esperada));
            if let Err(erro) = api.start_dtls(true) {
                let _ = resposta.send(Err(crate::FalhaAoDiscar::Protocolo(erro.to_string())));
                return;
            }
            api.start_sctp(true);
        }
        let (ctl, voz) = configurar_canais(&mut rtc);
        for ipv4 in [true, false] {
            if alvos.iter().any(|a| a.is_ipv4() == ipv4) {
                if let Ok(candidato) = Candidate::host(self.virtual_local(ipv4), "udp") {
                    rtc.add_local_candidate(candidato);
                }
            }
        }
        for alvo in &alvos {
            if let Ok(candidato) = Candidate::host(*alvo, "udp") {
                rtc.add_remote_candidate(candidato);
            }
        }
        let id = self.inserir(
            rtc,
            Papel::Cliente,
            primeiro,
            u,
            certificado.impressao(),
            ctl,
            voz,
            agora,
            discagem.prazo,
            Some(resposta),
        );
        let entrada = self.pares.get_mut(&id).map(|par| {
            blindar(&self.contadores.panicos_contidos, || {
                par.rtc.handle_input(Input::Timeout(agora))
            })
        });
        if let Some(Err(erro)) = entrada {
            self.falhou(id, &erro);
            return;
        }
        self.drenar(id);
    }

    fn abrir_canal(
        &mut self,
        id: usize,
        bidirecional: bool,
        resposta: oneshot::Sender<Result<(Envio, Option<Recebe>), Motivo>>,
    ) {
        let Some(par) = self.pares.get_mut(&id) else {
            let _ = resposta.send(Err(Motivo::Caiu("a conexão acabou".into())));
            return;
        };
        if par.fechando.is_some() || !par.pronto {
            let _ = resposta.send(Err(par.comp.motivo_atual()));
            return;
        }
        if par.canais.len() >= crate::MAX_CANAIS {
            let _ = resposta.send(Err(Motivo::Caiu(format!(
                "o teto de {} canais por conexão",
                crate::MAX_CANAIS
            ))));
            return;
        }
        let canal = par.rtc.direct_api().create_data_channel(ChannelConfig {
            label: if bidirecional {
                crate::LABEL_BI
            } else {
                crate::LABEL_UNI
            }
            .into(),
            ordered: true,
            reliability: Reliability::Reliable,
            negotiated: None,
            protocol: crate::PROTOCOLO_DOS_CANAIS.into(),
        });
        let mut novo = Canal::novo(if bidirecional {
            Tipo::Bi
        } else {
            Tipo::UniSaida
        });
        novo.abrindo = Some(resposta);
        par.canais.insert(canal, novo);
        self.drenar(id);
    }

    // ---- saída ------------------------------------------------------------

    /// O escalonador (R4).
    fn escrever(&mut self, id: usize) {
        let mut houve = false;
        {
            let Some(par) = self.pares.get_mut(&id) else {
                return;
            };
            if !par.conectado {
                return;
            }
            // 0. Os créditos (`0x04`), fora da fila dos dados: um crédito atrás de
            // dados que esperam o crédito do par travaria os dois lados.
            let ids: Vec<ChannelId> = par.canais.keys().copied().collect();
            for canal in &ids {
                if par.credito_retido {
                    break;
                }
                let Some(c) = par.canais.get_mut(canal) else {
                    continue;
                };
                let Some(limite) = c.credito_a_mandar else {
                    continue;
                };
                if !c.aberto {
                    continue;
                }
                let mensagem = crate::envelope::credito(limite);
                let escrito = par
                    .rtc
                    .channel(*canal)
                    .is_some_and(|mut h| h.write(true, &mensagem).unwrap_or(false));
                if escrito {
                    c.credito_a_mandar = None;
                    c.anunciou = true;
                    houve = true;
                }
            }
            // 1. O que tem prioridade: o `ctl` inteiro e todo corte.
            for canal in &ids {
                let e_ctl = *canal == par.ctl;
                while let Some(c) = par.canais.get_mut(canal) {
                    if !c.aberto {
                        break;
                    }
                    let Some(frente) = c.pendentes.front() else {
                        break;
                    };
                    if !(e_ctl || frente.depois == Depois::Corte) {
                        break;
                    }
                    if !escrever_um(&mut par.rtc, *canal, c) {
                        break;
                    }
                    houve = true;
                }
            }
            // 2. Volume, revezando, com a associação abaixo do limiar.
            let volume: Vec<ChannelId> = ids
                .iter()
                .copied()
                .filter(|c| {
                    par.canais
                        .get(c)
                        .is_some_and(|canal| canal.tipo.de_volume() && canal.aberto)
                })
                .collect();
            if !volume.is_empty() {
                let mut esperando: usize = volume
                    .iter()
                    .filter_map(|c| par.rtc.channel(*c).map(|mut h| h.buffered_amount()))
                    .sum();
                let mut parados = 0;
                while esperando < LIMIAR_DE_VOLUME && parados < volume.len() {
                    par.revezamento = (par.revezamento + 1) % volume.len();
                    let Some(&canal) = volume.get(par.revezamento) else {
                        break;
                    };
                    let Some(c) = par.canais.get_mut(&canal) else {
                        parados += 1;
                        continue;
                    };
                    let tamanho = c.pendentes.front().map_or(0, |p| p.mensagem.len());
                    if tamanho == 0 || !escrever_um(&mut par.rtc, canal, c) {
                        parados += 1;
                        continue;
                    }
                    parados = 0;
                    esperando += tamanho;
                    houve = true;
                }
                // O maior volume que a associação já teve na frente da voz: é a
                // medida do R4 que não depende da CPU nem da carga da máquina.
                par.comp
                    .contadores
                    .maior_volume_no_sctp
                    .fetch_max(esperando as u64, Ordering::Relaxed);
            }
            // 3. Fechar o que terminou e já saiu do SCTP.
            let mut fechar = Vec::new();
            for (canal, c) in &par.canais {
                if c.fechar && c.pendentes.is_empty() && c.tipo.de_volume() {
                    let vazio = par
                        .rtc
                        .channel(*canal)
                        .is_none_or(|mut h| h.buffered_amount() == 0);
                    if vazio {
                        fechar.push(*canal);
                    }
                }
            }
            for canal in fechar {
                if let Some(c) = par.canais.get_mut(&canal) {
                    c.fechar = false;
                }
                par.rtc.direct_api().close_data_channel(canal);
                houve = true;
            }
        }
        if houve {
            self.drenar(id);
        }
    }

    // ---- o laço do str0m -------------------------------------------------

    fn drenar(&mut self, id: usize) {
        for _ in 0..100_000 {
            let saida = {
                let Some(par) = self.pares.get_mut(&id) else {
                    return;
                };
                blindar(&self.contadores.panicos_contidos, || par.rtc.poll_output())
            };
            match saida {
                Ok(Output::Timeout(quando)) => {
                    if let Some(par) = self.pares.get_mut(&id) {
                        par.proximo = quando;
                        if !par.rtc.is_alive() {
                            let motivo = par
                                .motivo_remoto
                                .clone()
                                .unwrap_or_else(|| Motivo::Caiu("o par fechou".into()));
                            self.encerrar(id, motivo, None);
                        }
                    }
                    return;
                }
                Ok(Output::Transmit(t)) => {
                    let destino = para_o_socket(t.destination, self.local);
                    if self.saida.send_to(&t.contents, destino).is_err() {
                        self.contadores
                            .datagramas_perdidos_na_saida
                            .fetch_add(1, Ordering::Relaxed);
                        if let Some(par) = self.pares.get_mut(&id) {
                            par.falhas_de_envio = par.falhas_de_envio.saturating_add(1);
                        }
                    }
                    if let Some(par) = self.pares.get_mut(&id) {
                        par.ultimo_tx = Instant::now();
                        par.comp
                            .bytes_enviados
                            .fetch_add(t.contents.len() as u64, Ordering::Relaxed);
                        par.comp.pacotes_enviados.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(Output::Event(evento)) => self.evento(id, evento),
                Err(erro) => {
                    self.falhou(id, &erro);
                    return;
                }
            }
        }
    }

    fn falhou(&mut self, id: usize, erro: &str0m::RtcError) {
        let texto = erro.to_string();
        let falha = if texto.contains("fingerprint") {
            let vista = self.pares.get_mut(&id).and_then(|par| {
                par.rtc
                    .direct_api()
                    .remote_dtls_fingerprint()
                    .and_then(|f| f.bytes.as_slice().try_into().ok())
            });
            crate::FalhaAoDiscar::ImpressaoDiferente { vista }
        } else {
            crate::FalhaAoDiscar::Protocolo(texto.clone())
        };
        self.encerrar(id, Motivo::Caiu(texto), Some(falha));
    }

    fn evento(&mut self, id: usize, evento: Event) {
        match evento {
            Event::IceConnectionStateChange(estado) if estado.is_connected() => {
                if let Some(par) = self.pares.get_mut(&id) {
                    // A nomeação vem de uma entrada já validada pelo ICE. Não
                    // iniciar no primeiro Binding nem renovar com repetições:
                    // o prazo geral e os portões de admissão continuam valendo.
                    par.espera_do_dtls_desde.get_or_insert(par.ultimo_rx);
                }
            }
            Event::Connected => {
                if let Some(par) = self.pares.get_mut(&id) {
                    par.conectado = true;
                    let vista = par
                        .rtc
                        .direct_api()
                        .remote_dtls_fingerprint()
                        .and_then(|f| <[u8; 32]>::try_from(f.bytes.as_slice()).ok());
                    if let Some(vista) = vista {
                        let _ = par.comp.impressao_par.set(vista);
                    }
                }
                self.talvez_pronto(id);
            }
            Event::ChannelOpen(canal, label) => self.canal_abriu(id, canal, &label),
            Event::ChannelData(dados) => self.dados(id, dados.id, dados.data),
            Event::ChannelClose(canal) => self.canal_fechou(id, canal),
            Event::ChannelBufferedAmountLow(_) => self.escrever(id),
            Event::PeerStats(stats) => {
                if let Some(par) = self.pares.get(&id) {
                    let rtt = stats.rtt.or_else(|| {
                        stats
                            .selected_candidate_pair
                            .as_ref()
                            .and_then(|p| p.current_round_trip_time)
                    });
                    if let Some(rtt) = rtt {
                        par.comp.rtt_us.store(
                            u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX).max(1),
                            Ordering::Relaxed,
                        );
                    }
                    if let Ok(mut perda) = par.comp.perda_na_saida.lock() {
                        *perda = stats.egress_loss_fraction;
                    }
                    if let Some(caminho) = &stats.selected_candidate_pair {
                        if let Ok(mut remoto) = par.comp.remoto.lock() {
                            *remoto = caminho.remote.addr;
                        }
                    }
                }
            }
            Event::Closed => {
                let motivo = self
                    .pares
                    .get(&id)
                    .and_then(|p| p.motivo_remoto.clone())
                    .unwrap_or_else(|| Motivo::Caiu("o par fechou o DTLS".into()));
                self.encerrar(id, motivo, None);
            }
            _ => {}
        }
    }

    fn canal_abriu(&mut self, id: usize, canal: ChannelId, label: &str) {
        let Some(cmd) = self.cmd.upgrade() else {
            return;
        };
        let Some(par) = self.pares.get_mut(&id) else {
            return;
        };
        if let Some(c) = par.canais.get_mut(&canal) {
            c.aberto = true;
            // Um canal aberto por este lado já é da aplicação: um "b" também é
            // lido por ela, e anuncia a janela (dentro do agregado).
            c.aceito = true;
            if let Some(resposta) = c.abrindo.take() {
                let envio = Envio::novo(
                    cmd.clone(),
                    Arc::clone(&par.comp),
                    canal,
                    Arc::clone(&c.credito),
                    Arc::clone(&c.estado),
                );
                let recebe = (c.tipo == Tipo::Bi).then(|| {
                    let (tx, rx) = mpsc::unbounded_channel();
                    c.entrada = Some(tx);
                    Recebe::novo(
                        rx,
                        Arc::clone(&c.na_fila),
                        cmd.clone(),
                        Arc::clone(&par.comp),
                        canal,
                    )
                });
                if resposta.send(Ok((envio, recebe))).is_err() {
                    c.fim_local = true;
                    c.fechar = true;
                }
            }
            if let Some(par) = self.pares.get_mut(&id) {
                rever_credito(par);
            }
            self.talvez_pronto(id);
            self.escrever(id);
            return;
        }
        // Um canal que o par abriu.
        let protocolo = par
            .rtc
            .channel(canal)
            .and_then(|h| h.config().map(|c| c.protocol.clone()))
            .unwrap_or_default();
        let tipo = match label {
            crate::LABEL_UNI => Some(Tipo::UniEntrada),
            crate::LABEL_BI => Some(Tipo::Bi),
            _ => None,
        };
        let aceito = par.pronto
            && par.fechando.is_none()
            && protocolo == crate::PROTOCOLO_DOS_CANAIS
            && par.canais.len() < crate::MAX_CANAIS
            // As filas de canais que o par já terminou e ninguém leu também
            // contam: sem isso, abrir, encher 1 MiB e terminar, sem parar,
            // guardava filas sem teto.
            && par.comp.filas_vivas.load(Ordering::Relaxed) < crate::MAX_CANAIS;
        let (Some(tipo), true) = (tipo, aceito) else {
            par.comp
                .contadores
                .canais_recusados
                .fetch_add(1, Ordering::Relaxed);
            tracing::debug!(conexao = id, label, protocolo, "canal recusado ao chegar");
            par.rtc.direct_api().close_data_channel(canal);
            self.drenar(id);
            return;
        };
        let mut c = Canal::novo(tipo);
        c.aberto = true;
        // Sem crédito até a aplicação aceitar (`Comando::Aceito`).
        let (tx, rx) = mpsc::unbounded_channel();
        c.entrada = Some(tx);
        let recebe = Recebe::novo(
            rx,
            Arc::clone(&c.na_fila),
            cmd.clone(),
            Arc::clone(&par.comp),
            canal,
        );
        let entregue = match (tipo, par.lado.as_ref()) {
            (Tipo::UniEntrada, Some(lado)) => lado.uni.try_send(recebe).is_ok(),
            (Tipo::Bi, Some(lado)) => {
                let envio = Envio::novo(
                    cmd,
                    Arc::clone(&par.comp),
                    canal,
                    Arc::clone(&c.credito),
                    Arc::clone(&c.estado),
                );
                lado.bi.try_send((envio, recebe)).is_ok()
            }
            _ => false,
        };
        if entregue {
            par.canais.insert(canal, c);
        } else {
            par.comp
                .contadores
                .canais_recusados
                .fetch_add(1, Ordering::Relaxed);
            par.rtc.direct_api().close_data_channel(canal);
            self.drenar(id);
        }
    }

    fn violacao(&mut self, id: usize, canal: ChannelId, porque: &str) {
        let Some(par) = self.pares.get_mut(&id) else {
            return;
        };
        par.comp
            .contadores
            .violacoes
            .fetch_add(1, Ordering::Relaxed);
        tracing::debug!(conexao = id, porque, "violação do enlace");
        if canal == par.ctl || !par.pronto {
            let falha = crate::FalhaAoDiscar::Protocolo(porque.to_owned());
            self.encerrar(
                id,
                Motivo::Caiu(format!("violação do enlace: {porque}")),
                Some(falha),
            );
            return;
        }
        if let Some(c) = par.canais.get_mut(&canal) {
            if let Some(entrada) = c.entrada.take() {
                let _ = entrada.send(Entrada::Corte(crate::CORTE_VIOLACAO));
            }
            c.estado
                .parar(crate::FalhaDoCanal::Cortado(crate::CORTE_VIOLACAO));
            if !c.fim_local {
                c.fim_local = true;
                c.pendentes.clear();
                c.pendentes.push_back(Pendente {
                    mensagem: crate::envelope::corte(crate::CORTE_VIOLACAO),
                    _credito: None,
                    depois: Depois::Corte,
                });
            }
        }
        self.escrever(id);
    }

    fn dados(&mut self, id: usize, canal: ChannelId, bytes: Vec<u8>) {
        let Some(par) = self.pares.get_mut(&id) else {
            return;
        };
        if canal == par.voz {
            if bytes == crate::KEEPALIVE_DA_VOZ {
                return;
            }
            let entregue = bytes.len() <= crate::MAX_VOZ
                && par
                    .lado
                    .as_ref()
                    .is_some_and(|l| l.voz.try_send(bytes).is_ok());
            if !entregue {
                par.comp
                    .contadores
                    .voz_descartada
                    .fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
        if bytes.len() > crate::MAX_MENSAGEM {
            self.violacao(id, canal, "mensagem acima do teto");
            return;
        }
        let mensagem = match crate::envelope::ler(&bytes) {
            Ok(m) => m,
            Err(erro) => {
                self.violacao(id, canal, &erro.to_string());
                return;
            }
        };
        let e_ctl = canal == par.ctl;
        let papel = par.papel;
        let esperando_apresentacao =
            e_ctl && papel == Papel::Cliente && par.comp.apresentacao.get().is_none();
        match mensagem {
            crate::envelope::Mensagem::Apresentacao(corpo) => {
                if esperando_apresentacao {
                    let _ = par.comp.apresentacao.set(corpo.to_vec());
                    self.talvez_pronto(id);
                } else {
                    self.violacao(id, canal, "apresentação fora do lugar");
                }
                return;
            }
            _ if esperando_apresentacao => {
                self.violacao(id, canal, "o anfitrião não se apresentou primeiro");
                return;
            }
            _ => {}
        }
        if let crate::envelope::Mensagem::Credito(limite) = mensagem {
            // O par fala contrapressão. Na primeira vez, o anfitrião passa a
            // anunciar crédito também, em todo canal que ele lê.
            let primeira = !par.fala_credito;
            par.fala_credito = true;
            if primeira {
                rever_credito(par);
            }
            if let Some(c) = par.canais.get_mut(&canal) {
                // Só em quem escreve neste canal; num "u" que o par abriu, quem
                // escreve é ele, e um crédito de volta não tem sentido.
                if matches!(c.tipo, Tipo::Ctl | Tipo::Bi | Tipo::UniSaida) {
                    c.limite_do_par = c.limite_do_par.max(limite);
                }
            }
            self.escrever(id);
            return;
        }
        let Some(c) = par.canais.get_mut(&canal) else {
            return;
        };
        if c.tipo == Tipo::UniSaida {
            match mensagem {
                crate::envelope::Mensagem::Fim if c.fim_local => {
                    // A confirmação de entrega: quem lê recebeu o fim.
                    c.estado.entregue();
                    c.fechar = true;
                    self.escrever(id);
                }
                crate::envelope::Mensagem::Corte(codigo) => {
                    // Quem lê parou: quem escreve fica sabendo no próximo write.
                    c.estado.parar(crate::FalhaDoCanal::Cortado(codigo));
                    c.credito.close();
                    c.pendentes.clear();
                    c.fim_local = true;
                    c.fechar = true;
                    self.escrever(id);
                }
                _ => self.violacao(id, canal, "dados num canal só de saída"),
            }
            return;
        }
        match mensagem {
            crate::envelope::Mensagem::Dados(corpo) => {
                let alem_do_credito = c.recebidos + corpo.len() as u64 > c.limite_para_o_par;
                c.recebidos += corpo.len() as u64;
                let Some(entrada) = &c.entrada else {
                    return;
                };
                let na_fila = c.na_fila.load(Ordering::Relaxed);
                // Passar do crédito anunciado é violar o contrato (Δ8): o mesmo
                // corte da fila, contado. O teto de 1 MiB da fila fica como guarda.
                if alem_do_credito || na_fila + corpo.len() > crate::MAX_FILA_DE_ENTRADA {
                    par.comp
                        .contadores
                        .cortes_por_fila
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = entrada.send(Entrada::Corte(crate::CORTE_FILA));
                    c.entrada = None;
                    c.estado
                        .parar(crate::FalhaDoCanal::Cortado(crate::CORTE_FILA));
                    if !c.fim_local {
                        c.fim_local = true;
                        c.pendentes.clear();
                        c.pendentes.push_back(Pendente {
                            mensagem: crate::envelope::corte(crate::CORTE_FILA),
                            _credito: None,
                            depois: Depois::Corte,
                        });
                    }
                    if e_ctl {
                        let motivo = Motivo::Caiu("o controle estourou a fila de entrada".into());
                        self.encerrar(id, motivo, None);
                        return;
                    }
                    self.rever(id);
                    self.escrever(id);
                    return;
                }
                c.na_fila.fetch_add(corpo.len(), Ordering::Relaxed);
                par.comp.pausa_de_teste.talvez_parar();
                if entrada.send(Entrada::Dados(corpo.to_vec())).is_err() {
                    // Quem lê já largou o `Recebe`: nada fica na conta.
                    c.na_fila.fetch_sub(corpo.len(), Ordering::Relaxed);
                    c.entrada = None;
                    self.rever(id);
                }
            }
            crate::envelope::Mensagem::Fim => {
                if c.fim_remoto {
                    self.violacao(id, canal, "dois fins no mesmo sentido");
                    return;
                }
                c.fim_remoto = true;
                if let Some(entrada) = c.entrada.take() {
                    let _ = entrada.send(Entrada::Fim);
                }
                match c.tipo {
                    // Num "u", quem lê devolve o fim: é a confirmação de
                    // entrega que o `Envio::entregue` de quem escreve espera.
                    // Quem escreve fecha o canal ao recebê-la.
                    Tipo::UniEntrada if !c.fim_local => {
                        c.fim_local = true;
                        c.pendentes.push_back(Pendente {
                            mensagem: crate::envelope::fim(),
                            _credito: None,
                            depois: Depois::Nada,
                        });
                    }
                    Tipo::Bi if c.fim_local => {
                        c.estado.entregue();
                        c.fechar = true;
                    }
                    _ => {}
                }
                // O crédito concedido e não usado deste canal voltou ao agregado.
                self.rever(id);
                self.escrever(id);
            }
            crate::envelope::Mensagem::Corte(codigo) => {
                c.fim_remoto = true;
                if let Some(entrada) = c.entrada.take() {
                    let _ = entrada.send(Entrada::Corte(codigo));
                }
                c.estado.parar(crate::FalhaDoCanal::Cortado(codigo));
                c.credito.close();
                c.pendentes.clear();
                c.fim_local = true;
                c.fechar = true;
                if e_ctl {
                    // O par fechou (ou largou a leitura do `ctl`, que no enlace é
                    // fechar). A despedida volta no fio antes do DTLS cair: sem
                    // ela, quem largou o `ctl` via «caiu» em vez de «fechada»
                    // (`quem_hospeda_fecha::a_despedida_avisa…`). O código é o do
                    // `close` deste lado, se houve, ou o que o par mandou.
                    if par.codigo_de_fechar.is_none() {
                        par.codigo_de_fechar = Some(codigo);
                    }
                    let recebido = Motivo::Fechada { codigo };
                    par.motivo_remoto = Some(recebido.clone());
                    self.despedir_no_fio(id);
                    self.encerrar(id, recebido, None);
                    return;
                }
                self.rever(id);
                self.escrever(id);
            }
            crate::envelope::Mensagem::Apresentacao(_) | crate::envelope::Mensagem::Credito(_) => {}
        }
    }

    /// Revê o crédito agregado de um par: toda transição que solta uma reserva
    /// (fim, corte, fechamento, fila largada) passa por aqui, ou o canal que
    /// esperava vaga no agregado espera para sempre.
    fn rever(&mut self, id: usize) {
        if let Some(par) = self.pares.get_mut(&id) {
            rever_credito(par);
        }
    }

    fn canal_fechou(&mut self, id: usize, canal: ChannelId) {
        let Some(par) = self.pares.get_mut(&id) else {
            return;
        };
        let e_ctl = canal == par.ctl;
        if let Some(mut c) = par.canais.remove(&canal) {
            if c.tipo != Tipo::Ctl && c.na_fila.load(Ordering::Relaxed) > 0 {
                par.filas_orfas.push(Arc::clone(&c.na_fila));
            }
            let falha = if c.fim_remoto && c.fim_local {
                crate::FalhaDoCanal::Fim
            } else {
                crate::FalhaDoCanal::Caiu(Motivo::Caiu("o canal fechou".into()))
            };
            if let Some(resposta) = c.abrindo.take() {
                let _ = resposta.send(Err(Motivo::Caiu("o par recusou o canal".into())));
            }
            c.morrer(falha);
        }
        if !e_ctl {
            rever_credito(par);
            self.escrever(id);
            return;
        }
        {
            let motivo = par
                .motivo_remoto
                .clone()
                .unwrap_or_else(|| Motivo::Caiu("o controle fechou".into()));
            self.encerrar(id, motivo, None);
        }
    }

    fn talvez_pronto(&mut self, id: usize) {
        let Some(cmd) = self.cmd.upgrade() else {
            return;
        };
        let Some(par) = self.pares.get_mut(&id) else {
            return;
        };
        if par.pronto || !par.conectado {
            return;
        }
        let aberto = |canal: ChannelId| par.canais.get(&canal).is_some_and(|c| c.aberto);
        if !aberto(par.ctl) || !aberto(par.voz) {
            return;
        }
        if par.papel == Papel::Cliente && par.comp.apresentacao.get().is_none() {
            return;
        }
        let ctl = par.ctl;
        let comp = Arc::clone(&par.comp);
        let Some(c) = par.canais.get_mut(&ctl) else {
            return;
        };
        let (tx, rx) = mpsc::unbounded_channel();
        c.entrada = Some(tx);
        c.aceito = true;
        let envio = Envio::novo(
            cmd.clone(),
            Arc::clone(&comp),
            ctl,
            Arc::clone(&c.credito),
            Arc::clone(&c.estado),
        );
        let recebe = Recebe::novo(
            rx,
            Arc::clone(&c.na_fila),
            cmd.clone(),
            Arc::clone(&comp),
            ctl,
        );
        if par.papel == Papel::Anfitriao {
            c.pendentes.push_front(Pendente {
                mensagem: crate::envelope::apresentacao(&par.apresentacao),
                _credito: None,
                depois: Depois::Nada,
            });
        }
        // Quem liga anuncia a janela do `ctl` na hora. O anfitrião só anuncia
        // depois de ver um crédito do par (`fala_credito`), e por isso a
        // apresentação continua sendo a primeira mensagem do `ctl` dele.
        rever_credito(par);
        let (conexao, lado) = Conexao::nova(comp, cmd, (envio, recebe));
        par.lado = Some(lado);
        par.pronto = true;
        par.ultimo_rx = Instant::now();
        par.prazo = Instant::now() + Duration::from_secs(365 * 24 * 60 * 60);
        let papel = par.papel;
        let ip = par.ip;
        let criado = par.criado;
        let espera = par.espera.take();
        match papel {
            Papel::Anfitriao => {
                self.falhas_por_ip.remove(&ip);
                if self.chegadas.try_send(conexao).is_err() {
                    // A fila cheia ou fechada: a vaga do balde fica retida.
                    // Devolvê-la aqui deixaria o mesmo IP girar apertos
                    // completos que ninguém consome (nem a portaria de quem
                    // hospeda) sem taxa nenhuma.
                    tracing::info!(%ip, "aperto completo encerrado: a fila de chegadas está cheia ou fechada");
                    self.encerrar(id, Motivo::Caiu("ninguém aceitou a conexão".into()), None);
                    return;
                }
                // Entregue: não é o laço que o balde de criações para.
                self.devolver_a_criacao(criado, ip);
            }
            Papel::Cliente => {
                if let Some(espera) = espera {
                    if espera.send(Ok(conexao)).is_err() {
                        return;
                    }
                }
            }
        }
        self.escrever(id);
    }

    // ---- o tempo ----------------------------------------------------------

    fn tique(&mut self, agora: Instant) {
        let ids: Vec<usize> = self.pares.keys().copied().collect();
        for id in ids {
            let Some(par) = self.pares.get_mut(&id) else {
                continue;
            };
            if !par.pronto && agora >= par.prazo {
                let falha = if par.algum_pacote {
                    crate::FalhaAoDiscar::Prazo
                } else {
                    crate::FalhaAoDiscar::SemResposta
                };
                self.encerrar(
                    id,
                    Motivo::Caiu("o aperto não terminou no prazo".into()),
                    Some(falha),
                );
                continue;
            }
            if par.papel == Papel::Anfitriao
                && !par.dtls_visto
                && par
                    .espera_do_dtls_desde
                    .is_some_and(|desde| agora >= desde + ESPERA_PELO_DTLS)
            {
                self.contadores
                    .pendentes_sem_dtls
                    .fetch_add(1, Ordering::Relaxed);
                self.encerrar(id, Motivo::Caiu("o par não começou o DTLS".into()), None);
                continue;
            }
            if par.pronto && agora >= par.ultimo_rx + crate::OCIOSO {
                self.encerrar(
                    id,
                    Motivo::Caiu("ocioso: o par parou de responder".into()),
                    None,
                );
                continue;
            }
            if par.fechando.is_some_and(|f| agora >= f) {
                self.despedir_no_fio(id);
                self.encerrar(id, Motivo::Local, None);
                continue;
            }
            if par.pronto && agora >= par.ultimo_tx + crate::KEEPALIVE {
                let voz = par.voz;
                if let Some(mut canal) = par.rtc.channel(voz) {
                    let _ = canal.write(true, &crate::KEEPALIVE_DA_VOZ);
                }
                par.ultimo_tx = agora;
                self.drenar(id);
            }
            let Some(par) = self.pares.get_mut(&id) else {
                continue;
            };
            if agora >= par.proximo {
                if let Err(erro) = blindar(&self.contadores.panicos_contidos, || {
                    par.rtc.handle_input(Input::Timeout(agora))
                }) {
                    self.falhou(id, &erro);
                    continue;
                }
                self.drenar(id);
                self.escrever(id);
            }
        }
    }

    fn encerrar(&mut self, id: usize, motivo: Motivo, falha: Option<crate::FalhaAoDiscar>) {
        let Some(mut par) = self.pares.remove(&id) else {
            return;
        };
        self.vivas.store(self.pares.len(), Ordering::Relaxed);
        let agora = Instant::now();
        if !par.pronto && par.papel == Papel::Anfitriao && !par.superado {
            // Uma linha por aperto encerrado, sem endereços, credenciais ou
            // conteúdo. "Gerados" inclui falhas locais de send_to: não prova
            // que um datagrama chegou à rede ou ao outro aparelho.
            tracing::info!(
                conexao = id,
                idade_ms = agora.saturating_duration_since(par.criado).as_millis() as u64,
                ice_nomeado = par.espera_do_dtls_desde.is_some(),
                dtls_recebido = par.dtls_visto,
                datagramas_gerados = par.comp.pacotes_enviados.load(Ordering::Relaxed),
                falhas_de_envio = par.falhas_de_envio,
                bytes_recebidos = par.comp.bytes_recebidos.load(Ordering::Relaxed),
                candidatos_ipv4 = par
                    .remotos_do_chamado
                    .iter()
                    .filter(|c| c.is_ipv4())
                    .count(),
                candidatos_ipv6 = par
                    .remotos_do_chamado
                    .iter()
                    .filter(|c| c.is_ipv6())
                    .count(),
                "aperto do anfitrião encerrado antes de abrir os canais"
            );
        }
        self.credenciais.remove(&par.comp.credencial);
        self.donos.retain(|_, dono| *dono != id);
        if par.papel == Papel::Anfitriao {
            // Todo U que terminou, por falha ou fechado, vira lápide: um Binding
            // de consentimento atrasado nunca recria o `Rtc`.
            self.enterrar(agora, par.comp.credencial.clone());
            if !par.pronto && !par.superado {
                self.falhou_o_ip(agora, par.ip);
            }
        }
        if par.rtc.is_alive() {
            let _ = blindar(&self.contadores.panicos_contidos, || par.rtc.close());
            for _ in 0..256 {
                match blindar(&self.contadores.panicos_contidos, || par.rtc.poll_output()) {
                    Ok(Output::Transmit(t)) => {
                        let _ = self
                            .saida
                            .send_to(&t.contents, para_o_socket(t.destination, self.local));
                    }
                    Ok(Output::Event(_)) => {}
                    Ok(Output::Timeout(_)) | Err(_) => break,
                }
            }
        }
        let motivo = par.motivo_remoto.clone().unwrap_or(motivo);
        for canal in par.canais.values_mut() {
            if let Some(resposta) = canal.abrindo.take() {
                let _ = resposta.send(Err(motivo.clone()));
            }
            canal.morrer(crate::FalhaDoCanal::Caiu(motivo.clone()));
        }
        par.lado = None;
        par.comp.motivo.send_replace(Some(motivo.clone()));
        if let Some(espera) = par.espera.take() {
            let falha = falha.unwrap_or(crate::FalhaAoDiscar::Protocolo(motivo.to_string()));
            let _ = espera.send(Err(falha));
        }
        tracing::debug!(conexao = id, %motivo, "conexão encerrada");
    }
}

/// O que um canal que este lado lê ainda segura de memória: o que está na fila
/// e não foi lido, mais o crédito concedido que o par ainda pode usar.
fn em_aberto(c: &Canal) -> u64 {
    let fila = c.na_fila.load(Ordering::Relaxed) as u64;
    let livre = if c.fim_remoto || c.entrada.is_none() {
        0
    } else {
        c.limite_para_o_par.saturating_sub(c.recebidos)
    };
    fila.saturating_add(livre)
}

/// Revê o crédito (`0x04`) de todos os canais que este lado lê, como o
/// `criarCreditoDaConexao` do navegador: cada um quer `consumidos + janela`,
/// mas a soma do que os canais de volume seguram ([`em_aberto`], e as filas
/// órfãs) não passa de [`crate::MAX_CREDITO_DA_CONEXAO`]. O `ctl` fica fora da
/// soma. Os famintos (menos de um pedaço de folga) são vistos primeiro, para um
/// leitor rápido não tomar sempre a vaga que outro espera.
fn rever_credito(par: &mut Par) {
    if !(par.papel == Papel::Cliente || par.fala_credito) {
        return;
    }
    let janela = crate::MAX_FILA_DE_ENTRADA as u64;
    let teto = crate::MAX_CREDITO_DA_CONEXAO as u64;
    let maximo_de_dados = crate::MAX_DADOS as u64;
    par.filas_orfas
        .retain(|fila| Arc::strong_count(fila) > 1 && fila.load(Ordering::Relaxed) > 0);
    // Somas saturadas: o contador não dá a volta (ver `Recebe::descartar_fila`),
    // e, se um dia der, o agregado fica cheio em vez de derrubar a tarefa.
    let mut total: u64 = par
        .filas_orfas
        .iter()
        .map(|fila| fila.load(Ordering::Relaxed) as u64)
        .chain(
            par.canais
                .values()
                .filter(|c| c.le() && c.tipo != Tipo::Ctl)
                .map(em_aberto),
        )
        .fold(0, u64::saturating_add);
    let faminto = |c: &Canal| c.limite_para_o_par.saturating_sub(c.recebidos) < maximo_de_dados;
    let mut ids: Vec<(bool, ChannelId)> = par
        .canais
        .iter()
        .filter(|(_, c)| c.le() && c.aceito && c.aberto && !c.fim_remoto && c.entrada.is_some())
        .map(|(id, c)| (!faminto(c), *id))
        .collect();
    ids.sort_unstable_by_key(|(nao_faminto, _)| *nao_faminto);
    for (_, canal) in ids {
        let Some(c) = par.canais.get_mut(&canal) else {
            continue;
        };
        let fixo = c.tipo == Tipo::Ctl;
        let consumidos = c
            .recebidos
            .saturating_sub(c.na_fila.load(Ordering::Relaxed) as u64);
        let mut novo = consumidos + janela;
        if !fixo {
            novo = novo.min(c.limite_para_o_par + teto.saturating_sub(total));
        }
        if novo <= c.limite_para_o_par {
            continue;
        }
        let com_fome = faminto(c) && novo - c.recebidos >= maximo_de_dados;
        if c.anunciou && novo - c.limite_para_o_par < janela / 4 && !com_fome {
            continue;
        }
        let antes = em_aberto(c);
        c.limite_para_o_par = novo;
        c.credito_a_mandar = Some(novo);
        if !fixo {
            total = total.saturating_sub(antes).saturating_add(em_aberto(c));
        }
    }
    par.comp.credito_em_aberto.store(total, Ordering::Relaxed);
}

/// Escreve a mensagem da frente de um canal no SCTP. `false` se não coube.
fn escrever_um(rtc: &mut Rtc, canal: ChannelId, c: &mut Canal) -> bool {
    let Some(frente) = c.pendentes.front() else {
        return false;
    };
    // O crédito do par (`0x04`): só os corpos de dados contam. Sem espaço (e
    // antes do primeiro anúncio, o limite é 0), o canal espera, e quem escreve
    // espera junto (o crédito de saída do `Envio` não volta).
    let corpo = match frente.mensagem.first() {
        Some(&crate::envelope::TAG_DADOS) => frente.mensagem.len().saturating_sub(1) as u64,
        _ => 0,
    };
    if corpo > 0 && c.enviados + corpo > c.limite_do_par {
        return false;
    }
    let Some(mut handle) = rtc.channel(canal) else {
        return false;
    };
    match handle.write(true, &frente.mensagem) {
        Ok(true) => {}
        Ok(false) => return false,
        Err(_) => {
            c.morrer(crate::FalhaDoCanal::Caiu(Motivo::Caiu(
                "o SCTP recusou a escrita".into(),
            )));
            return false;
        }
    }
    let Some(pendente) = c.pendentes.pop_front() else {
        return false;
    };
    c.enviados += corpo;
    match pendente.depois {
        Depois::Nada => {}
        Depois::Fim => {
            // Um "u" espera quem lê devolver o fim (a confirmação de entrega)
            // para fechar; um "b" fecha quando os dois fins passaram.
            if c.tipo == Tipo::Bi && c.fim_remoto {
                c.estado.entregue();
                c.fechar = true;
            }
        }
        Depois::Corte => {
            c.fechar = true;
            c.corte_saiu = true;
            c.estado.parar(crate::FalhaDoCanal::Fim);
        }
    }
    true
}

#[cfg(test)]
mod chamado_testes;

#[cfg(test)]
mod tests {
    use super::*;

    /// R4 (b): com um buffer pequeno de propósito, o valor concedido é o que o
    /// sistema relata, e o enlace continua funcionando (só sem a promessa de voz
    /// sem perda numa rajada). Nada de sysctl: o pedido pequeno é deste socket.
    #[tokio::test]
    async fn com_um_buffer_pequeno_o_concedido_e_relatado_e_o_enlace_anda() {
        let pedido = 16 * 1024;
        let cru = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let concedido = ajustar_buffers(&cru, pedido).unwrap();
        assert_eq!(
            Some(concedido),
            socket2::SockRef::from(&cru).recv_buffer_size().ok(),
            "o concedido é o que o sistema relata de volta"
        );
        assert!(concedido < BUFFER_DE_RECEPCAO);

        let certificado = crate::CertificadoDtls::efemero().unwrap();
        let host = Ponta::de_socket_pedindo(
            std::net::UdpSocket::bind("127.0.0.1:0").unwrap(),
            Some(ConfigDoAnfitriao::novo(certificado.clone())),
            pedido,
        )
        .unwrap();
        assert!(host.buffer_de_recepcao().unwrap() < BUFFER_DE_RECEPCAO);
        let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
            .await
            .unwrap();
        let discagem = Discagem::nova(
            vec![host.local_addr()],
            Pino::Impressao(certificado.impressao()),
        );
        let (a, b) = tokio::join!(cliente.discar(discagem), host.aceitar());
        let (a, b) = (a.unwrap(), b.unwrap());
        let (mut envio, _ar) = a.controle().unwrap();
        let (_be, mut recebe) = b.controle().unwrap();
        envio.write_all(&[5; 40_000]).await.unwrap();
        let mut buf = vec![0_u8; 40_000];
        recebe.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, vec![5; 40_000]);
    }

    /// R2: um furo manda no máximo [`INDICACOES_POR_CANDIDATO`] Indications a cada um
    /// de no máximo [`CANDIDATOS_POR_FURO`] candidatos, e a ponta faz no máximo
    /// [`FUROS_POR_MINUTO`] furos por minuto. O filtro de produção recusa loopback,
    /// então o teste passa um que o aceita; o resto é o caminho de produção.
    #[tokio::test]
    async fn o_furo_tem_teto_por_candidato_por_chamada_e_por_minuto() {
        let ponta = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
            .await
            .unwrap();
        let mut alvos = Vec::new();
        let mut sockets = Vec::new();
        for _ in 0..5 {
            let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            alvos.push(s.local_addr().unwrap());
            sockets.push(s);
        }
        let aceita: fn(SocketAddr) -> bool = |_| true;
        assert_eq!(ponta.furar_com(&alvos, aceita), CANDIDATOS_POR_FURO);
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let mut buf = [0_u8; 64];
        for (i, s) in sockets.iter().enumerate() {
            let mut vistas = 0;
            while let Ok(Ok((n, _))) =
                tokio::time::timeout(Duration::from_millis(200), s.recv_from(&mut buf)).await
            {
                assert_eq!(n, 20);
                assert_eq!(buf[..2], [0x00, 0x11], "Binding Indication");
                vistas += 1;
            }
            let esperado = if i < CANDIDATOS_POR_FURO {
                INDICACOES_POR_CANDIDATO
            } else {
                0
            };
            assert_eq!(vistas, esperado, "candidato {i}");
        }
        // O teto por minuto: o primeiro furo já gastou uma vaga.
        for _ in 1..FUROS_POR_MINUTO {
            assert!(ponta.furar_com(&alvos[..1], aceita) > 0);
        }
        assert_eq!(ponta.furar_com(&alvos[..1], aceita), 0);
        assert_eq!(ponta.contadores().furos_recusados, 1);
        // E o filtro de produção recusa loopback.
        assert_eq!(ponta.furar(&alvos[..1]), 0);
    }
}

#[cfg(test)]
mod dtls_prazo_testes;
