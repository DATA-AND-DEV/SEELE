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

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::control::ServerMessage;
use seele_proto::ids::{AttachmentId, CageId, ClientMessageId, LineId, MessageId, PilotId};
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
    /// Uma transferência andou. ADR 0027.
    Transferencia(Transferencia),
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
            Self::Transferencia(estado) => f.debug_tuple("Transferencia").field(estado).finish(),
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
    Anexar(Box<Anexo>),
    SalvarAnexo {
        anexo: AttachmentId,
        destino: std::path::PathBuf,
    },
    /// Baixar um anexo pequeno **para a memória**, para olhar os bytes dele.
    ///
    /// O segundo comando deste enum que carrega para onde responder, e pelo
    /// mesmo motivo do `PesarLinha`: a resposta só serve a quem perguntou,
    /// enquanto a caixa que ela enche estiver aberta. Um `Aviso` levaria
    /// megabytes pelo barramento de eventos, que existe para presença e
    /// andamento.
    PreverAnexo {
        anexo: AttachmentId,
        resposta: tokio::sync::oneshot::Sender<Previa>,
    },
    Sair,
}

/// Quanto tempo se espera o Dogma abrir o fluxo de um anexo pedido.
///
/// Uma recusa nunca abre fluxo nenhum — a razão vem pelo controle —, então sem
/// prazo esta espera seria para sempre. Dez segundos é muito mais do que um
/// Dogma doméstico leva para começar a mandar e pouco para deixar uma tela
/// esperando por bytes que não vêm.
const ESPERA_DE_ANEXO: Duration = Duration::from_secs(10);

/// Um arquivo para mandar, com a mensagem que vai junto.
///
/// ADR 0027. O corpo viaja com o arquivo e não num `Dizer` separado: a
/// mensagem só é publicada quando os bytes chegam inteiros, e duas metades da
/// mesma mensagem em dois caminhos teriam uma ordem para errar.
#[derive(Debug, Clone)]
pub struct Anexo {
    /// Em que Linha.
    pub linha: LineId,
    /// A chave de idempotência da mensagem. É por ela que a tela reconhece a
    /// própria subida, e é ela que torna uma retentativa segura.
    pub id: ClientMessageId,
    /// O que a pessoa escreveu ao lado do arquivo. Pode ser vazio.
    pub corpo: String,
    /// Onde o arquivo está nesta máquina. Nunca sai daqui.
    pub caminho: std::path::PathBuf,
    /// Que nome dar a ele do outro lado.
    pub nome: String,
    /// Que tipo alegar que ele é. Alegação, e tratada como tal.
    pub tipo: String,
}

/// O que voltou de um pedido de prévia. ADR 0027.
///
/// **Nada disto encosta no disco.** É a linha entre prever e salvar: salvar é
/// um ato de quem recebeu, num lugar que a pessoa escolheu, e uma miniatura que
/// deixasse uma cópia num diretório de cache teria feito esse ato acontecer sem
/// ninguém pedir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Previa {
    /// Os bytes, inteiros e conferidos contra o hash, só na memória.
    Bytes(Vec<u8>),
    /// Maior do que esta janela desenha. Nenhum byte foi lido.
    ///
    /// Enumerado e não erro: nada deu errado, e quem está olhando merece uma
    /// frase diferente da que uma transferência quebrada recebe.
    GrandeDemais {
        /// O que o Dogma teria mandado.
        tamanho: u64,
    },
    /// Não veio: expirou, não existe, ou não chegou inteiro. A razão, quando é
    /// do Dogma, chega pelo controle como `ServerMessage::AttachmentUnavailable`.
    NaoVeio,
}

