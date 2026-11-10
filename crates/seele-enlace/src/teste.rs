//! Ajudantes para os testes de quem usa o enlace: um par de verdade em loopback.
//!
//! Não é simulação: dois sockets UDP em 127.0.0.1, ICE, DTLS e SCTP inteiros. É
//! o que o `seele-core` e o `seele-server` usam onde antes montavam um par quinn
//! em processo.

use std::collections::VecDeque;
use std::net::SocketAddr;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::Instant;

use crate::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Pino, Ponta};

/// Um par aberto: (quem ligou, o anfitrião), e as duas pontas, que precisam
/// viver enquanto o par for usado.
pub struct ParDeTeste {
    /// O lado de quem ligou.
    pub cliente: Conexao,
    /// O lado do anfitrião.
    pub anfitriao: Conexao,
    /// A ponta de quem ligou.
    pub ponta_do_cliente: Ponta,
    /// A ponta do anfitrião.
    pub ponta_do_anfitriao: Ponta,
    /// O certificado do anfitrião.
    pub certificado: CertificadoDtls,
}

/// Abre um par em loopback, com um certificado efêmero no anfitrião.
///
/// # Panics
///
/// Se o par não abrir: é ajudante de teste.
#[allow(clippy::expect_used)]
pub async fn par_em_loopback() -> ParDeTeste {
    let certificado = CertificadoDtls::efemero().expect("certificado de teste");
    par_com(ConfigDoAnfitriao::novo(certificado.clone()), certificado).await
}

