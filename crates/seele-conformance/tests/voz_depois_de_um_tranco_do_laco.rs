//! **A voz aberta, e o laço que fica alguns segundos sem ler** (o irmão de
//! `voz_de_quem_volta_e_senta_depois.rs`, pelos dois achados do P1 de 25/10):
//! uma pausa desta máquina NO MEIO da sessão — um tranco de CPU, o processo
//! suspenso — com a voz já tocando.
//!
//! Durante a pausa ninguém lê o canal de voz. A fila do enlace
//! ([`seele_enlace::FILA_DE_VOZ`]) guarda os datagramas mais velhos e descarta
//! os mais novos quando enche. Na volta, o relógio de reprodução reacerta
//! (`seele_audio::playout`, `MAX_CATCHUP_FRAMES`) em vez de repor a pausa, e o
//! laço lê a fila inteira de uma vez antes do primeiro tique. É isso que se
//! reproduz aqui, por tempo simulado e com o buffer de jitter de verdade:
//! - um emissor contínuo (ou com pausas de fala, DTX), um quadro de 20 ms por
//!   tique, sem perda no fio, com a `seq` e o carimbo perto de dar a volta;
//! - uma pausa de `N` segundos sem tique e sem leitura;
//! - na volta, os `FILA_DE_VOZ` datagramas mais velhos da pausa entregues de
//!   uma vez, e depois um por tique.
//!
//! A medida é a **decisão de cada tique** (`Decision`): o que o buffer mandou
//! tocar — o quadro, a ocultação, o silêncio, o conforto —, e não só os
//! contadores, que poderiam esconder um reparo que pula em vez de tocar.
//!
//! Sem o guarda do teto (`JitterBuffer::drain_gap`): a pausa de 8 s dava 18% de
//! tiques sem voz na janela de 5 a 65 s depois da volta, e a fila ainda a
//! 200 ms 90 s depois; o buraco se reabria a cada volta da fila.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "num teste, o pânico é o relatório; os números são de uma simulação"
)]

use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig, JitterMetrics};
use seele_audio::{FRAME_MS, FRAME_SAMPLES};

/// Quadros (e tiques) por segundo.
const POR_SEGUNDO: u32 = 1000 / FRAME_MS;

/// A fala antes da pausa: 30 s.
const ANTES: u32 = 30 * POR_SEGUNDO;

/// A janela de regime, em tiques depois da volta.
const COMECO_DA_JANELA: u32 = 5 * POR_SEGUNDO;
const FIM_DA_JANELA: u32 = 65 * POR_SEGUNDO;

/// Quando a profundidade é conferida, em tiques depois da volta.
const CONFERE_A_PROFUNDIDADE: u32 = 90 * POR_SEGUNDO;

/// A fração máxima de tiques sem voz na janela, com um fio que não perdeu nada.
const SEM_VOZ_MAXIMO: f64 = 0.05;

/// O teto do alvo do buffer (`specs/03-audio.md`).
const PROFUNDIDADE_MAXIMA_MS: f64 = 200.0;

/// A `seq` e o carimbo do primeiro quadro: a `seq` dá a volta 20 s depois, e o
/// carimbo de 32 bits, 25 s depois — os dois antes da pausa, que começa aos
/// 30 s.
const SEQ0: u16 = u16::MAX - (20 * POR_SEGUNDO) as u16 + 1;
const TS0: u32 = u32::MAX - 25 * POR_SEGUNDO * FRAME_SAMPLES as u32 + 1;
const _: () = assert!(SEQ0 > u16::MAX - 2_000 && TS0 > u32::MAX - 2_000_000);

/// Como o emissor fala.
#[derive(Debug, Clone, Copy)]
enum Fala {
    /// Sem pausa nenhuma.
    Continua,
    /// `fala` s falando e `pausa` s calado (DTX: nada no fio, o carimbo anda).
    ComPausas { fala: u32, pausa: u32 },
}

impl Fala {
    /// Se o quadro `k` (em tempo) vai ao fio.
    fn manda(self, k: u32) -> bool {
        match self {
            Self::Continua => true,
            Self::ComPausas { fala, pausa } => {
                k % ((fala + pausa) * POR_SEGUNDO) < fala * POR_SEGUNDO
            }
        }
    }
}

/// O que a simulação mediu, por tique, depois da volta.
#[derive(Debug, Default)]
struct Medida {
    tocados: u32,
    ocultados: u32,
    silencios: u32,
    confortos: u32,
    famintos: u32,
    maior_trecho_sem_voz: u32,
    /// O maior trecho de tiques seguidos sem `Play` nos primeiros 5 s depois
    /// da volta, de qualquer espécie (ocultação, silêncio, conforto, fome).
    maior_trecho_sem_play_no_inicio: u32,
    profundidade_no_fim_ms: f64,
    metricas: JitterMetrics,
}

