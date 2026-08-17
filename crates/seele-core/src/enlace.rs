//! O enlace com um Dogma, incluindo o que fazer quando ele cai.
//!
//! [`Client`] é uma conexão: enquanto ela existe, funciona; quando cai, acaba.
//! Isto é a **sessão**, que é outra coisa — ela atravessa quedas. É aqui que
//! mora a bateria interna de `specs/07-tema-evangelion.md`:
//!
//! > Quando a conexão cai, o cliente não fecha nem mostra um spinner. Ele entra
//! > em bateria interna: contagem regressiva de 5 minutos, tentativas de
//! > reconexão listadas, interface esmaecida mas ainda legível.
//!
//! # Por que isto existia pela metade
//!
//! [`crate::Battery`] estava escrita e testada. A TUI sabia desenhar a tela.
//! Nada chamava uma coisa da outra: `Battery::new` não aparecia fora do próprio
//! módulo, e ao cair o cliente ia direto para "ENLACE PERDIDO". Cada peça
//! correta, a junção ausente — que é o tipo de falha que teste de unidade não
//! pega, porque cada unidade passa.
//!
//! # Por que uma tarefa, e não um objeto que a casca conduz
//!
//! As duas cascas chamam o cliente de dentro de um `tokio::select!`, e o
//! `select!` cancela quem perde a corrida. Ler já foi resolvido assim — uma
//! tarefa dona do fluxo entregando por canal. **Escrever tem o mesmo problema**:
//! `frame::write` faz dois `write_all`, e cancelado entre eles deixa meio
//! quadro no fio. Um `Enlace` que a casca conduzisse teria que escrever de
//! dentro do `select!`, e reintroduziria o defeito pelo outro lado.
//!
//! Então a conexão inteira mora numa tarefa. A casca fala por comandos e ouve
//! por avisos, e as duas pontas são canais — seguros de cancelar por contrato.
//!
//! # O que a reconexão restaura, e o que não
//!
//! Restaura o Cage, a Linha, o A.T. Field e o isolamento: é o que a pessoa
//! escolheu, e voltar sem isso seria voltar para outro lugar. **Não** restaura
//! a voz sozinha — a conexão é nova, e com ela o canal de mídia. A casca recebe
//! [`Aviso::Reconectado`] com o canal novo e reabre o áudio. É honesto: o
//! caminho de áudio realmente recomeça.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::control::ServerMessage;
use seele_proto::ids::{CageId, ClientMessageId, LineId, MessageId, PilotId};
use tokio::sync::mpsc;

use crate::battery::{Action, Battery, Link};
use crate::client::{Client, ConnectError, MediaChannel, SessionInfo};
use crate::tofu::PinDecision;
use crate::tofu::PinStore;
use crate::tofu::{verdict, Verdict};

/// Onde ficar batendo, e com que credencial.
#[derive(Debug, Clone)]
pub struct Destino {
    /// Endereço do Dogma.
    pub servidor: SocketAddr,
    /// O nome que o TLS recebe. Ver [`Client::connect`].
    pub nome_tls: String,
    /// Sob que chave o pin é arquivado. Ver [`Client::connect`].
    pub chave_do_pin: String,
    /// Como aparecer no roster.
    pub apelido: String,
    /// Convite de uso único ou senha do Dogma.
    pub segredo: Option<String>,
    /// A impressão digital que o convite prometeu, quando veio de um link.
    ///
    /// `None` para quem digitou o endereço à mão — aí não há o que conferir, e
    /// o primeiro contato segue sendo cego, como sempre foi.
    pub impressao_esperada: Option<String>,
}

/// O que a casca precisa saber.
pub enum Aviso {
    /// O Dogma disse algo.
    Mensagem(Box<ServerMessage>),
    /// Onde o enlace está, e quanto resta da bateria.
    ///
    /// Repetido a cada tica enquanto a bateria corre, porque a contagem
    /// regressiva **é** a informação: `specs/07-tema-evangelion.md` pede 04:59
    /// descendo na tela, e um número que só muda quando o estado muda ficaria
    /// parado exatamente durante os cinco minutos em que ele importa.
    Estado {
        /// Online, na bateria, ou descarregado.
        estado: Link,
        /// Quanto falta dos cinco minutos. `None` quando online.
        restante: Option<Duration>,
    },
    /// A conexão voltou, e com ela um canal de mídia novo.
    Reconectado {
        /// O canal de voz da conexão nova.
        media: Box<MediaChannel>,
        /// A sessão nova. O `ssrc` muda a cada conexão (falha G1).
        sessao: Box<SessionInfo>,
    },
    /// Acabou. Ou os cinco minutos passaram, ou não vale a pena tentar.
    Encerrado(Motivo),
}

impl std::fmt::Debug for Aviso {
    /// À mão porque [`MediaChannel`] embrulha uma conexão do quinn, que não
    /// tem `Debug`. O que interessa num log é qual aviso é, não o socket.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mensagem(mensagem) => f.debug_tuple("Mensagem").field(mensagem).finish(),
            Self::Estado { estado, restante } => f
                .debug_struct("Estado")
                .field("estado", estado)
                .field("restante", restante)
                .finish(),
            Self::Reconectado { sessao, .. } => f
                .debug_struct("Reconectado")
                .field("sessao", sessao)
                .finish(),
            Self::Encerrado(motivo) => f.debug_tuple("Encerrado").field(motivo).finish(),
        }
    }
}

/// Por que a sessão acabou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// A bateria interna descarregou: cinco minutos sem reconectar.
    Descarregou,
    /// O Dogma recusou, e insistir não muda a resposta.
    Recusado(String),
    /// Alguém pediu para sair.
    Pedido,
}

