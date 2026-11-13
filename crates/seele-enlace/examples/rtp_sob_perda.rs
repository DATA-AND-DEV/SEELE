//! Mede como um vídeo sintético de tela se comporta sob perda quando vai por
//! **RTP** (NACK/RTX, pedido de quadro-chave e estimativa de banda do str0m),
//! com uma voz ao lado, nos mesmos cenários de `tela_sob_perda.rs`. É bancada,
//! não produto: imprime tabelas em Markdown, e o relatório é
//! `docs/medidas/2026-11-10-rtp-sob-perda.md`.
//!
//! ```text
//! cargo run --release -p seele-enlace --example rtp_sob_perda
//! cargo run --release -p seele-enlace --example rtp_sob_perda -- \
//!     [--variante fixo,adaptativo,a,voz] [--so <pedaço do nome do cenário>] \
//!     [--segundos 20] [--rodadas 3] [--semente 20261110] [--aquecimento 0] \
//!     [--espera-ms 2000] [--reordem 1000] [--pli-ms 300] [--entre-chaves-ms 300] \
//!     [--inicial-bps 300000] [--sem-teto-rtx] [--pular 3] [--reordem-voz 0] \
//!     [--perda 0.10] [--jitter-ms 0] [--serie]
//! ```
//!
//! # O banco
//!
//! Dois `Rtc` do str0m num processo só — quem transmite e quem recebe —, cada
//! um com o seu socket UDP em 127.0.0.1, e o `Cano` de `seele_enlace::teste`
//! no meio. Quem transmite só fala com o endereço do cano; quem recebe só vê
//! o socket do cano (o candidato dele é aprendido das checagens STUN, como um
//! NAT faria). A oferta e a resposta SDP são trocadas **em memória**, sem
//! candidatos dentro; ICE, DTLS, SRTP, RTP e RTCP passam pelo cano.
//!
//! # A carga
//!
//! - Vídeo: H.264 de mentira, 30 quadros/s. Cada quadro é um Annex-B que o
//!   empacotador do str0m aceita: a chave é SPS + PPS + NAL 5, o delta é um
//!   NAL 1; depois do cabeçalho do NAL vêm a marca, o número do quadro, o
//!   instante da captura e o tamanho, e o resto é enchimento.
//! - Voz: uma trilha Opus de mentira, 50 pacotes/s de 120 bytes.
//!
//! # As variantes
//!
//! - **fixo**: deltas de 6 KiB e chave de 80 KiB (≈1,47 Mbps), sem estimativa
//!   de banda e sem ritmo: a carga da linha «A» da medida do SCTP.
//! - **adaptativo**: a estimativa de banda do str0m ligada (`enable_bwe`, TWCC)
//!   e o codificador de mentira atrás dela: delta = estimativa × 0,85 ÷ 30 ÷ 8,
//!   chave = 8 × o delta, entre 200 kbps e 2,79 Mbps; pula `--pular` quadros
//!   depois de cada chave, como a casca do aplicativo.
//! - **a**: o controle. O emissor da linha «A» de `tela_sob_perda.rs` (canal
//!   `u` confiável do enlace, SCTP), copiado para cá, no mesmo cano.
//! - **voz**: só a trilha de voz por RTP: a referência do que o vídeo custa.
//!
//! # O modelo de decodificação
//!
//! O mesmo da medida do SCTP: um delta só é exibível se todos os quadros desde
//! a última chave chegaram inteiros. Quem decide é o número do quadro dentro
//! dos bytes, e não a palavra do str0m (`MediaData::contiguous` é conferido
//! contra ele). Cadeia quebrada: o receptor manda PLI, repete a cada
//! `--pli-ms`, e fica congelado até uma chave chegar inteira.
//!
//! **Quanto o receptor espera uma retransmissão** é o
//! `RtcConfig::set_reordering_timeout_video` do str0m (`--espera-ms`; o padrão
//! do str0m é 2 s): o prazo conta da chegada do primeiro pacote do quadro mais
//! velho ainda incompleto; vencido, o quadro é largado e a cadeia quebra.
//!
//! # As métricas
//!
//! - Latência de quadro: do instante da captura até o quadro sair do str0m
//!   exibível. **Sem buffer de reprodução**: um reprodutor de verdade soma o
//!   dele.
//! - Congelado: a soma dos intervalos de mais de 200 ms sem quadro novo
//!   exibível, sobre a janela (as bordas da janela contam).
//! - RTX: datagramas com o tipo de carga de retransmissão que saíram do socket
//!   de quem transmite (o cabeçalho RTP não é cifrado). No adaptativo o mesmo
//!   tipo de carga leva também o enchimento das sondas da estimativa.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::struct_excessive_bools,
    missing_docs,
    reason = "bancada de medida: falha é pânico, e os números cabem nas conversões"
)]

