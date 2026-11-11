//! Confere o vídeo e a voz **do produto** entre duas máquinas de verdade, com um
//! `seeled` de verdade no meio. É o `tela_do_produto_sob_perda` (variante
//! «produto») partido em dois processos, sem emulador de rede nenhum: a rede é a
//! que houver entre as máquinas.
//!
//! ```text
//! # o aparelho de mentira da voz não compila sem `debug_assertions`; a opção
//! # liga só isso, na linha de comando, e o resto do perfil é o `release`
//! cargo run --release --config 'profile.release.debug-assertions=true' \
//!     -p seele-conformance --example tela_entre_maquinas -- \
//!     transmite <ip:porta do seeled> [--segundos 30]
//! … --example tela_entre_maquinas -- \
//!     assiste <ip:porta do seeled> [--segundos 30] [--espera 120]
//! ```
//!
//! A ordem de subida não importa: quem transmite começa sem esperar ninguém, e
//! quem assiste espera a transmissão aparecer por até `--espera` segundos. A
//! janela de quem assiste começa no primeiro quadro que chega; para ela caber
//! inteira, dê a quem transmite mais segundos do que a quem assiste, ou suba
//! quem assiste primeiro.
//!
//! Quem assiste escolhe a transmissão mais nova da sala e diz qual (uma rodada
//! anterior morta sem despedida deixa a dela no servidor por uns 20 s). Ctrl+C
//! fecha a rodada: a linha final sai, e o processo se despede do servidor.
//!
//! `SEELE_LOG=1 RUST_LOG=warn,seele_core=info` manda o registro do produto ao
//! STDERR.
//!
//! # O que é de verdade
//!
//! Uma `seele_ffi::Connection` por processo, discando o servidor do argumento
//! (a impressão digital não é conferida, como no exemplo-fonte). O vídeo entra
//! pelo caminho da **câmera** (`Connection::compartilhar_camera` e
//! `Connection::quadro_da_camera`); depois dele é tudo o do produto. A voz é a
//! do produto, em modo aberto nos dois, sentados na primeira sala do servidor.
//!
//! # O que é de mentira
//!
//! - O codificador: quadros de enchimento em forma de H.264, com o tamanho que
//!   um codificador obediente ao `Event::CameraBudget` teria. Faz o que a casca
//!   faz (`apps/seele-app/ui/camera.js`): mira 0,85 do orçamento a 30 quadros,
//!   manda chave no começo, quando pedem, depois de um quadro recusado e a cada
//!   troca de orçamento, pula 3 quadros depois de uma chave, cede à fila do
//!   núcleo e para em `Event::CameraPaused`.
//! - O microfone e a saída: o aparelho de mentira do `seele-audio` (um tom
//!   contínuo; a saída conta os blocos que tocou com som).
//!
//! # As linhas
//!
//! Uma a cada 5 s e uma no fim (`final=sim`), em `chave=valor`. `nd` é «não
//! medido», e nunca um zero inventado.
//!
//! # RTP ou canal: o que a linha sabe, e o que ela não sabe
//!
//! A FFI **não expõe** por onde o vídeo foi. O que a linha traz:
//!
//! - `protocolo_negociado` e `video_pelo_canal_forcado`: as duas entradas da
//!   decisão de `seele_core::client` (liga a mídia com protocolo ≥
//!   `PROTOCOLO_DA_MIDIA` e sem a variável `SEELE_VIDEO_PELO_CANAL`).
//! - `transporte_esperado`: a mesma conta, refeita aqui. É **esperado**, e não
//!   medido: se o núcleo mudar a regra, esta linha não fica sabendo.
//!
//! Nada nos quadros que chegam distingue os dois caminhos (medido em
//! 11/11/2026: o prefixo de início chega com quatro bytes pelos dois). A prova
//! de fora é a linha do `seeled`: «a mídia RTP desta sessão foi ligada», uma
//! por pessoa que ligou, e nenhuma para quem ficou no canal.
//!
//! # A saída
//!
//! `assiste` sai com 1 se não chegou quadro nenhum ou se a voz do outro não foi
//! tocada com som; `transmite` sai com 1 se não conseguiu transmitir. 2 é erro
//! de uso ou de conexão.

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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seele_audio::device::mentira;
use seele_ffi::{
    ConnectConfig, Connection, ConnectionError, Event, EventListener, LimitesDeTela, Telemetry,
    VoiceMode,
};