/// O que a casca manda fazer.
#[derive(Debug)]
enum Comando {
    InserirPlug(CageId),
    EjetarPlug,
    AbrirLinha(LineId),
    Dizer {
        linha: LineId,
        corpo: String,
        id: ClientMessageId,
    },
    Historico {
        linha: LineId,
        limite: u16,
    },
    AtField(bool),
    Isolamento(bool),
    CriarCage {
        nome: String,
        limite: u16,
        linha: Option<LineId>,
    },
    CriarLinha {
        nome: String,
    },
    RenomearCage {
        cage: CageId,
        nome: String,
    },
    RenomearLinha {
        linha: LineId,
        nome: String,
    },
    Expulsar {
        piloto: PilotId,
    },
    Banir {
        piloto: PilotId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    },
    RemoverMensagem {
        mensagem: MessageId,
    },
    MoverPiloto {
        piloto: PilotId,
        cage: CageId,
    },
    ApagarCage {
        cage: CageId,
    },
    ApagarLinha {
        linha: LineId,
    },
    PesarLinha {
        linha: LineId,
    },
    Sair,
}

/// A sessão com um Dogma, viva através de quedas.
pub struct Enlace {
    comandos: mpsc::Sender<Comando>,
    avisos: mpsc::UnboundedReceiver<Aviso>,
    /// O que se sabia na última conexão.
    sessao: SessionInfo,
    media: MediaChannel,
    estado: Link,
    /// Quanto resta dos cinco minutos, atualizado a cada aviso.
    restante: Option<Duration>,
    /// O que o TOFU decidiu no primeiro contato. ADR 0003.
    pin: PinDecision,
    /// O que a conferência com o convite concluiu. ADR 0006.
    veredito: Verdict,
    /// O último tempo de ida e volta, em microssegundos. Zero é desconhecido.
    ///
    /// Um átomo e não um aviso: a barra de telemetria lê isto quatro vezes por
    /// segundo, e transformar cada medição num aviso encheria a fila de coisas
    /// que ninguém precisa ver acontecer — só ver o valor atual.
    rtt: Arc<std::sync::atomic::AtomicU64>,
    tarefa: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for Enlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enlace")
            .field("estado", &self.estado)
            .field("dogma", &self.sessao.dogma)
            .finish()
    }
}

/// Fila de comandos. Controle é raro; isto é folga, não capacidade.
const COMANDOS: usize = 32;

/// Quanto tempo cada endereço do convite ganha antes de o próximo ser tentado.
///
/// Quatro segundos. O número sai de duas contas em sentidos opostos. Para
/// baixo: um aperto de mão QUIC numa rede doméstica com perda cabe folgado em
/// quatro segundos, e cortar antes disso descartaria um endereço que ia dar
/// certo. Para cima: um endereço que não volta — o público visto de dentro da
/// própria casa, o de uma VPN que não aceita entrada — gasta o prazo inteiro,
/// e ele é multiplicado pelo número de candidatos antes de a sala abrir.
///
/// É por isso que a ordem do convite importa mais que este número: com a rede
/// de casa em primeiro, o caso comum nunca chega a esperar nada disto.
const PRAZO_POR_CANDIDATO: Duration = Duration::from_secs(4);

impl Enlace {
    /// Conecta no primeiro endereço que atender, tentando um de cada vez.
    ///
    /// Um convite pode trazer vários endereços do mesmo Dogma — ADR 0006 — e
    /// eles não são intercambiáveis: o da rede de casa não é alcançável de
    /// fora, e o público que o roteador abriu costuma não voltar para dentro,
    /// porque a maioria dos roteadores domésticos não faz *hairpin*.
    ///
    /// # Em série, e não em corrida
    ///
    /// Uma corrida abriria vários apertos de mão contra o mesmo Dogma para
    /// descartar todos menos um — e cada aperto de mão fixa chave, gasta o
    /// convite de uso único do ADR 0021 e aparece no log de quem hospeda como
    /// uma tentativa. Em série nada disso acontece: no caso comum, o primeiro
    /// endereço é o da rede local e responde antes de o segundo ser cogitado.
    ///
    /// # O prazo, e por que a lista de um não tem nenhum
    ///
    /// Com mais de um candidato, cada um vale [`PRAZO_POR_CANDIDATO`]: um
    /// endereço que não volta não pode segurar a fila. Com um só, não há fila —
    /// e aí o caminho é exatamente o de antes, sem prazo novo e sem mudança de
    /// comportamento para um convite antigo.
    ///
    /// # Que erro sai quando nenhum entra
    ///
    /// O de quem **respondeu**, se algum respondeu: "a chave deste Dogma
    /// mudou" diz o que aconteceu, e "não alcancei" de um endereço que nunca ia
    /// voltar não diz nada. Sem nenhuma resposta, sai o erro do primeiro
    /// candidato, que é o endereço que a pessoa mais provavelmente esperava
    /// usar.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar`], escolhido como acima.
    pub async fn conectar_entre(
        destinos: Vec<Destino>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        let mut candidatos = destinos.into_iter().peekable();
        let Some(primeiro) = candidatos.next() else {
            // Ninguém chama assim, e devolver um erro é melhor que entrar num
            // laço que termina sem resposta nenhuma.
            return Err(ConnectError::Unreachable);
        };
        if candidatos.peek().is_none() {
            return Self::conectar(primeiro, chave, pins).await;
        }

        let mut primeira_falha: Option<ConnectError> = None;
        let mut respondeu: Option<ConnectError> = None;
        for destino in std::iter::once(primeiro).chain(candidatos) {
            let onde = destino.servidor;
            let chave_do_pin = destino.chave_do_pin.clone();
            let fixado_antes = pins.pinned(&chave_do_pin);
            let tentativa = Self::conectar(destino, chave.clone(), Arc::clone(&pins));

            let falha = match tokio::time::timeout(PRAZO_POR_CANDIDATO, tentativa).await {
                Ok(Ok(enlace)) => return Ok(enlace),
                Ok(Err(erro)) => erro,
                Err(_) => {
                    // O aperto de mão foi cancelado no meio, e o `conectar` não
                    // chegou à limpeza dele. O pin que o TLS possa ter escrito
                    // some aqui, pelo motivo escrito em `desfazer_pin_orfao`.
                    desfazer_pin_orfao(pins.as_ref(), &chave_do_pin, fixado_antes.as_deref());
                    ConnectError::HandshakeTimeout
                }
            };
            tracing::info!(%onde, erro = %falha, "este endereço do convite não deu; indo ao próximo");
            if respondeu.is_none() && alguem_respondeu(&falha) {
                respondeu = Some(falha.clone());
            }
            if primeira_falha.is_none() {
                primeira_falha = Some(falha);
            }
        }

        Err(respondeu
            .or(primeira_falha)
            .unwrap_or(ConnectError::Unreachable))
    }

