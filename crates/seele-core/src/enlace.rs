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
use seele_proto::control::{
    ConsentimentoDePar, DisconnectReason, MotivoDeFalhaDePar, ServerMessage,
};
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, VoiceRoomId,
};
use seele_proto::signal::SignalBand;
use tokio::sync::mpsc;

use crate::battery::{Action, Battery, Link};
use crate::client::{Client, ConnectError, MediaChannel, SessionInfo};
use crate::par;
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
    /// Um operador acabou com esta sessão, e o motivo veio no fio.
    ///
    /// Separado de [`Self::Recusado`] porque a casca precisa dizer **qual** foi:
    /// «você foi expulso» e «você foi banido» são frases diferentes, e
    /// `EndReason::CredentialRejected` — o que `Recusado` vira do outro lado do
    /// FFI — mandaria a pessoa conferir a senha de uma conta que está certa.
    Moderado(DisconnectReason),
}

/// As despedidas que **acabam** com a sessão, em vez de começarem a bateria.
///
/// Quase toda `Disconnecting` é uma queda com nome: manutenção, desligamento,
/// keepalive vencido, ficar para trás no barramento. Para todas elas reconectar
/// é o conserto, e é o que a bateria interna existe para fazer — o próprio
/// [`DisconnectReason::FellBehind`] tem isso escrito no doc dele.
///
/// Estas duas são o contrário: alguém do outro lado decidiu que esta pessoa não
/// fica. Reconectar as desfaz — e não em teoria. Medido em
/// `expulsar_acaba_com_a_sessao_e_deixa_voltar`, com sonda no servidor: a
/// conexão expulsa fecha, a bateria sobe outra em seguida, e ela **redeclara a
/// sala de voz guardada**. A pessoa expulsa reaparece sentada onde estava,
/// segundos depois, sem que ninguém tenha apertado nada; e como o assento agora
/// pertence à conexão nova, a saída da conexão expulsa não esvazia nada e
/// ninguém na sala é informado de que ela saiu. O verbo inteiro desfeito por um
/// recurso feito para túnel de trem.
///
/// `Banned` está aqui pelo mesmo motivo, e não porque a volta funcione: a
/// portaria a recusa. O que ela produz hoje é uma reconexão inútil a cada
/// batida da bateria por cinco minutos, e uma casca que diz «reconectando» a
/// quem foi banido.
fn a_sessao_acabou_aqui(motivo: DisconnectReason) -> bool {
    matches!(motivo, DisconnectReason::Kicked | DisconnectReason::Banned)
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
    EntrarNaVoiceRoom(VoiceRoomId),
    SairDaVoiceRoom,
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
    /// Declare ao servidor a identidade efêmera deste par.
    ///
    /// **Só existe para atravessar o canal de comandos.** Quem manda isto é
    /// [`Enlace::declarar_identidade_de_par`], chamado uma vez, do lado de
    /// fora, sobre o `Enlace` que já venceu a corrida de candidatos — nunca de
    /// dentro de `Motor::rodar`/`executar` por conta própria. Ver o doc de
    /// [`Motor::declarar_identidade_de_par`] para o porquê de precisar de
    /// `&mut self` inteiro, e não só do `&mut Client` que
    /// [`Motor::executar`] empresta para todo o resto deste `enum`.
    DeclararIdentidadeDePar,
    /// «Isto é o que eu consinto no caminho entre pares», inclusive quando a
    /// resposta nova é «nada».
    ///
    /// Substituiu um `EmprestarSubida(bool)`, que misturava as duas razões
    /// independentes do §5 — ver [`ConsentimentoDePar`]. Guardado em
    /// [`Motor::consentimento`] e redito a cada reconexão.
    ConsentirNoCaminhoEntrePares(ConsentimentoDePar),
    Sair,
}

/// Quanto tempo se espera o servidor abrir o fluxo de um anexo pedido.
///
/// Uma recusa nunca abre fluxo nenhum — a razão vem pelo controle —, então sem
/// prazo esta espera seria para sempre. Dez segundos é muito mais do que um
/// servidor doméstico leva para começar a mandar e pouco para deixar uma tela
/// esperando por bytes que não vêm.
const ESPERA_DE_ANEXO: Duration = Duration::from_secs(10);

/// Quanto tempo se dá ao caminho entre pares antes de cair para o servidor.
///
/// Generoso o bastante para um furo de NAT em rede doméstica — o roteiro de
/// duas máquinas de `docs/teste-duas-maquinas.md` é quem mede o real — e curto
/// o bastante para quem está esperando a imagem não ficar olhando para uma
/// tela parada além do razoável: passado ele, `crate::par::por_onde` cai para o
/// servidor sem drama, e é exatamente esse o ponto da malha ser alívio e nunca
/// dependência.
const PRAZO_DO_PAR: Duration = Duration::from_secs(3);

/// Quanto dura **cada** tentativa de discar para um par, dentro de
/// [`PRAZO_DO_PAR`].
///
/// Ver [`discar_ate_o_prazo`] para por que são várias e não uma. Meio segundo
/// é folgado para um aperto de mão em rede local e curto o bastante para
/// caberem seis tentativas no prazo — um par que exista responde na primeira
/// ou na segunda, e um que não exista custa o mesmo prazo total de antes.
const TENTATIVA_DE_PAR: Duration = Duration::from_millis(500);

