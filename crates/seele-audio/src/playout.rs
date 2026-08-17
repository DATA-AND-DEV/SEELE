//! Quantos quadros de 20 ms o dispositivo já devia ter recebido.
//!
//! `specs/03-audio.md` põe a reprodução num quadro de 20 ms, e o laço de voz
//! (`seele-core`) empurra esse quadro de dentro de um laço que também captura,
//! codifica, envia e recebe. Esse laço **não é um relógio**: ele dorme um
//! punhado de milissegundos por volta e a duração real dessa soneca é do
//! sistema operacional, não nossa.
//!
//! # O defeito que este módulo existe para impedir
//!
//! O laço perguntava `if agora >= próximo` e produzia **um** quadro. Enquanto a
//! volta do laço durar bem menos que 20 ms isso se sustenta: um atraso qualquer
//! é reposto nas voltas seguintes, porque cada volta entrega 20 ms de áudio
//! gastando três de relógio.
//!
//! Quando a volta passa a durar **mais** que 20 ms, a mesma linha vira um
//! vazamento permanente: entrega 20 ms de áudio a cada volta, e a volta custa
//! mais que 20 ms de relógio. O anel de reprodução esvazia na diferença, para
//! sempre, e o que sai do alto-falante é picotado — não por perda de rede, não
//! por codec, mas porque ninguém encheu o anel a tempo. **Nada no áudio
//! recebido explicaria isso**, e é por isso que essa falha se disfarça tão bem
//! de problema de rede.
//!
//! A duração da volta depende da granularidade do temporizador do sistema, que
//! não é a mesma em toda parte. Um laço afinado numa máquina onde
//! `sleep(2 ms)` dorme 2 ms passa a vazar numa máquina onde a mesma chamada
//! dorme 16 — e as duas máquinas rodam o mesmo binário, na mesma sala, na
//! mesma conversa.
//!
//! # O que este relógio faz em vez disso
//!
//! [`PlayoutClock::due`] responde **quantos** quadros venceram, e não se um
//! venceu. Repor é a regra, não a exceção. O que ele não faz é repor sem
//! limite: uma máquina que hibernou volta com horas de atraso, e despejar horas
//! de áudio no anel seria trocar um defeito audível por um travamento.
//!
//! # E é o instrumento, também
//!
//! [`PlayoutMetrics::worst_lateness_ms`] é a duração da volta do laço, medida
//! no único lugar que importa: o ponto onde o prazo é conferido. Ler esse
//! número numa máquina que pica o áudio responde a pergunta que custou uma
//! rodada de palpites — *é a rede ou é esta máquina?* — com um número, e não
//! com uma suspeita.

use std::time::{Duration, Instant};

/// Quantos quadros de atraso o relógio repõe de uma vez.
///
/// Quatro quadros são 80 ms, e o anel entre o laço e o dispositivo guarda 100
/// (`RING_MS` em `seele-core`). Repor mais do que cabe no anel não é repor: o
/// excedente é descartado na entrada do anel, e o que se ganha é o custo de ter
/// decodificado. Acima disto o relógio prefere reacertar — ver
/// [`PlayoutMetrics::resyncs`].
const MAX_CATCHUP_FRAMES: u32 = 4;

/// O que o relógio de reprodução viu acontecer.
///
/// Dados puros, como todo este crate. Quem desenha decide o que "atraso máximo
/// de 31 ms" parece na tela.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayoutMetrics {
    /// Quadros que [`PlayoutClock::due`] mandou produzir, desde o começo.
    pub frames_due: u64,

    /// Quantos desses foram reposição, e não o quadro da vez.
    ///
    /// **Zero é o normal.** Qualquer coisa acima disso significa que o laço de
    /// voz está demorando mais que um quadro por volta, e que a reprodução só
    /// está em dia porque este relógio a está segurando. Era exatamente o
    /// número que faltava para separar "a rede está picotando" de "esta máquina
    /// não consegue empurrar 50 quadros por segundo".
    pub catchup_frames: u64,

    /// Quantas vezes o atraso passou do que dá para repor e o relógio reacertou.
    ///
    /// Uma hibernação produz uma. Muitas seguidas significam que a máquina para
    /// por dezenas de milissegundos de cada vez.
    pub resyncs: u64,

    /// O maior atraso já visto ao conferir o prazo, em milissegundos.
    ///
    /// Esta é a duração da volta do laço de voz, medida onde ela dói. Abaixo de
    /// um quadro (20 ms) o laço acompanha; acima, ele só acompanha porque este
    /// relógio repõe. É o número a pedir de quem relata áudio picotado.
    pub worst_lateness_ms: f64,
}