    /// Conecta pela primeira vez.
    ///
    /// A primeira conexão falha para fora: quem não conseguiu entrar não tem
    /// sessão para segurar, e uma bateria interna antes de haver sessão seria
    /// uma contagem regressiva para reconectar a lugar nenhum.
    ///
    /// # Errors
    ///
    /// Devolve o motivo de não ter conseguido conectar, incluindo
    /// [`ConnectError::InviteMismatch`] quando o link prometia outra
    /// identidade.
    pub async fn conectar(
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        // Antes de o TLS ter chance de escrever qualquer coisa. Ver
        // [`desfazer_pin_orfao`].
        let fixado_antes = pins.pinned(&destino.chave_do_pin);

        let resultado = Client::connect(
            destino.servidor,
            &destino.nome_tls,
            &destino.chave_do_pin,
            &destino.apelido,
            &chave,
            Arc::clone(&pins),
            destino.segredo.as_deref(),
        )
        .await;

        let mut cliente = match resultado {
            Ok(cliente) => cliente,
            Err(erro) => {
                desfazer_pin_orfao(
                    pins.as_ref(),
                    &destino.chave_do_pin,
                    fixado_antes.as_deref(),
                );
                return Err(erro);
            }
        };

        let pin = cliente.pin_decision().clone();
        let veredito = match conferir(&destino, &pin, pins.as_ref()) {
            Ok(veredito) => veredito,
            Err(erro) => {
                // Derrubar, não só relatar. E explicitamente, não por `Drop`.
                //
                // Soltar o `Client` **acaba** fechando a conexão — medido em
                // ~85 ms contra um Dogma de verdade, com e sem esta linha —, mas
                // pelo caminho longo: `Client::connect` deixa uma tarefa de
                // leitura dona do `RecvStream`, e ela só descobre que ninguém
                // escuta quando o servidor manda o quadro seguinte. Contra um
                // Dogma que fala (telemetria a cada segundo) isso é rápido;
                // contra um que emudeceu, é o tempo ocioso do QUIC inteiro,
                // com uma sessão de pé do lado de quem acabou de ser recusado.
                //
                // Ou seja: a conclusão não mudou, o motivo sim. Fechar aqui não
                // depende de o servidor dizer nada. A medição, e o que ela
                // implica para quem tenta testar esta linha, está em
                // `crates/seele-conformance/tests/convite.rs` — apagá-la não
                // deixa nenhum teste vermelho, e isso está dito lá por escrito.
                //
                // E fecha **dizendo o que foi**: o motivo viaja no
                // `CONNECTION_CLOSE` e é o que fica no log do Dogma. Fechar
                // como `ejected` faria uma recusa de convite parecer um piloto
                // saindo, que é o único jeito de esconder a recusa de quem tem
                // o log na mão.
                cliente.close(crate::client::INVITE_REFUSED);
                return Err(erro);
            }
        };

        let sessao = cliente.session().clone();
        let media = cliente.media();
        let rtt = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let (comandos_tx, comandos_rx) = mpsc::channel(COMANDOS);
        let (avisos_tx, avisos_rx) = mpsc::unbounded_channel();

        let motor = Motor {
            destino,
            chave,
            pins,
            cliente: Some(cliente),
            bateria: Battery::new(),
            inicio: Instant::now(),
            cage: None,
            linha: None,
            at_field: false,
            isolamento: false,
            avisos: avisos_tx,
            rtt: Arc::clone(&rtt),
        };
        let tarefa = tokio::spawn(motor.rodar(comandos_rx));

        Ok(Self {
            comandos: comandos_tx,
            avisos: avisos_rx,
            sessao,
            media,
            estado: Link::Online,
            restante: None,
            pin,
            veredito,
            rtt,
            tarefa,
        })
    }

    /// O próximo aviso.
    ///
    /// Seguro de cancelar: é um `recv` de canal. As cascas chamam isto dentro
    /// de um `select!`, e é essa propriedade que faz o resto do desenho ser o
    /// que é.
    pub async fn proximo(&mut self) -> Aviso {
        let aviso = self
            .avisos
            .recv()
            .await
            .unwrap_or(Aviso::Encerrado(Motivo::Pedido));

        match &aviso {
            Aviso::Estado { estado, restante } => {
                self.estado = *estado;
                self.restante = *restante;
            }
            Aviso::Reconectado { media, sessao } => {
                self.estado = Link::Online;
                self.restante = None;
                self.media = (**media).clone();
                self.sessao = (**sessao).clone();
            }
            _ => {}
        }
        aviso
    }

    /// Onde o enlace está.
    #[must_use]
    pub fn estado(&self) -> Link {
        self.estado
    }

    /// Quanto resta dos cinco minutos, enquanto a bateria corre.
    #[must_use]
    pub fn restante(&self) -> Option<Duration> {
        self.restante
    }

    /// O que o TOFU decidiu ao conectar. ADR 0003.
    #[must_use]
    pub fn pin_decision(&self) -> &PinDecision {
        &self.pin
    }

    /// O que a conferência de identidade concluiu nesta conexão.
    #[must_use]
    pub fn veredito(&self) -> &Verdict {
        &self.veredito
    }

