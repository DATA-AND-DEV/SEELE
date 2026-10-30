//! O túnel de jogo no núcleo (protocolo 12): a porta de quem joga, o caminho
//! que ela usa e as medidas que o produto precisa contar.
//!
//! # As duas metades
//!
//! - **Quem joga** abre uma porta em `127.0.0.1` ([`abrir_porta_local`]) e leva
//!   cada conexão TCP num canal `t`, cada fluxo UDP num canal `j`. Começa pela
//!   estrela (a conexão com o servidor, que repassa) e, quando chega a ligação
//!   direta, as conexões **novas** vão por ela.
//! - **A máquina** aceita os canais — da conexão do servidor e das conexões de
//!   par que o servidor apresentou — e liga cada um no jogo, em
//!   `127.0.0.1:porta`.
//!
//! # O que este módulo não faz
//!
//! Não fala com o servidor: quem manda `EntrarNoTunel` e lê `TunelConcedido`
//! é o `enlace.rs`, que entrega aqui uma [`Via`] (a fábrica de canais de um
//! caminho). É por isso que os testes de unidade usam o par de pontas de
//! `seele_enlace::teste` e provam o túnel sem servidor.
//!
//! # O produto sabe e conta
//!
//! Toda conexão, todo fluxo e todo túnel que acaba tem um motivo enumerado
//! ([`Queda`]), vai para o log com `mod_id`, `tunel` e `caminho`, e entra no
//! [`RelatorioDoTunel`] que a casca recebe como [`Aviso::Tunel`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::pending;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

// Republicada para a FFI nomear a recusa sem tocar em `seele-proto` (ADR 0002).
use seele_proto::control::{ClientMessage, MotivoDeFalhaDePar, ServerMessage};
use seele_proto::ids::PersonId;
pub use seele_proto::tunel::RecusaDeTunel;
use seele_proto::tunel::{
    AberturaDeTunel, MotivoDoFimDoTunel, ProtocoloDeTunel, SinalDeTunel, TipoDeCanalDeTunel,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::enlace::Aviso;
use crate::par::{cumprir_apresentacao, motivo_de_falha, Papel, ParLigado};
use crate::transporte::{
    CanalDeTunel, Conexao, Confiabilidade, Datagramas, Envio, Motivo, PontaDaSessao, Recebe, Recusa,
};

/// Quantas portas depois da preferida se tenta antes de desistir.
const PORTAS_DE_FOLGA: u16 = 64;
/// Quanto um fluxo UDP sem tráfego vive. Passado isto ele fecha, contado como
/// [`Queda::Fechado`], e o próximo datagrama do jogo abre outro.
const OCIOSIDADE_DO_FLUXO_UDP: Duration = Duration::from_secs(60);
/// De quanto em quanto o canal de controle de um caminho manda `Ping`.
const INTERVALO_DO_PING: Duration = Duration::from_secs(2);
/// Quanto se espera um `Pong` antes de dar o canal de controle por perdido.
const PRAZO_DO_PONG: Duration = Duration::from_secs(5);
/// Quanto a máquina espera a abertura de um canal que chegou.
const PRAZO_DA_ABERTURA: Duration = Duration::from_secs(10);
/// De quanto em quanto sai o relatório enquanto há tráfego.
const TICA_DO_RELATORIO: Duration = Duration::from_secs(1);
/// Quantos datagramas esperam, por fluxo, a abertura do canal `j`.
const FILA_DO_FLUXO: usize = 256;
/// O código com que se corta um canal cujo outro lado já não existe.
const CODIGO_CORTADO: u32 = 0x5345_0030;

/// A chave de um túnel: `(mod_id, nome)`.
pub(crate) type Chave = (String, String);

// ----------------------------------------------------------------- tipos

/// Por onde o túnel anda agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caminho {
    /// Direto com a máquina, pela malha. `par` é a pessoa dela, como o
    /// servidor a nomeou em `AlcanceTunelPor.quem`: é com ela que o produto
    /// diz «ligação direta com <apelido>».
    Direto {
        /// Quem serve.
        par: PersonId,
    },
    /// Pelo servidor, que repassa.
    Estrela,
}

impl Caminho {
    fn rotulo(self) -> &'static str {
        match self {
            Self::Direto { .. } => "direto",
            Self::Estrela => "estrela",
        }
    }
}

/// Por que uma conexão, um fluxo ou o túnel inteiro acabou.
///
/// Enumerado e nunca texto livre (`specs/02-protocolo.md`): a casca decide a
/// frase. A ordem das variantes é a ordem em que [`RelatorioDoTunel::quedas`]
/// as lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Queda {
    /// Nenhuma porta local coube: a preferida e as 64 seguintes estavam tomadas.
    PortaOcupada,
    /// O canal (ou a ligação direta que o levava) caiu no meio.
    CaminhoCaiu,
    /// A máquina não conseguiu ligar em `127.0.0.1:porta`: o jogo não escuta.
    JogoRecusou,
    /// A máquina parou de servir o túnel.
    MaquinaParou,
    /// A máquina saiu do servidor.
    MaquinaSaiu,
    /// A pessoa perdeu a ação `entrar-no-jogo`.
    SemPermissao,
    /// O MOD foi desligado.
    ModDesligado,
    /// A conexão com o servidor acabou.
    ServidorSaiu,
    /// Fim normal: o jogo fechou a conexão, o fluxo UDP ficou ocioso, ou a
    /// pessoa saiu do túnel.
    Fechado,
}

/// O que o túnel mediu, para a casca desenhar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioDoTunel {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O nome do túnel.
    pub nome: String,
    /// A porta em `127.0.0.1` em que o jogo de quem joga deve ligar.
    pub porta_local: u16,
    /// Se `porta_local` é a porta do jogo (e não uma das seguintes).
    pub preferida: bool,
    /// A porta do jogo, a que se tentou primeiro. Com `preferida: false` é ela
    /// que estava ocupada, e é o que a casca diz a quem joga.
    pub desejada: u16,
    /// Por onde as conexões novas andam.
    pub caminho: Caminho,
    /// A ida e volta de **ponta a ponta** (pela estrela inclui os dois
    /// trechos), medida no canal de controle. `None` até o primeiro `Pong`.
    pub latencia: Option<Duration>,
    /// Bytes por segundo de quem joga para a máquina, na última janela.
    pub bytes_por_segundo_ida: u64,
    /// Bytes por segundo da máquina para quem joga, na última janela.
    pub bytes_por_segundo_volta: u64,
    /// Conexões TCP e fluxos UDP vivos agora.
    pub conexoes: u32,
    /// Quantas vezes cada motivo acabou algo, desde que o túnel abriu.
    pub quedas: Vec<(Queda, u32)>,
    /// Datagramas UDP que não saíram: fila cheia aqui, fila cheia no enlace,
    /// ou acima de `MAX_DADOS`. Nada se perde sem ser contado.
    pub udp_descartados: u64,
    /// `Some` no último relatório de um túnel: por que ele acabou. A casca não
    /// tinha como saber que o fluxo de relatórios tinha parado.
    pub encerrado: Option<Queda>,
}

// ------------------------------------------------------------ porta local

/// A escuta de quem joga.
pub(crate) enum Escuta {
    Tcp(TcpListener),
    Udp(Arc<UdpSocket>),
}

/// A porta de quem joga: sempre em `127.0.0.1`.
///
/// Quem a recebe de [`abrir_porta_local`] é dono da escuta; fechá-la é largar
/// este valor. A que [`crate::enlace::Enlace::entrar_no_tunel`] devolve é só a
/// descrição: a escuta fica com o túnel, e é por isso que derrubar o túnel
/// fecha a porta.
#[derive(Debug)]
pub struct PortaLocal {
    /// A porta em que se escuta.
    pub porta: u16,
    /// Se é a que se pediu (a do jogo), e não uma das seguintes.
    pub preferida: bool,
    /// A porta que se pediu (a do jogo), ocupada ou não; zero quando se pediu
    /// qualquer uma. A casca precisa dela para dizer «a 25565 estava ocupada»:
    /// só `preferida: false` não diz qual era.
    pub desejada: u16,
    endereco: SocketAddr,
    protocolo: ProtocoloDeTunel,
    escuta: Option<Escuta>,
}

impl std::fmt::Debug for Escuta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(_) => f.write_str("Tcp"),
            Self::Udp(_) => f.write_str("Udp"),
        }
    }
}

impl PortaLocal {
    /// O endereço em que a escuta está de fato ligada — lido do socket, e não
    /// suposto.
    #[must_use]
    pub fn endereco(&self) -> SocketAddr {
        self.endereco
    }

    /// O que a porta carrega.
    #[must_use]
    pub fn protocolo(&self) -> ProtocoloDeTunel {
        self.protocolo
    }

    /// Uma cópia da descrição, sem a escuta: o que se devolve a quem pede
    /// entrada num túnel que já está aberto.
    fn descrita(&self) -> Self {
        Self {
            porta: self.porta,
            preferida: self.preferida,
            desejada: self.desejada,
            endereco: self.endereco,
            protocolo: self.protocolo,
            escuta: None,
        }
    }

    /// Tira a escuta, deixando a descrição. `None` só se a porta já foi
    /// separada.
    fn separar(mut self) -> Option<(Self, Escuta)> {
        let escuta = self.escuta.take()?;
        Some((self, escuta))
    }
}

/// Confere que ninguém escuta a `porta` em `0.0.0.0`, que é o que um servidor de
/// jogo de verdade costuma fazer.
///
/// **Por que não basta ligar em `127.0.0.1`.** No macOS (e no Windows) ligar
/// `127.0.0.1:P` dá certo enquanto `0.0.0.0:P` escuta: as duas coexistem, e a
/// conexão do jogo iria para a mais específica, a nossa, sem que o servidor
/// que a pessoa tem aberto recebesse nada. A porta pareceria livre e o jogo
/// entraria no túnel em vez do servidor local.
///
/// O teste liga o curinga **sem** guardar o socket. No Unix com
/// `SO_REUSEADDR` (medido no macOS: sem ele, uma conexão que o nosso lado fechou
/// primeiro, em `TIME_WAIT`, faria uma porta livre parecer ocupada; com ele, o
/// curinga ainda enxerga um escutador de verdade); no Windows sem, porque lá o
/// `SO_REUSEADDR` deixaria roubar a porta.
fn curinga_livre(protocolo: ProtocoloDeTunel, porta: u16) -> std::io::Result<()> {
    use socket2::{Domain, Protocol, Socket, Type};
    if porta == 0 {
        return Ok(());
    }
    let (tipo, protocolo_do_so) = match protocolo {
        ProtocoloDeTunel::Tcp => (Type::STREAM, Protocol::TCP),
        ProtocoloDeTunel::Udp => (Type::DGRAM, Protocol::UDP),
    };
    let socket = Socket::new(Domain::IPV4, tipo, Some(protocolo_do_so))?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, porta)).into())
}

fn ligar(protocolo: ProtocoloDeTunel, porta: u16) -> std::io::Result<(Escuta, SocketAddr)> {
    // Antes de qualquer coisa: um escutador em `0.0.0.0` torna a porta ocupada
    // mesmo que `127.0.0.1` ainda aceite o `bind`.
    curinga_livre(protocolo, porta)?;
    // **Só `127.0.0.1`.** A porta de quem joga é de quem está na máquina; em
    // `0.0.0.0` a rede de casa inteira entraria no túnel por ela.
    match protocolo {
        ProtocoloDeTunel::Tcp => {
            let escuta = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, porta))?;
            escuta.set_nonblocking(true)?;
            let endereco = escuta.local_addr()?;
            Ok((Escuta::Tcp(TcpListener::from_std(escuta)?), endereco))
        }
        ProtocoloDeTunel::Udp => {
            let socket = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, porta))?;
            socket.set_nonblocking(true)?;
            let endereco = socket.local_addr()?;
            Ok((
                Escuta::Udp(Arc::new(UdpSocket::from_std(socket)?)),
                endereco,
            ))
        }
    }
}

/// O mesmo de [`abrir_porta_local`], sem o `async`: o motor do enlace o chama
/// de dentro de um tratador síncrono. Precisa de um runtime do tokio em curso.
pub(crate) fn abrir_sincrono(
    protocolo: ProtocoloDeTunel,
    preferida: u16,
) -> std::io::Result<PortaLocal> {
    let candidatas: Vec<u16> = if preferida == 0 {
        vec![0]
    } else {
        (preferida..=preferida.saturating_add(PORTAS_DE_FOLGA)).collect()
    };
    let mut ultimo = None;
    for (indice, porta) in candidatas.into_iter().enumerate() {
        match ligar(protocolo, porta) {
            Ok((escuta, endereco)) => {
                return Ok(PortaLocal {
                    porta: endereco.port(),
                    preferida: indice == 0,
                    desejada: preferida,
                    endereco,
                    protocolo,
                    escuta: Some(escuta),
                });
            }
            Err(erro) => ultimo = Some(erro),
        }
    }
    Err(ultimo.unwrap_or_else(|| std::io::Error::other("nenhuma porta para tentar")))
}

/// Abre a porta de quem joga em `127.0.0.1`.
///
/// Tenta `preferida..=preferida+64`, a primeira livre; só então devolve o erro
/// da última tentativa. `preferida == 0` pede qualquer porta livre.
///
/// # Errors
///
/// A última causa, quando nenhuma das 65 portas pôde ser aberta.
pub async fn abrir_porta_local(
    protocolo: ProtocoloDeTunel,
    preferida: u16,
) -> std::io::Result<PortaLocal> {
    abrir_sincrono(protocolo, preferida)
}

// ---------------------------------------------------------------- medidas

/// O que um túnel conta enquanto anda. Atômicos, porque quem mede são as
/// tarefas das conexões e quem lê é o laço do túnel.
pub(crate) struct Medidas {
    ida: AtomicU64,
    volta: AtomicU64,
    conexoes: AtomicU32,
    /// Microssegundos; zero é «ainda não medido».
    latencia_us: AtomicU64,
    /// Datagramas que a fila de um fluxo, aqui, não comportou.
    udp_perdidos: AtomicU64,
    quedas: Mutex<BTreeMap<Queda, u32>>,
    aviso: Option<mpsc::UnboundedSender<Evento>>,
}

impl Medidas {
    pub(crate) fn nova(aviso: Option<mpsc::UnboundedSender<Evento>>) -> Self {
        Self {
            ida: AtomicU64::new(0),
            volta: AtomicU64::new(0),
            conexoes: AtomicU32::new(0),
            latencia_us: AtomicU64::new(0),
            udp_perdidos: AtomicU64::new(0),
            quedas: Mutex::new(BTreeMap::new()),
            aviso,
        }
    }

    /// Acorda o laço do túnel: algo mudou e vale um relatório já.
    fn mudou(&self) {
        if let Some(aviso) = &self.aviso {
            let _ = aviso.send(Evento::Mudou);
        }
    }