impl Medida {
    /// A fração de tiques da janela sem voz: ocultação ou silêncio de perda.
    fn sem_voz(&self) -> f64 {
        let lugares = self.tocados + self.ocultados + self.silencios;
        if lugares == 0 {
            return 0.0;
        }
        f64::from(self.ocultados + self.silencios) / f64::from(lugares)
    }
}

/// Simula `ANTES` de fala, uma pausa de `pausa_s` do laço (zero: nenhuma) e
/// `CONFERE_A_PROFUNDIDADE` tiques depois, e mede a janela de regime.
fn simular(fala: Fala, pausa_s: u32) -> Medida {
    let mut buffer: JitterBuffer<u32> = JitterBuffer::new(JitterConfig::default());
    let quadro = FRAME_SAMPLES as u32;
    // A `seq` conta o que foi ao fio; o carimbo, o tempo.
    let mut enviados: u16 = 0;
    let mut datagrama = |k: u32| -> Option<(u16, u32)> {
        if !fala.manda(k) {
            return None;
        }
        let seq = SEQ0.wrapping_add(enviados);
        enviados = enviados.wrapping_add(1);
        Some((seq, TS0.wrapping_add(k.wrapping_mul(quadro))))
    };
    let chega = |buffer: &mut JitterBuffer<u32>, (seq, ts): (u16, u32), tique: u32, k: u32| {
        buffer.push(seq, ts, f64::from(tique) * f64::from(FRAME_MS), k);
    };

    for k in 0..ANTES {
        if let Some(d) = datagrama(k) {
            chega(&mut buffer, d, k, k);
        }
        let _ = buffer.tick();
    }

    // A pausa: nenhum tique, nenhuma leitura; a fila do enlace guarda os mais
    // velhos até o teto e descarta o resto.
    let pausa = pausa_s * POR_SEGUNDO;
    let volta = ANTES + pausa;
    let mut fila = Vec::new();
    for k in ANTES..volta {
        if let Some(d) = datagrama(k) {
            if fila.len() < seele_enlace::FILA_DE_VOZ {
                fila.push((d, k));
            }
        }
    }
    for (d, k) in fila {
        chega(&mut buffer, d, volta, k);
    }

    let mut medida = Medida::default();
    let mut trecho = 0;
    let mut sem_play = 0;
    for passo in 0..=CONFERE_A_PROFUNDIDADE {
        let k = volta + passo;
        if passo > 0 {
            if let Some(d) = datagrama(k) {
                chega(&mut buffer, d, k, k);
            }
        }
        let decisao = buffer.tick();
        if passo < COMECO_DA_JANELA {
            if matches!(decisao, Decision::Play(_)) {
                sem_play = 0;
            } else {
                sem_play += 1;
                medida.maior_trecho_sem_play_no_inicio =
                    medida.maior_trecho_sem_play_no_inicio.max(sem_play);
            }
        }
        if (COMECO_DA_JANELA..FIM_DA_JANELA).contains(&passo) {
            match decisao {
                Decision::Play(_) => {
                    medida.tocados += 1;
                    trecho = 0;
                }
                Decision::Conceal => medida.ocultados += 1,
                Decision::Silence => medida.silencios += 1,
                Decision::Comfort => medida.confortos += 1,
                Decision::Starved => medida.famintos += 1,
            }
            if matches!(decisao, Decision::Conceal | Decision::Silence) {
                trecho += 1;
                medida.maior_trecho_sem_voz = medida.maior_trecho_sem_voz.max(trecho);
            }
        }
    }
    medida.profundidade_no_fim_ms = buffer.depth_ms();
    medida.metricas = buffer.metrics();
    medida
}

/// O regime de cada cenário: quase nenhum tique sem voz, e a fila de volta ao
/// alvo.
fn conferir(nome: &str, medida: &Medida) {
    println!("{nome}: sem voz {:.4} {medida:?}", medida.sem_voz());
    assert!(
        medida.sem_voz() <= SEM_VOZ_MAXIMO,
        "{nome}: {:.3} dos tiques sem voz na janela de {} a {} s depois da volta, num fio sem \
         perda (ocultados {}, silêncios {}, tocados {}, maior trecho {} tiques): {medida:?}",
        medida.sem_voz(),
        COMECO_DA_JANELA / POR_SEGUNDO,
        FIM_DA_JANELA / POR_SEGUNDO,
        medida.ocultados,
        medida.silencios,
        medida.tocados,
        medida.maior_trecho_sem_voz
    );
    assert!(
        medida.profundidade_no_fim_ms <= PROFUNDIDADE_MAXIMA_MS,
        "{nome}: a fila ainda tinha {} ms {} s depois da volta: {medida:?}",
        medida.profundidade_no_fim_ms,
        CONFERE_A_PROFUNDIDADE / POR_SEGUNDO
    );
}