    /// O último tempo de ida e volta medido.
    #[must_use]
    pub fn rtt(&self) -> Option<Duration> {
        match self.rtt.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            micros => Some(Duration::from_micros(micros)),
        }
    }

    /// O que se sabe da sessão. Muda a cada reconexão.
    #[must_use]
    pub fn sessao(&self) -> &SessionInfo {
        &self.sessao
    }

    /// O canal de voz da conexão atual.
    #[must_use]
    pub fn media(&self) -> MediaChannel {
        self.media.clone()
    }

    /// Entra num Cage. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn inserir_plug(&self, cage: CageId) -> Result<(), Fechado> {
        self.mandar(Comando::InserirPlug(cage)).await
    }

    /// Sai do Cage.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn ejetar_plug(&self) -> Result<(), Fechado> {
        self.mandar(Comando::EjetarPlug).await
    }

    /// Abre uma Linha. Restaurada depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn abrir_linha(&self, linha: LineId) -> Result<(), Fechado> {
        self.mandar(Comando::AbrirLinha(linha)).await
    }

    /// Diz alguma coisa.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn dizer(
        &self,
        linha: LineId,
        corpo: String,
        id: ClientMessageId,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Dizer { linha, corpo, id }).await
    }

    /// Pede histórico.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn historico(&self, linha: LineId, limite: u16) -> Result<(), Fechado> {
        self.mandar(Comando::Historico { linha, limite }).await
    }

    /// Liga ou desliga o A.T. Field. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn at_field(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::AtField(ligado)).await
    }

    /// Liga ou desliga o isolamento total. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn isolamento(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Isolamento(ligado)).await
    }

    /// Pede ao Dogma que faça um Cage.
    ///
    /// Pede, e só. Nada aqui confere se este piloto pode: a `specs/08-seguranca.md`
    /// põe a decisão no servidor, e um core que recusasse por conta própria
    /// seria uma segunda autoridade para manter de acordo com a primeira. A
    /// resposta chega como aviso — `CageCreated` se aconteceu, `Alert` com
    /// `PermissionDenied` se não.
    ///
    /// **Não** é refeito ao reconectar, ao contrário do Cage e da Linha
    /// abertos. Aqueles são onde a pessoa estava, e voltar sem eles é voltar
    /// para outro lugar; este é uma coisa que se faz uma vez. Repetido depois de
    /// uma queda, ele criaria uma sala minutos mais tarde, do nada, e mais uma
    /// se a pessoa já tivesse pedido de novo à mão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_cage(
        &self,
        nome: String,
        limite: u16,
        linha: Option<LineId>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::CriarCage {
            nome,
            limite,
            linha,
        })
        .await
    }

    /// Pede ao Dogma que faça uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_linha(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::CriarLinha { nome }).await
    }

    /// Pede ao Dogma que renomeie um Cage.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_cage(&self, cage: CageId, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearCage { cage, nome }).await
    }

    /// Pede ao Dogma que renomeie uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_linha(&self, linha: LineId, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearLinha { linha, nome }).await
    }

    /// Pede ao Dogma que acabe com a sessão de alguém.
    ///
    /// Pede, e só — como os verbos de sala, e pela mesma razão: a
    /// `specs/08-seguranca.md` põe a decisão no servidor, e um core que
    /// recusasse por conta própria seria uma segunda autoridade para manter de
    /// acordo com a primeira. Esconder o botão é conveniência; quem nega é o
    /// Dogma, e ele responde com `Alert` de `PermissionDenied` quando nega.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e pelo mesmo
    /// motivo: expulsar é coisa que se faz uma vez, e repetida minutos depois
    /// derrubaria alguém que já tinha voltado.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn expulsar(&self, piloto: PilotId) -> Result<(), Fechado> {
        self.mandar(Comando::Expulsar { piloto }).await
    }

    /// Pede ao Dogma que impeça alguém de voltar.
    ///
    /// `expira_em` em segundos desde a época; `None` é para sempre. O `motivo`
    /// é para o registro de quem hospeda e nunca chega a quem foi banido — a
    /// `specs/08-seguranca.md` quer falha uniforme, e a recusa que essa pessoa
    /// encontra na volta é a mesma qualquer que seja o texto.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn banir(
        &self,
        piloto: PilotId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Banir {
            piloto,
            motivo,
            expira_em,
        })
        .await
    }

    /// Pede ao Dogma que tire uma mensagem da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn remover_mensagem(&self, mensagem: MessageId) -> Result<(), Fechado> {
        self.mandar(Comando::RemoverMensagem { mensagem }).await
    }

    /// Pede ao Dogma que mova alguém para um Cage.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn mover_piloto(&self, piloto: PilotId, cage: CageId) -> Result<(), Fechado> {
        self.mandar(Comando::MoverPiloto { piloto, cage }).await
    }

    /// Pede ao Dogma que destrua um Cage.
    ///
    /// Pede, e só, como todo verbo daqui. Quem recusa é o Dogma: sem
    /// `administrar_dogma` volta `Alert` com `PermissionDenied`, e no único
    /// Cage que resta volta `Alert` com `LastCage`, que é frase diferente.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e de moderação e
    /// pelo mesmo motivo, com uma ponta a mais: repetido minutos depois, este
    /// destruiria a sala que alguém fez no lugar da que sumiu.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_cage(&self, cage: CageId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarCage { cage }).await
    }

    /// Pede ao Dogma que destrua uma Linha, e tudo que foi escrito nela.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_linha(&self, linha: LineId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarLinha { linha }).await
    }

    /// Pergunta quanto custaria destruir uma Linha. Não destrói nada.
    ///
    /// A resposta chega como `LineWeighed` no fluxo de avisos, como toda
    /// resposta deste enlace. É o que enche a caixa de confirmação com número
    /// contado no banco — uma casca segura uma página de histórico e chutaria
    /// para baixo por todo o passado da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn pesar_linha(&self, linha: LineId) -> Result<(), Fechado> {
        self.mandar(Comando::PesarLinha { linha }).await
    }

    /// Encerra por vontade própria.
    pub async fn sair(&self) {
        let _ = self.mandar(Comando::Sair).await;
    }

    async fn mandar(&self, comando: Comando) -> Result<(), Fechado> {
        self.comandos.send(comando).await.map_err(|_| Fechado)
    }
}

impl Drop for Enlace {
    fn drop(&mut self) {
        self.tarefa.abort();
    }
}

/// A sessão acabou; não há para quem mandar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fechado;

impl std::fmt::Display for Fechado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a sessão já acabou")
    }
}

impl std::error::Error for Fechado {}

// ------------------------------------------------------------------- o motor