use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use seele_enlace::teste::{
    config_de_rtc_cru, par_por_cano, Cano, ConfigDoCano, ContagemDoSentido, Sentido,
};
use seele_enlace::{Conexao, Envio, Recebe};
use str0m::bwe::{Bitrate, BweKind};
use str0m::format::Codec;
use str0m::media::{Direction, KeyframeRequestKind, MediaData, MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::time::MissedTickBehavior;

const QUADROS_POR_SEGUNDO: u32 = 30;
const DELTA: usize = 6 * 1024;
const CHAVE: usize = 80 * 1024;
const VOZ: usize = 120;
const VOZ_A_CADA: Duration = Duration::from_millis(20);
const CONGELADO: Duration = Duration::from_millis(200);
const ATRASO: Duration = Duration::from_millis(30);
/// O teto e o piso do orçamento de vídeo do produto (câmera em 720p).
const TETO_BPS: u64 = 2_790_000;
const PISO_BPS: u64 = 200_000;
/// A fração do orçamento que a casca do aplicativo entrega ao codificador.
const FRACAO: f64 = 0.85;
/// Quantas vezes o delta a chave pesa no codificador de mentira adaptativo.
const CHAVE_EM_DELTAS: usize = 8;
/// De quanto em quanto a estimativa de banda é amostrada dentro da janela.
const AMOSTRA: Duration = Duration::from_millis(250);
/// tipo (1), id (4), instante (8), tamanho (4): o cabeçalho de um quadro no `u`.
const CABECALHO_DO_QUADRO: usize = 17;

/// Um SPS e um PPS com forma de H.264. Ninguém os decodifica: existem para o
/// empacotador do str0m montar o STAP-A que antecede um IDR de verdade.
const SPS: [u8; 8] = [0x67, 0x42, 0xC0, 0x1F, 0x8C, 0x8D, 0x40, 0x50];
const PPS: [u8; 4] = [0x68, 0xCE, 0x3C, 0x80];
const INICIO_DE_NAL: [u8; 4] = [0, 0, 0, 1];
const NAL_CHAVE: u8 = 0x65;
const NAL_DELTA: u8 = 0x41;
const MARCA: &[u8; 4] = b"SEEL";
/// A marca e três campos em hexadecimal: id (8), instante (16), tamanho (8).
/// Em texto para não haver `00 00 01` dentro do NAL.
const CAMPOS: usize = 4 + 8 + 16 + 8;

/// O relógio da medida: um só no processo, lido pelos dois lados.
fn agora_us() -> u64 {
    static ZERO: OnceLock<Instant> = OnceLock::new();
    ZERO.get_or_init(Instant::now).elapsed().as_micros() as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Variante {
    Voz,
    A,
    Fixo,
    Adaptativo,
}

impl Variante {
    const fn nome(self) -> &'static str {
        match self {
            Self::Voz => "voz (RTP)",
            Self::A => "A (SCTP, controle)",
            Self::Fixo => "RTP fixo",
            Self::Adaptativo => "RTP adaptativo",
        }
    }

    fn de(texto: &str) -> Self {
        match texto {
            "voz" => Self::Voz,
            "a" => Self::A,
            "fixo" => Self::Fixo,
            "adaptativo" => Self::Adaptativo,
            outro => panic!("variante desconhecida: {outro}"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Cenario {
    nome: &'static str,
    perda: f64,
    banda_bps: Option<u64>,
}

const CENARIOS: [Cenario; 7] = [
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
    fn sentido(&self, jitter: Duration) -> Sentido {
        Sentido {
            perda: self.perda,
            atraso: ATRASO,
            jitter,
            banda_bps: self.banda_bps,
            // 250 ms de fila na banda do gargalo.
            fila_max: self
                .banda_bps
                .map_or(64 * 1024, |bps| (bps / 8 / 4) as usize),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Opcoes {
    segundos: u64,
    rodadas: u64,
    semente: u64,
    aquecimento: u64,
    /// `set_reordering_timeout_video`: quanto se espera uma retransmissão.
    espera: Duration,
    /// `set_reordering_size_video`: quantos quadros inteiros esperam atrás de
    /// um buraco antes de sair assim mesmo.
    reordem: usize,
    reordem_da_voz: usize,
    repetir_o_pedido: Duration,
    entre_chaves: Duration,
    inicial_bps: u64,
    sem_teto_de_rtx: bool,
    pular_apos_a_chave: u32,
    /// O jitter do cano, uniforme entre zero e isto, em cada sentido (o cano
    /// não troca a ordem). Zero na grade.
    jitter: Duration,
    /// Escreve no STDERR, segundo a segundo, o que cada célula RTP fez.
    serie: bool,
}

// ---- o quadro ----------------------------------------------------------------

/// Um quadro com forma de H.264 Annex-B, com `tamanho` bytes ao todo.
fn quadro_h264(chave: bool, id: u32, instante_us: u64, tamanho: usize) -> Vec<u8> {
    let minimo = INICIO_DE_NAL.len() * 3 + SPS.len() + PPS.len() + 1 + CAMPOS;
    let tamanho = tamanho.max(minimo);
    let mut q = Vec::with_capacity(tamanho);
    if chave {
        q.extend_from_slice(&INICIO_DE_NAL);
        q.extend_from_slice(&SPS);
        q.extend_from_slice(&INICIO_DE_NAL);
        q.extend_from_slice(&PPS);
    }
    q.extend_from_slice(&INICIO_DE_NAL);
    q.push(if chave { NAL_CHAVE } else { NAL_DELTA });
    q.extend_from_slice(MARCA);
    q.extend_from_slice(format!("{id:08x}{instante_us:016x}{tamanho:08x}").as_bytes());
    q.resize(tamanho, 0xA5);
    q
}

/// O que o receptor lê de volta: (é chave, id, instante, tamanho declarado).
fn ler_quadro_h264(dados: &[u8]) -> Option<(bool, u32, u64, usize)> {
    let onde = dados
        .get(..96.min(dados.len()))?
        .windows(MARCA.len())
        .position(|j| j == MARCA)?;
    let nal = *dados.get(onde.checked_sub(1)?)?;
    let campos = std::str::from_utf8(dados.get(onde + 4..onde + CAMPOS)?).ok()?;
    let id = u32::from_str_radix(campos.get(0..8)?, 16).ok()?;
    let instante = u64::from_str_radix(campos.get(8..24)?, 16).ok()?;
    let tamanho = usize::from_str_radix(campos.get(24..32)?, 16).ok()?;
    Some((nal & 0x1F == 5, id, instante, tamanho))
}

// ---- o que se conta -----------------------------------------------------------

/// Tudo o que os dois lados contam, acumulado desde o começo da célula. A
/// janela é a diferença entre dois retratos.
#[derive(Debug, Default, Clone, Copy)]
struct Numeros {
    // quem transmite
    gerados: u64,
    chaves: u64,
    bytes_gerados: u64,
    escritas_recusadas: u64,
    pedidos_recebidos: u64,
    pedidos_ignorados: u64,
    nacks_recebidos: u64,
    dgramas: u64,
    bytes: u64,
    dgramas_de_video: u64,
    bytes_de_video: u64,
    dgramas_de_rtx: u64,
    bytes_de_rtx: u64,
    dgramas_de_voz: u64,
    dgramas_de_rtcp: u64,
    fora_do_cano: u64,
    // quem recebe
    recebidos: u64,
    retorno_dgramas: u64,
    retorno_bytes: u64,
    pedidos: u64,
    quebras: u64,
    nao_contiguos: u64,
    desacordos: u64,
    ilegiveis: u64,
    /// Quadros que o str0m entregou com um tamanho que não é o declarado.
    tamanho_errado: u64,
    quadros_entregues: u64,
}

impl Numeros {
    fn menos(&self, antes: &Self) -> Self {
        Self {
            gerados: self.gerados - antes.gerados,
            chaves: self.chaves - antes.chaves,
            bytes_gerados: self.bytes_gerados - antes.bytes_gerados,
            escritas_recusadas: self.escritas_recusadas - antes.escritas_recusadas,
            pedidos_recebidos: self.pedidos_recebidos - antes.pedidos_recebidos,
            pedidos_ignorados: self.pedidos_ignorados - antes.pedidos_ignorados,
            nacks_recebidos: self.nacks_recebidos.saturating_sub(antes.nacks_recebidos),
            dgramas: self.dgramas - antes.dgramas,
            bytes: self.bytes - antes.bytes,
            dgramas_de_video: self.dgramas_de_video - antes.dgramas_de_video,
            bytes_de_video: self.bytes_de_video - antes.bytes_de_video,
            dgramas_de_rtx: self.dgramas_de_rtx - antes.dgramas_de_rtx,
            bytes_de_rtx: self.bytes_de_rtx - antes.bytes_de_rtx,
            dgramas_de_voz: self.dgramas_de_voz - antes.dgramas_de_voz,
            dgramas_de_rtcp: self.dgramas_de_rtcp - antes.dgramas_de_rtcp,
            fora_do_cano: self.fora_do_cano - antes.fora_do_cano,
            recebidos: self.recebidos - antes.recebidos,
            retorno_dgramas: self.retorno_dgramas - antes.retorno_dgramas,
            retorno_bytes: self.retorno_bytes - antes.retorno_bytes,
            pedidos: self.pedidos - antes.pedidos,
            quebras: self.quebras - antes.quebras,
            nao_contiguos: self.nao_contiguos - antes.nao_contiguos,
            desacordos: self.desacordos - antes.desacordos,
            ilegiveis: self.ilegiveis - antes.ilegiveis,
            tamanho_errado: self.tamanho_errado - antes.tamanho_errado,
            quadros_entregues: self.quadros_entregues - antes.quadros_entregues,
        }
    }
}

/// O que os dois lados e quem mede dividem.
#[derive(Default)]
struct Painel {
    numeros: Mutex<Numeros>,
    /// (quando ficou exibível, a latência), em microssegundos.
    exibidos: Mutex<Vec<(u64, u64)>>,
    /// O instante de envio de cada pacote de voz.
    voz_enviada: Mutex<Vec<u64>>,
    /// (instante de envio, atraso) de cada pacote de voz que chegou.
    voz_chegou: Mutex<Vec<(u64, u64)>>,
    /// A última estimativa de banda do str0m, em bits/s (0: nenhuma ainda).
    estimativa_bps: AtomicU64,
    emissor_ligado: AtomicBool,
    receptor_ligado: AtomicBool,
    transmitir: AtomicBool,
    parar_o_emissor: AtomicBool,
    parar_o_receptor: AtomicBool,
}

impl Painel {
    fn contar(&self, f: impl FnOnce(&mut Numeros)) {
        f(&mut self.numeros.lock().unwrap());
    }

    fn retrato(&self) -> Numeros {
        *self.numeros.lock().unwrap()
    }
}

/// O que uma célula (cenário × variante × rodada) mediu, já recortado na janela.
#[derive(Debug, Default, Clone)]
struct Medida {
    variante_rtp: bool,
    com_video: bool,
    exibidos: Vec<(u64, u64)>,
    n: Numeros,
    bytes_udp: u64,
    /// Datagramas que sumiram sem o cano sortear: tem de ser zero.
    sumidos: i64,
    estimativas_bps: Vec<u64>,
    voz_enviada: u64,
    voz_atrasos_us: Vec<u64>,
    ida: ContagemDoSentido,
    comeco_us: u64,
    janela_us: u64,
}

// ---- quem transmite por RTP -----------------------------------------------------

#[derive(Clone, Copy)]
struct TiposDeCarga {
    video: u8,
    rtx: Option<u8>,
    voz: u8,
}

/// O que um datagrama que sai é, pelo cabeçalho (que o SRTP não cifra).
fn classificar(dados: &[u8], tipos: Option<TiposDeCarga>, n: &mut Numeros) {
    n.dgramas += 1;
    n.bytes += dados.len() as u64;
    let (Some(primeiro), Some(segundo)) = (dados.first(), dados.get(1)) else {
        return;
    };
    // RFC 7983: 128 a 191 é RTP ou RTCP; o resto é STUN ou DTLS.
    if !(128..=191).contains(primeiro) {
        return;
    }
    // RFC 5761: os tipos 192 a 223 no segundo byte são RTCP.
    if (192..=223).contains(segundo) {
        n.dgramas_de_rtcp += 1;
        return;
    }
    let Some(tipos) = tipos else { return };
    let tipo = segundo & 0x7F;
    if tipo == tipos.video {
        n.dgramas_de_video += 1;
        n.bytes_de_video += dados.len() as u64;
    } else if Some(tipo) == tipos.rtx {
        n.dgramas_de_rtx += 1;
        n.bytes_de_rtx += dados.len() as u64;
    } else if tipo == tipos.voz {
        n.dgramas_de_voz += 1;
    }
}

/// Entrega ao `Rtc` o que chegou no socket, sem esperar: até 64 datagramas.
fn receber_o_que_ha(
    rtc: &mut Rtc,
    socket: &UdpSocket,
    local: SocketAddr,
    buffer: &mut [u8],
    aceitar: impl Fn(SocketAddr) -> bool,
    mut contar: impl FnMut(),
) {
    for _ in 0..64 {
        let Ok((tamanho, de)) = socket.try_recv_from(buffer) else {
            return;
        };
        if !aceitar(de) {
            continue;
        }
        contar();
        let Ok(recebido) = Receive::new(Protocol::Udp, de, local, &buffer[..tamanho]) else {
            continue;
        };
        let _ = rtc.handle_input(Input::Receive(Instant::now(), recebido));
    }
}

async fn transmitir_por_rtp(
    mut rtc: Rtc,
    socket: UdpSocket,
    cano: SocketAddr,
    mid_do_video: Mid,
    mid_da_voz: Mid,
    variante: Variante,
    op: Opcoes,
    painel: Arc<Painel>,
) {
    let local = socket.local_addr().unwrap();
    let periodo = Duration::from_secs(1) / QUADROS_POR_SEGUNDO;
    let mut buffer = vec![0_u8; 2000];
    let mut tipos: Option<TiposDeCarga> = None;
    let mut teto_de_rtx_tirado = false;
    let mut proximo_quadro: Option<Instant> = None;
    let mut proxima_voz: Option<Instant> = None;
    let mut comeco: Option<(Instant, u64)> = None;
    let mut id = 0_u32;
    let mut numero_da_voz = 0_u32;
    let mut quer_chave = true;
    let mut ultima_chave: Option<Instant> = None;
    let mut pular = 0_u32;

    loop {
        if painel.parar_o_emissor.load(Ordering::Acquire) {
            return;
        }
        let agora = Instant::now();

        if tipos.is_none() && painel.emissor_ligado.load(Ordering::Acquire) {
            let do_video = rtc.writer(mid_do_video).and_then(|w| {
                w.payload_params()
                    .find(|p| p.spec().codec == Codec::H264)
                    .map(|p| (*p.pt(), p.resend().map(|r| *r)))
            });
            let da_voz = rtc.writer(mid_da_voz).and_then(|w| {
                w.payload_params()
                    .find(|p| p.spec().codec == Codec::Opus)
                    .map(|p| *p.pt())
            });
            if let (Some((video, rtx)), Some(voz)) = (do_video, da_voz) {
                tipos = Some(TiposDeCarga { video, rtx, voz });
            }
        }
        if op.sem_teto_de_rtx && !teto_de_rtx_tirado {
            if let Some(fluxo) = rtc.direct_api().stream_tx_by_mid(mid_do_video, None) {
                // Os valores padrão do str0m (1024 pacotes, 3 s), sem o teto de
                // 15% de retransmissão sobre o que sai.
                fluxo.set_rtx_cache(1024, Duration::from_secs(3), None);
                teto_de_rtx_tirado = true;
            }
        }

        if let (Some(tipos), true) = (tipos, painel.transmitir.load(Ordering::Acquire)) {
            let (zero, zero_us) = *comeco.get_or_insert((agora, agora_us()));
            // A voz.
            let voz = proxima_voz.get_or_insert(agora);
            if *voz <= agora {
                let instante = agora_us();
                let mut pacote = Vec::with_capacity(VOZ);
                pacote.push(0xF8);
                pacote.extend_from_slice(&numero_da_voz.to_be_bytes());
                pacote.extend_from_slice(&instante.to_be_bytes());
                pacote.resize(VOZ, 0x5A);
                let tempo = MediaTime::new(
                    u64::from(numero_da_voz) * 960,
                    str0m::media::Frequency::FORTY_EIGHT_KHZ,
                );
                numero_da_voz += 1;
                if let Some(escritor) = rtc.writer(mid_da_voz) {
                    if escritor
                        .write(tipos.voz.into(), agora, tempo, pacote)
                        .is_ok()
                    {
                        painel.voz_enviada.lock().unwrap().push(instante);
                    }
                }
                *voz += VOZ_A_CADA;
                if *voz <= agora {
                    // Atrasou mais de um período: os pacotes do intervalo não saem.
                    *voz = agora + VOZ_A_CADA;
                }
            }
            // O vídeo.
            let quadro = proximo_quadro.get_or_insert(agora);
            if variante != Variante::Voz && *quadro <= agora {
                *quadro += periodo;
                if *quadro <= agora {
                    // A captura não volta no tempo: o quadro seguinte é fresco.
                    *quadro = agora + periodo;
                }
                if pular > 0 {
                    pular -= 1;
                } else {
                    let delta = if variante == Variante::Adaptativo {
                        let estimativa = match painel.estimativa_bps.load(Ordering::Relaxed) {
                            0 => op.inicial_bps,
                            bps => bps,
                        };
                        let alvo = estimativa.clamp(PISO_BPS, TETO_BPS) as f64 * FRACAO;
                        (alvo / f64::from(QUADROS_POR_SEGUNDO) / 8.0) as usize
                    } else {
                        DELTA
                    };
                    let chave = quer_chave;
                    let tamanho = match (chave, variante) {
                        (false, _) => delta,
                        (true, Variante::Adaptativo) => delta * CHAVE_EM_DELTAS,
                        (true, _) => CHAVE,
                    };
                    let instante = agora_us();
                    let dados = quadro_h264(chave, id, instante, tamanho);
                    let tamanho = dados.len() as u64;
                    // 90 kHz, contado do começo da transmissão.
                    let tempo = MediaTime::from_90khz((instante - zero_us) * 9 / 100);
                    let escrito = rtc
                        .writer(mid_do_video)
                        .is_some_and(|w| w.write(tipos.video.into(), agora, tempo, dados).is_ok());
                    if escrito {
                        id += 1;
                        if chave {
                            quer_chave = false;
                            ultima_chave = Some(agora);
                            if variante == Variante::Adaptativo {
                                pular = op.pular_apos_a_chave;
                            }
                        }
                        painel.contar(|n| {
                            n.gerados += 1;
                            n.bytes_gerados += tamanho;
                            n.chaves += u64::from(chave);
                        });
                    } else {
                        painel.contar(|n| n.escritas_recusadas += 1);
                    }
                }
            }
            let _ = zero;
        }

        let _ = rtc.handle_input(Input::Timeout(agora));
        let prazo = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(quando)) => break quando,
                Ok(Output::Transmit(t)) => {
                    if t.destination == cano {
                        painel.contar(|n| classificar(&t.contents, tipos, n));
                        let _ = socket.send_to(&t.contents, t.destination).await;
                    } else {
                        painel.contar(|n| n.fora_do_cano += 1);
                    }
                }
                Ok(Output::Event(evento)) => match evento {
                    Event::Connected => painel.emissor_ligado.store(true, Ordering::Release),
                    Event::KeyframeRequest(_) => {
                        // Uma chave que saiu há pouco ainda está a caminho (ou
                        // sendo completada por retransmissão): o pedido é dela.
                        let recente = ultima_chave.is_some_and(|c| c.elapsed() < op.entre_chaves);
                        painel.contar(|n| {
                            n.pedidos_recebidos += 1;
                            n.pedidos_ignorados += u64::from(recente);
                        });
                        if !recente {
                            quer_chave = true;
                        }
                    }
                    Event::EgressBitrateEstimate(BweKind::Twcc { estimate, .. }) => {
                        painel
                            .estimativa_bps
                            .store(estimate.as_u64(), Ordering::Relaxed);
                    }
                    Event::MediaEgressStats(s) if s.mid == mid_do_video => {
                        painel.contar(|n| n.nacks_recebidos = s.nacks);
                    }
                    _ => {}
                },
                Err(_) => return,
            }
        };

        let mut acordar = prazo.min(agora + Duration::from_millis(20));
        for outro in [proximo_quadro, proxima_voz].into_iter().flatten() {
            acordar = acordar.min(outro);
        }
        tokio::select! {
            pronto = socket.readable() => {
                if pronto.is_err() {
                    return;
                }
                receber_o_que_ha(&mut rtc, &socket, local, &mut buffer, |de| de == cano, || {});
            }
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(acordar)) => {}
        }
    }
}

// ---- quem recebe por RTP --------------------------------------------------------

/// O decodificador de mentira: decide o que é exibível.
struct Tela {
    ultimo: Option<u32>,
    quebrada: bool,
    ultimo_pedido: Option<Instant>,
    /// Escreve no STDERR cada quadro que não tem o tamanho que declara.
    detalhar: bool,
}

impl Tela {
    fn quadro(&mut self, dados: &MediaData, painel: &Painel) {
        let lido = ler_quadro_h264(&dados.data);
        let contiguo = dados.contiguous;
        painel.contar(|n| {
            n.quadros_entregues += 1;
            n.nao_contiguos += u64::from(!contiguo);
            n.ilegiveis += u64::from(lido.is_none());
        });
        let Some((chave, id, instante, tamanho)) = lido else {
            self.quebrar(painel);
            return;
        };
        if self.ultimo.is_some_and(|u| id <= u) {
            return;
        }
        let inteiro = dados.data.len() == tamanho;
        if !inteiro {
            painel.contar(|n| n.tamanho_errado += 1);
            if self.detalhar {
                eprintln!(
                    "    quadro {id} ({}) com {} bytes, declarava {tamanho}; contíguo: {contiguo}; \
                     último exibido: {:?}; pacotes {:?}",
                    if chave { "chave" } else { "delta" },
                    dados.data.len(),
                    self.ultimo,
                    dados.seq_range,
                );
            }
        }
        // A cadeia segue se este é o quadro seguinte ao último exibido.
        let segue = self.ultimo.is_some_and(|u| id == u + 1);
        if !chave && segue != contiguo && !self.quebrada {
            // O str0m e o número do quadro não concordam sobre o buraco.
            painel.contar(|n| n.desacordos += 1);
        }
        let exibivel = inteiro && (chave || (segue && !self.quebrada));
        if exibivel {
            let agora = agora_us();
            painel
                .exibidos
                .lock()
                .unwrap()
                .push((agora, agora.saturating_sub(instante)));
            self.ultimo = Some(id);
            self.quebrada = false;
        } else {
            self.quebrar(painel);
        }
    }

    fn quebrar(&mut self, painel: &Painel) {
        if !self.quebrada {
            self.quebrada = true;
            // O primeiro pedido sai já, sem esperar o intervalo.
            self.ultimo_pedido = None;
            painel.contar(|n| n.quebras += 1);
        }
    }
}

async fn receber_por_rtp(
    mut rtc: Rtc,
    socket: UdpSocket,
    emissor: SocketAddr,
    mid_do_video: Mid,
    mid_da_voz: Mid,
    op: Opcoes,
    painel: Arc<Painel>,
) {
    let local = socket.local_addr().unwrap();
    let mut buffer = vec![0_u8; 2000];
    let mut tela = Tela {
        ultimo: None,
        quebrada: false,
        ultimo_pedido: None,
        detalhar: op.serie,
    };
    let mut vozes_vistas: HashSet<u32> = HashSet::new();

    loop {
        if painel.parar_o_receptor.load(Ordering::Acquire) {
            return;
        }
        let agora = Instant::now();
        if tela.quebrada
            && tela
                .ultimo_pedido
                .is_none_or(|quando| agora.duration_since(quando) >= op.repetir_o_pedido)
        {
            let pedido = rtc
                .writer(mid_do_video)
                .is_some_and(|mut w| w.request_keyframe(None, KeyframeRequestKind::Pli).is_ok());
            if pedido {
                tela.ultimo_pedido = Some(agora);
                painel.contar(|n| n.pedidos += 1);
            }
        }

        let _ = rtc.handle_input(Input::Timeout(agora));
        let prazo = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(quando)) => break quando,
                Ok(Output::Transmit(t)) => {
                    // Quem recebe só conhece o socket do cano. Um datagrama
                    // direto a quem transmite seria um atalho por fora dele.
                    if t.destination == emissor {
                        painel.contar(|n| n.fora_do_cano += 1);
                    } else {
                        painel.contar(|n| {
                            n.retorno_dgramas += 1;
                            n.retorno_bytes += t.contents.len() as u64;
                        });
                        let _ = socket.send_to(&t.contents, t.destination).await;
                    }
                }
                Ok(Output::Event(evento)) => match evento {
                    Event::Connected => painel.receptor_ligado.store(true, Ordering::Release),
                    Event::MediaData(dados) if dados.mid == mid_do_video => {
                        tela.quadro(&dados, &painel);
                    }
                    Event::MediaData(dados)
                        if dados.mid == mid_da_voz && dados.data.len() == VOZ =>
                    {
                        let n = u32::from_be_bytes(dados.data[1..5].try_into().unwrap());
                        let saiu = u64::from_be_bytes(dados.data[5..13].try_into().unwrap());
                        if vozes_vistas.insert(n) {
                            painel
                                .voz_chegou
                                .lock()
                                .unwrap()
                                .push((saiu, agora_us().saturating_sub(saiu)));
                        }
                    }
                    _ => {}
                },
                Err(_) => return,
            }
        };

        let acordar = prazo.min(agora + Duration::from_millis(20));
        tokio::select! {
            pronto = socket.readable() => {
                if pronto.is_err() {
                    return;
                }
                receber_o_que_ha(
                    &mut rtc,
                    &socket,
                    local,
                    &mut buffer,
                    |de| de != emissor,
                    || painel.contar(|n| n.recebidos += 1),
                );
            }
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(acordar)) => {}
        }
    }
}

