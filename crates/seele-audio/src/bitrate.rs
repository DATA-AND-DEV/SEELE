//! O bitrate segue a perda de subida, em três faixas.
//!
//! `specs/03-audio.md` fecha o parâmetro em «16–48 kbps, adaptativo», e detalha:
//! «cai para 16 kbps sob perda > 5%, sobe de volta gradualmente». Este módulo é
//! essa frase, e nada além dela.
//!
//! # Por que faixas, e não uma curva
//!
//! Porque trocar de bitrate **reconstrói o encoder**: o `shiguredo_opus` não
//! expõe setter em tempo de execução (ADR 0008, conferido no fonte do binding), e
//! a reconstrução custa um quadro sem histórico de predição. O ADR 0010 chamou
//! uma malha contínua de inaceitável por isso, e tinha razão.
//!
//! Três faixas, com histerese e permanência, tornam a troca rara — um punhado por
//! chamada, e nenhuma numa chamada cujo regime de rede não muda. A objeção não é
//! contornada: ela é a restrição que desenha este módulo. Ver o ADR 0036.
//!
//! # Por que o relógio é parâmetro
//!
//! Porque a permanência é a metade da malha que mais erra, e um teste que
//! provasse «não sobe antes de dez segundos» com um `sleep` de dez segundos não
//! seria rodado por ninguém.

use std::time::{Duration, Instant};

/// As faixas, da melhor para a pior.
///
/// Os extremos são os de `specs/03-audio.md`. O ponto do meio existe para que a
/// queda sob perda moderada não vá direto ao piso — cair ao fundo por 6% de
/// perda gastaria qualidade que o enlace ainda comportava.
pub const FAIXAS_BPS: [u32; 3] = [48_000, 32_000, 16_000];

/// Quando descer, quando subir, e quanto esperar antes de subir.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limiares {
    /// Acima disto, desce uma faixa na medida seguinte.
    pub descer_acima_de: f32,
    /// Abaixo disto, começa a contar a permanência para subir.
    pub subir_abaixo_de: f32,
    /// Quanto tempo a perda tem de ficar boa antes de uma subida.
    pub permanencia: Duration,
}

impl Default for Limiares {
    fn default() -> Self {
        Self {
            // `specs/03-audio.md`, textual.
            descer_acima_de: 0.05,
            // Três pontos de histerese. Larga o bastante para que ruído de
            // medida não atravesse os dois limiares na mesma chamada, que é o
            // que produziria troca de faixa sem mudança de regime.
            subir_abaixo_de: 0.02,
            permanencia: Duration::from_secs(10),
        }
    }
}

/// Escolhe a faixa a partir de uma sequência de medidas de perda.
#[derive(Debug)]
pub struct Controlador {
    limiares: Limiares,
    /// Índice em [`FAIXAS_BPS`]. Zero é a melhor.
    indice: usize,
    /// Desde quando a perda está abaixo do limiar de subida, sem interrupção.
    bom_desde: Option<Instant>,
}

impl Default for Controlador {
    fn default() -> Self {
        Self::novo()
    }
}

impl Controlador {
    /// Um controlador na faixa de cima, com os limiares da spec.
    #[must_use]
    pub fn novo() -> Self {
        Self::com_limiares(Limiares::default())
    }

    /// Um controlador na faixa de cima, com limiares escolhidos.
    #[must_use]
    pub fn com_limiares(limiares: Limiares) -> Self {
        Self {
            limiares,
            // Começa no teto e desce sob evidência. É o que «adaptativo» quer
            // dizer, e é o que responde ao pedido de qualidade máxima sem
            // inventar um número que a spec não tenha.
            indice: 0,
            bom_desde: None,
        }
    }

    /// A faixa em vigor.
    #[must_use]
    pub fn bitrate_bps(&self) -> u32 {
        FAIXAS_BPS
            .get(self.indice)
            .copied()
            .unwrap_or(FAIXAS_BPS[0])
    }