/// O que uma tentativa do caminho entre pares, resolvida numa tarefa solta,
/// devolve ao laço de [`Motor::rodar`] para ele agir.
///
/// Só existe porque a tentativa é assíncrona e pode levar até
/// [`PRAZO_DO_PAR`]: bloquear o laço principal por isso pausaria a voz, o ping
/// e o resto desta sessão até o par responder ou o prazo vencer. A tarefa roda
/// solta (ver [`Motor::assistir_por_par`]) e devolve o que decidiu por aqui; só
/// quem tem o `&mut Client` — o laço de `rodar` — pode falar com o servidor.
#[derive(Debug)]
enum ResultadoDoPar {
    /// O par não veio, ou ligou e nunca abriu a transmissão: o servidor
    /// precisa saber, para poder ele mesmo assumir. Ver
    /// [`seele_proto::control::ClientMessage::ParFalhou`].
    ParFalhou {
        /// Qual transmissão.
        screen: ScreenId,
        /// O que aconteceu.
        motivo: MotivoDeFalhaDePar,
    },
}

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
            // Candidato único: não há corrida, e ainda assim a declaração só
            // sai depois de a conexão estar de pé. Ver o doc de
            // `Enlace::declarar_identidade_de_par`.
            if let Ok(enlace) = &resultado {
                let _ = enlace.declarar_identidade_de_par().await;
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
            // Só o vencedor declara, e só agora que se sabe quem venceu — ver
            // o doc de `Enlace::declarar_identidade_de_par` para o porquê de
            // não declarar dentro de `conectar_por`.
            let _ = enlace.declarar_identidade_de_par().await;
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
        let enlace = Self::conectar_por(&endpoint, None, destino, chave, pins).await?;
        let _ = enlace.declarar_identidade_de_par().await;
        Ok(enlace)
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
                // como `ejected` faria uma recusa de convite parecer uma pessoa
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
        let (resultados_do_par_tx, resultados_do_par) = mpsc::unbounded_channel();

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
            identidade_de_par: None,
            atendendo_pares: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            consentimento: ConsentimentoDePar::de_ninguem(),
            repasse: Arc::new(RepasseDeTela::default()),
            resultados_do_par,
            resultados_do_par_tx,
            tarefas_de_par: TarefasDePar::default(),
        };
        // **Não declara identidade aqui.** Achado do fix round 2:
        // `conectar_por` é o funil que **cada candidato** de uma conexão com
        // múltiplos destinos atravessa (`tentar_entre`), não só o vencedor da
        // corrida. Declarar deste lado faria todo candidato que conectasse
        // mandar `EmprestarSubida`, e o candidato perdedor — cuja conexão é
        // fechada logo depois — apagaria ao sair a declaração que o vencedor
        // acabara de fazer: `Pares` é chaveado por pessoa, e os dois
        // candidatos são a mesma pessoa. Só o vencedor declara, e só depois de
        // decidido quem venceu — ver `Enlace::declarar_identidade_de_par` e
        // onde ela é chamada em `conectar`/`tentar_entre`.
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

    /// Entra numa sala de voz. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn entrar_na_voice_room(&self, voice_room: VoiceRoomId) -> Result<(), Fechado> {
        self.mandar(Comando::EntrarNaVoiceRoom(voice_room)).await
    }

    /// Sai da sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn sair_da_voice_room(&self) -> Result<(), Fechado> {
        self.mandar(Comando::SairDaVoiceRoom).await
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
    /// Pede, e só. Nada aqui confere se esta pessoa pode: a `specs/08-seguranca.md`
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

    /// Declara ao servidor a identidade efêmera desta sessão para o caminho
    /// entre pares.
    ///
    /// **Chamada uma vez, sobre quem já venceu** — nunca de dentro do funil
    /// que conecta candidatos. Achado do fix round 2: `conectar_por` conecta
    /// **cada** candidato de uma conexão com múltiplos destinos, não só o
    /// vencedor da corrida (`Enlace::tentar_entre`), e declarar de lá dentro
    /// fazia todo candidato mandar `EmprestarSubida` pela própria conexão —
    /// o perdedor, cuja conexão fecha logo depois, apagava ao sair a
    /// declaração que o vencedor tinha acabado de fazer (`Pares` é chaveado
    /// por pessoa, e os dois candidatos são a mesma pessoa). Por isso esta
    /// função só é chamada nos pontos de retorno de `conectar` e
    /// `tentar_entre`, depois de já se saber quem ganhou.
    async fn declarar_identidade_de_par(&self) -> Result<(), Fechado> {
        self.mandar(Comando::DeclararIdentidadeDePar).await
    }

    /// Passa a valer este consentimento no caminho entre pares — inclusive
    /// quando a resposta nova é «nada».
    ///
    /// **É opt-in, e as duas metades são independentes** (§5 da spec de
    /// 05/09). Ver [`ConsentimentoDePar`]: privacidade, porque o endereço de
    /// quem consente é entregue à outra ponta; e custo, porque quem empresta
    /// passa a subir cópias para outras pessoas. Nada disto acontece sem esta
    /// chamada.
    ///
    /// **A retirada alcança o que já está no ar.** Desligar não é só parar de
    /// aceitar pedidos novos: o repasse em curso é cancelado e os caminhos de
    /// par abertos são derrubados, aqui, antes de a declaração nova sair —
    /// ver `Motor::passar_a_consentir`. Quem estava sendo servido por esta
    /// máquina volta a ser servido pelo servidor.
    ///
    /// # O teto desta versão
    ///
    /// [`PARES_QUE_ESTA_VERSAO_ATENDE`] é o máximo que este cliente consegue
    /// honrar, e valores acima dele são **baixados** antes de virar declaração:
    /// declarar um teto que esta máquina não atende faria o servidor apontá-la
    /// duas vezes e a segunda pessoa esperar o prazo do par vencer por uma
    /// promessa que nunca teve como ser cumprida.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn consentir_no_caminho_entre_pares(
        &self,
        consentimento: ConsentimentoDePar,
    ) -> Result<ConsentimentoDePar, Fechado> {
        // **Um `let` só, usado duas vezes.** O que vai no fio e o que volta a
        // quem pediu não podem diferir, e a maneira de garantir isso é não
        // haver dois valores — e não uma asserção dizendo que os dois são
        // iguais.
        let valeu = cabivel(consentimento);
        self.mandar(Comando::ConsentirNoCaminhoEntrePares(valeu))
            .await?;
        Ok(valeu)
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
    /// A identidade efêmera desta sessão para o caminho entre pares.
    ///
    /// Gerada na primeira vez que é necessária e guardada depois: quem chama
    /// `par::passar_a_atender` e depois `par::ligar` na mesma ponta precisa
    /// das duas — é para essa cópia que `Identidade::clone` existe, e é por
    /// isso que a identidade tem de sobreviver ao primeiro uso.
    identidade_de_par: Option<par::Identidade>,
    /// Se já há uma tentativa de servir um par em curso.
    ///
    /// No A1 quem empresta serve um par por vez (`crate::par::atender` só
    /// aceita uma ligação). Um `SirvaTelaPara` que chegasse no meio de outro
    /// não teria vaga — a escolha aqui é recusar em silêncio para o servidor
    /// (que não espera resposta nenhuma desta mensagem) e deixar só o rastro.
    ///
    /// `Arc<AtomicBool>` e não `bool` simples: a tarefa solta que
    /// [`Motor::servir_par`] cria precisa devolver a vaga quando termina, e
    /// ela não tem `&mut Motor` — só a cópia deste punho.
    atendendo_pares: Arc<std::sync::atomic::AtomicBool>,
    /// O que a pessoa consentiu no caminho entre pares.
    ///
    /// Guardado aqui e não só mandado uma vez porque a declaração **não
    /// sobrevive à sessão**: o servidor a apaga em `Pares::saiu` quando a
    /// conexão morre, e cada reconexão tem de dizer de novo quem esta máquina
    /// é e o que ela consente. Sem este campo, uma queda de rede tiraria a
    /// pessoa da malha caladamente e ela só descobriria por ninguém mais ser
    /// servido.
    ///
    /// Nasce em [`ConsentimentoDePar::de_ninguem`]: um opt-in que começasse
    /// ligado não seria um opt-in.
    consentimento: ConsentimentoDePar,
    /// Onde o repasse ao par vai buscar os bytes da tela que chega do
    /// servidor. Ver [`RepasseDeTela`].
    repasse: Arc<RepasseDeTela>,
    /// Por onde as tarefas soltas do caminho entre pares devolvem o que
    /// decidiram — ver [`ResultadoDoPar`].
    resultados_do_par: mpsc::UnboundedReceiver<ResultadoDoPar>,
    /// A metade de [`Self::resultados_do_par`] que as tarefas soltas recebem,
    /// para mandar de volta. Clonada a cada tarefa nova: um `Sender` barato
    /// de clonar, e cada tarefa é dona da própria cópia.
    resultados_do_par_tx: mpsc::UnboundedSender<ResultadoDoPar>,
    /// As tarefas soltas do caminho entre pares — a única coisa que alcança
    /// aquele caminho depois de ele ter começado.
    ///
    /// **Sem elas, `assistir(tela, false)` mentia.** Ele mandava
    /// `UnwatchScreen` ao servidor e nada mais: a tarefa de
    /// [`Motor::assistir_por_par`] seguia viva, lendo do par, e quem tinha
    /// acabado de fechar a janela continuava recebendo a tela por baixo —
    /// gastando a subida de quem empresta com imagem que ninguém olhava. Pior,
    /// quando aquele fluxo enfim terminasse, o `ParFalhou` de rotina sairia e
    /// pediria ao servidor a tela de volta.
    ///
    /// **E guardá-las num mapa cru não bastava**, pelo mesmo motivo por outra
    /// porta: um mapa largado desprende as tarefas em vez de cancelá-las, e
    /// elas sobreviviam à morte do motor inteiro. Ver [`TarefasDePar`].
    tarefas_de_par: TarefasDePar,
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

/// As tarefas soltas do caminho entre pares, e quem é dono delas.
///
/// # Uma alça largada desprende a tarefa; não a cancela
///
/// Enquanto estas alças moravam num `HashMap` cru dentro do [`Motor`], os dois
/// caminhos que matam o motor largavam o mapa sem tocá-lo — [`Enlace::drop`]
/// aborta a tarefa de [`Motor::rodar`], e um `Comando::Sair` a faz voltar —, e
/// cada tarefa de par ficava **viva** depois disso: lendo do par, com a conexão
/// QUIC de pé, gastando a subida de quem empresta por uma sessão que já tinha
/// acabado. E não em silêncio: a tarefa escreve no mesmo canal de avisos que o
/// `Enlace` continua segurando depois de `sair()`, então a casca recebia quadro
/// de tela **depois** do `Encerrado` que ela mesma pediu.
///
/// A tarefa de servir um par ([`Motor::servir_par`]) era pior: a alça dela não
/// era guardada em lugar nenhum. Ela não tinha sequer o acidente que salvava as
/// outras — descobrir no primeiro `send` que ninguém escuta —, porque não fala
/// com a casca: repassa bytes ao par até a conexão morrer.
///
/// # Por que um dono com `Drop`, e não uma chamada de limpeza
///
/// Porque «alguém tem de se lembrar de cancelar» é a forma de defeito que este
/// arquivo já pagou duas vezes. Atrás de um dono, cancelar passa a ser o que
/// acontece quando o motor some — por `abort`, por `return`, ou por um caminho
/// que ainda não existe.
///
/// É a escolha oposta à de [`TelaViva`], e o contraste é de propósito: a bomba
/// tem um fim direito a esperar — o `Fim` que fecha o fluxo —, e abortá-la
/// cortaria um quadro no meio. Estas tarefas não têm fim nenhum a esperar:
/// passam a vida paradas num `read` do par, sem ponto onde conferir um pedido
/// de parada.
#[derive(Default)]
struct TarefasDePar {
    /// A alça da tarefa que busca cada tela num par.
    ///
    /// Por [`ScreenId`] porque é isso que o comando nomeia — ver
    /// [`Motor::assistir_por_par`].
    caminhos: std::collections::HashMap<ScreenId, tokio::task::JoinHandle<()>>,
    /// A alça da tarefa que serve um par, quando esta máquina está servindo.
    ///
    /// Uma só porque no A1 quem empresta serve um par por vez — ver
    /// [`Motor::atendendo_pares`] e [`VagaDeAtendimento`].
    servindo: Option<tokio::task::JoinHandle<()>>,
}

impl TarefasDePar {
    /// Passa a ser dona da tarefa que busca esta tela num par.
    ///
    /// Um `AssistaTelaPor` novo para a mesma tela substitui a alça e aborta a
    /// anterior: duas tarefas lendo a mesma tela seriam duas cópias chegando, e
    /// a segunda nomeação é a que o servidor tem de pé.
    fn assistir(&mut self, screen: ScreenId, tarefa: tokio::task::JoinHandle<()>) {
        // As que já acabaram sozinhas saem daqui: a alça de uma tarefa morta
        // não aborta nada, e sem esta linha o mapa cresceria por sessão a cada
        // transmissão assistida.
        self.caminhos.retain(|_, alca| !alca.is_finished());
        if let Some(anterior) = self.caminhos.insert(screen, tarefa) {
            anterior.abort();
        }
    }

    /// Derruba o caminho desta tela, se houver um.
    fn parar_de_assistir(&mut self, screen: ScreenId) {
        if let Some(caminho) = self.caminhos.remove(&screen) {
            caminho.abort();
        }
    }

    /// Passa a ser dona da tarefa que serve um par.
    ///
    /// A anterior é abortada por garantia e não por necessidade:
    /// [`VagaDeAtendimento`] já impede que uma segunda comece enquanto a
    /// primeira corre, e uma alça que sobre aqui é de tarefa morta, sobre a
    /// qual `abort` não faz nada.
    fn servir(&mut self, tarefa: tokio::task::JoinHandle<()>) {
        if let Some(anterior) = self.servindo.replace(tarefa) {
            anterior.abort();
        }
    }

    /// Derruba **todos** os caminhos de par abertos, e deixa o resto de pé.
    ///
    /// Quem chama é a retirada do consentimento de assistir por par
    /// ([`Motor::passar_a_consentir`]): os caminhos existem por causa do
    /// endereço que a pessoa acabou de tirar de circulação. A tarefa que
    /// **serve** não é tocada — é o outro consentimento, e retirar um não é
    /// retirar o outro.
    fn parar_de_assistir_a_tudo(&mut self) {
        for (_, caminho) in self.caminhos.drain() {
            caminho.abort();
        }
    }

    /// Cancela a tarefa que serve um par, se houver uma.
    ///
    /// Quem chama é a retirada do consentimento de emprestar a conexão. A vaga
    /// volta por tabela: ela viaja para dentro da tarefa, e o [`Drop`] de
    /// [`VagaDeAtendimento`] a devolve quando a tarefa é solta.
    fn parar_de_servir(&mut self) {
        if let Some(servindo) = self.servindo.take() {
            servindo.abort();
        }
    }

    /// Cancela tudo o que está de pé.
    ///
    /// Chamado por [`Motor::cair`], onde o motor **sobrevive** ao corte e por
    /// isso não há `Drop` nenhum para fazer isto sozinho.
    fn largar_tudo(&mut self) {
        self.parar_de_assistir_a_tudo();
        self.parar_de_servir();
    }
}

impl Drop for TarefasDePar {
    fn drop(&mut self) {
        self.largar_tudo();
    }
}

/// A vaga de «estou servindo um par», enquanto ela está tomada.
///
/// # Por que um punho com `Drop`, e não duas escritas
///
/// A vaga era tomada no laço do motor e devolvida na **última linha** do corpo
/// da tarefa que serve. Enquanto ninguém cancelava aquela tarefa, as duas
/// escritas bastavam. Cancelá-la — e é o que [`TarefasDePar`] passou a fazer —
/// faz a última linha nunca correr, e uma vaga que não volta é esta máquina
/// **fora da malha para sempre**, sem erro em lugar nenhum: o servidor continua
/// apontando este par, porque a vaga dele voltou, e o cliente recusa cada
/// pedido em silêncio pela vaga que ficou.
///
/// Como guarda, devolver a vaga passa a ser o que acontece de todo jeito — pelo
/// fim do corpo, pelas saídas antecipadas de [`Motor::servir_par`], e pelo
/// `abort`, que solta a tarefa e com ela tudo o que ela segurava.
struct VagaDeAtendimento(Arc<std::sync::atomic::AtomicBool>);

impl VagaDeAtendimento {
    /// Toma a vaga, se ela estiver livre.
    ///
    /// `compare_exchange` e não um `load` seguido de `store`: conferir e tomar
    /// viram um ato só, sem instante entre os dois por onde um segundo pedido
    /// passasse pela mesma porta.
    ///
    /// `AcqRel` e não `Relaxed` porque a vaga guarda mais do que si mesma: quem
    /// a toma arma a ponta com `par::passar_a_atender`, e quem a devolve a
    /// desarmou antes. As duas metades têm de ser vistas em ordem por quem
    /// tomar a vaga depois.
    fn tomar(punho: &Arc<std::sync::atomic::AtomicBool>) -> Option<Self> {
        punho
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()?;
        Some(Self(Arc::clone(punho)))
    }
}

impl Drop for VagaDeAtendimento {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
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
            // Pelo mesmo motivo dos dois de cima, e é um `Arc`: quem lê a tela
            // do servidor precisa do ponto de encontro com quem serve um par
            // (ver `RepasseDeTela`), e o braço logo abaixo não pode pedir
            // `self` de novo.
            let repasse_da_tela = Arc::clone(&self.repasse);
            // Tirado pela mesma razão dos dois de cima: `resultados_do_par` é
            // um campo próprio, disjunto de `self.cliente`, e emprestá-lo aqui
            // é o que deixa o braço novo do `select!` chamar `cliente.par_falhou`
            // sem pedir `self` de novo.
            let resultados_do_par = &mut self.resultados_do_par;

            // Só há o que ler quando há conexão. Sem ela, a espera é o relógio.
            let houve_evento = match self.cliente.as_mut() {
                Some(cliente) => tokio::select! {
                    evento = cliente.next_event() => Some(evento),
                    // Uma tela alheia chegando. O roteador de `Client::connect`
                    // já separou este fluxo dos anexos; aqui ele vira quadros.
                    Some(fluxo) = espera_da_fila(&fila_de_telas) => {
                        escoar_tela_alheia(
                            avisos_da_tela.clone(),
                            fluxo,
                            DeOndeVeioATela::Servidor(Arc::clone(&repasse_da_tela)),
                        );
                        None
                    }
                    // O que uma tarefa solta do caminho entre pares (ver
                    // `Motor::assistir_por_par`) decidiu. Só chega aqui um
                    // `ParFalhou` — ver `ResultadoDoPar` — porque só o laço de
                    // `rodar`, dono do `&mut Client`, pode falar com o servidor.
                    Some(resultado) = resultados_do_par.recv() => {
                        match resultado {
                            ResultadoDoPar::ParFalhou { screen, motivo } => {
                                if let Err(erro) = cliente.par_falhou(screen, motivo).await {
                                    tracing::warn!(
                                        %erro,
                                        ?screen,
                                        "não deu para avisar o servidor que o par falhou"
                                    );
                                }
                            }
                        }
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
                            // Guardado, não perdido: entrar numa sala de voz durante a
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
                        // um operador voltaria, depois de uma queda, para a
                        // sala de voz de onde foi tirado: o motor refaz a última sala de voz
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
                        // A despedida que não é queda — ver `a_sessao_acabou_aqui`.
                        // Lida aqui e agida **depois** do aviso, logo abaixo:
                        // quem desenha a tela precisa da mensagem para saber
                        // dizer «você foi expulso», e um `return` antes dela
                        // trocaria a frase certa por um fim mudo.
                        let despedida = match &mensagem {
                            ServerMessage::Disconnecting { reason }
                                if a_sessao_acabou_aqui(*reason) =>
                            {
                                Some(*reason)
                            }
                            _ => None,
                        };
                        // Antes de o aviso sair, porque a tela é a única coisa
                        // desta casa que **age** sobre uma mensagem em vez de
                        // repassá-la: é aqui que a transmissão ganha nome e a
                        // bomba nasce.
                        self.a_tela_ouviu(&mensagem);
                        let _ = self.avisos.send(Aviso::Mensagem(Box::new(mensagem)));
                        if let Some(reason) = despedida {
                            tracing::info!(?reason, "o servidor acabou com esta sessão");
                            return self.encerrar(Motivo::Moderado(reason));
                        }
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
        let antes = self.bateria.state();
        let acao = self.bateria.poll(agora);
        // **A segunda porta da bateria, e por muito tempo a única sem porteiro.**
        //
        // [`Motor::cair`] trata a queda que o transporte *avisa*: o fluxo
        // devolve erro e o motor larga a conexão inteira, caminho entre pares
        // junto. Mas há outra porta, e é aqui: três `Ping` sem resposta
        // (`Battery::poll_online`) põem a bateria de pé **por dentro**, e
        // devolvem `Action::Wait` — nenhum erro, nenhuma chamada a `cair`.
        //
        // É a porta que uma queda de verdade usa. Um caminho que some sem
        // avisar — NAT que reescreve, rota que cai, a máquina que dorme — não
        // produz erro de fluxo nenhum: produz silêncio, e silêncio é o que os
        // pings contam. Medido em
        // `a_queda_de_uma_conexao_so_derruba_o_caminho_do_par`, com um relé
        // cortado: sem esta linha, a tarefa de par da conexão morta atravessa a
        // bateria **e a reconexão** inteiras, e 1336 quadros da mídia velha
        // chegam à casca depois de `Reconectado` — por um caminho que o servidor
        // novo não montou e não conhece.
        if matches!(antes, Link::Online)
            && matches!(self.bateria.state(), Link::InternalBattery { .. })
        {
            self.soltar_a_conexao();
        }
        match acao {
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

    /// A conexão morreu, e o transporte avisou. Entra na bateria e conta para a
    /// casca.
    fn cair(&mut self) {
        self.soltar_a_conexao();
        let agora = self.inicio.elapsed();
        let antes = self.bateria.state();
        self.bateria.on_connection_lost(agora);
        if antes != self.bateria.state() {
            self.anunciar();
        }
    }

    /// Tudo o que uma conexão perdida leva junto, **sem tocar na bateria**.
    ///
    /// Separado de [`Self::cair`] porque há duas portas para a bateria e as
    /// duas têm de passar por aqui: o erro de fluxo, que chega a `cair`, e três
    /// `Ping` sem resposta, que põem a bateria de pé por dentro de
    /// `Battery::poll` sem erro nenhum — ver o comentário em [`Self::passo`].
    /// Naquela porta a bateria já mudou de estado quando isto corre, e chamar
    /// `on_connection_lost` de novo reiniciaria a contagem dos cinco minutos.
    ///
    /// Idempotente: chamada duas vezes, a segunda não tem o que largar.
    fn soltar_a_conexao(&mut self) {
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
        self.largar_o_caminho_entre_pares();
    }

    /// O caminho entre pares não sobrevive à conexão que o montou.
    ///
    /// # Por que a queda derruba isto, e não só a saída
    ///
    /// [`Motor::encerrar`] não precisa disto: `rodar` devolve logo depois, o
    /// `Motor` é solto, e [`TarefasDePar`] cancela tudo ao ser solto junto.
    /// **[`Motor::cair`] é o caso em que o motor sobrevive ao corte** — a
    /// sessão entra na bateria e volta noutra conexão —, e é o único em que
    /// alguém tem de pedir.
    ///
    /// E tem de pedir porque nada daquele caminho vale para a conexão nova. A
    /// nomeação que pôs estas tarefas de pé foi feita pela sessão que morreu, e
    /// o servidor já a apagou do lado dele (`Pares::saiu`); as tarefas discam
    /// pela ponta QUIC daquele cliente, que morreu com ele. O que sobra é
    /// imagem chegando por um caminho que ninguém mais reconhece.
    ///
    /// # A fila é trocada, e não esvaziada
    ///
    /// `resultados_do_par` não tem fundo, e o braço que o lê em [`Motor::rodar`]
    /// **só existe quando há cliente**: durante a bateria tudo o que as tarefas
    /// disseram fica parado ali. A primeira volta do laço depois da reconexão
    /// entregaria esses relatos à conexão nova — um `ParFalhou` de uma nomeação
    /// que morreu com a sessão anterior, mandando o servidor desfazer um
    /// caminho que ele acabou de montar para a substituta.
    ///
    /// Trocar o par de pontas, em vez de drenar a fila, é o que fecha a corrida
    /// junto: uma tarefa abortada ainda pode escrever entre o pedido de
    /// cancelamento e o `await` em que ela morre, e drenar antes disso deixaria
    /// esse relato passar. Com as pontas trocadas ela escreve para um recebedor
    /// que já não existe, e o `send` falha — que é o que se quer dela.
    ///
    /// O que vier **depois** da queda continua chegando: quem for criado da
    /// reconexão em diante clona a ponta nova.
    fn largar_o_caminho_entre_pares(&mut self) {
        self.tarefas_de_par.largar_tudo();
        // O repasse ao par morre com elas: o `Sender` largado fecha o canal, e
        // quem estivesse escrevendo termina o fluxo direito em vez de ficar
        // parado num canal que ninguém mais alimenta. Sem isto, o punho do
        // repasse atravessaria a queda apontando para um par de outra sessão.
        self.repasse.desligar();
        let (tx, rx) = mpsc::unbounded_channel();
        self.resultados_do_par_tx = tx;
        self.resultados_do_par = rx;
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
                    let _ = cliente.enter_voice_room(voice_room).await;
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
                // A sessão anterior já foi apagada de `Pares` (`Pares::saiu`,
                // no servidor) quando ela caiu — sem redeclarar aqui, esta
                // pessoa reconecta sem identidade nenhuma registrada até a
                // próxima vez que este método for chamado por acaso. Ver o
                // doc de `Motor::declarar_identidade_de_par`.
                //
                // **Com a escolha de emprestar, e não com `false` fixo.** Quem
                // optou por emprestar a subida sairia da malha na primeira
                // queda de rede, sem nada na tela mudando — ver o doc de
                // `Motor::consentimento`.
                self.declarar_identidade_de_par(self.consentimento).await;

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
        // Especial, e antes do empréstimo de `cliente` logo abaixo: declarar
        // identidade precisa do `&mut self` inteiro (identidade **e**
        // cliente — ver `Motor::declarar_identidade_de_par`), e o resto deste
        // método já empresta só `self.cliente`. Os dois empréstimos não
        // convivem no mesmo escopo.
        //
        // **A opção de emprestar é lembrada, e é ela que vai no fio nas duas.**
        // `DeclararIdentidadeDePar` é chamada na conexão e em cada reconexão,
        // e passava `false` fixo; com o comando de emprestar existindo, um
        // `false` fixo aqui desfaria em silêncio, na primeira queda de rede, a
        // escolha que a pessoa fez — ela continuaria na malha na tela dela e
        // fora dela no servidor.
        match comando {
            Comando::DeclararIdentidadeDePar => {
                self.declarar_identidade_de_par(self.consentimento).await;
                return;
            }
            Comando::ConsentirNoCaminhoEntrePares(novo) => {
                self.passar_a_consentir(novo).await;
                return;
            }
            _ => {}
        }
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        let resultado = match comando {
            // Nada a mandar ao servidor: é estado desta máquina.
            Comando::LembrarCaminho(_) => Ok(()),
            Comando::EntrarNaVoiceRoom(voice_room) => cliente.enter_voice_room(voice_room).await,
            Comando::SairDaVoiceRoom => cliente.leave_voice_room().await,
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
            // Tratados acima, antes deste empréstimo de `cliente` — nunca
            // chegam aqui de verdade. `match` continua exaustivo porque o
            // `enum` inteiro é um só, e um braço a menos aqui quebraria a
            // primeira vez que `Comando` ganhasse mais uma variante.
            Comando::DeclararIdentidadeDePar | Comando::ConsentirNoCaminhoEntrePares(_) => Ok(()),

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
                    // **Quem para de assistir para de receber, e o caminho do
                    // par cai junto.** Avisar só o servidor deixava metade: a
                    // tarefa de [`Motor::assistir_por_par`] continuava lendo do
                    // par, e a imagem seguia chegando por baixo do pedido de
                    // parar — gastando a subida de quem empresta com uma janela
                    // fechada. E o `ParFalhou` de rotina que aquele fluxo
                    // acabaria mandando pedia a tela de volta ao servidor.
                    //
                    // `abort` e não um sinal cooperativo: a tarefa passa a vida
                    // parada num `read` do par, e ela não tem ponto onde
                    // conferir um pedido de parada. É por isso que ela é **uma
                    // tarefa só** — ver [`ler_a_tela_alheia`]: enquanto a
                    // leitura acontecia numa segunda tarefa, abortar esta
                    // deixava aquela lendo.
                    self.tarefas_de_par.parar_de_assistir(tela);
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
            // O caminho entre pares: vá buscar esta tela naquele par, e caia
            // para o servidor sem drama se ele não vier. Ver
            // `Motor::assistir_por_par`.
            ServerMessage::AssistaTelaPor {
                screen,
                ref enderecos,
                ref impressao,
            } => {
                self.assistir_por_par(screen, enderecos.clone(), impressao.clone());
            }
            // O caminho entre pares, do outro lado: passe a atender e disque
            // para quem vai me buscar. Ver `Motor::servir_par`.
            ServerMessage::SirvaTelaPara {
                screen,
                ref enderecos,
                ref impressao,
            } => {
                self.servir_par(screen, enderecos.clone(), impressao.clone());
            }
            _ => {}
        }
    }

    // -------------------------------------------------------- caminho entre pares

    /// A ponta QUIC do caminho entre pares — a mesma que já fala com o
    /// servidor, e não uma nova.
    ///
    /// **Achado do fix round 2.** A primeira versão desta função abria um
    /// socket próprio (`crate::client::local_endpoint`), e o §3.1 da spec de
    /// 05/09 proíbe isso com todas as letras: *"Não é escuta nova, socket
    /// novo nem porta nova [...] aquela porta já tem mapeamento de NAT vivo,
    /// mantido pelo `keep_alive_interval` da conexão com o servidor"*. Uma
    /// porta nova tem endereço público **desconhecido do servidor** — ele só
    /// vê a origem da conexão de controle, que é a que `Client::endpoint`
    /// devolve — e o furo simultâneo mirava um endereço onde nada atendia.
    ///
    /// `None` só quando não há conexão viva agora. Não deveria acontecer
    /// quando isto é chamado: só se chega aqui processando uma
    /// `ServerMessage`, que só existe porque há conexão, ou declarando
    /// identidade logo depois de conectar.
    fn ponta_de_pares(&self) -> Option<quinn::Endpoint> {
        self.cliente.as_ref().map(Client::endpoint)
    }

    /// A identidade efêmera desta sessão para o caminho entre pares, gerada na
    /// primeira vez que é necessária e reaproveitada depois.
    ///
    /// # Errors
    ///
    /// [`par::ErroDePar::Certificado`] se o `rcgen` não gerar o certificado.
    fn identidade_de_par(&mut self) -> Result<par::Identidade, par::ErroDePar> {
        if let Some(identidade) = &self.identidade_de_par {
            return Ok(identidade.clone());
        }
        let identidade = par::identidade_efemera()?;
        self.identidade_de_par = Some(identidade.clone());
        Ok(identidade)
    }

    /// Declara ao servidor a identidade efêmera deste par — **mesmo sem optar
    /// por emprestar a subida**.
    ///
    /// # Por que declarar sempre, e não só quando alguém empresta
    ///
    /// Achado do fix round 1 da Task 8: a parede simétrica da Task 5 exige
    /// certificado dos dois lados de toda ligação entre pares, e quem só
    /// assiste (`AssistaTelaPor`) também disca com a própria identidade
    /// quando `SirvaTelaPara` manda alguém procurá-la — ver
    /// [`Motor::assistir_por_par`]. Sem esta declaração, só quem emprestasse
    /// teria impressão registrada no servidor, e a discagem de todo mundo que
    /// só assiste seria recusada como `SemCertificado`. Um ruling meu de
    /// pré-voo misturava «quem eu sou» com «eu empresto»; `crate::par` e
    /// `seele-server/src/pares.rs` documentam a separação inteira.
    ///
    /// # Por que aqui, e não sob demanda na primeira mensagem do caminho
    ///
    /// Por então já seria tarde: o servidor só pode apontar esta pessoa a
    /// quem for procurá-la se a declaração já tiver chegado **antes** de
    /// alguém pedir. Por isso é chamada na conexão inicial e em cada
    /// reconexão — a sessão anterior já foi apagada de `Pares` na saída
    /// (`Pares::saiu`, no servidor), e a identidade efêmera não sobrevive a
    /// ela sem ser dita de novo.
    ///
    /// `locais` só leva os endereços de rede local desta máquina quando
    /// `emprestando` for `true` — **achado do fix round 3, e correção de um
    /// vazamento de privacidade que o round 2 introduziu.** O §5 da spec
    /// nomeia privacidade como a primeira das duas razões independentes do
    /// opt-in — um servidor não é necessariamente entre amigos (ADR 0021) —,
    /// e o round 2 fez esta função publicar a topologia de rede interna de
    /// **toda** máquina que apenas conecta, `emprestando: false` incluído.
    /// Quem só assiste declara a impressão e nada mais: o público que o
    /// servidor já vê na conexão de controle basta para quem empresta discar
    /// de volta, e o furo simultâneo cobre o resto.
    ///
    /// Quem chama passa a escolha da pessoa, guardada em
    /// [`Motor::emprestando`]: `Comando::EmprestarSubida` a muda, e a conexão
    /// inicial e cada reconexão a repetem. Enquanto ninguém optar por
    /// emprestar, `locais_de_pares` nem chega a rodar — ver
    /// [`locais_a_publicar`].
    async fn declarar_identidade_de_par(&mut self, consentimento: ConsentimentoDePar) {
        let identidade = match self.identidade_de_par() {
            Ok(identidade) => identidade,
            Err(erro) => {
                tracing::warn!(%erro, "não deu para gerar a identidade deste par");
                return;
            }
        };
        let Some(ponta) = self.ponta_de_pares() else {
            return;
        };
        let locais = locais_a_publicar(consentimento, || locais_de_pares(&ponta));
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        if let Err(erro) = cliente
            .emprestar_subida(consentimento, par::impressao(&identidade), locais)
            .await
        {
            tracing::warn!(%erro, "não deu para declarar a identidade deste par ao servidor");
        }
    }

    /// Passa a valer este consentimento — e desfaz agora o que ele revoga.
    ///
    /// # Por que a retirada alcança o que já está no ar
    ///
    /// Porque o contrário é a pessoa continuar pagando pela decisão que acabou
    /// de desfazer. Um consentimento que só valesse para o pedido seguinte
    /// deixaria a cópia em curso subindo depois de o interruptor ter sido
    /// desligado, e o único aviso seria a conta de internet dela.
    ///
    /// As duas metades caem em lugares diferentes, porque são coisas
    /// diferentes:
    ///
    /// - **deixar de emprestar a conexão** cancela a tarefa que serve um par,
    ///   e o [`Drop`] de [`VagaDeAtendimento`] devolve a vaga por tabela;
    /// - **deixar de assistir por par** derruba os caminhos abertos. Quem
    ///   reabre o cano do servidor para esta máquina é o **servidor**, ao
    ///   receber a declaração nova — ver `session::devolver_ao_servidor`. Ele
    ///   é quem tem o cano; este lado só desfaz o que é dele.
    ///
    /// **Só o que o consentimento novo revoga.** A declaração é repetida a
    /// cada reconexão, e uma retirada escrita larga demais derrubaria todo
    /// caminho vivo a cada volta da bateria interna.
    async fn passar_a_consentir(&mut self, novo: ConsentimentoDePar) {
        if !novo.empresta_conexao() {
            self.tarefas_de_par.parar_de_servir();
        }
        if !novo.assiste_por_par {
            self.tarefas_de_par.parar_de_assistir_a_tudo();
        }
        self.consentimento = novo;
        self.declarar_identidade_de_par(novo).await;
    }

    /// `ServerMessage::AssistaTelaPor`: vá buscar esta tela naquele par.
    ///
    /// Roda numa tarefa solta porque `par::por_onde` pode levar até
    /// [`PRAZO_DO_PAR`] — bloquear o laço de [`Motor::rodar`] por isso
    /// pausaria a voz, o ping e o resto desta sessão até o par responder ou o
    /// prazo vencer.
    ///
    /// `por_onde` **nunca erra** — essa é a promessa dela, escrita no próprio
    /// doc: quem chama não tem decisão a tomar sobre a falha do par. O que
    /// este método faz com o `PorOndeAssistir::Servidor` que ela devolve —
    /// desde o fix round 1, já com o motivo enumerado dentro — é mandar o
    /// aviso ao servidor pelo canal de [`ResultadoDoPar`]: só o laço de
    /// `rodar`, dono do `&mut Client`, pode falar com ele.
    ///
    /// Disca com a **própria** identidade — a parede simétrica da Task 5 faz
    /// quem atende exigir certificado sempre, e sem uma identidade para
    /// apresentar todo `SirvaTelaPara` real seria recusado como
    /// `SemCertificado`. `Motor::declarar_identidade_de_par` garante que ela
    /// já existe (e já foi dita ao servidor) desde a conexão.
    fn assistir_por_par(
        &mut self,
        screen: ScreenId,
        enderecos: Vec<SocketAddr>,
        impressao: String,
    ) {
        // **O pedido que chegou depois da retirada.** O servidor escolhe e
        // difunde; a retirada desta máquina viaja no sentido contrário. As
        // duas se cruzam no fio, e atender o pedido velho seria discar — e ser
        // discado — com o endereço que esta máquina acabou de tirar de
        // circulação. Quem tem a palavra final é quem paga a conta.
        if !self.consentimento.assiste_por_par {
            tracing::info!(
                ?screen,
                "chegou um pedido para assistir por par depois de este consentimento ter sido \
                 retirado; esta tela continua vindo do servidor"
            );
            return;
        }
        let Some(ponta) = self.ponta_de_pares() else {
            tracing::warn!(
                ?screen,
                "sem conexão com o servidor: não há ponta para o caminho entre pares"
            );
            let _ = self.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
                screen,
                motivo: MotivoDeFalhaDePar::NaoAlcancou,
            });
            return;
        };
        let identidade = match self.identidade_de_par() {
            Ok(identidade) => identidade,
            Err(erro) => {
                tracing::warn!(%erro, ?screen, "não deu para gerar a identidade deste par");
                let _ = self.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
                    screen,
                    motivo: MotivoDeFalhaDePar::NaoAlcancou,
                });
                return;
            }
        };
        let avisos = self.avisos.clone();
        let resultados = self.resultados_do_par_tx.clone();
        let tarefa = tokio::spawn(async move {
            match discar_ate_o_prazo(&ponta, &enderecos, &impressao, &identidade).await {
                par::PorOndeAssistir::Par(ligado) => {
                    // A conta de bytes é de quem empresta — `crate::par::repassar`
                    // escreve por pedaço do lado dele. Do lado de quem assiste
                    // muda uma coisa só, e ela está no doc de
                    // `DeOndeVeioATela`: um fluxo que acaba no meio é o par
                    // tendo caído, e é daqui que o servidor fica sabendo.
                    match tokio::time::timeout(PRAZO_DO_PAR, ligado.conexao.accept_uni()).await {
                        // [`ler_a_tela_alheia`] e não [`escoar_tela_alheia`],
                        // que é a diferença entre a alça valer e não valer:
                        // `escoar` abre uma tarefa nova, e abortar **esta**
                        // deixaria aquela lendo do par. Aqui a leitura acontece
                        // dentro da tarefa que o motor guarda, e é por isso que
                        // o corpo foi separado da tarefa.
                        Ok(Ok(fluxo)) => {
                            ler_a_tela_alheia(
                                avisos,
                                fluxo,
                                DeOndeVeioATela::Par {
                                    screen,
                                    resultados: resultados.clone(),
                                },
                            )
                            .await;
                        }
                        Ok(Err(erro)) => {
                            tracing::warn!(%erro, ?screen, "o par ligou e a transmissão não abriu");
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen,
                                motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                            });
                        }
                        Err(_prazo) => {
                            tracing::warn!(
                                ?screen,
                                "o par ligou e não abriu a transmissão a tempo"
                            );
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen,
                                motivo: MotivoDeFalhaDePar::ParouDeMandar,
                            });
                        }
                    }
                }
                // `por_onde` já classificou o motivo — `ImpressaoNaoBate`
                // continua sendo o evento de segurança que é, agora também
                // para o servidor, e não só no `tracing` local de
                // `classificar` em `par.rs`.
                par::PorOndeAssistir::Servidor(motivo) => {
                    let _ = resultados.send(ResultadoDoPar::ParFalhou { screen, motivo });
                }
            }
        });
        self.tarefas_de_par.assistir(screen, tarefa);
    }

    /// `ServerMessage::SirvaTelaPara`: passe a atender, e disque para o outro
    /// lado — as duas tentativas simultâneas são o furo.
    ///
    /// Como [`Motor::assistir_por_par`], roda solta pela mesma razão de prazo.
    /// Ao contrário dela, quem empresta **nunca** manda `ParFalhou`:
    /// `seele_proto::control::ClientMessage::ParFalhou` é explícita que só
    /// quem recebe manda essa mensagem, porque só quem recebe sabe que a
    /// imagem parou — quem empresta pode ter caído sem chegar a saber de nada.
    fn servir_par(&mut self, screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String) {
        // O mesmo cruzamento, do lado de quem empresta — e aqui o que estava
        // em jogo é a subida desta máquina. Ver [`Motor::assistir_por_par`].
        if !self.consentimento.empresta_conexao() {
            tracing::info!(
                ?screen,
                "chegou um pedido para servir um par depois de este consentimento ter sido \
                 retirado; esta máquina não sobe esta cópia"
            );
            return;
        }
        // **Tomada aqui, e devolvida por quem a segura.** Ver
        // [`VagaDeAtendimento`]: enquanto isto eram duas escritas, a devolução
        // morava na última linha do corpo da tarefa — e um `abort` faz essa
        // linha nunca correr. Como guarda, cada saída antecipada abaixo
        // devolve a vaga ao voltar, sem precisar dizer.
        let Some(vaga) = VagaDeAtendimento::tomar(&self.atendendo_pares) else {
            // No A1 quem empresta serve um par por vez (`par::atender` só
            // aceita uma ligação). Sem vaga, e sem resposta esperada desta
            // mensagem — só o rastro.
            tracing::warn!(
                ?screen,
                "um pedido para servir chegou enquanto este par já servia outro"
            );
            return;
        };
        let Some(ponta) = self.ponta_de_pares() else {
            tracing::warn!(
                ?screen,
                "sem conexão com o servidor: não há ponta para o caminho entre pares"
            );
            return;
        };
        let identidade = match self.identidade_de_par() {
            Ok(identidade) => identidade,
            Err(erro) => {
                tracing::warn!(%erro, ?screen, "não deu para gerar a identidade deste par");
                return;
            }
        };
        self.passar_a_servir(vaga, ponta, identidade, screen, enderecos, impressao);
    }

    /// Põe de pé a tarefa que serve este par, e passa a ser dono dela.
    ///
    /// # Por que separada de [`Motor::servir_par`]
    ///
    /// Porque é a metade que dá para provar. O que fica lá em cima precisa de
    /// um [`Client`] vivo — `ponta_de_pares` sai dele —, e um `Client` precisa
    /// de um servidor do outro lado: nada disso cabe num teste de unidade
    /// deste crate, e é por isso que o resto do caminho entre pares é provado
    /// em `seele-conformance`. **A posse da alça não estava em nenhum dos
    /// dois.** Medido: com `self.tarefas_de_par.servir(tarefa)` trocado por
    /// largar a alça, a suíte inteira continuava verde.
    ///
    /// Daqui para baixo não há `Client` nenhum — só uma ponta QUIC, uma
    /// identidade e endereços —, e é o que
    /// `testes::cair_devolve_a_vaga_de_quem_estava_servindo_um_par` monta à
    /// mão para prender as duas coisas que este método faz: guardar a alça e
    /// entregar a vaga a quem a solta.
    fn passar_a_servir(
        &mut self,
        vaga: VagaDeAtendimento,
        ponta: quinn::Endpoint,
        identidade: par::Identidade,
        screen: ScreenId,
        enderecos: Vec<SocketAddr>,
        impressao: String,
    ) {
        let repasse = Arc::clone(&self.repasse);
        let tarefa = tokio::spawn(async move {
            // **A vaga viaja para dentro da tarefa, e morre com ela.** É o que
            // faz o `abort` de [`TarefasDePar`] devolvê-la: soltar a tarefa
            // solta tudo o que ela segurava, e o guarda é uma dessas coisas.
            let _vaga = vaga;
            let ligado = servir_um_par(ponta, identidade, enderecos, impressao).await;
            match ligado {
                Some(ligado) => {
                    tracing::info!(
                        par = %ligado.conexao.remote_address(),
                        como = ?ligado.como,
                        ?screen,
                        "este par está sendo servido"
                    );
                    repassar_a_tela(&repasse, &ligado, screen).await;
                }
                // As duas tentativas — passar_a_atender+atender e ligar — não
                // deram em nada. Sem `ParFalhou` daqui: é quem assiste que vai
                // notar a falta de imagem e avisar o servidor.
                None => tracing::info!(
                    ?screen,
                    "nenhuma das duas tentativas de servir este par deu certo"
                ),
            }
            // **Devolvida depois do repasse, e não depois da ligação.** A vaga
            // é «estou servindo um par», e servir é o repasse — devolvê-la
            // assim que a conexão fecha deixaria um segundo `SirvaTelaPara`
            // entrar por cima de um repasse em curso, e `crate::par::atender`
            // só tem uma vaga. É onde `_vaga` morre, e é por isso que ele é
            // ligado no topo deste corpo e não perto de `servir_um_par`.
            drop(_vaga);
        });
        // E o motor passa a ser dono dela: sem esta linha a tarefa não tinha
        // alça nenhuma em lugar nenhum, e sobrevivia à sessão inteira
        // repassando a tela a um par por uma conexão que já tinha morrido.
        self.tarefas_de_par.servir(tarefa);
    }
}