// ---- uma célula por RTP ---------------------------------------------------------

async fn socket_local() -> UdpSocket {
    let socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    // Uma chave de 80 KiB sai de uma vez: o buffer não pode ser o gargalo.
    let _ = socket2::SockRef::from(&socket).set_recv_buffer_size(4 * 1024 * 1024);
    let _ = socket2::SockRef::from(&socket).set_send_buffer_size(1024 * 1024);
    socket
}

async fn medir_rtp(cenario: Cenario, variante: Variante, semente: u64, op: Opcoes) -> Medida {
    let socket_de_quem_transmite = socket_local().await;
    let socket_de_quem_recebe = socket_local().await;
    let endereco_de_quem_transmite = socket_de_quem_transmite.local_addr().unwrap();
    let endereco_de_quem_recebe = socket_de_quem_recebe.local_addr().unwrap();
    let cano = Cano::abrir(
        endereco_de_quem_recebe,
        ConfigDoCano::simetrico(cenario.sentido(op.jitter), semente),
    )
    .await
    .unwrap();

    let config = || {
        config_de_rtc_cru()
            .clear_codecs()
            .enable_h264(true)
            .enable_opus(true, false)
            .set_stats_interval(Some(Duration::from_millis(500)))
            .set_reordering_size_video(op.reordem)
            .set_reordering_timeout_video(Some(op.espera))
            .set_reordering_size_audio(op.reordem_da_voz)
    };
    let agora = Instant::now();
    let mut quem_transmite = if variante == Variante::Adaptativo {
        config()
            .enable_bwe(Some(Bitrate::bps(op.inicial_bps)))
            .build(agora)
    } else {
        config().build(agora)
    };
    let mut quem_recebe = config().build(agora);
    if variante == Variante::Adaptativo {
        quem_transmite
            .bwe()
            .set_desired_bitrate(Bitrate::bps(TETO_BPS));
    }

    // A oferta e a resposta, em memória e **antes** de qualquer candidato
    // local: assim nenhuma das duas carrega um endereço, e os dois lados só
    // se acham pelo cano.
    let mut mudanca = quem_transmite.sdp_api();
    let mid_do_video = mudanca.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
    let mid_da_voz = mudanca.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
    let (oferta, pendente) = mudanca.apply().unwrap();
    let resposta = quem_recebe.sdp_api().accept_offer(oferta).unwrap();
    quem_transmite
        .sdp_api()
        .accept_answer(pendente, resposta)
        .unwrap();
    quem_transmite.add_local_candidate(Candidate::host(endereco_de_quem_transmite, "udp").unwrap());
    quem_recebe.add_local_candidate(Candidate::host(endereco_de_quem_recebe, "udp").unwrap());
    quem_transmite.add_remote_candidate(Candidate::host(cano.endereco(), "udp").unwrap());

    let painel = Arc::new(Painel::default());
    let tarefas = [
        tokio::spawn(transmitir_por_rtp(
            quem_transmite,
            socket_de_quem_transmite,
            cano.endereco(),
            mid_do_video,
            mid_da_voz,
            variante,
            op,
            Arc::clone(&painel),
        )),
        tokio::spawn(receber_por_rtp(
            quem_recebe,
            socket_de_quem_recebe,
            endereco_de_quem_transmite,
            mid_do_video,
            mid_da_voz,
            op,
            Arc::clone(&painel),
        )),
    ];

    let limite = Instant::now() + Duration::from_secs(60);
    while !(painel.emissor_ligado.load(Ordering::Acquire)
        && painel.receptor_ligado.load(Ordering::Acquire))
    {
        assert!(Instant::now() < limite, "o par RTP não ligou em 60 s");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // Uma folga para o aperto assentar antes de o relógio da medida começar.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Sem aquecimento, a janela abre **antes** de a transmissão começar: o
    // primeiro quadro (a chave) tem de cair dentro dela.
    if op.aquecimento > 0 {
        painel.transmitir.store(true, Ordering::Release);
        tokio::time::sleep(Duration::from_secs(op.aquecimento)).await;
    }
    let comeco_us = agora_us();
    let antes = painel.retrato();
    let ida_antes = cano.ida();
    painel.transmitir.store(true, Ordering::Release);
    let mut estimativas_bps = Vec::new();
    let fim = tokio::time::Instant::now() + Duration::from_secs(op.segundos);
    let mut amostra = tokio::time::interval(AMOSTRA);
    amostra.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut amostras = 0_u64;
    let mut anterior = (antes, 0_usize);
    while tokio::time::Instant::now() < fim {
        amostra.tick().await;
        if variante == Variante::Adaptativo {
            estimativas_bps.push(painel.estimativa_bps.load(Ordering::Relaxed));
        }
        amostras += 1;
        if op.serie && amostras.is_multiple_of(4) {
            // Segundo a segundo, para investigar uma célula: não muda a medida.
            let agora = painel.retrato();
            let exibidos = painel.exibidos.lock().unwrap().len();
            let d = agora.menos(&anterior.0);
            eprintln!(
                "    t={:>2} s · est. {:>5} kbps · exibidos {:>2} · gerados {:>2} ({:>4} kbps) · \
                 chaves {} · pedidos {} · quebras {} · vídeo {:>3} + rtx {:>3} datagramas · NACKs {}",
                amostras / 4,
                painel.estimativa_bps.load(Ordering::Relaxed) / 1000,
                exibidos - anterior.1,
                d.gerados,
                d.bytes_gerados * 8 / 1000,
                d.chaves,
                d.pedidos,
                d.quebras,
                d.dgramas_de_video,
                d.dgramas_de_rtx,
                d.nacks_recebidos,
            );
            anterior = (agora, exibidos);
        }
    }
    let fim_us = agora_us();
    let depois = painel.retrato();
    let ida_depois = cano.ida();
    let volta = cano.volta();

    // Quem transmite para; um segundo para o que está a caminho; quem recebe para.
    painel.parar_o_emissor.store(true, Ordering::Release);
    tokio::time::sleep(Duration::from_secs(1)).await;
    painel.parar_o_receptor.store(true, Ordering::Release);
    for tarefa in tarefas {
        let _ = tokio::time::timeout(Duration::from_secs(2), tarefa).await;
    }
    let no_fim = painel.retrato();
    let ida_no_fim = cano.ida();
    // A conferência do cano, sobre a célula inteira: o que saiu de quem
    // transmite tem de ser o que o cano viu, e o que o cano repassou tem de
    // ser o que quem recebe leu.
    let visto_pelo_cano = ida_no_fim.repassados + ida_no_fim.descartados();
    let sumidos = (no_fim.dgramas as i64 - visto_pelo_cano as i64).abs()
        + (ida_no_fim.repassados as i64 - no_fim.recebidos as i64).abs();

    let janela = |quando: u64| (comeco_us..=fim_us).contains(&quando);
    let exibidos: Vec<(u64, u64)> = painel
        .exibidos
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|(quando, _)| janela(*quando))
        .collect();
    let voz_enviada = painel
        .voz_enviada
        .lock()
        .unwrap()
        .iter()
        .filter(|saiu| janela(**saiu))
        .count() as u64;
    let voz_atrasos_us: Vec<u64> = painel
        .voz_chegou
        .lock()
        .unwrap()
        .iter()
        .filter(|(saiu, _)| janela(*saiu))
        .map(|(_, atraso)| *atraso)
        .collect();
    let n = depois.menos(&antes);
    eprintln!(
        "  {} · {} · semente {semente}: gerado {:.2} MB, no fio vídeo {:.2} MB + rtx {:.2} MB, \
         tudo {:.2} MB; cano repassou {:.2} MB ({} de {} datagramas); sumidos {sumidos}; \
         fora do cano {}; não contíguos {}, desacordos {}, ilegíveis {}, tamanho errado {}; \
         volta: {} perdidos de {} (a célula inteira); pedidos de chave ignorados {}; \
         escritas recusadas {}",
        cenario.nome,
        variante.nome(),
        n.bytes_gerados as f64 / 1e6,
        n.bytes_de_video as f64 / 1e6,
        n.bytes_de_rtx as f64 / 1e6,
        n.bytes as f64 / 1e6,
        (ida_depois.bytes_repassados - ida_antes.bytes_repassados) as f64 / 1e6,
        ida_depois.repassados - ida_antes.repassados,
        n.dgramas,
        no_fim.fora_do_cano,
        n.nao_contiguos,
        n.desacordos,
        n.ilegiveis,
        n.tamanho_errado,
        volta.perdidos_no_sorteio,
        volta.repassados + volta.descartados(),
        n.pedidos_ignorados,
        n.escritas_recusadas,
    );
    Medida {
        variante_rtp: true,
        com_video: variante != Variante::Voz,
        exibidos,
        n,
        bytes_udp: n.bytes,
        sumidos: sumidos + no_fim.fora_do_cano as i64,
        estimativas_bps,
        voz_enviada,
        voz_atrasos_us,
        ida: ContagemDoSentido {
            repassados: ida_depois.repassados - ida_antes.repassados,
            bytes_repassados: ida_depois.bytes_repassados - ida_antes.bytes_repassados,
            perdidos_no_sorteio: ida_depois.perdidos_no_sorteio - ida_antes.perdidos_no_sorteio,
            descartados_na_fila: ida_depois.descartados_na_fila - ida_antes.descartados_na_fila,
        },
        comeco_us,
        janela_us: fim_us - comeco_us,
    }
}

