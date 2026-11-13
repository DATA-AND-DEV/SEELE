//! A supressão de ruído por rede neural: o RNNoise, em Rust puro.
//!
//! # Por que existe
//!
//! Relato do dono (10/11/2026): «o nosso não funciona direito, ainda estão
//! ouvindo as batidas no teclado». A [`crate::supressao::Supressao`] é
//! subtração espectral: aprende o ruído que fica (ventilador, ar-condicionado)
//! e não tem como aprender um estalo. Medido em
//! `docs/medidas/2026-11-11-teclado-na-supressao.md`: a batida atravessa a
//! subtração espectral com 0,6 dB a menos, e o RNNoise com 8,5 dB a menos. O
//! cabeçalho de `supressao.rs` previa as duas coisas desde que foi escrito.
//!
//! **Reduz, não remove.** A medida é com sinal sintético; uma tecla de verdade
//! pode sair melhor ou pior que isso.
//!
//! # O que ela é
//!
//! O `nnnoiseless`, a porta em Rust puro do RNNoise da Xiph (BSD-3-Clause),
//! com o modelo dentro do crate: nenhuma ligação em C (ADR 0007), nenhum
//! arquivo a empacotar.
//!
//! # A mesma interface, a mesma ordem, o mesmo atraso
//!
//! Ela entra no lugar da subtração espectral em
//! `captura → supressão → decisão de ativação → ganho → codificação`, com os
//! mesmos métodos, e atrasa o sinal os mesmos **10 ms** (um bloco do RNNoise).
//!
//! # A força
//!
//! A rede não tem botão de intensidade. A força do produto (0 a 1) é a
//! mistura entre o sinal limpo e o original, este atrasado os mesmos 10 ms
//! para as duas metades somarem em fase. Em zero o quadro sai como entrou,
//! sem atraso: é o caminho sem filtro que o F02 pede.
//!
//! # Tempo real
//!
//! Tudo é reservado na construção. [`SupressaoNeural::processar`] não aloca,
//! e é o que `tests/realtime_safety.rs` confere.

use nnnoiseless::DenoiseState;

/// O bloco do RNNoise: 480 amostras, 10 ms a 48 kHz. Meio quadro do produto.
const BLOCO: usize = DenoiseState::FRAME_SIZE;

/// O RNNoise trabalha na escala de 16 bits; o produto, em −1 a 1.
const ESCALA: f32 = 32_768.0;

/// A supressão de ruído por rede neural. Ver o cabeçalho do módulo.
pub struct SupressaoNeural {
    forca: f32,
    estado: Box<DenoiseState<'static>>,
    /// O que chegou e ainda não completou um bloco.
    pendente: [f32; BLOCO],
    pendentes: usize,
    /// O bloco anterior, como entrou: é o original atrasado da mistura.
    anterior: [f32; BLOCO],
    /// A entrada do bloco, na escala do RNNoise, e a saída dele.
    dentro: [f32; BLOCO],
    fora: [f32; BLOCO],
    /// A chance de voz que a rede deu ao último bloco, de 0 a 1.
    chance_de_voz: f32,
}

impl std::fmt::Debug for SupressaoNeural {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupressaoNeural")
            .field("forca", &self.forca)
            .field("pendentes", &self.pendentes)
            .finish_non_exhaustive()
    }
}

impl SupressaoNeural {
    /// Uma supressão nova, com a força dada (0 desliga, 1 é o sinal da rede).
    #[must_use]
    pub fn nova(forca: f32) -> Self {
        Self {
            forca: forca.clamp(0.0, 1.0),
            estado: DenoiseState::new(),
            pendente: [0.0; BLOCO],
            pendentes: 0,
            anterior: [0.0; BLOCO],
            dentro: [0.0; BLOCO],
            fora: [0.0; BLOCO],
            chance_de_voz: 0.0,
        }
    }

    /// A força de agora.
    #[must_use]
    pub const fn forca(&self) -> f32 {
        self.forca
    }

    /// Troca a força. Vale a partir do próximo bloco.
    pub fn ajustar(&mut self, forca: f32) {
        self.forca = forca.clamp(0.0, 1.0);
    }

    /// A chance de haver voz no último bloco processado, de 0 a 1, como a
    /// rede a estimou. Zero enquanto a supressão está desligada.
    #[must_use]
    pub const fn chance_de_voz(&self) -> f32 {
        self.chance_de_voz
    }

    /// Quanto a supressão atrasa o sinal, em amostras: um bloco, 10 ms.
    #[must_use]
    pub const fn atraso_em_amostras() -> usize {
        BLOCO
    }