const QUADROS_POR_SEGUNDO: u32 = 30;
/// `FRACAO_DO_TETO_DA_CAMERA` de `apps/seele-app/ui/camera.js`.
const FRACAO_DO_TETO: f64 = 0.85;
/// Quantas vezes o delta pesa um quadro-chave.
const FATOR_DA_CHAVE: usize = 8;
/// `QUADROS_APOS_A_CHAVE` de `camera.js`.
const QUADROS_APOS_A_CHAVE: u32 = 3;
/// `LIMIAR_DA_FILA_DA_CAMERA` de `camera.js`.
const LIMIAR_DA_FILA_DA_CAMERA: u32 = 4;
/// `SONDA_DA_FILA_DA_CAMERA_MS` de `camera.js`.
const SONDA_DA_FILA_DA_CAMERA: Duration = Duration::from_millis(100);
/// O piso de `configuracaoDaCamera` em `camera.js`: `Math.max(100_000, …)`.
const PISO_DO_CODIFICADOR_BPS: f64 = 100_000.0;
/// Código de início, tipo da NAL, número do quadro (16) e instante (16).
const CABECALHO: usize = 5 + 16 + 16;
/// Acima disto sem quadro novo, a imagem está congelada.
const CONGELADO_ACIMA_DE: f64 = 0.2;
/// Onde a janela de quem assiste se parte: o começo e o resto.
const COMECO: f64 = 10.0;
const ENTRE_LINHAS: Duration = Duration::from_secs(5);
/// Quanto cada passo da subida (entrar na sala, abrir a câmera) pode levar.
const PRAZO: Duration = Duration::from_secs(60);
/// A saída de mentira pede um bloco a cada 10 ms.
const BLOCOS_POR_SEGUNDO: f64 = 100.0;

const USO: &str = "uso:\n  tela_entre_maquinas transmite <ip:porta do seeled> [--segundos 30]\n  \
                   tela_entre_maquinas assiste   <ip:porta do seeled> [--segundos 30] [--espera 120]";

// ---------------------------------------------------------------------------
// A linha de comando
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Papel {
    Transmite,
    Assiste,
}

impl Papel {
    const fn nome(self) -> &'static str {
        match self {
            Self::Transmite => "transmite",
            Self::Assiste => "assiste",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Opcoes {
    papel: Papel,
    servidor: SocketAddr,
    segundos: u64,
    /// Até quando quem assiste espera a transmissão aparecer.
    espera: u64,
}

fn opcoes(mut argumentos: impl Iterator<Item = String>) -> Result<Opcoes, String> {
    let papel = match argumentos.next().as_deref() {
        Some("transmite") => Papel::Transmite,
        Some("assiste") => Papel::Assiste,
        Some(outro) => return Err(format!("papel desconhecido: {outro}")),
        None => return Err("falta o papel".into()),
    };
    let servidor = argumentos.next().ok_or("falta o endereço do seeled")?;
    let servidor = servidor
        .parse()
        .map_err(|erro| format!("`{servidor}` não é um ip:porta ({erro})"))?;
    let mut opcoes = Opcoes {
        papel,
        servidor,
        segundos: 30,
        espera: 120,
    };
    while let Some(nome) = argumentos.next() {
        let valor = argumentos
            .next()
            .ok_or_else(|| format!("falta o valor de {nome}"))?;
        let numero = valor
            .parse()
            .map_err(|erro| format!("{nome} {valor}: {erro}"))?;
        match nome.as_str() {
            "--segundos" => opcoes.segundos = numero,
            "--espera" => opcoes.espera = numero,
            outra => return Err(format!("opção desconhecida: {outra}")),
        }
    }
    Ok(opcoes)
}

// ---------------------------------------------------------------------------
// O que as linhas têm em comum
// ---------------------------------------------------------------------------

/// A hora do relógio da máquina, em UTC: é por ela que se casam as linhas das
/// duas máquinas e as do `seeled`.
fn hora() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        % 86_400_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}Z",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn medida(valor: Option<f64>, casas: usize) -> String {
    valor.map_or_else(|| "nd".into(), |v| format!("{v:.casas$}"))
}

/// A telemetria da conexão. `rtt_ms` e `perda` são `nd` enquanto não medidos;
/// o `jitter_ms` a FFI entrega sem dizer se mediu.
fn telemetria(t: &Telemetry) -> String {
    format!(
        "rtt_ms={} jitter_ms={:.3} perda={} sinal={}",
        medida(t.rtt_ms.map(f64::from), 3),
        t.jitter_ms,
        medida(t.loss_fraction.map(f64::from), 4),
        t.signal,
    )
}

/// O que este processo sabe sobre por onde o vídeo dele vai: as duas entradas
/// da decisão do núcleo e a conta refeita. **Esperado, e não medido** — a FFI
/// não diz por onde o vídeo foi.
fn transporte(protocolo: u8, forcado: bool) -> String {
    let esperado = if protocolo == 0 {
        "nd"
    } else if forcado || protocolo < seele_proto::screen::PROTOCOLO_DA_MIDIA {
        "canal"
    } else {
        "rtp"
    };
    format!(
        "protocolo_negociado={protocolo} video_pelo_canal_forcado={} transporte_esperado={esperado}",
        sim(forcado)
    )
}

/// Se a variável que segura o vídeo no canal está neste processo.
fn video_pelo_canal_forcado() -> bool {
    std::env::var_os(seele_core::client::VIDEO_PELO_CANAL).is_some()
}

const fn sim(valor: bool) -> &'static str {
    if valor {
        "sim"
    } else {
        "nao"
    }
}

