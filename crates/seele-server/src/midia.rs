//! O vídeo por RTP nas duas bordas da sala (ADR 0067).
//!
//! A sala de voz continua repassando o fluxo de tela **como ele sempre foi**:
//! a abertura e os quadros enquadrados (`[tipo | tamanho]` + corpo), byte por
//! byte, com a fila por espectador, a entrada no quadro-chave e o corte. Nada
//! disso mudou, e é de propósito: é a parte medida e provada.
//!
//! O que este módulo faz é **traduzir nas bordas**, para as sessões cujo par
//! disse que ligou a mídia (`ClientMessage::MidiaLigada`):
//!
//! - **na entrada** ([`EntradaPorMidia`]): os quadros que chegam pela mídia RTP
//!   de quem transmite viram o fluxo enquadrado que a sala espera;
//! - **na saída** ([`SaidaPorMidia`]): o fluxo enquadrado que a sala entrega a
//!   um espectador vira quadros na mídia RTP dele.
//!
//! Assim uma transmissão que sobe por RTP desce por canal para quem fala um
//! protocolo antigo (e para a página do navegador), e o contrário: nenhuma
//! combinação fica sem imagem. E a perda no caminho de cada um é recuperada
//! no RTP daquele caminho, sem custar aos outros.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_proto::screen::{
    e_quadro_chave_h264, LeitorDeTela, LidoDaTela, TipoDeQuadro, MAX_QUADRO_LEN, SCREEN_HEADER_LEN,
};
use seele_proto::{PersonId, ScreenId, ServerMessage};
use tokio::sync::mpsc;

use crate::server::{Event, Server};
use crate::tela::{AberturaDeTela, Pedaco};
use crate::transporte::{EventoDeMidia, Midia, Recusa, RemetenteDeMidia, Trilha};

/// A faixa por onde a transmissão de uma pessoa sobe, e por onde a que uma
/// sessão assiste desce. Uma pessoa transmite uma coisa por vez, e a tarefa
/// que escreve para um espectador leva uma transmissão por vez.
pub const FAIXA: u8 = 0;

/// Quantos quadros recebidos esperam a tarefa que os leva à sala. Cheia, o
/// quadro é largado e o vídeo recomeça num quadro-chave.
pub const FILA_DE_ENTRADA: usize = 64;

/// De quanto em quanto a entrada volta a olhar se a transmissão ainda vale,
/// quando nenhum quadro chega. O RTP não tem «o fluxo fechou»: quem diz que a
/// transmissão acabou é o controle.
pub const ESPERA_DA_ENTRADA: Duration = Duration::from_millis(250);

/// O menor intervalo entre dois pedidos de quadro-chave a quem transmite.
pub const ENTRE_PEDIDOS_DE_CHAVE: Duration = Duration::from_millis(300);

/// Um quadro que subiu pela mídia de quem transmite.
#[derive(Debug)]
pub struct QuadroQueSobe {
    trilha: Trilha,
    contiguo: bool,
    bytes: Arc<[u8]>,
}

/// O que a sessão guarda da mídia RTP da conexão dela.
#[derive(Clone)]
pub struct MidiaDaSessao {
    remetente: RemetenteDeMidia,
    /// Para onde vão os quadros da transmissão desta pessoa, enquanto ela
    /// transmite por RTP.
    entrada: Arc<Mutex<Option<mpsc::Sender<QuadroQueSobe>>>>,
    /// A transmissão que desce pela [`FAIXA`] agora.
    assistindo: Arc<Mutex<Option<ScreenId>>>,
}

impl MidiaDaSessao {
    /// Liga a mídia da conexão e devolve o que a sessão guarda e o lado de
    /// leitura, que vai para [`ler`].
    #[must_use]
    pub fn ligar(conexao: &crate::transporte::Conexao) -> (Self, Midia) {
        let midia = conexao.ligar_midia();
        let sessao = Self {
            remetente: midia.remetente(),
            entrada: Arc::default(),
            assistindo: Arc::default(),
        };
        (sessao, midia)
    }

    /// Uma transmissão desta pessoa começa (ou recomeça, numa geração nova) a
    /// subir por RTP: os quadros passam a ir para a entrada devolvida. A
    /// entrada anterior, se havia, vê o fim.
    #[must_use]
    pub fn abrir_entrada(&self) -> EntradaPorMidia {
        let (tx, rx) = mpsc::channel(FILA_DE_ENTRADA);
        if let Ok(mut entrada) = self.entrada.lock() {
            *entrada = Some(tx);
        }
        EntradaPorMidia {
            quadros: rx,
            remetente: self.remetente.clone(),
            espera_chave: true,
            ultimo_pedido: None,
        }
    }

    /// O lado de saída, para a tarefa que escreve as transmissões a que esta
    /// sessão assiste.
    #[must_use]
    pub fn saida(
        &self,
        server: Arc<Server>,
        person: PersonId,
        avisos: mpsc::Sender<ServerMessage>,
    ) -> SaidaPorMidia {
        SaidaPorMidia {
            remetente: self.remetente.clone(),
            assistindo: Arc::clone(&self.assistindo),
            server,
            person,
            avisos,
        }
    }
}