    /// Dobra uma medida de perda, e diz se a faixa mudou.
    ///
    /// `Some(bps)` **só** quando houve troca: quem chama reconstrói o encoder com
    /// isso, e devolver o valor atual a cada medida faria a reconstrução
    /// acontecer cinquenta vezes por segundo — exatamente o que este desenho
    /// existe para não fazer.
    pub fn observar(&mut self, perda: f32, agora: Instant) -> Option<u32> {
        if perda > self.limiares.descer_acima_de {
            // Descer é imediato: quem está perdendo pacote já está sendo ouvido
            // mal, e esperar para confirmar é esperar em cima do problema.
            self.bom_desde = None;
            if self.indice + 1 < FAIXAS_BPS.len() {
                self.indice += 1;
                return Some(self.bitrate_bps());
            }
            return None;
        }

        if perda >= self.limiares.subir_abaixo_de {
            // A zona morta entre os dois limiares. Nem desce nem conta tempo para
            // subir — e zerar a contagem aqui é o que impede uma medida
            // beirando o limiar de acumular permanência aos pedaços.
            self.bom_desde = None;
            return None;
        }

        match self.bom_desde {
            None => {
                self.bom_desde = Some(agora);
                None
            }
            Some(desde) => {
                if agora.duration_since(desde) < self.limiares.permanencia {
                    return None;
                }
                // A contagem recomeça a cada subida: subir duas faixas exige
                // duas permanências inteiras, que é o «gradualmente» da spec.
                self.bom_desde = Some(agora);
                if self.indice == 0 {
                    return None;
                }
                self.indice -= 1;
                Some(self.bitrate_bps())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Controlador, Limiares, FAIXAS_BPS};
    use std::time::{Duration, Instant};

    fn limiares() -> Limiares {
        Limiares {
            descer_acima_de: 0.05,
            subir_abaixo_de: 0.02,
            permanencia: Duration::from_secs(10),
        }
    }

    #[test]
    fn comeca_na_faixa_de_cima() {
        // O pedido de qualidade máxima é este: começa-se onde a qualidade é
        // melhor e desce-se sob evidência, em vez de começar no meio por
        // precaução e nunca subir.
        assert_eq!(Controlador::novo().bitrate_bps(), FAIXAS_BPS[0]);
        assert_eq!(FAIXAS_BPS[0], 48_000);
    }

    #[test]
    fn perda_alta_derruba_uma_faixa_por_medida() {
        let agora = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());

        assert_eq!(controlador.observar(0.10, agora), Some(32_000));
        assert_eq!(controlador.observar(0.10, agora), Some(16_000));
    }

    #[test]
    fn o_piso_da_spec_e_o_piso_de_verdade() {
        // Dezesseis kbps é o fundo declarado. Abaixo dele não há faixa, e
        // continuar «descendo» seria devolver troca sem troca — cada uma
        // reconstruindo o encoder por nada.
        let agora = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, agora);
        let _ = controlador.observar(0.10, agora);

        assert_eq!(controlador.bitrate_bps(), 16_000);
        assert_eq!(
            controlador.observar(0.99, agora),
            None,
            "desceu abaixo do piso da spec"
        );
    }

    #[test]
    fn nao_sobe_antes_da_permanencia_inteira() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);
        assert_eq!(controlador.bitrate_bps(), 32_000);

        // Nove segundos de rede boa não bastam. É o «gradualmente» da spec, e é
        // o que impede uma trégua curta de custar uma reconstrução.
        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(9)),
            None
        );
    }

    #[test]
    fn sobe_depois_da_permanencia() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);

        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(10)),
            Some(48_000)
        );
    }

    #[test]
    fn subir_duas_faixas_custa_duas_permanencias() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);
        let _ = controlador.observar(0.10, inicio);
        assert_eq!(controlador.bitrate_bps(), 16_000);

        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(10)),
            Some(32_000)
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(15)),
            None,
            "subiu a segunda faixa com meia permanência"
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(20)),
            Some(48_000)
        );
    }

    /// O teste que justifica a histerese existir.
    ///
    /// Uma medida oscilando dentro da zona morta não pode produzir troca
    /// nenhuma. Com um limiar único ela produziria uma por medida, e cada uma
    /// reconstrói o encoder — o pior de todos os mundos, e o defeito que este
    /// desenho existe para não ter.
    #[test]
    fn oscilar_na_zona_morta_nao_troca_de_faixa() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());

        for passo in 0..100_u64 {
            let perda = if passo % 2 == 0 { 0.03 } else { 0.045 };
            assert_eq!(
                controlador.observar(perda, inicio + Duration::from_secs(passo)),
                None,
                "trocou de faixa no passo {passo} sem a rede mudar de regime"
            );
        }
        assert_eq!(controlador.bitrate_bps(), 48_000);
    }

    /// Uma medida ruim no meio zera a contagem da permanência.
    #[test]
    fn uma_medida_ruim_recomeca_a_contagem() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);

        assert_eq!(controlador.observar(0.0, inicio), None);
        // Oito segundos de bom, depois uma medida na zona morta.
        assert_eq!(
            controlador.observar(0.03, inicio + Duration::from_secs(8)),
            None
        );
        // Os dez segundos contam do zero, e não do começo.
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(9)),
            None
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(18)),
            None,
            "aproveitou a contagem anterior à interrupção"
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(19)),
            Some(48_000)
        );
    }
}