    /// Passa um quadro pela supressão. `para` sai com o que ficou pronto.
    ///
    /// Com a força em zero, o quadro sai como entrou. Senão, cada bloco de 480
    /// amostras completo sai limpo, atrasado um bloco; o que sobra de um
    /// quadro que não é múltiplo de 480 espera o próximo.
    pub fn processar(&mut self, quadro: &[f32], para: &mut Vec<f32>) {
        para.clear();
        if self.forca <= 0.0 {
            // O caminho sem filtro: zero atraso, zero diferença no sinal.
            self.chance_de_voz = 0.0;
            para.extend_from_slice(quadro);
            return;
        }
        for amostra in quadro {
            if let Some(vaga) = self.pendente.get_mut(self.pendentes) {
                *vaga = *amostra;
            }
            self.pendentes += 1;
            if self.pendentes == BLOCO {
                self.pendentes = 0;
                self.um_bloco(para);
            }
        }
    }

    fn um_bloco(&mut self, para: &mut Vec<f32>) {
        for (dentro, amostra) in self.dentro.iter_mut().zip(&self.pendente) {
            *dentro = amostra * ESCALA;
        }
        self.chance_de_voz = self.estado.process_frame(&mut self.fora, &self.dentro);
        // A saída da rede corresponde ao bloco **anterior** (o atraso dela), e
        // é com ele que se mistura.
        let limpo = self.forca;
        let original = 1.0 - self.forca;
        for (fora, antes) in self.fora.iter().zip(&self.anterior) {
            para.push((fora / ESCALA).mul_add(limpo, antes * original));
        }
        self.anterior = self.pendente;
    }
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use crate::supressao::Supressao;
    use crate::FRAME_SAMPLES;

    const TAXA: usize = 48_000;