/// Lê a mídia de uma sessão até a conexão acabar: os quadros de quem
/// transmite vão para a entrada aberta, e o pedido de quadro-chave de quem
/// assiste (PLI) vira o mesmo pedido que o `RequestKeyFrame` faz.
pub async fn ler(mut midia: Midia, sessao: MidiaDaSessao, server: Arc<Server>, person: PersonId) {
    // Um quadro largado por fila cheia: o próximo de vídeo não é contíguo.
    let mut furo = false;
    while let Some(evento) = midia.receber().await {
        match evento {
            EventoDeMidia::Quadro {
                faixa: FAIXA,
                trilha,
                contiguo,
                bytes,
                ..
            } => {
                let destino = sessao.entrada.lock().ok().and_then(|e| e.clone());
                let Some(destino) = destino else {
                    // Mídia de quem não abriu transmissão: nada a fazer com
                    // ela. Quem abre sem pedir já é recusado no controle.
                    continue;
                };
                let video = trilha == Trilha::Video;
                let quadro = QuadroQueSobe {
                    trilha,
                    contiguo: contiguo && !(furo && video),
                    bytes,
                };
                let entrou = destino.try_send(quadro).is_ok();
                if video {
                    furo = !entrou;
                }
            }
            EventoDeMidia::PediramChave { faixa: FAIXA } => {
                let screen = sessao.assistindo.lock().ok().and_then(|a| *a);
                if let Some(screen) = screen {
                    pedir_a_chave(&server, person, screen).await;
                }
            }
            // As outras faixas não levam nada neste servidor.
            EventoDeMidia::Quadro { .. } | EventoDeMidia::PediramChave { .. } => {}
        }
    }
}

/// Pede um quadro-chave a quem transmite `screen`, em nome de `person`.
///
/// Só é chamada para uma transmissão que **este servidor** pôs na faixa de
/// `person` (a sala o convidou a assistir): por construção não atravessa
/// salas, que é o que o `RequestKeyFrame` confere à mão.
async fn pedir_a_chave(server: &Server, person: PersonId, screen: ScreenId) {
    let dono = server.telas.lock().await.dono(screen);
    if let Some(sharer) = dono {
        let _ = server.events.send(Event::KeyFrameRequested {
            screen,
            person,
            sharer,
        });
    }
}

/// O que a entrada leu.
#[derive(Debug)]
pub enum Lido {
    /// Um pedaço do fluxo enquadrado, pronto para a sala.
    Bytes(Vec<u8>),
    /// Nada desta vez: é hora de conferir se a transmissão ainda vale.
    Nada,
    /// A entrada acabou.
    Fim,
}

/// Os quadros que sobem por RTP, lidos como o fluxo enquadrado da sala.
#[derive(Debug)]
pub struct EntradaPorMidia {
    quadros: mpsc::Receiver<QuadroQueSobe>,
    remetente: RemetenteDeMidia,
    /// Falta um quadro-chave: os quadros comuns são largados até ele. Começa
    /// assim, porque o fluxo da sala tem de começar numa chave.
    espera_chave: bool,
    ultimo_pedido: Option<Instant>,
}

impl EntradaPorMidia {
    /// O próximo pedaço.
    pub async fn ler(&mut self) -> Lido {
        let quadro = match tokio::time::timeout(ESPERA_DA_ENTRADA, self.quadros.recv()).await {
            Err(_) => return Lido::Nada,
            Ok(None) => return Lido::Fim,
            Ok(Some(quadro)) => quadro,
        };
        // O enquadramento da sala recusa um quadro vazio ou grande demais, e
        // a recusa encerra a transmissão. Aqui o quadro só é largado: um
        // pacote estranho na mídia não é motivo para apagar a tela de todos.
        if quadro.bytes.is_empty() || quadro.bytes.len() > MAX_QUADRO_LEN {
            return Lido::Nada;
        }
        let tipo = match quadro.trilha {
            Trilha::Som => TipoDeQuadro::Som,
            Trilha::Video => {
                let chave = e_quadro_chave_h264(&quadro.bytes);
                if !quadro.contiguo && !chave {
                    self.espera_chave = true;
                }
                if self.espera_chave && !chave {
                    // **Um quadro comum sem a cadeia inteira não vai à sala.**
                    // Repassá-lo seria mandar a N pessoas uma imagem que
                    // nenhuma delas decodifica.
                    self.pedir_chave();
                    return Lido::Nada;
                }
                self.espera_chave = false;
                if chave {
                    TipoDeQuadro::Chave
                } else {
                    TipoDeQuadro::Comum
                }
            }
        };
        let mut pedaco = Vec::with_capacity(5 + quadro.bytes.len());
        pedaco.push(tipo as u8);
        // Cabe: o tamanho foi conferido contra `MAX_QUADRO_LEN` acima.
        let tamanho = u32::try_from(quadro.bytes.len()).unwrap_or(u32::MAX);
        pedaco.extend_from_slice(&tamanho.to_be_bytes());
        pedaco.extend_from_slice(&quadro.bytes);
        Lido::Bytes(pedaco)
    }

