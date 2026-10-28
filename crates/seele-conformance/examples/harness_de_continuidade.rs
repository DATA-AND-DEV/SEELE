//! **O harness de continuidade da 0.17**: hospedagem e sessão de cliente
//! independentes, medidas por uma hora real, pelos caminhos de verdade da FFI,
//! da voz e da tela — **sem interface e sem microfone físico**.
//!
//! # O que roda
//!
//! - **`--papel emissor`**: uma pessoa que **fala** (a FFI, com o tom do
//!   aparelho de mentira) e **transmite uma tela sintética em H.264** (o
//!   codificador do produto, quadros gerados, nenhuma captura) por um par cru,
//!   como o helper `tela_e_voz_sinteticas`;
//! - **`--papel receptor`**: uma pessoa pela FFI que **fala e ouve** (o mesmo
//!   tom), **assiste a tela** e, aos `--saida-em-s`, faz o ciclo **sair do
//!   servidor inteiro → reconectar → esperar `--espera-s` (12) → reentrar na
//!   voz**, e continua depois;
//! - **`--papel tudo`** (o padrão): um servidor local em memória e os dois
//!   papéis num processo, para a fumaça. Cada papel tem a sua casa e a sua
//!   identidade.
//!
//! Para dois processos, ou duas máquinas, o servidor é um `seeled` à parte e o
//! endereço é explícito (`--servidor`). O padrão é o localhost; fora dele só
//! com `--permitir-lan`, e o log avisa.
//!
//! # O que sai (JSONL na saída padrão, uma linha por evento)
//!
//! Todas as linhas têm `t` (UTC, ISO-8601 com milissegundos) e `evento`:
//! `inicio`, `agregado` (a cada `--tick-s`, 5), `fonte` (uma por fonte de voz,
//! a cada tick), `ciclo` (`passo` = `sair`, `aparelho_fechou`, `reconectado`,
//! `espera_fim`, `reentrou`, `voz_voltou`), `aviso`, `erro` e `fim` (sempre a
//! última, com `motivo` e `ok`).
//!
//! # O que NÃO é
//!
//! - Os contadores são de **continuidade** (blocos com som nos dois sentidos,
//!   maior silêncio entre blocos com som, filas). **Nenhum é latência
//!   física**, e o aparelho de mentira não é o WASAPI nem o CoreAudio.
//! - `tela.recebidos_transporte` conta os eventos `ScreenFrame` que a FFI
//!   entregaria à janela: mede o **transporte**, não o decodificador.
//! - A tela do emissor é gerada e codificada aqui, e escrita no fluxo de tela
//!   por um par cru; **não passa pela captura da FFI** (`compartilhar_tela`
//!   escolhe monitor ou janela de verdade, e não há costura de teste para uma
//!   captura sintética sem mudar o runtime).
//!
//! # O que sai em cada fluxo
//!
//! - **STDOUT**: só o JSONL acima (`inicio`, `agregado`, `ciclo`, `aviso`,
//!   `erro`, `fim`). O `agregado` traz as contagens de continuidade, a rede do
//!   `snapshot` (rtt, jitter, perda) e a tela; **não traz campo por fonte**.
//! - **STDERR**: o log do produto (`tracing`, `seele_ffi=info`). A linha
//!   `diagnóstico da voz: sessao=… | ssrc=… prof_ms=… jitter_ms=… perda=… plc=…
//!   comfort=… late=…` (por fonte) é a que importa, e **só existe** se o build
//!   for de **debug**, com o recurso `aparelho-de-mentira` da `seele-ffi`, e com
//!   `SEELE_AUDIO_DE_MENTIRA=1` no ambiente. **Sem um dos três, NÃO há dados
//!   por fonte** (e o `inicio` diz `diag_pedido: false`). Quem converte a linha
//!   em JSON é o consumidor do stderr.
//!
//! No debug, o próprio harness se relança com a variável (o `unsafe_code` é
//! proibido para pô-la no processo); a feature é do comando:
//!
//! ```text
//! cargo run -p seele-conformance \
//!   --features seele-ffi/aparelho-de-mentira --example harness_de_continuidade -- ...
//! ```

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    reason = "um exemplo de bancada: a linha é o relatório"
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use ed25519_dalek::SigningKey;
use seele_audio::device::mentira;
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{ConnectConfig, Connection, Event, EventListener, VoiceMode};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_video::codec::{Cadencia, CodificaVideo, ConfigDoCodificador, QuadroI420, Resolucao};
use serde_json::{json, Map, Value};
use tokio::sync::watch;

#[path = "../tests/par_cru/mod.rs"]
mod par_cru;

const RESOLUCAO: Resolucao = Resolucao::P720;
const CHAVE_A_CADA: u64 = 30;

// ---------------------------------------------------------------------------
// Saída: JSONL, a hora, os erros.
// ---------------------------------------------------------------------------

/// A hora UTC em ISO-8601 com milissegundos.
fn iso(desde_a_epoca: Duration) -> String {
    let segundos = desde_a_epoca.as_secs();
    let (ano, mes, dia) = civil(i64::try_from(segundos / 86_400).unwrap_or(0));
    let no_dia = segundos % 86_400;
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}.{:03}Z",
        no_dia / 3_600,
        no_dia / 60 % 60,
        no_dia % 60,
        desde_a_epoca.subsec_millis()
    )
}

/// Dias desde 1970-01-01 → (ano, mês, dia), calendário gregoriano.
fn civil(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let mes = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (y_do(yoe, era, mes), mes, dia)
}

fn y_do(yoe: i64, era: i64, mes: u32) -> i64 {
    yoe + era * 400 + i64::from(mes <= 2)
}

fn agora() -> String {
    iso(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default())
}

/// Escreve uma linha JSONL, com `t` e `evento`.
fn linha(evento: &str, mut campos: Map<String, Value>) {
    let mut fora = Map::new();
    fora.insert("t".into(), Value::String(agora()));
    fora.insert("evento".into(), Value::String(evento.into()));
    fora.append(&mut campos);
    println!("{}", Value::Object(fora));
}

fn objeto(valor: &Value) -> Map<String, Value> {
    valor.as_object().cloned().unwrap_or_default()
}

/// As falhas que reprovam o veredito do `fim`.
static FALHAS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// `evento=erro`: sempre inequívoco, e reprova o `fim`.
fn erro(origem: &str, mensagem: &str) {
    linha(
        "erro",
        objeto(&json!({"origem": origem, "mensagem": mensagem})),
    );
    if let Ok(mut falhas) = FALHAS.lock() {
        falhas.push(format!("{origem}: {mensagem}"));
    }
}

fn aviso(mensagem: &str) {
    linha("aviso", objeto(&json!({"mensagem": mensagem})));
}

// ---------------------------------------------------------------------------
// Argumentos.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Papel {
    Emissor,
    Receptor,
    Tudo,
}

#[derive(Clone, Debug)]
struct Args {
    papel: Papel,
    servidor: Option<String>,
    casa: Option<PathBuf>,
    permitir_lan: bool,
    duracao_s: u64,
    saida_em_s: u64,
    espera_s: u64,
    depois_min_s: u64,
    ciclos: u32,
    intervalo_s: u64,
    timeout_s: Option<u64>,
    tick_s: u64,
    sem_tela: bool,
    openh264: Option<PathBuf>,
    silencio_max_ms: u64,
    prazo_som_s: u64,
    prazo_tela_s: u64,
    injetar: Option<String>,
}