/// O que roda na tarefa: a conexão, a bateria, e a política entre as duas.
struct Motor {
    destino: Destino,
    chave: SigningKey,
    pins: Arc<dyn PinStore>,
    cliente: Option<Client>,
    bateria: Battery,
    inicio: Instant,
    /// O que restaurar ao reconectar.
    cage: Option<CageId>,
    linha: Option<LineId>,
    at_field: bool,
    isolamento: bool,
    avisos: mpsc::UnboundedSender<Aviso>,
    rtt: Arc<std::sync::atomic::AtomicU64>,
}

/// De quanto em quanto tempo a bateria é consultada.
///
/// Menor que o intervalo de ping e muito menor que o menor backoff, para que
/// nem o ping nem uma tentativa de reconexão fiquem esperando a tica seguinte.
const TICA: Duration = Duration::from_millis(200);

impl Motor {
    async fn rodar(mut self, mut comandos: mpsc::Receiver<Comando>) {
        let mut tica = tokio::time::interval(TICA);
        tica.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Só há o que ler quando há conexão. Sem ela, a espera é o relógio.
            let houve_evento = match self.cliente.as_mut() {
                Some(cliente) => tokio::select! {
                    evento = cliente.next_event() => Some(evento),
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido),
                            Some(comando) => { self.executar(comando).await; None }
                        }
                    }
                    _ = tica.tick() => None,
                },
                None => tokio::select! {
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido),
                            // Guardado, não perdido: entrar num Cage durante a
                            // queda é uma intenção que vale quando voltar.
                            Some(comando) => { self.lembrar(&comando); None }
                        }
                    }
                    _ = tica.tick() => None,
                },
            };

            if let Some(evento) = houve_evento {
                match evento {
                    Ok(mensagem) => {
                        // O que a reconexão vai refazer também muda quando
                        // **outra pessoa** decide. Sem isto, alguém movido por
                        // um operador voltaria, depois de uma queda, para o
                        // Cage de onde foi tirado: o motor refaz o último Cage
                        // que este cliente pediu, e ele não pediu este.
                        if let ServerMessage::MovedToCage { cage } = mensagem {
                            self.cage = Some(cage);
                        }
                        if matches!(mensagem, ServerMessage::Pong { .. }) {
                            self.bateria.on_pong();
                            if let Some(medido) = self.cliente.as_ref().and_then(Client::rtt) {
                                let micros = u64::try_from(medido.as_micros()).unwrap_or(u64::MAX);
                                self.rtt
                                    .store(micros.max(1), std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        let _ = self.avisos.send(Aviso::Mensagem(Box::new(mensagem)));
                    }
                    // O fluxo caiu. Não é o fim da sessão: é o começo da
                    // bateria.
                    Err(erro) => {
                        tracing::debug!(%erro, "o enlace caiu");
                        self.cair();
                    }
                }
            }

            if self.passo().await {
                return;
            }
        }
    }

    /// Um passo da bateria. Devolve `true` quando a sessão acabou.
    async fn passo(&mut self) -> bool {
        let agora = self.inicio.elapsed();
        match self.bateria.poll(agora) {
            Action::SendPing => {
                if let Some(cliente) = self.cliente.as_mut() {
                    if cliente.send_ping().await.is_err() {
                        self.cair();
                    }
                }
            }
            Action::Reconnect => self.tentar().await,
            Action::EndSession => {
                self.encerrar(Motivo::Descarregou);
                return true;
            }
            Action::Wait => {}
        }

        // A contagem desce mesmo quando nada acontece, que é a maior parte do
        // tempo em que ela é vista.
        if matches!(self.bateria.state(), Link::InternalBattery { .. }) {
            self.anunciar();
        }
        false
    }

    /// A conexão morreu. Entra na bateria e conta para a casca.
    fn cair(&mut self) {
        self.cliente = None;
        let agora = self.inicio.elapsed();
        let antes = self.bateria.state();
        self.bateria.on_connection_lost(agora);
        if antes != self.bateria.state() {
            self.anunciar();
        }
    }

    /// Conta à casca onde o enlace está e quanto falta.
    fn anunciar(&mut self) {
        let agora = self.inicio.elapsed();
        let _ = self.avisos.send(Aviso::Estado {
            estado: self.bateria.state(),
            restante: self.bateria.remaining(agora),
        });
    }

    /// Uma tentativa de reconexão.
    ///
    /// Bloqueia a tarefa enquanto tenta, e isso é aceitável **aqui**: não há
    /// conexão para ler, e os comandos que chegarem esperam na fila em vez de
    /// se perder. O que não podia acontecer é isto rodar dentro do `select!` da
    /// casca, e não roda.
    async fn tentar(&mut self) {
        let resultado = Client::connect(
            self.destino.servidor,
            &self.destino.nome_tls,
            &self.destino.chave_do_pin,
            &self.destino.apelido,
            &self.chave,
            Arc::clone(&self.pins),
            self.destino.segredo.as_deref(),
        )
        .await;

        let agora = self.inicio.elapsed();
        match resultado {
            Ok(mut cliente) => {
                // Restaurar antes de anunciar. Uma casca que recebesse
                // "reconectado" e perguntasse o Cage antes de ele existir veria
                // uma sala vazia e acharia que perdeu gente.
                if let Some(cage) = self.cage {
                    let _ = cliente.insert_plug(cage).await;
                }
                if let Some(linha) = self.linha {
                    let _ = cliente.join_line(linha).await;
                }
                if self.at_field {
                    let _ = cliente.set_at_field(true).await;
                }
                if self.isolamento {
                    let _ = cliente.set_total_isolation(true).await;
                }

                let sessao = cliente.session().clone();
                let media = cliente.media();
                self.cliente = Some(cliente);
                self.bateria.on_reconnected();

                let _ = self.avisos.send(Aviso::Reconectado {
                    media: Box::new(media),
                    sessao: Box::new(sessao),
                });
            }
            // Uma recusa não melhora com insistência, e insistir contra uma
            // credencial rejeitada é a diferença entre reconectar e martelar.
            Err(erro) if !vale_insistir(&erro) => {
                self.encerrar(Motivo::Recusado(format!("{erro:?}")));
            }
            Err(erro) => {
                tracing::debug!(?erro, "tentativa de reconexão falhou");
                self.bateria.on_reconnect_failed(agora);
                self.anunciar();
            }
        }
    }

    async fn executar(&mut self, comando: Comando) {
        self.lembrar(&comando);
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        let resultado = match comando {
            Comando::InserirPlug(cage) => cliente.insert_plug(cage).await,
            Comando::EjetarPlug => cliente.eject_plug().await,
            Comando::AbrirLinha(linha) => cliente.join_line(linha).await,
            Comando::Dizer { linha, corpo, id } => cliente.send_message(linha, &corpo, id).await,
            Comando::Historico { linha, limite } => {
                cliente.fetch_history(linha, None, limite).await
            }
            Comando::AtField(ligado) => cliente.set_at_field(ligado).await,
            Comando::Isolamento(ligado) => cliente.set_total_isolation(ligado).await,
            Comando::CriarCage {
                nome,
                limite,
                linha,
            } => cliente.create_cage(&nome, limite, linha).await,
            Comando::CriarLinha { nome } => cliente.create_line(&nome).await,
            Comando::RenomearCage { cage, nome } => cliente.rename_cage(cage, &nome).await,
            Comando::RenomearLinha { linha, nome } => cliente.rename_line(linha, &nome).await,
            Comando::Expulsar { piloto } => cliente.kick_pilot(piloto).await,
            Comando::Banir {
                piloto,
                motivo,
                expira_em,
            } => {
                cliente
                    .ban_pilot(piloto, motivo.as_deref(), expira_em)
                    .await
            }
            Comando::RemoverMensagem { mensagem } => cliente.remove_message(mensagem).await,
            Comando::MoverPiloto { piloto, cage } => cliente.move_pilot(piloto, cage).await,
            Comando::ApagarCage { cage } => cliente.delete_cage(cage).await,
            Comando::ApagarLinha { linha } => cliente.delete_line(linha).await,
            Comando::PesarLinha { linha } => cliente.weigh_line(linha).await,
            Comando::Sair => return,
        };
        if resultado.is_err() {
            self.cair();
        }
    }

    /// Guarda o que a reconexão vai ter que refazer.
    fn lembrar(&mut self, comando: &Comando) {
        match comando {
            Comando::InserirPlug(cage) => self.cage = Some(*cage),
            Comando::EjetarPlug => self.cage = None,
            Comando::AbrirLinha(linha) => self.linha = Some(*linha),
            Comando::AtField(ligado) => self.at_field = *ligado,
            Comando::Isolamento(ligado) => self.isolamento = *ligado,
            // Fazer uma sala e moderar alguém **não** entram aqui, e a ausência
            // é deliberada nos dois casos. O que se refaz ao reconectar é onde
            // a pessoa estava — o Cage, a Linha, os dois silêncios —, porque
            // voltar sem isso é voltar para outro lugar. Fazer uma sala é coisa
            // que se faz uma vez; repetida depois de uma queda, ela apareceria
            // minutos mais tarde do nada, e duplicada se a pessoa já tivesse
            // pedido de novo à mão. Expulsar é pior: refeito depois de cinco
            // minutos de bateria, derrubaria de novo alguém que já tinha
            // voltado, e ninguém entenderia por quê.
            //
            // Apagar é o pior dos três, e por isso vale escrevê-lo: refeito
            // depois da queda, ele destruiria a sala que alguém fez no lugar da
            // que sumiu — e a confirmação que autorizou o primeiro pedido dizia
            // o tamanho de **outro** estrago. Pesar uma Linha também não volta:
            // é uma pergunta, e a resposta que interessava era a de quando a
            // caixa estava aberta.
            _ => {}
        }
    }

    fn encerrar(&mut self, motivo: Motivo) {
        if let Some(mut cliente) = self.cliente.take() {
            cliente.disconnect();
        }
        let _ = self.avisos.send(Aviso::Encerrado(motivo));
    }
}