/// A voz do outro neste processo: o que a saída de mentira tocou.
fn voz(aparelho: &str, sentado_ha: Duration) -> String {
    let c = mentira::contagem(aparelho);
    let tocada = if sentado_ha.is_zero() {
        None
    } else {
        Some(c.blocos_com_som as f64 * 100.0 / (sentado_ha.as_secs_f64() * BLOCOS_POR_SEGUNDO))
    };
    format!(
        "voz_aparelhos_abertos={} voz_blocos_capturados={} voz_blocos_tocados_com_som={} \
         voz_tocada_pct={} voz_do_outro_chegou={}",
        c.abertos,
        c.blocos,
        c.blocos_com_som,
        medida(tocada, 1),
        sim(c.blocos_com_som > 0),
    )
}

/// Um quadro de mentira: cara de H.264 em Annex-B, o número, o instante da
/// captura, e enchimento até o tamanho pedido.
fn quadro(chave: bool, numero: u64, capturado_ns: u64, tamanho: usize) -> Vec<u8> {
    let tamanho = tamanho.max(CABECALHO);
    let mut bytes = Vec::with_capacity(tamanho);
    bytes.extend_from_slice(&[0, 0, 0, 1, if chave { 0x65 } else { 0x41 }]);
    // Em hexadecimal, e o enchimento sem zeros: um número binário traria
    // `00 00 01` dentro do quadro, que é um prefixo de início de NAL, e o
    // empacotador de H.264 do RTP partiria o quadro ali.
    bytes.extend_from_slice(format!("{numero:016x}{capturado_ns:016x}").as_bytes());
    bytes.extend((CABECALHO..tamanho).map(|i| (i % 251) as u8 | 0x80));
    bytes
}

// ---------------------------------------------------------------------------
// Entrar e sentar
// ---------------------------------------------------------------------------

async fn entrar(
    servidor: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
    olhos: Arc<dyn EventListener>,
) -> Result<Arc<Connection>, String> {
    let config = ConnectConfig {
        server: servidor.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id(apelido)),
        playback_device: Some(mentira::id(apelido)),
    };
    let foco = seele_ffi::foco::FocoDeVoz::novo();
    let conexao = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .map_err(|erro| format!("a thread que conecta caiu: {erro}"))?
    .map(|(conexao, _)| conexao)
    .map_err(|falha| {
        let trilha: Vec<String> = falha
            .trail
            .iter()
            .map(|passo| format!("{:?} aos {} ms", passo.etapa, passo.em_ms))
            .collect();
        format!(
            "não consegui entrar no seeled em {servidor}: {:?}. Por onde a chegada passou: {}",
            falha.error,
            if trilha.is_empty() {
                "nada chegou a ser tentado".into()
            } else {
                trilha.join(", ")
            }
        )
    })?;
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

/// Senta na primeira sala do servidor, com a voz aberta.
async fn sentar(conexao: &Arc<Connection>) -> Result<(), String> {
    if !ate(PRAZO, || !conexao.snapshot().voice_rooms.is_empty()).await {
        return Err(format!(
            "o servidor não mostrou sala nenhuma em {} s",
            PRAZO.as_secs()
        ));
    }
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao
        .enter_voice_room(sala, None)
        .map_err(|erro| format!("o pedido de entrar na sala não saiu: {erro:?}"))?;
    let sentou = ate(PRAZO, || {
        conexao
            .snapshot()
            .voice_rooms
            .iter()
            .any(|sala| sala.occupied_by_us)
    })
    .await;
    if !sentou {
        return Err(format!(
            "o servidor não sentou esta pessoa na sala em {} s",
            PRAZO.as_secs()
        ));
    }
    conexao.set_voice_mode(VoiceMode::Open);
    Ok(())
}

/// O que as duas janelas guardam do fim da sessão.
#[derive(Default)]
struct Fim(Mutex<Option<String>>);

impl Fim {
    fn anotar(&self, event: &Event) {
        let texto = match event {
            Event::Ended { reason } => format!("acabou ({reason:?})"),
            Event::CaiuComMotivo { reason } => format!("caiu e vai voltar ({reason:?})"),
            _ => return,
        };
        println!("{} sessao: {texto}", hora());
        *self.0.lock().unwrap() = Some(texto);
    }

    fn dito(&self) -> String {
        self.0
            .lock()
            .unwrap()
            .clone()
            .map_or_else(|| "nao".into(), |texto| format!("\"{texto}\""))
    }

    /// A sessão acabou, ou pediram para parar (Ctrl+C): a rodada fecha a
    /// conta com o que tem.
    fn acabou(&self) -> bool {
        INTERROMPIDO.load(Ordering::SeqCst)
            || self
                .0
                .lock()
                .unwrap()
                .as_deref()
                .is_some_and(|texto| texto.starts_with("acabou"))
    }
}

/// Ctrl+C: a rodada para, imprime a linha final e se despede do servidor.
static INTERROMPIDO: AtomicBool = AtomicBool::new(false);

/// Quanto se espera a despedida chegar ao servidor antes de o processo sair.
const DESPEDIDA: Duration = Duration::from_secs(2);

