//! **A voz atrasa depois de um tempo de conversa** — a regressão de produto.
//!
//! Relato da 0.17: a conversa começa em dia e, minutos depois, quem ouve está
//! centenas de milissegundos atrás. A prova de componentes do Codex
//! (um script local de diagnóstico, não versionado; o resultado está registrado abaixo) mediu com o `jitter.rs` e o
//! `playout.rs` reais, na agenda do laço de voz do `seele-core` — um `tick` do
//! buffer por quadro que o [`PlayoutClock::due`] diz vencido:
//!
//! | cenário                     | 1º min | 11º min | 30º min |
//! |-----------------------------|--------|---------|---------|
//! | 0 ppm                       | 20 ms  | 20 ms   | 20 ms   |
//! | +100 ppm                    | 26 ms  | 86 ms   | 200 ms  |
//! | +300 ppm                    | 38 ms  | 218 ms  | 560 ms  |
//! | pausa de CPU de 500 ms (10') | 20 ms  | 460 ms  | 460 ms  |
//!
//! Este arquivo é a mesma simulação, dentro do repositório, com os mesmos
//! componentes e a mesma agenda, e com critérios de latência **e** de qualidade.
//! Ele mede tudo por fora, pelo que sai do buffer: cada quadro carrega o próprio
//! número e o instante em que nasceu no relógio de quem fala, de modo que a idade
//! do que toca, os quadros pulados e os vãos são contados do lado de quem ouve,
//! e não lidos de um contador do próprio buffer.
//!
//! # Por que o soak de 10 minutos não via isto
//!
//! `seele-conformance/tests/soak_audio.rs` intercala DTX: o falante para a cada
//! poucos segundos, e o buffer pula o resto de uma pausa de conforto sempre que a
//! fila passa do alvo (`jitter.rs`, «Silêncio com a voz já esperando na fila é
//! atraso puro»). Cada pausa drenava, de graça, o que a deriva tinha acumulado. E
//! a agenda dele era a mesma de quem manda, sem relógio independente. Aqui a fala
//! é **contínua** por 30 minutos, sem nenhum quadro de conforto, e o relógio de
//! quem fala anda à parte do de quem ouve: o único jeito de drenar o excesso é o
//! buffer tirá-lo da fala.
//!
//! # Os critérios, e por que estes números
//!
//! - **Teto de latência: 60 ms**, depois dos primeiros 10 s, em ±100 e ±300 ppm.
//!   O alvo assenta em 20 ms (o piso de `specs/03-audio.md`); o descarte só
//!   acontece quando a fila passou um quadro inteiro do alvo durante uma janela
//!   inteira, então a idade oscila entre 20 e 40 ms. Mais um quadro de folga para
//!   a quantização da agenda dá 60. Hoje: 200 e 560 ms.
//! - **Recuperação de uma pausa de 500 ms: abaixo do teto em até 20 s**, e de
//!   lá até o fim. A pausa entra na estimativa de jitter (uma rajada de meio
//!   segundo é jitter, para quem recebe), o alvo sobe para ~90 ms e desce
//!   devagar — a assimetria que `specs/03-audio.md` pede. O que o buffer deve é
//!   acompanhar o alvo, e não ficar 440 ms acima dele para sempre. Além disso,
//!   **em até 5 s o excesso cai abaixo de 100 ms**: a drenagem rápida não espera
//!   o alvo descer.
//! - **Descarte limitado ao que a deriva exige.** Cada quadro tirado da fala é
//!   uma emenda de 20 ms, audível como um estalo leve. Em 30 min, +300 ppm somam
//!   540 ms = 27 quadros; o buffer pode tirar no máximo isso mais 2. Em 0 ppm,
//!   **nenhum**. Na pausa, no máximo o excesso (23 quadros) mais 2.
//! - **Espaçamento mínimo entre emendas**: 1 s no regime de deriva (uma emenda
//!   por segundo é 2% do tempo, quarenta vezes o que 500 ppm pedem) e 200 ms na
//!   recuperação (nunca duas emendas a menos de nove quadros intactos uma da
//!   outra).
//! - **Nenhum vão inventado**: com rede perfeita e fala contínua, depois de
//!   começar, todo tique toca um quadro real. Sem ocultação, sem silêncio, sem
//!   fome — exceto o que a deriva negativa obriga (quem fala devagar deixa a fila
//!   vazia um tique a cada 20 ms de deriva, e isso é contado e limitado também).
//! - **Ordem**: nenhum quadro toca duas vezes nem fora de ordem, através da
//!   volta da sequência de 16 bits (90 mil quadros em 30 min dão uma volta).
//!
//! # O que isto não mede
//!
//! Não há Opus, dispositivo, transporte nem rede com jitter aqui: é a combinação
//! buffer + relógio de reprodução, na agenda do core. O som de uma emenda depende
//! do decodificador e não é medido; o que se limita é quantas há e quão perto.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_audio::playout::PlayoutClock;
use seele_audio::FRAME_MS;