    pub(crate) fn quedar(&self, queda: Queda) {
        *self
            .quedas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(queda)
            .or_insert(0) += 1;
        self.mudou();
    }

    pub(crate) fn quedas(&self) -> Vec<(Queda, u32)> {
        self.quedas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(queda, quantas)| (*queda, *quantas))
            .collect()
    }

    fn latencia(&self) -> Option<Duration> {
        match self.latencia_us.load(Ordering::Relaxed) {
            0 => None,
            micros => Some(Duration::from_micros(micros)),
        }
    }

    fn medir(&self, ida_e_volta: Option<Duration>) {
        let micros = ida_e_volta.map_or(0, |d| {
            u64::try_from(d.as_micros()).unwrap_or(u64::MAX).max(1)
        });
        if self.latencia_us.swap(micros, Ordering::Relaxed) == 0 && micros != 0 {
            // A primeira medida é uma mudança de estado.
            self.mudou();
        }
    }
}

/// Uma conexão ou fluxo vivo: conta enquanto existir.
struct ConexaoViva<'a>(&'a Medidas);

impl<'a> ConexaoViva<'a> {
    fn nova(medidas: &'a Medidas) -> Self {
        medidas.conexoes.fetch_add(1, Ordering::Relaxed);
        medidas.mudou();
        Self(medidas)
    }
}

impl Drop for ConexaoViva<'_> {
    fn drop(&mut self) {
        self.0.conexoes.fetch_sub(1, Ordering::Relaxed);
        self.0.mudou();
    }
}

/// O que acordou o laço do túnel.
pub(crate) enum Evento {
    /// Algo mudou: sai um relatório.
    Mudou,
    /// O túnel acabou, por este motivo.
    Fim(Queda),
    /// A ligação direta chegou.
    Direto(Arc<ParLigado>, PersonId),
    /// A ligação direta desta geração caiu.
    DiretoCaiu(u32),
    /// Um fluxo UDP acabou.
    FluxoAcabou { origem: SocketAddr, fluxo: u32 },
}

/// Uma tarefa que morre com quem a segura.
pub(crate) struct AbortaAoLargar(Option<tokio::task::JoinHandle<()>>);

impl AbortaAoLargar {
    pub(crate) fn nova<F>(futuro: F) -> Self
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        Self(Some(tokio::spawn(futuro)))
    }

    /// Larga sem abortar: a tarefa termina sozinha.
    fn soltar(mut self) {
        self.0.take();
    }

    /// Tira a alça, sem abortar, para quem quiser esperar a tarefa acabar.
    fn tirar_alca(mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.0.take()
    }
}

impl Drop for AbortaAoLargar {
    fn drop(&mut self) {
        if let Some(tarefa) = self.0.take() {
            tarefa.abort();
        }
    }
}

// -------------------------------------------------------------------- via

/// A fábrica de canais de um caminho: a conexão com o servidor (estrela) ou a
/// ligação direta com o par.
///
/// O túnel não toca a [`Conexao`] por outro lugar, e é isto que deixa os
/// testes de unidade usarem o par de pontas de `seele_enlace::teste`.
#[derive(Clone)]
pub(crate) struct Via {
    conexao: Conexao,
}

impl Via {
    pub(crate) fn nova(conexao: Conexao) -> Self {
        Self { conexao }
    }

    async fn abrir(&self, como: Confiabilidade) -> Result<CanalDeTunel, Motivo> {
        self.conexao.abrir_tunel(como).await
    }

    async fn aceitar(&self) -> Result<CanalDeTunel, Motivo> {
        self.conexao.aceitar_tunel().await
    }

    /// Quando a conexão acabou.
    async fn fechou(&self) -> Motivo {
        self.conexao.closed().await
    }

    /// Datagramas `j` que o enlace descartou nesta conexão: fila cheia ou acima
    /// do teto. Somados ao que o túnel descarta por conta própria.
    fn descartes(&self) -> u64 {
        let contadores = self.conexao.contadores();
        contadores
            .tunel_descartado
            .saturating_add(contadores.tunel_grande_demais)
    }
}

// ------------------------------------------------------------ enquadramento

async fn escrever_abertura_t(envio: &mut Envio, abertura: &AberturaDeTunel) -> Result<(), String> {
    crate::frame::write(envio, abertura)
        .await
        .map_err(|erro| erro.to_string())
}

fn abertura_j(abertura: &AberturaDeTunel) -> Result<Vec<u8>, String> {
    seele_proto::control::encode(abertura).map_err(|erro| erro.to_string())
}

fn abertura(mod_id: &str, nome: &str, fluxo: u32, tipo: TipoDeCanalDeTunel) -> AberturaDeTunel {
    AberturaDeTunel {
        mod_id: mod_id.to_owned(),
        nome: nome.to_owned(),
        fluxo,
        tipo,
        de: None,
    }
}

/// Se um cliente que negociou `carimbo` pode mandar `mensagem`: o verbo tem de
/// ter nascido num número que o servidor fala. Um servidor 11 leria uma
/// variante que não conhece e o fluxo de controle desalinharia.
#[must_use]
pub(crate) fn verbo_cabe(carimbo: u8, mensagem: &ClientMessage) -> bool {
    seele_proto::vocabulario::desde_do_cliente(mensagem) <= carimbo
}

// ------------------------------------------------------------------ pontes

/// Como uma ponte TCP acabou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FimDaPonte {
    /// Os dois lados terminaram direito.
    Normal,
    /// O socket local (o jogo) falhou ou resetou.
    Local,
    /// O canal caiu ou foi cortado.
    Canal,
}

impl FimDaPonte {
    fn queda(self) -> Queda {
        match self {
            Self::Normal | Self::Local => Queda::Fechado,
            Self::Canal => Queda::CaminhoCaiu,
        }
    }
}

/// Copia entre um socket TCP local e um canal `t`, nos dois sentidos.
///
/// `sai` conta o que vai do socket para o canal, `entra` o que vem do canal.
/// Quem acaba mal derruba o outro sentido; quem acaba direito espera o outro.
async fn ponte_tcp(
    tcp: TcpStream,
    mut envio: Envio,
    mut recebe: Recebe,
    sai: &AtomicU64,
    entra: &AtomicU64,
) -> FimDaPonte {
    let (mut le, mut escreve) = tcp.into_split();
    let ida = async {
        let mut buf = vec![0_u8; seele_enlace::MAX_DADOS];
        loop {
            match le.read(&mut buf).await {
                Ok(0) => {
                    let _ = envio.finish();
                    return FimDaPonte::Normal;
                }
                Ok(n) => {
                    let Some(pedaco) = buf.get(..n) else {
                        return FimDaPonte::Local;
                    };
                    if envio.write_all(pedaco).await.is_err() {
                        return FimDaPonte::Canal;
                    }
                    sai.fetch_add(n as u64, Ordering::Relaxed);
                }
                Err(_) => {
                    let _ = envio.reset(CODIGO_CORTADO);
                    return FimDaPonte::Local;
                }
            }
        }
    };
    let volta = async {
        let mut buf = vec![0_u8; seele_enlace::MAX_DADOS];
        loop {
            match recebe.read(&mut buf).await {
                Ok(Some(n)) => {
                    let Some(pedaco) = buf.get(..n) else {
                        return FimDaPonte::Local;
                    };
                    if escreve.write_all(pedaco).await.is_err() {
                        return FimDaPonte::Local;
                    }
                    entra.fetch_add(n as u64, Ordering::Relaxed);
                }
                Ok(None) => {
                    let _ = escreve.shutdown().await;
                    return FimDaPonte::Normal;
                }
                Err(_) => return FimDaPonte::Canal,
            }
        }
    };
    tokio::pin!(ida, volta);
    let (mut fim_ida, mut fim_volta) = (None, None);
    while fim_ida.is_none() || fim_volta.is_none() {
        tokio::select! {
            fim = &mut ida, if fim_ida.is_none() => {
                fim_ida = Some(fim);
                if fim != FimDaPonte::Normal { break; }
            }
            fim = &mut volta, if fim_volta.is_none() => {
                fim_volta = Some(fim);
                if fim != FimDaPonte::Normal { break; }
            }
        }
    }
    [fim_ida, fim_volta]
        .into_iter()
        .flatten()
        .fold(FimDaPonte::Normal, |pior, fim| match (pior, fim) {
            (FimDaPonte::Canal, _) | (_, FimDaPonte::Canal) => FimDaPonte::Canal,
            (FimDaPonte::Local, _) | (_, FimDaPonte::Local) => FimDaPonte::Local,
            _ => FimDaPonte::Normal,
        })
}

// ------------------------------------------------------------ quem joga

/// O que o laço de um túnel de quem joga sabe de si.
struct Contexto {
    mod_id: String,
    nome: String,
    porta_local: u16,
    preferida: bool,
    desejada: u16,
    medidas: Arc<Medidas>,
    avisos: mpsc::UnboundedSender<Aviso>,
}

impl Contexto {
    /// Registra o fim de uma conexão ou fluxo: no relatório e no log.
    fn terminou(&self, caminho: &str, fluxo: u32, queda: Queda, detalhe: &str) {
        if queda == Queda::Fechado {
            tracing::debug!(mod_id = %self.mod_id, tunel = %self.nome, caminho, fluxo, ?queda, detalhe, "túnel: conexão fechada");
        } else {
            tracing::warn!(mod_id = %self.mod_id, tunel = %self.nome, caminho, fluxo, ?queda, detalhe, "túnel: conexão caiu");
        }
        self.medidas.quedar(queda);
    }

    fn relatorio(
        &self,
        caminho: Caminho,
        ida_bps: u64,
        volta_bps: u64,
        descartes: u64,
        encerrado: Option<Queda>,
    ) -> RelatorioDoTunel {
        RelatorioDoTunel {
            mod_id: self.mod_id.clone(),
            nome: self.nome.clone(),
            porta_local: self.porta_local,
            preferida: self.preferida,
            desejada: self.desejada,
            caminho,
            latencia: self.medidas.latencia(),
            bytes_por_segundo_ida: ida_bps,
            bytes_por_segundo_volta: volta_bps,
            conexoes: self.medidas.conexoes.load(Ordering::Relaxed),
            quedas: self.medidas.quedas(),
            udp_descartados: descartes
                .saturating_add(self.medidas.udp_perdidos.load(Ordering::Relaxed)),
            encerrado,
        }
    }
}

/// O túnel de quem joga, vivo. Largá-lo o derruba: a porta fecha e toda
/// conexão e fluxo morre.
pub(crate) struct TunelDeJogador {
    eventos: mpsc::UnboundedSender<Evento>,
    tarefa: Option<AbortaAoLargar>,
    discagem: Option<AbortaAoLargar>,
    /// A máquina com quem se tentou a ligação direta: é a chave de
    /// [`Ligacoes`], e é por ela que a poda sabe quem ainda precisa do quê.
    quem: Option<PersonId>,
    /// A descrição da porta, para quem pedir entrada num túnel que já está
    /// aberto receber a mesma porta, e não uma nova.
    porta: PortaLocal,
}

/// O que `AlcanceTunelPor` entrega para discar a máquina.
pub(crate) struct Alcance {
    pub(crate) mod_id: String,
    pub(crate) nome: String,
    pub(crate) ponta: PontaDaSessao,
    pub(crate) enderecos: Vec<SocketAddr>,
    pub(crate) impressao: String,
    pub(crate) prazo: Duration,
    /// A pessoa da máquina, para o caminho dizer com quem é a ligação.
    pub(crate) quem: PersonId,
}

/// Quem avisa o servidor de um verbo que o túnel quer mandar.
pub(crate) type Enviar = Arc<dyn Fn(ClientMessage) + Send + Sync>;

/// Onde está a discagem da ligação direta com uma máquina.
#[derive(Clone)]
enum EstadoDaLigacao {
    Discando,
    Pronta(Arc<ParLigado>),
    Falhou(MotivoDeFalhaDePar, String),
    /// Chegou a existir e acabou.
    Caiu,
}

struct Ligacao {
    estado: watch::Receiver<EstadoDaLigacao>,
    /// A discagem, e depois a espera pelo fim da conexão. Larga-la a aborta, e
    /// com ela a ligação (se nenhum túnel a segura mais).
    _tarefa: AbortaAoLargar,
}

/// As ligações diretas de quem joga, **uma por máquina**.
///
/// Dois túneis da mesma máquina (um TCP e um UDP, o caso comum de um jogo)
/// são apresentados pelo servidor duas vezes, às mesmas duas pontas. Se cada
/// túnel discasse a sua, as duas discagens e os dois atendimentos disputariam
/// as mesmas conexões de entrada e se casariam trocados: a máquina atenderia
/// a ligação do túnel A achando que era do B, e a outra discagem esgotaria o
/// prazo. Medido na prova `tunel_de_jogo.rs`: o segundo túnel nunca ia direto.
/// Por isso a discagem é uma só, e todo túnel da máquina espera por ela.
#[derive(Default)]
pub(crate) struct Ligacoes {
    por_maquina: HashMap<PersonId, Ligacao>,
}

impl Ligacoes {
    /// A discagem em curso (ou pronta) para esta máquina, começando uma se não
    /// houver — ou se a que havia falhou ou caiu.
    fn obter(&mut self, alcance: Alcance) -> watch::Receiver<EstadoDaLigacao> {
        if let Some(viva) = self.por_maquina.get(&alcance.quem) {
            if matches!(
                *viva.estado.borrow(),
                EstadoDaLigacao::Discando | EstadoDaLigacao::Pronta(_)
            ) {
                return viva.estado.clone();
            }
        }
        let (tx, rx) = watch::channel(EstadoDaLigacao::Discando);
        let Alcance {
            ponta,
            enderecos,
            impressao,
            prazo,
            quem,
            ..
        } = alcance;
        let tarefa = AbortaAoLargar::nova(async move {
            match cumprir_apresentacao(&ponta, Papel::Busca, &enderecos, impressao, prazo).await {
                Ok(ligado) => {
                    let ligado = Arc::new(ligado);
                    let conexao = ligado.conexao.clone();
                    tx.send_replace(EstadoDaLigacao::Pronta(ligado));
                    conexao.closed().await;
                    tx.send_replace(EstadoDaLigacao::Caiu);
                }
                Err(erro) => {
                    tx.send_replace(EstadoDaLigacao::Falhou(
                        motivo_de_falha(&erro),
                        erro.to_string(),
                    ));
                }
            }
        });
        self.por_maquina.insert(
            quem,
            Ligacao {
                estado: rx.clone(),
                _tarefa: tarefa,
            },
        );
        rx
    }

    /// Larga as ligações de que nenhum túnel precisa mais.
    fn podar(&mut self, jogando: &HashMap<Chave, TunelDeJogador>) {
        let precisadas: HashSet<PersonId> = jogando.values().filter_map(|t| t.quem).collect();
        self.por_maquina.retain(|quem, _| precisadas.contains(quem));
    }
}

