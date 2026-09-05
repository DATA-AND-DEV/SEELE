//! O enlace com um servidor, incluindo o que fazer quando ele cai.
//!
//! [`Client`] é uma conexão: enquanto ela existe, funciona; quando cai, acaba.
//! Isto é a **sessão**, que é outra coisa — ela atravessa quedas. É aqui que
//! mora a bateria interna de `specs/07-estetica.md`:
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
//! Restaura a sala de voz, a Linha, o mudo e o isolamento: é o que a pessoa
//! escolheu, e voltar sem isso seria voltar para outro lugar. **Não** restaura
//! a voz sozinha — a conexão é nova, e com ela o canal de mídia. A casca recebe
//! [`Aviso::Reconectado`] com o canal novo e reabre o áudio. É honesto: o
//! caminho de áudio realmente recomeça.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::control::ServerMessage;
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, VoiceRoomId,
};
use seele_proto::signal::SignalBand;
use tokio::sync::mpsc;

use crate::battery::{Action, Battery, Link};
use crate::client::{Client, ConnectError, MediaChannel, SessionInfo};
use crate::tofu::PinDecision;
use crate::tofu::PinStore;
use crate::tofu::{verdict, Verdict};
use crate::video::{LimitesDeTela, PedidoDeTela};

/// Onde ficar batendo, e com que credencial.
#[derive(Debug, Clone)]
pub struct Destino {
    /// Endereço do servidor.
    pub servidor: SocketAddr,
    /// O nome que o TLS recebe. Ver [`Client::connect`].
    pub nome_tls: String,
    /// Sob que chave o pin é arquivado. Ver [`Client::connect`].
    pub chave_do_pin: String,
    /// Como aparecer no roster.
    pub apelido: String,
    /// Convite de uso único ou senha do servidor.
    pub segredo: Option<String>,
    /// A impressão digital que o convite prometeu, quando veio de um link.
    ///
    /// `None` para quem digitou o endereço à mão — aí não há o que conferir, e
    /// o primeiro contato segue sendo cego, como sempre foi.
    pub impressao_esperada: Option<String>,
}

/// O que a casca precisa saber.
pub enum Aviso {
    /// O servidor disse algo.
    Mensagem(Box<ServerMessage>),
    /// Onde o enlace está, e quanto resta da bateria.
    ///
    /// Repetido a cada tica enquanto a bateria corre, porque a contagem
    /// regressiva **é** a informação: `specs/07-estetica.md` pede 04:59
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
    /// Uma transmissão de tela alheia começou a chegar.
    ///
    /// Vem antes de qualquer quadro e carrega o que a casca precisa para armar
    /// o decodificador: tamanho e codec. Sem ela, o primeiro quadro chegaria a
    /// uma tela que ainda não sabe de que tamanho é a imagem.
    TelaAbriu {
        /// Qual transmissão, para casar com o que o `Snapshot` já diz.
        tela: ScreenId,
        /// Largura em pixels, como o cabeçalho a declarou.
        largura: u16,
        /// Altura em pixels.
        altura: u16,
    },
    /// Um quadro comprimido de uma tela alheia.
    ///
    /// Os bytes vão crus, como saíram do codificador do outro lado: quem
    /// decodifica é a casca. Esta camada não decodifica de propósito — o
    /// decodificador do sistema, que a janela alcança, é acelerado por hardware
    /// e não exige o módulo do Cisco em quem só assiste. Só quem transmite
    /// precisa dele.
    /// Um pacote de som da tela que se está assistindo.
    ///
    /// Separado do [`Self::TelaQuadro`] porque o destino é outro: a imagem vai
    /// para a casca desenhar, o som vai para a mistura de saída — e é lá que o
    /// isolamento total decide se ele toca.
    TelaSom {
        /// Qual transmissão.
        tela: ScreenId,
        /// Um pacote Opus, como o outro lado o produziu.
        bytes: Vec<u8>,
    },
    /// Um quadro comprimido de uma tela alheia.
    ///
    /// Os bytes vão crus, como saíram do codificador do outro lado: quem
    /// decodifica é a casca. Esta camada não decodifica de propósito — o
    /// decodificador do sistema, que a janela alcança, é acelerado por hardware
    /// e não exige o módulo do Cisco em quem só assiste. Só quem transmite
    /// precisa dele.
    TelaQuadro {
        /// Qual transmissão.
        tela: ScreenId,
        /// Se dá para começar a decodificar por este.
        chave: bool,
        /// O quadro, em Annex-B.
        bytes: Vec<u8>,
    },
    /// Chegou uma transmissão e esta versão não sabe lê-la.
    ///
    /// **É a resposta à tela preta.** Quando o cabeçalho de um fluxo de tela não
    /// decodifica — versão do protocolo que este build não fala, campo que mudou
    /// de forma —, este lado não sabe nem o número da transmissão, então não há
    /// `TelaFechou` a mandar. Antes disto ele simplesmente voltava, e a casca
    /// nunca ficava sabendo que houve alguma coisa: nenhum evento, nenhum
    /// desenho, nenhuma frase.
    ///
    /// Foi relatado assim: «quem assiste com uma versão mais velha vê tela
    /// preta, sem mensagem nenhuma, e a sessão morre em ~3 segundos sem dizer
    /// por quê». A parte «sem mensagem nenhuma» é esta variante que não existia.
    ///
    /// O motivo viaja como texto porque é para uma pessoa ler, e porque o que o
    /// causou é justamente um formato que este build não conhece — enumerá-lo
    /// exigiria conhecer de antemão o que ainda não foi inventado.
    TelaIlegivel {
        /// O que o decodificador do cabeçalho respondeu.
        motivo: String,
    },
    /// A transmissão que estava chegando acabou.
    TelaFechou {
        /// Qual transmissão.
        tela: ScreenId,
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
            Self::Transferencia(estado) => f.debug_tuple("Transferencia").field(estado).finish(),
            Self::TelaAbriu {
                tela,
                largura,
                altura,
            } => f
                .debug_struct("TelaAbriu")
                .field("tela", tela)
                .field("largura", largura)
                .field("altura", altura)
                .finish(),
            Self::TelaIlegivel { motivo } => f
                .debug_struct("TelaIlegivel")
                .field("motivo", motivo)
                .finish(),
            // Pelo mesmo motivo do quadro: os bytes viram um número.
            Self::TelaSom { tela, bytes } => f
                .debug_struct("TelaSom")
                .field("tela", tela)
                .field("bytes", &bytes.len())
                .finish(),
            // Os bytes viram um número: um quadro-chave de 1080p tem 65 KiB, e
            // despejá-los num log é apagar o log.
            Self::TelaQuadro { tela, chave, bytes } => f
                .debug_struct("TelaQuadro")
                .field("tela", tela)
                .field("chave", chave)
                .field("bytes", &bytes.len())
                .finish(),
            Self::TelaFechou { tela } => f.debug_struct("TelaFechou").field("tela", tela).finish(),
            Self::Encerrado(motivo) => f.debug_tuple("Encerrado").field(motivo).finish(),
        }
    }
}

/// Por que a sessão acabou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// A bateria interna descarregou: cinco minutos sem reconectar.
    Descarregou,
    /// O servidor recusou, e insistir não muda a resposta.
    Recusado(String),
    /// Alguém pediu para sair.
    Pedido,
}

/// O que a casca manda fazer.
#[derive(Debug)]
enum Comando {
    /// Semeia a sonda com o caminho medido da última vez com este servidor.
    ///
    /// Comando e não parâmetro de `conectar`: os quatro `conectar*` públicos
    /// passam por um funil privado, e acrescentar um argumento a todos eles
    /// para um número opcional obrigaria cada chamador a dizer «não tenho» —
    /// enquanto quem tem é um só, o app, que é quem abre a lista de conhecidos.
    ///
    /// Chegar depois da conexão não custa nada: a sonda só começa a medir
    /// quando a tela transmite, e ninguém transmite antes de entrar.
    LembrarCaminho(u32),
    InserirPlug(VoiceRoomId),
    EjetarPlug,
    AbrirLinha(ChannelId),
    Dizer {
        linha: ChannelId,
        corpo: String,
        id: ClientMessageId,
    },
    Historico {
        linha: ChannelId,
        limite: u16,
    },
    Muted(bool),
    Isolamento(bool),
    CriarVoiceRoom {
        nome: String,
        limite: u16,
        linha: Option<ChannelId>,
    },
    CriarLinha {
        nome: String,
    },
    RenomearVoiceRoom {
        voice_room: VoiceRoomId,
        nome: String,
    },
    RenomearLinha {
        linha: ChannelId,
        nome: String,
    },
    RenomearServer {
        nome: String,
    },
    /// A imagem de perfil de quem está usando este cliente.
    /// O apelido de quem está usando este cliente.
    MeuApelido {
        /// O nome novo.
        nome: String,
    },
    MinhaImagem {
        /// A figura, ou `None` para não ter.
        icone: Option<Vec<u8>>,
    },
    IconeDoServer {
        icone: Option<Vec<u8>>,
    },
    Expulsar {
        pessoa: PersonId,
    },
    Banir {
        pessoa: PersonId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    },
    RemoverMensagem {
        mensagem: MessageId,
    },
    MoverPersono {
        pessoa: PersonId,
        voice_room: VoiceRoomId,
    },
    ApagarVoiceRoom {
        voice_room: VoiceRoomId,
    },
    ApagarLinha {
        linha: ChannelId,
    },
    PesarLinha {
        linha: ChannelId,
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
    /// Comece a transmitir esta fonte, com estes tetos.
    ///
    /// Boxeado porque carrega o módulo do Cisco carregado e a captura numa
    /// caixa, e um enum é do tamanho do maior braço dele: sem a caixa, toda
    /// tecla digitada pagaria por eles na fila.
    CompartilharTela {
        fonte: Box<PedidoDeTela>,
        limites: LimitesDeTela,
    },
    /// Troque os tetos da transmissão que já está de pé.
    ///
    /// Sem caixa, ao contrário de [`Self::CompartilharTela`]: aqui não vai
    /// módulo nem captura, só três números.
    AjustarLimitesDaTela {
        /// Os tetos novos, como a pessoa os escolheu.
        limites: LimitesDeTela,
    },
    /// Pare de transmitir.
    PararDeCompartilhar,
    /// Peça um quadro-chave a quem está compartilhando.
    ///
    /// De quem **recebe**, e é o que faz alguém que entra no meio de uma
    /// transmissão ver alguma coisa: sem ele chegam só diferenças de um quadro
    /// que nunca se viu, e o decodificador as descarta.
    /// Pede para receber, ou parar de receber, a imagem de uma transmissão.
    ///
    /// **Um comando com uma bandeira, e não dois comandos.** Os dois lados
    /// carregam a mesma coisa — qual tela — e diferem numa palavra; separá-los
    /// duplicaria o caminho inteiro até o `Client` para trocar um verbo no fim.
    Assistir {
        /// Qual transmissão.
        tela: ScreenId,
        /// `true` para receber, `false` para parar.
        quero: bool,
    },
    PedirQuadroChave {
        /// Qual transmissão. O servidor confere se ela é mesmo a da sala.
        tela: ScreenId,
    },
    Sair,
}

/// Quanto tempo se espera o servidor abrir o fluxo de um anexo pedido.
///
/// Uma recusa nunca abre fluxo nenhum — a razão vem pelo controle —, então sem
/// prazo esta espera seria para sempre. Dez segundos é muito mais do que um
/// servidor doméstico leva para começar a mandar e pouco para deixar uma tela
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
    pub linha: ChannelId,
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
        /// O que o servidor teria mandado.
        tamanho: u64,
    },
    /// Não veio: expirou, não existe, ou não chegou inteiro. A razão, quando é
    /// do servidor, chega pelo controle como `ServerMessage::AttachmentUnavailable`.
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
    /// O servidor cortou o fluxo: recusou. A razão vem pelo controle, como
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
    /// Não deu para salvar. Se o motivo for do servidor, ele vem pelo controle
    /// como `ServerMessage::AttachmentUnavailable`.
    NaoSalvou {
        /// Qual anexo.
        anexo: AttachmentId,
    },
}

/// Um candidato que o laço está prestes a tentar, para quem observa de fora.
///
/// Existe porque o laço é o único lugar que sabe as duas coisas ao mesmo tempo:
/// **qual** endereço está sendo tentado agora e **se um `LEVE` saiu por ele**. A
/// segunda só é verdade depois do envio, e quem publica etapas fica uma camada
/// acima — [`crate::chegada::Chegada`], que antes disto publicava a primeira
/// tentativa antes de o laço começar e não via nenhuma das outras.
///
/// Três campos e não o [`Destino`] inteiro: o que atravessa aqui já estava no
/// convite de quem vai ler, e mandar o segredo de entrada junto seria pôr uma
/// credencial numa trilha que vira log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tentativa {
    /// Qual da lista, contando do zero.
    pub candidato: u8,
    /// O endereço que está sendo tentado.
    pub onde: SocketAddr,
    /// Um `LEVE` saiu pelo ponto de encontro por causa deste candidato.
    ///
    /// O que `avisar_pelo_candidato` respondeu: verdadeiro só quando o
    /// datagrama saiu de verdade. Sem bilhete, num candidato que não precisa de
    /// furo, ou com o envio recusado, é falso — e é falso pelo mesmo critério
    /// que faz o log daquela função só dizer que avisou quando avisou.
    pub avisou: bool,
}