const TRINTA_MINUTOS_MS: u64 = 1_800_000;
const PAUSA_EM_MS: u64 = 600_000;
const AQUECIMENTO_MS: u64 = 10_000;
const TETO_MS: f64 = 60.0;
const RECUPERACAO_MS: u64 = 20_000;
const DRENAGEM_RAPIDA_MS: u64 = 5_000;
const EXCESSO_DRENADO_MS: f64 = 100.0;

/// Um quadro como o laço o recebe, com o que é preciso para medi-lo por fora.
#[derive(Debug, Clone, Copy)]
struct Quadro {
    /// Quantos pacotes saíram antes deste — o que a sequência conta. Um salto
    /// aqui entre dois tocados é quadro que chegou e não tocou: emenda.
    enviado: u64,
    nasceu_ms: f64,
}

/// O que varia entre os cenários.
#[derive(Debug, Clone, Copy)]
struct Cenario {
    ppm: f64,
    pausa_ms: u64,
    /// Fala de 5 s e pausa de 1 s, com DTX: na pausa nada vai ao fio, a
    /// sequência para e o carimbo anda — o que o soak antigo fazia.
    dtx: bool,
    /// Onde o carimbo de 32 bits começa. Perto do fim, ele dá a volta no meio.
    carimbo_inicial: u32,
}

impl Cenario {
    const fn fala(ppm: f64, pausa_ms: u64) -> Self {
        Self {
            ppm,
            pausa_ms,
            dtx: false,
            carimbo_inicial: 0,
        }
    }
}

/// Idade a partir da qual a reprodução voltou à base: o alvo mínimo (20 ms)
/// mais um quadro de oscilação da agenda.
const BASE_MS: f64 = 40.0;

/// O que saiu do buffer, tique a tique, visto de quem ouve.
#[derive(Debug, Default)]
struct Medida {
    idade_1min: f64,
    idade_11min: f64,
    idade_30min: f64,
    /// Maior idade depois do aquecimento, fora da janela de recuperação.
    pior_fora_da_recuperacao: f64,
    /// Quando a idade ficou abaixo do teto e não passou mais, contado do fim
    /// da pausa. `None` se nunca voltou.
    recuperou_em_ms: Option<u64>,
    /// Quando a idade caiu abaixo de `EXCESSO_DRENADO_MS`, contado do fim da pausa.
    drenou_em_ms: Option<u64>,
    /// Quando a idade voltou a `BASE_MS` e não passou mais, contado do fim da pausa.
    na_base_em_ms: Option<u64>,
    /// Quadros que nunca tocaram: emendas do buffer.
    descartados: u64,
    /// Instantes das emendas, em ms de quem ouve.
    emendas_em: Vec<u64>,
    /// Tiques depois de começar que não tocaram um quadro real.
    vaos: u64,
    ocultados: u64,
    fora_de_ordem: u64,
}

fn simular(ppm: f64, pausa_ms: u64) -> Medida {
    simular_cenario(Cenario::fala(ppm, pausa_ms))
}

