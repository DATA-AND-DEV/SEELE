//! Mede o compartilhamento de vídeo **do produto** sob perda e banda estreita:
//! o servidor de verdade, dois clientes de verdade, e o controle de taxa de
//! verdade no meio. É bancada, não produto: imprime tabelas em Markdown, e o
//! relatório é `docs/medidas/2026-11-10-tela-do-produto-sob-perda.md`.
//!
//! ```text
//! # o aparelho de mentira da voz não compila sem `debug_assertions`; a opção
//! # liga só isso, na linha de comando, e o resto do perfil é o `release`
//! cargo run --release --config 'profile.release.debug-assertions=true' \
//!     -p seele-conformance --example tela_do_produto_sob_perda
//! … --example tela_do_produto_sob_perda -- \
//!     [--segundos 20] [--rodadas 3] [--semente 20261110] [--so <pedaço do cenário>] \
//!     [--variante produto|fixo|cru|ambas|todas] [--aquecimento 0] [--atraso-ms 30] \
//!     [--fracao 0.85] [--fator-da-chave 8] [--sem-chave-na-troca] [--recusa-sem-chave] \
//!     [--sem-contrapressao] \
//!     [--escolha-bps 1470000] [--caminho-do-servidor-bps 50000000]
//! ```
//!
//! `SEELE_LOG=1 RUST_LOG=warn,seele_core=info` manda o registro do produto ao
//! STDERR.
//!
//! # O que é de verdade
//!
//! - `seele_server::Daemon`, e duas `seele_ffi::Connection`: quem transmite e
//!   quem assiste. Quem transmite disca o [`Cano`], que repassa ao servidor;
//!   quem assiste disca o servidor direto, por um caminho limpo.
//! - O vídeo entra pelo caminho da **câmera** (`Connection::compartilhar_camera`
//!   e `Connection::quadro_da_camera`), que é a porta do produto para quadros
//!   codificados fora do núcleo. Depois dela é tudo o do produto: o cadenciador
//!   de `seele-core/src/de_fora.rs`, a `Transmissao` com o balde e a chave em
//!   fatias (`tela.rs`), o `TetoDeVideo`, a `Sonda` (`caminho.rs`) e a faixa de
//!   sinal que o servidor devolve no `PersonState`.
//! - A voz de quem transmite: o microfone é o aparelho de mentira do
//!   `seele-audio` (um tom contínuo), em modo aberto; daí em diante é o Opus e
//!   o canal de voz do produto. Quem assiste a toca no aparelho de mentira dele.
//!
//! # O que é de mentira
//!
//! O codificador. Os quadros são enchimento com o tamanho que um codificador
//! obediente teria, e levam nos primeiros bytes o número do quadro e o instante
//! da captura. Há dois:
//!
//! - **produto** — faz o que a casca do aplicativo faz
//!   (`apps/seele-app/ui/camera.js`): mira `--fracao` (0,85, o
//!   `FRACAO_DO_TETO_DA_CAMERA` de lá) do `Event::CameraBudget`, a 30 quadros;
//!   manda chave (`--fator-da-chave` × o delta, 8 por padrão) no começo, a cada
//!   `Event::CameraKeyFrameRequested`, depois de um quadro recusado e — como a
//!   casca, que reconfigura o codificador — a cada troca de orçamento; pula 3
//!   quadros depois de uma chave (`QUADROS_APOS_A_CHAVE`); e para em
//!   `Event::CameraPaused`.
//! - **fixo** — o controle: ignora o orçamento. Deltas de 6 KiB a 30 quadros/s
//!   (≈1,47 Mbps) e chave de 80 KiB, a carga de
//!   `crates/seele-enlace/examples/tela_sob_perda.rs`. Continua mandando chave
//!   quando o núcleo pede ou recusa (sem isso a cadeia de deltas não fecharia)
//!   e continua parando em `CameraPaused`. **O balde do produto continua
//!   valendo**: o que passa do teto é descartado antes do fio.
//! - **cru** — o controle do banco: o emissor da linha «A» daquela medida, sem
//!   núcleo de cliente nenhum (um par que fala o fio à mão: sem balde, sem
//!   teto, sem sonda, sem fila), com a mesma carga do **fixo**, contra o mesmo
//!   servidor e a mesma pessoa assistindo. Sem voz.
//!
//! `--aquecimento <s>` roda o codificador esse tempo antes de abrir a janela
//! (o regime, depois de a sonda assentar). `--recusa-sem-chave` não manda
//! chave depois de um quadro recusado: é como se o quadro recusado não
//! tivesse sido codificado, que é o que a bomba da tela do sistema faz.
//!
//! # As métricas
//!
//! - Latência: do instante em que o quadro foi entregue a `quadro_da_camera`
//!   até o `Event::ScreenFrame` dele em quem assiste.
//! - Congelado: a soma dos intervalos de mais de 200 ms sem quadro novo, sobre
//!   a janela (o começo e o fim da janela contam como bordas).
//! - Orçamento: o último `CameraBudget`, amostrado a cada 250 ms (zero enquanto
//!   a câmera está parada por `CameraPaused`).
//! - Sinal do servidor: o `signal` do `PersonState` de quem transmite, lido no
//!   retrato dele (`Snapshot::presentes`). É o número de que o motor tira a
//!   faixa que corta o teto. Sinal local: `Snapshot::telemetry.signal`, o que a
//!   janela mostra — é outra conta, feita no cliente.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::print_stdout,
    clippy::print_stderr,
    missing_docs,
    reason = "um exemplo de bancada: o pânico e a linha impressa são o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use seele_audio::device::mentira;
use seele_enlace::teste::{Cano, ConfigDoCano, ContagemDoSentido, Sentido};
use seele_ffi::{
    ConnectConfig, Connection, ConnectionError, Event, EventListener, LimitesDeTela, VoiceMode,
};
use seele_proto::signal::SignalBand;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

#[path = "../tests/par_cru/mod.rs"]
mod par_cru;

const QUADROS_POR_SEGUNDO: u32 = 30;
/// A carga do controle: a de `tela_sob_perda.rs`.
const DELTA_FIXO: usize = 6 * 1024;
const CHAVE_FIXA: usize = 80 * 1024;
/// `QUADROS_APOS_A_CHAVE` de `apps/seele-app/ui/camera.js`.
const QUADROS_APOS_A_CHAVE: u32 = 3;
/// `LIMIAR_DA_FILA_DA_CAMERA` de `camera.js`.
const LIMIAR_DA_FILA_DA_CAMERA: u32 = 4;
/// `SONDA_DA_FILA_DA_CAMERA_MS` de `camera.js`.
const SONDA_DA_FILA_DA_CAMERA: Duration = Duration::from_millis(100);
/// O piso de `configuracaoDaCamera` em `camera.js`: `Math.max(100_000, …)`.
const PISO_DO_CODIFICADOR_BPS: f64 = 100_000.0;
/// Código de início, tipo da NAL, número do quadro (8) e instante (8).
const CABECALHO: usize = 5 + 16 + 16;
const ATRASO: Duration = Duration::from_millis(30);
const CONGELADO_ACIMA_DE: Duration = Duration::from_millis(200);
const AMOSTRA: Duration = Duration::from_millis(250);
/// Quanto se espera, depois da janela, pelos quadros que ainda estão a caminho.
const DRENAGEM: Duration = Duration::from_secs(3);
const PRAZO: Duration = Duration::from_secs(60);