/// Repassa a este par a tela que esta máquina está recebendo do servidor.
///
/// **É o último elo do subprojeto A.** As tarefas anteriores fazem dois
/// clientes se ligarem e fazem `crate::par::repassar` saber escrever; esta
/// função é o que liga a tela que chega à ligação que existe — sem ela, quem
/// empresta a subida abre a conexão com o par e não lhe manda byte nenhum, e
/// quem assiste vê exatamente o que veria se a malha não existisse.
///
/// Não devolve nada e não avisa o servidor de falha nenhuma, de propósito:
/// `seele_proto::control::ClientMessage::ParFalhou` é explícita que só quem
/// **recebe** manda essa mensagem. Um repasse que morre no meio é imagem que
/// para do lado de lá, e é de lá que o aviso sai.
async fn repassar_a_tela(repasse: &RepasseDeTela, ligado: &par::ParLigado, screen: ScreenId) {
    // Sem abertura não há o que repassar: ou nenhuma transmissão está chegando
    // agora, ou ela acabou entre o pedido do servidor e a ligação fechar. Um
    // par ligado num fluxo sem cabeçalho não decodifica nada, e mandar-lhe
    // pedaços soltos seria pior que não mandar nada.
    // `abertura_de` e não `abertura`: com duas transmissões no ar, a que está
    // sendo repassada pode não ser a que o servidor mandou servir. Repassar a
    // outra seria entregar ao par uma tela com o nome de outra — ver o doc de
    // `EstadoDoRepasse::qual`.
    let Some(abertura) = repasse.abertura_de(screen) else {
        tracing::warn!(
            ?screen,
            "o par ligou e esta máquina não está recebendo do servidor a transmissão que lhe \
             mandaram repassar; quem assiste continua sendo servido pelo servidor"
        );
        return;
    };
    let (pedacos_tx, pedacos_rx) = mpsc::channel(PEDACOS_A_ESPERA_DO_PAR);
    repasse.ligar(pedacos_tx);
    let resultado = par::repassar(ligado, &abertura, pedacos_rx).await;
    repasse.desligar();
    match resultado {
        Ok(()) => tracing::info!(?screen, "o repasse desta tela ao par terminou"),
        Err(erro) => tracing::warn!(%erro, ?screen, "o repasse desta tela ao par falhou"),
    }
}