// ---- o controle: a linha «A» de `tela_sob_perda.rs`, copiada ----------------------

fn quadro_no_fluxo(tipo: u8, id: u32, instante: u64, tamanho: usize) -> Vec<u8> {
    let mut q = Vec::with_capacity(CABECALHO_DO_QUADRO + tamanho);
    q.push(tipo);
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&instante.to_be_bytes());
    q.extend_from_slice(&(tamanho as u32).to_be_bytes());
    q.resize(CABECALHO_DO_QUADRO + tamanho, 0xA5);
    q
}

/// (A): um quadro por vez no `u`. O `interval` com `Skip` é a captura: quem
/// escreve atrasado perde os quadros do intervalo, e o próximo é fresco.
async fn emitir_no_fluxo(mut envio: Envio, painel: Arc<Painel>) {
    let mut captura = tokio::time::interval(Duration::from_secs(1) / QUADROS_POR_SEGUNDO);
    captura.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut id = 0_u32;
    loop {
        captura.tick().await;
        let (tipo, tamanho) = if id == 0 { (1, CHAVE) } else { (0, DELTA) };
        let quadro = quadro_no_fluxo(tipo, id, agora_us(), tamanho);
        if envio.write_all(&quadro).await.is_err() {
            return;
        }
        painel.contar(|n| {
            n.gerados += 1;
            n.bytes_gerados += quadro.len() as u64;
            n.chaves += u64::from(tipo == 1);
        });
        id += 1;
    }
}