    fn pedir_chave(&mut self) {
        let agora = Instant::now();
        if self
            .ultimo_pedido
            .is_some_and(|antes| agora.saturating_duration_since(antes) < ENTRE_PEDIDOS_DE_CHAVE)
        {
            return;
        }
        self.ultimo_pedido = Some(agora);
        self.remetente.pedir_chave(FAIXA);
    }
}

/// O fluxo que a sala entrega a um espectador, escrito na mídia RTP dele.
pub struct SaidaPorMidia {
    remetente: RemetenteDeMidia,
    assistindo: Arc<Mutex<Option<ScreenId>>>,
    server: Arc<Server>,
    person: PersonId,
    avisos: mpsc::Sender<ServerMessage>,
}

impl SaidaPorMidia {
    /// Escreve uma transmissão inteira, do convite ao fim.
    pub async fn escrever(&self, mut convite: AberturaDeTela) {
        let screen = convite.screen;
        let Ok(abertura) = <[u8; SCREEN_HEADER_LEN]>::try_from(convite.abertura.as_slice()) else {
            tracing::warn!(%screen, "a abertura de uma transmissão não tem onze bytes; o espectador ficou sem ela");
            return;
        };
        let mut leitor = LeitorDeTela::novo();
        if leitor.empurrar(&abertura).is_err() {
            return;
        }
        if let Ok(mut assistindo) = self.assistindo.lock() {
            *assistindo = Some(screen);
        }
        // A abertura vai pelo controle, antes do primeiro quadro: é ela que
        // diz a quem assiste de que transmissão é a faixa.
        if self
            .avisos
            .send(ServerMessage::TelaPorMidia {
                faixa: FAIXA,
                abertura,
            })
            .await
            .is_err()
        {
            return;
        }
        let comeco = Instant::now();
        let mut espera_chave = false;
        let mut malformado_dito = false;
        let mut limpo = false;
        'fluxo: while let Some(pedaco) = convite.pedacos.recv().await {
            let bytes = match pedaco {
                Pedaco::Bytes(bytes) => bytes,
                Pedaco::Fim => {
                    limpo = true;
                    break;
                }
            };
            if leitor.empurrar(&bytes).is_err() {
                break;
            }
            while let Some(lido) = leitor.proximo() {
                let LidoDaTela::Quadro { tipo, bytes } = lido else {
                    continue;
                };
                let micros = u64::try_from(comeco.elapsed().as_micros()).unwrap_or(u64::MAX);
                let (trilha, tempo) = match tipo {
                    TipoDeQuadro::Som => (Trilha::Som, micros.saturating_mul(48) / 1000),
                    TipoDeQuadro::Chave | TipoDeQuadro::Comum => {
                        if !crate::transporte::annex_b_valido(&bytes) {
                            // **O que quem transmite mandou não é H.264 em
                            // Annex B.** No canal isso passava como bytes e
                            // morria no decodificador de quem assiste; na
                            // mídia o empacotador o recusa. O quadro é largado
                            // aqui, e os comuns param até uma chave que sirva.
                            if !malformado_dito {
                                malformado_dito = true;
                                tracing::warn!(
                                    person = %self.person,
                                    %screen,
                                    "um quadro de tela que não é Annex B não foi à mídia do espectador"
                                );
                            }
                            espera_chave = true;
                            continue;
                        }
                        if espera_chave && tipo != TipoDeQuadro::Chave {
                            continue;
                        }
                        (Trilha::Video, micros.saturating_mul(90) / 1000)
                    }
                };
                match self.remetente.mandar(FAIXA, trilha, tempo, bytes.into()) {
                    Ok(()) => {
                        if trilha == Trilha::Video {
                            espera_chave = false;
                        }
                    }
                    Err(Recusa::Fechada) => break 'fluxo,
                    Err(Recusa::Cheia | Recusa::Grande) => {
                        // **Um quadro que não saiu quebra a cadeia de quem
                        // assiste, e ele não tem como saber** (a sequência do
                        // RTP segue sem buraco). Os comuns param até a próxima
                        // chave, e a chave é pedida em nome dele.
                        if trilha == Trilha::Video && !espera_chave {
                            espera_chave = true;
                            tracing::warn!(
                                person = %self.person,
                                %screen,
                                "um quadro de tela não coube na mídia do espectador; o vídeo dele recomeça num quadro-chave"
                            );
                            pedir_a_chave(&self.server, self.person, screen).await;
                        }
                    }
                }
            }
        }
        // No canal, o fim é do fluxo: `finish` para a transmissão que acabou,
        // `reset` para a cópia que se perdeu. Na mídia não há fluxo para
        // fechar, e o fim vai dito pelo controle, com a mesma diferença.
        let _ = self
            .avisos
            .send(ServerMessage::FimDaTelaPorMidia {
                faixa: FAIXA,
                limpo,
            })
            .await;
        if let Ok(mut assistindo) = self.assistindo.lock() {
            if *assistindo == Some(screen) {
                *assistindo = None;
            }
        }
    }
}