/// A sessão com um servidor, viva através de quedas.
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
    /// O caminho de subida que a sonda mediu, em bits por segundo. Zero é
    /// «ninguém mediu ainda».
    ///
    /// Um átomo pela mesma razão que o `rtt`: quem o lê é quem grava a lista de
    /// conhecidos, e transformar cada janela de medição num aviso encheria a
    /// fila de coisas que ninguém precisa ver acontecer.
    caminho_medido: Arc<std::sync::atomic::AtomicU32>,
    tarefa: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for Enlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enlace")
            .field("estado", &self.estado)
            .field("server", &self.sessao.server)
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

/// Quanto se espera antes de disparar o próximo candidato da corrida.
///
/// # O número que esta constante existe para apagar
///
/// Medido em campo, com o cliente rodando de um 5G contra um servidor de
/// verdade: quatro candidatos, os três primeiros sem chance, **9,6 segundos**
/// queimados — e o quarto respondeu em **358 ms**. Em série, o quarto quase
/// nunca era alcançado, e a tela dizia «tempo esgotado» sobre um servidor que
/// estava no ar.
///
/// Aqui o quarto **começa** em 750 ms, e a conversa abre em ~1,1 s.
///
/// # A previsão que estava escrita aqui, e a resposta
///
/// O que esta constante substituiu — `PRAZO_DA_PRIMEIRA_VOLTA`, a paciência
/// curta da primeira de duas voltas em série — carregava esta frase, e ela
/// merece sobreviver ao código que a hospedava:
///
/// > O que isto **não** faz é tentar em paralelo, que seria melhor ainda. Vários
/// > apertos de mão ao mesmo tempo escrevem pins ao mesmo tempo, e o pin é a
/// > propriedade do ADR 0003: um vencedor e três órfãos exigem um desenho
/// > próprio para decidir qual fica. Fica registrado como o próximo passo, e não
/// > como esquecimento.
///
/// Este é o próximo passo, e o desenho próprio existe: a limpeza de pin dos
/// perdedores roda depois do vencedor e pula a chave dele. Ver o ADR 0037, e o
/// bloco que faz isso em [`Enlace::tentar_entre`].
///
/// # Por que 250 ms
///
/// É o número do RFC 8305. A medição da pendência nº 26 o justifica:
/// com quatro candidatos o último começa em 750 ms, e o bom respondeu em 358 ms
/// depois disso — contra os 9,6 s que a série cobrava. Encurtar para 150 ms
/// ganharia ~300 ms e poria mais apertos de mão simultâneos numa rede lenta,
/// onde vários teriam fechado sozinhos. Ver o ADR 0037.
const DEFASAGEM_ENTRE_CANDIDATOS: Duration = Duration::from_millis(250);

/// O que uma corrida produziu.
///
/// As falhas vêm junto com o vencedor de propósito: quem chama precisa delas
/// para a limpeza de pin órfão dos perdedores, e precisa saber **quem** venceu
/// para pular a chave dele. Ver o ADR 0037.
#[derive(Debug)]
struct Corrida<T> {
    /// Quem fechou primeiro, e a posição dele na lista que foi corrida.
    vencedor: Option<(usize, T)>,
    /// Quem não fechou, na ordem em que desistiram.
    falhas: Vec<(usize, ConnectError)>,
}

/// Corre `quantos` tentativas, disparando uma a cada `defasagem`, e fica com a
/// primeira que fechar.
///
/// É o RFC 8305 — «Happy Eyeballs» — e a razão de existir está medida na
/// pendência nº 26: em série, três candidatos sem chance custam 9,6 s antes de o
/// quarto sequer ser tentado.
///
/// # Por que genérica sobre `T`
///
/// Para que a defasagem e o «primeiro vence» tenham teste. Um teste que
/// provasse isto com sockets de verdade dependeria de rede, e o que precisa ser
/// provado aqui não é a rede: é que o segundo candidato **começa** sem esperar o
/// primeiro terminar, e que o rápido vence mesmo estando por último na lista.
///
/// # Por que `JoinSet` e não `futures::FuturesUnordered`
///
/// Para não acrescentar dependência. O `tokio` já entra com `features =
/// ["full"]`, e este repositório documenta em `Cargo.toml` o que cada
/// dependência arrasta — uma a mais para isto seria custo sem necessidade.
///
/// # Cancelamento
///
/// As tentativas perdedoras são derrubadas com o `JoinSet` ao fim desta função.
/// Elas não continuam escrevendo em lugar nenhum, e o que uma delas possa ter
/// escrito em disco — um pin de TLS — é assunto de quem chama, que sabe qual
/// chave o vencedor usou.
async fn correr<T, F, Fut>(quantos: usize, defasagem: Duration, tentar: F) -> Corrida<T>
where
    T: Send + 'static,
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = Result<T, ConnectError>> + Send + 'static,
{
    let mut corredores = tokio::task::JoinSet::new();
    let mut falhas: Vec<(usize, ConnectError)> = Vec::new();
    let mut proximo = 0_usize;

    loop {
        // Dispara o próximo, se ainda houver. O primeiro sai sem espera nenhuma.
        if proximo < quantos {
            let posicao = proximo;
            let futuro = tentar(posicao);
            corredores.spawn(async move { (posicao, futuro.await) });
            proximo += 1;
        }

        if corredores.is_empty() {
            break;
        }

        // Enquanto os que já estão no ar correm, o relógio da defasagem anda. Se
        // alguém fechar antes dela, a corrida acaba ali — que é o caso comum, e
        // é o motivo de isto existir.
        let terminou = if proximo < quantos {
            tokio::select! {
                terminou = corredores.join_next() => terminou,
                () = tokio::time::sleep(defasagem) => continue,
            }
        } else {
            corredores.join_next().await
        };

        match terminou {
            Some(Ok((posicao, Ok(pronto)))) => {
                // O `JoinSet` derruba o resto ao ser recolhido no fim desta
                // função.
                return Corrida {
                    vencedor: Some((posicao, pronto)),
                    falhas,
                };
            }
            Some(Ok((posicao, Err(erro)))) => falhas.push((posicao, erro)),
            // Uma tentativa que entrou em pânico ou foi cancelada. Não é
            // vencedora e não tem erro próprio a contar.
            Some(Err(_)) => {}
            None => {
                if proximo >= quantos {
                    break;
                }
            }
        }
    }

    Corrida {
        vencedor: None,
        falhas,
    }
}