/// (A): cada quadro lido inteiro é exibível.
async fn receber_do_fluxo(mut recebe: Recebe, painel: Arc<Painel>) {
    loop {
        let mut cabecalho = [0_u8; CABECALHO_DO_QUADRO];
        if recebe.read_exact(&mut cabecalho).await.is_err() {
            return;
        }
        let instante = u64::from_be_bytes(cabecalho[5..13].try_into().unwrap());
        let tamanho = u32::from_be_bytes(cabecalho[13..17].try_into().unwrap()) as usize;
        let mut corpo = vec![0_u8; tamanho];
        if recebe.read_exact(&mut corpo).await.is_err() {
            return;
        }
        let agora = agora_us();
        painel
            .exibidos
            .lock()
            .unwrap()
            .push((agora, agora.saturating_sub(instante)));
    }
}

async fn emitir_voz_no_enlace(conexao: Conexao, painel: Arc<Painel>) {
    let mut passo = tokio::time::interval(VOZ_A_CADA);
    passo.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut n = 0_u32;
    loop {
        passo.tick().await;
        let instante = agora_us();
        let mut d = Vec::with_capacity(VOZ);
        d.push(1);
        d.extend_from_slice(&n.to_be_bytes());
        d.extend_from_slice(&instante.to_be_bytes());
        d.resize(VOZ, 0);
        n += 1;
        // Recusada ou não, conta como enviada: o que não saiu é perda.
        let _ = conexao.send_datagram(d);
        painel.voz_enviada.lock().unwrap().push(instante);
    }
}