const AJUDA: &str = "\
harness_de_continuidade — continuidade da hospedagem e da sessão, por JSONL

  --papel emissor|receptor|tudo   (padrão: tudo, com servidor local em memória)
  --servidor <ip:porta>           o seeled; padrão 127.0.0.1:18401 (emissor e receptor)
  --permitir-lan                  consente com um servidor fora do localhost
  --casa <pasta>                  a casa descartável (padrão: uma temporária)
  --duracao-s <n>                 padrão 3600 (60 min reais)
  --saida-em-s <n>                quando o receptor sai do servidor; padrão 2700 (45 min)
  --espera-s <n>                  entre reconectar e reentrar na voz; padrão 12
  --depois-min-s <n>              quanto tempo continuar depois de reentrar; padrão 600
  --ciclos <n>                    quantos ciclos de saída; padrão 1
  --intervalo-s <n>               entre as saídas, se --ciclos > 1
  --timeout-s <n>                 o timeout global; padrão duração + 300
  --tick-s <n>                    o intervalo do agregado; padrão 5
  --sem-tela                      sem a tela sintética (só voz)
  --openh264 <arquivo|pasta>      a libopenh264 / openh264.dll (ou SEELE_OPENH264)
  --prazo-som-s <n>               o primeiro som em CADA entrada na voz (receptor); padrão 15
  --prazo-tela-s <n>              o primeiro quadro / o progresso da tela depois de cada entrada; padrão 20
  --injetar tela-para|voz-muda|panico   SÓ para as provas negativas
  --silencio-max-ms <n>           o maior silêncio tolerado entre blocos com som; padrão 2000

Por fonte: SEELE_AUDIO_DE_MENTIRA=1, debug e --features seele-ffi/aparelho-de-mentira; a linha sai no STDERR.
Saída: exit 0 só se o `fim` diz ok; 1 reprovado; 2 argumentos; 3 tela
indisponível; 4 timeout; 130 Ctrl-C; 143 SIGTERM.";

impl Args {
    fn ler(mut entrada: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut args = Self {
            papel: Papel::Tudo,
            servidor: None,
            casa: None,
            permitir_lan: false,
            duracao_s: 3_600,
            saida_em_s: 2_700,
            espera_s: 12,
            depois_min_s: 600,
            ciclos: 1,
            intervalo_s: 0,
            timeout_s: None,
            tick_s: 5,
            sem_tela: false,
            openh264: None,
            silencio_max_ms: 2_000,
            prazo_som_s: 15,
            prazo_tela_s: 20,
            injetar: None,
        };
        let mut intervalo_dito = false;
        while let Some(chave) = entrada.next() {
            let mut valor = |nome: &str| {
                entrada
                    .next()
                    .ok_or_else(|| format!("`{nome}` precisa de um valor"))
            };
            let numero = |texto: String, nome: &str| {
                texto
                    .parse::<u64>()
                    .map_err(|_| format!("`{nome}` precisa de um número inteiro, veio `{texto}`"))
            };
            match chave.as_str() {
                "--papel" => {
                    args.papel = match valor("--papel")?.as_str() {
                        "emissor" => Papel::Emissor,
                        "receptor" => Papel::Receptor,
                        "tudo" => Papel::Tudo,
                        outro => return Err(format!("papel desconhecido `{outro}`")),
                    };
                }
                "--servidor" => args.servidor = Some(valor("--servidor")?),
                "--casa" => args.casa = Some(PathBuf::from(valor("--casa")?)),
                "--permitir-lan" => args.permitir_lan = true,
                "--duracao-s" => args.duracao_s = numero(valor("--duracao-s")?, "--duracao-s")?,
                "--saida-em-s" | "--reconectar-aos-s" => {
                    args.saida_em_s = numero(valor("--saida-em-s")?, "--saida-em-s")?
                }
                "--espera-s" => args.espera_s = numero(valor("--espera-s")?, "--espera-s")?,
                "--depois-min-s" => {
                    args.depois_min_s = numero(valor("--depois-min-s")?, "--depois-min-s")?;
                }
                "--ciclos" => {
                    args.ciclos = u32::try_from(numero(valor("--ciclos")?, "--ciclos")?)
                        .map_err(|_| "`--ciclos` grande demais".to_owned())?;
                }
                "--intervalo-s" => {
                    args.intervalo_s = numero(valor("--intervalo-s")?, "--intervalo-s")?;
                    intervalo_dito = true;
                }
                "--timeout-s" => {
                    args.timeout_s = Some(numero(valor("--timeout-s")?, "--timeout-s")?)
                }
                "--tick-s" => args.tick_s = numero(valor("--tick-s")?, "--tick-s")?.max(1),
                "--sem-tela" => args.sem_tela = true,
                "--openh264" => args.openh264 = Some(PathBuf::from(valor("--openh264")?)),
                "--silencio-max-ms" => {
                    args.silencio_max_ms =
                        numero(valor("--silencio-max-ms")?, "--silencio-max-ms")?;
                }
                "--prazo-som-s" => {
                    args.prazo_som_s = numero(valor("--prazo-som-s")?, "--prazo-som-s")?;
                }
                "--prazo-tela-s" => {
                    args.prazo_tela_s = numero(valor("--prazo-tela-s")?, "--prazo-tela-s")?;
                }
                "--injetar" => {
                    // SÓ para as provas negativas: tela-para | voz-muda | panico.
                    let qual = valor("--injetar")?;
                    if !["tela-para", "voz-muda", "panico"].contains(&qual.as_str()) {
                        return Err(format!("`--injetar {qual}` desconhecido"));
                    }
                    args.injetar = Some(qual);
                }
                "--ajuda" | "--help" | "-h" => return Err(String::new()),
                outro => return Err(format!("argumento desconhecido `{outro}`")),
            }
        }
        if args.papel != Papel::Emissor && args.ciclos == 0 {
            return Err("`--ciclos 0`: sem ciclo, o harness não prova a volta".into());
        }
        if !intervalo_dito {
            args.intervalo_s = args.espera_s + args.depois_min_s + 60;
        }
        // O plano tem de caber: a última saída, a espera, a volta e o «depois».
        if args.papel != Papel::Emissor && args.ciclos > 0 {
            let ultima = args.saida_em_s + u64::from(args.ciclos - 1) * args.intervalo_s;
            let precisa = ultima + args.espera_s + args.depois_min_s + 5;
            if precisa > args.duracao_s {
                return Err(format!(
                    "o plano não cabe: a última saída em {ultima} s, mais {} s de espera e {} s de \
                     «depois» pedem {precisa} s, e a duração é {} s",
                    args.espera_s, args.depois_min_s, args.duracao_s
                ));
            }
        }
        Ok(args)
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_s.unwrap_or(self.duracao_s + 300))
    }
}

/// O endereço do servidor e a política de LAN.
fn endereco_permitido(args: &Args, texto: &str) -> Result<SocketAddr, String> {
    let alvo: SocketAddr = texto
        .parse()
        .map_err(|_| format!("`--servidor {texto}` não é ip:porta"))?;
    if !alvo.ip().is_loopback() {
        if !args.permitir_lan {
            return Err(format!(
                "{alvo} fica fora do localhost: diga `--permitir-lan` se é de propósito"
            ));
        }
        aviso(&format!(
            "LAN consentida: o tráfego deste harness vai para {alvo}, fora desta máquina"
        ));
    }
    Ok(alvo)
}