/// Disca para o par até o prazo acabar, e não uma vez só.
///
/// # Por que a primeira tentativa pode falhar sem ninguém ter feito nada errado
///
/// O servidor manda `SirvaTelaPara` e `AssistaTelaPor` no mesmo instante, pelo
/// mesmo barramento. Quem empresta só **passa a atender** quando a mensagem
/// dele chega e a tarefa dele roda (`crate::par::passar_a_atender`), e quem
/// assiste disca quando a mensagem dele chega e a tarefa dele roda. Se a
/// segunda vencer a primeira por um milissegundo, a discagem bate numa porta
/// que ainda não abriu — e uma tentativa única leria isso como
/// `NaoAlcancou`, que faria o servidor assumir uma transmissão que o par
/// serviria perfeitamente um instante depois.
///
/// Não é folga inventada: é o mesmo prazo de sempre, [`PRAZO_DO_PAR`],
/// gasto em tentativas de [`TENTATIVA_DE_PAR`] em vez de numa espera só.
///
/// **`ImpressaoNaoBate` não é retentada.** É evento de segurança — alguém
/// respondeu no lugar de quem o servidor apresentou —, e insistir contra quem
/// se faz passar por outro é dar-lhe mais tentativas, não menos.
async fn discar_ate_o_prazo(
    ponta: &quinn::Endpoint,
    enderecos: &[SocketAddr],
    impressao: &str,
    identidade: &par::Identidade,
) -> par::PorOndeAssistir {
    let ate = tokio::time::Instant::now() + PRAZO_DO_PAR;
    let mut ultimo = MotivoDeFalhaDePar::NaoAlcancou;
    while tokio::time::Instant::now() < ate {
        match par::por_onde(
            ponta,
            enderecos,
            impressao.to_owned(),
            Some(identidade),
            TENTATIVA_DE_PAR,
        )
        .await
        {
            par::PorOndeAssistir::Par(ligado) => return par::PorOndeAssistir::Par(ligado),
            par::PorOndeAssistir::Servidor(MotivoDeFalhaDePar::ImpressaoNaoBate) => {
                return par::PorOndeAssistir::Servidor(MotivoDeFalhaDePar::ImpressaoNaoBate);
            }
            par::PorOndeAssistir::Servidor(motivo) => ultimo = motivo,
        }
    }
    tracing::info!(
        ?ultimo,
        ?PRAZO_DO_PAR,
        "o prazo do par acabou sem ligação; a tela vem do servidor"
    );
    par::PorOndeAssistir::Servidor(ultimo)
}

/// Passa a atender e disca para o outro lado, ao mesmo tempo — as duas
/// tentativas simultâneas são o furo de NAT dos dois lados. A primeira que
/// fechar o aperto de mão vence, como em `par::testes::dois_pares_apresentados_se_ligam_pelos_dois_lados`.
///
/// `None` se nenhuma das duas fechar a tempo. Livre e não método de `Motor`
/// porque roda dentro do `tokio::spawn` de [`Motor::servir_par`], depois de o
/// `&mut Motor` já ter sido solto.
async fn servir_um_par(
    ponta: quinn::Endpoint,
    identidade: par::Identidade,
    enderecos: Vec<SocketAddr>,
    impressao: String,
) -> Option<par::ParLigado> {
    if let Err(erro) = par::passar_a_atender(&ponta, identidade.clone(), impressao.clone()) {
        tracing::warn!(%erro, "não deu para pôr esta ponta a atender o par");
        return None;
    }
    let atende = par::atender(ponta.clone(), PRAZO_DO_PAR);
    let disca = par::ligar(
        &ponta,
        &enderecos,
        impressao,
        Some(&identidade),
        PRAZO_DO_PAR,
    );
    let resultado = tokio::select! {
        atendido = atende => atendido,
        // **O fim da discagem não é uma resposta**, e tratá-lo como uma era
        // desistir de servir alguém que estava chegando. Quem assiste nunca
        // chama `par::atender` — só disca —, então a discagem **desta** ponta
        // não tem quem a atenda e não pode fechar. Ela existe por um efeito
        // só, que é metade do §3.3: abrir o mapeamento de NAT deste lado para
        // a discagem do outro entrar. Quando ela erra cedo — família de
        // endereço incompatível, `connect_with` recusando na hora, todos os
        // candidatos falhando rápido —, o braço que a esperava cancelava
        // `atender` e devolvia `None`.
        //
        // O prazo global continua sendo o de `par::atender`, que é o mesmo
        // [`PRAZO_DO_PAR`]: este braço nunca resolve, então é sempre o outro
        // que termina a espera.
        () = discagem_so_pelo_furo(disca) => None,
    };
    // **Desarma sempre, sirva ou não sirva.** Achado do fix round 3: sem
    // isto a ponta continuava aceitando conexões pelo resto da sessão — ver
    // o doc de [`par::parar_de_atender`]. A função já drena e recusa
    // sozinha qualquer sobra da corrida acima antes de desarmar de verdade —
    // ver o doc dela para o pânico que isso evita — porque o perigo mora no
    // `set_server_config(None)` que ela faz, não neste ponto de chamada: um
    // `servir_um_par` de amanhã sem essa linha não devia poder reabri-lo.
    par::parar_de_atender(&ponta).await;
    resultado
}