impl TunelDeJogador {
    /// Põe o túnel de pé sobre a escuta já aberta, começando pela estrela.
    pub(crate) fn iniciar(
        mod_id: &str,
        nome: &str,
        porta: PortaLocal,
        estrela: Via,
        avisos: mpsc::UnboundedSender<Aviso>,
    ) -> Option<(Self, PortaLocal)> {
        let (descricao, escuta) = porta.separar()?;
        let (tx, rx) = mpsc::unbounded_channel();
        let contexto = Arc::new(Contexto {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            porta_local: descricao.porta,
            preferida: descricao.preferida,
            desejada: descricao.desejada,
            medidas: Arc::new(Medidas::nova(Some(tx.clone()))),
            avisos,
        });
        let tarefa =
            AbortaAoLargar::nova(laco_do_jogador(contexto, escuta, estrela, tx.clone(), rx));
        Some((
            Self {
                eventos: tx,
                tarefa: Some(tarefa),
                discagem: None,
                quem: None,
                porta: descricao.descrita(),
            },
            descricao,
        ))
    }

    /// Pede a ligação direta com a máquina; falhando, avisa o servidor com
    /// `TunelFalhou` e o túnel segue pela estrela.
    ///
    /// A ligação é **da máquina, e não do túnel**: dois túneis da mesma máquina
    /// esperam a mesma discagem (ver [`Ligacoes`]).
    pub(crate) fn alcancar_por(
        &mut self,
        alcance: Alcance,
        enviar: Enviar,
        ligacoes: &mut Ligacoes,
    ) {
        let quem = alcance.quem;
        let mod_id = alcance.mod_id.clone();
        let nome = alcance.nome.clone();
        let mut estado = ligacoes.obter(alcance);
        self.quem = Some(quem);
        let eventos = self.eventos.clone();
        self.discagem = Some(AbortaAoLargar::nova(async move {
            loop {
                let visto = estado.borrow_and_update().clone();
                match visto {
                    EstadoDaLigacao::Pronta(ligado) => {
                        let _ = eventos.send(Evento::Direto(ligado, quem));
                        return;
                    }
                    EstadoDaLigacao::Falhou(motivo, erro) => {
                        tracing::info!(%mod_id, tunel = %nome, %erro, ?motivo, "túnel: a ligação direta não veio; segue pela estrela");
                        enviar(ClientMessage::TunelFalhou {
                            mod_id,
                            nome,
                            motivo,
                        });
                        return;
                    }
                    EstadoDaLigacao::Caiu => {
                        tracing::info!(%mod_id, tunel = %nome, "túnel: a ligação direta caiu antes de ser usada; segue pela estrela");
                        enviar(ClientMessage::TunelFalhou {
                            mod_id,
                            nome,
                            motivo: MotivoDeFalhaDePar::NaoAlcancou,
                        });
                        return;
                    }
                    EstadoDaLigacao::Discando => {
                        if estado.changed().await.is_err() {
                            return;
                        }
                    }
                }
            }
        }));
    }

    /// Como [`Self::encerrar`], devolvendo a alça da tarefa para quem quiser
    /// esperar o último relatório sair.
    pub(crate) fn encerrar_esperando(
        mut self,
        queda: Queda,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let _ = self.eventos.send(Evento::Fim(queda));
        self.tarefa.take().and_then(AbortaAoLargar::tirar_alca)
    }

    /// Acaba o túnel por este motivo: sai o último relatório e a tarefa
    /// termina sozinha (não é abortada, ou o relatório se perderia).
    pub(crate) fn encerrar(mut self, queda: Queda) {
        let _ = self.eventos.send(Evento::Fim(queda));
        if let Some(tarefa) = self.tarefa.take() {
            tarefa.soltar();
        }
    }
}

/// O canal de controle de um caminho: `Ping` a cada 2 s, e a latência é a ida
/// e volta do `Pong` de ponta a ponta.
async fn pingar(via: Via, contexto: Arc<Contexto>) {
    let mut n = 0_u32;
    loop {
        let canal = via.abrir(Confiabilidade::Ordenada).await;
        if let Ok(CanalDeTunel::Ordenado(mut envio, mut recebe)) = canal {
            let abertura = abertura(
                &contexto.mod_id,
                &contexto.nome,
                0,
                TipoDeCanalDeTunel::Controle,
            );
            if escrever_abertura_t(&mut envio, &abertura).await.is_ok() {
                loop {
                    n = n.wrapping_add(1);
                    let saiu = Instant::now();
                    if crate::frame::write(&mut envio, &SinalDeTunel::Ping { n })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    match tokio::time::timeout(
                        PRAZO_DO_PONG,
                        crate::frame::read::<SinalDeTunel>(&mut recebe),
                    )
                    .await
                    {
                        Ok(Ok(SinalDeTunel::Pong { n: devolvido })) if devolvido == n => {
                            contexto.medidas.medir(Some(saiu.elapsed()));
                        }
                        _ => break,
                    }
                    tokio::time::sleep(INTERVALO_DO_PING).await;
                }
            }
        }
        // O canal de controle caiu ou não abriu: a medida velha não vale.
        contexto.medidas.medir(None);
        tokio::time::sleep(INTERVALO_DO_PING).await;
    }
}

async fn conexao_do_jogo(
    tcp: TcpStream,
    via: Via,
    caminho: &'static str,
    fluxo: u32,
    contexto: Arc<Contexto>,
) {
    let _ = tcp.set_nodelay(true);
    let viva = ConexaoViva::nova(&contexto.medidas);
    let aberto = abrir_t(&via, &contexto, fluxo, TipoDeCanalDeTunel::Tcp).await;
    let (queda, detalhe) = match aberto {
        Ok((envio, recebe)) => {
            let fim = ponte_tcp(
                tcp,
                envio,
                recebe,
                &contexto.medidas.ida,
                &contexto.medidas.volta,
            )
            .await;
            (fim.queda(), format!("{fim:?}"))
        }
        Err(porque) => (Queda::CaminhoCaiu, porque),
    };
    // A conexão deixa de contar como viva ANTES de a queda ser contada: se o
    // túnel acabar agora, ela não entra duas vezes no relatório final.
    drop(viva);
    contexto.terminou(caminho, fluxo, queda, &detalhe);
}

async fn abrir_t(
    via: &Via,
    contexto: &Contexto,
    fluxo: u32,
    tipo: TipoDeCanalDeTunel,
) -> Result<(Envio, Recebe), String> {
    match via.abrir(Confiabilidade::Ordenada).await {
        Ok(CanalDeTunel::Ordenado(mut envio, recebe)) => {
            escrever_abertura_t(
                &mut envio,
                &abertura(&contexto.mod_id, &contexto.nome, fluxo, tipo),
            )
            .await?;
            Ok((envio, recebe))
        }
        Ok(CanalDeTunel::SemOrdem(_)) => Err("o enlace devolveu um canal j para um t".to_owned()),
        Err(motivo) => Err(motivo.to_string()),
    }
}

/// Quem é um fluxo UDP de quem joga.
struct DoFluxo {
    socket: Arc<UdpSocket>,
    origem: SocketAddr,
    caminho: &'static str,
    fluxo: u32,
}

/// Um fluxo UDP de quem joga: um endereço de origem, um canal `j`.
async fn fluxo_udp_do_jogo(
    de: DoFluxo,
    via: Via,
    contexto: Arc<Contexto>,
    mut entrada: mpsc::Receiver<Vec<u8>>,
    eventos: mpsc::UnboundedSender<Evento>,
) {
    let DoFluxo {
        socket,
        origem,
        caminho,
        fluxo,
    } = de;
    let viva = ConexaoViva::nova(&contexto.medidas);
    let (queda, detalhe) = 'fluxo: {
        let mut datagramas: Datagramas = match via.abrir(Confiabilidade::SemOrdem).await {
            Ok(CanalDeTunel::SemOrdem(datagramas)) => datagramas,
            Ok(CanalDeTunel::Ordenado(..)) => {
                break 'fluxo (
                    Queda::CaminhoCaiu,
                    "o enlace devolveu um canal t para um j".to_owned(),
                );
            }
            Err(motivo) => break 'fluxo (Queda::CaminhoCaiu, motivo.to_string()),
        };
        let primeiro = abertura_j(&abertura(
            &contexto.mod_id,
            &contexto.nome,
            fluxo,
            TipoDeCanalDeTunel::Udp,
        ));
        match primeiro {
            Ok(bytes) => {
                if datagramas.mandar(&bytes).is_err() {
                    break 'fluxo (Queda::CaminhoCaiu, "a abertura do j não saiu".to_owned());
                }
            }
            Err(porque) => break 'fluxo (Queda::CaminhoCaiu, porque),
        }
        let mut ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
        loop {
            tokio::select! {
                dado = entrada.recv() => {
                    let Some(dado) = dado else { break 'fluxo (Queda::Fechado, "o túnel largou o fluxo".to_owned()); };
                    ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
                    match datagramas.mandar(&dado) {
                        Ok(()) => { contexto.medidas.ida.fetch_add(dado.len() as u64, Ordering::Relaxed); }
                        // Cheia e Grande já foram contadas pelo enlace, e entram
                        // no relatório por `Via::descartes`.
                        Err(Recusa::Cheia | Recusa::Grande) => {}
                        Err(Recusa::Fechada) => break 'fluxo (Queda::CaminhoCaiu, "o canal j fechou".to_owned()),
                    }
                }
                chegou = datagramas.receber() => {
                    let Some(dado) = chegou else { break 'fluxo (Queda::CaminhoCaiu, "o canal j fechou".to_owned()); };
                    ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
                    contexto.medidas.volta.fetch_add(dado.len() as u64, Ordering::Relaxed);
                    let _ = socket.send_to(&dado, origem).await;
                }
                () = tokio::time::sleep_until(ate) => break 'fluxo (Queda::Fechado, "ocioso por 60 s".to_owned()),
            }
        }
    };
    // A conexão deixa de contar como viva ANTES de a queda ser contada: se o
    // túnel acabar agora, ela não entra duas vezes no relatório final.
    drop(viva);
    contexto.terminou(caminho, fluxo, queda, &detalhe);
    let _ = eventos.send(Evento::FluxoAcabou { origem, fluxo });
}

fn avisar_relatorio(contexto: &Contexto, relatorio: RelatorioDoTunel) {
    let _ = contexto.avisos.send(Aviso::Tunel(relatorio));
}

/// O laço de um túnel de quem joga.
async fn laco_do_jogador(
    contexto: Arc<Contexto>,
    escuta: Escuta,
    estrela: Via,
    tx: mpsc::UnboundedSender<Evento>,
    mut rx: mpsc::UnboundedReceiver<Evento>,
) {
    let medidas = Arc::clone(&contexto.medidas);
    let mut filhos: JoinSet<()> = JoinSet::new();
    let mut via = estrela.clone();
    let mut caminho = Caminho::Estrela;
    let mut geracao = 0_u32;
    let mut ligado: Option<Arc<ParLigado>> = None;
    let mut pingador = AbortaAoLargar::nova(pingar(via.clone(), Arc::clone(&contexto)));
    {
        let (tx, estrela) = (tx.clone(), estrela.clone());
        filhos.spawn(async move {
            let motivo = estrela.fechou().await;
            tracing::debug!(%motivo, "túnel: a conexão com o servidor acabou");
            let _ = tx.send(Evento::Fim(Queda::ServidorSaiu));
        });
    }
    let mut fluxos: HashMap<SocketAddr, (u32, mpsc::Sender<Vec<u8>>)> = HashMap::new();
    let mut proximo_fluxo = 0_u32;
    let mut buf_udp = vec![0_u8; 65_536];
    let mut tica = tokio::time::interval(TICA_DO_RELATORIO);
    tica.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (mut ida_antes, mut volta_antes) = (0_u64, 0_u64);
    let mut instante = Instant::now();
    let mut tinha_trafego = false;
    let mut descartes_vistos = 0_u64;
    let mut mudou = true;
    let mut descartes_acumulados = 0_u64;
    let mut base_de_descartes = via.descartes();
    let (mut ida_bps, mut volta_bps) = (0_u64, 0_u64);

    let fim = loop {
        tokio::select! {
            aceito = esperar_tcp(&escuta) => {
                match aceito {
                    Ok((tcp, origem)) => {
                        proximo_fluxo = proximo_fluxo.wrapping_add(1);
                        tracing::debug!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), fluxo = proximo_fluxo, %origem, "túnel: conexão do jogo aceita");
                        filhos.spawn(conexao_do_jogo(tcp, via.clone(), caminho.rotulo(), proximo_fluxo, Arc::clone(&contexto)));
                    }
                    Err(erro) => tracing::warn!(%erro, mod_id = %contexto.mod_id, tunel = %contexto.nome, "túnel: o accept da porta local falhou"),
                }
            }
            () = esperar_udp(&escuta) => {
                if let Escuta::Udp(socket) = &escuta {
                    match socket.try_recv_from(&mut buf_udp) {
                        Ok((n, origem)) => {
                            let dado = buf_udp.get(..n).unwrap_or_default().to_vec();
                            enviar_ao_fluxo(
                                socket, origem, dado, &mut fluxos, &mut proximo_fluxo, &via, caminho.rotulo(), &contexto, &tx, &mut filhos,
                            );
                        }
                        Err(erro) if erro.kind() == std::io::ErrorKind::WouldBlock => {}
                        // No Windows um ICMP de volta vira erro de leitura do
                        // socket; não é do túnel, e o próximo datagrama segue.
                        Err(erro) => tracing::debug!(%erro, "túnel: erro de leitura na porta UDP local"),
                    }
                }
            }
            evento = rx.recv() => {
                let Some(evento) = evento else { break Queda::Fechado; };
                match evento {
                    Evento::Mudou => mudou = true,
                    Evento::Fim(queda) => break queda,
                    Evento::Direto(par, quem) => {
                        geracao = geracao.wrapping_add(1);
                        descartes_acumulados = descartes_acumulados.saturating_add(via.descartes().saturating_sub(base_de_descartes));
                        via = Via::nova(par.conexao.clone());
                        base_de_descartes = via.descartes();
                        caminho = Caminho::Direto { par: quem };
                        tracing::info!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), "túnel: as conexões novas vão pela ligação direta");
                        let g = geracao;
                        let (tx, conexao) = (tx.clone(), par.conexao.clone());
                        let (mod_id, nome) = (contexto.mod_id.clone(), contexto.nome.clone());
                        filhos.spawn(async move {
                            let motivo = conexao.closed().await;
                            tracing::warn!(%mod_id, tunel = %nome, caminho = "direto", %motivo, "túnel: a ligação direta caiu");
                            let _ = tx.send(Evento::DiretoCaiu(g));
                        });
                        ligado = Some(par);
                        medidas.medir(None);
                        pingador = AbortaAoLargar::nova(pingar(via.clone(), Arc::clone(&contexto)));
                        mudou = true;
                    }
                    Evento::DiretoCaiu(g) if g == geracao && ligado.is_some() => {
                        descartes_acumulados = descartes_acumulados.saturating_add(via.descartes().saturating_sub(base_de_descartes));
                        via = estrela.clone();
                        base_de_descartes = via.descartes();
                        caminho = Caminho::Estrela;
                        ligado = None;
                        tracing::warn!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), "túnel: voltou para a estrela");
                        medidas.medir(None);
                        pingador = AbortaAoLargar::nova(pingar(via.clone(), Arc::clone(&contexto)));
                        mudou = true;
                    }
                    Evento::DiretoCaiu(_) => {}
                    Evento::FluxoAcabou { origem, fluxo } => {
                        if fluxos.get(&origem).is_some_and(|(id, _)| *id == fluxo) {
                            fluxos.remove(&origem);
                        }
                    }
                }
                if mudou {
                    let descartes = descartes_acumulados.saturating_add(via.descartes().saturating_sub(base_de_descartes));
                    avisar_relatorio(&contexto, contexto.relatorio(caminho, ida_bps, volta_bps, descartes, None));
                    mudou = false;
                }
            }
            _ = tica.tick() => {
                let (ida, volta) = (medidas.ida.load(Ordering::Relaxed), medidas.volta.load(Ordering::Relaxed));
                let ms = u64::try_from(instante.elapsed().as_millis()).unwrap_or(u64::MAX).max(1);
                instante = Instant::now();
                ida_bps = ida.saturating_sub(ida_antes).saturating_mul(1000) / ms;
                volta_bps = volta.saturating_sub(volta_antes).saturating_mul(1000) / ms;
                let trafego = ida != ida_antes || volta != volta_antes;
                (ida_antes, volta_antes) = (ida, volta);
                // Um relatório a mais depois de o tráfego parar, para a taxa
                // não ficar presa no último valor não nulo.
                let descartes = descartes_acumulados.saturating_add(via.descartes().saturating_sub(base_de_descartes));
                // E um sempre que um datagrama é descartado: o que se perde
                // tem de chegar à casca, mesmo quando mais nada mudou.
                let total = descartes.saturating_add(medidas.udp_perdidos.load(Ordering::Relaxed));
                if trafego || tinha_trafego || mudou || total != descartes_vistos {
                    avisar_relatorio(&contexto, contexto.relatorio(caminho, ida_bps, volta_bps, descartes, None));
                    descartes_vistos = total;
                    mudou = false;
                }
                tinha_trafego = trafego;
            }
            Some(_) = filhos.join_next(), if !filhos.is_empty() => {}
        }
    };

    // O fim: cada conexão viva acabou por este motivo, e o túnel também.
    let vivas = medidas.conexoes.load(Ordering::Relaxed);
    for _ in 0..=vivas {
        medidas.quedar(fim);
    }
    let descartes =
        descartes_acumulados.saturating_add(via.descartes().saturating_sub(base_de_descartes));
    tracing::info!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), ?fim, vivas, "túnel: encerrado");
    let mut ultimo = contexto.relatorio(caminho, 0, 0, descartes, Some(fim));
    ultimo.conexoes = 0;
    avisar_relatorio(&contexto, ultimo);
    drop(pingador);
    drop(ligado);
    drop(filhos);
}