    /// Um gerador repetível.
    struct Sorteio(u64);
    impl Sorteio {
        fn proximo(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32).mul_add(2.0, -1.0)
        }
    }

    /// Sala a −55 dBFS e um estalo a −18 dBFS a cada 180 ms, depois de 2 s só
    /// de sala. Devolve o sinal e onde estão os estalos.
    fn teclado(segundos: usize) -> (Vec<f32>, Vec<bool>) {
        let total = TAXA * segundos;
        let piso = 10f32.powf(-55.0 / 20.0);
        let estalo = 10f32.powf(-18.0 / 20.0);
        let periodo = TAXA * 180 / 1000;
        let duracao = TAXA * 6 / 1000;
        let mut sorteio = Sorteio(7);
        let mut sinal = vec![0f32; total];
        let mut marcas = vec![false; total];
        for i in 0..total {
            sinal[i] = piso * sorteio.proximo();
            let fase = i % periodo;
            if i > 2 * TAXA && fase < duracao * 4 {
                let queda = (-(fase as f32) / (duracao as f32 / 3.0)).exp();
                sinal[i] += estalo * queda * sorteio.proximo();
                marcas[i] = fase < duracao * 2;
            }
        }
        (sinal, marcas)
    }

    /// Harmônicos de 140 Hz com envelope de sílaba, a −20 dBFS, sobre a sala.
    fn voz(segundos: usize) -> Vec<f32> {
        let piso = 10f32.powf(-55.0 / 20.0);
        let amplitude = 10f32.powf(-20.0 / 20.0);
        let mut sorteio = Sorteio(11);
        (0..TAXA * segundos)
            .map(|i| {
                let t = i as f32 / TAXA as f32;
                let silaba = 0.5f32
                    .mul_add(-(std::f32::consts::TAU * 3.0 * t).cos(), 0.5)
                    .powi(2);
                let soma: f32 = (1..=12)
                    .map(|h| (std::f32::consts::TAU * 140.0 * h as f32 * t).sin() / h as f32)
                    .sum();
                piso.mul_add(sorteio.proximo(), amplitude * 0.4 * silaba * soma)
            })
            .collect()
    }

    fn pela_neural(sinal: &[f32], forca: f32) -> Vec<f32> {
        let mut supressao = SupressaoNeural::nova(forca);
        let mut saida = Vec::new();
        let mut bloco = Vec::new();
        for quadro in sinal.chunks_exact(FRAME_SAMPLES) {
            supressao.processar(quadro, &mut bloco);
            saida.extend_from_slice(&bloco);
        }
        saida
    }

    fn pela_espectral(sinal: &[f32]) -> Vec<f32> {
        let mut supressao = Supressao::nova(1.0);
        let mut saida = Vec::new();
        let mut bloco = Vec::new();
        for quadro in sinal.chunks_exact(FRAME_SAMPLES) {
            supressao.processar(quadro, &mut bloco);
            saida.extend_from_slice(&bloco);
        }
        saida
    }

    /// Quantos dB o estalo sai abaixo do que entrou, com a saída alinhada.
    fn queda_do_estalo(sinal: &[f32], marcas: &[bool], saida: &[f32], atraso: usize) -> f64 {
        let (mut antes, mut depois) = (0f64, 0f64);
        for i in 3 * TAXA..sinal.len() - 2 * FRAME_SAMPLES {
            if marcas[i] {
                antes += f64::from(sinal[i]).powi(2);
                depois += f64::from(saida[i + atraso]).powi(2);
            }
        }
        10.0 * (antes / depois.max(1e-20)).log10()
    }

    /// **A batida de teclado cai, e cai muito mais que na subtração
    /// espectral.** É o motivo de este módulo existir: reverter a troca no
    /// caminho da voz volta a deixar a batida passar inteira.
    #[test]
    fn a_batida_de_teclado_cai_mais_que_na_subtracao_espectral() {
        let (sinal, marcas) = teclado(8);
        let neural = queda_do_estalo(
            &sinal,
            &marcas,
            &pela_neural(&sinal, 1.0),
            SupressaoNeural::atraso_em_amostras(),
        );
        let espectral = queda_do_estalo(
            &sinal,
            &marcas,
            &pela_espectral(&sinal),
            Supressao::atraso_em_amostras(),
        );
        println!("estalo: neural −{neural:.1} dB, espectral −{espectral:.1} dB");
        assert!(neural > 6.0, "a rede só tirou {neural:.1} dB do estalo");
        assert!(
            espectral < 2.0,
            "a espectral tirou {espectral:.1} dB: a medida mudou"
        );
        assert!(neural > espectral + 5.0);
    }

    /// A voz atravessa: quase toda a energia dela fica.
    #[test]
    fn a_voz_atravessa() {
        let sinal = voz(6);
        let saida = pela_neural(&sinal, 1.0);
        let energia = |x: &[f32]| -> f64 {
            x[2 * TAXA..5 * TAXA]
                .iter()
                .map(|a| f64::from(*a).powi(2))
                .sum()
        };
        let sobra = energia(&saida) / energia(&sinal);
        println!("voz: sobra {:.0}%", sobra * 100.0);
        assert!(sobra > 0.85, "só {:.0}% da voz ficou", sobra * 100.0);
    }

    /// **O atraso é o que o método diz**: quem alinha a voz com a supressão
    /// (o registro do laço de voz, a telemetria) confia neste número.
    #[test]
    fn o_atraso_e_de_um_bloco() {
        let sinal = voz(4);
        let saida = pela_neural(&sinal, 1.0);
        let parecido = |atraso: usize| -> f64 {
            (2 * TAXA..3 * TAXA)
                .map(|i| f64::from(sinal[i]) * f64::from(saida[i + atraso]))
                .sum()
        };
        let melhor = (0..1500)
            .max_by(|a, b| parecido(*a).total_cmp(&parecido(*b)))
            .unwrap();
        let dito = SupressaoNeural::atraso_em_amostras();
        assert!(
            melhor.abs_diff(dito) <= 8,
            "o sinal sai {melhor} amostras depois, e o método diz {dito}"
        );
    }

    /// Com a força em zero, o quadro sai como entrou, sem atraso; e a força
    /// do meio mistura (a batida cai menos que com a força inteira).
    #[test]
    fn a_forca_zero_nao_mexe_e_a_do_meio_mistura() {
        let (sinal, marcas) = teclado(6);
        assert_eq!(
            pela_neural(&sinal, 0.0),
            sinal[..sinal.len() / FRAME_SAMPLES * FRAME_SAMPLES]
        );
        let atraso = SupressaoNeural::atraso_em_amostras();
        let inteira = queda_do_estalo(&sinal, &marcas, &pela_neural(&sinal, 1.0), atraso);
        let meia = queda_do_estalo(&sinal, &marcas, &pela_neural(&sinal, 0.5), atraso);
        assert!(
            meia > 1.0 && meia < inteira,
            "meia {meia:.1} dB, inteira {inteira:.1} dB"
        );
    }

    /// Um quadro que não é múltiplo do bloco não perde amostra: o que sobra
    /// espera o próximo.
    #[test]
    fn um_quadro_quebrado_nao_perde_amostra() {
        let sinal = voz(1);
        let mut supressao = SupressaoNeural::nova(1.0);
        let mut bloco = Vec::new();
        let mut total = 0;
        for pedaco in sinal.chunks(700) {
            supressao.processar(pedaco, &mut bloco);
            total += bloco.len();
        }
        assert_eq!(total, sinal.len() / BLOCO * BLOCO);
    }
}
