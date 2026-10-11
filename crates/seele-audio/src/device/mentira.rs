//! **O aparelho de mentira**: um microfone e uma saída que não são placa de som
//! nenhuma. Só com o recurso `aparelho-de-mentira`, e só para teste.
//!
//! # Por que ele existe
//!
//! A pendência 61 pediu uma medida que não dava para fazer: «a voz reabre
//! debaixo da tela de fim?». Medir pela FFI abria o microfone de verdade da
//! máquina de quem mede, e o aparelho de mentira que havia (`supervisor.rs`) é
//! uma peça do ciclo de reabertura, que não chega à FFI. Sem ele, o que se sabia
//! da voz numa queda era lido no código, não medido.
//!
//! # Como ele entra
//!
//! Pelo **mesmo** [`super::open`] de sempre, quando o id pedido de um dos lados
//! começa com [`PREFIXO`]. Tudo acima dele é o produto: a porta da voz da FFI, o
//! `Voice::start_preferring`, a reabertura na volta (`Voice::reopen`), a
//! reabertura do laço, o reamostrador, o codificador e o canal de mídia. Só os
//! dois retornos do `cpal` são trocados por uma linha que faz o que eles fazem:
//! entrega um bloco de captura e pede um bloco de saída a cada 10 ms, pelos
//! mesmos [`CaptureSink::on_capture`] e [`PlaybackSource::on_playback`].
//!
//! **O `cpal` nunca é tocado**, nem para enumerar: o id de mentira é decidido
//! antes de o host existir. E os dois lados são de mentira juntos, mesmo que só
//! um tenha sido pedido assim — uma metade de verdade seria um aparelho desta
//! máquina aberto por um teste.
//!
//! # O que um teste lê
//!
//! [`contagem`], por nome: quantos aparelhos com aquele nome estão abertos agora
//! e quantas vezes um abriu. É o que diz «a voz fechou» (zero abertos) e «a voz
//! reabriu» (uma abertura a mais) sem olhar nenhum estado da FFI — o aparelho é
//! a última coisa que fecha.
//!
//! [`CaptureSink::on_capture`]: crate::rt::CaptureSink::on_capture
//! [`PlaybackSource::on_playback`]: crate::rt::PlaybackSource::on_playback

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{AudioIo, CaptureDevice, PlaybackDevice, Vivos, Wanted};
use crate::rt::{capacity_for_ms, capture_path, playback_path, StreamCounters};
use crate::SAMPLE_RATE_HZ;

// O recurso nunca entra num build otimizado: um id com o prefixo abriria um
// aparelho que não ouve ninguém, e a pessoa falaria para o nada.
#[cfg(not(debug_assertions))]
compile_error!(
    "o recurso `aparelho-de-mentira` do seele-audio é só de teste e não pode ir num build \
     otimizado: com ele, um id de aparelho com o prefixo abre um microfone que não ouve nada"
);

/// O começo do id que abre o aparelho de mentira em vez de um de verdade.
///
/// O resto do id é o **nome** dele, que separa um teste do outro em
/// [`contagem`].
pub const PREFIXO: &str = "seele-mentira:";

/// O bloco que a linha entrega e pede a cada volta: 10 ms a 48 kHz.
const BLOCO: usize = 480;

/// A cadência da linha, a de um retorno de `cpal` com o bloco acima.
const CADENCIA: Duration = Duration::from_millis(10);

/// A frequência do tom que o microfone de mentira «ouve».
const TOM_HZ: f32 = 440.0;

/// A amplitude do tom: alta o bastante para abrir qualquer portão de voz.
const AMPLITUDE: f32 = 0.3;

/// O que se sabe dos aparelhos de um nome.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Contagem {
    /// Quantos estão abertos agora.
    pub abertos: usize,
    /// Quantas vezes um abriu, desde o começo do processo.
    pub aberturas: u64,
    /// Quantos blocos de captura foram entregues, somando todos.
    pub blocos: u64,
}

/// As contagens, por nome. Uma árvore e não um `HashMap`: são poucos nomes, e
/// a ordem não importa a ninguém.
static CONTAGENS: Mutex<BTreeMap<String, Contagem>> = Mutex::new(BTreeMap::new());

/// O que se sabe dos aparelhos de mentira com este nome.
#[must_use]
pub fn contagem(nome: &str) -> Contagem {
    CONTAGENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(nome)
        .copied()
        .unwrap_or_default()
}

/// O id completo de um aparelho de mentira com este nome.
#[must_use]
pub fn id(nome: &str) -> String {
    format!("{PREFIXO}{nome}")
}

/// Mexe na contagem de um nome.
fn contar(nome: &str, mexer: impl FnOnce(&mut Contagem)) {
    let mut contagens = CONTAGENS.lock().unwrap_or_else(PoisonError::into_inner);
    mexer(contagens.entry(nome.to_owned()).or_default());
}