async fn receber_voz_do_enlace(conexao: Conexao, painel: Arc<Painel>) {
    let mut vistos = HashSet::new();
    while let Ok(d) = conexao.read_datagram().await {
        if d.len() != VOZ {
            continue;
        }
        let n = u32::from_be_bytes(d[1..5].try_into().unwrap());
        let saiu = u64::from_be_bytes(d[5..13].try_into().unwrap());
        if vistos.insert(n) {
            painel
                .voz_chegou
                .lock()
                .unwrap()
                .push((saiu, agora_us().saturating_sub(saiu)));
        }
    }
}

async fn medir_a(cenario: Cenario, semente: u64, op: Opcoes) -> Medida {
    let config = ConfigDoCano::simetrico(cenario.sentido(op.jitter), semente);
    let (par, cano) = tokio::time::timeout(Duration::from_secs(60), par_por_cano(config))
        .await
        .expect("o par não abriu pelo cano em 60 s");
    let painel = Arc::new(Painel::default());
    let envio = par.cliente.open_uni().await.unwrap();
    let recebe = tokio::time::timeout(Duration::from_secs(30), par.anfitriao.accept_uni())
        .await
        .expect("o u não chegou")
        .unwrap();
    let mut tarefas = vec![
        tokio::spawn(receber_do_fluxo(recebe, Arc::clone(&painel))),
        tokio::spawn(emitir_no_fluxo(envio, Arc::clone(&painel))),
        tokio::spawn(receber_voz_do_enlace(
            par.anfitriao.clone(),
            Arc::clone(&painel),
        )),
    ];
    let falante = tokio::spawn(emitir_voz_no_enlace(
        par.cliente.clone(),
        Arc::clone(&painel),
    ));
    tokio::time::sleep(Duration::from_secs(op.aquecimento)).await;

    let comeco_us = agora_us();
    let antes = painel.retrato();
    let ida_antes = cano.ida();
    let bytes_antes = par.cliente.leitura().bytes_enviados;
    tokio::time::sleep(Duration::from_secs(op.segundos)).await;
    let fim_us = agora_us();
    let depois = painel.retrato();
    let ida_depois = cano.ida();
    let bytes_udp = par.cliente.leitura().bytes_enviados - bytes_antes;
    falante.abort();
    // Um segundo a mais para a voz que ainda está a caminho.
    tokio::time::sleep(Duration::from_secs(1)).await;
    for tarefa in tarefas.drain(..) {
        tarefa.abort();
    }
    par.cliente.close(0, b"fim");
    par.anfitriao.close(0, b"fim");

    let janela = |quando: u64| (comeco_us..=fim_us).contains(&quando);
    let exibidos = painel
        .exibidos
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|(quando, _)| janela(*quando))
        .collect();
    let voz_enviada = painel
        .voz_enviada
        .lock()
        .unwrap()
        .iter()
        .filter(|saiu| janela(**saiu))
        .count() as u64;
    let voz_atrasos_us = painel
        .voz_chegou
        .lock()
        .unwrap()
        .iter()
        .filter(|(saiu, _)| janela(*saiu))
        .map(|(_, atraso)| *atraso)
        .collect();
    let ida = ContagemDoSentido {
        repassados: ida_depois.repassados - ida_antes.repassados,
        bytes_repassados: ida_depois.bytes_repassados - ida_antes.bytes_repassados,
        perdidos_no_sorteio: ida_depois.perdidos_no_sorteio - ida_antes.perdidos_no_sorteio,
        descartados_na_fila: ida_depois.descartados_na_fila - ida_antes.descartados_na_fila,
    };
    eprintln!(
        "  {} · A · semente {semente}: gerado {:.2} MB, UDP de quem liga {:.2} MB; cano repassou {:.2} MB",
        cenario.nome,
        depois.menos(&antes).bytes_gerados as f64 / 1e6,
        bytes_udp as f64 / 1e6,
        ida.bytes_repassados as f64 / 1e6,
    );
    Medida {
        variante_rtp: false,
        com_video: true,
        exibidos,
        n: depois.menos(&antes),
        bytes_udp,
        sumidos: 0,
        estimativas_bps: Vec::new(),
        voz_enviada,
        voz_atrasos_us,
        ida,
        comeco_us,
        janela_us: fim_us - comeco_us,
    }
}