static EPOCA: OnceLock<Instant> = OnceLock::new();

/// Nanossegundos desde o começo do processo: o relógio dos carimbos.
fn agora_ns() -> u64 {
    EPOCA.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

#[derive(Debug, Clone, Copy)]
struct Cenario {
    nome: &'static str,
    perda: f64,
    banda_bps: Option<u64>,
}

const CENARIOS: &[Cenario] = &[
    Cenario {
        nome: "0%",
        perda: 0.0,
        banda_bps: None,
    },
    Cenario {
        nome: "0,3%",
        perda: 0.003,
        banda_bps: None,
    },
    Cenario {
        nome: "1%",
        perda: 0.01,
        banda_bps: None,
    },
    Cenario {
        nome: "3%",
        perda: 0.03,
        banda_bps: None,
    },
    Cenario {
        nome: "5%",
        perda: 0.05,
        banda_bps: None,
    },
    Cenario {
        nome: "1,0 Mbps, 0%",
        perda: 0.0,
        banda_bps: Some(1_000_000),
    },
    Cenario {
        nome: "2,0 Mbps, 1%",
        perda: 0.01,
        banda_bps: Some(2_000_000),
    },
];

impl Cenario {
    fn sentido(&self, atraso: Duration) -> Sentido {
        Sentido {
            perda: self.perda,
            atraso,
            jitter: Duration::ZERO,
            banda_bps: self.banda_bps,
            // 250 ms de fila na banda do gargalo, como em `tela_sob_perda.rs`.
            fila_max: self
                .banda_bps
                .map_or(64 * 1024, |bps| (bps / 8 / 4) as usize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variante {
    Produto,
    Fixo,
    Cru,
}

impl Variante {
    const fn nome(self) -> &'static str {
        match self {
            Self::Produto => "produto",
            Self::Fixo => "fixo",
            Self::Cru => "cru",
        }
    }
}

#[derive(Debug, Clone)]
struct Opcoes {
    segundos: u64,
    rodadas: u64,
    semente: u64,
    so: Option<String>,
    variantes: Vec<Variante>,
    fracao: f64,
    fator_da_chave: usize,
    chave_na_troca: bool,
    escolha_bps: Option<u32>,
    caminho_do_servidor_bps: u32,
    aquecimento: u64,
    atraso_ms: u64,
    recusa_sem_chave: bool,
    /// A contrapressão da casca pela fila do núcleo. `--sem-contrapressao` a
    /// desliga, e é a casca de antes de 10/11/2026.
    contrapressao: bool,
}

fn opcoes() -> Opcoes {
    let mut opcoes = Opcoes {
        segundos: 20,
        rodadas: 3,
        semente: 20_261_110,
        so: None,
        variantes: vec![Variante::Produto, Variante::Fixo],
        fracao: 0.85,
        fator_da_chave: 8,
        chave_na_troca: true,
        escolha_bps: None,
        caminho_do_servidor_bps: 50_000_000,
        aquecimento: 0,
        atraso_ms: ATRASO.as_millis() as u64,
        recusa_sem_chave: false,
        contrapressao: true,
    };
    let mut argumentos = std::env::args().skip(1);
    while let Some(nome) = argumentos.next() {
        let mut valor = || argumentos.next().expect("falta o valor da opção");
        match nome.as_str() {
            "--segundos" => opcoes.segundos = valor().parse().expect("--segundos"),
            "--rodadas" => opcoes.rodadas = valor().parse().expect("--rodadas"),
            "--semente" => opcoes.semente = valor().parse().expect("--semente"),
            "--so" => opcoes.so = Some(valor()),
            "--variante" => {
                opcoes.variantes = match valor().as_str() {
                    "produto" => vec![Variante::Produto],
                    "fixo" => vec![Variante::Fixo],
                    "cru" => vec![Variante::Cru],
                    "ambas" => vec![Variante::Produto, Variante::Fixo],
                    "todas" => vec![Variante::Produto, Variante::Fixo, Variante::Cru],
                    outra => panic!("variante desconhecida: {outra}"),
                }
            }
            "--fracao" => opcoes.fracao = valor().parse().expect("--fracao"),
            "--fator-da-chave" => {
                opcoes.fator_da_chave = valor().parse().expect("--fator-da-chave");
            }
            "--sem-chave-na-troca" => opcoes.chave_na_troca = false,
            "--escolha-bps" => opcoes.escolha_bps = Some(valor().parse().expect("--escolha-bps")),
            "--caminho-do-servidor-bps" => {
                opcoes.caminho_do_servidor_bps =
                    valor().parse().expect("--caminho-do-servidor-bps");
            }
            "--aquecimento" => opcoes.aquecimento = valor().parse().expect("--aquecimento"),
            "--atraso-ms" => opcoes.atraso_ms = valor().parse().expect("--atraso-ms"),
            "--recusa-sem-chave" => opcoes.recusa_sem_chave = true,
            "--sem-contrapressao" => opcoes.contrapressao = false,
            outra => panic!("opção desconhecida: {outra}"),
        }
    }
    opcoes
}

// ---------------------------------------------------------------------------
// As duas janelas
// ---------------------------------------------------------------------------

/// O que a janela de quem transmite ouve do núcleo, e o que o codificador de
/// mentira lê dela.
#[derive(Default)]
struct DeQuemTransmite {
    orcamento_bps: AtomicU32,
    quer_chave: AtomicBool,
    parada: AtomicBool,
    aberturas: AtomicU64,
    orcamentos: AtomicU64,
    pedidos_de_chave: AtomicU64,
    pausas: AtomicU64,
    motivos: Mutex<Vec<&'static str>>,
}

impl EventListener for DeQuemTransmite {
    fn on_event(&self, event: Event) {
        match event {
            // Em quem transmite só abre a própria câmera: no começo, e de novo
            // quando o teto volta depois de uma parada.
            Event::ScreenOpened { .. } => {
                self.parada.store(false, Ordering::SeqCst);
                self.quer_chave.store(true, Ordering::SeqCst);
                self.aberturas.fetch_add(1, Ordering::SeqCst);
            }
            Event::CameraBudget { bps, .. } => {
                self.orcamento_bps.store(bps, Ordering::SeqCst);
                self.orcamentos.fetch_add(1, Ordering::SeqCst);
            }
            Event::CameraKeyFrameRequested { .. } => {
                self.quer_chave.store(true, Ordering::SeqCst);
                self.pedidos_de_chave.fetch_add(1, Ordering::SeqCst);
            }
            Event::CameraPaused { reason, .. } => {
                self.parada.store(true, Ordering::SeqCst);
                self.pausas.fetch_add(1, Ordering::SeqCst);
                self.motivos.lock().unwrap().push(reason);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Chegada {
    numero: u64,
    capturado_ns: u64,
    chegou_ns: u64,
}

#[derive(Default)]
struct DeQuemAssiste {
    chegadas: Mutex<Vec<Chegada>>,
    aberturas: AtomicU64,
    fechadas: AtomicU64,
    ilegiveis: Mutex<Vec<String>>,
}

impl EventListener for DeQuemAssiste {
    fn on_event(&self, event: Event) {
        match event {
            Event::ScreenFrame { data, .. } => {
                // O relógio antes do base64: decodificar não é atraso da rede.
                let chegou_ns = agora_ns();
                let Some(bytes) = seele_ffi::de_base64(&data) else {
                    return;
                };
                // O prefixo de início volta com três ou com quatro bytes,
                // conforme o caminho (o RTP o reescreve).
                let Some(inicio) = bytes.windows(3).position(|j| j == [0, 0, 1]) else {
                    return;
                };
                let hex = |de: usize| {
                    bytes
                        .get(inicio + 4 + de..inicio + 20 + de)
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .and_then(|t| u64::from_str_radix(t, 16).ok())
                };
                let (Some(numero), Some(capturado_ns)) = (hex(0), hex(16)) else {
                    return;
                };
                self.chegadas.lock().unwrap().push(Chegada {
                    numero,
                    capturado_ns,
                    chegou_ns,
                });
            }
            Event::ScreenOpened { .. } => {
                self.aberturas.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenClosed { .. } => {
                self.fechadas.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenUnreadable { reason } => {
                self.ilegiveis.lock().unwrap().push(reason);
            }
            _ => {}
        }
    }
}

/// Um quadro de mentira: cara de H.264 em Annex-B, o número, o instante da
/// captura, e enchimento até o tamanho pedido.
fn quadro(chave: bool, numero: u64, capturado_ns: u64, tamanho: usize) -> Vec<u8> {
    let tamanho = tamanho.max(CABECALHO);
    let mut bytes = Vec::with_capacity(tamanho);
    bytes.extend_from_slice(&[0, 0, 0, 1, if chave { 0x65 } else { 0x41 }]);
    // Em hexadecimal, e o enchimento sem zeros: um número binário traz
    // `00 00 01` dentro do quadro, que é um prefixo de início de NAL. No canal
    // isso passa como bytes; no RTP o empacotador de H.264 partiria o quadro
    // ali e a volta não teria os mesmos bytes.
    bytes.extend_from_slice(format!("{numero:016x}{capturado_ns:016x}").as_bytes());
    bytes.extend((CABECALHO..tamanho).map(|i| (i % 251) as u8 | 0x80));
    bytes
}

// ---------------------------------------------------------------------------
// O servidor e as duas pessoas
// ---------------------------------------------------------------------------

async fn servidor(caminho_bps: u32) -> (SocketAddr, Arc<Daemon>) {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Bancada".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            // Declarada e larga: a perna de quem hospeda não aperta, e o teto
            // que vale é o de quem transmite (60% do que a sonda estima).
            caminho_bps: Some(caminho_bps),
            ..ServerConfig::default()
        })
        .await
        .expect("o servidor não subiu"),
    );
    let endereco = daemon.local_addr().expect("o endereço do servidor");
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    (endereco, daemon)
}

async fn entrar(
    endereco: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
    aparelho: &str,
    olhos: Arc<dyn EventListener>,
) -> Result<Arc<Connection>, String> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id(aparelho)),
        playback_device: Some(mentira::id(aparelho)),
    };
    let foco = seele_ffi::foco::FocoDeVoz::novo();
    let conexao = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .map(|(conexao, _)| conexao)
    .map_err(|falha| format!("{:?}", falha.error))?;
    conexao.subscribe(olhos);
    Ok(conexao)
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pronto()
}

async fn sentar(conexao: &Arc<Connection>) -> bool {
    if !ate(PRAZO, || !conexao.snapshot().voice_rooms.is_empty()).await {
        return false;
    }
    let sala = conexao.snapshot().voice_rooms[0].id;
    if conexao.enter_voice_room(sala, None).is_err() {
        return false;
    }
    ate(PRAZO, || {
        conexao
            .snapshot()
            .voice_rooms
            .iter()
            .any(|sala| sala.occupied_by_us)
    })
    .await
}

// ---------------------------------------------------------------------------
// Uma célula
// ---------------------------------------------------------------------------

/// Uma amostra do que o controle de taxa dizia, a cada [`AMOSTRA`].
#[derive(Debug, Clone, Copy)]
struct Amostra {
    orcamento_bps: u32,
    parada: bool,
    sinal_do_servidor: u8,
    sinal_local: u8,
    rtt_ms: Option<f32>,
    perda: Option<f32>,
}

/// O que o codificador de mentira contou.
#[derive(Debug, Default, Clone)]
struct Codificado {
    /// O número de cada quadro que o núcleo aceitou.
    aceitos: Vec<u64>,
    recusados: u64,
    sem_camera: u64,
    parados: u64,
    pulados: u64,
    chaves: u64,
    bytes: u64,
}

/// As colunas de uma célula: (nome, casas decimais).
const COLUNAS: &[(&str, usize)] = &[
    ("lat p50", 0),                     // 0
    ("p95", 0),                         // 1
    ("p99", 0),                         // 2
    ("máx", 0),                         // 3
    ("congelado %", 1),                 // 4
    ("maior intervalo ms", 0),          // 5
    ("quadros/s", 1),                   // 6
    ("orç. mín kbps", 0),               // 7
    ("orç. mediana kbps", 0),           // 8
    ("orç. final kbps", 0),             // 9
    ("avisos de orçamento", 0),         // 10
    ("pausas", 0),                      // 11
    ("parado %", 1),                    // 12
    ("sinal servidor mín", 0),          // 13
    ("sinal servidor med.", 0),         // 14
    ("sinal local mín", 0),             // 15
    ("sinal local med.", 0),            // 16
    ("fora de Nominal %", 1),           // 17
    ("aceitos", 0),                     // 18
    ("recusados (fila)", 0),            // 19
    ("não chegaram", 0),                // 20
    ("chaves", 0),                      // 21
    ("pedidos de chave", 0),            // 22
    ("MB no cano", 2),                  // 23
    ("voz tocada %", 1),                // 24
    ("rtt p50 ms", 0),                  // 25
    ("rtt máx ms", 0),                  // 26
    ("perda medida máx %", 2),          // 27
    ("cano perda %", 2),                // 28
    ("cano fila %", 2),                 // 29
    ("kbps oferecidos", 0),             // 30
    ("reaberturas em quem assiste", 0), // 31
];

/// As colunas da tabela do que quem assiste vê, e as do que o controle fez.
const DE_QUEM_ASSISTE: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 18, 19, 20, 21, 22, 30, 23, 24, 28, 29];
const DO_CONTROLE: &[usize] = &[7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 25, 26, 27, 31];
const DA_FAIXA: &[usize] = &[0, 1, 4, 6, 8, 19, 20, 22];

#[derive(Debug, Clone)]
struct Celula {
    valores: Vec<f64>,
    /// O orçamento a cada segundo da janela, em kbps (zero é parado).
    orcamento_por_segundo: Vec<u32>,
    /// O mesmo, nos segundos de aquecimento antes da janela.
    orcamento_no_aquecimento: Vec<u32>,
    motivos: Vec<&'static str>,
    tentativas: u32,
    /// O que não cabe em coluna: o que ajuda a entender uma célula estranha.
    nota: String,
}

fn percentil(ordenado: &[f64], p: f64) -> f64 {
    if ordenado.is_empty() {
        return f64::NAN;
    }
    let posicao = ((p * ordenado.len() as f64).ceil() as usize).clamp(1, ordenado.len());
    ordenado[posicao - 1]
}

fn mediana(mut valores: Vec<f64>) -> f64 {
    valores.retain(|v| !v.is_nan());
    valores.sort_by(f64::total_cmp);
    if valores.is_empty() {
        f64::NAN
    } else {
        valores[(valores.len() - 1) / 2]
    }
}

fn minimo(valores: &[f64]) -> f64 {
    valores
        .iter()
        .copied()
        .filter(|v| !v.is_nan())
        .fold(f64::NAN, f64::min)
}

fn maximo(valores: &[f64]) -> f64 {
    valores
        .iter()
        .copied()
        .filter(|v| !v.is_nan())
        .fold(f64::NAN, f64::max)
}

fn diferenca(depois: ContagemDoSentido, antes: ContagemDoSentido) -> ContagemDoSentido {
    ContagemDoSentido {
        repassados: depois.repassados - antes.repassados,
        bytes_repassados: depois.bytes_repassados - antes.bytes_repassados,
        perdidos_no_sorteio: depois.perdidos_no_sorteio - antes.perdidos_no_sorteio,
        descartados_na_fila: depois.descartados_na_fila - antes.descartados_na_fila,
    }
}

/// O sinal que o servidor devolveu a esta pessoa: o do `PersonState` dela.
fn sinal_do_servidor(conexao: &Connection) -> (u8, u8, Option<f32>, Option<f32>) {
    let retrato = conexao.snapshot();
    let do_servidor = retrato
        .presentes
        .iter()
        .find(|pessoa| pessoa.is_self)
        .map_or(u8::MAX, |pessoa| pessoa.signal);
    (
        do_servidor,
        retrato.telemetry.signal,
        retrato.telemetry.rtt_ms,
        retrato.telemetry.loss_fraction,
    )
}

/// As sete primeiras colunas: o que quem assiste viu na janela.
fn visto_por_quem_assiste(
    chegadas: &[Chegada],
    comeco_ns: u64,
    fim_ns: u64,
    segundos: f64,
) -> [f64; 7] {
    let na_janela: Vec<Chegada> = chegadas
        .iter()
        .copied()
        .filter(|c| c.chegou_ns >= comeco_ns && c.chegou_ns <= fim_ns)
        .collect();
    let mut latencias: Vec<f64> = na_janela
        .iter()
        .map(|c| c.chegou_ns.saturating_sub(c.capturado_ns) as f64 / 1e6)
        .collect();
    latencias.sort_by(f64::total_cmp);
    let mut congelado_ns = 0_u64;
    let mut maior_ns = 0_u64;
    let mut anterior = comeco_ns;
    for instante in na_janela
        .iter()
        .map(|c| c.chegou_ns)
        .chain(std::iter::once(fim_ns))
    {
        let intervalo = instante.saturating_sub(anterior);
        if intervalo > CONGELADO_ACIMA_DE.as_nanos() as u64 {
            congelado_ns += intervalo;
        }
        maior_ns = maior_ns.max(intervalo);
        anterior = instante;
    }
    [
        percentil(&latencias, 0.50),
        percentil(&latencias, 0.95),
        percentil(&latencias, 0.99),
        latencias.last().copied().unwrap_or(f64::NAN),
        congelado_ns as f64 * 100.0 / (segundos * 1e9),
        maior_ns as f64 / 1e6,
        na_janela.len() as f64 / segundos,
    ]
}

/// A célula do controle **cru**: o emissor da linha «A» de `tela_sob_perda.rs`
/// (um par que fala o fio à mão, sem o núcleo do cliente: sem balde, sem teto,
/// sem sonda, sem fila de câmera), contra o servidor e quem assiste de verdade.
///
/// Escreve um quadro por vez no fluxo confiável e só captura o seguinte quando
/// o anterior acabou de ser escrito; os quadros do intervalo não são gerados.
/// Sem voz: um par cru não tem o codificador de voz do produto.
async fn celula_crua(cenario: Cenario, semente: u64, opcoes: &Opcoes) -> Result<Celula, String> {
    use seele_proto::control::{ClientMessage, ServerMessage};
    use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};

    let (endereco, daemon) = servidor(opcoes.caminho_do_servidor_bps).await;
    let cano = Cano::abrir(
        endereco,
        ConfigDoCano::simetrico(
            cenario.sentido(Duration::from_millis(opcoes.atraso_ms)),
            semente,
        ),
    )
    .await
    .map_err(|erro| format!("o cano não abriu: {erro}"))?;
    let casa = tempfile::tempdir().map_err(|e| e.to_string())?;
    let olhos_de_quem_assiste = Arc::new(DeQuemAssiste::default());
    let assiste = entrar(
        endereco,
        casa.path(),
        "assiste",
        &format!("assiste-cru-{semente}-{}", agora_ns()),
        Arc::clone(&olhos_de_quem_assiste) as Arc<dyn EventListener>,
    )
    .await
    .map_err(|erro| format!("quem assiste não entrou: {erro}"))?;
    if !sentar(&assiste).await {
        return Err("quem assiste não sentou na sala".into());
    }

    let chave = ed25519_dalek::SigningKey::from_bytes(&[41; 32]);
    let mut par = par_cru::ate_a_prova(cano.endereco(), &chave, "cru").await;
    let sala = loop {
        if let ServerMessage::Session { voice_rooms, .. } =
            par.proximo().await.map_err(|e| e.to_string())?
        {
            break voice_rooms.first().ok_or("o servidor não tem sala")?.id;
        }
    };
    par.mandar(&ClientMessage::EnterVoiceRoom {
        voice_room: sala,
        password: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    par.mandar(&ClientMessage::StartScreenShare)
        .await
        .map_err(|e| e.to_string())?;
    let screen = loop {
        if let ServerMessage::ScreenShareStarted { screen, .. } =
            par.proximo().await.map_err(|e| e.to_string())?
        {
            break screen;
        }
    };
    let par_cru::Par {
        conexao,
        envio: _envio,
        mut recebe,
        ponta: _ponta,
    } = par;
    // O `ctl` é lido até o fim: um par que não lê é cortado.
    tokio::spawn(async move {
        while seele_server::frame::read::<ServerMessage>(&mut recebe)
            .await
            .is_ok()
        {}
    });
    let mut fluxo = conexao.open_uni().await.map_err(|e| e.to_string())?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await
        .map_err(|e| e.to_string())?;
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: seele_proto::screen::SCREEN_HEADER_VERSION,
        screen,
        source: ScreenSource::Monitor,
        codec: ScreenCodec::H264Baseline,
        width: 1280,
        height: 720,
    }
    .encode(&mut abertura)
    .map_err(|e| e.to_string())?;
    fluxo
        .write_all(&abertura)
        .await
        .map_err(|e| e.to_string())?;

    let duracao = Duration::from_secs(opcoes.segundos);
    let comeco = Instant::now();
    let comeco_ns = agora_ns();
    let cano_no_comeco = cano.ida();
    let mut relogio = tokio::time::interval(Duration::from_secs(1) / QUADROS_POR_SEGUNDO);
    relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut gerados = Vec::new();
    let mut bytes_gerados = 0_u64;
    let mut numero = 0_u64;
    while comeco.elapsed() < duracao {
        relogio.tick().await;
        let e_chave = numero == 0;
        numero += 1;
        let bytes = quadro(
            e_chave,
            numero,
            agora_ns(),
            if e_chave { CHAVE_FIXA } else { DELTA_FIXO },
        );
        let tipo = if e_chave {
            seele_core::tela::TipoDeQuadro::Chave
        } else {
            seele_core::tela::TipoDeQuadro::Comum
        };
        let mut fora = vec![tipo.byte()];
        fora.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        fora.extend_from_slice(&bytes);
        // Com prazo: se o crédito segurar a escrita além da drenagem, a célula
        // acaba em vez de ficar presa.
        match tokio::time::timeout(duracao + DRENAGEM, fluxo.write_all(&fora)).await {
            Ok(Ok(())) => {
                gerados.push(numero);
                bytes_gerados += fora.len() as u64;
            }
            _ => break,
        }
    }
    let fim_ns = agora_ns();
    let ida = diferenca(cano.ida(), cano_no_comeco);
    tokio::time::sleep(DRENAGEM).await;
    let chegadas = olhos_de_quem_assiste.chegadas.lock().unwrap().clone();
    drop(fluxo);
    drop(conexao);
    assiste.disconnect();
    daemon.shutdown();
    drop(cano);

    let segundos = duracao.as_secs_f64();
    let visto = visto_por_quem_assiste(&chegadas, comeco_ns, fim_ns, segundos);
    let chegaram: std::collections::HashSet<u64> = chegadas.iter().map(|c| c.numero).collect();
    let nao_chegaram = gerados.iter().filter(|n| !chegaram.contains(n)).count();
    let chegou_ao_cano = (ida.repassados + ida.descartados()) as f64;
    let mut valores = vec![f64::NAN; COLUNAS.len()];
    valores[..7].copy_from_slice(&visto);
    valores[18] = gerados.len() as f64;
    valores[20] = nao_chegaram as f64;
    valores[21] = 1.0;
    valores[23] = ida.bytes_repassados as f64 / 1e6;
    valores[28] = ida.perdidos_no_sorteio as f64 * 100.0 / chegou_ao_cano;
    valores[29] = ida.descartados_na_fila as f64 * 100.0 / chegou_ao_cano;
    valores[30] = bytes_gerados as f64 * 8.0 / segundos / 1000.0;
    Ok(Celula {
        valores,
        orcamento_por_segundo: Vec::new(),
        orcamento_no_aquecimento: Vec::new(),
        motivos: Vec::new(),
        tentativas: 1,
        nota: String::new(),
    })
}

/// Roda uma célula: um servidor, um cano, duas pessoas, uma janela de medida.
async fn celula(
    cenario: Cenario,
    variante: Variante,
    semente: u64,
    opcoes: &Opcoes,
    etiqueta: &str,
) -> Result<Celula, String> {
    if variante == Variante::Cru {
        return celula_crua(cenario, semente, opcoes).await;
    }
    let (endereco, daemon) = servidor(opcoes.caminho_do_servidor_bps).await;
    let cano = Cano::abrir(
        endereco,
        ConfigDoCano::simetrico(
            cenario.sentido(Duration::from_millis(opcoes.atraso_ms)),
            semente,
        ),
    )
    .await
    .map_err(|erro| format!("o cano não abriu: {erro}"))?;
    let casas = (
        tempfile::tempdir().map_err(|e| e.to_string())?,
        tempfile::tempdir().map_err(|e| e.to_string())?,
    );
    let aparelho_de_quem_assiste = format!("assiste-{etiqueta}");
    let aparelho_de_quem_transmite = format!("transmite-{etiqueta}");

    let olhos_de_quem_assiste = Arc::new(DeQuemAssiste::default());
    let assiste = entrar(
        endereco,
        casas.0.path(),
        "assiste",
        &aparelho_de_quem_assiste,
        Arc::clone(&olhos_de_quem_assiste) as Arc<dyn EventListener>,
    )
    .await
    .map_err(|erro| format!("quem assiste não entrou: {erro}"))?;
    let olhos = Arc::new(DeQuemTransmite::default());
    // Quem transmite disca o cano: o aperto inteiro passa pela perda também.
    let transmite = entrar(
        cano.endereco(),
        casas.1.path(),
        "transmite",
        &aparelho_de_quem_transmite,
        Arc::clone(&olhos) as Arc<dyn EventListener>,
    )
    .await
    .map_err(|erro| format!("quem transmite não entrou pelo cano: {erro}"))?;

    if !sentar(&assiste).await || !sentar(&transmite).await {
        return Err("alguém não sentou na sala".into());
    }
    // A voz de quem transmite fica aberta, como numa conversa; quem assiste
    // fica calado (aperte-para-falar, sem apertar), para a voz ir num sentido
    // só, como na medida do canal.
    transmite.set_voice_mode(VoiceMode::Open);
    // A voz e o `PersonState` andam alguns segundos antes da câmera, como
    // quando alguém já está na conversa e então liga a câmera.
    tokio::time::sleep(Duration::from_secs(3)).await;

    transmite
        .compartilhar_camera(
            1280,
            720,
            // Os de `LIMITES_DA_CAMERA` em `apps/seele-app/ui/camera.js`.
            LimitesDeTela {
                banda_bps: opcoes.escolha_bps,
                altura_maxima: 720,
                quadros_maximos: 30,
                prioridade: seele_core::retrato::Prioridade::Movimento,
                com_som: false,
            },
        )
        .map_err(|erro| format!("o pedido da câmera não saiu: {erro:?}"))?;
    if !ate(PRAZO, || {
        olhos.aberturas.load(Ordering::SeqCst) > 0 && olhos.orcamento_bps.load(Ordering::SeqCst) > 0
    })
    .await
    {
        return Err("a câmera não abriu do lado de quem transmite".into());
    }

    // ------------------------------------------------------------ a janela
    // O codificador roda o aquecimento e a janela; só a janela é contada.
    let duracao = Duration::from_secs(opcoes.segundos);
    let aquecimento = Duration::from_secs(opcoes.aquecimento);
    let inicio = Instant::now();
    let comeco = inicio + aquecimento;
    let fim = comeco + duracao;

    // O codificador de mentira.
    let codificando = {
        let olhos = Arc::clone(&olhos);
        let transmite = Arc::clone(&transmite);
        let opcoes = opcoes.clone();
        tokio::spawn(async move {
            let mut contado = Codificado::default();
            let mut relogio = tokio::time::interval(Duration::from_secs(1) / QUADROS_POR_SEGUNDO);
            relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut numero = 0_u64;
            let mut configurado = 0_u32;
            let mut pular = 0_u32;
            // A contrapressão da casca (`camera.js`, `filaDoNucleoCheia`): com a
            // fila do núcleo pela metade ela não codifica, a não ser um quadro
            // a cada 100 ms, que é como fica sabendo que a fila andou.
            let mut na_fila = 0_u32;
            let mut na_fila_em = Instant::now();
            while Instant::now() < fim {
                relogio.tick().await;
                let na_janela = Instant::now() >= comeco;
                if variante == Variante::Produto
                    && opcoes.contrapressao
                    && na_fila >= LIMIAR_DA_FILA_DA_CAMERA
                    && na_fila_em.elapsed() < SONDA_DA_FILA_DA_CAMERA
                {
                    contado.pulados += u64::from(na_janela);
                    continue;
                }
                if olhos.parada.load(Ordering::SeqCst) {
                    contado.parados += u64::from(na_janela);
                    continue;
                }
                let orcamento = olhos.orcamento_bps.load(Ordering::SeqCst);
                if variante == Variante::Produto && orcamento != configurado {
                    // A casca reconfigura o codificador, e o primeiro quadro de
                    // um codificador reconfigurado é chave.
                    if configurado != 0 && opcoes.chave_na_troca {
                        olhos.quer_chave.store(true, Ordering::SeqCst);
                    }
                    configurado = orcamento;
                }
                let chave = olhos.quer_chave.load(Ordering::SeqCst);
                if variante == Variante::Produto && pular > 0 && !chave {
                    pular -= 1;
                    contado.pulados += u64::from(na_janela);
                    continue;
                }
                let tamanho = match variante {
                    Variante::Fixo | Variante::Cru if chave => CHAVE_FIXA,
                    Variante::Fixo | Variante::Cru => DELTA_FIXO,
                    Variante::Produto => {
                        let mira = (f64::from(orcamento) * opcoes.fracao)
                            .floor()
                            .max(PISO_DO_CODIFICADOR_BPS);
                        let delta = (mira / f64::from(QUADROS_POR_SEGUNDO) / 8.0) as usize;
                        if chave {
                            delta * opcoes.fator_da_chave
                        } else {
                            delta
                        }
                    }
                };
                numero += 1;
                let bytes = quadro(chave, numero, agora_ns(), tamanho);
                let tamanho = bytes.len() as u64;
                match transmite.quadro_da_camera(chave, bytes) {
                    Ok(esperando) => {
                        na_fila = esperando;
                        na_fila_em = Instant::now();
                        if na_janela {
                            contado.aceitos.push(numero);
                            contado.bytes += tamanho;
                            contado.chaves += u64::from(chave);
                        }
                        if chave {
                            // Só baixa o pedido que este quadro atendeu: um
                            // pedido que chegou depois continua de pé.
                            olhos.quer_chave.store(false, Ordering::SeqCst);
                            pular = QUADROS_APOS_A_CHAVE;
                        }
                    }
                    // O quadro que não foi era referência do seguinte.
                    Err(ConnectionError::CameraQueueFull { .. }) => {
                        contado.recusados += u64::from(na_janela);
                        // `--recusa-sem-chave`: como se o quadro recusado não
                        // tivesse sido codificado (o que a bomba da tela do
                        // sistema faz: ela espera em vez de codificar).
                        if !opcoes.recusa_sem_chave {
                            olhos.quer_chave.store(true, Ordering::SeqCst);
                        }
                    }
                    Err(_) => {
                        contado.sem_camera += u64::from(na_janela);
                        olhos.quer_chave.store(true, Ordering::SeqCst);
                    }
                }
            }
            contado
        })
    };

    // As amostras do controle de taxa.
    let mut do_aquecimento = Vec::new();
    while Instant::now() < comeco {
        tokio::time::sleep(AMOSTRA.min(comeco.saturating_duration_since(Instant::now()))).await;
        do_aquecimento.push(if olhos.parada.load(Ordering::SeqCst) {
            0.0
        } else {
            f64::from(olhos.orcamento_bps.load(Ordering::SeqCst)) / 1000.0
        });
    }
    let comeco_ns = agora_ns();
    let cano_no_comeco = (cano.ida(), cano.volta());
    let voz_no_comeco = mentira::contagem(&aparelho_de_quem_assiste).blocos_com_som;
    let avisos_no_comeco = olhos.orcamentos.load(Ordering::SeqCst);
    let pedidos_no_comeco = olhos.pedidos_de_chave.load(Ordering::SeqCst);
    let pausas_no_comeco = olhos.pausas.load(Ordering::SeqCst);
    let mut amostras = Vec::new();
    while Instant::now() < fim {
        tokio::time::sleep(AMOSTRA).await;
        let (sinal_do_servidor, sinal_local, rtt_ms, perda) = sinal_do_servidor(&transmite);
        amostras.push(Amostra {
            orcamento_bps: olhos.orcamento_bps.load(Ordering::SeqCst),
            parada: olhos.parada.load(Ordering::SeqCst),
            sinal_do_servidor,
            sinal_local,
            rtt_ms,
            perda,
        });
    }
    let contado = codificando.await.expect("o codificador de mentira caiu");
    let fim_ns = agora_ns();
    let cano_no_fim = (cano.ida(), cano.volta());
    let voz_no_fim = mentira::contagem(&aparelho_de_quem_assiste).blocos_com_som;
    let avisos = olhos.orcamentos.load(Ordering::SeqCst) - avisos_no_comeco;
    let pedidos = olhos.pedidos_de_chave.load(Ordering::SeqCst) - pedidos_no_comeco;
    let pausas = olhos.pausas.load(Ordering::SeqCst) - pausas_no_comeco;
    let motivos = olhos.motivos.lock().unwrap().clone();

    tokio::time::sleep(DRENAGEM).await;
    let chegadas = olhos_de_quem_assiste.chegadas.lock().unwrap().clone();
    let reaberturas = olhos_de_quem_assiste
        .aberturas
        .load(Ordering::SeqCst)
        .saturating_sub(1);

    let _ = transmite.parar_de_compartilhar();
    transmite.disconnect();
    assiste.disconnect();
    daemon.shutdown();
    drop(cano);

    // ------------------------------------------------------------ as contas
    let segundos = duracao.as_secs_f64();
    let visto = visto_por_quem_assiste(&chegadas, comeco_ns, fim_ns, segundos);
    let chegaram: std::collections::HashSet<u64> = chegadas.iter().map(|c| c.numero).collect();
    let nao_chegaram = contado
        .aceitos
        .iter()
        .filter(|numero| !chegaram.contains(numero))
        .count();

    let orcamentos: Vec<f64> = amostras
        .iter()
        .map(|a| {
            if a.parada {
                0.0
            } else {
                f64::from(a.orcamento_bps) / 1000.0
            }
        })
        .collect();
    let do_servidor: Vec<f64> = amostras
        .iter()
        .map(|a| f64::from(a.sinal_do_servidor))
        .collect();
    let locais: Vec<f64> = amostras.iter().map(|a| f64::from(a.sinal_local)).collect();
    let fora_de_nominal = amostras
        .iter()
        .filter(|a| SignalBand::of(a.sinal_do_servidor) != SignalBand::Nominal)
        .count();
    let parado = amostras.iter().filter(|a| a.parada).count();
    let mut rtts: Vec<f64> = amostras
        .iter()
        .filter_map(|a| a.rtt_ms.map(f64::from))
        .collect();
    rtts.sort_by(f64::total_cmp);
    let perdas: Vec<f64> = amostras
        .iter()
        .filter_map(|a| a.perda.map(|p| f64::from(p) * 100.0))
        .collect();
    let ida = diferenca(cano_no_fim.0, cano_no_comeco.0);
    let chegou_ao_cano = (ida.repassados + ida.descartados()) as f64;
    let por_cento = |parte: u64| {
        if chegou_ao_cano > 0.0 {
            parte as f64 * 100.0 / chegou_ao_cano
        } else {
            f64::NAN
        }
    };
    // Dez blocos de 10 ms... cem por segundo: a saída de mentira pede um bloco
    // a cada 10 ms.
    let voz_tocada = (voz_no_fim - voz_no_comeco) as f64 * 100.0 / (segundos * 100.0);
    let amostras_por_segundo = (Duration::from_secs(1).as_millis() / AMOSTRA.as_millis()) as usize;
    let por_segundo = |serie: &[f64]| -> Vec<u32> {
        serie
            .chunks(amostras_por_segundo)
            .map(|pedaco| pedaco.last().copied().unwrap_or(0.0) as u32)
            .collect()
    };
    let orcamento_por_segundo = por_segundo(&orcamentos);
    let orcamento_no_aquecimento = por_segundo(&do_aquecimento);

    // Conferência: o vídeo passou mesmo pelo cano? Se quem transmite tivesse
    // achado um caminho direto, o cano veria só o aperto.
    if ida.bytes_repassados < contado.bytes / 4 && nao_chegaram < contado.aceitos.len() / 2 {
        return Err(format!(
            "o cano viu {} bytes e o codificador entregou {}: o vídeo não passou por ele",
            ida.bytes_repassados, contado.bytes
        ));
    }

    let valores = vec![
        visto[0],
        visto[1],
        visto[2],
        visto[3],
        visto[4],
        visto[5],
        visto[6],
        minimo(&orcamentos),
        mediana(orcamentos.clone()),
        orcamentos.last().copied().unwrap_or(f64::NAN),
        avisos as f64,
        pausas as f64,
        parado as f64 * 100.0 / amostras.len().max(1) as f64,
        minimo(&do_servidor),
        mediana(do_servidor),
        minimo(&locais),
        mediana(locais),
        fora_de_nominal as f64 * 100.0 / amostras.len().max(1) as f64,
        contado.aceitos.len() as f64,
        contado.recusados as f64,
        nao_chegaram as f64,
        contado.chaves as f64,
        pedidos as f64,
        ida.bytes_repassados as f64 / 1e6,
        voz_tocada,
        percentil(&rtts, 0.50),
        rtts.last().copied().unwrap_or(f64::NAN),
        maximo(&perdas),
        por_cento(ida.perdidos_no_sorteio),
        por_cento(ida.descartados_na_fila),
        contado.bytes as f64 * 8.0 / segundos / 1000.0,
        reaberturas as f64,
    ];
    assert_eq!(valores.len(), COLUNAS.len());
    let ultimo = chegadas.iter().map(|c| c.chegou_ns).max();
    let nota = format!(
        "quem assiste: {} quadros na execução inteira, o último {} s depois do começo da janela, \
         {} aberturas, {} fechadas, ilegíveis {:?}; quem transmite: {} aberturas, {} tiques parado, \
         {} pulados depois de chave, {} sem câmera",
        chegadas.len(),
        ultimo.map_or_else(
            || "—".to_owned(),
            |ns| format!("{:.1}", (ns as f64 - comeco_ns as f64) / 1e9)
        ),
        olhos_de_quem_assiste.aberturas.load(Ordering::SeqCst),
        olhos_de_quem_assiste.fechadas.load(Ordering::SeqCst),
        olhos_de_quem_assiste.ilegiveis.lock().unwrap(),
        olhos.aberturas.load(Ordering::SeqCst),
        contado.parados,
        contado.pulados,
        contado.sem_camera,
    );
    Ok(Celula {
        valores,
        orcamento_por_segundo,
        orcamento_no_aquecimento,
        motivos,
        tentativas: 1,
        nota,
    })
}

// ---------------------------------------------------------------------------
// As tabelas
// ---------------------------------------------------------------------------

fn numero(valor: f64, casas: usize) -> String {
    if valor.is_nan() {
        "—".to_owned()
    } else {
        format!("{valor:.casas$}").replace('.', ",")
    }
}

fn cabecalho(prefixo: &[&str], colunas: &[usize]) {
    let nomes: Vec<&str> = prefixo
        .iter()
        .copied()
        .chain(colunas.iter().map(|c| COLUNAS[*c].0))
        .collect();
    println!("| {} |", nomes.join(" | "));
    let alinhamento: Vec<&str> = prefixo
        .iter()
        .map(|_| "---")
        .chain(colunas.iter().map(|_| "--:"))
        .collect();
    println!("|{}|", alinhamento.join("|"));
}

fn linha(prefixo: &[String], colunas: &[usize], valores: &[f64]) {
    let celulas: Vec<String> = prefixo
        .iter()
        .cloned()
        .chain(colunas.iter().map(|c| numero(valores[*c], COLUNAS[*c].1)))
        .collect();
    println!("| {} |", celulas.join(" | "));
}

type Grade = Vec<(Cenario, Variante, Vec<Celula>)>;

fn medianas(celulas: &[Celula]) -> Vec<f64> {
    (0..COLUNAS.len())
        .map(|c| mediana(celulas.iter().map(|celula| celula.valores[c]).collect()))
        .collect()
}

fn tabela_de_medianas(titulo: &str, colunas: &[usize], grade: &Grade) {
    println!("\n### {titulo}\n");
    cabecalho(&["cenário", "codificador"], colunas);
    for (cenario, variante, celulas) in grade {
        if celulas.is_empty() {
            continue;
        }
        linha(
            &[cenario.nome.to_owned(), variante.nome().to_owned()],
            colunas,
            &medianas(celulas),
        );
    }
}

fn tabela_de_faixas(grade: &Grade) {
    println!("\n### Menor e maior valor entre as rodadas\n");
    let nomes: Vec<&str> = ["cenário", "codificador"]
        .into_iter()
        .chain(DA_FAIXA.iter().map(|c| COLUNAS[*c].0))
        .collect();
    println!("| {} |", nomes.join(" | "));
    println!(
        "|---|---|{}|",
        DA_FAIXA.iter().map(|_| "--:").collect::<Vec<_>>().join("|")
    );
    for (cenario, variante, celulas) in grade {
        if celulas.is_empty() {
            continue;
        }
        let faixas: Vec<String> = DA_FAIXA
            .iter()
            .map(|c| {
                let valores: Vec<f64> = celulas.iter().map(|celula| celula.valores[*c]).collect();
                let (menor, maior) = (minimo(&valores), maximo(&valores));
                if menor.is_nan() {
                    "—".to_owned()
                } else {
                    format!(
                        "{}–{}",
                        numero(menor, COLUNAS[*c].1),
                        numero(maior, COLUNAS[*c].1)
                    )
                }
            })
            .collect();
        println!(
            "| {} | {} | {} |",
            cenario.nome,
            variante.nome(),
            faixas.join(" | ")
        );
    }
}

fn tabela_de_rodadas(titulo: &str, colunas: &[usize], grade: &Grade) {
    println!("\n### {titulo}\n");
    cabecalho(&["cenário", "codificador", "rodada"], colunas);
    for (cenario, variante, celulas) in grade {
        for (indice, celula) in celulas.iter().enumerate() {
            linha(
                &[
                    cenario.nome.to_owned(),
                    variante.nome().to_owned(),
                    (indice + 1).to_string(),
                ],
                colunas,
                &celula.valores,
            );
        }
    }
}

fn tabela_do_orcamento(grade: &Grade) {
    println!("\n### O orçamento segundo a segundo (kbps; 0 é parado)\n");
    println!("| cenário | codificador | rodada | orçamento ao fim de cada segundo | paradas |");
    println!("|---|---|--:|---|---|");
    for (cenario, variante, celulas) in grade {
        for (indice, celula) in celulas.iter().enumerate() {
            let texto = |serie: &[u32]| {
                serie
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let mut serie = texto(&celula.orcamento_por_segundo);
            if !celula.orcamento_no_aquecimento.is_empty() {
                serie = format!(
                    "(aquecimento: {}) {serie}",
                    texto(&celula.orcamento_no_aquecimento)
                );
            }
            println!(
                "| {} | {} | {} | {} | {} |",
                cenario.nome,
                variante.nome(),
                indice + 1,
                serie,
                if celula.motivos.is_empty() {
                    "—".to_owned()
                } else {
                    celula.motivos.join(", ")
                }
            );
        }
    }
}

fn carga() -> String {
    std::process::Command::new("uptime")
        .output()
        .ok()
        .and_then(|saida| String::from_utf8(saida.stdout).ok())
        .map_or_else(
            || "(sem uptime)".to_owned(),
            |texto| texto.trim().to_owned(),
        )
}

#[tokio::main]
async fn main() {
    let opcoes = opcoes();
    if std::env::var_os("SEELE_LOG").is_some() {
        // O registro do produto no STDERR, para quem investiga uma célula:
        // `SEELE_LOG=1 RUST_LOG=seele_core=info …`.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }
    let _ = agora_ns();
    println!("opções: {opcoes:?}");
    println!("carga no começo: {}", carga());

    let mut grade: Grade = Vec::new();
    let mut falhas = Vec::new();
    for cenario in CENARIOS {
        if opcoes
            .so
            .as_deref()
            .is_some_and(|so| !cenario.nome.contains(so))
        {
            continue;
        }
        for variante in &opcoes.variantes {
            let mut celulas = Vec::new();
            for rodada in 0..opcoes.rodadas {
                let semente = opcoes.semente + rodada;
                let etiqueta = format!(
                    "{}-{}-{rodada}",
                    cenario.nome.replace([' ', ',', '%'], ""),
                    variante.nome()
                );
                let mut feita = None;
                for tentativa in 1..=3_u32 {
                    match celula(
                        *cenario,
                        *variante,
                        semente,
                        &opcoes,
                        &format!("{etiqueta}-{tentativa}"),
                    )
                    .await
                    {
                        Ok(mut celula) => {
                            celula.tentativas = tentativa;
                            feita = Some(celula);
                            break;
                        }
                        Err(erro) => {
                            eprintln!(
                                "{} / {} / rodada {}: tentativa {tentativa} falhou: {erro}",
                                cenario.nome,
                                variante.nome(),
                                rodada + 1
                            );
                            falhas.push(format!(
                                "{} / {} / rodada {} / tentativa {tentativa}: {erro}",
                                cenario.nome,
                                variante.nome(),
                                rodada + 1
                            ));
                        }
                    }
                }
                if let Some(celula) = feita {
                    eprintln!(
                        "{} / {} / rodada {}: p50 {} ms, {} quadros/s, congelado {}%, orçamento {:?}",
                        cenario.nome,
                        variante.nome(),
                        rodada + 1,
                        numero(celula.valores[0], 0),
                        numero(celula.valores[6], 1),
                        numero(celula.valores[4], 1),
                        celula.orcamento_por_segundo,
                    );
                    if !celula.nota.is_empty() {
                        eprintln!("    {}", celula.nota);
                    }
                    celulas.push(celula);
                }
            }
            grade.push((*cenario, *variante, celulas));
        }
    }

    println!("carga no fim: {}", carga());
    tabela_de_medianas(
        &format!(
            "O que quem assiste vê (mediana de {} rodadas de {} s)",
            opcoes.rodadas, opcoes.segundos
        ),
        DE_QUEM_ASSISTE,
        &grade,
    );
    tabela_de_medianas(
        "O que o controle de taxa fez (mediana das rodadas)",
        DO_CONTROLE,
        &grade,
    );
    tabela_de_faixas(&grade);
    tabela_do_orcamento(&grade);
    tabela_de_rodadas(
        "Todas as rodadas: o que quem assiste vê",
        DE_QUEM_ASSISTE,
        &grade,
    );
    tabela_de_rodadas(
        "Todas as rodadas: o que o controle de taxa fez",
        DO_CONTROLE,
        &grade,
    );
    println!("\n### Tentativas que falharam\n");
    if falhas.is_empty() {
        println!("Nenhuma.");
    } else {
        for falha in &falhas {
            println!("- {falha}");
        }
    }
}