impl Enlace {
    /// Conecta no primeiro endereço que atender, tentando um de cada vez.
    ///
    /// Um convite pode trazer vários endereços do mesmo server — ADR 0006 — e
    /// eles não são intercambiáveis: o da rede de casa não é alcançável de
    /// fora, e o público que o roteador abriu costuma não voltar para dentro,
    /// porque a maioria dos roteadores domésticos não faz *hairpin*.
    ///
    /// # Em série, e não em corrida
    ///
    /// Uma corrida abriria vários apertos de mão contra o mesmo servidor para
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
    /// O de quem **respondeu**, se algum respondeu: "a chave deste servidor
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
        Self::entre(destinos, bilhete, chave, pins, None).await
    }

    /// O mesmo, contando cada candidato a quem observa.
    ///
    /// A porta por onde [`crate::chegada::Chegada`] entra. Uma [`Tentativa`] sai
    /// por candidato, no instante em que a tentativa dele começa e com o aviso
    /// já decidido — que é a informação de que o caminho da tela é feito, e a
    /// única que não se pode ler do endereço.
    ///
    /// Um canal e não um retorno: as tentativas acontecem enquanto esta função
    /// corre, e quem desenha quer saber delas **durante**, não no fim. O canal é
    /// ilimitado porque a quantidade é a do convite — quatro endereços, no
    /// máximo — e um `send` bloqueante aqui poria a tela no caminho da conexão.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar_entre`].
    pub async fn conectar_entre_observado(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: mpsc::UnboundedSender<Tentativa>,
    ) -> Result<Self, ConnectError> {
        Self::entre(destinos, bilhete, chave, pins, Some(olhos)).await
    }

    /// A preparação que os dois compartilham, e o laço.
    async fn entre(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: Option<mpsc::UnboundedSender<Tentativa>>,
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
        Self::tentar_entre(
            destinos,
            batida.as_ref(),
            bilhete,
            chave,
            pins,
            olhos.as_ref(),
        )
        .await
    }

    /// O laço de tentativas, com ou sem furo de NAT.
    async fn tentar_entre(
        destinos: Vec<Destino>,
        batida: Option<&crate::encontro::Batida>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: Option<&mpsc::UnboundedSender<Tentativa>>,
    ) -> Result<Self, ConnectError> {
        // Uma cópia por tentativa, e o original vivo até o fim: um `Endpoint`
        // fecha o socket dele ao ser recolhido, e sem o original a porta que o
        // anfitrião furou voltaria para o sistema no meio do caminho.
        // **Um `Endpoint`, e não um por tentativa.**
        //
        // A razão de o socket ser um só continua a mesma — o NAT mapeia por porta
        // interna, e o furo que o anfitrião abriu vale para aquela porta. O que
        // mudou é a leitura da restrição: ela nunca foi «uma conexão por
        // socket», e sim **um leitor por socket**. Dois `Endpoint` sobre cópias
        // do mesmo descritor dividem uma fila de recepção e roubam pacote um do
        // outro; um `Endpoint` só dirige quantas conexões se queira,
        // demultiplexando por connection ID. Ver o ADR 0037.
        let endpoint = crate::client::local_endpoint(
            batida.and_then(crate::encontro::Batida::emprestar_socket),
        )?;
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
            contar(
                olhos,
                Tentativa {
                    candidato: 0,
                    onde: primeiro.servidor,
                    avisou: repeticao.is_some(),
                },
            );
            let resultado = Self::conectar_por(&endpoint, bilhete, primeiro, chave, pins).await;
            if let Some(repeticao) = repeticao {
                repeticao.abort();
            }
            return resultado;
        }

        let mut primeira_falha: Option<ConnectError> = None;
        let mut respondeu: Option<ConnectError> = None;

        // Duas voltas: a primeira com pouca paciência para todo mundo, a
        // segunda com a paciência inteira. Ver `PRAZO_DA_PRIMEIRA_VOLTA`.
        //
        // A lista é clonada porque ela é percorrida duas vezes; `Destino` é
        // barato — endereços e uma impressão digital.
        let todos: Vec<Destino> = std::iter::once(primeiro).chain(candidatos).collect();

        // A corrida do RFC 8305, no lugar das duas voltas em série.
        //
        // As duas voltas existiam para dar pouca paciência a todo mundo antes de
        // dar a paciência inteira a quem merecia. A corrida torna a primeira
        // metade desnecessária: ninguém espera o prazo de ninguém, então não há
        // por que encurtá-lo. `PRAZO_DA_PRIMEIRA_VOLTA` fica sem uso aqui e
        // continua valendo no caminho de candidato único, logo acima.
        //
        // O que **não** muda: `avisar_pelo_candidato` decide sozinho quem precisa
        // de furo, por `e_publico`, e o aviso continua saindo colado ao aperto de
        // mão que ele acompanha — agora escalonado junto com ele. Ver o ADR 0037.
        let chaves: Vec<(String, Option<String>)> = todos
            .iter()
            .map(|destino| {
                let chave = destino.chave_do_pin.clone();
                let antes = pins.pinned(&chave);
                (chave, antes)
            })
            .collect();

        let corredores = Arc::new(todos.clone());
        let quantos = corredores.len();
        let corrida = correr(quantos, DEFASAGEM_ENTRE_CANDIDATOS, |posicao| {
            let corredores = Arc::clone(&corredores);
            let batida = batida.cloned();
            let bilhete = bilhete.clone();
            let chave = chave.clone();
            let pins = Arc::clone(&pins);
            let endpoint = endpoint.clone();
            let olhos = olhos.cloned();
            async move {
                let Some(destino) = corredores.get(posicao).cloned() else {
                    return Err(ConnectError::Unreachable);
                };
                let onde = destino.servidor;

                // O aviso sai **agora**, para este candidato, e o aperto de mão
                // sai logo atrás dele. O furo do outro lado dura menos de um
                // segundo, e a única forma de o `Initial` caber dentro dele é os
                // dois saírem juntos — que continua valendo com a corrida,
                // porque cada corredor leva o seu.
                let repeticao = avisar_pelo_candidato(batida.as_ref(), onde).await;
                contar(
                    olhos.as_ref(),
                    Tentativa {
                        candidato: u8::try_from(posicao).unwrap_or(u8::MAX),
                        onde,
                        avisou: repeticao.is_some(),
                    },
                );

                // Um candidato privado de outra casa não devolve ICMP nenhum:
                // ele queima o prazo inteiro sem nunca ter tido chance. Aqui o
                // prazo curto não economiza tempo de parede — a corrida já faz
                // isso — e sim **solta** o corredor, em vez de deixá-lo
                // pendurado enquanto os outros terminam.
                let prazo = if e_de_outra_casa(onde) {
                    PRAZO_DE_CANDIDATO_DISTANTE
                } else {
                    PRAZO_POR_CANDIDATO
                };

                let tentativa = Self::conectar_por(&endpoint, bilhete, destino, chave, pins);
                let resultado = match tokio::time::timeout(prazo, tentativa).await {
                    Ok(resultado) => resultado,
                    // `SemResposta` e não `HandshakeTimeout`: este prazo é o do
                    // **candidato inteiro**, e queimá-lo é não ter recebido nada
                    // de volta. O aperto de mão tem um prazo próprio, dentro de
                    // `Client::connect`, e é ele quem sabe dizer «o servidor
                    // recebeu e demorou» — porque só ele roda depois de haver
                    // conexão.
                    //
                    // Enquanto os dois casos saíam por aqui, a separação escrita
                    // em `ConnectError::SemResposta` não chegava à tela uma vez
                    // sequer: quatro segundos de silêncio na LAN eram anunciados
                    // como problema de sincronização, e a frase mandava conferir
                    // versão e protocolo — as duas coisas que estavam certas.
                    Err(_) => Err(ConnectError::SemResposta),
                };
                // A repetição para quando o candidato termina, dando certo ou
                // não: avisar sobre um candidato que já falhou gastaria furo da
                // janela do anfitrião por um caminho que ninguém vai tentar de
                // novo.
                if let Some(repeticao) = repeticao {
                    repeticao.abort();
                }
                resultado
            }
        })
        .await;

        // A limpeza de pin órfão dos perdedores, **depois** do vencedor e
        // pulando a chave dele.
        //
        // `desfazer_pin_orfao` promete «só apaga o que este aperto escreveu», e
        // isso é exato em série e falso aqui: dois candidatos podem compartilhar
        // `chave_do_pin` — ela é `host:porta` do nome do convite, e alternativos
        // do mesmo nome colidem. Sem esta condição, limpar um perdedor
        // encontraria `fixado_antes == None` e `pinned() == Some`, e apagaria o
        // pin que o vencedor acabou de escrever: a confiança de primeiro contato
        // do ADR 0003 desfeita em silêncio.
        let chave_do_vencedor = corrida
            .vencedor
            .as_ref()
            .and_then(|(posicao, _)| chaves.get(*posicao))
            .map(|(chave, _)| chave.clone());
        for (posicao, _) in &corrida.falhas {
            let Some((chave_perdida, antes)) = chaves.get(*posicao) else {
                continue;
            };
            if chave_do_vencedor.as_deref() == Some(chave_perdida.as_str()) {
                continue;
            }
            desfazer_pin_orfao(pins.as_ref(), chave_perdida, antes.as_deref());
        }

        // **Quem ganhou, dito por extenso.** O log tinha uma linha por candidato
        // que falhou e nenhuma para o que deu certo, e a diferença custou uma
        // noite: com quatro segundos de silêncio na LAN e uma entrada pelo
        // endereço público no mesmo instante, não havia como saber, lendo o
        // rastro, por qual dos dois a conversa tinha subido — só dava para
        // inferir pelos milissegundos, e a inferência errou.
        //
        // A posição é lida daqui e não da trilha porque a trilha guarda a ordem
        // em que as tentativas **começaram**, e elas correm em paralelo: a
        // última a começar não é a que venceu. Aqui a corrida já terminou e
        // sabe o nome de quem chegou.
        if let Some((posicao, enlace)) = corrida.vencedor {
            let onde = todos.get(posicao).map(|destino| destino.servidor);
            tracing::info!(?onde, "este é o endereço que deu");
            return Ok(enlace);
        }

        for (posicao, falha) in corrida.falhas {
            let onde = todos.get(posicao).map(|destino| destino.servidor);
            tracing::info!(?onde, erro = %falha, "este endereço do convite não deu");
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
        let endpoint = crate::client::local_endpoint(None)?;
        Self::conectar_por(&endpoint, None, destino, chave, pins).await
    }

    /// O mesmo, pelo socket que já furou o NAT. Degrau 4 do ADR 0022.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar`].
    async fn conectar_por(
        endpoint: &quinn::Endpoint,
        bilhete: Option<seele_proto::uri::Bilhete>,
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        // Antes de o TLS ter chance de escrever qualquer coisa. Ver
        // [`desfazer_pin_orfao`].
        let fixado_antes = pins.pinned(&destino.chave_do_pin);

        let resultado = Client::connect_por(
            endpoint,
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
                // ~85 ms contra um servidor de verdade, com e sem esta linha —, mas
                // pelo caminho longo: `Client::connect` deixa uma tarefa de
                // leitura dona do `RecvStream`, e ela só descobre que ninguém
                // escuta quando o servidor manda o quadro seguinte. Contra um
                // servidor que fala (telemetria a cada segundo) isso é rápido;
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
                // `CONNECTION_CLOSE` e é o que fica no log do servidor. Fechar
                // como `ejected` faria uma recusa de convite parecer um pessoa
                // saindo, que é o único jeito de esconder a recusa de quem tem
                // o log na mão.
                cliente.close(crate::client::INVITE_REFUSED);
                return Err(erro);
            }
        };

        let sessao = cliente.session().clone();
        let media = cliente.media();
        let rtt = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let caminho_medido = Arc::new(std::sync::atomic::AtomicU32::new(0));

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
            voice_room: None,
            linha: None,
            muted: false,
            isolamento: false,
            avisos: avisos_tx,
            rtt: Arc::clone(&rtt),
            caminho_medido: Arc::clone(&caminho_medido),
            tela_pedida: None,
            tela_viva: None,
            faixa: FAIXA_INICIAL,
            caminho_de_quem_hospeda_bps: None,
            espectadores: 0,
            caminho: crate::caminho::Sonda::nova(),
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
            caminho_medido,
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

    /// Semeia a sonda com o caminho medido da última vez com este servidor.
    ///
    /// Sem isto, toda transmissão parte de
    /// [`crate::tela::CAMINHO_DA_PROVA_BPS`] — 2 Mbps supostos — e gasta os
    /// primeiros segundos reaprendendo um cano que já mediu. Medido em campo
    /// numa LAN: doze segundos de 540p até a escada reencontrar 1080p.
    ///
    /// Zero é «não sei», e não faz nada: é o que a lista de conhecidos guarda
    /// para um servidor onde ninguém compartilhou tela ainda.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn lembrar_o_caminho(&self, bps: u32) -> Result<(), Fechado> {
        if bps == 0 {
            return Ok(());
        }
        self.mandar(Comando::LembrarCaminho(bps)).await
    }

    /// O caminho de subida que a sonda mediu, em bits por segundo.
    ///
    /// Zero enquanto ninguém compartilhou tela: a sonda mede **enquanto a tela
    /// transmite**, porque é a tela que enche o cano. Quem grava a lista de
    /// conhecidos deve tratar zero como «não mediu» e deixar o valor antigo.
    #[must_use]
    pub fn caminho_medido(&self) -> u32 {
        self.caminho_medido
            .load(std::sync::atomic::Ordering::Relaxed)
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

    /// Entra num sala de voz. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn inserir_plug(&self, voice_room: VoiceRoomId) -> Result<(), Fechado> {
        self.mandar(Comando::InserirPlug(voice_room)).await
    }

    /// Sai da sala de voz.
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
    pub async fn abrir_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
        self.mandar(Comando::AbrirLinha(linha)).await
    }

    /// Diz alguma coisa.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn dizer(
        &self,
        linha: ChannelId,
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
    pub async fn historico(&self, linha: ChannelId, limite: u16) -> Result<(), Fechado> {
        self.mandar(Comando::Historico { linha, limite }).await
    }

    /// Liga ou desliga o mudo. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn muted(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Muted(ligado)).await
    }

    /// Liga ou desliga o isolamento total. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn isolamento(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Isolamento(ligado)).await
    }

    /// Pede ao servidor que faça uma sala de voz.
    ///
    /// Pede, e só. Nada aqui confere se este pessoa pode: a `specs/08-seguranca.md`
    /// põe a decisão no servidor, e um core que recusasse por conta própria
    /// seria uma segunda autoridade para manter de acordo com a primeira. A
    /// resposta chega como aviso — `VoiceRoomCreated` se aconteceu, `Alert` com
    /// `PermissionDenied` se não.
    ///
    /// **Não** é refeito ao reconectar, ao contrário da sala de voz e da Linha
    /// abertos. Aqueles são onde a pessoa estava, e voltar sem eles é voltar
    /// para outro lugar; este é uma coisa que se faz uma vez. Repetido depois de
    /// uma queda, ele criaria uma sala minutos mais tarde, do nada, e mais uma
    /// se a pessoa já tivesse pedido de novo à mão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_voice_room(
        &self,
        nome: String,
        limite: u16,
        linha: Option<ChannelId>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::CriarVoiceRoom {
            nome,
            limite,
            linha,
        })
        .await
    }

    /// Pede ao servidor que faça uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_linha(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::CriarLinha { nome }).await
    }

    /// Pede ao servidor que renomeie uma sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_voice_room(
        &self,
        voice_room: VoiceRoomId,
        nome: String,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearVoiceRoom { voice_room, nome })
            .await
    }

    /// Pede ao servidor que renomeie uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_linha(&self, linha: ChannelId, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearLinha { linha, nome }).await
    }

    /// Pede ao servidor que troque o próprio nome.
    ///
    /// Pede, e só, como os verbos de sala e pelo mesmo motivo: quem decide é o
    /// servidor, que quer `AdministerServer` para isto e responde `Alert` com
    /// `PermissionDenied` quando nega. Quando aceita, o nome novo volta para
    /// **todo mundo** como `ServerRenamed`, inclusive para quem pediu — é o que
    /// impede a tela de quem renomeou de ser a única com o nome certo.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e de moderação:
    /// dar nome é coisa que se faz uma vez, e repetido depois de cinco minutos
    /// de bateria desfaria o nome que outra pessoa pôs no meio.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_server(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearServer { nome }).await
    }

    /// Pede ao servidor que troque a própria imagem, ou que fique sem nenhuma.
    ///
    /// `None` tira a imagem, e é um verbo e não uma ausência: quem pôs tem que
    /// poder tirar.
    ///
    /// **Não** é refeito ao reconectar, pelo mesmo motivo do nome.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_icone(&self, icone: Option<Vec<u8>>) -> Result<(), Fechado> {
        self.mandar(Comando::IconeDoServer { icone }).await
    }

    /// Põe ou tira **a sua** imagem de perfil.
    ///
    /// Diferente do ícone do servidor num ponto: aquele exige permissão, este
    /// não. O servidor grava na linha de quem pediu e em nenhuma outra, e uma
    /// permissão aqui seria alguém podendo escolher a cara dos outros.
    ///
    /// **Não** é refeita ao reconectar, pelo mesmo motivo do nome e do ícone:
    /// ela está gravada no servidor, e reenviá-la a cada volta seria escrever
    /// de novo o que já está lá.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_minha_imagem(&self, icone: Option<Vec<u8>>) -> Result<(), Fechado> {
        self.mandar(Comando::MinhaImagem { icone }).await
    }

    /// Troca **o seu** apelido.
    ///
    /// **Não** é refeito ao reconectar: o nome fica gravado no servidor, e a
    /// reconexão volta a apresentar quem já se é. Reenviá-lo seria escrever de
    /// novo o que já está lá — e, num servidor onde outra pessoa tenha tomado
    /// o nome nesse meio-tempo, seria uma recusa a cada volta.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_meu_apelido(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::MeuApelido { nome }).await
    }

    /// Pede ao servidor que acabe com a sessão de alguém.
    ///
    /// Pede, e só — como os verbos de sala, e pela mesma razão: a
    /// `specs/08-seguranca.md` põe a decisão no servidor, e um core que
    /// recusasse por conta própria seria uma segunda autoridade para manter de
    /// acordo com a primeira. Esconder o botão é conveniência; quem nega é o
    /// servidor, e ele responde com `Alert` de `PermissionDenied` quando nega.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e pelo mesmo
    /// motivo: expulsar é coisa que se faz uma vez, e repetida minutos depois
    /// derrubaria alguém que já tinha voltado.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn expulsar(&self, pessoa: PersonId) -> Result<(), Fechado> {
        self.mandar(Comando::Expulsar { pessoa }).await
    }

    /// Pede ao servidor que impeça alguém de voltar.
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
        pessoa: PersonId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Banir {
            pessoa,
            motivo,
            expira_em,
        })
        .await
    }

    /// Pede ao servidor que tire uma mensagem da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn remover_mensagem(&self, mensagem: MessageId) -> Result<(), Fechado> {
        self.mandar(Comando::RemoverMensagem { mensagem }).await
    }

    /// Pede ao servidor que mova alguém para uma sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn mover_pessoa(
        &self,
        pessoa: PersonId,
        voice_room: VoiceRoomId,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::MoverPersono { pessoa, voice_room })
            .await
    }

    /// Pede ao servidor que destrua uma sala de voz.
    ///
    /// Pede, e só, como todo verbo daqui. Quem recusa é o servidor: sem
    /// `administrar_server` volta `Alert` com `PermissionDenied`, e no único
    /// sala de voz que resta volta `Alert` com `LastVoiceRoom`, que é frase diferente.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e de moderação e
    /// pelo mesmo motivo, com uma ponta a mais: repetido minutos depois, este
    /// destruiria a sala que alguém fez no lugar da que sumiu.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_voice_room(&self, voice_room: VoiceRoomId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarVoiceRoom { voice_room }).await
    }

    /// Pede ao servidor que destrua uma Linha, e tudo que foi escrito nela.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarLinha { linha }).await
    }

    /// Pergunta quanto custaria destruir uma Linha. Não destrói nada.
    ///
    /// A resposta chega como `ChannelWeighed` no fluxo de avisos, como toda
    /// resposta deste enlace. É o que enche a caixa de confirmação com número
    /// contado no banco — uma casca segura uma página de histórico e chutaria
    /// para baixo por todo o passado da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn pesar_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
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

    /// Começa a compartilhar a tela escolhida, com os tetos escolhidos.
    ///
    /// **Não volta com a transmissão pronta**, e não teria como: o que sai
    /// daqui é um `StartScreenShare`, e o nome da transmissão — o
    /// [`ScreenId`] — só chega depois, num `ScreenShareStarted` do fluxo de
    /// eventos. É por isso que este verbo não devolve nada além de «foi
    /// mandado»: entre o botão e o primeiro quadro há uma volta de rede, e
    /// prometer aqui seria prometer no lugar do servidor.
    ///
    /// **Não é refeito depois de uma queda**, ao contrário da sala de voz e da Linha.
    /// Ver o comentário de `Motor::lembrar`: uma transmissão que voltasse
    /// sozinha cinco minutos depois poria a tela de alguém no ar sem que
    /// ninguém tivesse apertado nada.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn compartilhar_tela(
        &self,
        fonte: PedidoDeTela,
        limites: LimitesDeTela,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::CompartilharTela {
            fonte: Box::new(fonte),
            limites,
        })
        .await
    }

    /// Troca os tetos de uma transmissão em curso. Ver
    /// [`Comando::AjustarLimitesDaTela`].
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn ajustar_limites_da_tela(&self, limites: LimitesDeTela) -> Result<(), Fechado> {
        self.mandar(Comando::AjustarLimitesDaTela { limites }).await
    }

    /// Para de compartilhar.
    ///
    /// Idempotente: parar sem estar compartilhando manda o verbo assim mesmo, e
    /// o servidor o ignora. A alternativa — conferir aqui — poria uma segunda
    /// autoridade sobre quem está transmitindo, e ela discordaria da primeira no
    /// primeiro atraso de rede.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn parar_de_compartilhar(&self) -> Result<(), Fechado> {
        self.mandar(Comando::PararDeCompartilhar).await
    }

    /// Pede um quadro-chave a quem está compartilhando.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a conexão já foi embora.
    pub async fn pedir_quadro_chave(&self, tela: ScreenId) -> Result<(), Fechado> {
        self.mandar(Comando::PedirQuadroChave { tela }).await
    }

    /// Passa a receber a imagem desta transmissão, ou para de receber.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a conexão já foi embora.
    pub async fn assistir(&self, tela: ScreenId, quero: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Assistir { tela, quero }).await
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
    voice_room: Option<VoiceRoomId>,
    linha: Option<ChannelId>,
    muted: bool,
    isolamento: bool,
    avisos: mpsc::UnboundedSender<Aviso>,
    rtt: Arc<std::sync::atomic::AtomicU64>,
    caminho_medido: Arc<std::sync::atomic::AtomicU32>,
    /// A escolha da pessoa, esperando a transmissão ganhar nome.
    ///
    /// **A bomba não pode nascer no comando**, e é a forma da coisa e não uma
    /// preguiça: [`Client::start_screen_share`] manda um pedido, e o
    /// [`ScreenId`] — que é o que `escoar` escreve no cabeçalho de abertura —
    /// só chega depois, num `ScreenShareStarted` do laço de mensagens. Entre os
    /// dois instantes, isto é a transmissão inteira.
    tela_pedida: Option<(Box<PedidoDeTela>, LimitesDeTela)>,
    /// A bomba viva desta pessoa, quando há uma.
    tela_viva: Option<TelaViva>,
    /// A faixa do sinal da **própria** voz, lida do que o servidor devolve.
    ///
    /// Era uma constante `Nominal`, e a dívida estava escrita no lugar dela: o
    /// teto respondia ao `HostUplink` e ao número de espectadores e **não** ao
    /// sinal da voz piorando — numa sala onde a voz começava a doer, a tela não
    /// cedia sozinha. Faltava metade da regra de aceite do §3.2.
    ///
    /// O que fechava a dívida já vinha pelo fio e ninguém guardava: o servidor
    /// calcula a taxa de cada pessoa e a devolve em `PersonState`, uma vez por
    /// segundo. O `Signal` da casca é a mesma conta feita de novo para
    /// desenhar — não é fonte, é cópia —, e é por isso que isto não precisou de
    /// comando novo nem de a casca falar com o núcleo.
    faixa: SignalBand,
    /// A subida de quem hospeda, como o `HostUplink` a mediu. §5.1.
    ///
    /// `None` é «não medido», e é assim que o zero do protocolo chega aqui: a
    /// perna fica no cano das provas em vez de zerar o teto.
    caminho_de_quem_hospeda_bps: Option<u32>,
    /// Quantos estão assistindo, como o `ScreenViewers` contou. O N do §5.1.
    espectadores: u32,
    /// A subida **desta** máquina, medida enquanto a tela enche.
    ///
    /// Era a perna que faltava, e a falta estava escrita no lugar dela: o teto
    /// respondia ao `HostUplink`, ao número de espectadores e à faixa da voz, e
    /// para o caminho de quem compartilha usava a suposição de
    /// `crate::tela::CAMINHO_DA_PROVA_BPS` — 2 Mbps do cano em que os spikes
    /// rodaram. Quem tinha menos que isso descobria pela voz doendo; quem tinha
    /// mais nunca descobria, e olhava 720p numa casa onde cabia 1080p.
    ///
    /// Quem mede é a [`crate::caminho::Sonda`], e quem a alimenta é este motor,
    /// na tica que ele já tem — ver [`Motor::medir_o_caminho`]. Era a pergunta
    /// 2 do §8.
    caminho: crate::caminho::Sonda,
}