/// Onde uma transferência está.
///
/// Enumerado, e é o que a tela precisa para não mentir: enquanto sobe, uma
/// barra com o total — que é sempre conhecido, porque quem escolheu o arquivo
/// sabe o tamanho dele. **Se cair, [`Transferencia::Caiu`]**, e a frase dessa
/// variante tem de dizer que recomeçar recomeça do zero: o ADR 0027 não tem
/// retomada, e isso precisa ser dito a quem está esperando em vez de
/// descoberto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transferencia {
    /// Subindo, com quantos bytes já foram de quantos.
    Subindo {
        /// Qual mensagem.
        id: ClientMessageId,
        /// Bytes que já saíram.
        feito: u64,
        /// Bytes ao todo. Sempre conhecido.
        total: u64,
    },
    /// Todos os bytes saíram. A mensagem aparece na Linha em seguida.
    Subiu {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// O Dogma cortou o fluxo: recusou. A razão vem pelo controle, como
    /// `ServerMessage::AttachmentRefused`.
    Recusada {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// O enlace caiu no meio. **Recomeçar recomeça do zero.**
    Caiu {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// Baixando, com quantos bytes já vieram de quantos.
    Baixando {
        /// Qual anexo.
        anexo: AttachmentId,
        /// Bytes que já chegaram.
        feito: u64,
        /// Bytes ao todo.
        total: u64,
    },
    /// O arquivo está no disco de quem recebeu, onde a pessoa escolheu.
    Salvo {
        /// Qual anexo.
        anexo: AttachmentId,
        /// Onde ficou.
        caminho: std::path::PathBuf,
    },
    /// Não deu para salvar. Se o motivo for do Dogma, ele vem pelo controle
    /// como `ServerMessage::AttachmentUnavailable`.
    NaoSalvou {
        /// Qual anexo.
        anexo: AttachmentId,
    },
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

/// Quanto se espera entre avisar o ponto de encontro e começar o aperto de mão.
///
/// É o que tem de caber entre o `LEVE` sair daqui e o `Initial` chegar ao NAT do
/// outro lado: uma perna até o ponto, mais uma perna do ponto até o anfitrião —
/// que somadas dão mais ou menos a ida e volta que o ADR 0022 mediu entre 20 e
/// 200 ms.
///
/// **Erra-se para baixo de propósito.** Errar para baixo custa um PTO do quinn,
/// que cabe folgado nos 4 s do candidato; errar para cima é pago sempre, por
/// todo mundo, inclusive por quem ia conectar de qualquer jeito.
const ESPERA_DO_FURO: Duration = Duration::from_millis(200);

/// Quantos avisos saem por candidato que precisa de furo, e de quanto em quanto.
///
/// Três, espaçados, **enquanto o aperto de mão corre**. É a retentativa que não
/// existia: antes eram dois avisos antes do laço, e um `AQUI` perdido custava a
/// conexão inteira em silêncio — o anfitrião nunca furava, o candidato queimava
/// os quatro segundos, e o erro que saía era o de outro endereço.
const AVISOS_POR_CANDIDATO: u8 = 3;
/// O intervalo entre eles.
const INTERVALO_DO_AVISO: Duration = Duration::from_millis(700);

/// O prazo de um candidato privado que não é desta rede.
///
/// Um `192.168.x.x` visto de outra casa não devolve ICMP nenhum: ele queima o
/// prazo inteiro. Um segundo cabe dez idas e voltas de rede local e um PTO.
///
/// **Nunca descartar, só encurtar.** Um /16 configurado à mão ou uma VPN
/// capturando a rota dão falso negativo, e falso negativo só custa velocidade.
const PRAZO_DE_CANDIDATO_DISTANTE: Duration = Duration::from_secs(1);

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
        Self::conectar_entre_com_bilhete(destinos, None, chave, pins).await
    }

    /// O mesmo, avisando o ponto de encontro do convite a cada candidato.
    ///
    /// Degrau 4 do ADR 0022. O bilhete vem do `enc` do `seele://` — ADR 0006 —,
    /// e o que ele acrescenta é um datagrama **colado em cada tentativa que
    /// precisa dele**: o ponto de encontro conta ao anfitrião de onde viemos, o
    /// anfitrião manda um pacote para cá, e o roteador dele passa a deixar
    /// entrar o aperto de mão que sai logo em seguida.
    ///
    /// # Por que colado, e não antes do laço
    ///
    /// Porque é o defeito que este ciclo existe para consertar. O aviso saía uma
    /// vez, antes de tudo, e o furo do outro lado abre por menos de um segundo;
    /// o primeiro candidato do convite é o da rede de casa, que de outra casa
    /// queima o prazo inteiro. Quando a vez do candidato refletido chegava, o
    /// furo tinha fechado havia segundos — quatro no melhor caso, doze no pior,
    /// e um teste de campo com duas casas falhou exatamente assim.
    ///
    /// Agora `encontro::Batida::preparar` só abre o socket e resolve o nome.
    /// **Nenhum pacote sai daqui**: quem manda é o laço, uma vez por candidato
    /// que precisa de furo, e mais dois avisos espaçados enquanto o aperto de
    /// mão corre.
    ///
    /// # Por que todos os candidatos passam pelo mesmo socket
    ///
    /// Porque o furo é por porta. O anfitrião abriu caminho para a porta de onde
    /// o aviso saiu, e um aperto de mão saindo de outra porta continuaria
    /// batendo numa porta fechada. Cada tentativa recebe uma cópia daquele
    /// socket — o original fica vivo aqui até o fim, para que a porta não seja
    /// devolvida ao sistema entre uma tentativa e a seguinte.
    ///
    /// # O que acontece quando não dá para bater
    ///
    /// Conecta como sempre conectou. Um ponto de encontro fora do ar, um convite
    /// sem impressão digital ou uma máquina sem rota nenhuma fazem o degrau 4
    /// não acontecer — e nenhum dos endereços do convite depende dele.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar_entre`].
    pub async fn conectar_entre_com_bilhete(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        // Preparado antes do laço porque o socket tem de ser um só — o NAT
        // mapeia por porta interna. Mas **nenhum pacote sai daqui**: o aviso é
        // por candidato, e é essa mudança que conserta a corrida.
        let batida = match &bilhete {
            Some(bilhete) => {
                let impressao = destinos
                    .first()
                    .and_then(|destino| destino.impressao_esperada.as_deref());
                crate::encontro::Batida::preparar(bilhete, impressao).await
            }
            None => None,
        };
        Self::tentar_entre(destinos, batida.as_ref(), bilhete, chave, pins).await
    }

    /// O laço de tentativas, com ou sem furo de NAT.
    async fn tentar_entre(
        destinos: Vec<Destino>,
        batida: Option<&crate::encontro::Batida>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        // Uma cópia por tentativa, e o original vivo até o fim: um `Endpoint`
        // fecha o socket dele ao ser recolhido, e sem o original a porta que o
        // anfitrião furou voltaria para o sistema no meio do caminho.
        let emprestar = || batida.and_then(crate::encontro::Batida::emprestar_socket);
        let mut candidatos = destinos.into_iter().peekable();
        let Some(primeiro) = candidatos.next() else {
            // Ninguém chama assim, e devolver um erro é melhor que entrar num
            // laço que termina sem resposta nenhuma.
            return Err(ConnectError::Unreachable);
        };
        if candidatos.peek().is_none() {
            // Um convite de um endereço só continua sem prazo novo — o caminho
            // de antes, para um link antigo. Mas o aviso ele leva: um convite
            // que traz **só** o endereço refletido é exatamente a casa que mais
            // depende do degrau 4, e pular o aviso aqui deixaria sem furo quem
            // não tem outra chance.
            let repeticao = avisar_pelo_candidato(batida, primeiro.servidor).await;
            let resultado = Self::conectar_por(emprestar(), bilhete, primeiro, chave, pins).await;
            if let Some(repeticao) = repeticao {
                repeticao.abort();
            }
            return resultado;
        }

        let mut primeira_falha: Option<ConnectError> = None;
        let mut respondeu: Option<ConnectError> = None;
        for destino in std::iter::once(primeiro).chain(candidatos) {
            let onde = destino.servidor;
            let chave_do_pin = destino.chave_do_pin.clone();
            let fixado_antes = pins.pinned(&chave_do_pin);

            // O aviso sai **agora**, para este candidato, e o aperto de mão sai
            // logo atrás dele. É a coordenação inteira desta tarefa: o furo do
            // outro lado dura menos de um segundo, e a única forma de o `Initial`
            // caber dentro dele é os dois saírem juntos.
            let repeticao = avisar_pelo_candidato(batida, onde).await;

            // Um candidato privado de outra casa não devolve ICMP nenhum: ele
            // queima o prazo inteiro sem nunca ter tido chance. Encurtar é o que
            // faz quatro endereços mortos custarem quatro segundos em vez de
            // dezesseis — e é só encurtar, nunca descartar, porque um /16 à mão
            // ou uma VPN capturando a rota dão falso negativo.
            let prazo = if e_de_outra_casa(onde) {
                PRAZO_DE_CANDIDATO_DISTANTE
            } else {
                PRAZO_POR_CANDIDATO
            };

            let tentativa = Self::conectar_por(
                emprestar(),
                bilhete.clone(),
                destino,
                chave.clone(),
                Arc::clone(&pins),
            );

            let falha = match tokio::time::timeout(prazo, tentativa).await {
                Ok(Ok(enlace)) => {
                    if let Some(repeticao) = repeticao {
                        repeticao.abort();
                    }
                    return Ok(enlace);
                }
                Ok(Err(erro)) => erro,
                Err(_) => {
                    // O aperto de mão foi cancelado no meio, e o `conectar` não
                    // chegou à limpeza dele. O pin que o TLS possa ter escrito
                    // some aqui, pelo motivo escrito em `desfazer_pin_orfao`.
                    desfazer_pin_orfao(pins.as_ref(), &chave_do_pin, fixado_antes.as_deref());
                    ConnectError::HandshakeTimeout
                }
            };
            // A repetição para quando o candidato termina, dando certo ou não:
            // avisar sobre um candidato que já falhou gastaria furo da janela do
            // anfitrião — sessenta por dez segundos — por um caminho que ninguém
            // vai tentar de novo.
            if let Some(repeticao) = repeticao {
                repeticao.abort();
            }
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
        Self::conectar_por(None, None, destino, chave, pins).await
    }

    /// O mesmo, pelo socket que já furou o NAT. Degrau 4 do ADR 0022.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar`].
    async fn conectar_por(
        local: Option<std::net::UdpSocket>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        // Antes de o TLS ter chance de escrever qualquer coisa. Ver
        // [`desfazer_pin_orfao`].
        let fixado_antes = pins.pinned(&destino.chave_do_pin);

        let resultado = Client::connect_por(
            local,
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
            // Guardado para a reconexão, e não só para a primeira entrada: uma
            // reconexão sai de um socket novo, com uma porta nova, e o caminho
            // que o anfitrião furou era para a porta velha. Sem bater de novo, a
            // bateria de cinco minutos contaria até o fim contra uma porta
            // fechada.
            bilhete,
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

    /// Manda um arquivo, num fluxo só dele.
    ///
    /// Volta assim que a transferência foi enfileirada, e não quando ela
    /// terminou: o andamento chega por [`Aviso::Transferencia`], e a mensagem
    /// só aparece na Linha depois de os bytes chegarem inteiros. É por isso que
    /// enquanto sobe só quem enviou a vê.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn anexar(&self, anexo: Anexo) -> Result<(), Fechado> {
        self.mandar(Comando::Anexar(Box::new(anexo))).await
    }

    /// Pede um anexo e grava onde quem recebeu escolheu.
    ///
    /// **Onde a pessoa escolheu, e em lugar nenhum mais.** O ADR 0027 não dá a
    /// cliente nenhum do SEELE um botão que abre arquivo; salvar é um ato de
    /// quem recebeu.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn salvar_anexo(
        &self,
        anexo: AttachmentId,
        destino: std::path::PathBuf,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::SalvarAnexo { anexo, destino }).await
    }

    /// Pede os bytes de um anexo **para a memória**, para olhar o começo deles.
    ///
    /// Devolve a caixa em que a resposta vai cair, e devolve **na hora**: quem
    /// chama decide onde esperar. Esperar aqui dentro faria a fila de comandos
    /// desta sessão parar pelo tempo de um download — ninguém conseguiria dizer
    /// uma frase enquanto uma prévia baixa, que é exatamente o bloqueio de
    /// cabeça de fila que o fluxo próprio de cada anexo existe para evitar.
    ///
    /// Um anexo maior que [`seele_core::preview::PREVIEW_LIMIT`] volta como
    /// [`Previa::GrandeDemais`] sem que um byte do corpo seja lido.
    ///
    /// [`seele_core::preview::PREVIEW_LIMIT`]: crate::preview::PREVIEW_LIMIT
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn prever_anexo(
        &self,
        anexo: AttachmentId,
    ) -> Result<tokio::sync::oneshot::Receiver<Previa>, Fechado> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::PreverAnexo { anexo, resposta })
            .await?;
        Ok(caixa)
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
    /// O bilhete de encontro do convite, quando o link trouxe um.
    ///
    /// Degrau 4 do ADR 0022, e ele vale por reconexão: ver [`Motor::tentar`].
    bilhete: Option<seele_proto::uri::Bilhete>,
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
        // O degrau 4 de novo, e não só na primeira entrada. Esta tentativa sai
        // de um socket novo — porta nova —, e o caminho que o anfitrião abriu
        // era para a porta anterior. Sem avisar o ponto de encontro outra vez, a
        // reconexão bate numa porta fechada até a bateria acabar.
        //
        // Aqui não há laço de candidatos: a reconexão volta ao endereço que
        // atendeu. Mesmo assim o aviso passa pelo mesmo `avisar_pelo_candidato`
        // da primeira entrada, e pela mesma razão — se o endereço que atendeu
        // for o da rede de casa, não há furo a pedir, e pedir gastaria a janela
        // do anfitrião a cada tica da bateria.
        let batida = match &self.bilhete {
            Some(bilhete) => {
                crate::encontro::Batida::preparar(
                    bilhete,
                    self.destino.impressao_esperada.as_deref(),
                )
                .await
            }
            None => None,
        };
        let repeticao = avisar_pelo_candidato(batida.as_ref(), self.destino.servidor).await;
        let furo = batida
            .as_ref()
            .and_then(crate::encontro::Batida::emprestar_socket);
        let resultado = Client::connect_por(
            furo,
            self.destino.servidor,
            &self.destino.nome_tls,
            &self.destino.chave_do_pin,
            &self.destino.apelido,
            &self.chave,
            Arc::clone(&self.pins),
            self.destino.segredo.as_deref(),
        )
        .await;

        // Esta tentativa acabou, dando certo ou não, e o que a repetição
        // avisaria daqui para a frente é sobre uma porta que já foi usada.
        if let Some(repeticao) = repeticao {
            repeticao.abort();
        }

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

            // Numa tarefa própria, e não aqui dentro. Executar vinte megabytes
            // no laço de comandos devolveria, dentro do cliente, exatamente o
            // bloqueio de cabeça de fila que o fluxo próprio existe para
            // evitar: ninguém conseguiria dizer uma frase enquanto o arquivo
            // sobe. `Transfers` é clonável para isto.
            Comando::Anexar(anexo) => {
                let transferencias = cliente.transfers();
                let avisos = self.avisos.clone();
                let id = anexo.id;
                tokio::spawn(async move {
                    let andamento = |feito, total| {
                        let _ = avisos.send(Aviso::Transferencia(Transferencia::Subindo {
                            id,
                            feito,
                            total,
                        }));
                    };
                    let pedido = crate::client::AttachmentRequest {
                        line: anexo.linha,
                        client_message_id: anexo.id,
                        body: &anexo.corpo,
                        replies_to: None,
                        path: &anexo.caminho,
                        file_name: &anexo.nome,
                        declared_type: &anexo.tipo,
                    };
                    let fim = match transferencias.send_attachment(&pedido, andamento).await {
                        Ok(crate::client::Sent::Delivered { .. }) => Transferencia::Subiu { id },
                        Ok(crate::client::Sent::Stopped { .. }) => Transferencia::Recusada { id },
                        Ok(crate::client::Sent::Interrupted { .. }) | Err(_) => {
                            Transferencia::Caiu { id }
                        }
                    };
                    let _ = avisos.send(Aviso::Transferencia(fim));
                });
                Ok(())
            }

            Comando::SalvarAnexo { anexo, destino } => {
                let transferencias = cliente.transfers();
                let avisos = self.avisos.clone();
                let pedido = cliente.fetch_attachment(anexo).await;
                tokio::spawn(async move {
                    let andamento = |feito, total| {
                        let _ = avisos.send(Aviso::Transferencia(Transferencia::Baixando {
                            anexo,
                            feito,
                            total,
                        }));
                    };
                    let fim = match transferencias
                        .receive_attachment(anexo, &destino, ESPERA_DE_ANEXO, andamento)
                        .await
                    {
                        Ok(_) => Transferencia::Salvo {
                            anexo,
                            caminho: destino,
                        },
                        Err(_) => Transferencia::NaoSalvou { anexo },
                    };
                    let _ = avisos.send(Aviso::Transferencia(fim));
                });
                pedido
            }

            // A mesma forma do `SalvarAnexo`, com duas diferenças que são a
            // decisão inteira: os bytes param na memória, e o teto que os
            // limita é o desta janela — não o do disco de quem hospeda.
            Comando::PreverAnexo { anexo, resposta } => {
                let transferencias = cliente.transfers();
                let pedido = cliente.fetch_attachment(anexo).await;
                tokio::spawn(async move {
                    let fim = match transferencias
                        .preview_attachment(anexo, crate::preview::PREVIEW_LIMIT, ESPERA_DE_ANEXO)
                        .await
                    {
                        Ok(crate::client::Previewed::Whole(bytes)) => Previa::Bytes(bytes),
                        Ok(crate::client::Previewed::TooBig { byte_size }) => {
                            Previa::GrandeDemais { tamanho: byte_size }
                        }
                        Err(_) => Previa::NaoVeio,
                    };
                    let _ = resposta.send(fim);
                });
                pedido
            }

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

/// Avisa o ponto de encontro por causa **deste** candidato, e deixa a repetição
/// correndo enquanto o aperto de mão dele acontece.
///
/// É o conserto do ciclo em uma função. O aviso saía uma vez, antes do laço, e o
/// furo que ele provoca do outro lado dura menos de um segundo; o aperto de mão
/// que devia atravessar aquele furo chegava de quatro a doze segundos depois,
/// porque o candidato da rede de casa vinha primeiro e gastava o prazo inteiro.
/// Aqui o aviso sai colado no candidato que precisa dele, e nada sai pelos
/// outros.
///
/// # O que devolve
///
/// A tarefa que repete o aviso, para quem chama abortá-la quando o candidato
/// termina — avisar sobre um candidato que já falhou gastaria furo da janela do
/// anfitrião por um caminho que ninguém vai tentar de novo.
///
/// `None` quando não houve aviso nenhum: sem bilhete, num candidato que não
/// precisa de furo, ou quando o envio foi recusado. Nos três não há nada para
/// abortar.
///
/// # Por que um aviso recusado não derruba nada
///
/// Porque ele é de **um** candidato. Um `ENETUNREACH` no caminho até o ponto de
/// encontro não diz nada sobre o candidato seguinte, e transformar essa falha em
/// erro de conexão trocaria um defeito por outro. O erro é registrado — e a
/// linha só diz que avisou quando avisou, porque a versão anterior deste código
/// afirmava «avisamos o ponto de encontro» mesmo quando o `send_to` tinha sido
/// recusado, e um log que mente sobre isso custa a próxima investigação inteira.
async fn avisar_pelo_candidato(
    batida: Option<&crate::encontro::Batida>,
    onde: SocketAddr,
) -> Option<tokio::task::JoinHandle<()>> {
    let batida = batida?;
    // Só o candidato refletido depende de alguém ter furado o caminho. O da rede
    // de casa não paga metadado, não gasta furo da janela do anfitrião, e não
    // espera um milissegundo.
    //
    // A conta da janela mudou de tamanho com esta tarefa, e é bom que esteja
    // escrita aqui, onde o gasto nasce. Cada aviso vira um furo do outro lado, e
    // um candidato público custa `AVISOS_POR_CANDIDATO` avisos, não um: a
    // repetição vive 1,4 s e o prazo de um candidato público morto é de 4,2 s,
    // então ela sempre gasta os três. Um convite de quatro candidatos públicos
    // custa **doze** furos a quem entra — onze a mais que antes desta tarefa.
    // Por isso `FUROS_POR_JANELA`, do lado do anfitrião, subiu de 20 para 60 no
    // mesmo commit: sem isso o teto passaria a barrar duas ou três entradas
    // legítimas simultâneas em vez de barrar abuso.
    //
    // O que esta guarda economiza continua sendo o principal: um convite só de
    // endereços de rede de casa não gasta furo nenhum.
    if !e_publico(onde.ip()) {
        return None;
    }

    if let Err(erro) = batida.avisar().await {
        tracing::info!(
            %erro,
            ponto = %batida.ponto(),
            candidato = %onde,
            "não deu para avisar o ponto de encontro por este candidato; o aperto de mão vai assim mesmo"
        );
        return None;
    }
    tracing::info!(
        ponto = %batida.ponto(),
        aviso = %batida.aviso(),
        candidato = %onde,
        "degrau 4: avisamos o ponto de encontro de que estamos chegando"
    );

    // O tempo de o `LEVE` chegar ao ponto, virar `AQUI`, e o `FURO` sair do
    // roteador do anfitrião. Depois disto o `Initial` do quinn encontra o
    // caminho aberto.
    tokio::time::sleep(ESPERA_DO_FURO).await;

    // A cópia divide o mesmo socket — é o `Arc` de dentro da `Batida`. Um
    // segundo socket faria o anfitrião furar para uma porta que o QUIC não usa.
    let repetindo = batida.clone();
    Some(tokio::spawn(async move {
        for _ in 1..AVISOS_POR_CANDIDATO {
            tokio::time::sleep(INTERVALO_DO_AVISO).await;
            if let Err(erro) = repetindo.avisar().await {
                tracing::debug!(%erro, "a repetição do aviso não saiu");
            }
        }
    }))
}

/// Se este endereço é de uma rede privada — a de casa **ou** a de outra casa.
///
/// Sem loopback de propósito: `127.0.0.1` não é uma rede de ninguém, e as duas
/// perguntas que se fazem com isto (precisa de furo? merece prazo curto?) têm
/// resposta própria para ele.
///
/// # Por que `to_canonical` na entrada
///
/// Porque a forma mapeada **é** o caso comum, não a borda. Um ponto de encontro
/// atrás de um socket de pilha dupla enxerga a origem de quem bateu como
/// `::ffff:a.b.c.d`, e é essa origem que volta no `AQUI` e vira o candidato
/// refletido do convite. Este crate já sabe disso — `encontro::mapear` existe
/// exatamente por causa dela.
///
/// Sem canonizar, `::ffff:192.168.1.5` não casava com nenhum dos ramos: o de
/// IPv4 nem era consultado, e o de IPv6 comparava `0x0000` contra `fc00`/`fe80`.
/// O endereço da rede de casa de alguém passava por público, queimava três
/// furos da janela do anfitrião, vazava metadado que ninguém pediu e ainda
/// levava o prazo cheio de quatro segundos em vez de um.
fn e_privado(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(quatro) => quatro.is_private() || quatro.is_link_local() || e_cgnat(quatro),
        IpAddr::V6(seis) => {
            let primeiro = seis.segments().first().copied().unwrap_or(0);
            // `fc00::/7`, os endereços locais únicos da RFC 4193, e `fe80::/10`,
            // o link-local — o par do `169.254.x.x` do outro lado.
            (primeiro & 0xfe00) == 0xfc00 || (primeiro & 0xffc0) == 0xfe80
        }
    }
}