fn simular_cenario(cenario: Cenario) -> Medida {
    let Cenario {
        ppm,
        pausa_ms,
        dtx,
        carimbo_inicial,
    } = cenario;
    let mut buffer: JitterBuffer<Quadro> = JitterBuffer::new(JitterConfig::default());
    let partida = Instant::now();
    let mut relogio = PlayoutClock::new(partida, FRAME_MS);
    // O relógio de quem fala: um quadro a cada 20 ms **dele**, que são
    // 20/(1+ppm) ms de quem ouve.
    let intervalo = f64::from(FRAME_MS) / (1.0 + ppm / 1_000_000.0);
    let fim_da_pausa = PAUSA_EM_MS + pausa_ms;

    // `slot` é o tempo de quem fala, em quadros: o carimbo anda com ele. `enviados`
    // é o que foi ao fio: a sequência anda com ele, e com DTX os dois se separam.
    let mut slot = 0_u64;
    let mut enviados = 0_u64;
    let mut em_transito: VecDeque<(u16, u32, Quadro)> = VecDeque::new();
    let mut medida = Medida::default();
    let mut idade = 0.0_f64;
    let mut ultimo: Option<u64> = None;
    let mut comecou = false;
    let mut ultima_acima_do_teto: Option<u64> = None;
    let mut ultima_acima_da_base: Option<u64> = None;

    for agora in 0..=TRINTA_MINUTOS_MS {
        while slot as f64 * intervalo <= agora as f64 {
            // 5 s de fala, 1 s de pausa: 250 quadros sim, 50 não.
            let calado = dtx && slot % 300 >= 250;
            if !calado {
                // `seq` de 16 bits e carimbo de 32, como no fio: em 30 min a
                // sequência dá uma volta inteira.
                #[allow(clippy::cast_possible_truncation)]
                let (seq, carimbo) = (
                    enviados as u16,
                    carimbo_inicial.wrapping_add((slot * 960) as u32),
                );
                em_transito.push_back((
                    seq,
                    carimbo,
                    Quadro {
                        enviado: enviados,
                        nasceu_ms: slot as f64 * intervalo,
                    },
                ));
                enviados += 1;
            }
            slot += 1;
        }
        // A pausa de CPU: quem fala continua, quem ouve não lê nem toca.
        if pausa_ms > 0 && agora >= PAUSA_EM_MS && agora < fim_da_pausa {
            continue;
        }
        while let Some((seq, carimbo, quadro)) = em_transito.pop_front() {
            buffer.push(seq, carimbo, agora as f64, quadro);
        }
        for _ in 0..relogio.due(partida + Duration::from_millis(agora)) {
            match buffer.tick() {
                Decision::Play(quadro) => {
                    comecou = true;
                    idade = agora as f64 - quadro.nasceu_ms;
                    if let Some(antes) = ultimo {
                        if quadro.enviado <= antes {
                            medida.fora_de_ordem += 1;
                        } else if quadro.enviado > antes + 1 {
                            medida.descartados += quadro.enviado - antes - 1;
                            medida.emendas_em.push(agora);
                        }
                    }
                    ultimo = Some(quadro.enviado);
                }
                Decision::Conceal => {
                    medida.ocultados += 1;
                    medida.vaos += 1;
                }
                Decision::Silence | Decision::Comfort | Decision::Starved => {
                    if comecou {
                        medida.vaos += 1;
                    }
                }
            }
        }

        if agora >= AQUECIMENTO_MS && idade > TETO_MS {
            ultima_acima_do_teto = Some(agora);
        }
        if agora >= AQUECIMENTO_MS && idade > BASE_MS {
            ultima_acima_da_base = Some(agora);
        }
        let na_recuperacao =
            pausa_ms > 0 && agora >= PAUSA_EM_MS && agora < fim_da_pausa + RECUPERACAO_MS;
        if agora >= AQUECIMENTO_MS && !na_recuperacao {
            medida.pior_fora_da_recuperacao = medida.pior_fora_da_recuperacao.max(idade);
        }
        if pausa_ms > 0
            && agora >= fim_da_pausa
            && medida.drenou_em_ms.is_none()
            && idade < EXCESSO_DRENADO_MS
        {
            medida.drenou_em_ms = Some(agora - fim_da_pausa);
        }
        match agora {
            60_000 => medida.idade_1min = idade,
            660_000 => medida.idade_11min = idade,
            TRINTA_MINUTOS_MS => medida.idade_30min = idade,
            _ => {}
        }
    }

    if pausa_ms > 0 {
        medida.recuperou_em_ms = match ultima_acima_do_teto {
            None => Some(0),
            Some(t) if t >= fim_da_pausa => Some(t - fim_da_pausa),
            Some(_) => Some(0),
        };
        medida.na_base_em_ms = match ultima_acima_da_base {
            Some(t) if t >= fim_da_pausa => Some(t - fim_da_pausa),
            _ => Some(0),
        };
    }
    medida
}