/// A tolerância explícita do maior trecho sem voz logo depois da volta: o
/// limiar da rajada (500 ms) mais dois quadros de folga para o alvo e a
/// quantização. Não é a recuperação integrada: é o modelo do buffer com o
/// buraco que a fila do enlace deixa.
const SEM_PLAY_DEPOIS_DA_VOLTA_MAXIMO_MS: u32 = 540;

/// **Depois da pausa, a voz do presente toca logo** (a LAN03 de 25/10, 22:58:
/// ~2,5 s de silêncio depois do SIGCONT, com a fila já corrigida). A fila do
/// enlace guarda os mais velhos da pausa e perde os seguintes; o buraco que
/// fica entre eles é tempo que já passou, e não pode ser pago tique a tique
/// como ocultação e silêncio. Mede-se o maior trecho sem `Play` nos primeiros
/// 5 s depois da volta, que é o que a janela de regime não via.
#[test]
fn depois_da_pausa_a_voz_do_presente_toca_logo() {
    for pausa_s in [4, 6, 8] {
        let medida = simular(Fala::Continua, pausa_s);
        let ms = medida.maior_trecho_sem_play_no_inicio * FRAME_MS;
        println!(
            "pausa de {pausa_s} s: maior trecho sem Play nos 5 s depois da volta {ms} ms; {:?}",
            medida.metricas
        );
        assert!(
            ms <= SEM_PLAY_DEPOIS_DA_VOLTA_MAXIMO_MS,
            "pausa de {pausa_s} s: {ms} ms seguidos sem voz nos 5 s depois da volta (o teto é \
             {SEM_PLAY_DEPOIS_DA_VOLTA_MAXIMO_MS} ms): {medida:?}"
        );
    }
}

#[test]
fn uma_pausa_do_laco_com_a_voz_aberta_drena_sem_reabrir_o_buraco() {
    for pausa_s in [4, 6, 8] {
        conferir(
            &format!("pausa de {pausa_s} s"),
            &simular(Fala::Continua, pausa_s),
        );
    }
}

#[test]
fn o_regime_normal_toca_todo_tique() {
    let medida = simular(Fala::Continua, 0);
    conferir("regime normal", &medida);
    assert_eq!(
        medida.tocados,
        FIM_DA_JANELA - COMECO_DA_JANELA,
        "{medida:?}"
    );
    assert_eq!(medida.metricas.buraco_pulado, 0);
}

#[test]
fn o_dtx_e_conforto_e_nao_perda() {
    let medida = simular(Fala::ComPausas { fala: 5, pausa: 1 }, 0);
    conferir("DTX", &medida);
    assert_eq!(medida.ocultados + medida.silencios, 0, "{medida:?}");
    assert!(medida.confortos + medida.famintos > 0, "{medida:?}");
    assert_eq!(medida.metricas.buraco_pulado, 0);
}

#[test]
fn a_volta_da_seq_e_do_carimbo_nao_cortam_a_voz() {
    // As duas voltas acontecem antes da pausa (aos 20 e aos 25 s); a fala de
    // 30 s inteira tem de ter tocado, e a janela depois também.
    let medida = simular(Fala::Continua, 0);
    assert_eq!(medida.metricas.frames_concealed, 0, "{medida:?}");
    assert_eq!(medida.metricas.frames_silenced, 0, "{medida:?}");
    assert_eq!(medida.metricas.late_discards, 0, "{medida:?}");
    assert_eq!(medida.metricas.resyncs, 0, "{medida:?}");
}

#[test]
fn a_pausa_com_dtx_tambem_drena() {
    conferir(
        "pausa de 8 s com DTX",
        &simular(Fala::ComPausas { fala: 5, pausa: 1 }, 8),
    );
}

/// O que uma rajada deixou: os tiques até a fila voltar ao alvo, as decisões
/// depois dela, as métricas e quantos quadros entregues nunca tocaram.
type DepoisDaRajada = (Option<u32>, Vec<Decision<u32>>, JitterMetrics, u64);