/// O mesmo, com a configuração do anfitrião dada.
///
/// # Panics
///
/// Se o par não abrir.
#[allow(clippy::expect_used)]
pub async fn par_com(config: ConfigDoAnfitriao, certificado: CertificadoDtls) -> ParDeTeste {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
    let ponta_do_anfitriao = Ponta::abrir(loopback, Some(config))
        .await
        .expect("ponta do anfitrião");
    let ponta_do_cliente = Ponta::abrir(loopback, None)
        .await
        .expect("ponta do cliente");
    let discagem = Discagem::nova(
        vec![ponta_do_anfitriao.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (cliente, anfitriao) = tokio::join!(
        ponta_do_cliente.discar(discagem),
        ponta_do_anfitriao.aceitar()
    );
    ParDeTeste {
        cliente: cliente.expect("a discagem de teste falhou"),
        anfitriao: anfitriao.expect("o anfitrião de teste não aceitou"),
        ponta_do_cliente,
        ponta_do_anfitriao,
        certificado,
    }
}

/// Segura (`true`) ou solta (`false`) os anúncios de crédito (`0x04`) que esta
/// conexão manda ao par: é como um teste atrasa o primeiro crédito, ou o nega.
/// Quem escreve do outro lado espera; fim, corte e apresentação continuam saindo.
pub fn reter_credito(conexao: &Conexao, reter: bool) {
    conexao.reter_credito(reter);
}

/// O que os canais de volume desta conexão seguravam na última revisão do
/// crédito: fila não lida mais crédito concedido e não usado. Nunca passa de
/// [`crate::MAX_CREDITO_DA_CONEXAO`].
#[must_use]
pub fn credito_em_aberto(conexao: &Conexao) -> u64 {
    conexao.credito_em_aberto()
}

/// A barreira de teste entre a soma na fila e o envio de uma mensagem de dados
/// (ver [`pausar_a_proxima_entrega`]).
#[derive(Default)]
pub(crate) struct PausaDeTeste {
    armada: AtomicBool,
    canais: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}

impl PausaDeTeste {
    /// Na tarefa do enlace: se armada, avisa que parou e espera ser solta (no
    /// máximo 10 s, para um teste quebrado não travar a tarefa para sempre).
    pub(crate) fn talvez_parar(&self) {
        if !self.armada.swap(false, Ordering::AcqRel) {
            return;
        }
        let par = self.canais.lock().ok().and_then(|mut c| c.take());
        if let Some((parou, solta)) = par {
            let _ = parou.send(());
            let _ = solta.recv_timeout(Duration::from_secs(10));
        }
    }
}

/// A próxima entrega de dados desta conexão parou na barreira.
pub struct Pausa {
    parou: mpsc::Receiver<()>,
    solta: mpsc::Sender<()>,
}

impl Pausa {
    /// Espera a tarefa do enlace parar na barreira (bloqueia a thread: chame de
    /// `spawn_blocking`, num runtime de várias threads).
    ///
    /// # Panics
    ///
    /// Se ela não parar em 10 s.
    #[allow(clippy::expect_used)]
    pub fn esperar_parar(&self) {
        self.parou
            .recv_timeout(Duration::from_secs(10))
            .expect("a entrega não chegou à barreira");
    }

    /// Solta a tarefa do enlace.
    pub fn soltar(self) {
        let _ = self.solta.send(());
    }
}

/// Arma a barreira: a próxima mensagem de dados que chegar a esta conexão para
/// **depois** de somada na fila e **antes** de entrar nela. É como um teste
/// monta, sem sorte, a corrida entre a tarefa do enlace e quem larga o `Recebe`
/// (`stop`/`drop`) numa runtime de várias threads.
#[must_use]
pub fn pausar_a_proxima_entrega(conexao: &Conexao) -> Pausa {
    let (parou_tx, parou_rx) = mpsc::channel();
    let (solta_tx, solta_rx) = mpsc::channel();
    let pausa = conexao.pausa_de_teste();
    if let Ok(mut canais) = pausa.canais.lock() {
        *canais = Some((parou_tx, solta_rx));
    }
    pausa.armada.store(true, Ordering::Release);
    Pausa {
        parou: parou_rx,
        solta: solta_tx,
    }
}

/// O que o DCEP trouxe da configuração de um canal `j`, como este lado a leu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigDoJogo {
    /// O canal entrega em ordem.
    pub ordenado: bool,
    /// O canal não retransmite (`MaxRetransmits { retransmits: 0 }`).
    pub sem_retransmissao: bool,
}

/// A configuração com que o `j` chegou a este lado: é como um teste prova que o
/// canal nasce sem ordem e sem retransmissão, e não só que os datagramas passam
/// (em loopback nada se perde, então um `j` confiável também passaria).
#[must_use]
pub fn configuracao_do_jogo(canal: &crate::Datagramas) -> ConfigDoJogo {
    let lida = canal.config_lida();
    ConfigDoJogo {
        ordenado: lida.ordenado,
        sem_retransmissao: lida.sem_retransmissao,
    }
}

// --------------------------------------------------------------------------
// o cano: um caminho ruim entre quem liga e o anfitrião
// --------------------------------------------------------------------------

/// O que o cano faz com os datagramas UDP de **um** sentido.
///
/// A ordem das etapas, para cada datagrama que chega: (1) o sorteio da perda;
/// (2) o gargalo, se houver banda — uma fila em bytes, que descarta **pela
/// cauda** o datagrama que não cabe; (3) o atraso fixo mais o jitter. O cano
/// **não reordena**: um datagrama nunca sai antes do que chegou antes dele (o
/// jitter estica o intervalo, não troca a ordem).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sentido {
    /// A fração perdida ao acaso, de 0.0 a 1.0. Perda independente por
    /// datagrama (Bernoulli), **não** em rajada.
    pub perda: f64,
    /// O atraso fixo de cada datagrama.
    pub atraso: Duration,
    /// O atraso a mais, sorteado uniforme entre zero e este valor.
    pub jitter: Duration,
    /// A banda do gargalo em bits por segundo, contando o corpo do datagrama
    /// UDP (sem os cabeçalhos IP e UDP). `None` é sem limite.
    pub banda_bps: Option<u64>,
    /// Quantos bytes cabem na fila do gargalo. Só vale com `banda_bps`.
    pub fila_max: usize,
}

impl Sentido {
    /// Um sentido que só repassa: sem perda, sem atraso e sem limite de banda.
    #[must_use]
    pub const fn limpo() -> Self {
        Self {
            perda: 0.0,
            atraso: Duration::ZERO,
            jitter: Duration::ZERO,
            banda_bps: None,
            fila_max: 64 * 1024,
        }
    }
}