/// Confere o que o convite prometeu contra o que o servidor ofereceu.
///
/// Devolve o veredito quando a conexão pode seguir, e o erro quando ela tem que
/// cair. Uma função à parte de [`Enlace::conectar`] porque tudo aqui é decisão
/// sobre valores, e sem isso a fiação inteira ficava sem guarda.
///
/// Os cinco desfechos são exercidos por teste, sem Dogma do outro lado — e os
/// dois de `PinDecision::Matches` importam tanto quanto os de primeiro contato:
/// é neles que mora a política de **não** derrubar. Um link velho contra um
/// servidor já conhecido avisa e segue, porque o TOFU já provou que é o mesmo
/// servidor de ontem; recusar ali trancaria a pessoa para fora de um Dogma que
/// ela usa. Enquanto `Matches` não tinha teste, alargar esta função para
/// recusar também nesse caso passava a suíte inteira.
fn conferir(
    destino: &Destino,
    pin: &PinDecision,
    pins: &dyn PinStore,
) -> Result<Verdict, ConnectError> {
    let veredito = verdict(pin, destino.impressao_esperada.as_deref());

    // O efeito vem **antes** de qualquer saída, e por isso está aqui e não
    // dentro do `if`: o verificador fixa a chave dentro do retorno de chamada
    // do TLS, então devolver o erro sem desfazer o pin deixaria a visita
    // seguinte — sem link para conferir — ver `Matches` e entrar sem hesitar no
    // servidor que acabou de ser rejeitado.
    aplicar_veredito(&veredito, pins, &destino.chave_do_pin);

    if let Verdict::InviteRefused { expected, offered } = &veredito {
        return Err(ConnectError::InviteMismatch {
            expected: expected.clone(),
            offered: offered.clone(),
        });
    }
    Ok(veredito)
}