/// O menor intervalo entre duas emendas, em ms, dentro de `[de, ate)`.
fn menor_espaco(emendas: &[u64], de: u64, ate: u64) -> Option<u64> {
    let dentro: Vec<u64> = emendas
        .iter()
        .copied()
        .filter(|t| *t >= de && *t < ate)
        .collect();
    dentro
        .windows(2)
        .filter_map(|par| match par {
            [antes, depois] => Some(depois - antes),
            _ => None,
        })
        .min()
}

fn relatar(nome: &str, m: &Medida) -> String {
    format!(
        "{nome}: idade 1'={:.1} 11'={:.1} 30'={:.1} pior={:.1} ms; descartados={} \
         emendas={} vaos={} ocultados={} fora_de_ordem={} drenou_em={:?} recuperou_em={:?} \
         na_base_em={:?}",
        m.idade_1min,
        m.idade_11min,
        m.idade_30min,
        m.pior_fora_da_recuperacao,
        m.descartados,
        m.emendas_em.len(),
        m.vaos,
        m.ocultados,
        m.fora_de_ordem,
        m.drenou_em_ms,
        m.recuperou_em_ms,
        m.na_base_em_ms,
    )
}

/// Quadros que a deriva soma em 30 minutos: o máximo que o buffer pode tirar.
fn quadros_de_deriva(ppm: f64) -> u64 {
    let ms = ppm.abs() / 1_000_000.0 * TRINTA_MINUTOS_MS as f64;
    (ms / f64::from(FRAME_MS)).ceil() as u64
}

#[test]
fn sem_deriva_nada_e_tirado_da_fala() {
    let m = simular(0.0, 0);
    eprintln!("{}", relatar("0 ppm", &m));
    assert!(
        m.pior_fora_da_recuperacao <= TETO_MS,
        "{}",
        relatar("0 ppm", &m)
    );
    assert_eq!(
        m.descartados,
        0,
        "emendou fala sem deriva: {}",
        relatar("0 ppm", &m)
    );
    assert_eq!(m.vaos, 0, "vão com rede perfeita: {}", relatar("0 ppm", &m));
    assert_eq!(m.fora_de_ordem, 0);
}

fn deriva_positiva(ppm: f64) {
    let nome = format!("+{ppm} ppm");
    let m = simular(ppm, 0);
    let r = relatar(&nome, &m);
    eprintln!("{r}");
    assert!(
        m.pior_fora_da_recuperacao <= TETO_MS,
        "a voz atrasou com a conversa (teto {TETO_MS} ms): {r}"
    );
    assert!(m.idade_30min <= TETO_MS, "{r}");
    let limite = quadros_de_deriva(ppm) + 2;
    assert!(
        m.descartados <= limite,
        "tirou mais fala que a deriva pede ({limite}): {r}"
    );
    assert!(
        menor_espaco(&m.emendas_em, 0, u64::MAX).is_none_or(|e| e >= 1_000),
        "emendas a menos de 1 s uma da outra: {r} {:?}",
        m.emendas_em
    );
    assert_eq!(m.vaos, 0, "{r}");
    assert_eq!(m.fora_de_ordem, 0, "{r}");
}

#[test]
fn mais_100_ppm_nao_acumula_atraso_em_30_minutos() {
    deriva_positiva(100.0);
}

#[test]
fn mais_300_ppm_nao_acumula_atraso_em_30_minutos() {
    deriva_positiva(300.0);
}

#[test]
fn menos_300_ppm_nao_atrasa_e_os_vaos_sao_os_da_deriva() {
    // A outra direção: quem fala devagar esvazia a fila. Não atrasa nunca, mas
    // cada 20 ms de deriva viram um tique sem quadro. O critério é que sejam só
    // esses, e não um vão por tique.
    let m = simular(-300.0, 0);
    let r = relatar("-300 ppm", &m);
    eprintln!("{r}");
    assert!(m.pior_fora_da_recuperacao <= TETO_MS, "{r}");
    assert_eq!(m.descartados, 0, "{r}");
    assert!(m.vaos <= quadros_de_deriva(-300.0) + 2, "{r}");
    assert_eq!(m.fora_de_ordem, 0, "{r}");
}