// ---------------------------------------------------------------------------
// A tela sintética (a mesma do helper `tela_e_voz_sinteticas`).
// ---------------------------------------------------------------------------

fn quadro(n: u64) -> QuadroI420 {
    let (largura, altura) = (RESOLUCAO.largura(), RESOLUCAO.altura());
    let passo = usize::try_from(n % 10_000).unwrap_or(0);
    let mut luma = vec![0_u8; largura * altura];
    for linha in 0..altura {
        for coluna in 0..largura {
            let faixa = ((coluna + passo * 4) / 80) % 8;
            luma[linha * largura + coluna] = 40 + u8::try_from(faixa * 25).unwrap_or(0);
        }
    }
    let topo = (passo * 6) % (altura - 120);
    let esquerda = (passo * 2) % (largura - 120);
    for linha in topo..topo + 120 {
        for coluna in esquerda..esquerda + 120 {
            luma[linha * largura + coluna] = 235;
        }
    }
    let (cl, ca) = (largura.div_ceil(2), altura.div_ceil(2));
    let mut u = vec![128_u8; cl * ca];
    let mut v = vec![128_u8; cl * ca];
    for linha in 0..ca {
        for coluna in 0..cl {
            let faixa = ((coluna * 2 + passo * 4) / 80) % 8;
            u[linha * cl + coluna] = 64 + u8::try_from(faixa * 16).unwrap_or(0);
            v[linha * cl + coluna] = 192 - u8::try_from(faixa * 16).unwrap_or(0);
        }
    }
    QuadroI420::novo(largura, altura, luma, u, v).expect("um quadro do padrão")
}

/// O codificador do produto. No macOS, o do sistema; com `--openh264` /
/// `SEELE_OPENH264` (ou, fora do macOS, nas pastas do executável e do `cwd`), o
/// OpenH264. **Sem codificador, devolve o erro: nada finge vídeo.**
fn codificador(args: &Args) -> Result<(Box<dyn CodificaVideo>, &'static str), String> {
    let config = ConfigDoCodificador {
        medidas: None,
        resolucao: RESOLUCAO,
        cadencia: Cadencia::Q30,
        teto_bps: 1_500_000,
    };
    let apontado = args
        .openh264
        .clone()
        .or_else(|| std::env::var_os("SEELE_OPENH264").map(PathBuf::from));
    let pasta_de = |caminho: &Path| {
        if caminho.is_dir() {
            caminho.to_path_buf()
        } else {
            caminho
                .parent()
                .map_or_else(|| caminho.to_path_buf(), Path::to_path_buf)
        }
    };
    #[cfg(target_os = "macos")]
    if apontado.is_none() {
        return Ok((
            Box::new(
                seele_video::codec::macos::Codificador::novo(&config)
                    .map_err(|erro| format!("armar o codificador do sistema: {erro}"))?,
            ),
            "videotoolbox",
        ));
    }
    let mut pastas: Vec<PathBuf> = apontado.iter().map(|c| pasta_de(c)).collect();
    if let Some(perto) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        pastas.push(perto.join(".."));
        pastas.push(perto);
    }
    if let Ok(atual) = std::env::current_dir() {
        pastas.push(atual);
    }
    let biblioteca =
        seele_video::BibliotecaDeVideo::procurar_e_carregar(&pastas).map_err(|erro| {
            format!(
                "o módulo OpenH264 não está nesta máquina ({erro}); passe --openh264 <arquivo> ou \
             SEELE_OPENH264, ou use --sem-tela. Nenhum vídeo é simulado"
            )
        })?;
    Ok((
        Box::new(
            seele_video::codec::Codificador::novo(&biblioteca, config)
                .map_err(|erro| format!("armar o OpenH264: {erro}"))?,
        ),
        "openh264",
    ))
}

// ---------------------------------------------------------------------------
// O estado vivo de um papel, e o medidor.
// ---------------------------------------------------------------------------

/// Conta os `ScreenFrame` que a janela receberia: o TRANSPORTE, não o decoder.
#[derive(Default)]
struct Olhos {
    quadros: AtomicU64,
    fechadas: AtomicU64,
}