/// A configuração do cano: um [`Sentido`] para cada lado e a semente do sorteio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigDoCano {
    /// De quem liga para o anfitrião.
    pub ida: Sentido,
    /// Do anfitrião para quem liga.
    pub volta: Sentido,
    /// A semente do gerador. Cada sentido tem o seu gerador, derivado dela: com
    /// a mesma semente, o n-ésimo datagrama de um sentido tem sempre o mesmo
    /// destino (passa ou se perde) e o mesmo jitter.
    pub semente: u64,
}

impl ConfigDoCano {
    /// O mesmo [`Sentido`] nos dois lados.
    #[must_use]
    pub const fn simetrico(sentido: Sentido, semente: u64) -> Self {
        Self {
            ida: sentido,
            volta: sentido,
            semente,
        }
    }
}

/// O que o cano contou num sentido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContagemDoSentido {
    /// Datagramas que saíram do outro lado.
    pub repassados: u64,
    /// Bytes que saíram do outro lado.
    pub bytes_repassados: u64,
    /// Datagramas que o sorteio da perda tirou.
    pub perdidos_no_sorteio: u64,
    /// Datagramas que não couberam na fila do gargalo.
    pub descartados_na_fila: u64,
}

impl ContagemDoSentido {
    /// Tudo o que este sentido descartou: o sorteio mais a fila.
    #[must_use]
    pub const fn descartados(&self) -> u64 {
        self.perdidos_no_sorteio + self.descartados_na_fila
    }
}

#[derive(Default)]
struct ContadoresDoSentido {
    repassados: AtomicU64,
    bytes_repassados: AtomicU64,
    perdidos_no_sorteio: AtomicU64,
    descartados_na_fila: AtomicU64,
    /// O destino dos primeiros [`DESTINOS_GUARDADOS`] datagramas: `true` é
    /// perdido no sorteio. É o que deixa um teste provar que a semente manda.
    destinos: Mutex<Vec<bool>>,
}

/// Quantos destinos (passou ou se perdeu no sorteio) o cano guarda por sentido.
pub const DESTINOS_GUARDADOS: usize = 4096;

impl ContadoresDoSentido {
    fn ler(&self) -> ContagemDoSentido {
        ContagemDoSentido {
            repassados: self.repassados.load(Ordering::Relaxed),
            bytes_repassados: self.bytes_repassados.load(Ordering::Relaxed),
            perdidos_no_sorteio: self.perdidos_no_sorteio.load(Ordering::Relaxed),
            descartados_na_fila: self.descartados_na_fila.load(Ordering::Relaxed),
        }
    }
}

/// O gerador do cano (SplitMix64): pequeno, sem dependência e o mesmo em toda
/// máquina. Não é para cripto; é para um sorteio que se repete.
struct Sorteio(u64);