/// Diz quando os quadros de reprodução vencem, e quantos.
///
/// Construído com o instante de partida e conferido a cada volta do laço. Não
/// tem relógio próprio de propósito: quem chama passa o `Instant`, o que torna
/// cada comportamento aqui verificável sem dormir e sem placa de som.
#[derive(Debug)]
pub struct PlayoutClock {
    period: Duration,
    /// Quando o próximo quadro vence.
    next: Instant,
    metrics: PlayoutMetrics,
}

impl PlayoutClock {
    /// Um relógio cujo primeiro quadro vence imediatamente.
    ///
    /// `frame_ms` é a duração do quadro — [`crate::FRAME_MS`] em todo uso real.
    /// Zero é tratado como um milissegundo: um período nulo faria [`Self::due`]
    /// dividir por zero, e um laço travado é pior que um quadro fora de compasso.
    #[must_use]
    pub fn new(now: Instant, frame_ms: u32) -> Self {
        Self {
            period: Duration::from_millis(u64::from(frame_ms.max(1))),
            next: now,
            metrics: PlayoutMetrics::default(),
        }
    }

    /// Quantos quadros produzir agora.
    ///
    /// Zero quando ainda não venceu nenhum. Um no compasso normal. Mais de um
    /// quando a volta anterior do laço demorou mais que um quadro — que é o
    /// caso que a versão anterior deste código não repunha, e por isso vazava.
    ///
    /// Acima de [`MAX_CATCHUP_FRAMES`] o relógio devolve o teto e reacerta o
    /// prazo em `now`, porque repor mais do que cabe no anel só produz áudio
    /// que será descartado na entrada dele.
    pub fn due(&mut self, now: Instant) -> u32 {
        let Some(late) = now.checked_duration_since(self.next) else {
            return 0;
        };

        let lateness_ms = late.as_secs_f64() * 1000.0;
        if lateness_ms > self.metrics.worst_lateness_ms {
            self.metrics.worst_lateness_ms = lateness_ms;
        }

        // `period` nunca é zero — ver `new`.
        let elapsed = late.as_nanos();
        let period = self.period.as_nanos().max(1);
        let overdue = u32::try_from(elapsed / period).unwrap_or(u32::MAX);
        let frames = overdue.saturating_add(1);

        if frames > MAX_CATCHUP_FRAMES {
            self.metrics.resyncs = self.metrics.resyncs.saturating_add(1);
            self.next = now + self.period;
            self.record(MAX_CATCHUP_FRAMES);
            return MAX_CATCHUP_FRAMES;
        }

        self.next += self.period * frames;
        self.record(frames);
        frames
    }

    /// O que este relógio viu.
    #[must_use]
    pub fn metrics(&self) -> PlayoutMetrics {
        self.metrics
    }