/// Aplica o que o veredito manda fazer com o pin.
///
/// Separado da decisão porque a decisão é uma tabela pura e isto é um efeito.
/// Só a recusa tem efeito: ela desfaz o pin que o verificador escreveu antes
/// de alguém poder julgar.
///
/// # Por que apagar aqui é seguro
///
/// `InviteRefused` nasce de duas decisões, e só uma delas chega aqui. De
/// `PinDecision::FirstContact` — nada estava fixado antes, então o `unpin`
/// remove exatamente o que este aperto de mão acabou de escrever. De
/// `PinDecision::Changed` também, e **essa** apagaria um pin antigo e legítimo,
/// que é o oposto do ADR 0003; ela não chega porque o verificador reprova a
/// chave trocada no TLS e a falha sobe como [`ConnectError::PinChanged`], sem
/// nunca virar veredito. Se algum dia `Changed` passar a chegar até aqui, esta
/// função precisa distinguir as duas antes de apagar nada.
fn aplicar_veredito(veredito: &Verdict, pins: &dyn PinStore, chave_do_pin: &str) {
    if matches!(veredito, Verdict::InviteRefused { .. }) {
        pins.unpin(chave_do_pin);
    }
}

/// Desfaz o pin que sobrou de um aperto de mão que não terminou.
///
/// O `TofuVerifier` fixa a chave dentro do TLS, e o aperto de mão continua
/// depois disso — abrir o fluxo de controle, o prazo, a credencial, a resposta.
/// Qualquer uma dessas saídas devolve erro com o pin já escrito.
///
/// O que isso estragava: o link promete `B`, o servidor daquele endereço
/// oferece `A` e falha o aperto de mão. `A` fica fixado. Na tentativa seguinte,
/// com o mesmo link, a decisão é `Matches { A }` e o veredito vira
/// `InviteDisagrees` em vez de `InviteRefused` — a conexão é **permitida**, sem
/// desfazer nada e sem erro. Uma falha de aperto de mão convertia a conferência
/// de *recusar* para *avisar*, para sempre, naquele endereço.
///
/// Só apaga o que este aperto escreveu: se já havia pin antes, ele fica.
fn desfazer_pin_orfao(pins: &dyn PinStore, chave_do_pin: &str, fixado_antes: Option<&str>) {
    if fixado_antes.is_none() && pins.pinned(chave_do_pin).is_some() {
        pins.unpin(chave_do_pin);
    }
}

/// Se este erro veio de alguém que **respondeu**.
///
/// A diferença decide qual erro sobra quando nenhum candidato entra. Um Dogma
/// que recusou o convite, ou cuja chave mudou, disse alguma coisa sobre o
/// mundo; um "não alcancei" de um endereço que nunca ia voltar não disse nada,
/// e mostrá-lo no lugar do outro manda a pessoa procurar problema de rede
/// enquanto o Dogma está ali, recusando.
fn alguem_respondeu(erro: &ConnectError) -> bool {
    matches!(
        erro,
        ConnectError::PinChanged { .. }
            | ConnectError::InviteMismatch { .. }
            | ConnectError::Refused { .. }
            | ConnectError::TlsRefused
            | ConnectError::ProtocolViolation
    )
}

