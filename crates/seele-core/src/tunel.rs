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
use seele_proto::tunel::{
    datagrama_de_abertura, ler_datagrama_j, AberturaDeTunel, DatagramaJ, MotivoDoFimDoTunel,
    SinalDeTunel, TipoDeCanalDeTunel, REPETE_A_ABERTURA_A_CADA, TAG_DE_DADO,
};
pub use seele_proto::tunel::{ProtocoloDeTunel, RecusaDeTunel};
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

/// O rótulo de [`Caminho::Direto`] nos logs e nas contas por caminho.
const ROTULO_DIRETO: &str = "direto";

impl Caminho {
    fn rotulo(self) -> &'static str {
        match self {
            Self::Direto { .. } => ROTULO_DIRETO,
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
    /// O servidor (ou a máquina) não tinha mais canal livre para uma conexão
    /// desta pessoa e a cortou **depois** de conceder o túnel. O túnel segue de
    /// pé; a conexão cortada conta também como `CaminhoCaiu`, e esta é a razão.
    SemCanais,
    /// Este computador não conseguiu aceitar a conexão do jogo na porta local
    /// (sem descritor de arquivo livre, por exemplo). Conta **uma vez por
    /// rajada** de erros seguidos, e o laço espera um pouco mais a cada erro em
    /// vez de girar.
    PortaFalhou,
    /// **Quem hospeda o SEELE saiu, e o túnel passava por ele** (a estrela):
    /// não há quem repasse. Um túnel que andava pela ligação direta segue sem
    /// quem hospeda, e só acaba assim se a ligação direta cair enquanto ele
    /// está fora — não há estrela para onde voltar. Diferente de
    /// [`Self::ServidorSaiu`], que é a sessão desta pessoa que acabou.
    AnfitriaoSaiu,
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
    /// Conexões TCP e fluxos UDP vivos agora (a soma das duas abaixo).
    pub conexoes: u32,
    /// As vivas que andam pela **ligação direta**.
    pub conexoes_diretas: u32,
    /// As vivas que andam pelo **anfitrião**. `caminho` só diz por onde as
    /// conexões *novas* vão; uma conexão TCP, ou um fluxo UDP, que começou
    /// antes do direto segue na estrela até acabar (não migra), e é este
    /// número que diz quantas são.
    pub conexoes_pela_estrela: u32,
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
    /// Das `conexoes`, as que andam pela ligação direta. As da estrela são a
    /// diferença.
    conexoes_diretas: AtomicU32,
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
            conexoes_diretas: AtomicU32::new(0),
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

/// Uma conexão ou fluxo vivo: conta enquanto existir, **no caminho em que
/// nasceu** (ela não migra).
struct ConexaoViva<'a> {
    medidas: &'a Medidas,
    direta: bool,
}

impl<'a> ConexaoViva<'a> {
    /// `caminho` é o rótulo de [`Caminho::rotulo`]: `"direto"` ou `"estrela"`.
    fn nova(medidas: &'a Medidas, caminho: &str) -> Self {
        let direta = caminho == ROTULO_DIRETO;
        medidas.conexoes.fetch_add(1, Ordering::Relaxed);
        if direta {
            medidas.conexoes_diretas.fetch_add(1, Ordering::Relaxed);
        }
        medidas.mudou();
        Self { medidas, direta }
    }
}

impl Drop for ConexaoViva<'_> {
    fn drop(&mut self) {
        self.medidas.conexoes.fetch_sub(1, Ordering::Relaxed);
        if self.direta {
            self.medidas
                .conexoes_diretas
                .fetch_sub(1, Ordering::Relaxed);
        }
        self.medidas.mudou();
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
    /// Algo aconteceu ao túnel que o laço não vê sozinho (o servidor recusou
    /// um canal por falta de vaga): conta-se a queda e sai um relatório.
    Quedou(Queda),
    /// Um fluxo UDP acabou.
    FluxoAcabou { origem: SocketAddr, fluxo: u32 },
    /// A conexão com quem hospeda (a estrela) acabou.
    EstrelaCaiu,
    /// Quem hospeda voltou e concedeu este túnel de novo: a estrela nova.
    NovaEstrela(Via),
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

/// O datagrama de abertura de um canal `j` (ver
/// [`seele_proto::tunel::datagrama_de_abertura`]).
///
/// O `j` não retransmite nem ordena, então a abertura não é «o primeiro
/// datagrama»: é o que o tag diz que é, e quem abre a repete (ver
/// [`Repeticao`]).
pub(crate) fn abertura_j(abertura: &AberturaDeTunel) -> Result<Vec<u8>, String> {
    datagrama_de_abertura(abertura).map_err(|erro| erro.to_string())
}

/// Quando quem abre um canal `j` repete a abertura.
///
/// A abertura vai uma vez, ao abrir, e o canal não tem como saber se ela
/// chegou: um datagrama perdido ou atrasado deixava o outro lado lendo carga
/// do jogo como se fosse abertura. Enquanto nada voltar pelo canal, a abertura
/// se repete antes do próximo dado quando o prazo vence, e o prazo dobra de
/// 100 ms até [`REPETE_A_ABERTURA_A_CADA`]: rápido o bastante para a primeira
/// perda não custar um segundo de jogo, e limitado para um fluxo só de ida
/// (que nunca ouve resposta) pagar no máximo uma abertura por segundo.
pub(crate) struct Repeticao {
    proxima: Instant,
    intervalo: Duration,
}

impl Repeticao {
    const PRIMEIRO_INTERVALO: Duration = Duration::from_millis(100);

    pub(crate) fn nova(agora: Instant) -> Self {
        Self {
            proxima: agora + Self::PRIMEIRO_INTERVALO,
            intervalo: Self::PRIMEIRO_INTERVALO,
        }
    }

    /// Se a abertura deve ir de novo antes do próximo dado.
    pub(crate) fn vencida(&self, agora: Instant) -> bool {
        agora >= self.proxima
    }

    /// Anota que a abertura foi repetida e marca a próxima.
    pub(crate) fn repetiu(&mut self, agora: Instant) {
        self.intervalo = (self.intervalo * 2).min(REPETE_A_ABERTURA_A_CADA);
        self.proxima = agora + self.intervalo;
    }

    /// O intervalo que vale depois da última repetição.
    #[cfg(test)]
    fn intervalo(&self) -> Duration {
        self.intervalo
    }
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
            conexoes_diretas: self.medidas.conexoes_diretas.load(Ordering::Relaxed),
            conexoes_pela_estrela: self
                .medidas
                .conexoes
                .load(Ordering::Relaxed)
                .saturating_sub(self.medidas.conexoes_diretas.load(Ordering::Relaxed)),
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

    /// O laço dele ainda corre: um túnel que acabou sozinho (a estrela caiu
    /// com ele nela) deixa só a entrada no mapa.
    fn vivo(&self) -> bool {
        !self.eventos.is_closed()
    }

    /// Quem hospeda voltou e concedeu o túnel de novo.
    fn nova_estrela(&self, via: Via) {
        let _ = self.eventos.send(Evento::NovaEstrela(via));
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
    let viva = ConexaoViva::nova(&contexto.medidas, caminho);
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
    let viva = ConexaoViva::nova(&contexto.medidas, caminho);
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
        let abertura_do_fluxo = match abertura_j(&abertura(
            &contexto.mod_id,
            &contexto.nome,
            fluxo,
            TipoDeCanalDeTunel::Udp,
        )) {
            Ok(bytes) => bytes,
            Err(porque) => break 'fluxo (Queda::CaminhoCaiu, porque),
        };
        if datagramas.mandar(&abertura_do_fluxo).is_err() {
            break 'fluxo (Queda::CaminhoCaiu, "a abertura do j não saiu".to_owned());
        }
        // Enquanto nada voltar, a abertura se repete (ver `Repeticao`): é a
        // única prova que este lado tem de que ela chegou.
        let mut repeticao = Repeticao::nova(Instant::now());
        let mut voltou = false;
        let mut ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
        loop {
            tokio::select! {
                dado = entrada.recv() => {
                    let Some(dado) = dado else { break 'fluxo (Queda::Fechado, "o túnel largou o fluxo".to_owned()); };
                    ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
                    let agora = Instant::now();
                    if !voltou && repeticao.vencida(agora) {
                        // Cheia e Grande: nada a fazer, a próxima vez repete; o
                        // fim do canal aparece no `mandar` do dado logo abaixo.
                        let _ = datagramas.mandar(&abertura_do_fluxo);
                        repeticao.repetiu(agora);
                    }
                    // O dado já nasceu com o tag na frente (ver `enviar_ao_fluxo`).
                    let carga = dado.len().saturating_sub(1) as u64;
                    match datagramas.mandar(&dado) {
                        Ok(()) => { contexto.medidas.ida.fetch_add(carga, Ordering::Relaxed); }
                        // Cheia e Grande já foram contadas pelo enlace, e entram
                        // no relatório por `Via::descartes`.
                        Err(Recusa::Cheia | Recusa::Grande) => {}
                        Err(Recusa::Fechada) => break 'fluxo (Queda::CaminhoCaiu, "o canal j fechou".to_owned()),
                    }
                }
                chegou = datagramas.receber() => {
                    let Some(dado) = chegou else { break 'fluxo (Queda::CaminhoCaiu, "o canal j fechou".to_owned()); };
                    ate = Instant::now() + OCIOSIDADE_DO_FLUXO_UDP;
                    match ler_datagrama_j(&dado) {
                        DatagramaJ::Dado(carga) => {
                            voltou = true;
                            contexto.medidas.volta.fetch_add(carga.len() as u64, Ordering::Relaxed);
                            let _ = socket.send_to(carga, origem).await;
                        }
                        // A máquina não abre canal `j` para quem joga: uma
                        // abertura ou um tag que não se conhece não é do jogo.
                        DatagramaJ::Abertura(_) | DatagramaJ::Ilegivel => {
                            tracing::debug!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho, fluxo, "túnel: datagrama j de volta sem tag de dado, descartado");
                        }
                    }
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

/// Os erros seguidos do `accept` da porta local.
///
/// Um erro persistente (sem descritor livre) devolvia na hora, e o laço girava
/// emitindo um aviso por volta. Cada erro seguido espera o dobro do anterior
/// (de 50 ms até 2 s), o primeiro da rajada é contado e dito, e um `accept`
/// que dá certo zera a conta.
#[derive(Default)]
struct ErrosDeAccept {
    seguidos: u32,
    pausa_ate: Option<Instant>,
}

impl ErrosDeAccept {
    const PRIMEIRA_PAUSA: Duration = Duration::from_millis(50);
    const TETO_DA_PAUSA: Duration = Duration::from_secs(2);

    /// Um `accept` falhou. `true` se é o primeiro da rajada (o que se conta e
    /// se diz).
    fn falhou(&mut self, agora: Instant) -> bool {
        self.seguidos = self.seguidos.saturating_add(1);
        let dobros = self.seguidos.saturating_sub(1).min(16);
        let pausa = Self::PRIMEIRA_PAUSA
            .saturating_mul(1_u32 << dobros)
            .min(Self::TETO_DA_PAUSA);
        self.pausa_ate = Some(agora + pausa);
        self.seguidos == 1
    }

    fn deu_certo(&mut self) {
        self.seguidos = 0;
        self.pausa_ate = None;
    }

    /// Espera o fim da pausa, se há uma.
    async fn pausa(&self) {
        if let Some(ate) = self.pausa_ate {
            tokio::time::sleep_until(ate).await;
        }
    }
}

/// O laço de um túnel de quem joga.
/// Vigia a estrela: quando a conexão com quem hospeda acaba, o laço ouve
/// [`Evento::EstrelaCaiu`] e decide (ver «sem quem hospeda» em
/// [`laco_do_jogador`]). A geração separa a estrela velha da nova: uma volta
/// de quem hospeda troca a estrela, e a queda da velha não fala da nova.
fn vigiar_a_estrela(
    filhos: &mut JoinSet<()>,
    estrela: &Via,
    tx: &mpsc::UnboundedSender<Evento>,
    vez: Arc<AtomicU32>,
) {
    let (tx, estrela) = (tx.clone(), estrela.clone());
    let minha = vez.load(Ordering::Acquire);
    filhos.spawn(async move {
        let motivo = estrela.fechou().await;
        tracing::debug!(%motivo, "túnel: a conexão com o servidor acabou");
        if vez.load(Ordering::Acquire) == minha {
            let _ = tx.send(Evento::EstrelaCaiu);
        }
    });
}

async fn laco_do_jogador(
    contexto: Arc<Contexto>,
    escuta: Escuta,
    mut estrela: Via,
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
    // **Sem quem hospeda** (API 8, «A reserva», último item): a estrela caiu e
    // o túnel segue pela ligação direta. Enquanto isto vale, não há para onde
    // voltar se o direto cair.
    let mut sem_estrela = false;
    let vez_da_estrela = Arc::new(AtomicU32::new(0));
    vigiar_a_estrela(&mut filhos, &estrela, &tx, Arc::clone(&vez_da_estrela));
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

    let mut erros_de_accept = ErrosDeAccept::default();
    let fim = loop {
        tokio::select! {
            aceito = async { erros_de_accept.pausa().await; esperar_tcp(&escuta).await } => {
                match aceito {
                    Ok((tcp, origem)) => {
                        erros_de_accept.deu_certo();
                        proximo_fluxo = proximo_fluxo.wrapping_add(1);
                        tracing::debug!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), fluxo = proximo_fluxo, %origem, "túnel: conexão do jogo aceita");
                        filhos.spawn(conexao_do_jogo(tcp, via.clone(), caminho.rotulo(), proximo_fluxo, Arc::clone(&contexto)));
                    }
                    Err(erro) => {
                        // Uma vez por rajada: o erro persistente não enche o log
                        // nem faz o laço girar (a pausa cresce a cada erro).
                        if erros_de_accept.falhou(Instant::now()) {
                            tracing::warn!(%erro, mod_id = %contexto.mod_id, tunel = %contexto.nome, "túnel: o accept da porta local falhou; a espera cresce a cada erro seguido");
                            medidas.quedar(Queda::PortaFalhou);
                        }
                    }
                }
            }
            () = esperar_udp(&escuta) => {
                if let Escuta::Udp(socket) = &escuta {
                    match socket.try_recv_from(&mut buf_udp) {
                        Ok((n, origem)) => {
                            // O tag de dado vai na frente já aqui, para o
                            // fluxo mandar o datagrama sem outra cópia.
                            let mut dado = Vec::with_capacity(n + 1);
                            dado.push(TAG_DE_DADO);
                            dado.extend_from_slice(buf_udp.get(..n).unwrap_or_default());
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
                    Evento::DiretoCaiu(g) if g == geracao && ligado.is_some() && sem_estrela => {
                        // Sem quem hospeda, não há estrela para onde voltar.
                        tracing::warn!(mod_id = %contexto.mod_id, tunel = %contexto.nome, "túnel: a ligação direta caiu sem quem hospeda; não há estrela para onde voltar");
                        break Queda::AnfitriaoSaiu;
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
                    Evento::EstrelaCaiu => {
                        // **Quem hospeda saiu.** Quem passava por ele cai, e
                        // sabe por quê; quem anda pela malha continua — as
                        // conexões vivas e as novas, pelo túnel já concedido.
                        if !matches!(caminho, Caminho::Direto { .. }) {
                            break Queda::AnfitriaoSaiu;
                        }
                        sem_estrela = true;
                        tracing::info!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), "túnel: quem hospeda saiu; a ligação direta segue");
                        mudou = true;
                    }
                    Evento::NovaEstrela(nova) => {
                        // Quem hospeda voltou e concedeu o túnel de novo: a
                        // estrela nova é o caminho de volta se o direto cair.
                        vez_da_estrela.fetch_add(1, Ordering::AcqRel);
                        estrela = nova;
                        vigiar_a_estrela(&mut filhos, &estrela, &tx, Arc::clone(&vez_da_estrela));
                        if sem_estrela {
                            tracing::info!(mod_id = %contexto.mod_id, tunel = %contexto.nome, caminho = caminho.rotulo(), "túnel: quem hospeda voltou; a estrela é de novo o caminho de volta");
                        }
                        sem_estrela = false;
                        if !matches!(caminho, Caminho::Direto { .. }) {
                            via = estrela.clone();
                            base_de_descartes = via.descartes();
                            pingador = AbortaAoLargar::nova(pingar(via.clone(), Arc::clone(&contexto)));
                        }
                        mudou = true;
                    }
                    Evento::Quedou(queda) => medidas.quedar(queda),
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
    /// Datagramas de carga que chegaram a um canal `j` **antes** da abertura
    /// dele: descartados e contados, nunca motivo para cortar o canal (a
    /// abertura pode só ter se atrasado ou se perdido, e quem abre a repete).
    dados_antes_da_abertura: AtomicU64,
    /// Quem a máquina está atendendo (ou esperando) pela malha: **uma ligação
    /// por pessoa**, com os túneis que o servidor apresentou a ela.
    atendendo: Arc<Mutex<HashMap<PersonId, Atendimento>>>,
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

    /// Quantos datagramas de carga chegaram a um canal `j` antes da abertura.
    #[must_use]
    pub(crate) fn dados_antes_da_abertura(&self) -> u64 {
        self.dados_antes_da_abertura.load(Ordering::Relaxed)
    }

    /// Com quantas pessoas a máquina mantém ligação direta agora.
    #[must_use]
    pub(crate) fn ligacoes_diretas(&self) -> usize {
        self.atendendo
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// O lado da máquina: aceita canais e os liga no jogo.
pub(crate) struct Maquina {
    estado: Arc<EstadoDaMaquina>,
    tarefas: JoinSet<()>,
    na_estrela: bool,
    /// O mesmo mapa de [`EstadoDaMaquina::atendendo`].
    atendendo: Arc<Mutex<HashMap<PersonId, Atendimento>>>,
    proximo_atendimento: u64,
    /// Sobe a cada vez que quem hospeda sai: o prazo de uma volta só confere
    /// as ligações se ele não saiu de novo no meio.
    vez_do_anfitriao: Arc<AtomicU64>,
}

/// **Quanto quem hospeda tem, depois de voltar, para apresentar de novo cada
/// túnel que seguiu pela malha sem ele.** Quem joga pede entrada de novo assim
/// que volta ([`Tuneis::reentrar`]); um túnel que ele não apresentou nesse
/// tempo é de quem não pode mais (perdeu a ação, foi banido, ou não voltou), e
/// sai da ligação direta como uma retirada. Sem isto, quem perdeu a permissão
/// enquanto quem hospeda estava fora seguia jogando pela malha até sair.
pub(crate) const PRAZO_DA_REAPRESENTACAO: Duration = Duration::from_secs(30);

/// De quanto em quanto quem joga pede entrada de novo, na volta de quem
/// hospeda, enquanto a máquina ainda não serve ([`Tuneis::reentrar`]).
const INTERVALO_DA_REENTRADA: Duration = Duration::from_secs(1);

/// Os túneis que o servidor apresentou a uma pessoa: o que a ligação direta
/// dela pode abrir. Um canal que nomeia outro túnel é cortado.
///
/// Cada túnel carrega um **sinal de vigia** que morre quando o túnel é
/// retirado (`TunelRetirado`): todo canal que andava por ele o segura e acaba
/// junto. O servidor só confere os canais que passam por ele; pela ligação
/// direta, quem confere é esta lista, e sem o sinal ela só crescia.
pub(crate) struct Permitidos {
    mapa: Mutex<HashMap<Chave, watch::Sender<()>>>,
    /// Os túneis que seguiram sem quem hospeda e que ele ainda não apresentou
    /// de novo depois de voltar ([`Maquina::anfitriao_voltou`]).
    a_confirmar: Mutex<HashSet<Chave>>,
    /// Vira `true` quando a lista esvazia por retirada: a ligação direta para
    /// de aceitar canais e espera os que ainda acabam.
    esvaziada: watch::Sender<bool>,
}

/// Quanto um canal que chega por uma ligação direta espera pela apresentação
/// do túnel que nomeia. O servidor apresenta o túnel à máquina e a quem joga
/// por dois caminhos independentes, e quem joga pode abrir o canal um instante
/// antes de a máquina ouvir a apresentação; sem a espera, esse canal seria
/// cortado por uma corrida, e não por falta de permissão.
const GRACA_DA_APRESENTACAO: Duration = Duration::from_millis(1500);

impl Permitidos {
    pub(crate) fn de(chave: Chave) -> Arc<Self> {
        Arc::new(Self {
            mapa: Mutex::new(HashMap::from([(chave, watch::Sender::new(()))])),
            a_confirmar: Mutex::new(HashSet::new()),
            esvaziada: watch::Sender::new(false),
        })
    }

    /// Todo túnel desta pessoa passa a esperar quem hospeda apresentá-lo de
    /// novo.
    fn marcar_a_confirmar(&self) {
        let chaves: HashSet<Chave> = self
            .mapa
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        *self
            .a_confirmar
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = chaves;
    }

    /// Os que quem hospeda não apresentou de novo, tirados da espera.
    fn nao_confirmados(&self) -> Vec<Chave> {
        self.a_confirmar
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect()
    }

    fn permitir(&self, chave: Chave) {
        self.a_confirmar
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&chave);
        self.mapa
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(chave)
            .or_insert_with(|| watch::Sender::new(()));
    }

    /// O sinal de vigia do túnel, se ele é permitido agora.
    fn vigia(&self, chave: &Chave) -> Option<watch::Receiver<()>> {
        self.mapa
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(chave)
            .map(watch::Sender::subscribe)
    }

    /// Tira o túnel da lista: o sinal de vigia morre e os canais que o seguram
    /// acabam. `true` se estava lá.
    ///
    /// Esvaziada a lista, avisa a ligação (`esvaziada`) para parar de aceitar
    /// canais **sem abortar** os que ainda estão acabando: cada um registra a
    /// própria queda (`SemPermissao`) ao ver o vigia morrer, e abortar a
    /// ligação no meio apagaria essa conta.
    fn retirar(&self, chave: &Chave) -> bool {
        let mut mapa = self.mapa.lock().unwrap_or_else(PoisonError::into_inner);
        let tinha = mapa.remove(chave).is_some();
        if mapa.is_empty() {
            self.esvaziada.send_replace(true);
        }
        tinha
    }

    fn vazio(&self) -> bool {
        self.mapa
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    /// O vigia do túnel se ele é permitido agora ou passa a ser dentro da
    /// graça; `None` se a graça venceu sem a apresentação chegar.
    async fn vigia_logo(&self, chave: &Chave) -> Option<watch::Receiver<()>> {
        let fim = Instant::now() + GRACA_DA_APRESENTACAO;
        loop {
            if let Some(vigia) = self.vigia(chave) {
                return Some(vigia);
            }
            if Instant::now() >= fim {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// Quanto a ligação direta espera, depois de esvaziada, os canais que ainda
/// estão acabando registrarem a queda deles.
const PRAZO_DA_DESPEDIDA: Duration = Duration::from_secs(2);

/// Espera a lista de permitidos da ligação esvaziar. Sem lista (a estrela)
/// nunca acontece.
async fn lista_esvaziada(esvaziada: &mut Option<watch::Receiver<bool>>) {
    match esvaziada {
        Some(rx) => {
            let _ = rx.wait_for(|vazia| *vazia).await;
        }
        None => pending().await,
    }
}

/// Espera o túnel ser retirado de quem abriu o canal. Sem vigia (a estrela, que
/// o servidor confere canal a canal) nunca acontece.
async fn retirada(vigia: &mut Option<watch::Receiver<()>>) {
    match vigia {
        // Nada é enviado por este sinal: `changed` só volta com erro, quando a
        // lista largou o túnel.
        Some(vigia) => while vigia.changed().await.is_ok() {},
        None => pending().await,
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
        let atendendo = Arc::new(Mutex::new(HashMap::new()));
        Self {
            estado: Arc::new(EstadoDaMaquina {
                servidos: Mutex::new(HashMap::new()),
                alheios: AtomicU64::new(0),
                dados_antes_da_abertura: AtomicU64::new(0),
                atendendo: Arc::clone(&atendendo),
            }),
            tarefas: JoinSet::new(),
            na_estrela: false,
            atendendo,
            proximo_atendimento: 0,
            vez_do_anfitriao: Arc::new(AtomicU64::new(0)),
        }
    }

    /// **Quem hospeda saiu**: a máquina segue servindo quem está ligado a ela
    /// pela malha — os túneis servidos ficam, as ligações diretas ficam e
    /// aceitam canais dos túneis que já foram apresentados a cada pessoa. A
    /// estrela acaba sozinha com a conexão dela; a próxima conexão volta a
    /// aceitá-la no primeiro `servir`. **Nada novo é apresentado sem ele**: uma
    /// pessoa nova, ou um túnel novo para quem já está, precisa de
    /// `SirvaTunelPara`.
    pub(crate) fn sem_anfitriao(&mut self) {
        self.vez_do_anfitriao.fetch_add(1, Ordering::AcqRel);
        self.na_estrela = false;
        let atendendo = self
            .atendendo
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for atendimento in atendendo.values() {
            atendimento.permitidos.marcar_a_confirmar();
        }
        tracing::info!(
            diretas = atendendo.len(),
            "túnel: quem hospeda saiu; a máquina segue servindo quem está ligado pela malha"
        );
    }

    /// **Quem hospeda voltou**: daqui a [`PRAZO_DA_REAPRESENTACAO`], todo túnel
    /// que seguiu pela malha e que ele não apresentou de novo sai da ligação
    /// direta ([`Self::conferir_a_volta`]).
    pub(crate) fn anfitriao_voltou(&mut self) {
        let vez = self.vez_do_anfitriao.load(Ordering::Acquire);
        let vez_agora = Arc::clone(&self.vez_do_anfitriao);
        let atendendo = Arc::clone(&self.atendendo);
        self.tarefas.spawn(async move {
            tokio::time::sleep(PRAZO_DA_REAPRESENTACAO).await;
            if vez_agora.load(Ordering::Acquire) == vez {
                conferir_a_volta(&atendendo);
            }
        });
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

    /// `TunelRetirado`: `quem` deixou de poder alcançar `mod_id/nome`.
    ///
    /// O servidor confere cada canal que passa **por ele**; pela ligação
    /// direta, quem confere é a máquina, e ela só aprendia o que a pessoa
    /// *ganhava*. Aqui o túnel sai do que a ligação aceita, os canais dessa
    /// pessoa naquele túnel acabam, e a ligação inteira cai quando nada mais é
    /// permitido a ela.
    pub(crate) fn retirar(&mut self, mod_id: &str, nome: &str, quem: PersonId) {
        let chave = (mod_id.to_owned(), nome.to_owned());
        let mut atendendo = self
            .atendendo
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(atendimento) = atendendo.get(&quem) else {
            tracing::debug!(%mod_id, tunel = %nome, %quem, "túnel: retirada de quem não tem ligação direta com esta máquina");
            return;
        };
        let tinha = atendimento.permitidos.retirar(&chave);
        tracing::info!(%mod_id, tunel = %nome, %quem, tinha, "túnel: a pessoa perdeu o túnel; os canais dela nele acabam");
        if atendimento.permitidos.vazio() {
            // A lista avisou a ligação (`Permitidos::retirar`); aqui só se
            // esquece o atendimento, para a próxima apresentação criar outro.
            atendendo.remove(&quem);
            tracing::info!(%mod_id, tunel = %nome, %quem, "túnel: nada mais é permitido a esta pessoa; a ligação direta cai");
        }
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

/// Tira das ligações diretas os túneis que quem hospeda não apresentou de
/// novo depois de voltar, como uma retirada (`SemPermissao` em cada canal), e
/// esquece quem ficou sem nenhum.
fn conferir_a_volta(atendendo: &Mutex<HashMap<PersonId, Atendimento>>) {
    let mut atendendo = atendendo.lock().unwrap_or_else(PoisonError::into_inner);
    atendendo.retain(|quem, atendimento| {
        for (mod_id, nome) in atendimento.permitidos.nao_confirmados() {
            tracing::warn!(%mod_id, tunel = %nome, %quem, "túnel: quem hospeda voltou e não apresentou de novo este túnel a esta pessoa; os canais dela nele acabam");
            atendimento.permitidos.retirar(&(mod_id, nome));
        }
        !atendimento.permitidos.vazio()
    });
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
    let mut esvaziada = restrito.as_ref().map(|p| p.esvaziada.subscribe());
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
            () = lista_esvaziada(&mut esvaziada) => {
                // Nada mais é permitido a esta pessoa: não se aceita canal
                // novo, e os que viviam acabam sozinhos (o vigia morreu) e
                // contam a própria queda antes de a ligação ir embora.
                let _ = tokio::time::timeout(PRAZO_DA_DESPEDIDA, async {
                    while filhos.join_next().await.is_some() {}
                })
                .await;
                tracing::debug!(caminho, "túnel: a ligação direta ficou sem túnel permitido");
                return;
            }
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
    // Se o túnel nomeado é um dos que esta via pode abrir. Na ligação direta
    // devolve o vigia dele (o canal acaba quando o túnel é retirado de quem
    // abriu); na estrela, onde o servidor já conferiu, é `Some(None)`.
    let da_via = async |aberta: &AberturaDeTunel| match &restrito {
        None => Some(None),
        Some(permitidos) => permitidos
            .vigia_logo(&(aberta.mod_id.clone(), aberta.nome.clone()))
            .await
            .map(Some),
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
            let vigia = da_via(&aberta).await;
            let Some(servido) = conferir(&aberta, vigia.is_some(), &estado, caminho) else {
                let _ = envio.reset(CODIGO_CORTADO);
                let _ = recebe.stop(CODIGO_CORTADO);
                return;
            };
            let mut vigia = vigia.flatten();
            match aberta.tipo {
                TipoDeCanalDeTunel::Controle => {
                    tokio::select! {
                        () = responder_aos_pings(envio, recebe) => {}
                        () = retirada(&mut vigia) => {}
                    }
                }
                _ => tcp_da_maquina(&aberta, &servido, envio, recebe, vigia, caminho).await,
            }
        }
        CanalDeTunel::SemOrdem(mut datagramas) => {
            let Some(aberta) = esperar_a_abertura_j(&mut datagramas, &estado, caminho).await else {
                return;
            };
            let vigia = da_via(&aberta).await;
            let Some(servido) = conferir(&aberta, vigia.is_some(), &estado, caminho) else {
                return;
            };
            if aberta.tipo == TipoDeCanalDeTunel::Udp {
                udp_da_maquina(&aberta, &servido, datagramas, vigia.flatten(), caminho).await;
            } else {
                estado.alheios.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Lê o canal `j` até a abertura, que vem marcada com [`TAG_DE_ABERTURA`].
///
/// **Carga que chega antes da abertura não corta o canal.** O `j` não ordena
/// nem retransmite: se a abertura se atrasou ou se perdeu, o primeiro datagrama
/// que aparece é do jogo, e lê-lo como abertura (como fazia o «primeiro
/// datagrama») cortava o canal e contava uma falta de concessão que não houve.
/// Aqui a carga é descartada e contada, e quem abriu repete a abertura. Só uma
/// abertura **malformada** corta, e a falta dela dentro do prazo.
async fn esperar_a_abertura_j(
    datagramas: &mut Datagramas,
    estado: &EstadoDaMaquina,
    caminho: &'static str,
) -> Option<AberturaDeTunel> {
    let fim = Instant::now() + PRAZO_DA_ABERTURA;
    let mut antes = 0_u64;
    loop {
        let bytes = match tokio::time::timeout_at(fim, datagramas.receber()).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                tracing::warn!(
                    caminho,
                    antes,
                    "túnel: canal j fechou sem abertura, cortado"
                );
                estado.alheios.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            Err(_) => {
                tracing::warn!(
                    caminho,
                    antes,
                    "túnel: canal j sem abertura no prazo, cortado"
                );
                estado.alheios.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };
        match ler_datagrama_j(&bytes) {
            DatagramaJ::Abertura(corpo) => {
                return match seele_proto::control::decode::<AberturaDeTunel>(corpo) {
                    Ok(aberta) => {
                        if antes > 0 {
                            tracing::info!(mod_id = %aberta.mod_id, tunel = %aberta.nome, caminho, antes, "túnel: a abertura do canal j chegou depois de carga, que foi descartada");
                        }
                        Some(aberta)
                    }
                    Err(erro) => {
                        tracing::warn!(caminho, %erro, "túnel: abertura do canal j malformada, cortado");
                        estado.alheios.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                };
            }
            DatagramaJ::Dado(_) | DatagramaJ::Ilegivel => {
                antes += 1;
                estado
                    .dados_antes_da_abertura
                    .fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    caminho,
                    antes,
                    "túnel: carga de canal j antes da abertura, descartada"
                );
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
    mut vigia: Option<watch::Receiver<()>>,
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
    let viva = ConexaoViva::nova(&servido.medidas, caminho);
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
        () = retirada(&mut vigia) => Queda::SemPermissao,
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
    mut vigia: Option<watch::Receiver<()>>,
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
    let viva = ConexaoViva::nova(&servido.medidas, caminho);
    let mut parada = servido.parada.subscribe();
    // O primeiro byte é o tag de dado, posto uma vez: o jogo escreve a partir
    // do segundo, e o datagrama sai como está, sem outra cópia.
    let mut buf = vec![0_u8; 65_537];
    if let Some(tag) = buf.first_mut() {
        *tag = TAG_DE_DADO;
    }
    let queda = loop {
        tokio::select! {
            chegou = datagramas.receber() => {
                let Some(bytes) = chegou else { break Queda::Fechado; };
                // A abertura repetida (a de antes pode ter se perdido no
                // caminho até aqui) já foi atendida: ignorada. Carga é o que
                // segue ao jogo.
                let DatagramaJ::Dado(dado) = ler_datagrama_j(&bytes) else { continue; };
                servido.medidas.ida.fetch_add(dado.len() as u64, Ordering::Relaxed);
                match socket.send(dado).await {
                    Ok(_) => {}
                    Err(erro) if erro.kind() == std::io::ErrorKind::ConnectionRefused => break Queda::JogoRecusou,
                    Err(erro) => tracing::debug!(%erro, "túnel: o jogo não recebeu um datagrama"),
                }
            }
            lido = socket.recv(buf.get_mut(1..).unwrap_or_default()) => match lido {
                Ok(n) => {
                    servido.medidas.volta.fetch_add(n as u64, Ordering::Relaxed);
                    let dado = buf.get(..=n).unwrap_or_default();
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
            () = retirada(&mut vigia) => break Queda::SemPermissao,
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
        // Só responde a um `ServirTunel` (a máquina pediu para servir algo que
        // a oferta aceita não trazia), e essa resposta acha o pedido em
        // `servindo` antes de chegar aqui. Depois de uma concessão a quem joga
        // ela não tem sentido; se um servidor a mandasse, o túnel de quem joga
        // não é o que foi recusado, e não acaba por isso.
        RecusaDeTunel::ForaDoAceite => None,
    }
}

/// Um pedido para servir em andamento: o que a máquina pediu, e quem espera.
pub(crate) struct Servico {
    protocolo: ProtocoloDeTunel,
    porta: u16,
    resposta: oneshot::Sender<Result<(), RecusaDeTunel>>,
}

/// Os pedidos em andamento e os túneis vivos de um enlace.
pub(crate) struct Tuneis {
    jogando: HashMap<Chave, TunelDeJogador>,
    ligacoes: Ligacoes,
    /// Quem espera a porta de um túnel. Mais de um quando a pessoa clica de novo
    /// antes de o servidor responder: o verbo sai uma vez só.
    entrando: HashMap<Chave, Vec<oneshot::Sender<Result<PortaLocal, RecusaDeTunel>>>>,
    /// Os pedidos para servir, **com o que a máquina pediu** (revisão final,
    /// I3): a concessão é registrada com este protocolo e esta porta, e não
    /// com os do `TunelConcedido` — um servidor hostil não leva a máquina a
    /// discar outra porta local dela.
    servindo: HashMap<Chave, Servico>,
    /// Os túneis que seguiram pela malha sem quem hospeda e que, na volta
    /// dele, pediram entrada de novo ([`Self::reentrar`]): a concessão que
    /// chega troca a estrela deles, e não abre outra porta.
    /// Com o fim do prazo de cada um: até lá, uma recusa porque a máquina
    /// ainda não serve de novo (ela também está voltando) é pedida de novo.
    reentrando: HashMap<Chave, Instant>,
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
            reentrando: HashMap::new(),
            maquina: Maquina::nova(),
            avisos,
        }
    }

    /// Tira do mapa os túneis cujo laço já acabou sozinho.
    fn podar_os_acabados(&mut self) {
        self.jogando.retain(|_, t| t.vivo());
        self.ligacoes.podar(&self.jogando);
    }

    /// **A conexão com quem hospeda caiu** (API 8, «A reserva», último item:
    /// sem quem hospeda, quem joga pela malha continua jogando).
    ///
    /// Nada é derrubado daqui: cada túnel de quem joga decide no próprio laço
    /// ([`Evento::EstrelaCaiu`]) — o que anda pela estrela acaba com
    /// [`Queda::AnfitriaoSaiu`], o que anda pela ligação direta segue. A
    /// máquina segue servindo quem já está ligado a ela pela malha
    /// ([`Maquina::sem_anfitriao`]). **Nada novo abre sem quem hospeda**: um
    /// túnel novo precisa da concessão e da apresentação dele, que conferem as
    /// permissões; o que segue é só o que ele já tinha concedido. Os pedidos
    /// que esperavam resposta não a terão.
    pub(crate) fn sem_anfitriao(&mut self) {
        self.entrando.clear();
        self.servindo.clear();
        self.reentrando.clear();
        self.podar_os_acabados();
        self.maquina.sem_anfitriao();
    }

    /// **Quem hospeda voltou**: os túneis que seguiram pela malha pedem
    /// entrada de novo, para quem hospeda os conhecer (e conferir que a
    /// pessoa ainda pode); a concessão troca a estrela deles, e a recusa os
    /// acaba com o motivo. Devolve as chaves cujo `EntrarNoTunel` tem de sair.
    /// A máquina dá às ligações diretas que seguiram um prazo para quem
    /// hospeda as apresentar de novo ([`Maquina::anfitriao_voltou`]).
    pub(crate) fn reentrar(&mut self) -> Vec<Chave> {
        self.podar_os_acabados();
        self.maquina.anfitriao_voltou();
        let chaves: Vec<Chave> = self.jogando.keys().cloned().collect();
        let prazo = Instant::now() + PRAZO_DA_REAPRESENTACAO;
        self.reentrando
            .extend(chaves.iter().cloned().map(|chave| (chave, prazo)));
        chaves
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
        self.podar_os_acabados();
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
        protocolo: ProtocoloDeTunel,
        porta: u16,
        resposta: oneshot::Sender<Result<(), RecusaDeTunel>>,
    ) {
        self.servindo.insert(
            chave,
            Servico {
                protocolo,
                porta,
                resposta,
            },
        );
    }

    /// Responde `recusa` a um pedido cujo verbo não saiu (ou foi recusado).
    pub(crate) fn recusar_pedido(&mut self, chave: &Chave, recusa: RecusaDeTunel) {
        for resposta in self.entrando.remove(chave).into_iter().flatten() {
            let _ = resposta.send(Err(recusa));
        }
        if let Some(servico) = self.servindo.remove(chave) {
            let _ = servico.resposta.send(Err(recusa));
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
        self.reentrando.clear();
        self.maquina.derrubar_tudo(queda);
        let _ = tokio::time::timeout(teto, async {
            for espera in esperas {
                let _ = espera.await;
            }
        })
        .await;
    }

    /// Tudo, de uma vez, sem esperar os relatórios. A queda da conexão não
    /// passa mais por aqui ([`Self::sem_anfitriao`]); fica para as provas.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn derrubar_tudo(&mut self, queda: Queda) {
        for (_, tunel) in self.jogando.drain() {
            tunel.encerrar(queda);
        }
        self.ligacoes.por_maquina.clear();
        self.entrando.clear();
        self.servindo.clear();
        self.reentrando.clear();
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
                self.recusado(mod_id, nome, *motivo, enviar);
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
            ServerMessage::TunelRetirado { mod_id, nome, quem } => {
                self.maquina.retirar(mod_id, nome, *quem);
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
        if let Some(servico) = self.servindo.remove(&chave) {
            // **A porta é a que esta máquina pediu** (I3). Uma concessão com
            // outra porta ou outro protocolo não é a do pedido: não se
            // registra, o servidor ouve que a máquina não serve, e quem pediu
            // ouve `fora-do-aceite`, com os dois no log.
            if (servico.protocolo, servico.porta) != (protocolo, porta) {
                tracing::warn!(
                    %mod_id,
                    tunel = %nome,
                    pedida = servico.porta,
                    concedida = porta,
                    protocolo_pedido = ?servico.protocolo,
                    protocolo_concedido = ?protocolo,
                    "túnel: a concessão traz outra porta ou outro protocolo que o pedido; a máquina não serve"
                );
                enviar(ClientMessage::PararTunel {
                    mod_id: mod_id.to_owned(),
                    nome: nome.to_owned(),
                });
                let _ = servico.resposta.send(Err(RecusaDeTunel::ForaDoAceite));
                return;
            }
            let resposta = servico.resposta;
            self.maquina
                .registrar(mod_id, nome, servico.protocolo, servico.porta);
            if let Some(estrela) = estrela {
                self.maquina
                    .aceitar_pela_estrela(Via::nova(estrela.clone()));
            }
            tracing::info!(%mod_id, tunel = %nome, porta, ?protocolo, "túnel: esta máquina serve");
            let _ = resposta.send(Ok(()));
            return;
        }
        if self.reentrando.remove(&chave).is_some() {
            if let (Some(tunel), Some(estrela)) = (self.jogando.get(&chave), estrela) {
                if tunel.vivo() {
                    tracing::info!(%mod_id, tunel = %nome, "túnel: quem hospeda voltou e concedeu de novo; a mesma porta segue");
                    tunel.nova_estrela(Via::nova(estrela.clone()));
                    return;
                }
            }
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
                    conexoes_diretas: 0,
                    conexoes_pela_estrela: 0,
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

    fn recusado(&mut self, mod_id: &str, nome: &str, motivo: RecusaDeTunel, enviar: &Enviar) {
        let chave = (mod_id.to_owned(), nome.to_owned());
        if let Some(prazo) = self.reentrando.get(&chave).copied() {
            // **A máquina também está voltando**: quem hospeda não a tem ainda
            // (`NaoExiste`, `MaquinaSaiu`) até a metade de servidor chamá-la
            // de novo e ela servir. Dentro do prazo, pede-se de novo; a
            // ligação direta segue enquanto isso.
            let ainda_vem = matches!(
                motivo,
                RecusaDeTunel::NaoExiste | RecusaDeTunel::MaquinaSaiu
            );
            if ainda_vem
                && Instant::now() < prazo
                && self.jogando.get(&chave).is_some_and(TunelDeJogador::vivo)
            {
                tracing::info!(%mod_id, tunel = %nome, ?motivo, "túnel: quem hospeda voltou mas a máquina ainda não serve; pede-se de novo");
                let enviar = Arc::clone(enviar);
                let (mod_id, nome) = (mod_id.to_owned(), nome.to_owned());
                tokio::spawn(async move {
                    tokio::time::sleep(INTERVALO_DA_REENTRADA).await;
                    enviar(ClientMessage::EntrarNoTunel { mod_id, nome });
                });
                return;
            }
            self.reentrando.remove(&chave);
            // **Quem hospeda voltou e não concede mais** (a pessoa perdeu a
            // ação, a máquina não serve): o túnel que seguiu pela malha acaba
            // com o motivo dele.
            let queda = fim_pela_recusa(motivo).unwrap_or(Queda::SemPermissao);
            tracing::warn!(%mod_id, tunel = %nome, ?motivo, ?queda, "túnel: quem hospeda voltou e recusou o túnel que seguia pela malha");
            if let Some(tunel) = self.jogando.remove(&chave) {
                tunel.encerrar(queda);
                self.ligacoes.podar(&self.jogando);
            }
            return;
        }
        if let Some(Servico { resposta, .. }) = self.servindo.remove(&chave) {
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
        } else if motivo == RecusaDeTunel::CanaisEsgotados {
            // O túnel segue, mas a pessoa tem de saber por que a conexão dela
            // caiu: só o `CaminhoCaiu` da conexão cortada não diria que foi
            // falta de vaga.
            if let Some(tunel) = self.jogando.get(&chave) {
                let _ = tunel.eventos.send(Evento::Quedou(Queda::SemCanais));
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use seele_enlace::teste::{par_em_loopback, ParDeTeste};
    use seele_proto::tunel::TAG_DE_ABERTURA;
    use tokio::net::TcpStream;

    const MOD: &str = "seele/jogo";
    const NOME: &str = "mundo";

    // ---- sem quem hospeda

    /// **Na volta de quem hospeda, a ligação direta só segue com o que ele
    /// apresentou de novo.** Sem ele, nada é conferido; na volta, um túnel que
    /// ele não apresentou outra vez a esta pessoa sai da ligação (os canais
    /// dela nele acabam), e a pessoa sem nenhum é esquecida.
    #[test]
    fn na_volta_de_quem_hospeda_so_segue_o_que_ele_apresentou_de_novo() {
        let chave = |n: &str| (MOD.to_owned(), n.to_owned());
        let de_ana = Permitidos::de(chave("a"));
        de_ana.permitir(chave("b"));
        let de_bia = Permitidos::de(chave("a"));
        let atendendo = Mutex::new(HashMap::from([
            (
                PersonId(1),
                Atendimento {
                    id: 1,
                    permitidos: Arc::clone(&de_ana),
                },
            ),
            (
                PersonId(2),
                Atendimento {
                    id: 2,
                    permitidos: Arc::clone(&de_bia),
                },
            ),
        ]));
        for a in atendendo.lock().unwrap().values() {
            a.permitidos.marcar_a_confirmar();
        }
        // Quem hospeda voltou e apresentou de novo só o `a` de Ana.
        de_ana.permitir(chave("a"));
        conferir_a_volta(&atendendo);
        assert!(
            de_ana.vigia(&chave("a")).is_some(),
            "o túnel apresentado de novo saiu"
        );
        assert!(
            de_ana.vigia(&chave("b")).is_none(),
            "o túnel que quem hospeda não apresentou de novo segue na ligação direta"
        );
        assert!(de_bia.vazio());
        let atendendo = atendendo.lock().unwrap();
        assert!(atendendo.contains_key(&PersonId(1)));
        assert!(
            !atendendo.contains_key(&PersonId(2)),
            "quem ficou sem nenhum túnel segue sendo atendido"
        );
    }

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

    /// **M7.** Um `accept` que falha sempre não pode girar: cada erro seguido
    /// espera o dobro (50 ms até 2 s), só o primeiro da rajada é dito, e um
    /// `accept` bom zera a conta.
    #[tokio::test(start_paused = true)]
    async fn o_accept_que_falha_sempre_espera_mais_a_cada_erro_e_so_a_rajada_e_dita() {
        let mut erros = ErrosDeAccept::default();
        let mut pausas = Vec::new();
        let mut ditos = 0;
        for _ in 0..9 {
            let agora = Instant::now();
            if erros.falhou(agora) {
                ditos += 1;
            }
            pausas.push(erros.pausa_ate.unwrap() - agora);
        }
        assert_eq!(ditos, 1, "a rajada foi dita mais de uma vez");
        assert_eq!(
            pausas,
            [50, 100, 200, 400, 800, 1600, 2000, 2000, 2000].map(Duration::from_millis)
        );
        // A pausa existe de verdade: o `accept` seguinte só vem depois dela.
        let antes = Instant::now();
        erros.pausa().await;
        assert!(Instant::now() - antes >= Duration::from_millis(2000));
        // Um `accept` bom zera a conta; a próxima rajada é dita de novo.
        erros.deu_certo();
        assert!(erros.pausa_ate.is_none());
        assert!(erros.falhou(Instant::now()));
    }

    // ---- o canal j: a abertura marcada, repetida, e a carga antes dela

    fn abertura_udp_de_teste() -> AberturaDeTunel {
        abertura(MOD, NOME, 99, TipoDeCanalDeTunel::Udp)
    }

    fn dado_marcado(carga: &[u8]) -> Vec<u8> {
        let mut d = vec![TAG_DE_DADO];
        d.extend_from_slice(carga);
        d
    }

    #[tokio::test(start_paused = true)]
    async fn a_abertura_repetida_dobra_o_intervalo_ate_um_segundo() {
        let mut repeticao = Repeticao::nova(Instant::now());
        assert!(!repeticao.vencida(Instant::now()));
        let mut vistos = Vec::new();
        for _ in 0..6 {
            tokio::time::advance(repeticao.intervalo() + Duration::from_millis(1)).await;
            assert!(repeticao.vencida(Instant::now()));
            repeticao.repetiu(Instant::now());
            assert!(
                !repeticao.vencida(Instant::now()),
                "repetiu e já está vencida de novo"
            );
            vistos.push(repeticao.intervalo());
        }
        assert_eq!(
            vistos,
            [200, 400, 800, 1000, 1000, 1000].map(Duration::from_millis)
        );
    }

    /// **I2, quem abre.** A abertura vai marcada e **repete** até algo voltar:
    /// aqui o primeiro datagrama «se perde» (o teste o descarta como a rede
    /// faria) e a repetição tem de chegar antes de a carga ser inútil. Depois
    /// que a máquina responde, a repetição para.
    #[tokio::test]
    async fn quem_joga_repete_a_abertura_do_j_ate_a_carga_voltar() {
        let par = par_em_loopback().await;
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let aberta = abrir_porta_local(ProtocoloDeTunel::Udp, 0).await.unwrap();
        let (_tunel, porta) =
            TunelDeJogador::iniciar(MOD, NOME, aberta, Via::nova(par.cliente.clone()), avisos)
                .unwrap();
        let jogo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        jogo.connect(porta.endereco()).await.unwrap();
        jogo.send(b"a").await.unwrap();

        // O túnel também abre o canal de controle (um `t`, o do ping): fica
        // guardado, sem resposta, enquanto se espera o `j`.
        let da_maquina = Via::nova(par.anfitriao.clone());
        let mut controles = Vec::new();
        let mut maquina = loop {
            match da_maquina.aceitar().await.unwrap() {
                CanalDeTunel::SemOrdem(d) => break d,
                t @ CanalDeTunel::Ordenado(..) => controles.push(t),
            }
        };
        // O primeiro datagrama é a abertura, marcada. A rede o perde.
        let primeiro = tokio::time::timeout(Duration::from_secs(5), maquina.receber())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(ler_datagrama_j(&primeiro), DatagramaJ::Abertura(_)),
            "o primeiro datagrama do j tem de ser a abertura marcada: {primeiro:?}"
        );

        // O jogo segue falando; a abertura tem de reaparecer.
        let mut repetida = false;
        let mut dado = false;
        for _ in 0..80 {
            jogo.send(b"b").await.unwrap();
            while let Ok(Some(b)) =
                tokio::time::timeout(Duration::from_millis(25), maquina.receber()).await
            {
                match ler_datagrama_j(&b) {
                    DatagramaJ::Abertura(corpo) => {
                        let de = seele_proto::control::decode::<AberturaDeTunel>(corpo).unwrap();
                        assert_eq!((de.mod_id.as_str(), de.nome.as_str()), (MOD, NOME));
                        repetida = true;
                    }
                    DatagramaJ::Dado(_) => dado = true,
                    DatagramaJ::Ilegivel => panic!("datagrama sem tag: {b:?}"),
                }
            }
            if repetida && dado {
                break;
            }
        }
        assert!(
            repetida && dado,
            "a abertura não foi repetida (repetida {repetida}, dado {dado})"
        );

        // A máquina responde: a carga volta ao jogo, e a repetição para.
        maquina.mandar(&dado_marcado(b"resposta")).unwrap();
        let mut buf = vec![0_u8; 2048];
        let n = tokio::time::timeout(Duration::from_secs(5), jogo.recv(&mut buf))
            .await
            .expect("a resposta não chegou ao jogo")
            .unwrap();
        assert_eq!(&buf[..n], b"resposta");
        // Esvazia o que ainda andava e confere o silêncio de aberturas por
        // mais que o teto da repetição (1 s).
        while tokio::time::timeout(Duration::from_millis(100), maquina.receber())
            .await
            .is_ok()
        {}
        let ate = Instant::now() + Duration::from_millis(1400);
        while Instant::now() < ate {
            jogo.send(b"c").await.unwrap();
            if let Ok(Some(b)) =
                tokio::time::timeout(Duration::from_millis(50), maquina.receber()).await
            {
                assert!(
                    !matches!(ler_datagrama_j(&b), DatagramaJ::Abertura(_)),
                    "a abertura seguiu sendo repetida depois de a carga voltar"
                );
            }
        }
    }

    /// **I2, quem recebe.** Carga antes da abertura é descartada e contada, a
    /// abertura repetida depois da primeira é ignorada, e o canal **não** é
    /// cortado: o eco do jogo só responde ao que vem depois da abertura.
    #[tokio::test]
    async fn a_carga_antes_da_abertura_e_contada_e_nao_corta_o_canal_na_maquina() {
        let (jogo, _eco) = eco_udp().await;
        let cena = cena(ProtocoloDeTunel::Udp, jogo).await;
        let estado = cena.maquina.estado();
        // Um canal `j` à mão, no lugar do túnel de quem joga.
        let CanalDeTunel::SemOrdem(canal) = Via::nova(cena._par.cliente.clone())
            .abrir(Confiabilidade::SemOrdem)
            .await
            .unwrap()
        else {
            panic!("pediu j");
        };
        canal.mandar(&dado_marcado(b"cedo")).unwrap();
        canal.mandar(&dado_marcado(b"cedo de novo")).unwrap();
        let abertura_marcada = abertura_j(&abertura_udp_de_teste()).unwrap();
        canal.mandar(&abertura_marcada).unwrap();
        canal.mandar(&abertura_marcada).unwrap();

        let mut canal = canal;
        let mut recebido = None;
        for _ in 0..80 {
            canal.mandar(&dado_marcado(b"eco?")).unwrap();
            if let Ok(Some(b)) =
                tokio::time::timeout(Duration::from_millis(50), canal.receber()).await
            {
                recebido = Some(b);
                break;
            }
        }
        assert_eq!(
            recebido.as_deref().map(ler_datagrama_j),
            Some(DatagramaJ::Dado(b"eco?".as_slice())),
            "o eco não voltou: o canal foi cortado pela carga que veio antes da abertura"
        );
        assert_eq!(estado.dados_antes_da_abertura(), 2);
        assert_eq!(
            estado.canais_alheios(),
            0,
            "o canal foi contado como alheio"
        );
    }

    #[tokio::test]
    async fn uma_abertura_j_malformada_corta_o_canal_e_conta() {
        let (jogo, _eco) = eco_udp().await;
        let cena = cena(ProtocoloDeTunel::Udp, jogo).await;
        let estado = cena.maquina.estado();
        let CanalDeTunel::SemOrdem(canal) = Via::nova(cena._par.cliente.clone())
            .abrir(Confiabilidade::SemOrdem)
            .await
            .unwrap()
        else {
            panic!("pediu j");
        };
        let mut lixo = vec![TAG_DE_ABERTURA];
        lixo.extend_from_slice(&[0xff; 9]);
        canal.mandar(&lixo).unwrap();
        esperar_ate(|| estado.canais_alheios() == 1).await;
        assert_eq!(estado.dados_antes_da_abertura(), 0);
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

    /// **I3.** O `caminho` do relatório só descreve as conexões **novas**: uma
    /// conexão que começou na estrela segue nela depois que o direto chega. O
    /// relatório diz quantas vivas andam por cada caminho, e a faixa do produto
    /// escreve «ligação direta» sem esconder o que ainda passa pelo anfitrião.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_relatorio_conta_as_conexoes_vivas_por_caminho_e_a_antiga_fica_na_estrela() {
        let (jogo, _eco) = eco_tcp().await;
        let mut cena = cena(ProtocoloDeTunel::Tcp, jogo).await;
        let alvo = cena.porta.endereco();

        // A primeira conexão nasce na estrela.
        let mut antiga = TcpStream::connect(alvo).await.unwrap();
        antiga.write_all(b"ola").await.unwrap();
        let mut volta = [0_u8; 3];
        antiga.read_exact(&mut volta).await.unwrap();
        let r = esperar_relatorio(&mut cena.avisos, |r| r.conexoes_pela_estrela == 1).await;
        assert_eq!((r.conexoes, r.conexoes_diretas), (1, 0), "{r:?}");
        assert_eq!(r.caminho, Caminho::Estrela);

        // A ligação direta chega: só as conexões NOVAS vão por ela.
        let serve = ponta_de_sessao();
        let busca = ponta_de_sessao();
        let impressao_serve = crate::par::impressao(serve.identidade());
        let impressao_busca = crate::par::impressao(busca.identidade());
        let (maquina_id, joga_id) = (PersonId(1), PersonId(2));
        let prazo = Duration::from_secs(5);
        cena.maquina.servir_a(Apresentacao {
            ponta: serve.clone(),
            quem: joga_id,
            mod_id: MOD.to_owned(),
            nome: NOME.to_owned(),
            enderecos: vec![busca.ponta().local_addr()],
            impressao: impressao_busca,
            prazo,
        });
        let mut ligacoes = Ligacoes::default();
        let mut estado = ligacoes.obter(Alcance {
            mod_id: MOD.to_owned(),
            nome: NOME.to_owned(),
            ponta: busca.clone(),
            enderecos: vec![serve.ponta().local_addr()],
            impressao: impressao_serve,
            prazo,
            quem: maquina_id,
        });
        let ligada = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match &*estado.borrow_and_update() {
                    EstadoDaLigacao::Pronta(ligado) => return Arc::clone(ligado),
                    EstadoDaLigacao::Discando => {}
                    _ => panic!("a ligação direta falhou ou caiu"),
                }
                estado.changed().await.unwrap();
            }
        })
        .await
        .expect("a ligação direta não veio em 20 s");
        cena.tunel
            .eventos
            .send(Evento::Direto(ligada, maquina_id))
            .unwrap();
        esperar_relatorio(&mut cena.avisos, |r| {
            matches!(r.caminho, Caminho::Direto { .. })
        })
        .await;

        let mut nova = TcpStream::connect(alvo).await.unwrap();
        nova.write_all(b"ola").await.unwrap();
        nova.read_exact(&mut volta).await.unwrap();
        let r = esperar_relatorio(&mut cena.avisos, |r| r.conexoes_diretas == 1).await;
        assert!(matches!(r.caminho, Caminho::Direto { .. }), "{r:?}");
        assert_eq!(
            (r.conexoes, r.conexoes_diretas, r.conexoes_pela_estrela),
            (2, 1, 1),
            "a conexão antiga tem de seguir contada na estrela: {r:?}"
        );

        // A antiga acaba: sobra só a direta.
        drop(antiga);
        let r = esperar_relatorio(&mut cena.avisos, |r| r.conexoes_pela_estrela == 0).await;
        assert_eq!((r.conexoes, r.conexoes_diretas), (1, 1), "{r:?}");
        drop(nova);
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
        assert!(permitidos
            .vigia_logo(&(MOD.to_owned(), "b".to_owned()))
            .await
            .is_some());
        // Um que ninguém apresentou é recusado quando a graça acaba.
        assert!(permitidos
            .vigia_logo(&(MOD.to_owned(), "c".to_owned()))
            .await
            .is_none());
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

    // ---- a retirada: quem perdeu o túnel perde também a ligação direta

    /// Põe à mão, na máquina, uma ligação direta de `quem` que aceita os
    /// túneis `nomes` (o que `servir_a` faz depois de discar).
    fn atender_direto(
        maquina: &mut Maquina,
        par: &ParDeTeste,
        quem: PersonId,
        nomes: &[&str],
    ) -> tokio::task::AbortHandle {
        let permitidos = Permitidos::de((MOD.to_owned(), nomes[0].to_owned()));
        for nome in &nomes[1..] {
            permitidos.permitir((MOD.to_owned(), (*nome).to_owned()));
        }
        let tarefa = maquina.tarefas.spawn(aceitar_canais(
            Via::nova(par.anfitriao.clone()),
            Some(Arc::clone(&permitidos)),
            maquina.estado(),
            "direto",
        ));
        maquina
            .atendendo
            .lock()
            .unwrap()
            .insert(quem, Atendimento { id: 1, permitidos });
        tarefa
    }

    /// Abre um canal `t` no túnel `nome` pela ligação e prova que o eco passa.
    async fn t_com_eco(par: &ParDeTeste, nome: &str) -> (Envio, Recebe) {
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = Via::nova(par.cliente.clone())
            .abrir(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(&mut envio, &abertura(MOD, nome, 1, TipoDeCanalDeTunel::Tcp))
            .await
            .unwrap();
        envio.write_all(b"ola").await.unwrap();
        let mut ouvido = [0_u8; 3];
        tokio::time::timeout(Duration::from_secs(5), recebe.read_exact(&mut ouvido))
            .await
            .expect("o eco não voltou")
            .unwrap();
        assert_eq!(&ouvido, b"ola");
        (envio, recebe)
    }

    /// Se o canal `t` acaba (fim ou erro) dentro do prazo.
    async fn t_acabou(recebe: &mut Recebe) -> bool {
        let mut buf = [0_u8; 16];
        matches!(
            tokio::time::timeout(Duration::from_secs(5), recebe.read(&mut buf)).await,
            Ok(Ok(None | Some(0)) | Err(_))
        )
    }

    /// **I1, a máquina.** O servidor só confere os canais que passam por ele;
    /// pela ligação direta quem confere é a máquina, e a lista de permitidos só
    /// crescia. `TunelRetirado` tira o túnel da lista: o canal vivo dessa
    /// pessoa nele acaba, um canal novo no mesmo túnel é cortado (e contado),
    /// os outros túneis dela seguem, e a ligação cai quando nada mais é
    /// permitido a ela.
    #[tokio::test]
    async fn a_retirada_corta_o_canal_direto_vivo_o_novo_e_derruba_a_ligacao_vazia() {
        let (jogo, _eco) = eco_tcp().await;
        let par = par_em_loopback().await;
        let mut maquina = Maquina::nova();
        maquina.registrar(MOD, NOME, ProtocoloDeTunel::Tcp, jogo);
        maquina.registrar(MOD, "outro", ProtocoloDeTunel::Tcp, jogo);
        let estado = maquina.estado();
        let quem = PersonId(5);
        let tarefa = atender_direto(&mut maquina, &par, quem, &[NOME, "outro"]);
        assert_eq!(estado.ligacoes_diretas(), 1);

        let (_envio_vivo, mut vivo) = t_com_eco(&par, NOME).await;
        let (_envio_outro, mut outro) = t_com_eco(&par, "outro").await;

        maquina.retirar(MOD, NOME, quem);

        // O canal que estava vivo naquele túnel acaba.
        assert!(
            t_acabou(&mut vivo).await,
            "o canal direto de quem perdeu o túnel seguiu vivo"
        );
        // Um canal novo no mesmo túnel é cortado, e contado.
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = Via::nova(par.cliente.clone())
            .abrir(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        escrever_abertura_t(&mut envio, &abertura(MOD, NOME, 2, TipoDeCanalDeTunel::Tcp))
            .await
            .unwrap();
        let _ = envio.write_all(b"ola").await;
        assert!(
            t_acabou(&mut recebe).await,
            "o canal novo foi servido depois da retirada"
        );
        esperar_ate(|| estado.canais_alheios() == 1).await;

        // O outro túnel da mesma pessoa não foi tocado, e a ligação segue.
        let mut buf = [0_u8; 16];
        assert!(
            tokio::time::timeout(Duration::from_millis(300), outro.read(&mut buf))
                .await
                .is_err(),
            "o outro túnel da pessoa foi cortado junto"
        );
        assert_eq!(estado.ligacoes_diretas(), 1);

        // Retirado o último, a ligação cai: a tarefa que a atendia acaba.
        assert!(!tarefa.is_finished());
        maquina.retirar(MOD, "outro", quem);
        assert_eq!(estado.ligacoes_diretas(), 0);
        esperar_ate(|| tarefa.is_finished()).await;
        assert!(t_acabou(&mut outro).await);
        // E ninguém mais aceita canais nela.
        let CanalDeTunel::Ordenado(mut envio, mut recebe) = Via::nova(par.cliente.clone())
            .abrir(Confiabilidade::Ordenada)
            .await
            .unwrap()
        else {
            panic!("pediu t");
        };
        let _ = escrever_abertura_t(
            &mut envio,
            &abertura(MOD, "outro", 3, TipoDeCanalDeTunel::Tcp),
        )
        .await;
        let _ = envio.write_all(b"ola").await;
        let mut buf = [0_u8; 3];
        assert!(
            tokio::time::timeout(Duration::from_millis(600), recebe.read_exact(&mut buf))
                .await
                .is_err(),
            "a ligação derrubada ainda serviu um canal"
        );
    }

    #[tokio::test]
    async fn a_retirada_de_quem_nao_tem_ligacao_direta_nao_faz_nada() {
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        tuneis.tratar(
            &ServerMessage::TunelRetirado {
                mod_id: MOD.into(),
                nome: NOME.into(),
                quem: PersonId(9),
            },
            None,
            None,
            &sem_enviar(),
            Duration::from_secs(1),
        );
        assert_eq!(tuneis.estado_da_maquina().ligacoes_diretas(), 0);
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
        // E fica dito por quê: o corte por falta de vaga entra nas quedas, e
        // não some atrás do `CaminhoCaiu` da conexão cortada.
        let r = esperar_relatorio(&mut recebidos, |r| {
            r.quedas.iter().any(|(q, _)| *q == Queda::SemCanais)
        })
        .await;
        assert!(
            r.encerrado.is_none(),
            "o corte de um canal não acaba o túnel"
        );

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

    /// **A máquina serve a porta que ela pediu** (revisão final, I3): um
    /// servidor hostil que concede outra (a 22 desta máquina) não a faz discar
    /// `127.0.0.1:22` a cada canal. A concessão é recusada, dita ao servidor
    /// (`PararTunel`) e a quem pediu (`fora-do-aceite`), e nada é registrado;
    /// a concessão certa, depois, serve a porta pedida.
    #[tokio::test]
    async fn a_concessao_com_outra_porta_nao_e_servida() {
        let par = par_em_loopback().await;
        let (avisos, _recebidos) = mpsc::unbounded_channel();
        let mut tuneis = Tuneis::nova(avisos);
        let mandados = Arc::new(Mutex::new(Vec::new()));
        let enviar: Enviar = {
            let mandados = Arc::clone(&mandados);
            Arc::new(move |m| mandados.lock().unwrap().push(m))
        };
        let concessao = |protocolo, porta| ServerMessage::TunelConcedido {
            mod_id: MOD.into(),
            nome: NOME.into(),
            protocolo,
            porta,
        };
        for (protocolo, porta) in [(ProtocoloDeTunel::Tcp, 22), (ProtocoloDeTunel::Udp, 25565)] {
            let (servir, servida) = oneshot::channel();
            tuneis.pedir_servico(
                (MOD.to_owned(), NOME.to_owned()),
                ProtocoloDeTunel::Tcp,
                25565,
                servir,
            );
            tuneis.tratar(
                &concessao(protocolo, porta),
                Some(&par.anfitriao),
                None,
                &enviar,
                Duration::from_secs(1),
            );
            assert_eq!(
                servida.await.unwrap(),
                Err(RecusaDeTunel::ForaDoAceite),
                "{protocolo:?} {porta}"
            );
            assert!(!tuneis.maquina.serve(MOD, NOME), "{protocolo:?} {porta}");
        }
        assert!(matches!(
            mandados.lock().unwrap().as_slice(),
            [
                ClientMessage::PararTunel { .. },
                ClientMessage::PararTunel { .. }
            ]
        ));
        let (servir, servida) = oneshot::channel();
        tuneis.pedir_servico(
            (MOD.to_owned(), NOME.to_owned()),
            ProtocoloDeTunel::Tcp,
            25565,
            servir,
        );
        tuneis.tratar(
            &concessao(ProtocoloDeTunel::Tcp, 25565),
            Some(&par.anfitriao),
            None,
            &enviar,
            Duration::from_secs(1),
        );
        servida.await.unwrap().unwrap();
        assert!(tuneis.maquina.serve(MOD, NOME));
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
        tuneis.pedir_servico(
            (MOD.to_owned(), NOME.to_owned()),
            ProtocoloDeTunel::Tcp,
            jogo,
            servir,
        );
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