/// Desconecta e espera a despedida sair.
///
/// `Connection::disconnect` só enfileira o pedido. Um processo que sai logo
/// depois deixa a sessão de pé no servidor até o prazo dele (uns 20 s, medido
/// em 11/11/2026), e com ela a transmissão: a rodada seguinte entra numa sala
/// com uma transmissão que ninguém mais manda.
async fn sair(conexao: &Connection) {
    conexao.disconnect();
    ate(DESPEDIDA, || conexao.snapshot().ended.is_some()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
}

// ---------------------------------------------------------------------------
// Quem transmite
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
    fim: Fim,
}

impl EventListener for DeQuemTransmite {
    fn on_event(&self, event: Event) {
        self.fim.anotar(&event);
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

/// O que o codificador de mentira contou.
#[derive(Default)]
struct Codificado {
    mandados: AtomicU64,
    chaves: AtomicU64,
    bytes: AtomicU64,
    recusados_pela_fila: AtomicU64,
    sem_camera: AtomicU64,
    parados: AtomicU64,
}

fn linha_de_quem_transmite(
    conexao: &Connection,
    olhos: &DeQuemTransmite,
    contado: &Codificado,
    desde: Instant,
    sentado: Instant,
    fim: bool,
) -> String {
    let segundos = desde.elapsed().as_secs_f64();
    let bytes = contado.bytes.load(Ordering::SeqCst);
    let motivos = olhos.motivos.lock().unwrap().join("+");
    format!(
        "{} papel=transmite final={} t_s={segundos:.1} quadros_mandados={} chaves_mandadas={} \
         pedidos_de_chave={} pausas={} motivos_das_pausas={} parada_agora={} \
         orcamento_kbps={} avisos_de_orcamento={} aberturas={} recusados_pela_fila={} \
         sem_camera={} kbps_mandados={} {} {} {} sessao_acabou={}",
        hora(),
        sim(fim),
        contado.mandados.load(Ordering::SeqCst),
        contado.chaves.load(Ordering::SeqCst),
        olhos.pedidos_de_chave.load(Ordering::SeqCst),
        olhos.pausas.load(Ordering::SeqCst),
        if motivos.is_empty() {
            "nenhum"
        } else {
            &motivos
        },
        sim(olhos.parada.load(Ordering::SeqCst)),
        // Zero avisos é «o núcleo ainda não disse», e não um orçamento de zero.
        medida(
            (olhos.orcamentos.load(Ordering::SeqCst) > 0)
                .then(|| f64::from(olhos.orcamento_bps.load(Ordering::SeqCst)) / 1000.0),
            0
        ),
        olhos.orcamentos.load(Ordering::SeqCst),
        olhos.aberturas.load(Ordering::SeqCst),
        contado.recusados_pela_fila.load(Ordering::SeqCst),
        contado.sem_camera.load(Ordering::SeqCst),
        medida(
            (segundos > 0.0).then(|| bytes as f64 * 8.0 / segundos / 1000.0),
            0
        ),
        telemetria(&conexao.snapshot().telemetry),
        voz(Papel::Transmite.nome(), sentado.elapsed()),
        transporte(conexao.protocolo_negociado(), video_pelo_canal_forcado()),
        olhos.fim.dito(),
    )
}

async fn transmitir(opcoes: Opcoes) -> Result<bool, String> {
    let casa = tempfile::tempdir().map_err(|erro| format!("a pasta temporária: {erro}"))?;
    let olhos = Arc::new(DeQuemTransmite::default());
    let conexao = entrar(
        opcoes.servidor,
        casa.path(),
        Papel::Transmite.nome(),
        Arc::clone(&olhos) as Arc<dyn EventListener>,
    )
    .await?;
    let resultado = transmitir_por(&conexao, &olhos, opcoes).await;
    if resultado.is_ok() {
        // A casca para a câmera antes de sair.
        let _ = conexao.parar_de_compartilhar();
    }
    sair(&conexao).await;
    resultado
}

async fn transmitir_por(
    conexao: &Arc<Connection>,
    olhos: &Arc<DeQuemTransmite>,
    opcoes: Opcoes,
) -> Result<bool, String> {
    println!(
        "{} transmite: entrou em {}; {}",
        hora(),
        opcoes.servidor,
        transporte(conexao.protocolo_negociado(), video_pelo_canal_forcado())
    );
    sentar(conexao).await?;
    let sentado = Instant::now();
    println!(
        "{} transmite: sentado na sala, voz aberta (o tom de mentira)",
        hora()
    );
    // A voz anda alguns segundos antes da câmera, como quando alguém já está
    // na conversa e então liga a câmera.
    tokio::time::sleep(Duration::from_secs(3)).await;

    conexao
        .compartilhar_camera(
            1280,
            720,
            // Os de `LIMITES_DA_CAMERA` em `apps/seele-app/ui/camera.js`.
            LimitesDeTela {
                banda_bps: None,
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
        return Err(format!(
            "a câmera não abriu em {} s (aberturas {}, avisos de orçamento {})",
            PRAZO.as_secs(),
            olhos.aberturas.load(Ordering::SeqCst),
            olhos.orcamentos.load(Ordering::SeqCst)
        ));
    }
    println!(
        "{} transmite: a câmera abriu; transmitindo por {} s, sem esperar quem assiste",
        hora(),
        opcoes.segundos
    );

    let comeco = Instant::now();
    let fim = comeco + Duration::from_secs(opcoes.segundos);
    let contado = Arc::new(Codificado::default());

    // O codificador de mentira.
    let codificando = {
        let olhos = Arc::clone(olhos);
        let conexao = Arc::clone(conexao);
        let contado = Arc::clone(&contado);
        tokio::spawn(async move {
            let mut relogio = tokio::time::interval(Duration::from_secs(1) / QUADROS_POR_SEGUNDO);
            relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // O número só anda quando o núcleo aceita o quadro: um buraco na
            // numeração de quem assiste é um quadro aceito que não chegou.
            let mut numero = 0_u64;
            let mut configurado = 0_u32;
            let mut pular = 0_u32;
            // A contrapressão da casca (`camera.js`, `filaDoNucleoCheia`): com a
            // fila do núcleo pela metade ela não codifica, a não ser um quadro
            // a cada 100 ms, que é como fica sabendo que a fila andou.
            let mut na_fila = 0_u32;
            let mut na_fila_em = Instant::now();
            while Instant::now() < fim && !olhos.fim.acabou() {
                relogio.tick().await;
                if na_fila >= LIMIAR_DA_FILA_DA_CAMERA
                    && na_fila_em.elapsed() < SONDA_DA_FILA_DA_CAMERA
                {
                    continue;
                }
                if olhos.parada.load(Ordering::SeqCst) {
                    contado.parados.fetch_add(1, Ordering::SeqCst);
                    continue;
                }
                let orcamento = olhos.orcamento_bps.load(Ordering::SeqCst);
                if orcamento != configurado {
                    // A casca reconfigura o codificador **sem pedir chave**
                    // (`camera.js`, desde 11/11/2026): a troca de orçamento
                    // não muda as medidas nem o codec.
                    configurado = orcamento;
                }
                let chave = olhos.quer_chave.load(Ordering::SeqCst);
                if pular > 0 && !chave {
                    pular -= 1;
                    continue;
                }
                let mira = (f64::from(orcamento) * FRACAO_DO_TETO)
                    .floor()
                    .max(PISO_DO_CODIFICADOR_BPS);
                let delta = (mira / f64::from(QUADROS_POR_SEGUNDO) / 8.0) as usize;
                let tamanho = if chave { delta * FATOR_DA_CHAVE } else { delta };
                let bytes = quadro(
                    chave,
                    numero + 1,
                    comeco.elapsed().as_nanos() as u64,
                    tamanho,
                );
                let tamanho = bytes.len() as u64;
                match conexao.quadro_da_camera(chave, bytes) {
                    Ok(esperando) => {
                        numero += 1;
                        na_fila = esperando;
                        na_fila_em = Instant::now();
                        contado.mandados.fetch_add(1, Ordering::SeqCst);
                        contado.bytes.fetch_add(tamanho, Ordering::SeqCst);
                        if chave {
                            contado.chaves.fetch_add(1, Ordering::SeqCst);
                            // Só baixa o pedido que este quadro atendeu.
                            olhos.quer_chave.store(false, Ordering::SeqCst);
                            pular = QUADROS_APOS_A_CHAVE;
                        }
                    }
                    // O quadro que não foi era referência do seguinte.
                    Err(ConnectionError::CameraQueueFull { .. }) => {
                        contado.recusados_pela_fila.fetch_add(1, Ordering::SeqCst);
                        olhos.quer_chave.store(true, Ordering::SeqCst);
                    }
                    Err(_) => {
                        contado.sem_camera.fetch_add(1, Ordering::SeqCst);
                        olhos.quer_chave.store(true, Ordering::SeqCst);
                    }
                }
            }
        })
    };

    while !codificando.is_finished() {
        let falta = fim.saturating_duration_since(Instant::now());
        tokio::time::sleep(ENTRE_LINHAS.min(falta).max(Duration::from_millis(50))).await;
        if Instant::now() < fim && !codificando.is_finished() {
            println!(
                "{}",
                linha_de_quem_transmite(conexao, olhos, &contado, comeco, sentado, false)
            );
        }
    }
    codificando
        .await
        .map_err(|erro| format!("o codificador de mentira caiu: {erro}"))?;
    println!(
        "{}",
        linha_de_quem_transmite(conexao, olhos, &contado, comeco, sentado, true)
    );
    let transmitiu = contado.mandados.load(Ordering::SeqCst) > 0;
    if !transmitiu {
        println!(
            "{} transmite: FALHOU — o núcleo não aceitou quadro nenhum",
            hora()
        );
    }
    Ok(transmitiu)
}

// ---------------------------------------------------------------------------
// Quem assiste
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Chegada {
    quando: Instant,
    /// O número que quem transmite escreveu no quadro, quando deu para ler.
    numero: Option<u64>,
    bytes: usize,
    chave: bool,
    /// De qual transmissão.
    tela: u32,
}

#[derive(Default)]
struct DeQuemAssiste {
    chegadas: Mutex<Vec<Chegada>>,
    aberturas: AtomicU64,
    fechadas: AtomicU64,
    ilegiveis: AtomicU64,
    fim: Fim,
}

impl EventListener for DeQuemAssiste {
    fn on_event(&self, event: Event) {
        self.fim.anotar(&event);
        match event {
            Event::ScreenFrame { screen, key, data } => {
                // O relógio antes do base64: decodificar não é atraso da rede.
                let quando = Instant::now();
                let Some(bytes) = seele_ffi::de_base64(&data) else {
                    return;
                };
                // O prefixo de início pode ter três ou quatro bytes.
                let inicio = bytes.windows(3).position(|j| j == [0, 0, 1]);
                let numero = inicio.and_then(|inicio| {
                    bytes
                        .get(inicio + 4..inicio + 20)
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .and_then(|t| u64::from_str_radix(t, 16).ok())
                });
                self.chegadas.lock().unwrap().push(Chegada {
                    quando,
                    numero,
                    bytes: bytes.len(),
                    chave: key,
                    tela: screen,
                });
            }
            Event::ScreenOpened { .. } => {
                self.aberturas.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenClosed { .. } => {
                self.fechadas.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenUnreadable { reason } => {
                self.ilegiveis.fetch_add(1, Ordering::SeqCst);
                println!("{} assiste: transmissão ilegível: {reason}", hora());
            }
            _ => {}
        }
    }
}

/// A fração de `[de, ate]` em que a imagem estava congelada, e `None` se o
/// trecho ainda não existe.
///
/// `chegadas` são os instantes dos quadros, em segundos desde o primeiro e em
/// ordem; `fim` é o fim da janela, e conta como borda. Um intervalo de mais de
/// [`CONGELADO_ACIMA_DE`] conta inteiro, e o que cai dentro do trecho é do
/// trecho: um congelamento que atravessa a marca dos 10 s não some por ter
/// sido partido em dois pedaços curtos.
fn fracao_congelada(chegadas: &[f64], de: f64, ate: f64, fim: f64) -> Option<f64> {
    let ate = ate.min(fim);
    if ate <= de {
        return None;
    }
    let mut congelado = 0.0;
    let mut anterior = 0.0_f64;
    for instante in chegadas.iter().copied().chain(std::iter::once(fim)) {
        if instante - anterior > CONGELADO_ACIMA_DE {
            congelado += (instante.min(ate) - anterior.max(de)).max(0.0);
        }
        anterior = instante;
    }
    Some(congelado / (ate - de))
}

/// O maior intervalo sem quadro novo, em segundos, com o fim como borda.
fn maior_intervalo(chegadas: &[f64], fim: f64) -> f64 {
    let mut maior = 0.0_f64;
    let mut anterior = 0.0_f64;
    for instante in chegadas.iter().copied().chain(std::iter::once(fim)) {
        maior = maior.max(instante - anterior);
        anterior = instante;
    }
    maior
}

/// Escolhe assistir a transmissão mais nova da sala que não é desta pessoa.
///
/// A sala pode ter mais de uma: a de uma rodada anterior que caiu sem se
/// despedir fica no servidor até o prazo dele, e quem assiste não é passado
/// sozinho de uma para a outra (medido em 11/11/2026: a transmissão nova ficou
/// com zero espectadores, e nenhum quadro chegou). Na janela do produto é a
/// pessoa que escolhe; aqui é a mais nova, e a escolha é dita na tela.
fn escolher_a_mais_nova(conexao: &Connection, pedida: &mut Option<u32>) {
    let retrato = conexao.snapshot();
    let alheias: Vec<u32> = retrato
        .transmissoes
        .iter()
        .filter(|t| !t.e_minha)
        .map(|t| t.tela)
        .collect();
    let Some(nova) = alheias.iter().copied().max() else {
        return;
    };
    if *pedida == Some(nova) {
        return;
    }
    match conexao.assistir(nova, true) {
        Ok(()) => println!(
            "{} assiste: escolheu a transmissão {nova} (transmissões alheias na sala: {alheias:?})",
            hora()
        ),
        Err(erro) => println!(
            "{} assiste: o pedido de assistir a transmissão {nova} não saiu: {erro:?}",
            hora()
        ),
    }
    // Pedida uma vez, mesmo que o pedido não tenha saído: a linha acima já
    // disse, e repetir a cada 10 ms não diria mais.
    *pedida = Some(nova);
}

fn linha_de_quem_assiste(
    conexao: &Connection,
    olhos: &DeQuemAssiste,
    sentado: Instant,
    ate: Instant,
    fim: bool,
) -> String {
    // Só o que chegou dentro da janela: o que vem depois do fim é drenagem.
    let chegadas: Vec<Chegada> = olhos
        .chegadas
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|c| c.quando <= ate)
        .collect();
    let primeiro = chegadas.first().map(|c| c.quando);
    let segundos = primeiro.map(|primeiro| ate.duration_since(primeiro).as_secs_f64());
    let instantes: Vec<f64> = primeiro.map_or_else(Vec::new, |primeiro| {
        chegadas
            .iter()
            .map(|c| c.quando.duration_since(primeiro).as_secs_f64())
            .collect()
    });
    let bytes: usize = chegadas.iter().map(|c| c.bytes).sum();
    let telas: std::collections::BTreeSet<u32> = chegadas.iter().map(|c| c.tela).collect();
    let retrato = conexao.snapshot();
    let numeros: std::collections::BTreeSet<u64> =
        chegadas.iter().filter_map(|c| c.numero).collect();
    // Os aceitos pelo núcleo de quem transmite que não chegaram aqui, entre o
    // primeiro e o último que chegaram.
    let buracos = match (numeros.first(), numeros.last()) {
        (Some(primeiro), Some(ultimo)) => {
            Some((ultimo - primeiro + 1 - numeros.len() as u64) as f64)
        }
        _ => None,
    };
    format!(
        "{} papel=assiste final={} t_s={} quadros_chegados={} chaves_chegadas={} \
         kbps_recebidos={} maior_intervalo_ms={} fracao_congelada_0_a_10s={} \
         fracao_congelada_depois_de_10s={} buracos_na_numeracao={} aberturas={} fechadas={} \
         ilegiveis={} transmissoes_na_sala={} telas_com_quadros={} {} {} {} sessao_acabou={}",
        hora(),
        sim(fim),
        medida(segundos, 1),
        chegadas.len(),
        chegadas.iter().filter(|c| c.chave).count(),
        medida(
            segundos
                .filter(|s| *s > 0.0)
                .map(|s| bytes as f64 * 8.0 / s / 1000.0),
            0
        ),
        medida(segundos.map(|s| maior_intervalo(&instantes, s) * 1000.0), 0),
        medida(
            segundos.and_then(|s| fracao_congelada(&instantes, 0.0, COMECO, s)),
            4
        ),
        medida(
            segundos.and_then(|s| fracao_congelada(&instantes, COMECO, s, s)),
            4
        ),
        medida(buracos, 0),
        olhos.aberturas.load(Ordering::SeqCst),
        olhos.fechadas.load(Ordering::SeqCst),
        olhos.ilegiveis.load(Ordering::SeqCst),
        retrato.transmissoes.len(),
        telas.len(),
        telemetria(&retrato.telemetry),
        voz(Papel::Assiste.nome(), sentado.elapsed()),
        transporte(conexao.protocolo_negociado(), video_pelo_canal_forcado()),
        olhos.fim.dito(),
    )
}

async fn assistir(opcoes: Opcoes) -> Result<bool, String> {
    let casa = tempfile::tempdir().map_err(|erro| format!("a pasta temporária: {erro}"))?;
    let olhos = Arc::new(DeQuemAssiste::default());
    let conexao = entrar(
        opcoes.servidor,
        casa.path(),
        Papel::Assiste.nome(),
        Arc::clone(&olhos) as Arc<dyn EventListener>,
    )
    .await?;
    let resultado = assistir_por(&conexao, &olhos, opcoes).await;
    sair(&conexao).await;
    resultado
}

async fn assistir_por(
    conexao: &Arc<Connection>,
    olhos: &Arc<DeQuemAssiste>,
    opcoes: Opcoes,
) -> Result<bool, String> {
    println!(
        "{} assiste: entrou em {}; {}",
        hora(),
        opcoes.servidor,
        transporte(conexao.protocolo_negociado(), video_pelo_canal_forcado())
    );
    sentar(conexao).await?;
    let sentado = Instant::now();
    println!(
        "{} assiste: sentado na sala, voz aberta (o tom de mentira); esperando a transmissão \
         aparecer por até {} s",
        hora(),
        opcoes.espera
    );

    let mut pedida = None;
    let mut chegou_algum = || {
        escolher_a_mais_nova(conexao, &mut pedida);
        !olhos.chegadas.lock().unwrap().is_empty()
    };
    let prazo = Instant::now() + Duration::from_secs(opcoes.espera);
    while !chegou_algum() && Instant::now() < prazo && !olhos.fim.acabou() {
        let passo = ENTRE_LINHAS.min(prazo.saturating_duration_since(Instant::now()));
        if !ate(passo, &mut chegou_algum).await && Instant::now() < prazo {
            println!(
                "{}",
                linha_de_quem_assiste(conexao, olhos, sentado, Instant::now(), false)
            );
        }
    }
    let primeiro = olhos.chegadas.lock().unwrap().first().map(|c| c.quando);
    let fim = if let Some(primeiro) = primeiro {
        println!(
            "{} assiste: o primeiro quadro chegou; a janela é de {} s a partir dele",
            hora(),
            opcoes.segundos
        );
        let fim = primeiro + Duration::from_secs(opcoes.segundos);
        while Instant::now() < fim && !olhos.fim.acabou() {
            let falta = fim.saturating_duration_since(Instant::now());
            // Enquanto espera a linha seguinte, segue a transmissão mais nova.
            ate(ENTRE_LINHAS.min(falta), || {
                chegou_algum();
                olhos.fim.acabou()
            })
            .await;
            if Instant::now() < fim {
                println!(
                    "{}",
                    linha_de_quem_assiste(conexao, olhos, sentado, Instant::now(), false)
                );
            }
        }
        fim.min(Instant::now())
    } else {
        println!(
            "{} assiste: nenhum quadro chegou em {} s de espera",
            hora(),
            opcoes.espera
        );
        Instant::now()
    };
    println!(
        "{}",
        linha_de_quem_assiste(conexao, olhos, sentado, fim, true)
    );
    let quadros = olhos.chegadas.lock().unwrap().len();
    let voz_chegou = mentira::contagem(Papel::Assiste.nome()).blocos_com_som > 0;
    if quadros == 0 {
        println!("{} assiste: FALHOU — não chegou quadro nenhum", hora());
    }
    if !voz_chegou {
        println!(
            "{} assiste: FALHOU — a voz do outro não foi tocada com som",
            hora()
        );
    }
    Ok(quadros > 0 && voz_chegou)
}

#[tokio::main]
async fn main() {
    let opcoes = match opcoes(std::env::args().skip(1)) {
        Ok(opcoes) => opcoes,
        Err(erro) => {
            eprintln!("{erro}\n{USO}");
            std::process::exit(2);
        }
    };
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("{} interrompido: fechando a rodada e saindo", hora());
            INTERROMPIDO.store(true, Ordering::SeqCst);
        }
        // O segundo Ctrl+C não espera nada.
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });
    let resultado = match opcoes.papel {
        Papel::Transmite => transmitir(opcoes).await,
        Papel::Assiste => assistir(opcoes).await,
    };
    let codigo = match resultado {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(erro) => {
            println!("{} {}: ERRO — {erro}", hora(), opcoes.papel.nome());
            2
        }
    };
    // As threads da conexão não seguram a saída: o código é o relatório.
    std::process::exit(codigo);
}

#[cfg(test)]
mod testes {
    use super::{fracao_congelada, maior_intervalo, opcoes, transporte, Papel};