/// Se insistir pode dar em outra coisa.
///
/// Uma credencial rejeitada ou um banimento não mudam de resposta por
/// repetição; uma queda de rede muda.
fn vale_insistir(erro: &ConnectError) -> bool {
    !matches!(
        erro,
        ConnectError::PinChanged { .. }
            | ConnectError::Refused { .. }
            | ConnectError::InviteMismatch { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insistir_contra_recusa_nao_muda_a_resposta() {
        // A diferença entre reconectar e martelar. Uma chave trocada é o alerta
        // do ADR 0003 e tentar de novo só o repetiria a cada backoff.
        assert!(!vale_insistir(&ConnectError::PinChanged {
            pinned: "a".into(),
            offered: "b".into(),
        }));
        // Um convite que não confere também não melhora com repetição: seria o
        // mesmo link errado contra o mesmo servidor a cada backoff.
        assert!(!vale_insistir(&ConnectError::InviteMismatch {
            expected: "a".into(),
            offered: "b".into(),
        }));
        assert!(vale_insistir(&ConnectError::Unreachable));
        assert!(vale_insistir(&ConnectError::HandshakeTimeout));
    }

    #[test]
    fn uma_recusa_desfaz_o_pin_que_o_verificador_acabou_de_escrever() {
        // Sem isto a recusa é decorativa: a visita seguinte, sem link para
        // conferir, veria `Matches` e entraria no servidor recém-rejeitado.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, &loja, "casa");

        assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
    }

    #[test]
    fn um_veredito_que_nao_recusa_deixa_o_pin_onde_esta() {
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, &loja, "casa");

        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    /// Um destino de teste onde a chave do pin e o nome TLS são **diferentes**.
    ///
    /// Diferentes de propósito: confundir os dois já custou caro a este projeto
    /// uma vez — dois Dogmas numa LAN dividindo a entrada `localhost`, e o
    /// segundo parecendo o primeiro com a chave trocada (`tofu.rs`). Um teste
    /// em que os dois valores são iguais não pega essa troca.
    fn destino_de_teste(impressao_esperada: Option<&str>) -> Destino {
        Destino {
            servidor: "127.0.0.1:1".parse().expect("endereço"),
            nome_tls: "localhost".into(),
            chave_do_pin: "casa".into(),
            apelido: "piloto".into(),
            segredo: None,
            impressao_esperada: impressao_esperada.map(str::to_owned),
        }
    }

    #[test]
    fn um_convite_que_nao_confere_derruba_a_conexao_e_desfaz_o_pin() {
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());
        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };

        let erro = conferir(&destino_de_teste(Some("bbbb2222")), &decisao, &loja)
            .expect_err("um convite que não confere tinha que derrubar a conexão");

        // Quem prometeu é o link, quem ofereceu é o servidor. Trocar os dois
        // faria a casca acusar o lado errado.
        assert_eq!(
            erro,
            ConnectError::InviteMismatch {
                expected: "bbbb2222".into(),
                offered: "aaaa1111".into(),
            }
        );
        // Sob a **chave do pin**, não sob o nome TLS.
        assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
    }

    #[test]
    fn um_convite_que_confere_deixa_seguir_e_diz_que_foi_conferido() {
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());
        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };

        let veredito = conferir(&destino_de_teste(Some("aaaa1111")), &decisao, &loja)
            .expect("o convite confere; não havia o que recusar");

        assert_eq!(
            veredito,
            Verdict::FirstContactVerified {
                fingerprint: "aaaa1111".into()
            }
        );
        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    #[test]
    fn sem_convite_o_primeiro_contato_segue_cego_como_sempre_foi() {
        // Quem digitou o endereço à mão não tem o que conferir, e recusar aqui
        // trancaria para fora todo mundo que não veio de um link.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());
        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };

        let veredito = conferir(&destino_de_teste(None), &decisao, &loja)
            .expect("sem convite não há o que recusar");

        assert_eq!(
            veredito,
            Verdict::FirstContact {
                fingerprint: "aaaa1111".into()
            }
        );
        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    #[test]
    fn um_convite_velho_contra_um_pin_que_bate_avisa_e_nao_derruba() {
        // A metade oposta da recusa, e a que some sem ninguém notar: com pin
        // estabelecido, o TOFU já provou que este é o servidor de ontem, então
        // quem está errado é o link. Derrubar aqui trancaria a pessoa para fora
        // de um Dogma que ela usa porque um amigo mandou um link velho.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());
        let decisao = PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        };

        let veredito = conferir(&destino_de_teste(Some("bbbb2222")), &decisao, &loja)
            .expect("um link velho não derruba um servidor já conhecido");

        assert_eq!(
            veredito,
            Verdict::InviteDisagrees {
                expected: "bbbb2222".into(),
                offered: "aaaa1111".into(),
            }
        );
        // Esta é a asserção que segura a política: sem ela o teste passa mesmo
        // se o aviso virar recusa, porque o veredito continuaria o mesmo e só o
        // efeito mudaria.
        assert_eq!(
            loja.pinned("casa"),
            Some("aaaa1111".into()),
            "o aviso desfez o pin, e a próxima visita entraria cega"
        );
    }

    #[test]
    fn um_convite_que_concorda_com_o_pin_nao_tem_nada_a_dizer() {
        // Completa a tabela: pin bate, link concorda, nada acontece.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());
        let decisao = PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        };

        let veredito = conferir(&destino_de_teste(Some("aaaa1111")), &decisao, &loja)
            .expect("não havia nada de errado para recusar");

        assert_eq!(veredito, Verdict::Known);
        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    #[test]
    fn um_aperto_de_mao_que_falhou_nao_deixa_o_pin_que_o_tls_escreveu() {
        // O verificador fixa dentro do retorno de chamada do TLS, e o aperto de
        // mão ainda tem quatro saídas de erro depois disso. O pin que sobrasse
        // de uma delas faria a visita seguinte ver `Matches`, e aí um convite
        // que **não** confere viraria `InviteDisagrees` — de recusar para
        // avisar, sem ninguém decidir isso.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        desfazer_pin_orfao(&loja, "casa", None);

        assert_eq!(loja.pinned("casa"), None);
    }

    #[test]
    fn um_pin_que_ja_existia_sobrevive_a_um_aperto_que_falhou() {
        // Só o que este aperto escreveu é órfão. Apagar um pin antigo porque a
        // rede caiu jogaria fora a memória de que o ADR 0003 depende.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        desfazer_pin_orfao(&loja, "casa", Some("aaaa1111"));

        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    #[test]
    fn o_que_a_reconexao_restaura_e_o_que_a_pessoa_escolheu() {
        let mut motor = motor_de_teste();

        motor.lembrar(&Comando::InserirPlug(CageId(2)));
        motor.lembrar(&Comando::AbrirLinha(LineId(7)));
        motor.lembrar(&Comando::AtField(true));
        motor.lembrar(&Comando::Isolamento(true));

        assert_eq!(motor.cage, Some(CageId(2)));
        assert_eq!(motor.linha, Some(LineId(7)));
        assert!(motor.at_field);
        assert!(motor.isolamento);

        // Ejetar não é uma queda: quem saiu do Cage não volta para ele.
        motor.lembrar(&Comando::EjetarPlug);
        assert_eq!(motor.cage, None);
    }

    #[test]
    fn dizer_nao_e_lembrado() {
        // Só estado é restaurado. Reenviar mensagens numa reconexão duplicaria
        // o que a pessoa disse, e a idempotência do protocolo protege contra
        // reenvio do **mesmo** identificador, não contra este erro.
        let mut motor = motor_de_teste();
        motor.lembrar(&Comando::Dizer {
            linha: LineId(1),
            corpo: "oi".into(),
            id: ClientMessageId(1),
        });
        assert_eq!(motor.linha, None);
    }

    #[test]
    fn moderar_nao_e_lembrado() {
        // Um `Expulsar` guardado seria refeito ao voltar da bateria: cinco
        // minutos depois, alguém que já tinha reconectado cairia de novo, sem
        // ninguém ter pedido nada. O mesmo para banir.
        let mut motor = motor_de_teste();
        motor.lembrar(&Comando::InserirPlug(CageId(2)));

        motor.lembrar(&Comando::Expulsar { piloto: PilotId(9) });
        motor.lembrar(&Comando::Banir {
            piloto: PilotId(9),
            motivo: None,
            expira_em: None,
        });
        motor.lembrar(&Comando::MoverPiloto {
            piloto: PilotId(9),
            cage: CageId(5),
        });
        motor.lembrar(&Comando::RemoverMensagem {
            mensagem: MessageId(1),
        });

        assert_eq!(
            motor.cage,
            Some(CageId(2)),
            "moderar outra pessoa mexeu em onde este cliente está"
        );
    }

    fn motor_de_teste() -> Motor {
        let (avisos, _) = mpsc::unbounded_channel();
        Motor {
            destino: Destino {
                servidor: "127.0.0.1:1".parse().expect("endereço"),
                nome_tls: "localhost".into(),
                chave_do_pin: "127.0.0.1:1".into(),
                apelido: "piloto".into(),
                segredo: None,
                impressao_esperada: None,
            },
            chave: SigningKey::from_bytes(&[7; 32]),
            pins: Arc::new(crate::tofu::MemoryPinStore::new()),
            cliente: None,
            bateria: Battery::new(),
            inicio: Instant::now(),
            cage: None,
            linha: None,
            at_field: false,
            isolamento: false,
            avisos,
            rtt: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}