/// O nome pedido, se um dos lados pediu um aparelho de mentira.
///
/// O da captura vence quando os dois pediram, e um lado só basta para os dois
/// serem de mentira: ver o doc do módulo.
pub(super) fn pedido(wanted: Wanted<'_>) -> Option<&str> {
    [wanted.capture, wanted.playback]
        .into_iter()
        .flatten()
        .find_map(|id| id.strip_prefix(PREFIXO))
}

/// A linha que faz as vezes dos dois retornos do `cpal`.
///
/// Largá-la para a linha e a espera, como largar os dois fluxos para o áudio.
pub(super) struct Linha {
    parar: Arc<AtomicBool>,
    linha: Option<std::thread::JoinHandle<()>>,
    nome: String,
}

impl Drop for Linha {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
        if let Some(linha) = self.linha.take() {
            let _ = linha.join();
        }
        contar(&self.nome, |contagem| {
            contagem.abertos = contagem.abertos.saturating_sub(1);
        });
    }
}

/// Abre um par de aparelhos de mentira com este nome.
///
/// Nunca falha: um aparelho de mentira que recusasse abrir levaria
/// `open_preferring`, no núcleo, a tentar o da máquina — e é exatamente o que
/// este módulo existe para não deixar acontecer.
pub(super) fn abrir(nome: &str, ring_ms: u32) -> AudioIo {
    let canais = NonZeroU16::MIN;
    let counters = StreamCounters::shared();
    let (mut sink, captured) = capture_path(
        capacity_for_ms(ring_ms, SAMPLE_RATE_HZ),
        canais,
        Arc::clone(&counters),
    );
    let (to_device, mut source) = playback_path(
        capacity_for_ms(ring_ms, SAMPLE_RATE_HZ),
        canais,
        Arc::clone(&counters),
    );

    let parar = Arc::new(AtomicBool::new(false));
    let parar_na_linha = Arc::clone(&parar);
    let nome_na_linha = nome.to_owned();
    contar(nome, |contagem| {
        contagem.abertos += 1;
        contagem.aberturas += 1;
    });
    let linha = std::thread::Builder::new()
        .name("seele-mentira".into())
        .spawn(move || {
            #[allow(clippy::cast_precision_loss, reason = "48 000 cabe exato num f32")]
            let passo = std::f32::consts::TAU * TOM_HZ / SAMPLE_RATE_HZ as f32;
            let mut fase = 0.0_f32;
            let mut entrada = [0.0_f32; BLOCO];
            let mut saida = [0.0_f32; BLOCO];
            let mut proxima = Instant::now();
            while !parar_na_linha.load(Ordering::Relaxed) {
                for amostra in &mut entrada {
                    *amostra = AMPLITUDE * fase.sin();
                    fase = (fase + passo) % std::f32::consts::TAU;
                }
                sink.on_capture::<f32>(&entrada);
                source.on_playback::<f32>(&mut saida);
                contar(&nome_na_linha, |contagem| contagem.blocos += 1);
                proxima += CADENCIA;
                std::thread::sleep(proxima.saturating_duration_since(Instant::now()));
            }
        })
        .ok();

    AudioIo {
        _vivos: Vivos::Mentira {
            _linha: Linha {
                parar,
                linha,
                nome: nome.to_owned(),
            },
        },
        captured,
        to_device,
        counters,
        capture_rate_hz: SAMPLE_RATE_HZ,
        playback_rate_hz: SAMPLE_RATE_HZ,
        capture: Some(CaptureDevice {
            id: id(nome),
            name: format!("microfone de mentira ({nome})"),
            default: false,
        }),
        playback: Some(PlaybackDevice {
            id: id(nome),
            name: format!("saída de mentira ({nome})"),
            default: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_id_de_mentira_abre_os_dois_lados_de_mentira_e_largar_fecha() {
        let nome = "teste-do-modulo";
        let antes = contagem(nome);
        let pedido = id(nome);
        let io = super::super::open(
            Wanted {
                capture: Some(&pedido),
                playback: None,
            },
            100,
        )
        .unwrap();
        assert_eq!(
            io.capture.as_ref().map(|d| d.id.as_str()),
            Some(pedido.as_str())
        );
        assert_eq!(
            io.playback.as_ref().map(|d| d.id.as_str()),
            Some(pedido.as_str())
        );
        assert_eq!(contagem(nome).abertos, antes.abertos + 1);
        assert_eq!(contagem(nome).aberturas, antes.aberturas + 1);
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            io.captured.slots() > 0,
            "a linha não entregou captura nenhuma em 60 ms"
        );
        drop(io);
        assert_eq!(contagem(nome).abertos, antes.abertos);
    }

    #[test]
    fn um_id_sem_o_prefixo_nao_e_de_mentira() {
        assert_eq!(
            pedido(Wanted {
                capture: Some("BuiltInMicrophoneDevice"),
                playback: None,
            }),
            None
        );
        assert_eq!(
            pedido(Wanted {
                capture: None,
                playback: Some("seele-mentira:x"),
            }),
            Some("x")
        );
    }
}