// ---- as contas ----------------------------------------------------------------

fn percentil(ordenado: &[f64], p: f64) -> f64 {
    if ordenado.is_empty() {
        return f64::NAN;
    }
    let posicao = ((p * ordenado.len() as f64).ceil() as usize).clamp(1, ordenado.len());
    ordenado[posicao - 1]
}

const COLUNAS: usize = 25;
const CABECALHO: &str = "| lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |";
const REGUA: &str =
    "|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|";
/// As casas decimais de cada coluna.
const CASAS: [usize; COLUNAS] = [
    0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 2, 0, 0, 2, 2,
];
/// As colunas que só existem com vídeo, e as que só existem por RTP.
const DO_VIDEO: std::ops::Range<usize> = 0..18;
const DO_RTP: [usize; 6] = [10, 11, 12, 13, 14, 19];
const DA_ESTIMATIVA: std::ops::Range<usize> = 15..18;

/// Os números de uma célula, na ordem das colunas.
fn linha(m: &Medida) -> [f64; COLUNAS] {
    let fim = m.comeco_us + m.janela_us;
    let segundos = m.janela_us as f64 / 1e6;
    let mut latencias: Vec<f64> = m.exibidos.iter().map(|(_, l)| *l as f64 / 1000.0).collect();
    latencias.sort_by(f64::total_cmp);
    let mut instantes: Vec<u64> = m.exibidos.iter().map(|(quando, _)| *quando).collect();
    instantes.sort_unstable();
    let mut congelado = 0_u64;
    let mut maior = 0_u64;
    let mut antes = m.comeco_us;
    for quando in instantes.iter().copied().chain([fim]) {
        let intervalo = quando.saturating_sub(antes);
        maior = maior.max(intervalo);
        if intervalo > CONGELADO.as_micros() as u64 {
            congelado += intervalo;
        }
        antes = quando;
    }
    let mut voz: Vec<f64> = m
        .voz_atrasos_us
        .iter()
        .map(|a| *a as f64 / 1000.0)
        .collect();
    voz.sort_by(f64::total_cmp);
    let mut estimativas: Vec<f64> = m
        .estimativas_bps
        .iter()
        .filter(|e| **e > 0)
        .map(|e| *e as f64 / 1000.0)
        .collect();
    let estimativa_final = estimativas.last().copied().unwrap_or(f64::NAN);
    estimativas.sort_by(f64::total_cmp);
    let ida_total = m.ida.repassados + m.ida.descartados();
    let mut valores = [
        percentil(&latencias, 0.50),
        percentil(&latencias, 0.95),
        percentil(&latencias, 0.99),
        latencias.last().copied().unwrap_or(f64::NAN),
        100.0 * congelado as f64 / m.janela_us as f64,
        maior as f64 / 1000.0,
        latencias.len() as f64 / segundos,
        m.n.gerados as f64,
        m.n.bytes_gerados as f64 * 8.0 / 1000.0 / segundos,
        m.n.chaves as f64,
        m.n.pedidos as f64,
        m.n.quebras as f64,
        m.n.nacks_recebidos as f64,
        m.n.dgramas_de_rtx as f64,
        100.0 * m.n.bytes_de_rtx as f64 / m.n.bytes_de_video.max(1) as f64,
        estimativas.first().copied().unwrap_or(f64::NAN),
        percentil(&estimativas, 0.50),
        estimativa_final,
        m.bytes_udp as f64 / 1e6,
        m.n.retorno_bytes as f64 * 8.0 / 1000.0 / segundos,
        100.0 * (1.0 - voz.len() as f64 / m.voz_enviada.max(1) as f64),
        percentil(&voz, 0.50),
        percentil(&voz, 0.95),
        100.0 * m.ida.perdidos_no_sorteio as f64 / ida_total.max(1) as f64,
        100.0 * m.ida.descartados_na_fila as f64 / ida_total.max(1) as f64,
    ];
    for (coluna, valor) in valores.iter_mut().enumerate() {
        let sem_video = !m.com_video && DO_VIDEO.contains(&coluna);
        let sem_rtp = !m.variante_rtp && DO_RTP.contains(&coluna);
        let sem_estimativa = m.estimativas_bps.is_empty() && DA_ESTIMATIVA.contains(&coluna);
        if sem_video || sem_rtp || sem_estimativa {
            *valor = f64::NAN;
        }
    }
    valores
}