async fn esperar_tcp(escuta: &Escuta) -> std::io::Result<(TcpStream, SocketAddr)> {
    match escuta {
        Escuta::Tcp(listener) => listener.accept().await,
        Escuta::Udp(_) => pending().await,
    }
}

async fn esperar_udp(escuta: &Escuta) {
    match escuta {
        Escuta::Udp(socket) => {
            let _ = socket.readable().await;
        }
        Escuta::Tcp(_) => pending().await,
    }
}

#[allow(clippy::too_many_arguments)]
fn enviar_ao_fluxo(
    socket: &Arc<UdpSocket>,
    origem: SocketAddr,
    dado: Vec<u8>,
    fluxos: &mut HashMap<SocketAddr, (u32, mpsc::Sender<Vec<u8>>)>,
    proximo_fluxo: &mut u32,
    via: &Via,
    caminho: &'static str,
    contexto: &Arc<Contexto>,
    eventos: &mpsc::UnboundedSender<Evento>,
    filhos: &mut JoinSet<()>,
) {
    let mut dado = Some(dado);
    if let Some((_, fila)) = fluxos.get(&origem) {
        match fila.try_send(dado.take().unwrap_or_default()) {
            Ok(()) => return,
            Err(mpsc::error::TrySendError::Full(_)) => {
                contexto
                    .medidas
                    .udp_perdidos
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            // O fluxo acabou e o aviso ainda não chegou: o datagrama abre outro.
            Err(mpsc::error::TrySendError::Closed(devolvido)) => dado = Some(devolvido),
        }
    }
    *proximo_fluxo = proximo_fluxo.wrapping_add(1);
    let fluxo = *proximo_fluxo;
    let (tx, rx) = mpsc::channel(FILA_DO_FLUXO);
    if let Some(dado) = dado {
        let _ = tx.try_send(dado);
    }
    fluxos.insert(origem, (fluxo, tx));
    tracing::debug!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho, fluxo, %origem, "túnel: fluxo UDP novo");
    filhos.spawn(fluxo_udp_do_jogo(
        DoFluxo {
            socket: Arc::clone(socket),
            origem,
            caminho,
            fluxo,
        },
        via.clone(),
        Arc::clone(contexto),
        rx,
        eventos.clone(),
    ));
}

// ------------------------------------------------------------- a máquina

/// Um túnel que esta máquina serve.
struct Servido {
    protocolo: ProtocoloDeTunel,
    porta: u16,
    medidas: Medidas,
    /// `true` quando a máquina parou ou o servidor encerrou: as conexões
    /// vivas acabam com o motivo guardado em `motivo`.
    parada: watch::Sender<Option<Queda>>,
}

/// O que as tarefas da máquina compartilham.
pub(crate) struct EstadoDaMaquina {
    servidos: Mutex<HashMap<Chave, Arc<Servido>>>,
    /// Canais cortados porque a abertura nomeava outro túnel, outro tipo, ou
    /// um túnel que esta máquina não serve.
    alheios: AtomicU64,
}

impl EstadoDaMaquina {
    fn servido(&self, chave: &Chave) -> Option<Arc<Servido>> {
        self.servidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(chave)
            .filter(|s| s.parada.borrow().is_none())
            .cloned()
    }

    /// Os motivos pelos quais conexões deste túnel acabaram, na máquina.
    #[must_use]
    pub(crate) fn quedas(&self, mod_id: &str, nome: &str) -> Vec<(Queda, u32)> {
        self.servidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(mod_id.to_owned(), nome.to_owned()))
            .map(|s| s.medidas.quedas())
            .unwrap_or_default()
    }

    /// Quantos canais foram cortados por nomearem outro túnel.
    #[must_use]
    pub(crate) fn canais_alheios(&self) -> u64 {
        self.alheios.load(Ordering::Relaxed)
    }
}

/// O lado da máquina: aceita canais e os liga no jogo.
pub(crate) struct Maquina {
    estado: Arc<EstadoDaMaquina>,
    tarefas: JoinSet<()>,
    na_estrela: bool,
    /// Quem a máquina está atendendo (ou esperando) pela malha: **uma ligação
    /// por pessoa**, com os túneis que o servidor apresentou a ela.
    atendendo: Arc<Mutex<HashMap<PersonId, Atendimento>>>,
    proximo_atendimento: u64,
}

/// Os túneis que o servidor apresentou a uma pessoa: o que a ligação direta
/// dela pode abrir. Um canal que nomeia outro túnel é cortado.
pub(crate) struct Permitidos(Mutex<HashSet<Chave>>);

/// Quanto um canal que chega por uma ligação direta espera pela apresentação
/// do túnel que nomeia. O servidor apresenta o túnel à máquina e a quem joga
/// por dois caminhos independentes, e quem joga pode abrir o canal um instante
/// antes de a máquina ouvir a apresentação; sem a espera, esse canal seria
/// cortado por uma corrida, e não por falta de permissão.
const GRACA_DA_APRESENTACAO: Duration = Duration::from_millis(1500);

impl Permitidos {
    pub(crate) fn de(chave: Chave) -> Arc<Self> {
        Arc::new(Self(Mutex::new(HashSet::from([chave]))))
    }

    fn permitir(&self, chave: Chave) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(chave);
    }

    fn permite(&self, chave: &Chave) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(chave)
    }

    /// Se o túnel é permitido agora ou passa a ser dentro da graça.
    async fn permite_logo(&self, chave: &Chave) -> bool {
        let fim = Instant::now() + GRACA_DA_APRESENTACAO;
        loop {
            if self.permite(chave) {
                return true;
            }
            if Instant::now() >= fim {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// O que `SirvaTunelPara` entrega para atender quem vai jogar.
pub(crate) struct Apresentacao {
    pub(crate) ponta: PontaDaSessao,
    pub(crate) quem: PersonId,
    pub(crate) mod_id: String,
    pub(crate) nome: String,
    pub(crate) enderecos: Vec<SocketAddr>,
    pub(crate) impressao: String,
    pub(crate) prazo: Duration,
}

pub(crate) struct Atendimento {
    id: u64,
    permitidos: Arc<Permitidos>,
}

impl Maquina {
    pub(crate) fn nova() -> Self {
        Self {
            estado: Arc::new(EstadoDaMaquina {
                servidos: Mutex::new(HashMap::new()),
                alheios: AtomicU64::new(0),
            }),
            tarefas: JoinSet::new(),
            na_estrela: false,
            atendendo: Arc::new(Mutex::new(HashMap::new())),
            proximo_atendimento: 0,
        }
    }

    pub(crate) fn estado(&self) -> Arc<EstadoDaMaquina> {
        Arc::clone(&self.estado)
    }

    /// Passa a servir `mod_id/nome`: as aberturas que o nomearem ligam em
    /// `127.0.0.1:porta`.
    pub(crate) fn registrar(
        &mut self,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
    ) {
        let (parada, _) = watch::channel(None);
        self.estado
            .servidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                (mod_id.to_owned(), nome.to_owned()),
                Arc::new(Servido {
                    protocolo,
                    porta,
                    medidas: Medidas::nova(None),
                    parada,
                }),
            );
    }

    /// Deixa de servir: as conexões vivas acabam com `queda`.
    pub(crate) fn parar(&mut self, mod_id: &str, nome: &str, queda: Queda) -> bool {
        let guarda = self
            .estado
            .servidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guarda
            .get(&(mod_id.to_owned(), nome.to_owned()))
            .is_some_and(|s| s.parada.send_replace(Some(queda)).is_none())
    }

    /// Se esta máquina serve `mod_id/nome` agora.
    pub(crate) fn serve(&self, mod_id: &str, nome: &str) -> bool {
        self.estado
            .servido(&(mod_id.to_owned(), nome.to_owned()))
            .is_some()
    }

    /// Começa a aceitar os canais que o servidor repassa. Uma vez por conexão.
    pub(crate) fn aceitar_pela_estrela(&mut self, via: Via) {
        if self.na_estrela {
            return;
        }
        self.na_estrela = true;
        let estado = Arc::clone(&self.estado);
        self.tarefas
            .spawn(aceitar_canais(via, None, estado, "estrela"));
    }

    /// `SirvaTunelPara`: atende a ligação direta de quem vai jogar, conferida
    /// pela impressão, e aceita nela só canais dos túneis que o servidor
    /// apresentou a essa pessoa.
    ///
    /// **A ligação é da pessoa, e não do túnel.** Quem joga dois túneis desta
    /// máquina recebe duas apresentações, e atender cada uma por conta própria
    /// as faria disputar a mesma conexão de entrada (ver [`Ligacoes`]). A
    /// segunda apresentação só acrescenta o túnel ao que a ligação já aceita.
    pub(crate) fn servir_a(&mut self, apresentacao: Apresentacao) {
        let Apresentacao {
            ponta,
            quem,
            mod_id,
            nome,
            enderecos,
            impressao,
            prazo,
        } = apresentacao;
        if !self.serve(&mod_id, &nome) {
            tracing::debug!(%mod_id, tunel = %nome, "túnel: apresentação para um túnel que esta máquina não serve");
            return;
        }
        let permitidos = {
            let mut atendendo = self
                .atendendo
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(ja) = atendendo.get(&quem) {
                ja.permitidos.permitir((mod_id.clone(), nome.clone()));
                tracing::debug!(%mod_id, tunel = %nome, %quem, "túnel: a ligação direta desta pessoa já está sendo atendida; o túnel entra nela");
                return;
            }
            self.proximo_atendimento += 1;
            let permitidos = Permitidos::de((mod_id.clone(), nome.clone()));
            atendendo.insert(
                quem,
                Atendimento {
                    id: self.proximo_atendimento,
                    permitidos: Arc::clone(&permitidos),
                },
            );
            permitidos
        };
        let id = self.proximo_atendimento;
        let estado = Arc::clone(&self.estado);
        let atendendo = Arc::clone(&self.atendendo);
        self.tarefas.spawn(async move {
            match cumprir_apresentacao(&ponta, Papel::Serve, &enderecos, impressao, prazo).await {
                Ok(ligado) => {
                    tracing::info!(%mod_id, tunel = %nome, %quem, "túnel: ligação direta atendida");
                    // `ligado` fica vivo enquanto o aceitador correr.
                    let via = Via::nova(ligado.conexao.clone());
                    aceitar_canais(via, Some(permitidos), estado, "direto").await;
                    tracing::info!(%mod_id, tunel = %nome, %quem, caminho = "direto", "túnel: a ligação direta da máquina acabou");
                    drop(ligado);
                }
                Err(erro) => {
                    tracing::info!(%mod_id, tunel = %nome, %quem, %erro, "túnel: ninguém veio pela ligação direta; fica a estrela");
                }
            }
            let mut atendendo = atendendo.lock().unwrap_or_else(PoisonError::into_inner);
            if atendendo.get(&quem).is_some_and(|a| a.id == id) {
                atendendo.remove(&quem);
            }
        });
    }

    /// A conexão com o servidor acabou, ou o enlace termina: nada mais serve.
    pub(crate) fn derrubar_tudo(&mut self, queda: Queda) {
        for servido in self
            .estado
            .servidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            servido.parada.send_replace(Some(queda));
        }
        self.tarefas.abort_all();
        self.atendendo
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.na_estrela = false;
    }
}

/// Aceita canais de uma via até ela acabar. `restrito` são os túneis a que a
/// via pertence (ligação direta); a estrela aceita qualquer túnel servido.
async fn aceitar_canais(
    via: Via,
    restrito: Option<Arc<Permitidos>>,
    estado: Arc<EstadoDaMaquina>,
    caminho: &'static str,
) {
    let mut filhos: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            canal = via.aceitar() => match canal {
                Ok(canal) => {
                    filhos.spawn(atender_canal(canal, restrito.clone(), Arc::clone(&estado), caminho));
                }
                Err(motivo) => {
                    tracing::debug!(caminho, %motivo, "túnel: a via de canais acabou");
                    return;
                }
            },
            Some(_) = filhos.join_next(), if !filhos.is_empty() => {}
        }
    }
}