impl Sorteio {
    fn proximo(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniforme em [0, 1), com 53 bits.
    #[allow(clippy::cast_precision_loss)]
    fn fracao(&mut self) -> f64 {
        (self.proximo() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// Um repasse UDP em loopback entre quem liga e o anfitrião, que perde, atrasa
/// e estreita o caminho como a [`ConfigDoCano`] mandar.
///
/// Quem liga disca [`Cano::endereco`]; o cano repassa ao anfitrião por um
/// socket próprio e devolve as respostas. Para o anfitrião, o par é o socket
/// do cano — é o que um NAT faria. Largar o `Cano` para o repasse.
pub struct Cano {
    endereco: SocketAddr,
    ida: Arc<ContadoresDoSentido>,
    volta: Arc<ContadoresDoSentido>,
    tarefas: Vec<tokio::task::JoinHandle<()>>,
}

impl Cano {
    /// Abre o cano diante de `anfitriao`.
    ///
    /// # Errors
    ///
    /// Se um dos dois sockets em 127.0.0.1 não abrir.
    pub async fn abrir(anfitriao: SocketAddr, config: ConfigDoCano) -> std::io::Result<Self> {
        let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
        let para_quem_liga = Arc::new(UdpSocket::bind(loopback).await?);
        let para_o_anfitriao = Arc::new(UdpSocket::bind(loopback).await?);
        let endereco = para_quem_liga.local_addr()?;
        let quem_liga: Arc<Mutex<Option<SocketAddr>>> = Arc::default();
        let ida: Arc<ContadoresDoSentido> = Arc::default();
        let volta: Arc<ContadoresDoSentido> = Arc::default();
        let tarefas = vec![
            tokio::spawn(encanar(
                Arc::clone(&para_quem_liga),
                Arc::clone(&para_o_anfitriao),
                Destino::Fixo(anfitriao),
                Some(Arc::clone(&quem_liga)),
                config.ida,
                Sorteio(config.semente),
                Arc::clone(&ida),
            )),
            tokio::spawn(encanar(
                para_o_anfitriao,
                para_quem_liga,
                Destino::Aprendido(quem_liga),
                None,
                config.volta,
                // Outro gerador para a volta: com o mesmo, os dois sentidos
                // perderiam os datagramas de mesmo índice.
                Sorteio(config.semente ^ 0xA5A5_5A5A_C3C3_3C3C),
                Arc::clone(&volta),
            )),
        ];
        Ok(Self {
            endereco,
            ida,
            volta,
            tarefas,
        })
    }

    /// O endereço que quem liga disca.
    #[must_use]
    pub const fn endereco(&self) -> SocketAddr {
        self.endereco
    }

    /// O que passou e o que se perdeu de quem liga para o anfitrião.
    #[must_use]
    pub fn ida(&self) -> ContagemDoSentido {
        self.ida.ler()
    }

    /// O que passou e o que se perdeu do anfitrião para quem liga.
    #[must_use]
    pub fn volta(&self) -> ContagemDoSentido {
        self.volta.ler()
    }

    /// O destino que o sorteio deu aos primeiros datagramas da ida, na ordem em
    /// que chegaram (`true` é perdido), até [`DESTINOS_GUARDADOS`].
    #[must_use]
    pub fn destinos_da_ida(&self) -> Vec<bool> {
        self.ida
            .destinos
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default()
    }
}

impl Drop for Cano {
    fn drop(&mut self) {
        for tarefa in &self.tarefas {
            tarefa.abort();
        }
    }
}

impl std::fmt::Debug for Cano {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cano")
            .field("endereco", &self.endereco)
            .field("ida", &self.ida())
            .field("volta", &self.volta())
            .finish_non_exhaustive()
    }
}

enum Destino {
    Fixo(SocketAddr),
    /// O endereço de quem liga: o de onde veio o último datagrama da ida.
    Aprendido(Arc<Mutex<Option<SocketAddr>>>),
}

impl Destino {
    fn agora(&self) -> Option<SocketAddr> {
        match self {
            Self::Fixo(endereco) => Some(*endereco),
            Self::Aprendido(visto) => visto.lock().ok().and_then(|v| *v),
        }
    }
}

/// O repasse de um sentido. Só acaba quando a tarefa é abortada.
async fn encanar(
    entrada: Arc<UdpSocket>,
    saida: Arc<UdpSocket>,
    destino: Destino,
    aprender: Option<Arc<Mutex<Option<SocketAddr>>>>,
    sentido: Sentido,
    mut sorteio: Sorteio,
    contadores: Arc<ContadoresDoSentido>,
) {
    let mut buffer = vec![0_u8; 64 * 1024];
    // A linha de atraso: (quando pode sair, o datagrama), na ordem de chegada.
    let mut a_caminho: VecDeque<(Instant, Vec<u8>)> = VecDeque::new();
    // Quando o gargalo termina de escoar o que já aceitou. O que há na fila
    // dele, em tempo, é `gargalo_livre_em - agora`.
    let mut gargalo_livre_em = Instant::now();

    loop {
        // Despacha o que venceu **antes** de voltar ao `select`: com a entrada
        // sempre pronta, o `select` poderia nunca escolher o temporizador, e o
        // cano mediria o escalonador do tokio em vez do caminho.
        let agora = Instant::now();
        while a_caminho
            .front()
            .is_some_and(|(quando, _)| *quando <= agora)
        {
            let Some((_, dados)) = a_caminho.pop_front() else {
                break;
            };
            if let Some(para) = destino.agora() {
                if saida.send_to(&dados, para).await.is_ok() {
                    contadores.repassados.fetch_add(1, Ordering::Relaxed);
                    contadores
                        .bytes_repassados
                        .fetch_add(dados.len() as u64, Ordering::Relaxed);
                }
            }
        }
        let proximo = a_caminho.front().map(|(quando, _)| *quando);

        tokio::select! {
            recebido = entrada.recv_from(&mut buffer) => {
                let Ok((tamanho, de)) = recebido else { continue };
                if let Some(visto) = &aprender {
                    if let Ok(mut visto) = visto.lock() {
                        *visto = Some(de);
                    }
                }
                let Some(bytes) = buffer.get(..tamanho) else { continue };
                let agora = Instant::now();

                // 1. A perda. O sorteio roda para todo datagrama, com perda
                // zero também: assim o n-ésimo datagrama consome sempre os
                // mesmos números, e a semente decide o destino dele.
                let sorteado = sorteio.fracao();
                let espera_a_mais = sentido.jitter.mul_f64(sorteio.fracao());
                let perdido = sorteado < sentido.perda;
                if let Ok(mut destinos) = contadores.destinos.lock() {
                    if destinos.len() < DESTINOS_GUARDADOS {
                        destinos.push(perdido);
                    }
                }
                if perdido {
                    contadores.perdidos_no_sorteio.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                // 2. O gargalo: uma fila em bytes, descarte pela cauda.
                let mut sai_do_gargalo = agora;
                if let Some(bps) = sentido.banda_bps.filter(|bps| *bps > 0) {
                    let comeca = gargalo_livre_em.max(agora);
                    let na_fila = bytes_em(comeca.saturating_duration_since(agora), bps);
                    if na_fila + tamanho as u64 > sentido.fila_max as u64 {
                        contadores.descartados_na_fila.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    gargalo_livre_em = comeca + tempo_de(tamanho as u64, bps);
                    sai_do_gargalo = gargalo_livre_em;
                }

                // 3. O atraso e o jitter. A linha de atraso é uma fila: quem
                // sorteou um jitter menor que o da frente espera a frente
                // sair, e é isso que não deixa a ordem trocar.
                let quando = sai_do_gargalo + sentido.atraso + espera_a_mais;
                a_caminho.push_back((quando, bytes.to_vec()));
            }
            () = tokio::time::sleep_until(proximo.unwrap_or(agora)), if proximo.is_some() => {}
        }
    }
}

/// Quanto tempo o gargalo leva para escoar `bytes`.
fn tempo_de(bytes: u64, bps: u64) -> Duration {
    Duration::from_nanos(bytes.saturating_mul(8_000_000_000) / bps.max(1))
}

/// Quantos bytes o gargalo escoa em `tempo`.
fn bytes_em(tempo: Duration, bps: u64) -> u64 {
    let nanos = u64::try_from(tempo.as_nanos()).unwrap_or(u64::MAX);
    u64::try_from(u128::from(nanos) * u128::from(bps) / 8_000_000_000).unwrap_or(u64::MAX)
}

/// Abre um par em que **todo** datagrama passa por um [`Cano`]: quem liga
/// disca o cano, e o anfitrião só vê o socket do cano. O aperto (ICE, DTLS e
/// SCTP) também passa por ele, com a perda e o atraso pedidos.
///
/// # Panics
///
/// Se o cano ou o par não abrir: é ajudante de teste.
#[allow(clippy::expect_used)]
pub async fn par_por_cano(config: ConfigDoCano) -> (ParDeTeste, Cano) {
    let certificado = CertificadoDtls::efemero().expect("certificado de teste");
    let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
    let ponta_do_anfitriao =
        Ponta::abrir(loopback, Some(ConfigDoAnfitriao::novo(certificado.clone())))
            .await
            .expect("ponta do anfitrião");
    let ponta_do_cliente = Ponta::abrir(loopback, None)
        .await
        .expect("ponta do cliente");
    let cano = Cano::abrir(ponta_do_anfitriao.local_addr(), config)
        .await
        .expect("os sockets do cano");
    let discagem = Discagem::nova(
        vec![cano.endereco()],
        Pino::Impressao(certificado.impressao()),
    );
    let (cliente, anfitriao) = tokio::join!(
        ponta_do_cliente.discar(discagem),
        ponta_do_anfitriao.aceitar()
    );
    let par = ParDeTeste {
        cliente: cliente.expect("a discagem pelo cano falhou"),
        anfitriao: anfitriao.expect("o anfitrião não aceitou pelo cano"),
        ponta_do_cliente,
        ponta_do_anfitriao,
        certificado,
    };
    (par, cano)
}