    /// Um congelamento que atravessa a marca dos 10 s é dos dois trechos, cada
    /// um com o seu pedaço, e nenhum pedaço some por ser curto.
    #[test]
    fn o_congelamento_que_atravessa_a_marca_e_repartido() {
        // Um quadro a cada 100 ms até 9,9 s, o seguinte só em 10,2 s, e a
        // janela acaba em 10,3 s: 300 ms congelados, 100 antes da marca e 200
        // depois dela.
        let mut chegadas: Vec<f64> = (0..100).map(|i| f64::from(i) / 10.0).collect();
        chegadas.push(10.2);
        let comeco = fracao_congelada(&chegadas, 0.0, 10.0, 10.3).unwrap();
        let resto = fracao_congelada(&chegadas, 10.0, 10.3, 10.3).unwrap();
        assert!((comeco - 0.1 / 10.0).abs() < 1e-9, "{comeco}");
        assert!((resto - 0.2 / 0.3).abs() < 1e-9, "{resto}");
        // Sem o intervalo longo, nada congelou em trecho nenhum.
        chegadas.insert(100, 10.0);
        chegadas.insert(101, 10.1);
        assert_eq!(fracao_congelada(&chegadas, 0.0, 10.0, 10.3), Some(0.0));
        assert_eq!(fracao_congelada(&chegadas, 10.0, 10.3, 10.3), Some(0.0));
    }