fn celula(valor: f64, casas: usize) -> String {
    if valor.is_nan() {
        "—".to_owned()
    } else {
        format!("{valor:.casas$}").replace('.', ",")
    }
}

fn em_colunas(valores: &[f64; COLUNAS]) -> String {
    valores
        .iter()
        .zip(CASAS)
        .map(|(v, casas)| celula(*v, casas))
        .collect::<Vec<_>>()
        .join(" | ")
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

fn faixa(valores: &[f64], casas: usize) -> String {
    let validos: Vec<f64> = valores.iter().copied().filter(|v| !v.is_nan()).collect();
    if validos.is_empty() {
        return "—".to_owned();
    }
    let menor = validos.iter().copied().fold(f64::INFINITY, f64::min);
    let maior = validos.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    format!("{}–{}", celula(menor, casas), celula(maior, casas))
}

fn opcoes() -> (Opcoes, Vec<Variante>, Option<String>, Option<f64>) {
    let mut op = Opcoes {
        segundos: 20,
        rodadas: 3,
        semente: 20_261_110,
        aquecimento: 0,
        espera: Duration::from_secs(2),
        reordem: 1000,
        reordem_da_voz: 0,
        repetir_o_pedido: Duration::from_millis(300),
        entre_chaves: Duration::from_millis(300),
        inicial_bps: 300_000,
        sem_teto_de_rtx: false,
        pular_apos_a_chave: 3,
        jitter: Duration::ZERO,
        serie: false,
    };
    let mut variantes = vec![Variante::Fixo, Variante::Adaptativo];
    let mut so = None;
    let mut perda = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut numero = |nome: &str| -> u64 {
            args.next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("{nome} pede um número"))
        };
        match arg.as_str() {
            "--segundos" => op.segundos = numero("--segundos"),
            "--rodadas" => op.rodadas = numero("--rodadas"),
            "--semente" => op.semente = numero("--semente"),
            "--aquecimento" => op.aquecimento = numero("--aquecimento"),
            "--espera-ms" => op.espera = Duration::from_millis(numero("--espera-ms")),
            "--reordem" => op.reordem = numero("--reordem") as usize,
            "--reordem-voz" => op.reordem_da_voz = numero("--reordem-voz") as usize,
            "--pli-ms" => op.repetir_o_pedido = Duration::from_millis(numero("--pli-ms")),
            "--entre-chaves-ms" => {
                op.entre_chaves = Duration::from_millis(numero("--entre-chaves-ms"));
            }
            "--inicial-bps" => op.inicial_bps = numero("--inicial-bps"),
            "--pular" => op.pular_apos_a_chave = numero("--pular") as u32,
            "--jitter-ms" => op.jitter = Duration::from_millis(numero("--jitter-ms")),
            "--sem-teto-rtx" => op.sem_teto_de_rtx = true,
            "--serie" => op.serie = true,
            "--so" => so = args.next(),
            "--perda" => perda = args.next().and_then(|v| v.parse().ok()),
            "--variante" => {
                variantes = args
                    .next()
                    .expect("--variante pede um nome")
                    .split(',')
                    .map(Variante::de)
                    .collect();
            }
            outro => panic!("opção desconhecida: {outro}"),
        }
    }
    (op, variantes, so, perda)
}

#[tokio::main]
async fn main() {
    let (op, variantes, so, perda) = opcoes();
    // `--so` com o nome exato escolhe um cenário só («3%» não traz o «0,3%»);
    // sem nome exato, vale o pedaço.
    let exato = so
        .as_ref()
        .is_some_and(|s| CENARIOS.iter().any(|c| c.nome == s));
    let cenarios: Vec<Cenario> = match perda {
        // Um cenário só, fora da grade: a perda dada, 30 ms, sem limite de banda.
        Some(perda) => vec![Cenario {
            nome: Box::leak(
                format!("{}%", perda * 100.0)
                    .replace('.', ",")
                    .into_boxed_str(),
            ),
            perda,
            banda_bps: None,
        }],
        None => CENARIOS
            .into_iter()
            .filter(|c| {
                so.as_ref().is_none_or(|s| {
                    if exato {
                        c.nome == s
                    } else {
                        c.nome.contains(s.as_str())
                    }
                })
            })
            .collect(),
    };
    eprintln!("{op:?}, {variantes:?}, {} cenários", cenarios.len());
    let mut resultados: BTreeMap<(usize, Variante), Vec<[f64; COLUNAS]>> = BTreeMap::new();
    let mut sumidos = 0_i64;

    println!("## Por rodada\n");
    println!("| cenário | variante | rodada {CABECALHO}");
    println!("|---|---|--:{REGUA}");
    for rodada in 0..op.rodadas {
        for (indice, cenario) in cenarios.iter().enumerate() {
            let semente = op.semente + rodada;
            // Uma célula por vez.
            for variante in &variantes {
                let medida = match variante {
                    Variante::A => tokio::spawn(medir_a(*cenario, semente, op)).await,
                    outra => tokio::spawn(medir_rtp(*cenario, *outra, semente, op)).await,
                };
                match medida {
                    Ok(m) => {
                        sumidos += m.sumidos;
                        let valores = linha(&m);
                        println!(
                            "| {} | {} | {} | {} |",
                            cenario.nome,
                            variante.nome(),
                            rodada + 1,
                            em_colunas(&valores)
                        );
                        resultados
                            .entry((indice, *variante))
                            .or_default()
                            .push(valores);
                    }
                    Err(erro) => println!(
                        "| {} | {} | {} | falhou: {erro} |",
                        cenario.nome,
                        variante.nome(),
                        rodada + 1
                    ),
                }
            }
        }
    }

    println!("\n## Mediana das rodadas\n");
    println!("| cenário | variante {CABECALHO}");
    println!("|---|---{REGUA}");
    for ((indice, variante), linhas) in &resultados {
        let mut medianas = [f64::NAN; COLUNAS];
        for (coluna, m) in medianas.iter_mut().enumerate() {
            *m = mediana(linhas.iter().map(|l| l[coluna]).collect());
        }
        println!(
            "| {} | {} | {} |",
            cenarios[*indice].nome,
            variante.nome(),
            em_colunas(&medianas)
        );
    }

    println!("\n## Menor e maior valor entre as rodadas\n");
    println!("| cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |");
    println!("|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|");
    for ((indice, variante), linhas) in &resultados {
        let coluna = |c: usize| -> String {
            let valores: Vec<f64> = linhas.iter().map(|l| l[c]).collect();
            faixa(&valores, CASAS[c])
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            cenarios[*indice].nome,
            variante.nome(),
            coluna(0),
            coluna(1),
            coluna(4),
            coluna(6),
            coluna(9),
            coluna(10),
            coluna(14),
            coluna(16),
            coluna(20),
            coluna(22),
        );
    }
    println!(
        "\nConferência do cano: {sumidos} datagramas sumidos fora do sorteio ou mandados por fora (tem de ser 0)."
    );
}