#[test]
fn uma_pausa_de_500_ms_nao_deixa_atraso_para_sempre() {
    let m = simular(0.0, 500);
    let r = relatar("pausa de 500 ms", &m);
    eprintln!("{r}");
    assert!(
        m.drenou_em_ms.is_some_and(|t| t <= DRENAGEM_RAPIDA_MS),
        "o excesso não caiu abaixo de {EXCESSO_DRENADO_MS} ms em {DRENAGEM_RAPIDA_MS} ms: {r}"
    );
    assert!(
        m.recuperou_em_ms.is_some_and(|t| t <= RECUPERACAO_MS),
        "não voltou abaixo de {TETO_MS} ms em {RECUPERACAO_MS} ms: {r}"
    );
    assert!(m.idade_11min <= TETO_MS, "{r}");
    assert!(m.idade_30min <= TETO_MS, "{r}");
    assert!(m.pior_fora_da_recuperacao <= TETO_MS, "{r}");
    // 25 quadros chegaram de uma vez e o relógio repôs 4: o excesso é de ~23.
    assert!(m.descartados <= 23 + 2, "tirou mais do que sobrou: {r}");
    let fim = PAUSA_EM_MS + 500;
    assert!(
        menor_espaco(&m.emendas_em, fim, fim + RECUPERACAO_MS).is_none_or(|e| e >= 200),
        "emendas a menos de 200 ms uma da outra na recuperação: {r} {:?}",
        m.emendas_em
    );
    assert!(
        m.na_base_em_ms.is_some_and(|t| t <= RECUPERACAO_MS),
        "não voltou a {BASE_MS} ms em {RECUPERACAO_MS} ms: {r}"
    );
    assert_eq!(m.ocultados, 0, "{r}");
    assert_eq!(m.fora_de_ordem, 0, "{r}");
}

#[test]
fn com_dtx_intercalado_as_pausas_pagam_a_deriva_sem_emenda() {
    // A forma do soak antigo — fala de 5 s, pausa de 1 s —, agora com relógio
    // independente a +300 ppm. Aqui as pausas de conforto drenam o excesso, e o
    // buffer não tem por que tirar nada da fala: zero emendas é o critério.
    let m = simular_cenario(Cenario {
        dtx: true,
        ..Cenario::fala(300.0, 0)
    });
    let r = relatar("DTX +300 ppm", &m);
    eprintln!("{r}");
    assert!(m.pior_fora_da_recuperacao <= TETO_MS, "{r}");
    assert_eq!(m.descartados, 0, "emendou fala que a pausa pagaria: {r}");
    assert_eq!(m.ocultados, 0, "DTX lido como perda: {r}");
    assert_eq!(m.fora_de_ordem, 0, "{r}");
}

#[test]
fn com_dtx_e_pausa_de_cpu_a_recuperacao_tambem_vale() {
    let m = simular_cenario(Cenario {
        dtx: true,
        ..Cenario::fala(300.0, 500)
    });
    let r = relatar("DTX +300 ppm, pausa de 500 ms", &m);
    eprintln!("{r}");
    assert!(
        m.recuperou_em_ms.is_some_and(|t| t <= RECUPERACAO_MS),
        "{r}"
    );
    assert!(m.idade_11min <= TETO_MS && m.idade_30min <= TETO_MS, "{r}");
    assert!(m.pior_fora_da_recuperacao <= TETO_MS, "{r}");
    assert_eq!(m.ocultados, 0, "{r}");
    assert_eq!(m.fora_de_ordem, 0, "{r}");
}

#[test]
fn a_volta_do_carimbo_de_32_bits_nao_quebra_a_compensacao() {
    // O carimbo de 48 kHz em 32 bits dá a volta a cada 24,8 h. Começando 20 s
    // antes dela, a volta cai no meio da fala, com deriva e com a pausa de CPU
    // depois: a emenda compara carimbos, e tem que compará-los pela volta.
    for pausa_ms in [0, 500] {
        let m = simular_cenario(Cenario {
            carimbo_inicial: u32::MAX - 960 * 1_000,
            ..Cenario::fala(300.0, pausa_ms)
        });
        let r = relatar(&format!("volta do carimbo, pausa {pausa_ms}"), &m);
        eprintln!("{r}");
        assert!(m.pior_fora_da_recuperacao <= TETO_MS, "{r}");
        assert!(m.idade_30min <= TETO_MS, "{r}");
        assert!(m.descartados <= quadros_de_deriva(300.0) + 23 + 2, "{r}");
        assert_eq!(m.vaos, 0, "{r}");
        assert_eq!(m.ocultados, 0, "{r}");
        assert_eq!(m.fora_de_ordem, 0, "{r}");
    }
}