impl EventListener for Olhos {
    fn on_event(&self, evento: Event) {
        match evento {
            Event::ScreenFrame { .. } => {
                self.quadros.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenClosed { .. } => {
                self.fechadas.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

struct Vivo {
    papel: &'static str,
    aparelho: String,
    conexao: Mutex<Option<Arc<Connection>>>,
    fase: Mutex<&'static str>,
    ciclos_feitos: AtomicU64,
    olhos: Arc<Olhos>,
    /// Só o emissor: os quadros que o codificador entregou, e os escritos no fio.
    tela_gerados: AtomicU64,
    tela_escritos: AtomicU64,
    tela_ativa: AtomicBool,
    /// O maior silêncio entre blocos com som, na janela do tick e no total.
    maior_silencio_ms: AtomicU64,
    maior_silencio_total_ms: AtomicU64,
    /// Quando a fase de voz começou (a inicial e cada reentrada).
    entrada_voz: Mutex<Instant>,
}

impl Vivo {
    fn novo(papel: &'static str, aparelho: String) -> Arc<Self> {
        Arc::new(Self {
            papel,
            aparelho,
            conexao: Mutex::new(None),
            fase: Mutex::new("conectando"),
            ciclos_feitos: AtomicU64::new(0),
            olhos: Arc::new(Olhos::default()),
            tela_gerados: AtomicU64::new(0),
            tela_escritos: AtomicU64::new(0),
            tela_ativa: AtomicBool::new(false),
            maior_silencio_ms: AtomicU64::new(0),
            maior_silencio_total_ms: AtomicU64::new(0),
            entrada_voz: Mutex::new(Instant::now()),
        })
    }
    fn fase(&self, nova: &'static str) {
        if nova == "voz" {
            if let Ok(mut entrada) = self.entrada_voz.lock() {
                *entrada = Instant::now();
            }
        }
        if let Ok(mut fase) = self.fase.lock() {
            *fase = nova;
        }
    }
    fn na_voz_ha(&self) -> Duration {
        self.entrada_voz
            .lock()
            .map_or(Duration::ZERO, |e| e.elapsed())
    }
    fn fase_atual(&self) -> &'static str {
        self.fase.lock().map_or("?", |f| *f)
    }
    fn conexao(&self) -> Option<Arc<Connection>> {
        self.conexao.lock().ok().and_then(|c| c.clone())
    }
    fn por(&self, conexao: Option<Arc<Connection>>) {
        if let Ok(mut c) = self.conexao.lock() {
            *c = conexao;
        }
    }
}

/// O medidor: a cada 250 ms o maior silêncio; a cada tick o agregado e as fontes.
async fn medidor(
    vivo: Arc<Vivo>,
    args: Arc<Args>,
    inicio: Instant,
    mut parar: watch::Receiver<bool>,
) {
    let mut amostra = tokio::time::interval(Duration::from_millis(250));
    let mut tick = tokio::time::interval(Duration::from_secs(args.tick_s));
    tick.tick().await;
    let mut ultimo_som = mentira::contagem(&vivo.aparelho).blocos_com_som;
    let mut ultimo_aumento = Instant::now();
    // Armado no primeiro som de CADA entrada na voz (o prazo do primeiro som é
    // outro gate): só então o silêncio entre blocos é continuidade quebrada.
    let mut armado = false;
    let (mut enviada_antes, mut recebida_antes, mut tela_antes) = (0_u64, 0_u64, 0_u64);
    let mut erro_de_sessao_dito = None;
    loop {
        tokio::select! {
            _ = amostra.tick() => {
                let som = mentira::contagem(&vivo.aparelho).blocos_com_som;
                let agora = Instant::now();
                if vivo.fase_atual() != "voz" {
                    armado = false;
                    // Fora da voz o silêncio é esperado: não conta, e rearma.
                    ultimo_aumento = agora;
                } else if som > ultimo_som {
                    armado = true;
                    ultimo_aumento = agora;
                } else if armado {
                    let parado = u64::try_from(agora.duration_since(ultimo_aumento).as_millis())
                        .unwrap_or(u64::MAX);
                    vivo.maior_silencio_ms.fetch_max(parado, Ordering::SeqCst);
                    vivo.maior_silencio_total_ms.fetch_max(parado, Ordering::SeqCst);
                }
                ultimo_som = som;
            }
            _ = tick.tick() => {
                let contagem = mentira::contagem(&vivo.aparelho);
                let conexao = vivo.conexao();
                let carimbo = conexao.as_ref().map(|c| c.carimbo());
                let mut campos = objeto(&json!({
                    "papel": vivo.papel,
                    "decorrido_s": inicio.elapsed().as_secs(),
                    "fase": vivo.fase_atual(),
                    "ciclos_feitos": vivo.ciclos_feitos.load(Ordering::SeqCst),
                    "sessao": carimbo.map(|c| c.sessao),
                    "geracao": carimbo.map(|c| c.geracao),
                    "continuidade": {
                        "nota": "blocos com som contados; NAO e latencia fisica",
                        "voz_enviada_blocos": contagem.blocos,
                        "voz_enviada_delta": contagem.blocos - enviada_antes,
                        "voz_recebida_blocos_com_som": contagem.blocos_com_som,
                        "voz_recebida_delta": contagem.blocos_com_som - recebida_antes,
                        "maior_silencio_ms": vivo.maior_silencio_ms.swap(0, Ordering::SeqCst),
                        "aparelhos_abertos": contagem.abertos,
                    },
                }));
                enviada_antes = contagem.blocos;
                recebida_antes = contagem.blocos_com_som;
                let tela_para_o_gate;
                if vivo.papel == "emissor" {
                    let escritos = vivo.tela_escritos.load(Ordering::SeqCst);
                    campos.insert("tela".into(), json!({
                        "ativa": vivo.tela_ativa.load(Ordering::SeqCst),
                        "gerados_pelo_codificador": vivo.tela_gerados.load(Ordering::SeqCst),
                        "escritos_no_fio": escritos,
                        "escritos_delta": escritos - tela_antes,
                        "nota": "codificador do produto + fluxo cru; nao e a captura da FFI",
                    }));
                    tela_para_o_gate = (vivo.tela_ativa.load(Ordering::SeqCst), escritos - tela_antes);
                    tela_antes = escritos;
                } else {
                    let recebidos = vivo.olhos.quadros.load(Ordering::SeqCst);
                    campos.insert("tela".into(), json!({
                        "recebidos_transporte": recebidos,
                        "recebidos_delta": recebidos - tela_antes,
                        "fechadas": vivo.olhos.fechadas.load(Ordering::SeqCst),
                        "nota": "mede o TRANSPORTE (eventos ScreenFrame); nao o decoder",
                    }));
                    tela_para_o_gate = (!args.sem_tela, recebidos - tela_antes);
                    tela_antes = recebidos;
                }
                // O progresso por janela: durante a voz, depois do prazo de
                // arranque de CADA entrada, os quadros têm de subir.
                let (esperada, delta) = tela_para_o_gate;
                if esperada
                    && vivo.fase_atual() == "voz"
                    && vivo.na_voz_ha() > Duration::from_secs(args.prazo_tela_s + args.tick_s)
                    && delta == 0
                {
                    erro(
                        &format!("{}.tela", vivo.papel),
                        "a tela não progrediu na janela do tick (arranque ou depois da reentrada)",
                    );
                }
                if let Some(conexao) = &conexao {
                    let retrato = conexao.snapshot();
                    campos.insert("rede".into(), json!({
                        "rtt_ms": retrato.telemetry.rtt_ms,
                        "jitter_ms": retrato.telemetry.jitter_ms,
                        "loss_fraction": retrato.telemetry.loss_fraction,
                        "bitrate_bps": retrato.telemetry.bitrate_bps,
                    }));
                    if retrato.ended.is_some() && erro_de_sessao_dito != carimbo.map(|c| c.sessao) {
                        erro_de_sessao_dito = carimbo.map(|c| c.sessao);
                        erro(vivo.papel, "a sessão acabou sozinha (snapshot.ended)");
                    }
                }
                let fontes: Vec<Map<String, Value>> = Vec::new();
                linha("agregado", campos);
                for fonte in fontes {
                    let mut c = objeto(&json!({
                        "papel": vivo.papel,
                        "sessao": carimbo.map(|c| c.sessao),
                    }));
                    c.extend(fonte);
                    linha("fonte", c);
                }
            }
            _ = parar.changed() => return,
        }
    }
}

// ---------------------------------------------------------------------------
// Conectar, sentar.
// ---------------------------------------------------------------------------

async fn conectar(
    alvo: SocketAddr,
    casa: &Path,
    apelido: &str,
    aparelho: &str,
    olhos: Option<Arc<Olhos>>,
    foco: Arc<FocoDeVoz>,
) -> Result<Arc<Connection>, String> {
    std::fs::create_dir_all(casa).map_err(|e| format!("a casa {}: {e}", casa.display()))?;
    let config = ConnectConfig {
        server: alvo.to_string(),
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
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .map_err(|e| format!("a thread que conecta caiu: {e}"))?
    .map_err(|falha| format!("{apelido} não entrou: {falha:?}"))?;
    if let Some(olhos) = olhos {
        conexao.subscribe(olhos as Arc<dyn EventListener>);
    }
    conexao.set_voice_mode(VoiceMode::Open);
    // Em CADA conexão (inclusive nas reconexões), antes de entrar na voz: o
    // RNNoise suprimiria o tom. O DSP NÃO é validado aqui.
    conexao.set_supressao_de_ruido(0.0);
    Ok(conexao)
}

/// Sentar como a janela senta: sem o foco, foca antes.
async fn sentar(foco: &Arc<FocoDeVoz>, conexao: &Arc<Connection>) -> Result<(), String> {
    if !conexao.tem_o_foco_de_voz() {
        let alvo = conexao.id_no_foco();
        let foco = Arc::clone(foco);
        tokio::task::spawn_blocking(move || foco.focar(alvo))
            .await
            .map_err(|e| format!("a troca do foco caiu: {e}"))?
            .map_err(|e| format!("a troca do foco foi recusada: {e:?}"))?;
    }
    let sala = conexao
        .snapshot()
        .voice_rooms
        .first()
        .map(|s| s.id)
        .ok_or("o servidor não tem sala de voz")?;
    conexao
        .enter_voice_room(sala, None)
        .map_err(|e| format!("não sentou: {e:?}"))
}

/// O primeiro som, num prazo finito, contado desde que a fase de voz começou.
async fn primeiro_som(vivo: &Arc<Vivo>, args: &Args, qual: &str, antes: u64) -> Option<u64> {
    let resta = Duration::from_secs(args.prazo_som_s).saturating_sub(vivo.na_voz_ha());
    let aparelho = vivo.aparelho.clone();
    let chegou = ate(resta, || {
        mentira::contagem(&aparelho).blocos_com_som > antes
    })
    .await;
    if chegou.is_none() {
        erro(
            vivo.papel,
            &format!(
                "entrada {qual}: nenhum bloco com som no prazo de {} s",
                args.prazo_som_s
            ),
        );
    }
    chegou.map(|_| ms(vivo.na_voz_ha()))
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> Option<Duration> {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return Some(comeco.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    pronto().then(|| comeco.elapsed())
}

async fn dorme_ou_para(prazo: Duration, parar: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        () = tokio::time::sleep(prazo) => false,
        _ = parar.changed() => true,
    }
}

fn ms(duracao: Duration) -> u64 {
    u64::try_from(duracao.as_millis()).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// O emissor.
// ---------------------------------------------------------------------------

async fn emissor(
    args: Arc<Args>,
    alvo: SocketAddr,
    casa: PathBuf,
    inicio: Instant,
    mut parar: watch::Receiver<bool>,
) -> Value {
    let vivo = Vivo::novo("emissor", "harness-emissor".into());
    let medindo = tokio::spawn(medidor(
        Arc::clone(&vivo),
        Arc::clone(&args),
        inicio,
        parar.clone(),
    ));
    let foco = FocoDeVoz::novo();
    let fala = match conectar(
        alvo,
        &casa.join("emissor"),
        "harness-fala",
        "harness-emissor",
        None,
        Arc::clone(&foco),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            erro("emissor", &e);
            return json!({"papel": "emissor", "entrou": false});
        }
    };
    vivo.por(Some(Arc::clone(&fala)));
    if let Err(e) = sentar(&foco, &fala).await {
        erro("emissor", &e);
    }
    vivo.fase("voz");

    // A tela sintética, pelo par cru.
    let parar_tela = Arc::new(AtomicBool::new(false));
    let mut tarefas_da_tela = Vec::new();
    let mut tela_pedida = !args.sem_tela;
    if tela_pedida {
        match codificador(&args) {
            Err(e) => {
                erro("emissor.tela", &e);
                tela_pedida = false;
            }
            Ok((mut cod, qual)) => {
                let chave = SigningKey::from_bytes(&identidade_da_tela(&casa));
                let mut par = par_cru::ate_a_prova(alvo, &chave, "harness-tela").await;
                let sala = match par.proximo().await {
                    Ok(ServerMessage::Session { voice_rooms, .. }) => {
                        voice_rooms.first().map(|s| s.id)
                    }
                    _ => None,
                };
                if let Some(sala) = sala {
                    let _ = par
                        .mandar(&ClientMessage::EnterVoiceRoom {
                            voice_room: sala,
                            password: None,
                        })
                        .await;
                    let _ = par.mandar(&ClientMessage::StartScreenShare).await;
                    let tela = loop {
                        match par.proximo().await {
                            Ok(ServerMessage::ScreenShareStarted { screen, .. }) => {
                                break Some(screen)
                            }
                            Ok(_) => {}
                            Err(_) => break None,
                        }
                    };
                    if let Some(screen) = tela {
                        let mut fluxo = par.conexao.open_uni().await.expect("o fluxo da tela");
                        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
                        fluxo
                            .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
                            .await
                            .expect("tipo do fluxo");
                        ScreenHeader {
                            version: seele_proto::PROTOCOL_VERSION,
                            screen,
                            source: ScreenSource::Monitor,
                            codec: ScreenCodec::H264Baseline,
                            width: u16::try_from(RESOLUCAO.largura()).unwrap(),
                            height: u16::try_from(RESOLUCAO.altura()).unwrap(),
                        }
                        .encode(&mut abertura)
                        .expect("cabeçalho");
                        fluxo.write_all(&abertura).await.expect("cabeçalho");
                        vivo.tela_ativa.store(true, Ordering::SeqCst);
                        linha(
                            "aviso",
                            objeto(
                                &json!({"mensagem": "tela sintética transmitindo", "codificador": qual, "tela": screen.0}),
                            ),
                        );

                        let (tx, mut rx) = tokio::sync::mpsc::channel::<(bool, Vec<u8>)>(8);
                        {
                            let vivo = Arc::clone(&vivo);
                            let parar_tela = Arc::clone(&parar_tela);
                            let parar_a_tela_depois_de_1 =
                                args.injetar.as_deref() == Some("tela-para");
                            std::thread::spawn(move || {
                                let mut n: u64 = 0;
                                let mut pendente = true;
                                while !parar_tela.load(Ordering::SeqCst) {
                                    if parar_a_tela_depois_de_1
                                        && vivo.tela_gerados.load(Ordering::SeqCst) >= 1
                                    {
                                        std::thread::sleep(Duration::from_millis(33));
                                        continue;
                                    }
                                    let pede = pendente || n.is_multiple_of(CHAVE_A_CADA);
                                    match cod.codificar(&quadro(n), pede) {
                                        Ok(Some(saiu)) => {
                                            pendente = false;
                                            vivo.tela_gerados.fetch_add(1, Ordering::SeqCst);
                                            if tx.blocking_send((saiu.chave, saiu.bytes)).is_err() {
                                                return;
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(e) => erro(
                                            "emissor.tela",
                                            &format!("codificar o quadro {n}: {e}"),
                                        ),
                                    }
                                    n += 1;
                                    std::thread::sleep(Duration::from_millis(33));
                                }
                            });
                        }
                        let vivo_w = Arc::clone(&vivo);
                        let parar_w = Arc::clone(&parar_tela);
                        tarefas_da_tela.push(tokio::spawn(async move {
                            while let Some((chave, bytes)) = rx.recv().await {
                                let tipo = if chave {
                                    seele_core::tela::TipoDeQuadro::Chave
                                } else {
                                    seele_core::tela::TipoDeQuadro::Comum
                                };
                                let mut fora = vec![tipo.byte()];
                                fora.extend_from_slice(
                                    &u32::try_from(bytes.len()).unwrap().to_be_bytes(),
                                );
                                fora.extend_from_slice(&bytes);
                                if fluxo.write_all(&fora).await.is_err() {
                                    if !parar_w.load(Ordering::SeqCst) {
                                        erro(
                                            "emissor.tela",
                                            "a escrita da tela falhou: o fluxo acabou",
                                        );
                                    }
                                    return;
                                }
                                vivo_w.tela_escritos.fetch_add(1, Ordering::SeqCst);
                            }
                            let _ = fluxo.finish();
                        }));
                        // O par fica drenando as mensagens do servidor até o fim.
                        let parar_p = Arc::clone(&parar_tela);
                        let mut parar_rx = parar.clone();
                        tarefas_da_tela.push(tokio::spawn(async move {
                            loop {
                                tokio::select! {
                                    visto = par.proximo() => if visto.is_err() {
                                        if !parar_p.load(Ordering::SeqCst) {
                                            erro("emissor.tela", "o par da tela perdeu o servidor");
                                        }
                                        return;
                                    },
                                    _ = parar_rx.changed() => break,
                                }
                            }
                            let _ = par.mandar(&ClientMessage::StopScreenShare).await;
                        }));
                    } else {
                        erro("emissor.tela", "o servidor não iniciou a transmissão");
                        tela_pedida = false;
                    }
                } else {
                    erro("emissor.tela", "o par da tela não recebeu a Session");
                    tela_pedida = false;
                }
            }
        }
    }

    // Segura até a duração ou o sinal.
    let _ = dorme_ou_para(
        Duration::from_secs(args.duracao_s).saturating_sub(inicio.elapsed()),
        &mut parar,
    )
    .await;

    // Limpeza. A partir daqui nada é esperado: os gates do medidor param.
    vivo.fase("fim");
    parar_tela.store(true, Ordering::SeqCst);
    let contagem = mentira::contagem("harness-emissor");
    let resumo = json!({
        "papel": "emissor",
        "voz_enviada_blocos": contagem.blocos,
        "voz_recebida_blocos_com_som": contagem.blocos_com_som,
        "maior_silencio_ms": vivo.maior_silencio_total_ms.load(Ordering::SeqCst),
        "tela_pedida": tela_pedida,
        "tela_gerados": vivo.tela_gerados.load(Ordering::SeqCst),
        "tela_escritos_no_fio": vivo.tela_escritos.load(Ordering::SeqCst),
    });
    if contagem.blocos == 0 {
        erro("emissor", "nenhum bloco de voz foi enviado");
    }
    if tela_pedida && vivo.tela_escritos.load(Ordering::SeqCst) == 0 {
        erro(
            "emissor.tela",
            "a tela foi pedida e nenhum quadro foi escrito",
        );
    }
    // O silêncio do emissor NÃO reprova: enquanto o receptor está fora do
    // servidor (o ciclo), não há quem fale com ele. Só o receptor tem o limite.
    fala.disconnect();
    for t in tarefas_da_tela {
        let _ = tokio::time::timeout(Duration::from_secs(3), t).await;
    }
    medindo.abort();
    resumo
}

/// A chave do par da tela: a mesma a cada rodada com a mesma casa, distinta da
/// do receptor (que tem a dele dentro da casa da FFI).
fn identidade_da_tela(casa: &Path) -> [u8; 32] {
    let arquivo = casa.join("emissor").join("tela.chave");
    if let Ok(bytes) = std::fs::read(&arquivo) {
        if let Ok(chave) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return chave;
        }
    }
    let chave: [u8; 32] = rand::random();
    let _ = std::fs::create_dir_all(casa.join("emissor"));
    let _ = std::fs::write(&arquivo, chave);
    chave
}

// ---------------------------------------------------------------------------
// O receptor, com o ciclo.
// ---------------------------------------------------------------------------

fn ciclo(passo: &str, k: u32, extra: &Value) {
    let mut campos = objeto(&json!({"papel": "receptor", "passo": passo, "ciclo": k}));
    campos.extend(objeto(extra));
    linha("ciclo", campos);
}

async fn receptor(
    args: Arc<Args>,
    alvo: SocketAddr,
    casa: PathBuf,
    inicio: Instant,
    mut parar: watch::Receiver<bool>,
) -> Value {
    const APARELHO: &str = "harness-receptor";
    let vivo = Vivo::novo("receptor", APARELHO.into());
    let medindo = tokio::spawn(medidor(
        Arc::clone(&vivo),
        Arc::clone(&args),
        inicio,
        parar.clone(),
    ));
    let foco = FocoDeVoz::novo();
    let casa = casa.join("receptor");
    let aqui = |olhos: &Arc<Olhos>| {
        conectar(
            alvo,
            &casa,
            "harness-receptor",
            APARELHO,
            Some(Arc::clone(olhos)),
            Arc::clone(&foco),
        )
    };

    let primeira = match aqui(&vivo.olhos).await {
        Ok(c) => c,
        Err(e) => {
            erro("receptor", &e);
            medindo.abort();
            return json!({"papel": "receptor", "entrou": false});
        }
    };
    vivo.por(Some(Arc::clone(&primeira)));
    vivo.fase("voz");
    let som_antes = mentira::contagem(APARELHO).blocos_com_som;
    if args.injetar.as_deref() == Some("voz-muda") {
        // Prova negativa: a voz só abre depois do prazo.
        tokio::time::sleep(Duration::from_secs(args.prazo_som_s + 2)).await;
    }
    if let Err(e) = sentar(&foco, &primeira).await {
        erro("receptor", &e);
    }
    primeiro_som(&vivo, &args, "inicial", som_antes).await;
    if args.injetar.as_deref() == Some("panico") {
        tokio::time::sleep(Duration::from_secs(2)).await;
        panic!("pânico injetado para a prova negativa");
    }

    // Quem assiste: escolhe a primeira transmissão de outra pessoa que apareça.
    let assistindo = {
        let vivo = Arc::clone(&vivo);
        let mut parar = parar.clone();
        let sem_tela = args.sem_tela;
        tokio::spawn(async move {
            loop {
                if sem_tela {
                    return;
                }
                if dorme_ou_para(Duration::from_secs(1), &mut parar).await {
                    return;
                }
                if vivo.fase_atual() != "voz" {
                    continue;
                }
                if let Some(c) = vivo.conexao() {
                    let alvo = c
                        .snapshot()
                        .transmissoes
                        .iter()
                        .find(|t| !t.e_minha && !t.assistida)
                        .map(|t| t.tela);
                    if let Some(tela) = alvo {
                        let _ = c.assistir(tela, true);
                    }
                }
            }
        })
    };

    let mut voltas = Vec::new();
    let mut ultima_reentrada: Option<(Instant, u64)> = None;
    let mut parado = false;
    let mut tela_na_reentrada: Option<u64> = None;
    for k in 1..=args.ciclos {
        let quando = Duration::from_secs(args.saida_em_s + u64::from(k - 1) * args.intervalo_s);
        if dorme_ou_para(quando.saturating_sub(inicio.elapsed()), &mut parar).await {
            parado = true;
            break;
        }
        // 1. Sai do servidor inteiro.
        ciclo(
            "sair",
            k,
            &json!({"decorrido_s": inicio.elapsed().as_secs()}),
        );
        vivo.fase("saindo");
        let velha = vivo.conexao();
        vivo.por(None);
        let t0 = Instant::now();
        if let Some(velha) = velha {
            velha.disconnect();
            drop(velha);
        }
        let fechou = ate(Duration::from_secs(10), || {
            mentira::contagem(APARELHO).abertos == 0
        })
        .await;
        match fechou {
            Some(d) => ciclo("aparelho_fechou", k, &json!({"em_ms": ms(d)})),
            None => erro(
                "receptor",
                "sair não fechou o aparelho da sessão velha em 10 s",
            ),
        }
        vivo.fase("fora");
        // 2. Reconecta.
        let t1 = Instant::now();
        let segunda = match aqui(&vivo.olhos).await {
            Ok(c) => c,
            Err(e) => {
                erro("receptor", &format!("reconectar: {e}"));
                parado = true;
                break;
            }
        };
        vivo.por(Some(Arc::clone(&segunda)));
        ciclo(
            "reconectado",
            k,
            &json!({"em_ms": ms(t1.elapsed()), "fora_ms": ms(t0.elapsed())}),
        );
        // 3. Espera, no servidor e fora da voz.
        vivo.fase("espera");
        if dorme_ou_para(Duration::from_secs(args.espera_s), &mut parar).await {
            parado = true;
            break;
        }
        // O estado ANTES da ação de entrar: a voz tem de estar fechada.
        let antes_abertos = mentira::contagem(APARELHO).abertos;
        ciclo(
            "espera_fim",
            k,
            &json!({
                "espera_s": args.espera_s,
                "aparelhos_abertos": antes_abertos,
                "tem_o_foco_de_voz": segunda.tem_o_foco_de_voz(),
                "nota": "estado antes de entrar na voz: aparelhos abertos tem de ser 0",
            }),
        );
        if antes_abertos != 0 {
            erro(
                "receptor",
                "na espera, a voz não estava fechada (aparelhos abertos)",
            );
        }
        // 4. Reentra na voz.
        let t2 = Instant::now();
        let antes = mentira::contagem(APARELHO).blocos_com_som;
        let tela_antes = vivo.olhos.quadros.load(Ordering::SeqCst);
        vivo.fase("voz");
        if let Err(e) = sentar(&foco, &segunda).await {
            erro("receptor", &format!("reentrar na voz: {e}"));
            parado = true;
            break;
        }
        ciclo(
            "reentrou",
            k,
            &json!({
                "em_ms": ms(t2.elapsed()),
                "aparelhos_abertos": mentira::contagem(APARELHO).abertos,
                "tem_o_foco_de_voz": segunda.tem_o_foco_de_voz(),
            }),
        );
        let voltou = primeiro_som(&vivo, &args, "reentrada", antes).await;
        if let Some(d) = voltou {
            ciclo(
                "voz_voltou",
                k,
                &json!({"em_ms": d, "nota": "primeiro bloco com som depois de reentrar"}),
            );
        }
        tela_na_reentrada = Some(tela_antes);
        vivo.ciclos_feitos.store(u64::from(k), Ordering::SeqCst);
        ultima_reentrada = Some((Instant::now(), mentira::contagem(APARELHO).blocos_com_som));
        voltas.push(json!({
            "ciclo": k,
            "fora_ms": ms(t0.elapsed()),
            "voz_voltou_ms": voltou,
        }));
    }

    if !parado {
        let _ = dorme_ou_para(
            Duration::from_secs(args.duracao_s).saturating_sub(inicio.elapsed()),
            &mut parar,
        )
        .await;
    }

    // O veredito do receptor. A partir daqui nada é esperado do medidor.
    vivo.fase("fim");
    let contagem = mentira::contagem(APARELHO);
    let fim_da_espera = *parar.borrow();
    if !fim_da_espera && !parado {
        if vivo.ciclos_feitos.load(Ordering::SeqCst) < u64::from(args.ciclos) {
            erro("receptor", "o ciclo de saída não terminou");
        }
        if let Some((quando, base)) = ultima_reentrada {
            if quando.elapsed() < Duration::from_secs(args.depois_min_s) {
                erro(
                    "receptor",
                    "o receptor não continuou o tempo mínimo depois de reentrar",
                );
            }
            if contagem.blocos_com_som <= base {
                erro(
                    "receptor",
                    "depois de reentrar, nenhum bloco com som chegou até o fim",
                );
            }
        }
        if let Some(base) = tela_na_reentrada {
            if !args.sem_tela && vivo.olhos.quadros.load(Ordering::SeqCst) <= base {
                erro(
                    "receptor.tela",
                    "depois da reentrada, nenhum quadro novo chegou (transporte)",
                );
            }
        }
        if !args.sem_tela && vivo.olhos.quadros.load(Ordering::SeqCst) == 0 {
            erro(
                "receptor.tela",
                "a tela era esperada e nenhum ScreenFrame chegou (transporte)",
            );
        }
    }
    if vivo.maior_silencio_total_ms.load(Ordering::SeqCst) > args.silencio_max_ms {
        erro(
            "receptor",
            "o maior silêncio entre blocos com som passou do limite",
        );
    }
    let resumo = json!({
        "papel": "receptor",
        "voz_enviada_blocos": contagem.blocos,
        "voz_recebida_blocos_com_som": contagem.blocos_com_som,
        "maior_silencio_ms": vivo.maior_silencio_total_ms.load(Ordering::SeqCst),
        "tela_recebidos_transporte": vivo.olhos.quadros.load(Ordering::SeqCst),
        "ciclos_feitos": vivo.ciclos_feitos.load(Ordering::SeqCst),
        "voltas": voltas,
    });
    assistindo.abort();
    if let Some(c) = vivo.conexao() {
        c.disconnect();
    }
    medindo.abort();
    resumo
}

// ---------------------------------------------------------------------------
// O processo.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Fim {
    Duracao,
    Timeout,
    CtrlC,
    Sigterm,
}

impl Fim {
    fn motivo(self) -> &'static str {
        match self {
            Self::Duracao => "duracao",
            Self::Timeout => "timeout",
            Self::CtrlC => "ctrl_c",
            Self::Sigterm => "sigterm",
        }
    }
}

async fn sinal() -> Fim {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => Fim::CtrlC,
            _ = term.recv() => Fim::Sigterm,
        }
    }
    #[cfg(windows)]
    {
        let mut fechar = tokio::signal::windows::ctrl_close().expect("ctrl_close");
        let mut desligar = tokio::signal::windows::ctrl_shutdown().expect("ctrl_shutdown");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => Fim::CtrlC,
            _ = fechar.recv() => Fim::Sigterm,
            _ = desligar.recv() => Fim::Sigterm,
        }
    }
}

/// No debug, sem a variável, o harness se relança com `SEELE_AUDIO_DE_MENTIRA=1`
/// (`unsafe_code` é proibido: não dá para pô-la neste processo). No Unix é um
/// `exec` (mesmo pid, os sinais continuam valendo); no Windows, um filho que
/// herda a saída e cujo código de saída é devolvido.
fn relancar_com_o_diagnostico() -> Option<std::process::ExitCode> {
    if !cfg!(debug_assertions)
        || std::env::var_os("SEELE_AUDIO_DE_MENTIRA").is_some()
        || std::env::args().any(|a| a == "--ajuda" || a == "--help" || a == "-h")
    {
        return None;
    }
    let mut comando = std::process::Command::new(std::env::current_exe().ok()?);
    comando
        .args(std::env::args().skip(1))
        .env("SEELE_AUDIO_DE_MENTIRA", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = comando.exec();
        None
    }
    #[cfg(not(unix))]
    {
        let estado = comando.status().ok()?;
        Some(std::process::ExitCode::from(
            u8::try_from(estado.code().unwrap_or(1)).unwrap_or(1),
        ))
    }
}

fn main() -> std::process::ExitCode {
    if let Some(saida) = relancar_com_o_diagnostico() {
        return saida;
    }
    let args = match Args::ler(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(mensagem) => {
            if mensagem.is_empty() {
                println!("{AJUDA}");
                return std::process::ExitCode::SUCCESS;
            }
            erro("argumentos", &mensagem);
            linha(
                "fim",
                objeto(&json!({"motivo": "argumentos", "ok": false, "exit": 2})),
            );
            return std::process::ExitCode::from(2);
        }
    };
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn,seele_ffi=info"))
        .try_init();
    let corre = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("o runtime");
    std::process::ExitCode::from(corre.block_on(correr(args)))
}

async fn correr(args: Args) -> u8 {
    let args = Arc::new(args);
    let inicio = Instant::now();
    linha(
        "inicio",
        objeto(&json!({
            "papel": format!("{:?}", args.papel).to_lowercase(),
            "versao_do_formato": 1,
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "so": std::env::consts::OS,
            "duracao_s": args.duracao_s,
            "saida_em_s": args.saida_em_s,
            "espera_s": args.espera_s,
            "depois_min_s": args.depois_min_s,
            "ciclos": args.ciclos,
            "timeout_s": args.timeout().as_secs(),
            "tick_s": args.tick_s,
            "diag_pedido": std::env::var("SEELE_AUDIO_DE_MENTIRA").as_deref() == Ok("1") && cfg!(debug_assertions),
            "aparelho": "mentira (NAO e WASAPI nem CoreAudio)",
            "continuidade": "contadores de blocos e filas; NAO e latencia fisica",
            "supressao_de_ruido": 0.0,
            "dsp": "supressao desligada em cada conexao para o tom servir de referencia; o DSP NAO esta sendo validado",
        })),
    );
    if std::env::var("SEELE_AUDIO_DE_MENTIRA").as_deref() != Ok("1") {
        aviso("sem SEELE_AUDIO_DE_MENTIRA=1 NÃO há dados por fonte (ver o cabeçalho)");
    }

    // O servidor.
    let (alvo, daemon) = if args.papel == Papel::Tudo {
        let daemon = Arc::new(
            match seele_server::Daemon::bind(seele_server::ServerConfig {
                name: "Harness".into(),
                listen: SocketAddr::from(([127, 0, 0, 1], 0)),
                database: seele_server::persistence::Location::Memory,
                caminho_bps: Some(10_000_000),
                ..seele_server::ServerConfig::default()
            })
            .await
            {
                Ok(d) => d,
                Err(e) => return acabar(&args, "erro", inicio, &[], 1, format!("{e}")),
            },
        );
        let alvo = daemon.local_addr().expect("o endereço do servidor local");
        let servindo = Arc::clone(&daemon);
        tokio::spawn(async move {
            let _ = servindo.run().await;
        });
        linha(
            "aviso",
            objeto(&json!({"mensagem": "servidor local em memória", "servidor": alvo.to_string()})),
        );
        (alvo, Some(daemon))
    } else {
        let texto = args
            .servidor
            .clone()
            .unwrap_or_else(|| "127.0.0.1:18401".into());
        match endereco_permitido(&args, &texto) {
            Ok(a) => (a, None),
            Err(e) => {
                erro("servidor", &e);
                return acabar(&args, "argumentos", inicio, &[], 2, String::new());
            }
        }
    };

    // A casa descartável, de cada papel.
    let temporaria = if args.casa.is_none() {
        tempfile::tempdir().ok()
    } else {
        None
    };
    let casa = args
        .casa
        .clone()
        .or_else(|| temporaria.as_ref().map(|t| t.path().to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);

    let (parar_tx, parar_rx) = watch::channel(false);
    let mut papeis = Vec::new();
    if matches!(args.papel, Papel::Emissor | Papel::Tudo) {
        papeis.push(tokio::spawn(emissor(
            Arc::clone(&args),
            alvo,
            casa.clone(),
            inicio,
            parar_rx.clone(),
        )));
    }
    if matches!(args.papel, Papel::Receptor | Papel::Tudo) {
        papeis.push(tokio::spawn(receptor(
            Arc::clone(&args),
            alvo,
            casa.clone(),
            inicio,
            parar_rx.clone(),
        )));
    }

    let todos = async {
        let mut resumos = Vec::new();
        for p in papeis {
            match p.await {
                Ok(resumo) => resumos.push(resumo),
                Err(e) => {
                    // Um pânico ou o cancelamento de uma tarefa REPROVA.
                    erro("tarefa", &format!("uma tarefa caiu: {e}"));
                    resumos.push(json!({"papel": "?", "caiu": e.to_string()}));
                }
            }
        }
        resumos
    };
    tokio::pin!(todos);
    let (fim, resumos) = tokio::select! {
        resumos = &mut todos => (Fim::Duracao, resumos),
        s = sinal() => {
            let _ = parar_tx.send(true);
            (s, (&mut todos).await)
        }
        () = tokio::time::sleep(args.timeout()) => {
            let _ = parar_tx.send(true);
            (Fim::Timeout, tokio::time::timeout(Duration::from_secs(15), &mut todos).await.unwrap_or_default())
        }
    };
    let _ = parar_tx.send(true);
    if let Some(daemon) = daemon {
        daemon.shutdown();
    }
    drop(temporaria);
    let codigo = match fim {
        Fim::Duracao => 0,
        Fim::Timeout => 4,
        Fim::CtrlC => 130,
        Fim::Sigterm => 143,
    };
    acabar(&args, fim.motivo(), inicio, &resumos, codigo, String::new())
}

/// A linha `fim`, sempre a última, e o código de saída.
fn acabar(
    args: &Args,
    motivo: &str,
    inicio: Instant,
    resumos: &[Value],
    codigo: u8,
    detalhe: String,
) -> u8 {
    if !detalhe.is_empty() {
        erro("servidor", &detalhe);
    }
    let falhas = FALHAS.lock().map(|f| f.clone()).unwrap_or_default();
    let tela_faltou = falhas.iter().any(|f| f.starts_with("emissor.tela"));
    // Um sinal ou timeout interrompe o plano: é um fim limpo, mas não aprova.
    let ok = falhas.is_empty() && codigo == 0;
    let saida = if codigo != 0 {
        codigo
    } else if ok {
        0
    } else if tela_faltou {
        3
    } else {
        1
    };
    linha(
        "fim",
        objeto(&json!({
            "motivo": motivo,
            "ok": ok,
            "exit": saida,
            "decorrido_s": inicio.elapsed().as_secs(),
            "falhas": falhas,
            "papel": format!("{:?}", args.papel).to_lowercase(),
            "resumos": resumos,
            "nota": "continuidade por contadores; nao e latencia fisica",
        })),
    );
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_hora_utc_e_iso_com_a_data() {
        // 25/10/2026 06:39:24.145 UTC.
        assert_eq!(
            iso(Duration::from_millis(1_792_910_364_145)),
            "2026-10-25T06:39:24.145Z"
        );
        assert_eq!(iso(Duration::from_secs(0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso(Duration::from_secs(951_782_400)),
            "2000-02-29T00:00:00.000Z"
        );
    }

    fn args(lista: &[&str]) -> Result<Args, String> {
        Args::ler(lista.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn o_padrao_e_uma_hora_com_saida_aos_45_min_e_12_s() {
        let a = args(&[]).unwrap();
        assert_eq!(
            (a.duracao_s, a.saida_em_s, a.espera_s, a.ciclos),
            (3_600, 2_700, 12, 1)
        );
        assert_eq!(a.timeout(), Duration::from_secs(3_900));
        assert_eq!(a.papel, Papel::Tudo);
    }

    #[test]
    fn um_plano_que_nao_cabe_e_recusado() {
        let erro = args(&["--duracao-s", "100", "--saida-em-s", "90"]).unwrap_err();
        assert!(erro.contains("não cabe"), "{erro}");
        assert!(args(&[
            "--duracao-s",
            "100",
            "--saida-em-s",
            "20",
            "--depois-min-s",
            "10"
        ])
        .is_ok());
    }

    #[test]
    fn a_lan_so_com_o_argumento_consciente() {
        let a = args(&["--papel", "receptor"]).unwrap();
        assert!(endereco_permitido(&a, "127.0.0.1:1").is_ok());
        assert!(endereco_permitido(&a, "[::1]:1").is_ok());
        let recusa = endereco_permitido(&a, "192.168.0.5:18401").unwrap_err();
        assert!(recusa.contains("--permitir-lan"), "{recusa}");
        let b = args(&["--papel", "receptor", "--permitir-lan"]).unwrap();
        assert!(endereco_permitido(&b, "192.168.0.5:18401").is_ok());
    }

    #[test]
    fn o_padrao_muda_a_cada_quadro() {
        assert_ne!(quadro(0).luma(), quadro(1).luma());
    }
}