/// A discagem de quem empresta, que abre o furo e não decide nada.
///
/// **Nunca resolve, de propósito.** Um `select!` que espera esta função espera
/// só o outro braço; o que esta metade faz é manter a discagem viva enquanto
/// [`par::atender`] tem prazo, pelo efeito de abrir o mapeamento de NAT desta
/// ponta. O resultado dela vai para o rastro e para lugar nenhum mais: quem
/// assiste nunca atende, então uma discagem que «deu certo» aqui seria uma
/// surpresa, e uma que falhou é o esperado.
async fn discagem_so_pelo_furo<F>(disca: F)
where
    F: std::future::Future<Output = Result<par::ParLigado, par::ErroDePar>>,
{
    match disca.await {
        // Não é o caminho de produção — quem assiste não atende —, mas se um
        // dia for, largar a conexão aqui é o certo: é `atender` que decide.
        Ok(_) => tracing::debug!("a discagem de quem empresta fechou; quem decide é o atendimento"),
        Err(erro) => {
            tracing::debug!(%erro, "a discagem de quem empresta não fechou, como se espera")
        }
    }
    std::future::pending().await
}

/// Os endereços de rede local desta máquina, na porta que `ponta` já usa.
///
/// **Achado do fix round 2.** Enumeração simples, sem a ordenação por
/// heurística de VPN que `seele-server::alcance::interfaces::descobrir` faz
/// para o convite: aqui não há convite nem degrau de furo a preparar, só uma
/// lista de candidatos que `par::ligar` já tenta todos em paralelo. O ADR
/// 0002 proíbe este crate de depender de `seele-server`, então a pergunta —
/// "quais endereços desta máquina servem para alguém bater neles" — é
/// refeita aqui, com o mesmo crate (`if_addrs`).
///
/// Vazio se a enumeração falhar ou não achar nenhum endereço utilizável: o
/// público que o servidor já vê na conexão de controle continua sobrando
/// como candidato, e a ausência de locais não impede o furo, só tira o atalho
/// de LAN.
///
/// # Sem caminho de produção hoje
///
/// **Nada em `apps/` nem no `seele-ffi` liga o empréstimo**, e esta função só
/// roda quando alguém o liga (ver [`locais_a_publicar`]). O único chamador do
/// caminho inteiro no repositório é o teste de integração
/// `seele-conformance/tests/tela_por_um_par.rs`, que fala com um servidor em
/// memória — então **o atalho de LAN nunca foi exercitado contra uma rede de
/// verdade**. `docs/teste-duas-maquinas.md` diz o mesmo, e este parágrafo
/// existe para que a próxima pessoa não conclua o contrário lendo só o código.
fn locais_de_pares(ponta: &quinn::Endpoint) -> Vec<SocketAddr> {
    let Ok(local) = ponta.local_addr() else {
        return Vec::new();
    };
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|interface| interface.addr.ip())
            .filter(|ip| e_endereco_de_rede_local(*ip))
            .map(|ip| SocketAddr::new(ip, local.port()))
            .collect(),
        Err(erro) => {
            tracing::warn!(%erro, "não deu para enumerar os endereços locais desta máquina");
            Vec::new()
        }
    }
}

/// Quantos pares esta versão do cliente consegue atender ao mesmo tempo.
///
/// **Um, e o número é desta implementação e não do protocolo.**
/// `crate::par::atender` aceita uma ligação por vez e
/// [`VagaDeAtendimento`] é um punho só, então servir dois seria uma promessa
/// que este cliente não tem como cumprir — e quem pagaria por ela seria a
/// segunda pessoa, esperando o prazo do par vencer por uma cópia que nunca
/// chegaria. A casa chama isto de «existir não é funcionar», e é por isso que
/// [`Enlace::consentir_no_caminho_entre_pares`] **baixa** um teto maior em vez
/// de o mandar adiante.
///
/// O campo no fio é um `u8` e o servidor respeita o número que chegar
/// (`seele_server::pares::Pares::escolher`): quando o subprojeto B ensinar
/// este cliente a servir vários, é esta constante que sobe, e nada mais — sem
/// versão nova de protocolo.
pub const PARES_QUE_ESTA_VERSAO_ATENDE: u8 = 1;

/// Baixa um consentimento ao que esta versão do cliente consegue honrar.
///
/// Só o teto muda; `assiste_por_par` atravessa como veio, porque não há nada
/// nesta máquina que o limite. Ver [`PARES_QUE_ESTA_VERSAO_ATENDE`] para o
/// porquê de baixar em vez de mandar adiante.
fn cabivel(consentimento: ConsentimentoDePar) -> ConsentimentoDePar {
    ConsentimentoDePar {
        pares_que_atende: consentimento
            .pares_que_atende
            .min(PARES_QUE_ESTA_VERSAO_ATENDE),
        ..consentimento
    }
}

/// Quais endereços de rede local uma declaração publica.
///
/// **A regra do opt-in, isolada para poder ser presa por teste — achado do
/// fix round 3.** Só quem optou por emprestar publica os endereços da própria
/// máquina: o §5 da spec nomeia privacidade como a primeira das duas razões
/// independentes do opt-in, e lembra que um servidor não é necessariamente
/// entre amigos (ADR 0021). Quem só assiste declara a impressão e nada mais —
/// o público que o servidor já vê na conexão de controle basta para quem
/// empresta discar de volta, e o furo simultâneo cobre o resto.
///
/// # Consentir em assistir por par **não** destranca esta publicação
///
/// E é deliberado. As duas metades do §5 protegem coisas diferentes:
/// `assiste_por_par` decide se o endereço **público** desta máquina — o que o
/// servidor já vê, e o único de que quem empresta precisa — pode ser
/// *entregue* a quem a serve; `empresta_conexao` decide se a **topologia de
/// rede interna** desta máquina é publicada, o que só faz sentido para quem
/// vai ser procurado por vários. Destrancar a segunda com a primeira daria a
/// quem consentiu no menor o custo do maior.
///
/// `todos` é adiado (`FnOnce`) de propósito: enumerar as interfaces desta
/// máquina é trabalho que quem não empresta nem chega a fazer, e um argumento
/// já avaliado esconderia dentro do chamador justamente a decisão que este
/// guarda existe para prender.
///
/// # O ramo `true` não tem caminho de produção hoje
///
/// **Nada em `apps/` nem no `seele-ffi` chama `Enlace::emprestar_subida`**, e
/// sem isso `emprestando` é sempre `false` em produção: o ramo que publica
/// endereços só roda no teste de integração
/// `seele-conformance/tests/tela_por_um_par.rs`. O guarda do opt-in está preso
/// por teste; o que não foi exercitado é o **caminho de LAN** que ele
/// destranca. Ver `docs/teste-duas-maquinas.md`, que registra o mesmo.
fn locais_a_publicar<F: FnOnce() -> Vec<SocketAddr>>(
    consentimento: ConsentimentoDePar,
    todos: F,
) -> Vec<SocketAddr> {
    if consentimento.empresta_conexao() {
        todos()
    } else {
        Vec::new()
    }
}

/// Se este endereço vale como "local" para o caminho entre pares.
///
/// Loopback e não especificado não saem desta máquina; link-local
/// (`169.254.0.0/16`, `fe80::/10`) não sai do cabo — o mesmo motivo que
/// `seele-server::alcance::interfaces::descobrir` já documenta para o
/// convite. Tudo o mais entra, inclusive um endereço público diretamente
/// atribuído a uma interface: um duplicado do que o servidor já vê não faz
/// mal, `Pares::declarou` já lida com isso.
fn e_endereco_de_rede_local(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(),
        IpAddr::V6(v6) => !v6.segments().first().is_some_and(|s| s & 0xffc0 == 0xfe80),
    }
}

/// Quantos pedaços de tela ficam à espera de sair para o par.
///
/// **Um número, e a razão de ele existir é o que se faz quando ele estoura.**
/// Quem repassa está assistindo à mesma tela, e a leitura dele não pode
/// esperar a escrita para o par: um par que parou de ler prenderia a imagem de
/// quem empresta, que é o oposto de «a malha é alívio». Cheio, o repasse é
/// **desligado inteiro** — nunca é descartado um pedaço no meio, porque um
/// buraco no fluxo desloca o enquadramento de quem recebe para sempre.
///
/// # Por qual mecanismo quem assiste volta ao servidor
///
/// Desligado o destino, o `Sender` cai, [`crate::par::repassar`] termina o
/// fluxo do par **direito** — e um fluxo que termina direito não é um erro do
/// outro lado. Por isso o fim limpo de um fluxo de par é reportado como
/// `ClientMessage::ParFalhou { motivo: ParouDeMandar }` em
/// [`escoar_tela_alheia`], e não só o fim torto: é esse relato que faz o
/// servidor religar o cano de quem assiste. Sem ele, esta constante estourar
/// seria tela em branco permanente — quem assiste já saiu do cano do servidor
/// desde que o par foi apontado, e nada o recolocaria lá.
///
/// Trinta e dois pedaços são cerca de um segundo a trinta quadros por segundo,
/// que é muito mais do que uma escrita para um par saudável leva e pouco o
/// bastante para a memória não crescer sem limite atrás de um par doente.
const PEDACOS_A_ESPERA_DO_PAR: usize = 32;

/// A tela que chega do servidor, aberta para quem a repassa a um par.
///
/// # Por que um lugar partilhado, e não um argumento
///
/// As duas pontas desta ligação nascem em momentos diferentes e vivem em
/// tarefas diferentes. A tela alheia chega quando quem compartilha abre o
/// fluxo, e é lida por [`escoar_tela_alheia`], numa tarefa própria que dura o
/// que a transmissão durar. O pedido para servir um par chega depois — pode
/// chegar muito depois — como `ServerMessage::SirvaTelaPara`, e é atendido por
/// outra tarefa ([`Motor::servir_par`]). Nenhuma das duas pode ser argumento
/// da outra; o que as liga é este ponto de encontro, que o [`Motor`] cria uma
/// vez e empresta às duas.
///
/// # O que ele guarda
///
/// A **abertura** da transmissão que está chegando — os bytes crus do
/// cabeçalho, que [`crate::par::repassar`] escreve tal e qual —, e o
/// **destino** dos pedaços, quando há um par sendo servido. Sem abertura não
/// há o que repassar: um par ligado no meio de uma transmissão cujo cabeçalho
/// ele nunca viu não decodifica nada.
///
/// `std::sync::Mutex` e não o do `tokio`: nada aqui dentro espera por nada, e
/// os dois punhos são segurados por microssegundos. Um mutex assíncrono aqui
/// só acrescentaria pontos de suspensão a caminhos que não os têm.
#[derive(Debug, Default)]
struct RepasseDeTela {
    /// Tudo sob um punho só. Ver o doc de [`EstadoDoRepasse`].
    estado: std::sync::Mutex<EstadoDoRepasse>,
}

/// O que o repasse guarda, e por que num punho só.
///
/// **Porque as três coisas se decidem juntas.** «Estes bytes vão para o par?»
/// é uma pergunta sobre a tela em curso *e* sobre haver destino; separada em
/// dois punhos, ela é respondida em dois instantes, e entre os dois a
/// transmissão pode ter trocado. Um quadro copiado para o fluxo de outra tela
/// não dá erro em lugar nenhum — dá duas telas fundidas numa só na janela de
/// quem assiste, que é o defeito que este guarda existe para impedir.
#[derive(Debug, Default)]
struct EstadoDoRepasse {
    /// De qual transmissão é o repasse em curso.
    ///
    /// **É a identidade que faltava, e ela é uma só de propósito.** Havia um
    /// [`RepasseDeTela`] por [`Motor`] e nenhuma marca de tela: com duas
    /// transmissões no ar na mesma sala — o cenário do §0 do desenho —, a
    /// segunda `abriu()` sobrescrevia a primeira, os quadros das duas entravam
    /// intercalados no mesmo fluxo do par, e o primeiro `fechou()` matava o
    /// repasse da outra. Quem recebia rotulava tudo com o `screen` do fluxo e
    /// via as duas telas fundidas, sem um erro em lugar nenhum.
    ///
    /// **A versão que repassa as duas é do subprojeto B**, e não cabe aqui:
    /// ela exige uma marca de tela no fio entre pares — mudança de protocolo,
    /// que é a fundação daquele subprojeto. O que cabe hoje é a honestidade:
    /// uma tela por vez, dito em voz alta, com quem assiste a outra
    /// continuando a receber do servidor pelo caminho de sempre.
    ///
    /// A identidade é o [`ScreenId`] e **não** o dono da transmissão: este
    /// lado não sabe de quem é uma tela alheia. `ScreenHeader` não carrega
    /// pessoa e [`Aviso::TelaAbriu`] também não — e não precisa carregar: o
    /// `ScreenId` é atribuído pelo servidor e é único por transmissão, que é
    /// exatamente a pergunta que este campo responde.
    qual: Option<ScreenId>,
    /// Os bytes crus do cabeçalho da transmissão que chega agora do servidor.
    abertura: Option<Vec<u8>>,
    /// Para onde copiar cada quadro, enquanto há um par a servir.
    destino: Option<mpsc::Sender<Vec<u8>>>,
}