/// Uma transmissão desta pessoa que está no ar.
///
/// Não guarda a tarefa que escoa, e a ausência é deliberada: ela acaba sozinha
/// quando a bomba manda o [`EventoDaBomba::Fim`](crate::EventoDaBomba::Fim) —
/// **e é assim que o fluxo fecha direito**. Abortá-la seria cortar no meio de um
/// quadro e deixar quem assiste esperando o resto dele.
#[derive(Debug)]
struct TelaViva {
    /// O nome que o servidor deu a esta transmissão.
    tela: ScreenId,
    /// A alça da thread do codificador. Largá-la para a bomba.
    bomba: crate::bomba::Bomba,
    /// O que a pessoa escolheu, guardado porque o teto é recalculado a cada
    /// `ScreenViewers` e a escolha é uma das pernas dele (§5).
    limites: LimitesDeTela,
}

/// Em que faixa o sinal da voz começa, antes de o servidor dizer a primeira.
///
/// Otimista de propósito, e o motivo é qual erro custa mais: começar em
/// `Critical` pararia a tela de alguém cuja voz está ótima, por causa de um
/// dado que ainda não chegou. Começar em `Nominal` deixa a tela abrir e ceder
/// no primeiro `PersonState`, que vem uma vez por segundo.
const FAIXA_INICIAL: SignalBand = SignalBand::Nominal;

/// De quanto em quanto tempo a bateria é consultada.
///
/// Menor que o intervalo de ping e muito menor que o menor backoff, para que
/// nem o ping nem uma tentativa de reconexão fiquem esperando a tica seguinte.
const TICA: Duration = Duration::from_millis(200);

/// A faixa nova, quando este `PersonState` for sobre esta pessoa e mudar de faixa.
///
/// Separada do `Motor` porque é a decisão inteira, e uma decisão sobre valores
/// não precisa de conexão QUIC para ser conferida — o mesmo argumento que
/// `alcance::Alcance::decidir` usa no servidor.
///
/// `None` em três casos, e os três importam: a mensagem é sobre outra pessoa; a
/// sessão ainda não existe, e então ela não é sobre ninguém que este `Motor`
/// conheça; ou a faixa não mudou, e refazer o teto a cada chegada acordaria a
/// thread do codificador uma vez por segundo para lhe dizer o que ela já sabe.
fn faixa_nova(
    atual: SignalBand,
    estado: &seele_proto::control::PersonState,
    eu: Option<PersonId>,
) -> Option<SignalBand> {
    if eu != Some(estado.person) {
        return None;
    }
    let nova = SignalBand::of(estado.signal);
    (nova != atual).then_some(nova)
}