/// Se um candidato depende de alguém ter furado o caminho até ele.
///
/// A negação de "privado, loopback, sem destino ou multicast". Endereços de
/// documentação (`203.0.113.x`, TEST-NET-3) contam como públicos, e é de
/// propósito: eles são globais em tudo que importa aqui — o sistema os roteia
/// para a porta de saída, e é isso que o furo cobre.
///
/// # As três últimas perguntas são feitas na forma escrita, e não na canônica
///
/// `e_privado` canoniza porque a forma mapeada de um endereço privado aparece no
/// campo — é assim que o ponto de encontro reflete a origem de quem está atrás
/// de pilha dupla. Loopback, "sem destino" e multicast **não** são canonizados,
/// e isso é uma escolha, com preço conhecido.
///
/// O que ela compra: um candidato que é público para esta função e que ainda
/// assim responde numa máquina só — `[::ffff:127.0.0.1]:porta`, um socket de
/// verdade no loopback. É o que torna possível medir que o `Initial` sai
/// **depois** do `LEVE`, e por quanto; sem ele isso exigiria duas casas e um NAT
/// entre elas.
///
/// O que ela custa, e não é hipótese: o loopback mapeado **pode** entrar num
/// convite. `alcance::anunciar_com_porta` empurra o endereço refletido para a
/// lista conferindo só a família contra a pilha da escuta, sem filtro de
/// loopback nenhum — então um Dogma cujo ponto de encontro roda na mesma
/// máquina, atrás de socket de pilha dupla, observa `::ffff:127.0.0.1` como
/// origem e publica isso. Quando acontece, quem entra gasta
/// [`AVISOS_POR_CANDIDATO`] furos da janela do anfitrião (três, não um),
/// 3 × 96 bytes de metadado que ninguém pediu, e [`ESPERA_DO_FURO`] a mais antes
/// do aperto de mão; o Dogma fura contra o próprio loopback, e a conexão sobe
/// assim mesmo, porque o candidato sempre foi alcançável sem furo nenhum.
///
/// O que ela **não** custa é segurança, e é por isso que o preço é aceitável: o
/// destino do furo é `bilhete.aviso()`, fixado em `Batida::preparar` e embutido
/// no datagrama. O candidato decide apenas **se** o `LEVE` sai, nunca **para
/// onde** o anfitrião fura. Um candidato mal classificado não redireciona pacote
/// contra terceiro nenhum.
fn e_publico(ip: IpAddr) -> bool {
    !e_privado(ip) && !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast()
}