    fn record(&mut self, frames: u32) {
        self.metrics.frames_due = self.metrics.frames_due.saturating_add(u64::from(frames));
        self.metrics.catchup_frames = self
            .metrics
            .catchup_frames
            .saturating_add(u64::from(frames.saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FRAME_MS;

    /// Um instante de partida qualquer, longe do zero do relógio da máquina.
    fn base() -> Instant {
        Instant::now()
    }

    fn ms(count: u64) -> Duration {
        Duration::from_millis(count)
    }

    #[test]
    fn o_primeiro_quadro_vence_na_hora() {
        // Sem isto o áudio começaria 20 ms depois de haver o que tocar, o que
        // não é audível — mas o teste existe para fixar de onde o compasso
        // parte, já que tudo abaixo conta a partir daqui.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);
        assert_eq!(clock.due(start), 1);
        assert_eq!(clock.due(start), 0, "o segundo só vence 20 ms depois");
    }

    #[test]
    fn no_compasso_normal_sai_um_quadro_por_periodo() {
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);

        for tick in 0..100_u64 {
            assert_eq!(
                clock.due(start + ms(tick * 20)),
                1,
                "a tica {tick} devia render exatamente um quadro"
            );
        }
        assert_eq!(clock.metrics().catchup_frames, 0, "nada a repor aqui");
        assert_eq!(clock.metrics().frames_due, 100);
    }

    #[test]
    fn um_laco_mais_lento_que_o_quadro_continua_em_tempo_real() {
        // **O defeito, em número.** Um laço cuja volta dura 31 ms — que é o que
        // um `sleep(2 ms)` mais um `timeout(1 ms)` custam numa máquina cujo
        // temporizador tem granularidade de 15,6 ms — precisa de 1,55 quadros
        // por volta. Entregando um, entrega 64% do áudio e o resto é silêncio
        // no anel: exatamente o som picotado que se atribui à rede.
        //
        // Aqui o relógio tem que fechar a conta: em dez segundos de relógio,
        // dez segundos de áudio, com folga de um quadro.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);

        let volta_ms = 31_u64;
        let voltas = 10_000 / volta_ms;
        let mut frames = 0_u64;
        for volta in 0..voltas {
            frames += u64::from(clock.due(start + ms(volta * volta_ms)));
        }

        let esperado = (voltas * volta_ms) / u64::from(FRAME_MS);
        assert!(
            frames.abs_diff(esperado) <= 1,
            "produziu {frames} quadros onde o relógio pedia {esperado} — \
             o anel de reprodução esvazia na diferença"
        );
        assert!(
            clock.metrics().catchup_frames > 0,
            "um laço mais lento que o quadro tem que aparecer como reposição"
        );
        assert_eq!(
            clock.metrics().resyncs,
            0,
            "31 ms de volta é atraso de rotina, não motivo para reacertar"
        );
    }

    #[test]
    fn uma_pausa_curta_e_reposta_inteira() {
        // Uma volta que atrasou 80 ms — um redesenho pesado, um GC da casca —
        // é reposta, e é reposta de uma vez. É o caso que a versão com `if`
        // repunha devagar e só enquanto a volta fosse curta.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);
        assert_eq!(clock.due(start), 1);

        assert_eq!(
            clock.due(start + ms(80)),
            4,
            "80 ms de atraso são quatro quadros, e todos os quatro são devidos"
        );
        assert_eq!(clock.metrics().catchup_frames, 3);
        assert_eq!(clock.metrics().resyncs, 0);
    }

    #[test]
    fn uma_hibernacao_reacerta_em_vez_de_despejar_horas_de_audio() {
        // O outro extremo, e o motivo de haver teto: repor uma hora de atraso
        // seriam 180 mil quadros decodificados numa volta só. O laço não
        // voltaria, e o sintoma seria um travamento no lugar de um estalo.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);
        assert_eq!(clock.due(start), 1);

        let depois = start + Duration::from_secs(3600);
        assert_eq!(clock.due(depois), MAX_CATCHUP_FRAMES);
        assert_eq!(clock.metrics().resyncs, 1);

        // E o compasso volta a andar a partir de agora, não de uma hora atrás.
        assert_eq!(clock.due(depois), 0, "reacertou para o presente");
        assert_eq!(clock.due(depois + ms(20)), 1);
    }

    #[test]
    fn o_atraso_maximo_e_a_duracao_da_volta_do_laco() {
        // A métrica que torna a próxima medição conclusiva. Ela não é uma
        // estatística sobre áudio: é quanto tempo passou entre duas conferidas
        // do prazo, que é a volta do laço de voz. Abaixo de 20 ms o laço
        // acompanha sozinho; acima, só acompanha porque isto repõe.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);
        clock.due(start);
        clock.due(start + ms(51));

        let visto = clock.metrics().worst_lateness_ms;
        assert!(
            (visto - 31.0).abs() < 0.5,
            "o atraso medido foi {visto:.1} ms, e a volta durou 51 - 20 = 31"
        );

        // Uma volta curta depois não apaga o pior caso: quem lê isto quer saber
        // se a máquina *alguma vez* passou do prazo.
        clock.due(start + ms(71));
        assert!((clock.metrics().worst_lateness_ms - 31.0).abs() < 0.5);
    }

    #[test]
    fn um_relogio_com_periodo_zero_nao_divide_por_zero() {
        // Não acontece com `FRAME_MS`, e é o tipo de coisa que só se descobre
        // em produção. Um milissegundo é o piso.
        let start = base();
        let mut clock = PlayoutClock::new(start, 0);
        assert!(clock.due(start + ms(10)) > 0);
    }

    #[test]
    fn perguntar_antes_do_prazo_nao_produz_nada_e_nao_estraga_o_compasso() {
        // O laço pergunta muito mais vezes do que produz — é o caso comum.
        let start = base();
        let mut clock = PlayoutClock::new(start, FRAME_MS);
        clock.due(start);

        for antes in 1..20_u64 {
            assert_eq!(clock.due(start + ms(antes)), 0);
        }
        assert_eq!(clock.due(start + ms(20)), 1);
        assert_eq!(clock.metrics().frames_due, 2);
    }
}