impl Motor {
    async fn rodar(mut self, mut comandos: mpsc::Receiver<Comando>) {
        let mut tica = tokio::time::interval(TICA);
        tica.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Tirados antes do `select!`, e é o que faz o braço da tela
            // compilar: o braço de cima já pega `self.cliente` emprestado
            // mutável, e um segundo empréstimo de `self` no mesmo `select!` —
            // mesmo imutável — é recusado. Estes dois punhos não emprestam
            // nada: um `Arc` e um remetente, os dois baratos de clonar.
            let fila_de_telas = self
                .cliente
                .as_ref()
                .map(crate::client::Client::fila_de_telas);
            let avisos_da_tela = self.avisos.clone();

            // Só há o que ler quando há conexão. Sem ela, a espera é o relógio.
            let houve_evento = match self.cliente.as_mut() {
                Some(cliente) => tokio::select! {
                    evento = cliente.next_event() => Some(evento),
                    // Uma tela alheia chegando. O roteador de `Client::connect`
                    // já separou este fluxo dos anexos; aqui ele vira quadros.
                    Some(fluxo) = espera_da_fila(&fila_de_telas) => {
                        escoar_tela_alheia(avisos_da_tela.clone(), fluxo);
                        None
                    }
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
                            // Guardado, não perdido: entrar num sala de voz durante a
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
                        // sala de voz de onde foi tirado: o motor refaz o último sala de voz
                        // que este cliente pediu, e ele não pediu este.
                        if let ServerMessage::MovedToVoiceRoom { voice_room } = mensagem {
                            self.voice_room = Some(voice_room);
                        }
                        if matches!(mensagem, ServerMessage::Pong { .. }) {
                            self.bateria.on_pong();
                            if let Some(medido) = self.cliente.as_ref().and_then(Client::rtt) {
                                let micros = u64::try_from(medido.as_micros()).unwrap_or(u64::MAX);
                                self.rtt
                                    .store(micros.max(1), std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        // Antes de o aviso sair, porque a tela é a única coisa
                        // desta casa que **age** sobre uma mensagem em vez de
                        // repassá-la: é aqui que a transmissão ganha nome e a
                        // bomba nasce.
                        self.a_tela_ouviu(&mensagem);
                        let _ = self.avisos.send(Aviso::Mensagem(Box::new(mensagem)));
                    }
                    // O fluxo caiu. Não é o fim da sessão: é o começo da
                    // bateria.
                    Err(erro) => {
                        // **`warn` e não `debug`.** Esta linha é a única que diz
                        // por que uma sessão acabou do lado de quem estava
                        // dentro, e ela estava abaixo do filtro: o log do
                        // cliente ficava **mudo** enquanto a conexão caía.
                        //
                        // Custou um diagnóstico inteiro. Um relato de campo —
                        // «minha sessão some para o host e o compartilhamento
                        // para sozinho, mas no Mac eu ainda estou na call» —
                        // teve de ser cruzado com o log do servidor para se
                        // saber sequer que a conexão havia caído, e o motivo
                        // continuou sem aparecer em lugar nenhum.
                        //
                        // Não é ruidosa: acontece uma vez, e o que vem depois é
                        // a bateria de reconexão.
                        tracing::warn!(%erro, "o enlace caiu");
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
        // Antes da bateria, e não depois: o passo pode encerrar a sessão, e uma
        // leitura depois disso seria contra um cliente que já foi embora.
        self.medir_o_caminho();
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
        // Os contadores do `quinn` morrem com a conexão, e a janela aberta
        // contra eles daria uma medida absurda na conexão seguinte. O que se
        // aprendeu sobre a casa desta pessoa fica: quem cai e volta em cinco
        // segundos volta para o mesmo cano.
        self.caminho.esquecer_a_conexao();
        // **Uma bomba que sobrevive ao enlace é uma thread codificando para uma
        // conexão morta**, e ela não pararia sozinha: a captura continua
        // entregando quadros e `escoar` só descobre a queda no próximo fluxo
        // que não abre. Além do custo, é o indicador de gravação do macOS aceso
        // sobre uma transmissão que ninguém está recebendo.
        //
        // O pedido morre junto, e é a mesma decisão de [`Self::lembrar`]: uma
        // transmissão que voltasse sozinha depois de cinco minutos de bateria
        // poria a tela de alguém no ar sem que ninguém tivesse apertado nada.
        self.tela_pedida = None;
        self.parar_a_tela();
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
        // O Endpoint desta reconexão. `match` e não `?` porque esta função não
        // devolve `Result`: uma falha aqui é a mesma coisa que uma tentativa que
        // não deu, e segue pelo mesmo caminho que todas as outras.
        let resultado = match crate::client::local_endpoint(furo) {
            Ok(endpoint) => {
                Client::connect_por(
                    &endpoint,
                    self.destino.servidor,
                    &self.destino.nome_tls,
                    &self.destino.chave_do_pin,
                    &self.destino.apelido,
                    &self.chave,
                    Arc::clone(&self.pins),
                    self.destino.segredo.as_deref(),
                )
                .await
            }
            Err(erro) => Err(erro),
        };

        // Esta tentativa acabou, dando certo ou não, e o que a repetição
        // avisaria daqui para a frente é sobre uma porta que já foi usada.
        if let Some(repeticao) = repeticao {
            repeticao.abort();
        }

        let agora = self.inicio.elapsed();
        match resultado {
            Ok(mut cliente) => {
                // Restaurar antes de anunciar. Uma casca que recebesse
                // "reconectado" e perguntasse a sala de voz antes de ele existir veria
                // uma sala vazia e acharia que perdeu gente.
                if let Some(voice_room) = self.voice_room {
                    let _ = cliente.insert_plug(voice_room).await;
                }
                if let Some(linha) = self.linha {
                    let _ = cliente.join_channel(linha).await;
                }
                if self.muted {
                    let _ = cliente.set_muted(true).await;
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
            // Nada a mandar ao servidor: é estado desta máquina.
            Comando::LembrarCaminho(_) => Ok(()),
            Comando::InserirPlug(voice_room) => cliente.insert_plug(voice_room).await,
            Comando::EjetarPlug => cliente.eject_plug().await,
            Comando::AbrirLinha(linha) => cliente.join_channel(linha).await,
            Comando::Dizer { linha, corpo, id } => cliente.send_message(linha, &corpo, id).await,
            Comando::Historico { linha, limite } => {
                cliente.fetch_history(linha, None, limite).await
            }
            Comando::Muted(ligado) => cliente.set_muted(ligado).await,
            Comando::Isolamento(ligado) => cliente.set_total_isolation(ligado).await,
            Comando::CriarVoiceRoom {
                nome,
                limite,
                linha,
            } => cliente.create_voice_room(&nome, limite, linha).await,
            Comando::CriarLinha { nome } => cliente.create_channel(&nome).await,
            Comando::RenomearVoiceRoom { voice_room, nome } => {
                cliente.rename_voice_room(voice_room, &nome).await
            }
            Comando::RenomearLinha { linha, nome } => cliente.rename_channel(linha, &nome).await,
            Comando::RenomearServer { nome } => cliente.rename_server(&nome).await,
            Comando::IconeDoServer { icone } => cliente.set_server_icon(icone).await,
            Comando::MinhaImagem { icone } => cliente.set_person_icon(icone).await,
            Comando::MeuApelido { nome } => cliente.set_nickname(nome).await,
            Comando::Expulsar { pessoa } => cliente.kick_person(pessoa).await,
            Comando::Banir {
                pessoa,
                motivo,
                expira_em,
            } => {
                cliente
                    .ban_person(pessoa, motivo.as_deref(), expira_em)
                    .await
            }
            Comando::RemoverMensagem { mensagem } => cliente.remove_message(mensagem).await,
            Comando::MoverPersono { pessoa, voice_room } => {
                cliente.move_person(pessoa, voice_room).await
            }
            Comando::ApagarVoiceRoom { voice_room } => cliente.delete_voice_room(voice_room).await,
            Comando::ApagarLinha { linha } => cliente.delete_channel(linha).await,
            Comando::PesarLinha { linha } => cliente.weigh_channel(linha).await,

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
                        channel: anexo.linha,
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

            // O pedido é guardado **antes** de o verbo sair, e a ordem
            // importa: a resposta do servidor chega pelo mesmo laço que trouxe
            // este comando, e guardar depois abriria uma janela em que o
            // `ScreenShareStarted` chega e não encontra a escolha da pessoa.
            Comando::CompartilharTela { fonte, limites } => {
                self.tela_pedida = Some((fonte, limites));
                cliente.start_screen_share().await
            }

            // Os tetos da pessoa, na transmissão que já existe.
            //
            // Sem verbo para o servidor: os tetos são desta ponta. O que atravessa
            // a rede é o resultado deles — a resolução no cabeçalho do fluxo
            // novo —, e não a escolha em si. `TelaEmCurso::pedido` é lido do
            // que está guardado aqui, e é por isso que ele é escrito antes de a
            // bomba responder: a coluna «pedido» é a escolha, e ela vale desde
            // o aperto, mesmo que o degrau demore um quadro a acompanhar.
            Comando::AjustarLimitesDaTela { limites } => {
                let Some(viva) = self.tela_viva.as_mut() else {
                    // Ninguém está transmitindo. Não é erro: é alguém que
                    // apertou APLICAR na janela um instante depois de a
                    // transmissão cair sozinha.
                    return;
                };
                viva.limites = limites;
                let (resolucao, cadencia) = (limites.resolucao, limites.cadencia);
                viva.bomba.escolha(resolucao, cadencia);
                // E o teto de novo, porque a banda escolhida é uma das pernas
                // dele: sem esta linha, mexer só na banda não mudaria nada.
                self.reconferir_o_teto();
                return;
            }

            // A bomba morre aqui, e não quando o `ScreenShareStopped` voltar:
            // quem apertou parar não deve continuar capturando enquanto uma
            // volta de rede acontece, e o servidor pode nunca responder.
            Comando::PararDeCompartilhar => {
                self.tela_pedida = None;
                // Pelos campos e não por [`Self::parar_a_tela`]: `cliente` é um
                // empréstimo de `self.cliente` que ainda está vivo na linha
                // abaixo, e um método pegaria `self` inteiro.
                self.espectadores = 0;
                self.caminho.esquecer_a_conexao();
                matar(self.tela_viva.take());
                cliente.stop_screen_share().await
            }

            Comando::Assistir { tela, quero } => {
                if quero {
                    cliente.watch_screen(tela).await
                } else {
                    cliente.unwatch_screen(tela).await
                }
            }
            Comando::PedirQuadroChave { tela } => cliente.request_key_frame(tela).await,

            Comando::Sair => return,
        };
        if resultado.is_err() {
            self.cair();
        }
    }

    // ------------------------------------------------------------------ tela

    /// O que uma mensagem do servidor faz com a tela desta pessoa.
    fn a_tela_ouviu(&mut self, mensagem: &ServerMessage) {
        match *mensagem {
            // A faixa da **própria** voz, que é a perna que faltava no teto do
            // §3.2. O servidor calcula a taxa de cada pessoa e a devolve aqui uma
            // vez por segundo; o que faltava era guardar a sua.
            //
            // `SignalBand::of` e não um limiar escrito aqui: a conta de onde
            // começa cada faixa é do `seele-proto`, e duas cópias dela
            // divergiriam no dia em que uma mudasse.
            ServerMessage::PersonState(ref estado) => {
                let eu = self.cliente.as_ref().map(|c| c.session().person);
                if let Some(nova) = faixa_nova(self.faixa, estado, eu) {
                    {
                        self.faixa = nova;
                        // Só quando muda, e só se houver tela: a taxa chega uma
                        // vez por segundo e quase sempre na mesma faixa, e
                        // refazer o teto a cada chegada seria acordar a thread
                        // do codificador sessenta vezes por minuto para lhe
                        // dizer o que ela já sabe.
                        self.reconferir_o_teto();
                    }
                }
            }
            ServerMessage::ScreenShareStarted { person, screen, .. } => {
                self.talvez_ligar_a_bomba(person, screen);
            }
            ServerMessage::ScreenShareStopped { screen, .. } => {
                if self.e_a_minha_tela(screen) {
                    self.parar_a_tela();
                }
            }
            // §3.3: quadro-chave quando quem recebe pede, e nunca periódico.
            // Um de 1080p custa 65 KiB, quatro vezes um quadro comum.
            ServerMessage::KeyFrameRequested { screen, .. } => {
                if let Some(viva) = self.tela_viva.as_ref() {
                    if viva.tela == screen {
                        let _ = viva.bomba.chave();
                    }
                }
            }
            ServerMessage::ScreenViewers { tela, quantos } => {
                if self.e_a_minha_tela(tela) {
                    self.espectadores = quantos;
                    self.reconferir_o_teto();
                }
            }
            // Zero é **ausência de medida** e nunca zero bits por segundo — o
            // contrato está escrito no próprio quadro do protocolo.
            ServerMessage::HostUplink { bps } => {
                self.caminho_de_quem_hospeda_bps = (bps > 0).then_some(bps);
                self.reconferir_o_teto();
            }
            _ => {}
        }
    }
}

/// A espera de uma fila que pode não existir.
///
/// `None` quando não há conexão, e aí este braço do `select!` nunca acorda —
/// que é o certo: sem conexão não chega tela nenhuma. Um braço que devolvesse
/// `None` de imediato giraria o laço a full CPU.
async fn espera_da_fila(fila: &Option<crate::client::FilaDeTelas>) -> Option<quinn::RecvStream> {
    match fila {
        Some(fila) => fila.lock().await.recv().await,
        None => std::future::pending().await,
    }
}

/// Lê uma transmissão alheia até o fim, numa tarefa própria.
///
/// Própria porque ler uma tela dura o que a transmissão durar, e fazer isso no
/// laço principal pararia a voz, a presença e as mensagens de todo mundo
/// enquanto alguém compartilha — que é o oposto do que a §3.2 da spec pede
/// quando diz que a voz nunca cede à tela.
///
/// Livre e não método por causa do `select!` que a chama: o outro braço já tem
/// o `Motor` emprestado mutável, e um método aqui seria um segundo empréstimo.
/// O que ela precisa do motor é o remetente de avisos, que vem por argumento.
///
/// Os quadros saem pelo mesmo canal de avisos que todo o resto, e por isso
/// chegam à casca na ordem em que foram lidos.
fn escoar_tela_alheia(avisos: mpsc::UnboundedSender<Aviso>, fluxo: quinn::RecvStream) {
    {
        tokio::spawn(async move {
            let mut recepcao = match crate::tela::Recepcao::do_fluxo_ja_tipado(fluxo).await {
                Ok(recepcao) => recepcao,
                Err(erro) => {
                    // **`warn!` e não `debug!`, e a casca fica sabendo.**
                    //
                    // Alguém do outro lado abriu um fluxo de tela e este build
                    // não conseguiu ler o cabeçalho dele. Não é ruído: é uma
                    // transmissão que existe e que esta pessoa não vai ver, e a
                    // causa quase sempre é versão diferente dos dois lados.
                    //
                    // Voltar calado era o que produzia a tela preta: sem
                    // `TelaAbriu` não há o que desenhar, e sem `TelaFechou` não
                    // há o que apagar — a casca não sabia que tinha havido nada.
                    tracing::warn!(
                        %erro,
                        "chegou uma transmissão de tela que esta versão não sabe ler"
                    );
                    let _ = avisos.send(Aviso::TelaIlegivel {
                        motivo: erro.to_string(),
                    });
                    return;
                }
            };
            let cabecalho = *recepcao.cabecalho();
            let tela = cabecalho.screen;
            if avisos
                .send(Aviso::TelaAbriu {
                    tela,
                    largura: cabecalho.width,
                    altura: cabecalho.height,
                })
                .is_err()
            {
                return;
            }

            loop {
                match recepcao.proximo_quadro().await {
                    Ok(Some(quadro)) => {
                        // **O som não atravessa a ponte.** Ele vai para a
                        // mistura, aqui em Rust, e nunca para a casca: a janela
                        // não tem o que fazer com um pacote Opus, e mandá-la
                        // decodificar seria dar a ela um trabalho que este lado
                        // já sabe fazer — e que precisa acontecer no mesmo lugar
                        // onde o isolamento total vale.
                        let aviso = if quadro.tipo == crate::tela::TipoDeQuadro::Som {
                            Aviso::TelaSom {
                                tela,
                                bytes: quadro.bytes,
                            }
                        } else {
                            Aviso::TelaQuadro {
                                tela,
                                chave: quadro.chave(),
                                bytes: quadro.bytes,
                            }
                        };
                        if avisos.send(aviso).is_err() {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(erro) => {
                        // Um quadro torto encerra esta transmissão e não a
                        // conexão: o fluxo já perdeu o sincronismo, e continuar
                        // lendo dele é ler lixo. Quem transmite recomeça com um
                        // fluxo novo se quiser.
                        // `warn!`: o fluxo perdeu o sincronismo no meio, e do
                        // lado de quem assiste isso é a imagem congelando. O
                        // `TelaFechou` logo abaixo ao menos apaga o palco, que é
                        // mais do que o caso do cabeçalho tinha.
                        tracing::warn!(%erro, %tela, "a transmissão alheia terminou torta");
                        break;
                    }
                }
            }
            let _ = avisos.send(Aviso::TelaFechou { tela });
        });
    }
}

impl Motor {
    fn e_a_minha_tela(&self, tela: ScreenId) -> bool {
        self.tela_viva
            .as_ref()
            .is_some_and(|viva| viva.tela == tela)
    }

    /// O pedido guardado vira bomba, se este `ScreenShareStarted` for o dele.
    ///
    /// A guarda que mora aqui é uma só, e é a que precisa do [`Client`]: **é
    /// desta pessoa?** O quadro sai do barramento do servidor para a sala de voz inteiro,
    /// e sem ela quem apenas assiste ligaria a captura da própria tela ao ver
    /// outro começar. As outras duas — já há uma viva, e o pedido existe — moram
    /// em [`Self::nascer_a_tela`], porque valem também para quem o chama de um
    /// teste.
    ///
    /// **A `Bomba` não vai para uma bomba a mais quando o servidor reenvia.** Ele
    /// reenvia `ScreenShareStarted` a cada pessoa que entra num sala de voz onde já há
    /// transmissão, e quem transmite recebe o reenvio junto; o pedido já foi
    /// consumido no primeiro, então o segundo não acha nada.
    fn talvez_ligar_a_bomba(&mut self, pessoa: PersonId, tela: ScreenId) {
        let escoadouro = match self.cliente.as_ref() {
            Some(cliente) if cliente.session().person == pessoa => cliente.escoadouro_de_tela(),
            _ => return,
        };

        let avisos = self.avisos.clone();
        self.nascer_a_tela(tela, move |origem, mut eventos| {
            // Numa tarefa própria pelo mesmo motivo de `Comando::Anexar`:
            // escoar dura o que a transmissão durar, e fazê-lo dentro do laço
            // pararia quem lê a conexão e atende comandos até a pessoa parar de
            // compartilhar.
            tokio::spawn(async move {
                // O espelho: os mesmos avisos que uma tela alheia produz, pelo
                // mesmo caminho, para a casca não ter dois modos de desenhar a
                // mesma coisa. Quem compartilha era a única pessoa da sala que
                // não via o que estava mostrando — o servidor não devolve a
                // transmissão a quem a produziu, e com razão.
                let espelho = |visto: crate::bomba::EspelhoDaTela<'_>| {
                    let aviso = match visto {
                        crate::bomba::EspelhoDaTela::Abriu { largura, altura } => {
                            Aviso::TelaAbriu {
                                tela,
                                largura,
                                altura,
                            }
                        }
                        crate::bomba::EspelhoDaTela::Quadro { chave, bytes } => Aviso::TelaQuadro {
                            tela,
                            chave,
                            bytes: bytes.to_vec(),
                        },
                    };
                    let _ = avisos.send(aviso);
                };
                let fim = escoadouro
                    .escoar_espelhado(tela, origem, &mut eventos, espelho)
                    .await;
                let _ = avisos.send(Aviso::TelaFechou { tela });
                // Pelo mesmo motivo da de cima: são as duas linhas que contam o
                // fim de uma transmissão, uma vez cada, e as duas estavam
                // apagadas. A contagem do caminho bom diz quantos quadros
                // saíram e quantos o teto descartou — é o que separa «parou» de
                // «nunca andou».
                match fim {
                    Ok(contagem) => tracing::info!(?contagem, "a transmissão de tela acabou"),
                    Err(erro) => tracing::warn!(%erro, "a transmissão de tela caiu"),
                }
            });
        });
    }

    /// O pedido guardado vira bomba, e `escoar` recebe o canal dela.
    ///
    /// Quem escoa entra por fora, e a costura é o que torna esta máquina de
    /// estados afirmável: escrever no fio é a única metade daqui que precisa de
    /// uma conexão QUIC viva, e sem separá-la «pedido guardado → nome chegando →
    /// bomba nascendo» só seria exercível contra um servidor de verdade — que é o
    /// mesmo que dizer que nunca seria exercido.
    fn nascer_a_tela(
        &mut self,
        tela: ScreenId,
        escoar: impl FnOnce(seele_proto::screen::ScreenSource, mpsc::Receiver<crate::EventoDaBomba>),
    ) {
        if self.tela_viva.is_some() {
            return;
        }
        let Some((pedido, limites)) = self.tela_pedida.take() else {
            return;
        };

        let PedidoDeTela {
            biblioteca,
            captura,
            origem,
        } = *pedido;
        let arranjo = crate::bomba::Arranjo {
            teto: self.teto_de_video(limites.banda_bps),
            faixa: self.faixa,
            escolha_de_resolucao: limites.resolucao,
            cadencia: limites.cadencia,
            prioridade: limites.prioridade,
        };

        // `|| {}` é a resposta que este crate consegue dar ao §2, e a bomba diz
        // por quê no cabeçalho dela: baixar a prioridade da thread pede
        // `setpriority`/`SetThreadPriority`, e `unsafe_code` é `forbid` neste
        // workspace. A ausência fica visível aqui em vez de escondida lá.
        let (bomba, eventos) = match crate::bomba::ligar(biblioteca, captura, arranjo, || {}) {
            Ok(ligada) => ligada,
            Err(erro) => {
                tracing::warn!(%erro, "a thread do codificador de tela não nasceu");
                return;
            }
        };

        escoar(origem, eventos);
        self.tela_viva = Some(TelaViva {
            tela,
            bomba,
            limites,
        });
    }

    /// O teto de agora, com as três pernas do §5.1.
    ///
    /// As três, enfim: a de quem hospeda chega pelo `HostUplink`, o N pelo
    /// `ScreenViewers`, e a de quem compartilha sai da [`crate::caminho::Sonda`]
    /// — que é a pergunta 2 do §8 respondida, e não mais o cano das provas
    /// assumido para sempre. Enquanto a sonda não mediu nada, ela devolve
    /// exatamente aquela suposição, então a primeira transmissão de uma sessão
    /// abre com o mesmo teto de antes.
    fn teto_de_video(&self, escolha_bps: Option<u32>) -> crate::tela::TetoDeVideo {
        let mut teto = crate::tela::TetoDeVideo::com_caminho(self.caminho.estimativa());
        if let Some(medido) = self.caminho_de_quem_hospeda_bps {
            teto = teto.com_caminho_de_quem_hospeda(medido);
        }
        teto.com_espectadores(self.espectadores)
            .com_escolha(escolha_bps)
    }

    /// Uma leitura do transporte para a sonda, e a ordem para a bomba quando a
    /// estimativa andou.
    ///
    /// Chamado em toda volta do laço, que é cinco vezes por segundo — a janela
    /// de amostragem é da sonda, não daqui, e ler mais vezes que o necessário
    /// custa uma cópia de contadores.
    ///
    /// **Só enquanto esta pessoa está compartilhando**, e a condição é a medida
    /// inteira: sem transmissão não há quem encha o cano, e o que sairia pelo
    /// soquete seria a voz — que diz que está bom a 40 kbps e não diz quanto
    /// cabe. É a frase do §8 pergunta 2, e é por isso que a resposta é a tela.
    fn medir_o_caminho(&mut self) {
        let (Some(cliente), Some(viva)) = (self.cliente.as_ref(), self.tela_viva.as_ref()) else {
            return;
        };
        let amostra = crate::caminho::Amostra {
            transporte: cliente.amostra_do_transporte(),
            teto: self.teto_de_video(viva.limites.banda_bps).teto(self.faixa),
            faixa: self.faixa,
        };
        if self.caminho.observar(Instant::now(), &amostra).is_some() {
            self.reconferir_o_teto();
        }
        // **Fora do `if`, e de propósito.** O `Some` acima diz que a estimativa
        // *mudou*; o que quem grava a lista de conhecidos quer é o valor de
        // agora, e uma sessão inteira sem mudança nenhuma continua tendo um
        // número que vale a pena lembrar.
        self.caminho_medido.store(
            self.caminho.estimativa(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Conta à bomba que o teto andou.
    ///
    /// Uma ordem só para as três coisas que o mexem, porque o N já mora dentro
    /// do [`crate::tela::TetoDeVideo`] pela perna de quem hospeda.
    fn reconferir_o_teto(&self) {
        let Some(viva) = self.tela_viva.as_ref() else {
            return;
        };
        let teto = self.teto_de_video(viva.limites.banda_bps);
        let _ = viva.bomba.teto(teto, self.faixa);
    }

    /// Mata a bomba, se houver uma.
    fn parar_a_tela(&mut self) {
        // A contagem morre junto: ela é de **uma** transmissão, e deixá-la de pé
        // faria a próxima nascer dividindo a perna de quem hospeda pelo público
        // da anterior — um teto apertado sem que ninguém estivesse assistindo.
        self.espectadores = 0;
        // E a janela da sonda também: entre esta transmissão e a próxima o cano
        // fica vazio, e uma janela que atravessasse esse buraco mediria o
        // silêncio. A estimativa fica — o cano é o mesmo.
        self.caminho.esquecer_a_conexao();
        matar(self.tela_viva.take());
    }

    /// Guarda o que a reconexão vai ter que refazer.
    fn lembrar(&mut self, comando: &Comando) {
        match comando {
            Comando::LembrarCaminho(bps) => {
                tracing::info!(bps, "a sonda começa do caminho lembrado deste servidor");
                self.caminho = crate::caminho::Sonda::partindo_de(*bps);
            }
            Comando::InserirPlug(voice_room) => self.voice_room = Some(*voice_room),
            Comando::EjetarPlug => self.voice_room = None,
            Comando::AbrirLinha(linha) => self.linha = Some(*linha),
            Comando::Muted(ligado) => self.muted = *ligado,
            Comando::Isolamento(ligado) => self.isolamento = *ligado,
            // Fazer uma sala e moderar alguém **não** entram aqui, e a ausência
            // é deliberada nos dois casos. O que se refaz ao reconectar é onde
            // a pessoa estava — a sala de voz, a Linha, os dois silêncios —, porque
            // voltar sem isso é voltar para outro lugar. Fazer uma sala é coisa
            // que se faz uma vez; repetida depois de uma queda, ela apareceria
            // minutos mais tarde do nada, e duplicada se a pessoa já tivesse
            // pedido de novo à mão. Expulsar é pior: refeito depois de cinco
            // minutos de bateria, derrubaria de novo alguém que já tinha
            // voltado, e ninguém entenderia por quê.
            //
            // Nomear o servidor e dar-lhe uma imagem também não entram, pelo
            // primeiro motivo: são coisas que se fazem uma vez. Refeito depois
            // da queda, um `RenomearServer` desfaria o nome que **outra pessoa**
            // pôs nos cinco minutos em que este cliente esteve fora — e o nome
            // do servidor é de todo mundo que está dentro, ao contrário da sala de voz em
            // que esta pessoa estava sentada.
            //
            // Apagar é o pior dos três, e por isso vale escrevê-lo: refeito
            // depois da queda, ele destruiria a sala que alguém fez no lugar da
            // que sumiu — e a confirmação que autorizou o primeiro pedido dizia
            // o tamanho de **outro** estrago. Pesar uma Linha também não volta:
            // é uma pergunta, e a resposta que interessava era a de quando a
            // caixa estava aberta.
            //
            // **Compartilhar a tela é o que menos pode voltar dos dois.** Refeito
            // depois de cinco minutos de bateria, ele poria o monitor de alguém
            // no ar sem que ninguém tivesse apertado nada, e minutos depois de a
            // pessoa ter desistido — a captura já morreu na queda, por
            // [`Self::cair`], e ressuscitá-la seria a única coisa neste enum que
            // liga uma câmera sozinha.
            _ => {}
        }
    }

    fn encerrar(&mut self, motivo: Motivo) {
        // Antes de derrubar a conexão: a bomba fecha o fluxo dela ao morrer, e
        // o fim do fluxo é a segunda maneira de dizer «parei» (§3.6). Derrubar
        // primeiro trocaria isso por um fluxo cortado.
        self.tela_pedida = None;
        self.parar_a_tela();
        if let Some(mut cliente) = self.cliente.take() {
            cliente.disconnect();
        }
        let _ = self.avisos.send(Aviso::Encerrado(motivo));
    }
}

/// Mata uma transmissão, se houver uma.
///
/// Livre e não método porque o caminho de parar corre com um empréstimo do
/// [`Client`] vivo na mão, e um `&mut self` o atropelaria.
fn matar(viva: Option<TelaViva>) {
    let Some(viva) = viva else {
        return;
    };
    // `Bomba::parar` junta a thread do codificador, e juntar **bloqueia**. A
    // casca gráfica roda este motor num runtime de thread única, e bloquear aqui
    // pararia junto a tarefa que escoa — que é justamente quem tem de esvaziar o
    // canal para a thread conseguir entregar o `Fim` dela. Na piscina de bloqueio
    // o mesmo fim não custa nada ao laço.
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => {
            runtime.spawn_blocking(move || viva.bomba.parar());
        }
        // Fora de um runtime — um teste que monta o motor à mão. Aqui não há
        // tarefa nenhuma para proteger do bloqueio.
        Err(_) => viva.bomba.parar(),
    }
}

/// Confere o que o convite prometeu contra o que o servidor ofereceu.
///
/// Devolve o veredito quando a conexão pode seguir, e o erro quando ela tem que
/// cair. Uma função à parte de [`Enlace::conectar`] porque tudo aqui é decisão
/// sobre valores, e sem isso a fiação inteira ficava sem guarda.
///
/// Os cinco desfechos são exercidos por teste, sem servidor do outro lado — e os
/// dois de `PinDecision::Matches` importam tanto quanto os de primeiro contato:
/// é neles que mora a política de **não** derrubar. Um link velho contra um
/// servidor já conhecido avisa e segue, porque o TOFU já provou que é o mesmo
/// servidor de ontem; recusar ali trancaria a pessoa para fora de um servidor que
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

/// Conta um candidato a quem estiver observando, se alguém estiver.
///
/// Um `send` recusado é silêncio de propósito: o canal só fecha quando quem
/// observava desistiu, e a conexão não pode depender de uma tela estar viva —
/// é a mesma regra que faz o `watch` da chegada ignorar o `send` sem ouvinte.
fn contar(olhos: Option<&mpsc::UnboundedSender<Tentativa>>, tentativa: Tentativa) {
    if let Some(olhos) = olhos {
        let _ = olhos.send(tentativa);
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
/// loopback nenhum — então um servidor cujo ponto de encontro roda na mesma
/// máquina, atrás de socket de pilha dupla, observa `::ffff:127.0.0.1` como
/// origem e publica isso. Quando acontece, quem entra gasta
/// [`AVISOS_POR_CANDIDATO`] furos da janela do anfitrião (três, não um),
/// 3 × 96 bytes de metadado que ninguém pediu, e [`ESPERA_DO_FURO`] a mais antes
/// do aperto de mão; o servidor fura contra o próprio loopback, e a conexão sobe
/// assim mesmo, porque o candidato sempre foi alcançável sem furo nenhum.
///
/// O que ela **não** custa é segurança, e é por isso que o preço é aceitável: o
/// destino do furo é `bilhete.aviso()`, fixado em `Batida::preparar` e embutido
/// no datagrama. O candidato decide apenas **se** o `LEVE` sai, nunca **para
/// onde** o anfitrião fura. Um candidato mal classificado não redireciona pacote
/// contra terceiro nenhum.
pub(crate) fn e_publico(ip: IpAddr) -> bool {
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
/// A diferença decide qual erro sobra quando nenhum candidato entra. Um servidor
/// que recusou o convite, ou cuja chave mudou, disse alguma coisa sobre o
/// mundo; um "não alcancei" de um endereço que nunca ia voltar não disse nada,
/// e mostrá-lo no lugar do outro manda a pessoa procurar problema de rede
/// enquanto o servidor está ali, recusando.
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
    /// uma vez — dois servidores numa LAN dividindo a entrada `localhost`, e o
    /// segundo parecendo o primeiro com a chave trocada (`tofu.rs`). Um teste
    /// em que os dois valores são iguais não pega essa troca.
    fn destino_de_teste(impressao_esperada: Option<&str>) -> Destino {
        Destino {
            servidor: "127.0.0.1:1".parse().expect("endereço"),
            nome_tls: "localhost".into(),
            chave_do_pin: "casa".into(),
            apelido: "pessoa".into(),
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
        // de um servidor que ela usa porque um amigo mandou um link velho.
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

        motor.lembrar(&Comando::InserirPlug(VoiceRoomId(2)));
        motor.lembrar(&Comando::AbrirLinha(ChannelId(7)));
        motor.lembrar(&Comando::Muted(true));
        motor.lembrar(&Comando::Isolamento(true));

        assert_eq!(motor.voice_room, Some(VoiceRoomId(2)));
        assert_eq!(motor.linha, Some(ChannelId(7)));
        assert!(motor.muted);
        assert!(motor.isolamento);

        // Ejetar não é uma queda: quem saiu da sala de voz não volta para ele.
        motor.lembrar(&Comando::EjetarPlug);
        assert_eq!(motor.voice_room, None);
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
        // loopback, um fazendo de ponto de encontro e outro fazendo de server.
        // Nenhum dos dois responde nada — o que se mede é de onde os pacotes
        // saíram, e é isso que o outro lado usaria.
        //
        // # Por que o convite tem dois endereços, e por que o primeiro é público
        //
        // Porque desde o aviso por candidato **nem todo candidato avisa**: o da
        // rede de casa não precisa de furo nenhum, e o loopback do servidor de
        // teste é ainda menos. Com um convite de um endereço só, no loopback,
        // nada chegaria ao ponto de encontro e este teste mediria o silêncio.
        //
        // Então o convite traz `203.0.113.7` — TEST-NET-3, RFC 5737, público em
        // tudo que importa aqui e onde não há servidor nenhum — na frente, e o
        // server de teste atrás. O aviso sai por causa do primeiro; o aperto de
        // mão que chega ao segundo é o do mesmo laço, pelo socket emprestado da
        // mesma `Batida`. É exatamente a fiação de produção, e é a porta dela
        // que se compara.
        let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback não abriu");
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback não abriu");
        let onde_o_server_atende = server.local_addr().expect("endereço");

        let bilhete = seele_proto::uri::Bilhete::novo(
            ponto.local_addr().expect("endereço").to_string(),
            "45.33.32.156:41234",
        )
        .expect("bilhete de teste");
        let destino = |servidor: SocketAddr| Destino {
            servidor,
            nome_tls: "localhost".into(),
            chave_do_pin: servidor.to_string(),
            apelido: "pessoa".into(),
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
            vec![destino(refletido), destino(onde_o_server_atende)],
            Some(bilhete),
            SigningKey::from_bytes(&[7; 32]),
            Arc::new(crate::tofu::MemoryPinStore::new()),
        ));

        let mut balde = [0_u8; 1500];
        // Folgado: o servidor de teste é o **segundo** candidato, e só é tentado
        // depois de o primeiro queimar o prazo dele.
        let prazo = Duration::from_secs(15);

        let (_, de_quem_bateu) = tokio::time::timeout(prazo, ponto.recv_from(&mut balde))
            .await
            .expect("nada chegou ao ponto de encontro")
            .expect("o ponto de encontro não leu");
        let (_, de_quem_conecta) = tokio::time::timeout(prazo, server.recv_from(&mut balde))
            .await
            .expect("nada chegou ao servidor")
            .expect("o servidor não leu");

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
            linha: ChannelId(1),
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
        motor.lembrar(&Comando::InserirPlug(VoiceRoomId(2)));

        motor.lembrar(&Comando::Expulsar {
            pessoa: PersonId(9),
        });
        motor.lembrar(&Comando::Banir {
            pessoa: PersonId(9),
            motivo: None,
            expira_em: None,
        });
        motor.lembrar(&Comando::MoverPersono {
            pessoa: PersonId(9),
            voice_room: VoiceRoomId(5),
        });
        motor.lembrar(&Comando::RemoverMensagem {
            mensagem: MessageId(1),
        });

        assert_eq!(
            motor.voice_room,
            Some(VoiceRoomId(2)),
            "moderar outra pessoa mexeu em onde este cliente está"
        );
    }

    // ------------------------------------------------------- a tela

    /// Uma captura de mentira: entrega sempre um quadro do tamanho pedido.
    ///
    /// A da máquina precisa de monitor e de permissão, e nenhum dos dois existe
    /// num agente de integração contínua. O que estes testes provam é a máquina
    /// de estados, não a ScreenCaptureKit.
    #[derive(Debug)]
    struct CapturaDeMentira;

    #[derive(Debug)]
    struct FonteDeMentira {
        largura: usize,
        altura: usize,
        passo: std::sync::atomic::AtomicUsize,
    }

    impl crate::video::FonteDeQuadros for FonteDeMentira {
        fn tomar(&self) -> Option<seele_video::codec::QuadroI420> {
            let passo = self
                .passo
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let (largura, altura) = (self.largura, self.altura);
            let mut luma = Vec::with_capacity(largura * altura);
            for linha in 0..altura {
                for coluna in 0..largura {
                    // Bordas duras: um quadro chapado sairia com trinta bytes e
                    // não provaria que o codificador rodou.
                    let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                    luma.push(if claro { 235 } else { 16 });
                }
            }
            let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
            seele_video::codec::QuadroI420::novo(largura, altura, luma, croma.clone(), croma).ok()
        }
    }

    impl crate::video::Captura for CapturaDeMentira {
        type Fonte = FonteDeMentira;

        fn iniciar(
            &mut self,
            resolucao: seele_video::codec::Resolucao,
            _cadencia: seele_video::codec::Cadencia,
        ) -> Result<Self::Fonte, crate::video::CapturaRecusou> {
            Ok(FonteDeMentira {
                largura: resolucao.largura(),
                altura: resolucao.altura(),
                passo: std::sync::atomic::AtomicUsize::new(0),
            })
        }
    }

    /// O módulo do Cisco, ou `None` com o motivo impresso.
    ///
    /// **Pula em vez de falhar, e o motivo é a licença**: o módulo não pode
    /// morar neste repositório, e um teste que o exigisse seria vermelho em toda
    /// máquina limpa — um teste sempre vermelho é um teste que todo mundo
    /// aprende a ignorar. Mesma decisão de `crate::bomba` e de `crate::video`.
    fn biblioteca_de_teste() -> Option<seele_video::BibliotecaDeVideo> {
        let mut pastas = Vec::new();
        if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
            let caminho = std::path::PathBuf::from(apontado);
            pastas.push(if caminho.is_dir() {
                caminho
            } else {
                caminho.parent().map_or_else(|| caminho.clone(), Into::into)
            });
        }
        pastas.push(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target"),
        );
        match seele_video::BibliotecaDeVideo::procurar_e_carregar(&pastas) {
            Ok(biblioteca) => Some(biblioteca),
            Err(motivo) => {
                // Ver `seele-video/tests/ida_e_volta.rs`: onde o codec é
                // exigido, faltar é falha e não licença para pular. Um teste
                // que volta cedo conta como passado, e é assim que uma suíte
                // fica verde sem nunca ter rodado.
                // Só onde há módulo publicado; no Linux o Cisco não publica.
                assert!(
                    std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                        || seele_video::modulo::publicado_para_este_sistema().is_none(),
                    "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado \
                     e ele não está aqui: {motivo}"
                );
                eprintln!(
                    "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                     isso. Aponte-o com SEELE_OPENH264.\n  Ligue SEELE_EXIGE_CODEC para que \
                     faltar vire falha em vez de pulo."
                );
                None
            }
        }
    }

    fn pedido_de_teste(biblioteca: seele_video::BibliotecaDeVideo) -> PedidoDeTela {
        PedidoDeTela {
            biblioteca,
            captura: crate::video::CapturaEmCaixa::nova(CapturaDeMentira),
            origem: seele_proto::screen::ScreenSource::Monitor,
        }
    }

    #[test]
    fn a_bomba_so_nasce_quando_o_server_da_nome_a_transmissao() {
        let Some(biblioteca) = biblioteca_de_teste() else {
            return;
        };
        let mut motor = motor_de_teste();

        // O botão foi apertado: o que existe é o pedido, e mais nada. Uma bomba
        // aqui seria uma bomba sem `ScreenId` para pôr no cabeçalho do fluxo.
        motor.tela_pedida = Some((
            Box::new(pedido_de_teste(biblioteca)),
            crate::video::LimitesDeTela::default(),
        ));
        assert!(
            motor.tela_viva.is_none(),
            "o pedido sozinho já tinha ligado a captura"
        );

        // O servidor respondeu, e a transmissão ganhou nome.
        let mut canal = None;
        motor.nascer_a_tela(ScreenId(7), |origem, eventos| {
            canal = Some((origem, eventos));
        });

        let viva = motor.tela_viva.as_ref().expect("a bomba não nasceu");
        assert_eq!(viva.tela, ScreenId(7), "a bomba nasceu com outro nome");
        assert!(
            motor.tela_pedida.is_none(),
            "o pedido sobreviveu à transmissão que ele abriu, e o próximo \
             `ScreenShareStarted` abriria uma segunda"
        );

        let (origem, mut eventos) = canal.expect("quem escoa não recebeu o canal");
        assert_eq!(
            origem,
            seele_proto::screen::ScreenSource::Monitor,
            "o cabeçalho do fluxo sairia dizendo que é uma janela"
        );

        // E ela está codificando de verdade: o primeiro evento é o fluxo a
        // abrir, que só sai depois de a captura e o codificador armarem.
        match eventos.blocking_recv() {
            Some(crate::EventoDaBomba::Fluxo { geracao, .. }) => assert_eq!(geracao, 1),
            outro => panic!("a bomba não abriu fluxo nenhum: {outro:?}"),
        }

        // E morre com o enlace. Uma bomba que sobrevivesse à queda seria uma
        // thread codificando para uma conexão morta.
        motor.cair();
        assert!(
            motor.tela_viva.is_none(),
            "a bomba sobreviveu à queda do enlace"
        );
        assert!(
            motor.tela_pedida.is_none(),
            "o pedido sobreviveu à queda, e a reconexão poria a tela de alguém \
             no ar sem que ninguém apertasse nada"
        );

        // A thread acabou de fato: o `Fim` é a última coisa que ela manda, e o
        // canal fecha depois dele.
        let mut acabou = false;
        while let Some(evento) = eventos.blocking_recv() {
            if matches!(evento, crate::EventoDaBomba::Fim(None)) {
                acabou = true;
            }
        }
        assert!(acabou, "a thread do codificador não disse que acabou");
    }

    /// O fecho: do pedido guardado ao primeiro quadro lido do outro lado.
    ///
    /// **É a pergunta que esta tarefa existe para responder** — apertar
    /// compartilhar faz um quadro sair pela conexão? — e ela não se responde
    /// olhando `tela_viva`: o que prova é o cabeçalho que quem recebe lê e os
    /// bytes que vêm depois dele. `crate::bomba` já prova a bomba contra uma
    /// conexão; o que estava sem prova é a costura do meio, que é o
    /// `Escoadouro` que o [`Client`] embrulha e a tarefa que o motor solta.
    #[tokio::test(flavor = "multi_thread")]
    async fn do_pedido_guardado_ao_quadro_lido_do_outro_lado() {
        let Some(biblioteca) = biblioteca_de_teste() else {
            return;
        };
        let (saida, entrada) = crate::tela::tests::par().await;
        let escoadouro = crate::bomba::Escoadouro::nova(saida);
        let tela = ScreenId(0x00C0_FFEE);

        let mut motor = motor_de_teste();
        motor.tela_pedida = Some((
            Box::new(pedido_de_teste(biblioteca)),
            crate::video::LimitesDeTela::default(),
        ));

        // A mesma forma da produção: uma tarefa própria, porque escoar dura o
        // que a transmissão durar.
        let mut escoando = None;
        motor.nascer_a_tela(tela, |origem, mut eventos| {
            escoando = Some(tokio::spawn(async move {
                escoadouro.escoar(tela, origem, &mut eventos).await
            }));
        });
        let escoando = escoando.expect("quem escoa não foi chamado");

        let mut recepcao = crate::tela::Recepcao::aceitar(&entrada)
            .await
            .expect("aceitar o fluxo da tela");
        assert_eq!(
            recepcao.cabecalho().screen,
            tela,
            "o fluxo abriu com o nome de outra transmissão"
        );
        assert_eq!(
            recepcao.cabecalho().source,
            seele_proto::screen::ScreenSource::Monitor,
            "o cabeçalho diz janela sobre um monitor"
        );

        let primeiro = recepcao
            .proximo_quadro()
            .await
            .expect("ler o primeiro quadro")
            .expect("o fluxo não podia ter acabado");
        assert!(primeiro.chave(), "o primeiro quadro de um fluxo é chave");
        assert!(!primeiro.bytes.is_empty(), "saiu um quadro vazio");

        // E a queda do enlace fecha tudo: a bomba morre e o fluxo termina.
        motor.cair();
        let contagem = escoando
            .await
            .expect("a tarefa de escoar")
            .expect("escoar até o fim");
        assert!(contagem.enviados >= 1, "nenhum quadro chegou ao fio");
        assert_eq!(contagem.fluxos, 1);
    }

    #[test]
    fn sem_pedido_guardado_um_nome_de_transmissao_nao_liga_nada() {
        // O servidor reenvia `ScreenShareStarted` a cada pessoa que entra num sala de voz
        // onde já há transmissão — inclusive a quem está transmitindo. Sem esta
        // guarda, cada pessoa entrando na sala ligaria outra captura da mesma
        // tela.
        let mut motor = motor_de_teste();
        let mut chamou = false;
        motor.nascer_a_tela(ScreenId(1), |_, _| chamou = true);
        assert!(!chamou, "ligou uma bomba que ninguém pediu");
        assert!(motor.tela_viva.is_none());
    }

    #[test]
    fn compartilhar_tela_nao_e_lembrado_para_a_reconexao() {
        // Refeito depois de cinco minutos de bateria, poria o monitor de alguém
        // no ar sem que ninguém tivesse apertado nada — e minutos depois de a
        // pessoa ter desistido.
        let mut motor = motor_de_teste();
        motor.voice_room = Some(VoiceRoomId(3));
        motor.lembrar(&Comando::PararDeCompartilhar);
        assert_eq!(motor.voice_room, Some(VoiceRoomId(3)));
    }

    /// A metade da regra de aceite do §3.2 que faltava.
    ///
    /// O teto respondia ao `HostUplink` e ao número de espectadores e **não**
    /// ao sinal da voz piorando: numa sala onde a voz começava a doer, a tela
    /// não cedia sozinha. A perna que faltava já vinha pelo fio — o servidor
    /// devolve a taxa de cada pessoa em `PersonState`, uma vez por segundo — e
    /// ninguém guardava a sua.
    #[test]
    fn a_faixa_da_voz_desce_pelo_que_o_server_devolve() {
        use seele_proto::control::{PersonState, Presence};

        let eu = PersonId(7);
        let outra = PersonId(9);
        let estado = |pessoa: PersonId, taxa: u8| PersonState {
            person: pessoa,
            signal: taxa,
            speaking: false,
            muted: false,
            total_isolation: false,
            presence: Presence::Available,
        };

        // A voz doendo derruba a faixa, e é isto que faz a tela ceder.
        assert_eq!(
            faixa_nova(SignalBand::Nominal, &estado(eu, 10), Some(eu)),
            Some(SignalBand::Critical),
            "a taxa despencou e a faixa não acompanhou"
        );

        // A de outra pessoa não move nada. Sem esta guarda, a tela cederia
        // porque a conexão **de outro** piorou — e quem compartilha ficaria
        // pagando pelo vizinho.
        assert_eq!(
            faixa_nova(SignalBand::Nominal, &estado(outra, 10), Some(eu)),
            None
        );

        // Sem sessão não há «esta pessoa». Uma mensagem antes do aperto de mão
        // terminar não é sobre ninguém que este motor conheça.
        assert_eq!(faixa_nova(SignalBand::Nominal, &estado(eu, 10), None), None);

        // E a mesma faixa não vira ordem: a taxa chega uma vez por segundo e
        // quase sempre no mesmo degrau, e refazer o teto a cada chegada
        // acordaria a thread do codificador para lhe dizer o que ela já sabe.
        assert_eq!(
            faixa_nova(SignalBand::Critical, &estado(eu, 10), Some(eu)),
            None
        );

        // E ela sobe de volta: ceder não pode ser de mão única, ou a tela
        // ficaria pequena para sempre depois do primeiro engasgo.
        assert_eq!(
            faixa_nova(SignalBand::Critical, &estado(eu, 100), Some(eu)),
            Some(SignalBand::Nominal)
        );
    }

    /// **A perna que faltava no teto, ligada.**
    ///
    /// O motor usava `TetoDeVideo::novo()` para a perna de quem compartilha, o
    /// que quer dizer o cano das provas — 2 Mbps — para sempre, em toda casa.
    /// Quem tinha fibra via 720p a sessão inteira. Agora aquela perna sai da
    /// [`crate::caminho::Sonda`], e este teste é o fio entre as duas: janelas
    /// cheias e calmas entram, e o teto que sai da mesma função sobe.
    #[test]
    fn o_teto_sai_do_caminho_que_a_sonda_mediu_e_nao_da_suposicao() {
        use crate::caminho::{Amostra, Transporte};
        use crate::tela::Teto;

        let mut motor = motor_de_teste();
        // O servidor declarou uma subida larga, então a perna dele sai da frente e
        // quem manda no `min` do §5.1 é a desta máquina. Sem isto o teto ficaria
        // preso em 1200 kbps por causa da **outra** perna, que é um achado
        // separado — ver `caminho::tests::sem_a_subida_do_server_...`.
        motor.caminho_de_quem_hospeda_bps = Some(100_000_000);

        // Antes de medir, a suposição de sempre: a primeira transmissão de uma
        // sessão abre exatamente com o teto que abria antes deste módulo.
        assert_eq!(
            motor.teto_de_video(None).teto(SignalBand::Nominal),
            Teto::Bps(1_200_000)
        );

        // Cinco janelas cheias e sem piora, como a tica do motor as entregaria.
        let inicio = Instant::now();
        let mut bytes = 0_u64;
        for segundo in 0..12_u32 {
            let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);
            // O que sai pelo soquete numa janela em que a tela encheu o teto: o
            // orçamento inteiro mais a voz.
            bytes += u64::from(teto.bps() + 60_000) / 8;
            let amostra = Amostra {
                transporte: Transporte {
                    bytes_enviados: bytes,
                    ida_e_volta: Duration::from_millis(20),
                    ..Transporte::default()
                },
                teto,
                faixa: SignalBand::Nominal,
            };
            motor
                .caminho
                .observar(inicio + Duration::from_secs(u64::from(segundo)), &amostra);
        }

        let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);
        assert!(
            teto.bps() > 1_200_000,
            "doze janelas cheias e o teto continuou na suposição: {teto:?}"
        );
        assert_eq!(
            teto.resolucao_estimada(),
            Some(seele_video::codec::Resolucao::P720),
            "o caminho medido comprava 720p e a tela continuou menor"
        );
    }

    /// Sem transmissão não há quem encha o cano, e ler o transporte ali seria
    /// medir a voz — que diz que está bom a 40 kbps e não diz quanto cabe.
    #[test]
    fn sem_tela_no_ar_a_sonda_nao_e_alimentada() {
        let mut motor = motor_de_teste();
        let antes = motor.caminho.estimativa();
        motor.medir_o_caminho();
        assert_eq!(motor.caminho.estimativa(), antes);
        assert_eq!(
            motor.teto_de_video(None).teto(SignalBand::Nominal),
            crate::tela::Teto::Bps(1_200_000)
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
                apelido: "pessoa".into(),
                segredo: None,
                impressao_esperada: None,
            },
            chave: SigningKey::from_bytes(&[7; 32]),
            pins: Arc::new(crate::tofu::MemoryPinStore::new()),
            cliente: None,
            bateria: Battery::new(),
            inicio: Instant::now(),
            voice_room: None,
            linha: None,
            muted: false,
            isolamento: false,
            avisos,
            rtt: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            tela_pedida: None,
            tela_viva: None,
            faixa: FAIXA_INICIAL,
            caminho_de_quem_hospeda_bps: None,
            espectadores: 0,
            caminho: crate::caminho::Sonda::nova(),
            caminho_medido: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}
