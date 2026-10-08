//! O toque da sala pessoal: um tom sintetizado, tocado só na saída.
//!
//! Spec de 23/09, «O toque»: a webview minimizada estrangula um `setInterval`,
//! e o app não tinha som de interface nenhum. O toque é tocado pelo Rust, na
//! saída escolhida no SEELE, **sem abrir o microfone**: quem não está numa
//! ligação não pode ver o indicador de microfone acender porque alguém ligou.
//! Sem arquivo e sem `data:` na CSP, porque o som é feito aqui.
//!
//! # A forma
//!
//! Duas notas curtas e um silêncio, num ciclo de dois segundos. Cada nota sobe
//! e desce em [`BORDA_MS`]: um degrau na forma de onda é um clique.
//!
//! # Regras de tempo real
//!
//! [`Toque::preencher`] roda dentro do callback do `cpal`: sem alocação, sem
//! trava, sem registro. `tests/realtime_safety.rs` prova.

use crate::rt::RawSample;

/// A primeira nota, em hertz (um sol).
pub const NOTA_BAIXA_HZ: f32 = 784.0;
/// A segunda nota, em hertz (um dó acima).
pub const NOTA_ALTA_HZ: f32 = 1_046.5;
/// Quanto dura cada nota.
pub const NOTA_MS: u32 = 180;
/// O ciclo inteiro: as duas notas e o silêncio.
pub const CICLO_MS: u32 = 2_000;
/// Quanto cada nota leva para subir e para descer.
pub const BORDA_MS: u32 = 6;
/// O volume padrão: audível, e longe do teto da escala.
pub const VOLUME: f32 = 0.2;

/// O gerador do toque, numa taxa de amostragem.
#[derive(Debug, Clone)]
pub struct Toque {
    taxa_hz: u32,
    amostra: u64,
    volume: f32,
}

impl Toque {
    /// Um toque no começo do ciclo, no volume padrão.
    #[must_use]
    pub fn novo(taxa_hz: u32) -> Self {
        Self {
            taxa_hz: taxa_hz.max(1),
            amostra: 0,
            volume: VOLUME,
        }
    }

    /// O mesmo toque noutro volume, de 0 a 1.
    #[must_use]
    pub fn com_volume(self, volume: f32) -> Self {
        Self {
            volume: volume.clamp(0.0, 1.0),
            ..self
        }
    }

    /// A próxima amostra, mono.
    pub fn proxima(&mut self) -> f32 {
        let taxa = u64::from(self.taxa_hz);
        let ciclo = (taxa * u64::from(CICLO_MS) / 1000).max(1);
        let nota = (taxa * u64::from(NOTA_MS) / 1000).max(1);
        let borda = (taxa * u64::from(BORDA_MS) / 1000).max(1);
        let no_ciclo = self.amostra % ciclo;
        self.amostra = (self.amostra + 1) % ciclo;

        let (frequencia, dentro) = if no_ciclo < nota {
            (NOTA_BAIXA_HZ, no_ciclo)
        } else if no_ciclo < 2 * nota {
            (NOTA_ALTA_HZ, no_ciclo - nota)
        } else {
            return 0.0;
        };
        // Sobe no começo da nota e desce no fim: zero nas duas pontas.
        let ate_o_fim = nota - 1 - dentro;
        let envelope = (dentro.min(ate_o_fim) as f32 / borda as f32).min(1.0);
        let segundos = dentro as f32 / self.taxa_hz as f32;
        self.volume * envelope * (2.0 * std::f32::consts::PI * frequencia * segundos).sin()
    }

    /// Preenche um bloco intercalado de `canais` canais com a mesma amostra
    /// em cada um.
    pub fn preencher<T: RawSample>(&mut self, saida: &mut [T], canais: usize) {
        for quadro in saida.chunks_mut(canais.max(1)) {
            let amostra = T::from_f32(self.proxima());
            for lugar in quadro {
                *lugar = amostra;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAXA: u32 = 48_000;

    fn um_ciclo() -> Vec<f32> {
        let mut toque = Toque::novo(TAXA);
        (0..TAXA as usize * CICLO_MS as usize / 1000)
            .map(|_| toque.proxima())
            .collect()
    }

    #[test]
    fn o_ciclo_tem_duas_notas_e_depois_silencio() {
        let ciclo = um_ciclo();
        let fim_das_notas = TAXA as usize * 2 * NOTA_MS as usize / 1000;
        assert!(
            ciclo
                .iter()
                .take(fim_das_notas)
                .any(|a| a.abs() > VOLUME * 0.9),
            "as notas não soaram no começo do ciclo"
        );
        assert!(
            ciclo.iter().skip(fim_das_notas).all(|a| *a == 0.0),
            "o silêncio do ciclo tem som"
        );
    }

    #[test]
    fn nenhuma_amostra_passa_do_volume() {
        assert!(
            um_ciclo().iter().all(|a| a.abs() <= VOLUME + f32::EPSILON),
            "o toque passou do volume pedido"
        );
    }

    #[test]
    fn nao_ha_degrau_na_forma_de_onda() {
        // A maior inclinação de uma senoide de amplitude A em f Hz é 2πfA por
        // segundo. Um degrau entre duas amostras acima disso é um clique.
        let teto = 2.0 * std::f32::consts::PI * NOTA_ALTA_HZ * VOLUME / TAXA as f32 * 1.05;
        let ciclo = um_ciclo();
        let pior = ciclo
            .windows(2)
            .filter_map(|par| match par {
                [antes, depois] => Some((depois - antes).abs()),
                _ => None,
            })
            .fold(0.0_f32, f32::max);
        assert!(
            pior <= teto,
            "o toque tem um degrau de {pior}, acima de {teto}: isso é um clique"
        );
    }

    #[test]
    fn o_toque_volta_ao_comeco_a_cada_ciclo() {
        let mut toque = Toque::novo(TAXA);
        let primeiro: Vec<f32> = (0..100).map(|_| toque.proxima()).collect();
        for _ in 100..TAXA as usize * CICLO_MS as usize / 1000 {
            toque.proxima();
        }
        let segundo: Vec<f32> = (0..100).map(|_| toque.proxima()).collect();
        assert_eq!(
            primeiro, segundo,
            "o segundo ciclo não começou igual ao primeiro"
        );
    }

    #[test]
    fn mudo_e_silencio() {
        let mut toque = Toque::novo(TAXA).com_volume(0.0);
        assert!(
            (0..TAXA).all(|_| toque.proxima() == 0.0),
            "o toque mudo soou"
        );
    }

    #[test]
    fn preencher_poe_a_mesma_amostra_em_cada_canal() {
        let mut estereo = vec![0.0_f32; 2 * 480];
        Toque::novo(TAXA).preencher(&mut estereo, 2);
        let mut mono = Toque::novo(TAXA);
        for quadro in estereo.chunks(2) {
            let esperado = mono.proxima();
            assert_eq!(quadro, [esperado, esperado], "os canais divergiram");
        }
    }
}