/// `100.64.0.0/10`, que a RFC 6598 reservou para CGNAT.
fn e_cgnat(quatro: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = quatro.octets();
    a == 100 && (64..128).contains(&b)
}

/// Se este candidato é um endereço privado que **não** é desta rede.
///
/// A pergunta é feita com destino: `connect` num socket UDP não manda pacote
/// nenhum, mas faz o núcleo escolher a rota, e `local_addr` conta **qual
/// endereço meu o sistema usaria para alcançar aquele destino**.
///
/// Isto não é o truque que o ADR 0022 reprovou. Lá a pergunta era "qual é o meu
/// endereço", respondida pela rota padrão, e uma VPN capturava a resposta. Aqui
/// há destino, e é exatamente o que o `connect` responde.
///
/// Quem responde `true` ganha [`PRAZO_DE_CANDIDATO_DISTANTE`] em vez dos quatro
/// segundos: um `192.168.x.x` visto de outra casa não devolve ICMP nenhum e
/// queima o prazo inteiro sem nunca ter tido chance. **Nunca descartar, só
/// encurtar** — um /16 configurado à mão ou uma VPN capturando a rota dão falso
/// negativo, e falso negativo só custa velocidade.
///
/// # A escolha da família da sonda não tem falsificador nesta máquina
///
/// Dito de frente, porque é uma afirmação sem teste que reprove sozinho. A
/// canonização do alvo, logo abaixo, existe por dois motivos, e só um deles é
/// falsificável aqui: a comparação de faixa está defendida por `mesma_rede`, que
/// canoniza por conta própria, mas **a família da sonda só erra numa máquina sem
/// IPv6**. Ali o `bind` de `[::]:0` falha, esta função devolve `false` por
/// omissão, e o candidato da outra casa volta a custar os quatro segundos
/// inteiros.
///
/// Numa máquina de pilha dupla — a que roda estes testes — as duas defesas se
/// sobrepõem, e tirar a canonização daqui não acende nada. A ironia é que a
/// máquina sem IPv6 é exatamente a casa atrás de CGNAT que o degrau 4 existe
/// para servir: o caso que mais depende desta linha é o único em que ela pode
/// ser medida. Por isso ele está na lista do portão de campo, na seção 8.4 do
/// spec deste ciclo, e não numa suíte que roda aqui.
fn e_de_outra_casa(candidato: SocketAddr) -> bool {
    if !e_privado(candidato.ip()) {
        return false;
    }
    // Na forma canônica, e não na escrita, pelo mesmo motivo de `e_privado` —
    // com um agravante próprio, que é a **família da sonda**. Um
    // `::ffff:10.255.255.1` escrito como veio faria a sonda ser aberta em IPv6;
    // numa máquina sem IPv6 o `bind` falha, esta função devolve `false` por
    // omissão, e o candidato da outra casa volta a custar os quatro segundos
    // inteiros. Com a forma canônica a sonda é v4, que é o que o destino é.
    //
    // (A comparação de faixa também precisa da forma canônica, e `mesma_rede`
    // canoniza por conta própria — ver o doc dela. Aqui não se confia nisso: as
    // duas coisas são defeitos diferentes e cada uma se defende.)
    let alvo = SocketAddr::new(candidato.ip().to_canonical(), candidato.port());
    // Da mesma família do destino: uma sonda IPv4 não tem o que responder sobre
    // um `fd00::` e chamaria de distante o vizinho do lado.
    let daqui_qualquer = if alvo.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0))
    };
    let Ok(sonda) = std::net::UdpSocket::bind(daqui_qualquer) else {
        return false;
    };
    if sonda.connect(alvo).is_err() {
        // Sem rota para lá: é de outra casa, e o sistema já sabe disso.
        return true;
    }
    let Ok(daqui) = sonda.local_addr() else {
        return false;
    };
    !mesma_rede(daqui.ip(), alvo.ip())
}