impl RepasseDeTela {
    /// Uma transmissão do servidor abriu com este cabeçalho.
    ///
    /// **A segunda tela não assume.** Se já há um repasse em curso de outra
    /// transmissão, esta é recusada e o `warn!` diz por quê — ver o doc de
    /// [`EstadoDoRepasse::qual`]. Quem assiste à tela recusada continua
    /// recebendo do servidor, que é o caminho de sempre: nenhuma imagem se
    /// perde, e nada se funde em silêncio.
    fn abriu(&self, tela: ScreenId, abertura: Vec<u8>) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if let Some(em_curso) = estado.qual {
            if em_curso != tela {
                tracing::warn!(
                    %em_curso,
                    nova = %tela,
                    "só uma tela por vez é repassada a um par nesta versão; esta segunda \
                     transmissão continua vindo do servidor para quem a assiste"
                );
                return;
            }
        }
        estado.qual = Some(tela);
        estado.abertura = Some(abertura);
    }

    /// A transmissão do servidor acabou: não há mais o que repassar, e o par
    /// que estava sendo servido vê o fluxo terminar direito.
    ///
    /// **Só se for a que está sendo repassada.** Sem esta conferência, o fim
    /// de uma segunda transmissão — que nunca chegou a ser repassada —
    /// derrubaria o repasse da primeira.
    fn fechou(&self, tela: ScreenId) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if estado.qual != Some(tela) {
            return;
        }
        estado.qual = None;
        estado.abertura = None;
        estado.destino = None;
    }

    /// O cabeçalho da transmissão que está chegando, se a que chega é esta.
    ///
    /// `None` quando o repasse em curso é de outra tela: servir o pedido do
    /// servidor com a abertura da tela errada seria entregar ao par uma
    /// transmissão com o nome de outra.
    fn abertura_de(&self, tela: ScreenId) -> Option<Vec<u8>> {
        let estado = self.estado.lock().ok()?;
        if estado.qual != Some(tela) {
            return None;
        }
        estado.abertura.clone()
    }

    /// Passa a copiar os pedaços para aqui.
    fn ligar(&self, destino: mpsc::Sender<Vec<u8>>) {
        if let Ok(mut estado) = self.estado.lock() {
            estado.destino = Some(destino);
        }
    }

    /// Para de copiar. O `Sender` largado fecha o canal, e
    /// [`crate::par::repassar`] termina o fluxo do par direito.
    fn desligar(&self) {
        if let Ok(mut estado) = self.estado.lock() {
            estado.destino = None;
        }
    }

    /// Copia mais um quadro para o par, se há um sendo servido.
    ///
    /// **Nunca espera, e nunca descarta um pedaço só.** Ver o doc de
    /// [`PEDACOS_A_ESPERA_DO_PAR`].
    ///
    /// **E só os da tela em curso.** Um quadro de outra transmissão entrando
    /// neste fluxo é o que fundia duas telas numa só.
    fn pedaco(&self, tela: ScreenId, bytes: Vec<u8>) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if estado.qual != Some(tela) {
            return;
        }
        let Some(destino) = estado.destino.as_ref() else {
            return;
        };
        match destino.try_send(bytes) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    quantos = PEDACOS_A_ESPERA_DO_PAR,
                    "o par parou de aceitar bytes; o repasse para ele é desligado"
                );
                estado.destino = None;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => estado.destino = None,
        }
    }
}