    /// Sem o resto da janela, o resto é «não medido», e não zero.
    #[test]
    fn o_trecho_que_nao_existe_nao_e_zero() {
        assert_eq!(fracao_congelada(&[0.0, 0.03], 10.0, 5.0, 5.0), None);
        assert!(fracao_congelada(&[0.0, 0.03], 0.0, 10.0, 5.0).is_some());
        assert!((maior_intervalo(&[0.0, 0.03], 5.0) - 4.97).abs() < 1e-9);
    }

    /// A linha do transporte diz as entradas e o esperado, e não afirma nada
    /// antes do aperto de mão.
    #[test]
    fn o_transporte_esperado_segue_a_regra_do_nucleo() {
        let midia = seele_proto::screen::PROTOCOLO_DA_MIDIA;
        assert!(transporte(midia, false).ends_with("transporte_esperado=rtp"));
        assert!(transporte(midia, true).ends_with("transporte_esperado=canal"));
        assert!(transporte(midia - 1, false).ends_with("transporte_esperado=canal"));
        assert!(transporte(0, false).ends_with("transporte_esperado=nd"));
    }

    #[test]
    fn a_linha_de_comando() {
        let lida = |texto: &str| opcoes(texto.split_whitespace().map(str::to_owned));
        let o = lida("assiste 192.168.0.7:8383 --segundos 12 --espera 5").unwrap();
        assert_eq!(
            (o.papel, o.segundos, o.espera, o.servidor.port()),
            (Papel::Assiste, 12, 5, 8383)
        );
        assert_eq!(lida("transmite 10.0.0.1:1").unwrap().segundos, 30);
        assert!(lida("olha 10.0.0.1:1").is_err());
        assert!(lida("assiste casa").is_err());
        assert!(lida("assiste 10.0.0.1:1 --segundos").is_err());
    }
}