/// Confere a abertura contra o que a máquina serve e contra a via de onde veio.
fn conferir(
    abertura: &AberturaDeTunel,
    da_via: bool,
    estado: &EstadoDaMaquina,
    caminho: &str,
) -> Option<Arc<Servido>> {
    let chave = (abertura.mod_id.clone(), abertura.nome.clone());
    let corte = |porque: &str| {
        estado.alheios.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(mod_id = %abertura.mod_id, tunel = %abertura.nome, caminho, porque, "túnel: canal cortado na máquina");
    };
    if !da_via {
        corte("a ligação direta não é deste túnel");
        return None;
    }
    let Some(servido) = estado.servido(&chave) else {
        corte("esta máquina não serve esse túnel");
        return None;
    };
    let combina = matches!(
        (abertura.tipo, servido.protocolo),
        (TipoDeCanalDeTunel::Tcp, ProtocoloDeTunel::Tcp)
            | (TipoDeCanalDeTunel::Udp, ProtocoloDeTunel::Udp)
            | (TipoDeCanalDeTunel::Controle, _)
    );
    if !combina {
        corte("o tipo do canal não é o do túnel");
        return None;
    }
    Some(servido)
}

async fn atender_canal(
    canal: CanalDeTunel,
    restrito: Option<Arc<Permitidos>>,
    estado: Arc<EstadoDaMaquina>,
    caminho: &'static str,
) {
    // Se o túnel nomeado é um dos que esta via pode abrir.
    let da_via = async |aberta: &AberturaDeTunel| match &restrito {
        None => true,
        Some(permitidos) => {
            permitidos
                .permite_logo(&(aberta.mod_id.clone(), aberta.nome.clone()))
                .await
        }
    };
    match canal {
        CanalDeTunel::Ordenado(mut envio, mut recebe) => {
            let lida = tokio::time::timeout(
                PRAZO_DA_ABERTURA,
                crate::frame::read::<AberturaDeTunel>(&mut recebe),
            )
            .await;
            let Ok(Ok(aberta)) = lida else {
                tracing::warn!(caminho, "túnel: canal t sem abertura legível, cortado");
                estado.alheios.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let Some(servido) = conferir(&aberta, da_via(&aberta).await, &estado, caminho) else {
                let _ = envio.reset(CODIGO_CORTADO);
                let _ = recebe.stop(CODIGO_CORTADO);
                return;
            };
            match aberta.tipo {
                TipoDeCanalDeTunel::Controle => responder_aos_pings(envio, recebe).await,
                _ => tcp_da_maquina(&aberta, &servido, envio, recebe, caminho).await,
            }
        }
        CanalDeTunel::SemOrdem(mut datagramas) => {
            let primeiro = tokio::time::timeout(PRAZO_DA_ABERTURA, datagramas.receber()).await;
            let aberta = match primeiro {
                Ok(Some(bytes)) => seele_proto::control::decode::<AberturaDeTunel>(&bytes).ok(),
                _ => None,
            };
            let Some(aberta) = aberta else {
                tracing::warn!(caminho, "túnel: canal j sem abertura legível, cortado");
                estado.alheios.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let Some(servido) = conferir(&aberta, da_via(&aberta).await, &estado, caminho) else {
                return;
            };
            if aberta.tipo == TipoDeCanalDeTunel::Udp {
                udp_da_maquina(&aberta, &servido, datagramas, caminho).await;
            } else {
                estado.alheios.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// O canal de controle: cada `Ping` volta como `Pong`.
async fn responder_aos_pings(mut envio: Envio, mut recebe: Recebe) {
    while let Ok(sinal) = crate::frame::read::<SinalDeTunel>(&mut recebe).await {
        if let SinalDeTunel::Ping { n } = sinal {
            if crate::frame::write(&mut envio, &SinalDeTunel::Pong { n })
                .await
                .is_err()
            {
                return;
            }
        }
    }
}

/// Espera a máquina parar, e devolve por quê. Devolve o valor, e não o guarda
/// do `watch`, que não atravessa um `await` numa tarefa solta.
async fn esperar_parada(parada: &mut watch::Receiver<Option<Queda>>) -> Queda {
    match parada.wait_for(Option::is_some).await {
        Ok(visto) => (*visto).unwrap_or(Queda::MaquinaParou),
        Err(_) => Queda::MaquinaParou,
    }
}

fn alvo_do_jogo(servido: &Servido) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, servido.porta))
}

async fn tcp_da_maquina(
    aberta: &AberturaDeTunel,
    servido: &Arc<Servido>,
    mut envio: Envio,
    recebe: Recebe,
    caminho: &'static str,
) {
    let (mod_id, nome, fluxo) = (&aberta.mod_id, &aberta.nome, aberta.fluxo);
    let tcp = match TcpStream::connect(alvo_do_jogo(servido)).await {
        Ok(tcp) => tcp,
        Err(erro) => {
            // O jogo não escuta: quem joga vê o canal cortado, e esta é a
            // única ponta que sabe por quê.
            tracing::warn!(%mod_id, tunel = %nome, caminho, fluxo, porta = servido.porta, %erro, ?aberta.de, "túnel: o jogo recusou a conexão");
            servido.medidas.quedar(Queda::JogoRecusou);
            let _ = envio.reset(CODIGO_CORTADO);
            drop(recebe);
            return;
        }
    };
    let _ = tcp.set_nodelay(true);
    let viva = ConexaoViva::nova(&servido.medidas);
    let mut parada = servido.parada.subscribe();
    // Da máquina, `sai` é a volta do ponto de vista de quem joga.
    let ponte = ponte_tcp(
        tcp,
        envio,
        recebe,
        &servido.medidas.volta,
        &servido.medidas.ida,
    );
    let queda = tokio::select! {
        fim = ponte => fim.queda(),
        queda = esperar_parada(&mut parada) => queda,
    };
    if queda == Queda::Fechado {
        tracing::debug!(%mod_id, tunel = %nome, caminho, fluxo, "túnel: conexão fechada na máquina");
    } else {
        tracing::warn!(%mod_id, tunel = %nome, caminho, fluxo, ?queda, "túnel: conexão caiu na máquina");
    }
    drop(viva);
    servido.medidas.quedar(queda);
}

async fn udp_da_maquina(
    aberta: &AberturaDeTunel,
    servido: &Arc<Servido>,
    mut datagramas: Datagramas,
    caminho: &'static str,
) {
    let (mod_id, nome, fluxo) = (&aberta.mod_id, &aberta.nome, aberta.fluxo);
    let ligar = async {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        socket.connect(alvo_do_jogo(servido)).await?;
        std::io::Result::Ok(socket)
    };
    let socket = match ligar.await {
        Ok(socket) => socket,
        Err(erro) => {
            tracing::warn!(%mod_id, tunel = %nome, caminho, fluxo, %erro, "túnel: a máquina não abriu o socket UDP do fluxo");
            servido.medidas.quedar(Queda::JogoRecusou);
            return;
        }
    };
    let viva = ConexaoViva::nova(&servido.medidas);
    let mut parada = servido.parada.subscribe();
    let mut buf = vec![0_u8; 65_536];
    let queda = loop {
        tokio::select! {
            chegou = datagramas.receber() => {
                let Some(dado) = chegou else { break Queda::Fechado; };
                servido.medidas.ida.fetch_add(dado.len() as u64, Ordering::Relaxed);
                match socket.send(&dado).await {
                    Ok(_) => {}
                    Err(erro) if erro.kind() == std::io::ErrorKind::ConnectionRefused => break Queda::JogoRecusou,
                    Err(erro) => tracing::debug!(%erro, "túnel: o jogo não recebeu um datagrama"),
                }
            }
            lido = socket.recv(&mut buf) => match lido {
                Ok(n) => {
                    servido.medidas.volta.fetch_add(n as u64, Ordering::Relaxed);
                    let dado = buf.get(..n).unwrap_or_default();
                    // Cheia e Grande já foram contadas pelo enlace.
                    if datagramas.mandar(dado) == Err(Recusa::Fechada) { break Queda::CaminhoCaiu; }
                }
                Err(erro) if erro.kind() == std::io::ErrorKind::ConnectionRefused => break Queda::JogoRecusou,
                Err(erro) => {
                    tracing::debug!(%erro, "túnel: erro lendo o socket UDP do jogo");
                    break Queda::Fechado;
                }
            },
            queda = esperar_parada(&mut parada) => break queda,
        }
    };
    if queda == Queda::Fechado {
        tracing::debug!(%mod_id, tunel = %nome, caminho, fluxo, "túnel: fluxo UDP fechado na máquina");
    } else {
        tracing::warn!(%mod_id, tunel = %nome, caminho, fluxo, ?queda, "túnel: fluxo UDP caiu na máquina");
    }
    drop(viva);
    servido.medidas.quedar(queda);
}

// ------------------------------------------------------- o que o enlace usa

/// Os motivos de fim que o servidor anuncia, na linguagem das quedas.
#[must_use]
pub(crate) fn queda_do_fim(motivo: MotivoDoFimDoTunel) -> Queda {
    match motivo {
        MotivoDoFimDoTunel::MaquinaParou => Queda::MaquinaParou,
        MotivoDoFimDoTunel::MaquinaSaiu => Queda::MaquinaSaiu,
        MotivoDoFimDoTunel::ModDesligado => Queda::ModDesligado,
        MotivoDoFimDoTunel::SemPermissao => Queda::SemPermissao,
        MotivoDoFimDoTunel::PessoaSaiu => Queda::Fechado,
        MotivoDoFimDoTunel::AnfitriaoCaiu => Queda::ServidorSaiu,
    }
}

/// O que um `TunelRecusado` **depois** do `TunelConcedido` quer dizer: `Some`
/// quando acaba o túnel inteiro, `None` quando o servidor só cortou um canal
/// (a conexão que ele levava acaba sozinha, como `CaminhoCaiu`).
#[must_use]
pub(crate) fn fim_pela_recusa(motivo: RecusaDeTunel) -> Option<Queda> {
    match motivo {
        RecusaDeTunel::MaquinaSaiu => Some(Queda::MaquinaSaiu),
        RecusaDeTunel::SemPermissao => Some(Queda::SemPermissao),
        RecusaDeTunel::ModSemPorta => Some(Queda::ModDesligado),
        RecusaDeTunel::NaoExiste => Some(Queda::MaquinaParou),
        RecusaDeTunel::AcaoNaoDeclarada
        | RecusaDeTunel::NomeTomado
        | RecusaDeTunel::TuneisDemais
        | RecusaDeTunel::CanaisEsgotados
        | RecusaDeTunel::ServidorAntigo
        | RecusaDeTunel::PortaLocalOcupada
        | RecusaDeTunel::SemResposta => None,
    }
}

/// Os pedidos em andamento e os túneis vivos de um enlace.
pub(crate) struct Tuneis {
    jogando: HashMap<Chave, TunelDeJogador>,
    ligacoes: Ligacoes,
    /// Quem espera a porta de um túnel. Mais de um quando a pessoa clica de novo
    /// antes de o servidor responder: o verbo sai uma vez só.
    entrando: HashMap<Chave, Vec<oneshot::Sender<Result<PortaLocal, RecusaDeTunel>>>>,
    servindo: HashMap<Chave, oneshot::Sender<Result<(), RecusaDeTunel>>>,
    maquina: Maquina,
    avisos: mpsc::UnboundedSender<Aviso>,
}

impl Tuneis {
    pub(crate) fn nova(avisos: mpsc::UnboundedSender<Aviso>) -> Self {
        Self {
            jogando: HashMap::new(),
            ligacoes: Ligacoes::default(),
            entrando: HashMap::new(),
            servindo: HashMap::new(),
            maquina: Maquina::nova(),
            avisos,
        }
    }

    pub(crate) fn estado_da_maquina(&self) -> Arc<EstadoDaMaquina> {
        self.maquina.estado()
    }

    /// Registra quem espera a porta de um túnel. Devolve `true` quando o verbo
    /// `EntrarNoTunel` precisa sair, e `false` quando o pedido já foi atendido
    /// ou já está em andamento.
    ///
    /// **Entrar de novo num túnel aberto não refaz nada.** Um segundo clique em
    /// JOGAR chegava aqui, sobrescrevia a espera, mandava outro
    /// `EntrarNoTunel`, e a concessão que voltava substituía o túnel vivo (a
    /// porta velha fechava e levava a conexão do jogo, e a nova, com o jogo
    /// ainda em cima da velha, podia cair numa porta «ocupada» inventada). O
    /// túnel já aberto responde com a descrição que tem.
    pub(crate) fn pedir_entrada(
        &mut self,
        chave: Chave,
        resposta: oneshot::Sender<Result<PortaLocal, RecusaDeTunel>>,
    ) -> bool {
        if let Some(aberto) = self.jogando.get(&chave) {
            tracing::info!(mod_id = %chave.0, tunel = %chave.1, "túnel: já aberto; a porta de antes é devolvida");
            let _ = resposta.send(Ok(aberto.porta.descrita()));
            return false;
        }
        let fila = self.entrando.entry(chave).or_default();
        fila.push(resposta);
        fila.len() == 1
    }

    pub(crate) fn pedir_servico(
        &mut self,
        chave: Chave,
        resposta: oneshot::Sender<Result<(), RecusaDeTunel>>,
    ) {
        self.servindo.insert(chave, resposta);
    }

    /// Responde `recusa` a um pedido cujo verbo não saiu (ou foi recusado).
    pub(crate) fn recusar_pedido(&mut self, chave: &Chave, recusa: RecusaDeTunel) {
        for resposta in self.entrando.remove(chave).into_iter().flatten() {
            let _ = resposta.send(Err(recusa));
        }
        if let Some(resposta) = self.servindo.remove(chave) {
            let _ = resposta.send(Err(recusa));
        }
    }

    /// Deixa o túnel de quem joga. `true` se havia um.
    pub(crate) fn sair(&mut self, mod_id: &str, nome: &str) -> bool {
        if let Some(tunel) = self.jogando.remove(&(mod_id.to_owned(), nome.to_owned())) {
            tunel.encerrar(Queda::Fechado);
            self.ligacoes.podar(&self.jogando);
            true
        } else {
            false
        }
    }

    /// Deixa todos os túneis de um MOD.
    pub(crate) fn sair_do_mod(&mut self, mod_id: &str) -> Vec<String> {
        let nomes: Vec<String> = self
            .jogando
            .keys()
            .filter(|(m, _)| m == mod_id)
            .map(|(_, n)| n.clone())
            .collect();
        for nome in &nomes {
            self.sair(mod_id, nome);
        }
        nomes
    }

    /// A máquina para de servir. `true` se servia.
    pub(crate) fn parar_servico(&mut self, mod_id: &str, nome: &str) -> bool {
        self.maquina.parar(mod_id, nome, Queda::MaquinaParou)
    }

    /// Como [`Self::derrubar_tudo`], e só volta quando o último relatório de
    /// cada túnel (com `encerrado`) já saiu — ou o `teto` venceu. É o que deixa
    /// o enlace que acaba entregar esses relatórios **antes** do `Encerrado`.
    pub(crate) async fn derrubar_e_esperar(&mut self, queda: Queda, teto: Duration) {
        let esperas: Vec<_> = self
            .jogando
            .drain()
            .filter_map(|(_, tunel)| tunel.encerrar_esperando(queda))
            .collect();
        self.ligacoes.por_maquina.clear();
        self.entrando.clear();
        self.servindo.clear();
        self.maquina.derrubar_tudo(queda);
        let _ = tokio::time::timeout(teto, async {
            for espera in esperas {
                let _ = espera.await;
            }
        })
        .await;
    }

    /// Tudo o que uma conexão perdida leva junto.
    pub(crate) fn derrubar_tudo(&mut self, queda: Queda) {
        for (_, tunel) in self.jogando.drain() {
            tunel.encerrar(queda);
        }
        self.ligacoes.por_maquina.clear();
        self.entrando.clear();
        self.servindo.clear();
        self.maquina.derrubar_tudo(queda);
    }

    /// O que uma mensagem do servidor faz com os túneis.
    pub(crate) fn tratar(
        &mut self,
        mensagem: &ServerMessage,
        estrela: Option<&Conexao>,
        ponta: Option<&PontaDaSessao>,
        enviar: &Enviar,
        prazo: Duration,
    ) {
        match mensagem {
            ServerMessage::TunelConcedido {
                mod_id,
                nome,
                protocolo,
                porta,
            } => {
                self.concedido(mod_id, nome, *protocolo, *porta, estrela, enviar);
            }
            ServerMessage::TunelRecusado {
                mod_id,
                nome,
                motivo,
            } => {
                self.recusado(mod_id, nome, *motivo);
            }
            ServerMessage::TunelEncerrado {
                mod_id,
                nome,
                motivo,
            } => {
                let queda = queda_do_fim(*motivo);
                let chave = (mod_id.clone(), nome.clone());
                if let Some(tunel) = self.jogando.remove(&chave) {
                    tunel.encerrar(queda);
                    self.ligacoes.podar(&self.jogando);
                }
                self.maquina.parar(mod_id, nome, queda);
            }
            ServerMessage::AlcanceTunelPor {
                mod_id,
                nome,
                quem,
                enderecos,
                impressao,
            } => {
                let chave = (mod_id.clone(), nome.clone());
                let Some(tunel) = self.jogando.get_mut(&chave) else {
                    tracing::debug!(%mod_id, tunel = %nome, "túnel: alcance por um túnel que esta pessoa não joga");
                    return;
                };
                let Some(ponta) = ponta else {
                    enviar(ClientMessage::TunelFalhou {
                        mod_id: mod_id.clone(),
                        nome: nome.clone(),
                        motivo: MotivoDeFalhaDePar::NaoAlcancou,
                    });
                    return;
                };
                tunel.alcancar_por(
                    Alcance {
                        mod_id: mod_id.clone(),
                        nome: nome.clone(),
                        ponta: ponta.clone(),
                        enderecos: enderecos.clone(),
                        impressao: impressao.clone(),
                        prazo,
                        quem: *quem,
                    },
                    Arc::clone(enviar),
                    &mut self.ligacoes,
                );
            }
            ServerMessage::SirvaTunelPara {
                mod_id,
                nome,
                quem,
                enderecos,
                impressao,
                ..
            } => {
                let Some(ponta) = ponta else { return };
                self.maquina.servir_a(Apresentacao {
                    ponta: ponta.clone(),
                    quem: *quem,
                    mod_id: mod_id.clone(),
                    nome: nome.clone(),
                    enderecos: enderecos.clone(),
                    impressao: impressao.clone(),
                    prazo,
                });
            }
            _ => {}
        }
    }

    fn concedido(
        &mut self,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
        estrela: Option<&Conexao>,
        enviar: &Enviar,
    ) {
        let chave = (mod_id.to_owned(), nome.to_owned());
        if let Some(resposta) = self.servindo.remove(&chave) {
            self.maquina.registrar(mod_id, nome, protocolo, porta);
            if let Some(estrela) = estrela {
                self.maquina
                    .aceitar_pela_estrela(Via::nova(estrela.clone()));
            }
            tracing::info!(%mod_id, tunel = %nome, porta, ?protocolo, "túnel: esta máquina serve");
            let _ = resposta.send(Ok(()));
            return;
        }
        let esperas = self.entrando.remove(&chave).unwrap_or_default();
        if esperas.is_empty() {
            tracing::debug!(%mod_id, tunel = %nome, "túnel: concedido sem pedido em andamento");
            return;
        }
        let desfazer = || {
            enviar(ClientMessage::SairDoTunel {
                mod_id: mod_id.to_owned(),
                nome: nome.to_owned(),
            });
        };
        let Some(estrela) = estrela else {
            for resposta in esperas {
                let _ = resposta.send(Err(RecusaDeTunel::SemResposta));
            }
            return;
        };
        if esperas.iter().all(oneshot::Sender::is_closed) {
            // Quem pediu desistiu (o prazo venceu): a concessão não fica órfã.
            desfazer();
            return;
        }
        let aberta = match abrir_sincrono(protocolo, porta) {
            Ok(aberta) => aberta,
            Err(erro) => {
                tracing::warn!(%mod_id, tunel = %nome, %erro, preferida = porta, "túnel: nenhuma porta local coube");
                let medidas = Medidas::nova(None);
                medidas.quedar(Queda::PortaOcupada);
                let _ = self.avisos.send(Aviso::Tunel(RelatorioDoTunel {
                    mod_id: mod_id.to_owned(),
                    nome: nome.to_owned(),
                    porta_local: 0,
                    preferida: false,
                    desejada: porta,
                    caminho: Caminho::Estrela,
                    latencia: None,
                    bytes_por_segundo_ida: 0,
                    bytes_por_segundo_volta: 0,
                    conexoes: 0,
                    quedas: medidas.quedas(),
                    udp_descartados: 0,
                    encerrado: Some(Queda::PortaOcupada),
                }));
                desfazer();
                for resposta in esperas {
                    let _ = resposta.send(Err(RecusaDeTunel::PortaLocalOcupada));
                }
                return;
            }
        };
        let Some((tunel, descricao)) = TunelDeJogador::iniciar(
            mod_id,
            nome,
            aberta,
            Via::nova(estrela.clone()),
            self.avisos.clone(),
        ) else {
            for resposta in esperas {
                let _ = resposta.send(Err(RecusaDeTunel::CanaisEsgotados));
            }
            return;
        };
        // `pedir_entrada` não deixa chegar aqui com um túnel aberto do mesmo
        // nome; se chegasse (concessão repetida do servidor), o velho sai antes
        // e leva a porta dele.
        self.jogando.insert(chave, tunel);
        tracing::info!(%mod_id, tunel = %nome, porta = descricao.porta, preferida = descricao.preferida, "túnel: porta local aberta");
        for resposta in esperas {
            let _ = resposta.send(Ok(descricao.descrita()));
        }
    }

    fn recusado(&mut self, mod_id: &str, nome: &str, motivo: RecusaDeTunel) {
        let chave = (mod_id.to_owned(), nome.to_owned());
        if let Some(resposta) = self.servindo.remove(&chave) {
            let _ = resposta.send(Err(motivo));
            return;
        }
        if let Some(esperas) = self.entrando.remove(&chave) {
            for resposta in esperas {
                let _ = resposta.send(Err(motivo));
            }
            return;
        }
        // Depois da concessão: o servidor cortou um canal repassado, ou a
        // pessoa perdeu o direito. A conexão cortada acaba sozinha e se conta
        // como `CaminhoCaiu`; aqui só o que acaba o túnel inteiro vira fim.
        tracing::warn!(%mod_id, tunel = %nome, ?motivo, "túnel: o servidor recusou depois de conceder");
        if let Some(queda) = fim_pela_recusa(motivo) {
            if let Some(tunel) = self.jogando.remove(&chave) {
                tunel.encerrar(queda);
                self.ligacoes.podar(&self.jogando);
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use seele_enlace::teste::{par_em_loopback, ParDeTeste};
    use tokio::net::TcpStream;

    const MOD: &str = "seele/jogo";
    const NOME: &str = "mundo";

    // ---- a porta de quem joga

    #[tokio::test]
    async fn a_porta_de_quem_joga_so_escuta_no_loopback() {
        let porta = abrir_porta_local(ProtocoloDeTunel::Tcp, 0).await.unwrap();
        assert!(porta.endereco().ip().is_loopback());
        // E o IP de uma interface de rede que não é o loopback não alcança a
        // porta: ligada em `0.0.0.0`, esta conexão passaria.
        let de_interface =
            if_addrs::get_if_addrs()
                .unwrap()
                .into_iter()
                .find_map(|i| match i.ip() {
                    std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
                    _ => None,
                });
        if let Some(ip) = de_interface {
            let tentativa = TcpStream::connect((ip, porta.porta)).await;
            assert!(tentativa.is_err(), "a porta de quem joga atendeu em {ip}");
        }
    }

    #[tokio::test]
    async fn a_porta_ocupada_cai_na_proxima_e_diz_que_nao_era_a_preferida() {
        let ocupada = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = ocupada.local_addr().unwrap().port();
        let porta = abrir_porta_local(ProtocoloDeTunel::Tcp, p).await.unwrap();
        assert_ne!(porta.porta, p);
        assert!(!porta.preferida);
        // A casca diz «a <desejada> estava ocupada»: a porta pedida volta.
        assert_eq!(porta.desejada, p);
        assert!(porta.porta > p && porta.porta <= p.saturating_add(64));
    }

    /// **M1.** No macOS, ligar `127.0.0.1:P` funciona com `0.0.0.0:P` escutando;
    /// o jogo local ficaria sem voz e o túnel responderia no lugar dele. Quem
    /// já escuta no curinga faz a porta contar como ocupada.
    #[tokio::test]
    async fn quem_escuta_no_curinga_ocupa_a_porta_mesmo_em_loopback() {
        let tcp = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let p_tcp = tcp.local_addr().unwrap().port();
        let porta = abrir_porta_local(ProtocoloDeTunel::Tcp, p_tcp)
            .await
            .unwrap();
        assert_ne!(
            porta.porta, p_tcp,
            "ficou com a porta de um escutador em 0.0.0.0"
        );
        assert!(!porta.preferida);
        assert_eq!(porta.desejada, p_tcp);

        let udp = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let p_udp = udp.local_addr().unwrap().port();
        let porta = abrir_porta_local(ProtocoloDeTunel::Udp, p_udp)
            .await
            .unwrap();
        assert_ne!(
            porta.porta, p_udp,
            "ficou com a porta de um socket UDP em 0.0.0.0"
        );
        assert!(!porta.preferida);
    }

    /// A outra metade do guarda do curinga: ele não pode ver ocupada uma porta
    /// livre. Uma conexão que **este** lado fechou primeiro fica em `TIME_WAIT`
    /// na porta do túnel, e abrir o mesmo túnel de novo tem de devolver a mesma
    /// porta, a preferida.
    #[tokio::test]
    async fn a_porta_com_time_wait_continua_livre() {
        let porta = abrir_porta_local(ProtocoloDeTunel::Tcp, 0).await.unwrap();
        let p = porta.porta;
        let Some((_, Escuta::Tcp(escuta))) = porta.separar() else {
            panic!("uma porta TCP tem escuta TCP")
        };
        let cliente = TcpStream::connect((Ipv4Addr::LOCALHOST, p)).await.unwrap();
        let (aceita, _) = escuta.accept().await.unwrap();
        // O lado da porta fecha primeiro: o `TIME_WAIT` fica nele.
        drop(aceita);
        let mut cliente = cliente;
        let mut resto = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut cliente, &mut resto).await;
        drop(cliente);
        drop(escuta);
        let de_novo = abrir_porta_local(ProtocoloDeTunel::Tcp, p).await.unwrap();
        assert_eq!(
            de_novo.porta, p,
            "o TIME_WAIT fez a porta livre parecer ocupada"
        );
        assert!(de_novo.preferida);
    }

    #[tokio::test]
    async fn a_porta_livre_diz_que_era_a_preferida() {
        let livre = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let porta = abrir_porta_local(ProtocoloDeTunel::Tcp, livre)
            .await
            .unwrap();
        assert_eq!(porta.porta, livre);
        assert!(porta.preferida);
    }

    #[test]
    fn o_relatorio_conta_cada_queda_pelo_motivo() {
        let medidas = Medidas::nova(None);
        medidas.quedar(Queda::CaminhoCaiu);
        medidas.quedar(Queda::JogoRecusou);
        medidas.quedar(Queda::CaminhoCaiu);
        assert_eq!(
            medidas.quedas(),
            vec![(Queda::CaminhoCaiu, 2), (Queda::JogoRecusou, 1)]
        );
    }

    #[test]
    fn um_servidor_onze_nao_recebe_verbo_de_tunel() {
        let entrar = ClientMessage::EntrarNoTunel {
            mod_id: MOD.into(),
            nome: NOME.into(),
        };
        let servir = ClientMessage::ServirTunel {
            mod_id: MOD.into(),
            nome: NOME.into(),
            protocolo: ProtocoloDeTunel::Tcp,
            porta: 25565,
        };
        for verbo in [&entrar, &servir] {
            assert!(
                !verbo_cabe(11, verbo),
                "um servidor 11 leria uma variante que não conhece"
            );
            assert!(verbo_cabe(12, verbo));
        }
        // E o que sempre existiu continua cabendo.
        assert!(verbo_cabe(11, &ClientMessage::Ping { timestamp: 1 }));
    }

    // ---- ajudantes

    async fn eco_tcp() -> (u16, tokio::task::JoinHandle<()>) {
        let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let porta = escuta.local_addr().unwrap().port();
        let tarefa = tokio::spawn(async move {
            while let Ok((mut tcp, _)) = escuta.accept().await {
                tokio::spawn(async move {
                    let (mut le, mut escreve) = tcp.split();
                    let _ = tokio::io::copy(&mut le, &mut escreve).await;
                });
            }
        });
        (porta, tarefa)
    }

    async fn eco_udp() -> (u16, tokio::task::JoinHandle<()>) {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let porta = socket.local_addr().unwrap().port();
        let tarefa = tokio::spawn(async move {
            let mut buf = vec![0_u8; 65_536];
            while let Ok((n, de)) = socket.recv_from(&mut buf).await {
                let _ = socket.send_to(&buf[..n], de).await;
            }
        });
        (porta, tarefa)
    }

    /// Uma porta onde ninguém escuta.
    fn porta_morta() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    struct Cena {
        // As pontas têm de viver enquanto o par for usado.
        _par: ParDeTeste,
        maquina: Maquina,
        tunel: TunelDeJogador,
        porta: PortaLocal,
        avisos: mpsc::UnboundedReceiver<Aviso>,
    }

    /// Quem joga e a máquina, ligados pelo par de teste (a «estrela» sem
    /// servidor no meio) e com o jogo em `porta_do_jogo`.
    async fn cena(protocolo: ProtocoloDeTunel, porta_do_jogo: u16) -> Cena {
        let par = par_em_loopback().await;
        let mut maquina = Maquina::nova();
        maquina.registrar(MOD, NOME, protocolo, porta_do_jogo);
        maquina.aceitar_pela_estrela(Via::nova(par.anfitriao.clone()));
        let (tx, avisos) = mpsc::unbounded_channel();
        let aberta = abrir_porta_local(protocolo, 0).await.unwrap();
        let (tunel, porta) =
            TunelDeJogador::iniciar(MOD, NOME, aberta, Via::nova(par.cliente.clone()), tx).unwrap();
        Cena {
            _par: par,
            maquina,
            tunel,
            porta,
            avisos,
        }
    }

    async fn esperar_relatorio(
        avisos: &mut mpsc::UnboundedReceiver<Aviso>,
        serve: impl Fn(&RelatorioDoTunel) -> bool,
    ) -> RelatorioDoTunel {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match avisos.recv().await {
                    Some(Aviso::Tunel(r)) if serve(&r) => return r,
                    Some(_) => {}
                    None => panic!("os avisos acabaram sem o relatório esperado"),
                }
            }
        })
        .await
        .expect("o relatório esperado não veio em 15 s")
    }

    async fn esperar_ate(mut pronto: impl FnMut() -> bool) {
        for _ in 0..300 {
            if pronto() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("a condição não veio em 15 s");
    }

    // ---- quem joga, pela estrela, contra a máquina

    #[tokio::test]
    async fn o_tcp_do_jogo_vai_e_volta_pela_estrela_e_o_relatorio_mede() {
        let (jogo, _eco) = eco_tcp().await;
        let mut cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let mut tcp = TcpStream::connect(cena.porta.endereco()).await.unwrap();
        let enviado: Vec<u8> = (0..300_000_u32).map(|i| (i % 251) as u8).collect();
        let (mut le, mut escreve) = tcp.split();
        let escrita = async {
            escreve.write_all(&enviado).await.unwrap();
        };
        let leitura = async {
            let mut volta = vec![0_u8; enviado.len()];
            le.read_exact(&mut volta).await.unwrap();
            volta
        };
        let (_, volta) = tokio::join!(escrita, leitura);
        assert_eq!(volta, enviado, "o eco voltou diferente");

        let r = esperar_relatorio(&mut cena.avisos, |r| {
            r.bytes_por_segundo_ida > 0 && r.latencia.is_some()
        })
        .await;
        assert_eq!(r.caminho, Caminho::Estrela);
        assert_eq!((r.mod_id.as_str(), r.nome.as_str()), (MOD, NOME));
        assert_eq!(r.porta_local, cena.porta.porta);
        assert!(r.conexoes >= 1, "{r:?}");
        assert!(r.latencia.unwrap() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn o_jogo_que_nao_escuta_vira_jogo_recusou_na_maquina() {
        let morta = porta_morta();
        let cena = cena(ProtocoloDeTunel::Tcp, morta).await;
        let estado = cena.maquina.estado();
        let mut tcp = TcpStream::connect(cena.porta.endereco()).await.unwrap();
        let _ = tcp.write_all(b"oi").await;
        esperar_ate(|| {
            estado
                .quedas(MOD, NOME)
                .iter()
                .any(|(q, n)| *q == Queda::JogoRecusou && *n >= 1)
        })
        .await;
        // Quem joga vê a conexão cair: não há jogo do outro lado.
        let mut buf = [0_u8; 8];
        let lido = tokio::time::timeout(Duration::from_secs(10), tcp.read(&mut buf))
            .await
            .unwrap();
        assert!(matches!(lido, Ok(0) | Err(_)));
    }

    #[tokio::test]
    async fn a_maquina_que_para_derruba_a_conexao_com_o_motivo() {
        let (jogo, _eco) = eco_tcp().await;
        let mut cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let estado = cena.maquina.estado();
        let mut tcp = TcpStream::connect(cena.porta.endereco()).await.unwrap();
        tcp.write_all(b"x").await.unwrap();
        let mut um = [0_u8; 1];
        tcp.read_exact(&mut um).await.unwrap();
        assert!(cena.maquina.parar(MOD, NOME, Queda::MaquinaParou));
        esperar_ate(|| estado.quedas(MOD, NOME).contains(&(Queda::MaquinaParou, 1))).await;
        // E a conexão de quem joga acaba, contada.
        let r = esperar_relatorio(&mut cena.avisos, |r| {
            r.quedas
                .iter()
                .any(|(q, _)| *q == Queda::CaminhoCaiu || *q == Queda::Fechado)
        })
        .await;
        assert!(!r.quedas.is_empty());
    }

    #[tokio::test]
    async fn o_udp_do_jogo_vai_e_volta_e_o_descartado_entra_no_relatorio() {
        let (jogo, _eco) = eco_udp().await;
        let mut cena = cena(ProtocoloDeTunel::Udp, jogo).await;
        let cliente = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        cliente.connect(cena.porta.endereco()).await.unwrap();
        cliente.send(b"ola jogo").await.unwrap();
        let mut buf = vec![0_u8; 65_536];
        let n = tokio::time::timeout(Duration::from_secs(10), cliente.recv(&mut buf))
            .await
            .expect("o eco UDP não voltou")
            .unwrap();
        assert_eq!(&buf[..n], b"ola jogo");

        // Um datagrama acima de MAX_DADOS que o enlace recusa nesta conexão
        // (a mesma do túnel) entra no relatório: não some calado. O socket do
        // sistema não leva 16 KiB em loopback no Mac, então o excesso é
        // provocado direto no canal `j`.
        let CanalDeTunel::SemOrdem(canal) = cena
            ._par
            .cliente
            .abrir_tunel(Confiabilidade::SemOrdem)
            .await
            .unwrap()
        else {
            panic!("pediu j");
        };
        assert_eq!(
            canal.mandar(&vec![7_u8; seele_enlace::MAX_DADOS + 1]),
            Err(Recusa::Grande)
        );
        let r = esperar_relatorio(&mut cena.avisos, |r| r.udp_descartados >= 1).await;
        assert!(r.udp_descartados >= 1, "{r:?}");
        assert!(r.conexoes >= 1);
        assert_eq!(r.caminho, Caminho::Estrela);
    }

    #[tokio::test]
    async fn o_fim_do_tunel_fecha_a_porta_e_o_ultimo_relatorio_diz_por_que() {
        let (jogo, _eco) = eco_tcp().await;
        let mut cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let alvo = cena.porta.endereco();
        // Aberta, a porta atende.
        drop(TcpStream::connect(alvo).await.unwrap());
        cena.tunel.encerrar(Queda::ModDesligado);
        let r = esperar_relatorio(&mut cena.avisos, |r| r.encerrado.is_some()).await;
        assert_eq!(r.encerrado, Some(Queda::ModDesligado));
        assert!(r.quedas.iter().any(|(q, _)| *q == Queda::ModDesligado));
        assert_eq!(r.conexoes, 0);
        esperar_ate(|| std::net::TcpStream::connect(alvo).is_err()).await;
    }

    #[tokio::test]
    async fn sem_enlace_a_porta_fecha() {
        let (jogo, _eco) = eco_tcp().await;
        let cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let alvo = cena.porta.endereco();
        drop(TcpStream::connect(alvo).await.unwrap());
        // O túnel largado (o `Enlace` que acaba) derruba a porta.
        let Cena {
            tunel,
            _par,
            maquina,
            avisos,
            ..
        } = cena;
        drop(tunel);
        esperar_ate(|| std::net::TcpStream::connect(alvo).is_err()).await;
        drop((_par, maquina, avisos));
    }

    // ---- a máquina

    #[tokio::test]
    async fn a_abertura_de_outro_tunel_e_cortada_e_contada() {
        let (jogo, _eco) = eco_tcp().await;
        let par = par_em_loopback().await;
        let mut maquina = Maquina::nova();
        maquina.registrar(MOD, NOME, ProtocoloDeTunel::Tcp, jogo);
        maquina.registrar(MOD, "outro", ProtocoloDeTunel::Tcp, jogo);
        let estado = maquina.estado();
        // Uma ligação direta só abre os túneis apresentados à pessoa: a abertura que nomeia outro é cortada.
        let restrito = Some(Permitidos::de((MOD.to_owned(), NOME.to_owned())));
        let aceitando = tokio::spawn(aceitar_canais(
            Via::nova(par.anfitriao.clone()),
            restrito,
            Arc::clone(&estado),
            "direto",
        ));
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = par
            .cliente
            .abrir_tunel(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(
            &mut envio,
            &abertura(MOD, "outro", 1, TipoDeCanalDeTunel::Tcp),
        )
        .await
        .unwrap();
        esperar_ate(|| estado.canais_alheios() == 1).await;
        // O canal é cortado: a leitura de quem abriu acaba em erro ou fim.
        let mut buf = [0_u8; 4];
        let lido = tokio::time::timeout(Duration::from_secs(10), recebe.read(&mut buf))
            .await
            .unwrap();
        assert!(matches!(lido, Ok(None) | Err(_)), "{lido:?}");
        // E a do túnel certo passa.
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = par
            .cliente
            .abrir_tunel(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(&mut envio, &abertura(MOD, NOME, 2, TipoDeCanalDeTunel::Tcp))
            .await
            .unwrap();
        envio.write_all(b"ping").await.unwrap();
        let mut volta = [0_u8; 4];
        recebe.read_exact(&mut volta).await.unwrap();
        assert_eq!(&volta, b"ping");
        assert_eq!(estado.canais_alheios(), 1);
        aceitando.abort();
        drop(maquina);
    }

    /// A ponta de uma sessão em `127.0.0.1`, como o produto a arma.
    fn ponta_de_sessao() -> PontaDaSessao {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::transporte::abrir_ponta(Some(socket)).unwrap()
    }

    /// **O defeito que a prova `tunel_de_jogo.rs` achou.** Dois túneis da mesma
    /// máquina são apresentados duas vezes às mesmas duas pontas. Cada
    /// discagem e cada atendimento por conta própria disputavam as mesmas
    /// conexões e se casavam trocados: a máquina atendia a ligação do túnel A
    /// achando que era do B, e a outra discagem esgotava o prazo — o segundo
    /// túnel nunca ia direto. Aqui as duas apresentações dividem UMA ligação, e
    /// os dois túneis passam por ela.
    #[tokio::test(flavor = "multi_thread")]
    async fn dois_tuneis_da_mesma_maquina_dividem_uma_ligacao_direta() {
        let (jogo_a, _eco_a) = eco_tcp().await;
        let (jogo_b, _eco_b) = eco_tcp().await;
        let serve = ponta_de_sessao();
        let busca = ponta_de_sessao();
        let impressao_serve = crate::par::impressao(serve.identidade());
        let impressao_busca = crate::par::impressao(busca.identidade());
        let onde_serve = serve.ponta().local_addr();
        let onde_busca = busca.ponta().local_addr();
        let prazo = Duration::from_secs(5);
        let (maquina_id, joga_id) = (PersonId(1), PersonId(2));

        let mut maquina = Maquina::nova();
        maquina.registrar(MOD, "a", ProtocoloDeTunel::Tcp, jogo_a);
        maquina.registrar(MOD, "b", ProtocoloDeTunel::Tcp, jogo_b);
        // O servidor apresenta os dois túneis, um depois do outro.
        for nome in ["a", "b"] {
            maquina.servir_a(Apresentacao {
                ponta: serve.clone(),
                quem: joga_id,
                mod_id: MOD.to_owned(),
                nome: nome.to_owned(),
                enderecos: vec![onde_busca],
                impressao: impressao_busca.clone(),
                prazo,
            });
        }
        let mut ligacoes = Ligacoes::default();
        let alcance = |nome: &str| Alcance {
            mod_id: MOD.to_owned(),
            nome: nome.to_owned(),
            ponta: busca.clone(),
            enderecos: vec![onde_serve],
            impressao: impressao_serve.clone(),
            prazo,
            quem: maquina_id,
        };
        let mut a = ligacoes.obter(alcance("a"));
        let mut b = ligacoes.obter(alcance("b"));
        let pronta = |rx: &mut watch::Receiver<EstadoDaLigacao>| {
            let mut rx = rx.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(20), async {
                    loop {
                        match &*rx.borrow_and_update() {
                            EstadoDaLigacao::Pronta(ligado) => return Arc::clone(ligado),
                            EstadoDaLigacao::Discando => {}
                            EstadoDaLigacao::Falhou(_, erro) => panic!("a discagem falhou: {erro}"),
                            EstadoDaLigacao::Caiu => panic!("a ligação caiu"),
                        }
                        rx.changed().await.unwrap();
                    }
                })
                .await
                .expect("a ligação direta não veio em 20 s")
            }
        };
        let (ligada_a, ligada_b) = (pronta(&mut a).await, pronta(&mut b).await);
        assert!(
            Arc::ptr_eq(&ligada_a, &ligada_b),
            "dois túneis da mesma máquina discaram duas ligações"
        );

        // Os dois túneis passam pela única ligação, cada um no seu jogo.
        let via = Via::nova(ligada_a.conexao.clone());
        for (nome, fluxo) in [("a", 1), ("b", 2)] {
            let contexto = Contexto {
                mod_id: MOD.to_owned(),
                nome: nome.to_owned(),
                porta_local: 0,
                preferida: false,
                desejada: 0,
                medidas: Arc::new(Medidas::nova(None)),
                avisos: mpsc::unbounded_channel().0,
            };
            let (mut envio, mut recebe) = abrir_t(&via, &contexto, fluxo, TipoDeCanalDeTunel::Tcp)
                .await
                .unwrap();
            envio.write_all(b"ola").await.unwrap();
            let mut volta = [0_u8; 3];
            tokio::time::timeout(Duration::from_secs(10), recebe.read_exact(&mut volta))
                .await
                .unwrap_or_else(|_| panic!("o túnel {nome} não respondeu pela ligação direta"))
                .unwrap();
            assert_eq!(&volta, b"ola");
        }
        assert_eq!(maquina.estado().canais_alheios(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_ligacao_espera_a_apresentacao_atrasada_mas_nao_para_sempre() {
        let permitidos = Permitidos::de((MOD.to_owned(), "a".to_owned()));
        // Um túnel que o servidor ainda não apresentou a esta máquina, mas
        // apresenta dentro da graça: o canal que chegou antes não é cortado.
        let tardio = Arc::clone(&permitidos);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            tardio.permitir((MOD.to_owned(), "b".to_owned()));
        });
        assert!(
            permitidos
                .permite_logo(&(MOD.to_owned(), "b".to_owned()))
                .await
        );
        // Um que ninguém apresentou é recusado quando a graça acaba.
        assert!(
            !permitidos
                .permite_logo(&(MOD.to_owned(), "c".to_owned()))
                .await
        );
    }

    #[tokio::test]
    async fn o_controle_da_maquina_responde_pong_ao_ping() {
        let par = par_em_loopback().await;
        let mut maquina = Maquina::nova();
        maquina.registrar(MOD, NOME, ProtocoloDeTunel::Udp, porta_morta());
        maquina.aceitar_pela_estrela(Via::nova(par.anfitriao.clone()));
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = par
            .cliente
            .abrir_tunel(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(
            &mut envio,
            &abertura(MOD, NOME, 0, TipoDeCanalDeTunel::Controle),
        )
        .await
        .unwrap();
        crate::frame::write(&mut envio, &SinalDeTunel::Ping { n: 41 })
            .await
            .unwrap();
        let sinal = tokio::time::timeout(
            Duration::from_secs(10),
            crate::frame::read::<SinalDeTunel>(&mut recebe),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(sinal, SinalDeTunel::Pong { n: 41 });
    }

    #[tokio::test]
    async fn quem_nao_serve_o_tunel_tem_o_canal_cortado() {
        let par = par_em_loopback().await;
        let mut maquina = Maquina::nova();
        maquina.aceitar_pela_estrela(Via::nova(par.anfitriao.clone()));
        let estado = maquina.estado();
        let CanalDeTunel::Ordenado(mut envio, _recebe) = par
            .cliente
            .abrir_tunel(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(&mut envio, &abertura(MOD, NOME, 1, TipoDeCanalDeTunel::Tcp))
            .await
            .unwrap();
        esperar_ate(|| estado.canais_alheios() == 1).await;
    }

    // ---- o que o enlace faz com as mensagens do servidor

    fn sem_enviar() -> Enviar {
        Arc::new(|_| {})
    }

    #[tokio::test]
    async fn o_concedido_abre_a_porta_o_encerrado_a_fecha_e_a_recusa_tardia_so_acaba_o_que_acaba() {
        let par = par_em_loopback().await;
        let (avisos, mut recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let chave = (MOD.to_owned(), NOME.to_owned());
        let enviar = sem_enviar();
        let prazo = Duration::from_secs(3);

        let (resposta, caixa) = oneshot::channel();
        tuneis.pedir_entrada(chave.clone(), resposta);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 0,
            },
            Some(&par.cliente),
            None,
            &enviar,
            prazo,
        );
        let porta = caixa.await.unwrap().unwrap();
        assert!(porta.endereco().ip().is_loopback());
        let alvo = porta.endereco();
        // A descrição não segura a escuta: a porta é do túnel.
        drop(porta);
        drop(TcpStream::connect(alvo).await.unwrap());

        // Um canal cortado por falta de vaga não acaba o túnel.
        tuneis.tratar(
            &ServerMessage::TunelRecusado {
                mod_id: MOD.into(),
                nome: NOME.into(),
                motivo: RecusaDeTunel::CanaisEsgotados,
            },
            Some(&par.cliente),
            None,
            &enviar,
            prazo,
        );
        drop(TcpStream::connect(alvo).await.unwrap());

        // Perder a permissão acaba.
        tuneis.tratar(
            &ServerMessage::TunelRecusado {
                mod_id: MOD.into(),
                nome: NOME.into(),
                motivo: RecusaDeTunel::SemPermissao,
            },
            Some(&par.cliente),
            None,
            &enviar,
            prazo,
        );
        let r = esperar_relatorio(&mut recebidos, |r| r.encerrado.is_some()).await;
        assert_eq!(r.encerrado, Some(Queda::SemPermissao));
        esperar_ate(|| std::net::TcpStream::connect(alvo).is_err()).await;
    }

    /// **I4.** Um segundo JOGAR com o túnel aberto devolve a porta de antes: não
    /// reabre a escuta, não manda outro `EntrarNoTunel` e não inventa porta
    /// «ocupada». E dois cliques antes da resposta mandam o verbo uma vez só,
    /// e os dois recebem a mesma porta.
    #[tokio::test]
    async fn entrar_de_novo_num_tunel_aberto_devolve_a_mesma_porta_e_nao_manda_o_verbo() {
        let par = par_em_loopback().await;
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let chave = (MOD.to_owned(), NOME.to_owned());
        let enviar = sem_enviar();
        let prazo = Duration::from_secs(3);
        let conceder = |tuneis: &mut Tuneis| {
            tuneis.tratar(
                &ServerMessage::TunelConcedido {
                    mod_id: MOD.into(),
                    nome: NOME.into(),
                    protocolo: ProtocoloDeTunel::Tcp,
                    porta: 0,
                },
                Some(&par.cliente),
                None,
                &enviar,
                prazo,
            );
        };

        // Dois cliques antes da resposta: o verbo sai uma vez.
        let (r1, c1) = oneshot::channel();
        let (r2, c2) = oneshot::channel();
        assert!(
            tuneis.pedir_entrada(chave.clone(), r1),
            "o primeiro pedido manda o verbo"
        );
        assert!(
            !tuneis.pedir_entrada(chave.clone(), r2),
            "o segundo espera o primeiro"
        );
        conceder(&mut tuneis);
        let primeira = c1.await.unwrap().unwrap();
        let segunda = c2.await.unwrap().unwrap();
        assert_eq!(primeira.porta, segunda.porta);
        let alvo = primeira.endereco();

        // Um clique com o túnel já aberto: nada de verbo, nada de porta nova.
        let (r3, c3) = oneshot::channel();
        assert!(
            !tuneis.pedir_entrada(chave.clone(), r3),
            "com o túnel aberto o verbo não sai"
        );
        let terceira = c3.await.unwrap().unwrap();
        assert_eq!(terceira.porta, primeira.porta);
        assert_eq!(terceira.preferida, primeira.preferida);
        assert_eq!(terceira.desejada, primeira.desejada);
        assert_eq!(terceira.endereco(), alvo);
        // A escuta é a mesma, ainda de pé: um túnel substituído a teria fechado
        // e aberto outra.
        drop(TcpStream::connect(alvo).await.unwrap());
        assert_eq!(tuneis.jogando.len(), 1);
    }

    #[tokio::test]
    async fn a_recusa_antes_do_concedido_vai_a_quem_pediu_e_nao_abre_porta() {
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let (resposta, caixa) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), resposta);
        tuneis.tratar(
            &ServerMessage::TunelRecusado {
                mod_id: MOD.into(),
                nome: NOME.into(),
                motivo: RecusaDeTunel::SemPermissao,
            },
            None,
            None,
            &sem_enviar(),
            Duration::from_secs(1),
        );
        assert_eq!(
            caixa.await.unwrap().unwrap_err(),
            RecusaDeTunel::SemPermissao
        );
        assert!(tuneis.jogando.is_empty());
    }

    #[tokio::test]
    async fn quem_desistiu_do_pedido_nao_deixa_concessao_orfa() {
        let par = par_em_loopback().await;
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let (resposta, caixa) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), resposta);
        drop(caixa); // o prazo de quem pediu venceu
        let mandados = Arc::new(Mutex::new(Vec::new()));
        let enviar: Enviar = {
            let mandados = Arc::clone(&mandados);
            Arc::new(move |m| mandados.lock().unwrap().push(m))
        };
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 0,
            },
            Some(&par.cliente),
            None,
            &enviar,
            Duration::from_secs(1),
        );
        assert!(tuneis.jogando.is_empty());
        assert!(matches!(
            mandados.lock().unwrap().as_slice(),
            [ClientMessage::SairDoTunel { .. }]
        ));
    }

    #[tokio::test]
    async fn sem_porta_local_livre_a_recusa_e_local_e_o_relatorio_diz_porta_ocupada() {
        let par = par_em_loopback().await;
        // 65 portas seguidas tomadas: a preferida e as 64 de folga.
        let mut seguradas = Vec::new();
        let base = 'achar: loop {
            seguradas.clear();
            let primeira = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = primeira.local_addr().unwrap().port();
            if p > 65_000 {
                continue;
            }
            seguradas.push(primeira);
            for outra in p + 1..=p + 64 {
                match std::net::TcpListener::bind(("127.0.0.1", outra)) {
                    Ok(l) => seguradas.push(l),
                    Err(_) => continue 'achar,
                }
            }
            break p;
        };
        let (avisos, mut recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let (resposta, caixa) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), resposta);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: base,
            },
            Some(&par.cliente),
            None,
            &sem_enviar(),
            Duration::from_secs(1),
        );
        assert_eq!(
            caixa.await.unwrap().unwrap_err(),
            RecusaDeTunel::PortaLocalOcupada
        );
        let r = esperar_relatorio(&mut recebidos, |r| r.encerrado.is_some()).await;
        assert_eq!(r.encerrado, Some(Queda::PortaOcupada));
        drop(seguradas);
    }

    #[tokio::test]
    async fn concedido_sem_conexao_viva_e_sem_resposta() {
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let (resposta, caixa) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), resposta);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 0,
            },
            None,
            None,
            &sem_enviar(),
            Duration::from_secs(1),
        );
        assert_eq!(
            caixa.await.unwrap().unwrap_err(),
            RecusaDeTunel::SemResposta
        );
    }

    #[tokio::test]
    async fn o_enlace_que_acaba_entrega_o_relatorio_final_antes_de_voltar() {
        let (jogo, _eco) = eco_tcp().await;
        let cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let (avisos, mut recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let (entrar, entrada) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), entrar);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 0,
            },
            Some(&cena._par.cliente),
            None,
            &sem_enviar(),
            Duration::from_secs(1),
        );
        entrada.await.unwrap().unwrap();
        tuneis
            .derrubar_e_esperar(Queda::ServidorSaiu, Duration::from_secs(5))
            .await;
        // Já está na fila quando a espera volta: o `Encerrado` vem depois.
        let mut final_ = None;
        while let Ok(Aviso::Tunel(r)) = recebidos.try_recv() {
            if r.encerrado.is_some() {
                final_ = Some(r);
            }
        }
        assert_eq!(final_.unwrap().encerrado, Some(Queda::ServidorSaiu));
    }

    #[tokio::test]
    async fn o_fim_conta_so_as_conexoes_que_ainda_estavam_vivas() {
        let (jogo, _eco) = eco_tcp().await;
        let mut cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let alvo = cena.porta.endereco();
        // Uma conexão que o jogo fecha antes do fim...
        {
            let mut curta = TcpStream::connect(alvo).await.unwrap();
            curta.write_all(b"a").await.unwrap();
            let mut um = [0_u8; 1];
            curta.read_exact(&mut um).await.unwrap();
        }
        esperar_relatorio(&mut cena.avisos, |r| {
            r.conexoes == 0 && r.quedas.contains(&(Queda::Fechado, 1))
        })
        .await;
        // ...e uma que segue viva quando o túnel acaba.
        let mut viva = TcpStream::connect(alvo).await.unwrap();
        viva.write_all(b"b").await.unwrap();
        let mut um = [0_u8; 1];
        viva.read_exact(&mut um).await.unwrap();
        cena.tunel.encerrar(Queda::ModDesligado);
        let r = esperar_relatorio(&mut cena.avisos, |r| r.encerrado.is_some()).await;
        // Fechado: 1 (a que acabou antes). ModDesligado: 1 da viva + 1 do túnel.
        assert_eq!(
            r.quedas,
            vec![(Queda::ModDesligado, 2), (Queda::Fechado, 1)]
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect::<Vec<_>>()
        );
        drop(viva);
    }

    #[tokio::test]
    async fn derrubar_tudo_fecha_as_portas_e_a_maquina() {
        let par = par_em_loopback().await;
        let (jogo, _eco) = eco_tcp().await;
        let (avisos, mut recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let enviar = sem_enviar();
        // A máquina serve; quem joga entra no mesmo nome (pelo mesmo par).
        let (servir, servida) = oneshot::channel();
        tuneis.pedir_servico((MOD.to_owned(), NOME.to_owned()), servir);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: jogo,
            },
            Some(&par.anfitriao),
            None,
            &enviar,
            Duration::from_secs(1),
        );
        servida.await.unwrap().unwrap();
        assert!(tuneis.maquina.serve(MOD, NOME));
        let (entrar, entrada) = oneshot::channel();
        tuneis.pedir_entrada((MOD.to_owned(), NOME.to_owned()), entrar);
        tuneis.tratar(
            &ServerMessage::TunelConcedido {
                mod_id: MOD.into(),
                nome: NOME.into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: jogo,
            },
            Some(&par.cliente),
            None,
            &enviar,
            Duration::from_secs(1),
        );
        let alvo = entrada.await.unwrap().unwrap().endereco();
        let mut tcp = TcpStream::connect(alvo).await.unwrap();
        tcp.write_all(b"oi").await.unwrap();
        let mut dois = [0_u8; 2];
        tcp.read_exact(&mut dois).await.unwrap();
        assert_eq!(&dois, b"oi");

        tuneis.derrubar_tudo(Queda::ServidorSaiu);
        assert!(!tuneis.maquina.serve(MOD, NOME));
        let r = esperar_relatorio(&mut recebidos, |r| r.encerrado.is_some()).await;
        assert_eq!(r.encerrado, Some(Queda::ServidorSaiu));
        esperar_ate(|| std::net::TcpStream::connect(alvo).is_err()).await;
    }

    #[test]
    fn os_motivos_do_servidor_viram_quedas() {
        assert_eq!(
            queda_do_fim(MotivoDoFimDoTunel::SemPermissao),
            Queda::SemPermissao
        );
        assert_eq!(
            queda_do_fim(MotivoDoFimDoTunel::ModDesligado),
            Queda::ModDesligado
        );
        assert_eq!(
            queda_do_fim(MotivoDoFimDoTunel::AnfitriaoCaiu),
            Queda::ServidorSaiu
        );
        // Um canal cortado por falta de vaga não acaba o túnel.
        assert_eq!(fim_pela_recusa(RecusaDeTunel::CanaisEsgotados), None);
        assert_eq!(
            fim_pela_recusa(RecusaDeTunel::MaquinaSaiu),
            Some(Queda::MaquinaSaiu)
        );
    }
}