/// De onde uma transmissão de tela alheia está chegando.
///
/// **As duas pontas leem o mesmo formato e fazem coisas diferentes com o fim
/// dele**, e é isso que este `enum` carrega. Do servidor, o fim é a
/// transmissão acabando, e os pedaços do meio podem interessar a um par que
/// esta máquina sirva. De um par, o fim no meio é o par tendo caído — e quem
/// assiste é a **única** pessoa que sabe disso (ver o doc de
/// `ClientMessage::ParFalhou`), então é daqui que sai o aviso ao servidor.
///
/// # O byte de tipo, que só um dos dois já leu
///
/// Do servidor, o fluxo chega pelo roteador de `Client::connect`, que lê o
/// byte de tipo para saber para qual fila mandar o fluxo — então este lado
/// continua de onde ele parou (`Recepcao::do_fluxo_ja_tipado`). De um par não
/// há roteador nenhum: `crate::par::repassar` escreve o byte de tipo como
/// primeiro byte do fluxo, e quem lê tem de lê-lo (`Recepcao::do_fluxo`).
/// Trocar os dois é ler o primeiro byte do cabeçalho como se fosse o tipo, e
/// o cabeçalho inteiro sai deslocado.
enum DeOndeVeioATela {
    /// Do servidor. Os pedaços são copiados para o par que esta máquina serve,
    /// se ela estiver servindo algum.
    Servidor(Arc<RepasseDeTela>),
    /// De um par. Uma queda no meio vira `ParFalhou`.
    Par {
        /// Qual transmissão.
        screen: ScreenId,
        /// Por onde avisar o laço de [`Motor::rodar`], que é quem fala com o
        /// servidor.
        resultados: mpsc::UnboundedSender<ResultadoDoPar>,
    },
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
///
/// `de_onde` diz por onde a transmissão veio, e as duas coisas que dependem
/// disso — quem lê o byte de tipo, e o que significa o fluxo acabar no meio —
/// estão no doc de [`DeOndeVeioATela`].
fn escoar_tela_alheia(
    avisos: mpsc::UnboundedSender<Aviso>,
    fluxo: quinn::RecvStream,
    de_onde: DeOndeVeioATela,
) {
    tokio::spawn(ler_a_tela_alheia(avisos, fluxo, de_onde));
}

/// O corpo de [`escoar_tela_alheia`], sem a tarefa.
///
/// **Separado para que o caminho do par caiba numa tarefa só.**
/// [`Motor::assistir_por_par`] já roda solta — discar pode levar
/// [`PRAZO_DO_PAR`] —, e a leitura acontece dentro dela. Enquanto a leitura
/// abria uma **segunda** tarefa, a alça que o motor guardava não alcançava a
/// parte que importa: abortar a de fora deixava a de dentro lendo do par, e
/// quem tinha pedido para parar de assistir continuava recebendo imagem por
/// baixo. Uma tarefa só é o que faz a alça valer.
async fn ler_a_tela_alheia(
    avisos: mpsc::UnboundedSender<Aviso>,
    fluxo: quinn::RecvStream,
    de_onde: DeOndeVeioATela,
) {
    {
        {
            let aberto = match &de_onde {
                DeOndeVeioATela::Servidor(_) => {
                    crate::tela::Recepcao::do_fluxo_ja_tipado(fluxo).await
                }
                DeOndeVeioATela::Par { .. } => crate::tela::Recepcao::do_fluxo(fluxo).await,
            };
            let mut recepcao = match aberto {
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
                    // Um par que abre um fluxo ilegível é um par que não está
                    // servindo nada. Sem este aviso, quem assiste esperaria a
                    // imagem de alguém que nunca a vai mandar, e o servidor
                    // nunca saberia que tem de assumir.
                    if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                        let _ = resultados.send(ResultadoDoPar::ParFalhou {
                            screen: *screen,
                            motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                        });
                    }
                    return;
                }
            };
            let cabecalho = *recepcao.cabecalho();
            let tela = cabecalho.screen;
            if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                repasse.abriu(tela, recepcao.abertura().to_vec());
            }
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
                        // **A cópia para o par sai daqui, antes de qualquer
                        // decisão sobre o que a casca desenha.** Os bytes
                        // repassados são os mesmos que chegaram, na mesma
                        // ordem, som incluído: quem recebe do par tem de ver a
                        // mesma transmissão que quem recebe do servidor, e
                        // filtrar aqui produziria duas telas diferentes com o
                        // mesmo nome.
                        if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                            repasse.pedaco(tela, quadro.no_fio());
                        }
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
                    Ok(None) => {
                        // **O fim limpo também é um `ParFalhou`.** Quem
                        // assiste não tem como distinguir «a transmissão
                        // acabou» de «o par calou»: as duas chegam como um
                        // fluxo que termina sem erro. E há três caminhos que
                        // terminam limpo com a transmissão ainda no ar — a
                        // contrapressão de [`PEDACOS_A_ESPERA_DO_PAR`], quem
                        // empresta reconectando ao servidor, e quem empresta
                        // saindo da sala. Calar aqui é tela em branco
                        // permanente, porque o cano do servidor para esta
                        // pessoa foi desligado quando o par foi apontado.
                        //
                        // Então reporta sempre, e deixa o **servidor**
                        // adjudicar: ele é o único que sabe se a transmissão
                        // ainda existe, e o braço de `ParFalhou` dele já não
                        // faz nada quando ela acabou de verdade.
                        //
                        // `ParouDeMandar` e não `CaiuNoMeio`: nada caiu. O par
                        // fechou o fluxo direito e simplesmente não manda
                        // mais. Nenhum dos dois desacredita ninguém
                        // (`crate::pares::quem_desacreditar`, no servidor), e
                        // o motivo é o que fica no rastro.
                        if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen: *screen,
                                motivo: MotivoDeFalhaDePar::ParouDeMandar,
                            });
                        }
                        break;
                    }
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
                        // **Do par, isto é o par tendo caído** — e quem
                        // assiste é a única pessoa que sabe. É este aviso que
                        // faz o servidor assumir, e sem ele a promessa da spec
                        // («ninguém perde imagem por causa da máquina de outra
                        // pessoa») ficaria escrita e não cumprida: a tela
                        // congelaria e ninguém do outro lado ficaria sabendo.
                        if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen: *screen,
                                motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                            });
                        }
                        break;
                    }
                }
            }
            if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                repasse.fechou(tela);
            }
            let _ = avisos.send(Aviso::TelaFechou { tela });
        }
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
    /// desta pessoa?** O quadro sai do barramento do servidor para a sala de voz inteira,
    /// e sem ela quem apenas assiste ligaria a captura da própria tela ao ver
    /// outro começar. As outras duas — já há uma viva, e o pedido existe — moram
    /// em [`Self::nascer_a_tela`], porque valem também para quem o chama de um
    /// teste.
    ///
    /// **A `Bomba` não vai para uma bomba a mais quando o servidor reenvia.** Ele
    /// reenvia `ScreenShareStarted` a cada pessoa que entra numa sala de voz onde já há
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
            Comando::EntrarNaVoiceRoom(voice_room) => self.voice_room = Some(*voice_room),
            Comando::SairDaVoiceRoom => self.voice_room = None,
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

    /// Toda despedida do protocolo tem de escolher um lado, e escolher **aqui**.
    ///
    /// # O que este teste prova, e o que ele não prova
    ///
    /// Ele **não** prova comportamento: o que segura a expulsão é
    /// `expulsar_acaba_com_a_sessao_e_deixa_voltar`, em `seele-conformance`, e
    /// esse teste foi provado por reversão.
    ///
    /// O que ele prova é contra deriva: o `match` abaixo é exaustivo sem braço
    /// `_`, então uma variante nova de [`DisconnectReason`] **não compila** até
    /// alguém dizer de que lado ela cai. Sem isto ela cairia calada no lado de
    /// reconectar, que é onde estava o defeito que este conserto fechou.
    ///
    /// # Retificação de 10/09
    ///
    /// A versão anterior deste doc dizia que, com esta função devolvendo `true`
    /// para tudo, «a suíte inteira do workspace continua verde». **É falso, e
    /// este próprio teste é quem desmente**: ele não confere a tabela contra si
    /// mesma — escreve a decisão esperada variante a variante, à parte da
    /// função, e compara. Sob aquela mutação a primeira variante da lista já
    /// derruba o `assert_eq!` abaixo: `Incompatible`, esperado `false`, volta
    /// `true`. Medido às 05:51 de 10/09 e remedido na candidata de 496ba8c5.
    ///
    /// A lacuna real era outra, e mais estreita: **nenhum teste de
    /// comportamento** segurava o lado de reconectar da fronteira, com um
    /// servidor de verdade escrevendo a despedida no fio. Quem a fechou foi
    /// `uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao`, em
    /// `seele-conformance/tests/bateria_interna.rs`. O relatório histórico de
    /// 10/09 não foi reescrito; a retificação está no complemento de 496ba8c5.
    #[test]
    fn toda_despedida_do_protocolo_escolhe_um_lado() {
        // `DisconnectReason::` escrito por extenso e sem `_`: é o `match` que
        // fica vermelho, e não uma lista que alguém esqueceria de atualizar.
        for motivo in [
            DisconnectReason::Incompatible,
            DisconnectReason::CredentialRejected,
            DisconnectReason::HandshakeTimeout,
            DisconnectReason::Kicked,
            DisconnectReason::Banned,
            DisconnectReason::ServerFull,
            DisconnectReason::ScheduledMaintenance,
            DisconnectReason::ServerShuttingDown,
            DisconnectReason::Timeout,
            DisconnectReason::ProtocolViolation,
            DisconnectReason::RateLimited,
            DisconnectReason::FellBehind,
            DisconnectReason::AdmissionPending,
            DisconnectReason::AdmissionDenied,
            DisconnectReason::NicknameTaken,
        ] {
            let acaba = match motivo {
                // Alguém decidiu que esta pessoa não fica. Reconectar desfaz.
                DisconnectReason::Kicked | DisconnectReason::Banned => true,
                // A bateria é o conserto: o servidor volta, ou o cliente
                // reconecta e busca o histórico que faltou. O doc de
                // `FellBehind` diz isso com todas as letras.
                DisconnectReason::ScheduledMaintenance
                | DisconnectReason::ServerShuttingDown
                | DisconnectReason::Timeout
                | DisconnectReason::FellBehind
                | DisconnectReason::RateLimited
                | DisconnectReason::ProtocolViolation => false,
                // Nunca chegam ao motor: a portaria as devolve como
                // `ConnectError::Refused`, e não há sessão para acabar.
                DisconnectReason::Incompatible
                | DisconnectReason::CredentialRejected
                | DisconnectReason::HandshakeTimeout
                | DisconnectReason::ServerFull
                | DisconnectReason::AdmissionPending
                | DisconnectReason::AdmissionDenied
                | DisconnectReason::NicknameTaken => false,
            };
            assert_eq!(
                a_sessao_acabou_aqui(motivo),
                acaba,
                "{motivo:?} mudou de lado sem que este teste mudasse junto"
            );
        }
    }

    #[test]
    fn so_quem_empresta_publica_os_enderecos_da_propria_maquina() {
        // **O guarda do achado de privacidade do fix round 3.** O round 2
        // passou a publicar os endereços de interface de *todo* cliente,
        // inclusive de quem não empresta — contra o §5 da spec, que nomeia
        // privacidade como a primeira das duas razões do opt-in.
        //
        // **E consentir em assistir por par não basta**, que é a metade nova:
        // aquele consentimento decide se o endereço **público** desta máquina
        // pode ser entregue a quem a serve, e não se a topologia interna dela
        // é publicada. Destrancar a segunda com a primeira daria a quem
        // consentiu no menor o custo do maior — ver o doc desta função.
        //
        // Os endereços são inventados de propósito: a decisão não pode
        // depender de quais interfaces a máquina que roda o teste tem, senão
        // o guarda passa a valer só onde há uma LAN.
        let da_maquina = || vec![SocketAddr::from(([192, 168, 1, 10], 4444))];
        for calado in [
            ConsentimentoDePar::de_ninguem(),
            ConsentimentoDePar {
                pares_que_atende: 0,
                assiste_por_par: true,
            },
        ] {
            assert!(
                locais_a_publicar(calado, da_maquina).is_empty(),
                "quem não empresta ({calado:?}) publicou a topologia de rede interna da máquina"
            );
        }
        assert_eq!(
            locais_a_publicar(
                ConsentimentoDePar {
                    pares_que_atende: 1,
                    assiste_por_par: false,
                },
                da_maquina
            ),
            da_maquina(),
            "quem optou por emprestar deixou de publicar por onde ser alcançado"
        );
    }

    #[test]
    fn enderecos_locais_excluem_loopback_nao_especificado_e_link_local() {
        // A parte que `locais_de_pares` não pode errar: publicar loopback ou
        // um não-especificado não ajuda ninguém a discar, e link-local não
        // sai do cabo — mesmo motivo que
        // `seele-server::alcance::interfaces::descobrir` já documenta para o
        // convite.
        let locais = [IpAddr::from([192, 168, 1, 10]), IpAddr::from([10, 0, 0, 5])];
        for ip in locais {
            assert!(e_endereco_de_rede_local(ip), "{ip} devia contar como local");
        }
        let excluidos = [
            IpAddr::from([127, 0, 0, 1]),
            IpAddr::from([0, 0, 0, 0]),
            IpAddr::from([169, 254, 1, 1]),
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            IpAddr::V6(std::net::Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        ];
        for ip in excluidos {
            assert!(
                !e_endereco_de_rede_local(ip),
                "{ip} não devia contar como local"
            );
        }
    }

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

        motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2)));
        motor.lembrar(&Comando::AbrirLinha(ChannelId(7)));
        motor.lembrar(&Comando::Muted(true));
        motor.lembrar(&Comando::Isolamento(true));

        assert_eq!(motor.voice_room, Some(VoiceRoomId(2)));
        assert_eq!(motor.linha, Some(ChannelId(7)));
        assert!(motor.muted);
        assert!(motor.isolamento);

        // Ejetar não é uma queda: quem saiu da sala de voz não volta para ele.
        motor.lembrar(&Comando::SairDaVoiceRoom);
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
        motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2)));

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
        // O servidor reenvia `ScreenShareStarted` a cada pessoa que entra numa sala de voz
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
        let (resultados_do_par_tx, resultados_do_par) = mpsc::unbounded_channel();
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
            identidade_de_par: None,
            atendendo_pares: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            consentimento: ConsentimentoDePar::de_ninguem(),
            repasse: Arc::new(RepasseDeTela::default()),
            resultados_do_par,
            resultados_do_par_tx,
            tarefas_de_par: TarefasDePar::default(),
        }
    }

    /// **Uma tarefa de par não sobrevive ao motor que a criou.**
    ///
    /// # A alça largada não aborta nada
    ///
    /// `Motor::caminhos_de_par` guardava `JoinHandle`s num mapa comum, e
    /// **largar um `JoinHandle` desprende a tarefa — não a cancela**. Os dois
    /// caminhos que matam este motor largam o mapa sem tocá-lo:
    /// `Enlace::drop` aborta a tarefa de `rodar`, e um `Comando::Sair` a faz
    /// voltar; nos dois o `Motor` é solto, o mapa vai junto, e cada tarefa de
    /// par fica viva — lendo do par, com a conexão QUIC de pé, para uma sessão
    /// que já acabou.
    ///
    /// O que este teste prende é a **posse**: quem for dono das alças tem de
    /// abortá-las ao ser solto, e não confiar em ninguém se lembrar de pedir.
    /// A tarefa aqui é um contador que não para sozinho nunca — se ele
    /// continuar subindo depois de o motor sumir, é porque ninguém a cancelou.
    #[tokio::test(flavor = "multi_thread")]
    async fn destruir_o_motor_aborta_as_tarefas_de_par_que_ele_guarda() {
        let mut motor = motor_de_teste();

        let batidas = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let contando = Arc::clone(&batidas);
        let tarefa = tokio::spawn(async move {
            loop {
                contando.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
        motor.tarefas_de_par.assistir(ScreenId(1), tarefa);

        // Andar de verdade antes do corte: um contador parado em zero passaria
        // este teste sem que tarefa nenhuma tivesse chegado a existir.
        ate_que(
            "a tarefa de par dar sinal de vida antes de o motor ser destruído",
            Duration::from_secs(5),
            || batidas.load(std::sync::atomic::Ordering::Relaxed) > 0,
        )
        .await;

        drop(motor);

        // `abort` é um pedido, não um corte instantâneo: o runtime solta a
        // tarefa no próximo toque nela. A margem é para isso, e não para
        // esconder um cancelamento que não aconteceu — o que decide é o
        // contador ter **parado**, não onde ele parou.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let parou_em = batidas.load(std::sync::atomic::Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let depois = batidas.load(std::sync::atomic::Ordering::Relaxed);

        assert_eq!(
            depois, parou_em,
            "a tarefa de par continuou correndo depois de o motor ser destruído (subiu de \
             {parou_em} para {depois}): a alça foi largada em vez de abortada, e quem fechou a \
             sessão continua lendo do par com a conexão de pé"
        );
    }

    /// **Abortar quem servia um par devolve a vaga, em vez de prendê-la.**
    ///
    /// # O risco que o conserto vizinho criou
    ///
    /// Cancelar as tarefas de par fecha um defeito e abre outro. A vaga de
    /// `Motor::atendendo_pares` era devolvida na **última linha** do corpo da
    /// tarefa que serve — e um `abort` faz a última linha nunca correr. Uma
    /// vaga que não volta é esta máquina fora da malha para sempre e **sem erro
    /// em lugar nenhum**: o servidor continua apontando este par, porque a vaga
    /// dele voltou na queda, e o cliente recusa cada pedido em silêncio pela
    /// vaga que ficou. Ninguém do outro lado fica sabendo; só a imagem para de
    /// aliviar.
    ///
    /// É por isso que a vaga virou [`VagaDeAtendimento`], um punho com `Drop`:
    /// soltar a tarefa solta o guarda, e soltar o guarda devolve a vaga.
    ///
    /// # O que é de produção aqui, e o que não é
    ///
    /// De produção: quem toma a vaga (`VagaDeAtendimento::tomar`), quem põe a
    /// tarefa de pé e guarda a alça (`Motor::passar_a_servir`) e quem corta
    /// (`Motor::cair`). Montado à mão: só o que `Motor::servir_par` tiraria de
    /// um `Client` — a ponta QUIC, a identidade e o endereço do outro lado —,
    /// porque um `Client` precisa de um servidor e isso mora em
    /// `seele-conformance`.
    ///
    /// # Por que o prazo curto é o discriminador
    ///
    /// O alvo é um **buraco negro**: uma ponta bindada de verdade, sem
    /// `ServerConfig` nenhum, que nunca responde — o mesmo truque de
    /// [`servir_um_par_desarma_a_ponta_de_quem_empresta_depois_de_servir`], e
    /// pela mesma razão de não arriscar um `ECONNREFUSED` rápido do sistema.
    /// Contra ele `servir_um_par` gasta [`PRAZO_DO_PAR`] inteiro — três
    /// segundos — antes de desistir e devolver a vaga pelo fim do corpo.
    ///
    /// Então a vaga voltar **dentro de um segundo** só pode ter vindo do
    /// cancelamento, e não do fim natural. É esse intervalo que separa o
    /// conserto do defeito, e é por isso que a espera aqui tem prazo próprio em
    /// vez do de [`ate_que`]: sem a alça guardada não há o que abortar, e sem o
    /// `Drop` do guarda o `abort` prende a vaga para sempre.
    #[tokio::test(flavor = "multi_thread")]
    async fn cair_devolve_a_vaga_de_quem_estava_servindo_um_par() {
        let mut motor = motor_de_teste();
        let punho = Arc::clone(&motor.atendendo_pares);

        let vaga = VagaDeAtendimento::tomar(&punho).expect("a vaga nasce livre");
        assert!(
            punho.load(std::sync::atomic::Ordering::Acquire),
            "tomar a vaga não a marcou como tomada, e o resto deste teste não mede nada"
        );
        assert!(
            VagaDeAtendimento::tomar(&punho).is_none(),
            "a vaga foi tomada duas vezes: `crate::par::atender` só aceita uma ligação, e um \
             segundo `SirvaTelaPara` entraria por cima de um repasse em curso"
        );

        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let buraco_negro = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde_ninguem_atende = buraco_negro.local_addr().unwrap();
        let identidade = par::identidade_efemera().unwrap();
        let impressao = par::impressao(&identidade);
        motor.passar_a_servir(
            vaga,
            ponta,
            identidade,
            ScreenId(1),
            vec![onde_ninguem_atende],
            impressao,
        );

        motor.cair();

        ate_que(
            "a vaga de quem servia um par voltar depois da queda, antes de `PRAZO_DO_PAR`",
            Duration::from_secs(1),
            || !punho.load(std::sync::atomic::Ordering::Acquire),
        )
        .await;

        // E a prova que importa: a vaga não voltou só no papel. A conexão que
        // substitui a que caiu precisa **conseguir tomá-la**, senão esta
        // máquina reconecta viva e fora da malha.
        assert!(
            VagaDeAtendimento::tomar(&punho).is_some(),
            "a vaga voltou a `false` e mesmo assim não pôde ser tomada de novo"
        );
    }

    /// **A reativação tardia, e ela não é hipótese.** O servidor escolhe um
    /// par, difunde `AssistaTelaPor` pelo barramento, e só então a retirada de
    /// consentimento desta máquina chega lá. As duas se cruzam no fio, e sem o
    /// guarda o pedido velho seria atendido: esta máquina discaria — e seria
    /// discada — com o endereço que acabou de tirar de circulação.
    ///
    /// # Por que a asserção é sobre o relato, e não sobre a alça
    ///
    /// **Porque a alça não distingue nada, e a primeira versão deste teste
    /// passava sem o guarda.** Um `Motor` de unidade não tem `Client`, então
    /// `ponta_de_pares` devolve `None` e `assistir_por_par` sai cedo de
    /// qualquer maneira — `caminhos` fica vazio nos dois casos, e o teste
    /// media um estado idêntico chamando-o de prova. Medido revertendo o
    /// guarda: a suíte continuava verde.
    ///
    /// O que **de fato** difere é o relato. A saída por falta de ponta manda
    /// `ParFalhou { NaoAlcancou }` ao servidor; a saída por consentimento
    /// retirado não manda nada, e não deve mandar — nada falhou na rede, esta
    /// máquina é que recusou. Um relato de falha aqui seria o produto dizendo
    /// ao servidor uma coisa que não aconteceu.
    /// **Quem pede precisa saber o que valeu, e não o que pediu.**
    ///
    /// O teto desta versão do cliente é um par
    /// ([`PARES_QUE_ESTA_VERSAO_ATENDE`]), e um pedido acima dele é **baixado**
    /// antes de virar declaração — declarar o que esta máquina não atende faria
    /// a segunda pessoa esperar o prazo do par vencer por uma promessa que
    /// nunca teve como ser cumprida.
    ///
    /// Se essa correção fosse calada, a casca desenharia um deslizante em
    /// quatro e o servidor trabalharia com um, sem nada em lugar nenhum
    /// dizendo qual dos dois números vale. É a forma do defeito que mais custou
    /// caro neste repositório: o produto sabe e não conta. Por isso
    /// [`Enlace::consentir_no_caminho_entre_pares`] **devolve** o que valeu.
    ///
    /// # Por que o teste entra por [`cabivel`], e por que isso basta aqui
    ///
    /// Porque construir um [`Enlace`] de mentira exigiria um construtor que só
    /// os testes usam, dentro de um tipo de produção — e a casa já pagou por
    /// um guarda que existia e não funcionava porque o teste entrava por uma
    /// porta que a produção não usa.
    ///
    /// O que sobraria para aquele teste provar — que o valor devolvido é o
    /// mesmo que saiu no fio — **não é uma propriedade testável, é uma
    /// propriedade estrutural**: os dois saem do mesmo `let`, e não há caminho
    /// no código em que possam diferir. Uma asserção sobre isso mediria o
    /// compilador.
    #[test]
    fn o_teto_pedido_acima_do_que_esta_versao_atende_e_baixado() {
        assert_eq!(
            cabivel(ConsentimentoDePar {
                pares_que_atende: 9,
                assiste_por_par: true,
            }),
            ConsentimentoDePar {
                pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
                assiste_por_par: true,
            },
            "o teto pedido não foi baixado para o que esta versão atende"
        );
    }

    #[test]
    fn um_teto_que_cabe_atravessa_intacto() {
        // A outra metade: um guarda que baixasse tudo para zero passaria no
        // teste acima e desligaria a malha inteira sem um erro em lugar
        // nenhum. E a retirada — `de_ninguem` — tem de atravessar como está,
        // senão retirar o consentimento viraria um pedido de emprestar.
        for cabe in [
            ConsentimentoDePar::de_ninguem(),
            ConsentimentoDePar {
                pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
                assiste_por_par: false,
            },
            ConsentimentoDePar {
                pares_que_atende: 0,
                assiste_por_par: true,
            },
        ] {
            assert_eq!(cabivel(cabe), cabe, "{cabe:?} foi alterado sem precisar");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_pedido_para_assistir_por_par_que_chega_depois_da_retirada_nao_e_atendido() {
        let mut motor = motor_de_teste();
        motor.consentimento = ConsentimentoDePar::de_ninguem();

        motor.assistir_por_par(ScreenId(1), vec![endereco_qualquer()], impressao_qualquer());

        assert!(
            motor.resultados_do_par.try_recv().is_err(),
            "um `AssistaTelaPor` que chegou depois da retirada foi atendido: a discagem correu e \
             relatou uma falha de rede que não aconteceu"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deixar_de_emprestar_cancela_o_repasse_que_ja_estava_em_curso() {
        // **A retirada alcança o que já está no ar, e não só o pedido
        // seguinte.** Sem isto, desligar o empréstimo pararia de aceitar
        // pedidos novos e deixaria a cópia em curso subindo — a pessoa
        // continuaria pagando pela decisão que acabou de desfazer, e o único
        // aviso seria a conta de internet dela.
        //
        // A vaga voltar **antes** de `PRAZO_DO_PAR` é o discriminador, e ele é
        // escolhido e não arbitrário: o alvo é um buraco negro (uma ponta sem
        // `ServerConfig`, que nunca responde), contra o qual a tarefa gasta os
        // três segundos inteiros antes de desistir pelo fim do corpo.
        let mut motor = motor_de_teste();
        let punho = Arc::clone(&motor.atendendo_pares);
        let vaga = VagaDeAtendimento::tomar(&punho).expect("a vaga nasce livre");

        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let buraco_negro = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde_ninguem_atende = buraco_negro.local_addr().unwrap();
        let identidade = par::identidade_efemera().unwrap();
        let impressao = par::impressao(&identidade);
        motor.passar_a_servir(
            vaga,
            ponta,
            identidade,
            ScreenId(1),
            vec![onde_ninguem_atende],
            impressao,
        );

        motor
            .passar_a_consentir(ConsentimentoDePar::de_ninguem())
            .await;

        ate_que(
            "a vaga de quem servia um par voltar depois da retirada, antes de `PRAZO_DO_PAR`",
            Duration::from_secs(1),
            || !punho.load(std::sync::atomic::Ordering::Acquire),
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deixar_de_assistir_por_par_derruba_os_caminhos_de_par_abertos() {
        // A retirada do outro consentimento, e o mesmo princípio: quem diz
        // «não quero mais o meu endereço no ar» tem o caminho que existe por
        // causa daquele endereço desfeito agora, e não na próxima tela que
        // pedir. Quem reabre o cano do servidor é o servidor, ao receber a
        // declaração nova — ver `session::devolver_ao_servidor`.
        let mut motor = motor_de_teste();
        motor.tarefas_de_par.assistir(
            ScreenId(1),
            tokio::spawn(async { std::future::pending::<()>().await }),
        );
        assert_eq!(motor.tarefas_de_par.caminhos.len(), 1);

        motor
            .passar_a_consentir(ConsentimentoDePar {
                pares_que_atende: 1,
                assiste_por_par: false,
            })
            .await;

        assert!(
            motor.tarefas_de_par.caminhos.is_empty(),
            "retirar o consentimento de assistir por par deixou o caminho aberto de pé"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn continuar_consentindo_no_mesmo_nao_derruba_nada() {
        // O guarda contra o oposto, e ele é o que impede a malha de morrer na
        // primeira bateria interna: a declaração é repetida a **cada
        // reconexão** (`Motor::declarar_identidade_de_par`), e uma retirada
        // escrita larga demais derrubaria todo caminho vivo a cada volta.
        let mut motor = motor_de_teste();
        motor.consentimento = CONSENTE_TUDO;
        motor.tarefas_de_par.assistir(
            ScreenId(1),
            tokio::spawn(async { std::future::pending::<()>().await }),
        );

        motor.passar_a_consentir(CONSENTE_TUDO).await;

        assert_eq!(
            motor.tarefas_de_par.caminhos.len(),
            1,
            "redeclarar o mesmo consentimento derrubou um caminho de par vivo"
        );
    }

    fn endereco_qualquer() -> SocketAddr {
        SocketAddr::from(([192, 168, 1, 10], 4444))
    }

    fn impressao_qualquer() -> String {
        "a".repeat(64)
    }

    /// Quem consentiu nas duas metades, com o teto desta versão.
    const CONSENTE_TUDO: ConsentimentoDePar = ConsentimentoDePar {
        pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
        assiste_por_par: true,
    };

    /// **A fila da conexão que caiu não fala pela que a substitui.**
    ///
    /// # Por que um relato velho é pior do que um relato perdido
    ///
    /// `resultados_do_par` é um canal sem fundo, e o braço que o lê em
    /// `Motor::rodar` só existe **quando há cliente**. Durante a bateria não há:
    /// tudo o que as tarefas de par mandaram fica parado na fila. Quando
    /// `Motor::tentar` põe um cliente novo no lugar do que morreu, o primeiro
    /// braço a rodar entrega esses relatos **à conexão nova** — um `ParFalhou`
    /// de uma nomeação que morreu com a sessão anterior, pedindo ao servidor
    /// que desfaça um caminho que ele acabou de montar para a substituta.
    ///
    /// A segunda metade é a que impede o conserto de virar mudez: o que
    /// acontecer **depois** da queda continua tendo de chegar. Esvaziar a fila
    /// não pode ser fechá-la.
    #[tokio::test]
    async fn cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta() {
        let mut motor = motor_de_teste();

        // O relato da conexão que está morrendo, ainda em voo quando ela morre.
        let _ = motor.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
            screen: ScreenId(1),
            motivo: MotivoDeFalhaDePar::ParouDeMandar,
        });

        motor.cair();

        assert!(
            motor.resultados_do_par.try_recv().is_err(),
            "o relato da conexão que caiu continuou na fila: a próxima conexão o receberia como \
             se fosse dela, e mandaria o servidor desfazer um caminho que ele montou depois"
        );

        // E a atividade nova passa: a fila foi esvaziada, não fechada.
        let _ = motor.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
            screen: ScreenId(2),
            motivo: MotivoDeFalhaDePar::CaiuNoMeio,
        });
        assert!(
            matches!(
                motor.resultados_do_par.try_recv(),
                Ok(ResultadoDoPar::ParFalhou {
                    screen: ScreenId(2),
                    ..
                })
            ),
            "esvaziar a fila da conexão velha calou também o que veio depois da queda"
        );
    }

    /// Espera a condição valer, ou desiste com a mensagem de quem esperava.
    ///
    /// Dormir um tanto e afirmar é o que produz o teste que passa só na máquina
    /// de quem o escreveu.
    ///
    /// A paciência é de quem chama, e não uma constante daqui: em
    /// [`cair_devolve_a_vaga_de_quem_estava_servindo_um_par`] ela **é** a
    /// afirmação — esperar mais que [`PRAZO_DO_PAR`] deixaria o fim natural da
    /// tarefa passar por cancelamento.
    async fn ate_que<F: FnMut() -> bool>(o_que: &str, paciencia: Duration, mut condicao: F) {
        let fim = Instant::now() + paciencia;
        while Instant::now() < fim {
            if condicao() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("a paciência acabou esperando: {o_que}");
    }

    /// O ponto de chamada de `par::parar_de_atender` dentro de
    /// `servir_um_par` desarma a ponta de quem empresta depois de servir.
    ///
    /// # O achado do round 1 desta tarefa
    ///
    /// A tentativa anterior deste guarda media só do lado de quem **disca**
    /// uma segunda vez contra a ponta já servida — e lá, armada e desarmada
    /// são indistinguíveis: sem ninguém para chamar `ponta.accept()` de
    /// novo, as duas produzem o mesmo silêncio até o próprio prazo de
    /// discagem vencer. Medido e descartado — ver o relatório da Task 10.
    ///
    /// O sinal mora do lado de quem **atende**, como
    /// `par::testes::parar_de_atender_desarma_o_que_passar_a_atender_armou`
    /// já prova para a função isolada: pôr alguém pronto para aceitar
    /// (`par::atender`) depois do desarme, e só então discar. Armada, o
    /// aperto de mão completa; desarmada, `accept()` nunca rende nada e quem
    /// disca esgota o próprio prazo. Este teste faz o mesmo, mas contra
    /// `servir_um_par` — o ponto de chamada de produção, não a função pura.
    #[tokio::test(flavor = "multi_thread")]
    async fn servir_um_par_desarma_a_ponta_de_quem_empresta_depois_de_servir() {
        let ponta_empresta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde_empresta = ponta_empresta.local_addr().unwrap();
        let identidade_empresta = par::identidade_efemera().unwrap();
        let impressao_empresta = par::impressao(&identidade_empresta);

        // Quem "assiste": só disca, nunca atende — o mesmo papel de
        // `Motor::assistir_por_par`.
        let identidade_assiste = par::identidade_efemera().unwrap();
        let impressao_assiste = par::impressao(&identidade_assiste);

        // Um buraco negro: uma ponta de verdade, bindada, mas sem
        // `ServerConfig` nenhum. É o endereço que `servir_um_par` tenta
        // discar do lado dele — precisa ser um alvo que nunca responde, para
        // a corrida interna resolver pelo lado de `atender`, que é o que a
        // discagem de verdade abaixo aciona. Um pacote UDP para uma porta
        // fechada arriscaria um erro rápido demais (`ECONNREFUSED` do SO) e
        // venceria a corrida errada; este alvo só ignora, do jeito que um par
        // de verdade que não respondesse também ignoraria.
        let buraco_negro = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde_buraco_negro = buraco_negro.local_addr().unwrap();

        let servindo = tokio::spawn(servir_um_par(
            ponta_empresta.clone(),
            identidade_empresta,
            vec![onde_buraco_negro],
            impressao_assiste.clone(),
        ));

        // A discagem de verdade, de fora da tarefa acima — o papel de
        // `Motor::assistir_por_par` do lado de quem assiste.
        let ponta_assiste = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ligado = par::ligar(
            &ponta_assiste,
            &[onde_empresta],
            impressao_empresta.clone(),
            Some(&identidade_assiste),
            Duration::from_secs(3),
        )
        .await
        .expect("a discagem de quem assiste tinha tudo para fechar e não fechou");
        drop(ligado);

        let resultado = servindo.await.expect("a tarefa de servir_um_par");
        assert!(
            resultado.is_some(),
            "servir_um_par não serviu ninguém, e o resto deste teste não prova nada"
        );

        // **O discriminador.** Alguém pronto para aceitar, depois de
        // `servir_um_par` já ter devolvido: se a ponta continuasse armada —
        // por exemplo, com a chamada a `par::parar_de_atender` removida
        // deste ponto de chamada —, é este `atender` quem completaria o
        // aperto de mão, com a MESMA impressão que `passar_a_atender` já
        // tinha fixado dentro de `servir_um_par`.
        let _atendendo = tokio::spawn(par::atender(ponta_empresta, Duration::from_secs(2)));
        let ponta_de_novo = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let tentou = par::ligar(
            &ponta_de_novo,
            &[onde_empresta],
            impressao_empresta,
            Some(&identidade_assiste),
            Duration::from_millis(300),
        )
        .await;
        assert!(
            matches!(tentou, Err(par::ErroDePar::NaoAlcancou)),
            "a ponta de quem empresta continuou atendendo depois de servir_um_par devolver: \
             {tentou:?}"
        );
    }

    /// **Duas telas na mesma sala: a segunda não é repassada, e a primeira
    /// sobrevive ao fim dela.**
    ///
    /// O cenário é o do §0 do desenho — «numa call de 5 pessoas, 2 querem
    /// transmitir a tela» —, e antes deste guarda ele produzia o pior defeito
    /// que este repositório sabe nomear: a segunda `abriu()` sobrescrevia a
    /// primeira, os quadros das duas entravam intercalados no mesmo fluxo do
    /// par, quem recebia rotulava tudo com o `screen` do fluxo, e o primeiro
    /// `fechou()` matava o repasse da outra. Duas telas fundidas numa só, sem
    /// erro em lugar nenhum.
    #[tokio::test]
    async fn so_uma_tela_por_vez_e_repassada_e_o_fim_da_outra_nao_a_derruba() {
        let repasse = RepasseDeTela::default();
        let primeira = ScreenId(1);
        let segunda = ScreenId(2);
        let (para_o_par, mut do_par) = mpsc::channel(8);

        repasse.abriu(primeira, b"abertura-da-primeira".to_vec());
        repasse.ligar(para_o_par);

        // A segunda transmissão chega e **não** assume.
        repasse.abriu(segunda, b"abertura-da-segunda".to_vec());
        assert_eq!(
            repasse.abertura_de(primeira),
            Some(b"abertura-da-primeira".to_vec()),
            "a segunda tela assumiu o repasse da primeira"
        );
        assert_eq!(
            repasse.abertura_de(segunda),
            None,
            "o repasse aceitou servir a segunda tela com a abertura de outra"
        );

        // E os quadros dela não entram no fluxo do par.
        repasse.pedaco(segunda, b"quadro-da-segunda".to_vec());

        // O fim da segunda não derruba o repasse da primeira.
        repasse.fechou(segunda);
        repasse.pedaco(primeira, b"quadro-da-primeira".to_vec());

        assert_eq!(
            do_par.try_recv().ok(),
            Some(b"quadro-da-primeira".to_vec()),
            "o fim da segunda tela derrubou o repasse da primeira"
        );
        assert!(
            do_par.try_recv().is_err(),
            "um quadro da segunda tela entrou no fluxo do par da primeira — as duas chegam \
             fundidas a quem assiste"
        );

        // E o fim da primeira, esse sim, encerra o repasse.
        repasse.fechou(primeira);
        assert_eq!(repasse.abertura_de(primeira), None);
    }

    /// **A discagem de quem empresta falha na hora, e `atender` ainda serve.**
    ///
    /// Quem assiste nunca chama `par::atender` — só disca. Então a discagem de
    /// quem empresta não tem quem a atenda e **não pode** fechar: ela existe
    /// por um efeito só, abrir o mapeamento de NAT desta ponta. O `select!`
    /// tratava o fim dela como resposta, e bastava um erro rápido — lista de
    /// candidatos vazia, família de endereço incompatível, `connect_with`
    /// recusando na hora — para `atender` ser cancelado e quem empresta
    /// desistir de servir alguém que estava no meio do caminho.
    ///
    /// A lista vazia é o erro instantâneo mais limpo que existe: `par::ligar`
    /// não tem candidato para tentar e devolve `NaoAlcancou` sem esperar um
    /// milissegundo.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_discagem_que_falha_na_hora_nao_cancela_o_atendimento() {
        let ponta_empresta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde_empresta = ponta_empresta.local_addr().unwrap();
        let identidade_empresta = par::identidade_efemera().unwrap();
        let impressao_empresta = par::impressao(&identidade_empresta);

        let identidade_assiste = par::identidade_efemera().unwrap();
        let impressao_assiste = par::impressao(&identidade_assiste);

        // **Sem endereço nenhum para discar.** `par::ligar` desiste no mesmo
        // instante, e é esse instante que cancelava o `atender`.
        let servindo = tokio::spawn(servir_um_par(
            ponta_empresta.clone(),
            identidade_empresta,
            Vec::new(),
            impressao_assiste.clone(),
        ));

        // E quem assiste chega, como sempre chega: discando.
        let ponta_assiste = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ligado = par::ligar(
            &ponta_assiste,
            &[onde_empresta],
            impressao_empresta,
            Some(&identidade_assiste),
            Duration::from_secs(3),
        )
        .await;

        let resultado = servindo.await.expect("a tarefa de servir_um_par");
        assert!(
            resultado.is_some(),
            "a discagem de quem empresta falhou na hora e levou o `atender` junto: quem \
             empresta desistiu de servir alguém que estava chegando"
        );
        assert!(
            ligado.is_ok(),
            "quem assiste discou para uma ponta que devia estar atendendo e não fechou: {:?}",
            ligado.err()
        );
    }
}