/// **A rajada, como o laço a entrega**: depois de uma pausa da máquina, o
/// laço de voz (`seele_core::voice`, a recepção antes do playout) lê tudo o
/// que está pronto na fila do enlace e só então dá os tiques. Os quadros da
/// rajada são contíguos — nenhum faltou, eles só chegaram juntos —, e depois
/// dela a fala continua, um quadro vivo por tique.
///
/// «Voltar ao alvo» é a fila em no máximo o alvo mais um quadro; `None` se
/// não voltou em 90 s.
fn rajada(rajada_ms: u32) -> DepoisDaRajada {
    let mut buffer: JitterBuffer<u32> = JitterBuffer::new(JitterConfig::default());
    let quadro = FRAME_SAMPLES as u32;
    let mut entregues = std::collections::BTreeSet::new();
    let mut tocados = std::collections::BTreeSet::new();
    let mut push = |buffer: &mut JitterBuffer<u32>, k: u32, tique: u32| {
        buffer.push(
            SEQ0.wrapping_add(k as u16),
            TS0.wrapping_add(k.wrapping_mul(quadro)),
            f64::from(tique) * f64::from(FRAME_MS),
            k,
        );
        entregues.insert(k);
    };

    // A fala, intercalada como o laço intercala quando nada atrasa.
    for k in 0..ANTES {
        push(&mut buffer, k, k);
        if let Decision::Play(k) = buffer.tick() {
            tocados.insert(k);
        }
    }
    // A rajada: `rajada_ms` de quadros contíguos, todos antes do tique.
    let quadros = rajada_ms / FRAME_MS;
    let volta = ANTES + quadros;
    for k in ANTES..volta {
        push(&mut buffer, k, volta);
    }
    let mut decisoes = Vec::new();
    let mut recuperou = None;
    for passo in 0..CONFERE_A_PROFUNDIDADE {
        let k = volta + passo;
        push(&mut buffer, k, k);
        let decisao = buffer.tick();
        if let Decision::Play(k) = decisao {
            tocados.insert(k);
        }
        decisoes.push(decisao);
        let m = buffer.metrics();
        if recuperou.is_none() && m.depth_ms <= m.target_ms + f64::from(FRAME_MS) {
            recuperou = Some(passo);
        }
    }
    // O que ainda está na fila no fim não conta: ainda vai tocar.
    let fila = (buffer.depth_ms() / f64::from(FRAME_MS)) as u64;
    let nunca = entregues.difference(&tocados).count() as u64 - fila;
    (recuperou, decisoes, buffer.metrics(), nunca)
}

/// **Uma rajada de segundos não pode virar segundos de atraso.** De 1 s para
/// cima, quem ouve volta ao alvo em no máximo 1 s (50 tiques), e nenhum tique
/// depois da rajada fica sem voz.
#[test]
fn uma_rajada_de_segundos_volta_ao_alvo_em_um_segundo() {
    for rajada_ms in [1_000, 3_000, 5_120] {
        let (recuperou, decisoes, metricas, nunca) = rajada(rajada_ms);
        let sem_voz = decisoes
            .iter()
            .filter(|d| !matches!(d, Decision::Play(_)))
            .count();
        println!(
            "rajada de {rajada_ms} ms: voltou ao alvo em {recuperou:?} tiques, {sem_voz} tiques \
             sem voz, {nunca} quadros nunca tocaram; {metricas:?}"
        );
        assert_eq!(sem_voz, 0, "rajada de {rajada_ms} ms: {metricas:?}");
        assert!(
            recuperou.is_some_and(|tiques| tiques <= POR_SEGUNDO),
            "rajada de {rajada_ms} ms: a fila levou {recuperou:?} tiques (de 20 ms) para voltar \
             ao alvo, e o teto é {POR_SEGUNDO}: quem ouve ficou esse tempo atrás do presente. \
             {metricas:?}"
        );
    }
}

/// **Abaixo do limiar, nada se corta**: uma rajada de 400 ms continua na
/// compressão de 10% de sempre. Todo quadro que não tocou é uma emenda
/// contada (`descartes_por_atraso`), e a fila volta ao alvo devagar, como a
/// compressão faz.
#[test]
fn uma_rajada_curta_continua_na_compressao() {
    let (recuperou, decisoes, metricas, nunca) = rajada(400);
    println!(
        "rajada de 400 ms: voltou em {recuperou:?} tiques, {nunca} nunca tocaram; {metricas:?}"
    );
    assert!(
        decisoes.iter().all(|d| matches!(d, Decision::Play(_))),
        "{metricas:?}"
    );
    assert_eq!(
        nunca, metricas.descartes_por_atraso,
        "rajada de 400 ms: saíram quadros que não são emendas: {metricas:?}"
    );
    assert!(
        recuperou.is_some_and(|tiques| tiques > POR_SEGUNDO),
        "rajada de 400 ms: voltou em {recuperou:?} tiques, rápido demais para ser a compressão \
         de 10%: {metricas:?}"
    );
}