/// Um /24 para IPv4 e um /64 para IPv6.
///
/// É chute quando a rede é /16, e o chute é para o lado seguro: um vizinho
/// legítimo de outra faixa cai no prazo curto e ainda tem um segundo inteiro.
///
/// # Canoniza o que recebe, e não confia em quem chama
///
/// Sem isto, dois endereços na forma mapeada comparariam os quatro primeiros
/// grupos de `::ffff:x` contra os de `::ffff:y` — `0:0:0:0` dos dois lados,
/// iguais sempre — e **todo** par mapeado passaria por vizinho de porta. O
/// único chamador de hoje já canoniza antes de chegar aqui, então este defeito
/// é inalcançável; um segundo chamador o reabriria, e uma pré-condição que só o
/// comentário guarda é a próxima linha de uma lista de revisão.
fn mesma_rede(daqui: IpAddr, la: IpAddr) -> bool {
    match (daqui.to_canonical(), la.to_canonical()) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            a.octets().first_chunk::<3>() == b.octets().first_chunk::<3>()
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            a.segments().first_chunk::<4>() == b.segments().first_chunk::<4>()
        }
        _ => false,
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

    #[tokio::test]
    async fn o_aperto_de_mao_sai_da_mesma_porta_que_bateu_no_ponto_de_encontro() {
        // O degrau 4 do ADR 0022 em uma asserção, e é **a** asserção: o NAT
        // mapeia por porta interna, então o anfitrião fura o caminho para a
        // porta de onde o aviso saiu. Se o aperto de mão sair de outra porta, o
        // furo abre para a porta errada e a conexão continua batendo numa porta
        // fechada — em quase todo roteador doméstico, que filtra por endereço
        // **e** porta.
        //
        // Não precisa de NAT nenhum para ser provado: bastam dois sockets no
        // loopback, um fazendo de ponto de encontro e outro fazendo de Dogma.
        // Nenhum dos dois responde nada — o que se mede é de onde os pacotes
        // saíram, e é isso que o outro lado usaria.
        //
        // # Por que o convite tem dois endereços, e por que o primeiro é público
        //
        // Porque desde o aviso por candidato **nem todo candidato avisa**: o da
        // rede de casa não precisa de furo nenhum, e o loopback do Dogma de
        // teste é ainda menos. Com um convite de um endereço só, no loopback,
        // nada chegaria ao ponto de encontro e este teste mediria o silêncio.
        //
        // Então o convite traz `203.0.113.7` — TEST-NET-3, RFC 5737, público em
        // tudo que importa aqui e onde não há Dogma nenhum — na frente, e o
        // Dogma de teste atrás. O aviso sai por causa do primeiro; o aperto de
        // mão que chega ao segundo é o do mesmo laço, pelo socket emprestado da
        // mesma `Batida`. É exatamente a fiação de produção, e é a porta dela
        // que se compara.
        let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback não abriu");
        let dogma = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback não abriu");
        let onde_o_dogma_atende = dogma.local_addr().expect("endereço");

        let bilhete = seele_proto::uri::Bilhete::novo(
            ponto.local_addr().expect("endereço").to_string(),
            "45.33.32.156:41234",
        )
        .expect("bilhete de teste");
        let destino = |servidor: SocketAddr| Destino {
            servidor,
            nome_tls: "localhost".into(),
            chave_do_pin: servidor.to_string(),
            apelido: "piloto".into(),
            segredo: None,
            // Uma impressão digital de verdade: é dela que sai a marca do
            // aviso, e sem ela não se bate em ponto de encontro nenhum.
            impressao_esperada: Some(
                "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9".to_owned(),
            ),
        };
        let refletido: SocketAddr = "203.0.113.7:8383".parse().expect("endereço");

        // Ninguém atende do outro lado, então isto nunca volta com sucesso. O
        // que interessa acontece nos primeiros segundos, e a tarefa é
        // abandonada no fim.
        let tentativa = tokio::spawn(Enlace::conectar_entre_com_bilhete(
            vec![destino(refletido), destino(onde_o_dogma_atende)],
            Some(bilhete),
            SigningKey::from_bytes(&[7; 32]),
            Arc::new(crate::tofu::MemoryPinStore::new()),
        ));

        let mut balde = [0_u8; 1500];
        // Folgado: o Dogma de teste é o **segundo** candidato, e só é tentado
        // depois de o primeiro queimar o prazo dele.
        let prazo = Duration::from_secs(15);

        let (_, de_quem_bateu) = tokio::time::timeout(prazo, ponto.recv_from(&mut balde))
            .await
            .expect("nada chegou ao ponto de encontro")
            .expect("o ponto de encontro não leu");
        let (_, de_quem_conecta) = tokio::time::timeout(prazo, dogma.recv_from(&mut balde))
            .await
            .expect("nada chegou ao Dogma")
            .expect("o Dogma não leu");

        tentativa.abort();

        assert_eq!(
            de_quem_bateu.port(),
            de_quem_conecta.port(),
            "o aviso saiu de {de_quem_bateu} e o aperto de mão de {de_quem_conecta}: o \
             anfitrião furaria o caminho para uma porta que o QUIC não usa"
        );
    }

    /// Um ponto de encontro de teste que só anota quantos avisos chegaram.
    async fn ponto_que_conta() -> Option<(SocketAddr, Arc<std::sync::Mutex<usize>>)> {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
        let onde = socket.local_addr().ok()?;
        let quantos = Arc::new(std::sync::Mutex::new(0_usize));
        let contador = Arc::clone(&quantos);
        tokio::spawn(async move {
            let mut balde = [0_u8; seele_proto::encontro::TAMANHO];
            while socket.recv_from(&mut balde).await.is_ok() {
                if let Ok(mut conta) = contador.lock() {
                    *conta += 1;
                }
            }
        });
        Some((onde, quantos))
    }

    #[tokio::test]
    async fn a_reconexao_tambem_para_a_repeticao_quando_a_tentativa_acaba() {
        // O terceiro `abort`, e o mais caro de esquecer. Os outros dois estão
        // no laço de candidatos e acontecem uma vez por entrada;
        // este está em `Motor::tentar`, que roda **a cada tica da bateria** —
        // cinco minutos de reconexão contra um endereço público gastariam três
        // furos por tentativa em vez de um, contra uma janela que é de sessenta
        // por dez segundos.
        //
        // Nada precisa cair para provar isto, e nada precisa reconectar. O que
        // torna observável é a tentativa **acabar rápido**: com um nome TLS que
        // o quinn recusa, `Endpoint::connect` devolve erro antes de qualquer
        // pacote, muito antes de a repetição chegar ao segundo aviso — que
        // sairia 700 ms depois do primeiro.
        let Some((ponto, quantos)) = ponto_que_conta().await else {
            return;
        };
        let Ok(bilhete) = seele_proto::uri::Bilhete::novo(ponto.to_string(), "45.33.32.156:41234")
        else {
            panic!("o bilhete de teste não se monta");
        };

        let mut motor = motor_de_teste();
        motor.bilhete = Some(bilhete);
        // Público **e** alcançável na forma mapeada: é o que faz a reconexão
        // pedir furo. Com `127.0.0.1` na forma escrita não sairia aviso nenhum
        // e este teste mediria o próprio silêncio.
        motor.destino.servidor = "[::ffff:127.0.0.1]:9".parse().expect("endereço");
        motor.destino.nome_tls = "nome inválido com espaço".into();
        motor.destino.impressao_esperada =
            Some("3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9".to_owned());

        motor.tentar().await;
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let Ok(conta) = quantos.lock() else {
            return;
        };
        assert_eq!(
            *conta, 1,
            "a tentativa de reconexão acabou na hora e mesmo assim saíram {} \
             avisos: a repetição não foi abortada",
            *conta
        );
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
            bilhete: None,
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
