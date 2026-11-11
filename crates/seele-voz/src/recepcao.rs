//! A recepção desta ponta, numa janela: a perda e o jitter da voz que chega,
//! como o cliente os relata ao servidor (`ClientMessage::RelatoDeRecepcao`,
//! protocolo 13).
//!
//! # Por que uma janela, e não a conta do buffer
//!
//! O [`JitterMetrics::loss_fraction`] é **cumulativo desde que o buffer
//! nasceu**: uma razão que, uma vez subida, só decai assintoticamente. Para a
//! telemetria local isso sempre foi aceitável; para o sinal que os outros
//! veem, não — dez segundos ruins no começo de uma chamada de duas horas
//! ficariam no número até o fim, e dez segundos ruins no fim quase não o
//! moveriam. É o mesmo defeito que `seele-server/src/perda_de_subida.rs`
//! descreve do contador do transporte, e a resposta é a mesma: contar sobre
//! uma janela deslizante.
//!
//! Aqui a janela se faz por diferença: a cada volta (uma por segundo) quem
//! chama entrega as contagens cumulativas de cada fonte, e este módulo guarda
//! quanto elas andaram desde a volta anterior.
//!
//! # O que conta como perda
//!
//! O mesmo que no buffer: dos lugares que deviam ter tocado, os que foram
//! ocultados ou silenciados. O silêncio de quem parou de falar (DTX) **não**
//! é lugar nem perda — uma sala quieta não é uma conexão ruim.
//!
//! # A pior perda, o menor jitter
//!
//! Com mais de uma pessoa falando, cada fonte dá um número, e o relato leva
//! um só de cada grandeza:
//!
//! - **a perda é a pior entre as fontes.** O que falta numa fonte é o que o
//!   caminho de descida desta ponta perdeu **mais** o que a subida de quem
//!   fala perdeu; o receptor não separa as duas, mas o servidor separa, porque
//!   mede a subida de cada pessoa. Ele precisa da pior para descontar dela a
//!   pior subida;
//! - **o jitter é o menor entre as fontes.** O servidor não mede jitter de
//!   ninguém e não tem com o que descontar. A descida é comum a todas as
//!   fontes, e o que uma tem a mais que outra é da subida de quem fala: o
//!   menor é o que mais se aproxima do caminho desta ponta. Com uma pessoa só
//!   falando, o número ainda carrega a subida dela — não há como separar.
//!
//! Só entra na conta a fonte que **tocou o bastante na janela**
//! ([`MINIMO_DE_LUGARES`]). Uma fonte calada continua na tabela de quem fala
//! por meio minuto, com o último jitter congelado: sem este corte, quem falou
//! há vinte segundos decidiria o número de agora.
//!
//! O relógio entra por argumento, em milissegundos, como em
//! [`crate::fontes`]: é o que deixa o navegador usar o mesmo código e os
//! testes não esperarem.

use std::collections::VecDeque;

use crate::jitter::JitterMetrics;

/// Sobre quanto tempo a recepção é contada, em milissegundos.
///
/// Cinco segundos: a janela da perda de subida do servidor, com que este
/// número é comparado, e a validade do relato no protocolo
/// (`seele_proto::signal::VALIDADE_DO_RELATO_MS`).
pub const JANELA_MS: f64 = 5_000.0;

/// Quantos lugares uma fonte precisa ter tocado na janela para contar.
///
/// Dois segundos de fala. O servidor mede a subida de quem fala a partir de
/// cinquenta pacotes na mesma janela; pedir o dobro aqui faz com que, quando
/// esta ponta conta a perda de uma fonte, o servidor quase sempre já tenha a
/// subida dela para descontar — e uma fração sobre menos de cem lugares
/// anda de um em um por cento, que é mais ruído que medida.
pub const MINIMO_DE_LUGARES: u64 = 100;

/// O que uma fonte contou **até agora**, desde que o buffer dela nasceu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContagemDaFonte {
    /// De quem.
    pub ssrc: u32,
    /// Lugares que deviam ter tocado um quadro: os tocados, os ocultados e os
    /// silenciados. O silêncio de quem calou não entra.
    pub lugares: u64,
    /// Destes, os que não tinham quadro: ocultados e silenciados.
    pub faltaram: u64,
    /// O jitter de chegada de agora (RFC 3550), em milissegundos.
    pub jitter_ms: f64,
}

impl ContagemDaFonte {
    /// A contagem de um buffer de jitter, com a mesma definição de perda de
    /// [`JitterMetrics::loss_fraction`].
    #[must_use]
    pub fn do_buffer(ssrc: u32, metricas: &JitterMetrics) -> Self {
        let faltaram = metricas
            .frames_concealed
            .saturating_add(metricas.frames_silenced);
        Self {
            ssrc,
            lugares: metricas.frames_played.saturating_add(faltaram),
            faltaram,
            jitter_ms: metricas.jitter_ms,
        }
    }
}

/// O que a janela mediu. `None` é «não medido», e nunca zero.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RecepcaoMedida {
    /// A pior fração perdida entre as fontes ouvidas na janela, de 0.0 a 1.0.
    pub perda: Option<f32>,
    /// O menor jitter de chegada entre as fontes ouvidas na janela, em
    /// milissegundos.
    pub jitter_ms: Option<f32>,
}

/// Quanto uma fonte andou entre duas voltas.
#[derive(Debug, Clone, Copy)]
struct Passo {
    quando_ms: f64,
    lugares: u64,
    faltaram: u64,
}

#[derive(Debug)]
struct Fonte {
    ssrc: u32,
    /// As contagens da última volta: o passo seguinte é a diferença para elas.
    lugares: u64,
    faltaram: u64,
    jitter_ms: f64,
    passos: VecDeque<Passo>,
}

/// A recepção desta ponta sobre [`JANELA_MS`].
#[derive(Debug, Default)]
pub struct JanelaDeRecepcao {
    fontes: Vec<Fonte>,
}

impl JanelaDeRecepcao {
    /// Uma janela vazia.
    #[must_use]
    pub const fn nova() -> Self {
        Self { fontes: Vec::new() }
    }

    /// Uma volta: as contagens cumulativas de cada fonte que existe agora.
    ///
    /// Devolve o que a janela mede depois dela. Quem chama decide o compasso;
    /// o do protocolo é um segundo.
    ///
    /// - Uma fonte vista pela primeira vez só marca o ponto de partida: o que
    ///   ela contou antes desta janela existir não é desta janela.
    /// - Uma fonte cujas contagens **voltaram** (o buffer foi refeito: a
    ///   pessoa foi esquecida e voltou a falar, ou a voz reabriu) recomeça do
    ///   ponto de partida, sem passo.
    /// - Uma fonte que sumiu da lista é esquecida com o que tinha.
    pub fn observar(&mut self, agora_ms: f64, contagens: &[ContagemDaFonte]) -> RecepcaoMedida {
        self.fontes
            .retain(|fonte| contagens.iter().any(|c| c.ssrc == fonte.ssrc));
        for contagem in contagens {
            match self
                .fontes
                .iter_mut()
                .find(|fonte| fonte.ssrc == contagem.ssrc)
            {
                None => self.fontes.push(Fonte {
                    ssrc: contagem.ssrc,
                    lugares: contagem.lugares,
                    faltaram: contagem.faltaram,
                    jitter_ms: contagem.jitter_ms,
                    passos: VecDeque::new(),
                }),
                Some(fonte) => {
                    let recomecou =
                        contagem.lugares < fonte.lugares || contagem.faltaram < fonte.faltaram;
                    if recomecou {
                        fonte.passos.clear();
                    } else if contagem.lugares > fonte.lugares {
                        fonte.passos.push_back(Passo {
                            quando_ms: agora_ms,
                            lugares: contagem.lugares - fonte.lugares,
                            faltaram: contagem.faltaram - fonte.faltaram,
                        });
                    }
                    fonte.lugares = contagem.lugares;
                    fonte.faltaram = contagem.faltaram;
                    fonte.jitter_ms = contagem.jitter_ms;
                }
            }
        }

        let mut medida = RecepcaoMedida::default();
        for fonte in &mut self.fontes {
            while fonte
                .passos
                .front()
                .is_some_and(|passo| agora_ms - passo.quando_ms >= JANELA_MS)
            {
                fonte.passos.pop_front();
            }
            let lugares: u64 = fonte.passos.iter().map(|passo| passo.lugares).sum();
            if lugares < MINIMO_DE_LUGARES {
                continue;
            }
            let faltaram: u64 = fonte.passos.iter().map(|passo| passo.faltaram).sum();
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "contagens de uma janela de cinco segundos, e uma fração de 0 a 1"
            )]
            let perda = (faltaram.min(lugares) as f64 / lugares as f64) as f32;
            medida.perda = Some(medida.perda.map_or(perda, |pior| pior.max(perda)));
            if fonte.jitter_ms.is_finite() && fonte.jitter_ms >= 0.0 {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "milissegundos; f32 é o que o fio carrega"
                )]
                let jitter = fonte.jitter_ms as f32;
                medida.jitter_ms = Some(medida.jitter_ms.map_or(jitter, |menor| menor.min(jitter)));
            }
        }
        medida
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma fonte que fala sem parar: a cada volta de um segundo, cinquenta
    /// lugares, dos quais `faltam` não tinham quadro.
    struct Falando {
        contagem: ContagemDaFonte,
    }

    impl Falando {
        fn nova(ssrc: u32, ja_tocou: u64, ja_faltou: u64) -> Self {
            Self {
                contagem: ContagemDaFonte {
                    ssrc,
                    lugares: ja_tocou,
                    faltaram: ja_faltou,
                    jitter_ms: 1.0,
                },
            }
        }

        fn um_segundo(&mut self, faltam: u64) -> ContagemDaFonte {
            self.contagem.lugares += 50;
            self.contagem.faltaram += faltam;
            self.contagem
        }
    }

    fn perto(medido: Option<f32>, esperado: f32) -> bool {
        medido.is_some_and(|m| (m - esperado).abs() < 1e-6)
    }

    #[test]
    fn sem_fonte_nenhuma_nada_foi_medido() {
        let mut janela = JanelaDeRecepcao::nova();
        assert_eq!(janela.observar(0.0, &[]), RecepcaoMedida::default());
    }

    /// O que a fonte contou antes de a janela existir não é desta janela: uma
    /// perda de 50% acumulada no passado não aparece.
    #[test]
    fn a_primeira_volta_so_marca_o_ponto_de_partida() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 10_000, 5_000);
        assert_eq!(
            janela.observar(0.0, &[fonte.contagem]),
            RecepcaoMedida::default()
        );
        // Dois segundos limpos depois, a medida é 0% — e não os 50% de antes.
        janela.observar(1_000.0, &[fonte.um_segundo(0)]);
        let medida = janela.observar(2_000.0, &[fonte.um_segundo(0)]);
        assert!(perto(medida.perda, 0.0), "{medida:?}");
    }

    /// Abaixo do mínimo de lugares a fração seria ruído com aparência de
    /// medida: a resposta é «não sei».
    #[test]
    fn pouca_fala_nao_e_medida() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 0, 0);
        janela.observar(0.0, &[fonte.contagem]);
        let medida = janela.observar(1_000.0, &[fonte.um_segundo(25)]);
        assert_eq!(medida, RecepcaoMedida::default(), "50 lugares não bastam");
        let medida = janela.observar(2_000.0, &[fonte.um_segundo(25)]);
        assert!(perto(medida.perda, 0.5), "{medida:?}");
    }

    /// **O teste que justifica o módulo.** Uma rajada de perda sai da conta
    /// quando sai da janela; na fração cumulativa do buffer ela ficaria para
    /// sempre.
    #[test]
    fn a_perda_que_passou_sai_da_janela() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 0, 0);
        let mut agora = 0.0;
        janela.observar(agora, &[fonte.contagem]);
        // Três segundos a 20%.
        let mut medida = RecepcaoMedida::default();
        for _ in 0..3 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[fonte.um_segundo(10)]);
        }
        assert!(perto(medida.perda, 0.2), "{medida:?}");
        // Cinco segundos limpos: a rajada saiu inteira.
        for _ in 0..5 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[fonte.um_segundo(0)]);
        }
        assert!(perto(medida.perda, 0.0), "{medida:?}");
        // A cumulativa, no mesmo instante, ainda diria 7,5%.
        let cumulativa = fonte.contagem.faltaram as f64 / fonte.contagem.lugares as f64;
        assert!(cumulativa > 0.07, "{cumulativa}");
    }

    /// Quem parou de falar deixa de ser medida quando o que tocou sai da
    /// janela — e não fica com o último número, nem vira zero.
    #[test]
    fn quem_calou_deixa_de_ser_medida() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 0, 0);
        let mut agora = 0.0;
        janela.observar(agora, &[fonte.contagem]);
        for _ in 0..4 {
            agora += 1_000.0;
            janela.observar(agora, &[fonte.um_segundo(5)]);
        }
        // Calou: as contagens param, e a fonte segue na tabela.
        let mut medida = RecepcaoMedida::default();
        for _ in 0..3 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[fonte.contagem]);
        }
        assert!(perto(medida.perda, 0.1), "ainda na janela: {medida:?}");
        for _ in 0..3 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[fonte.contagem]);
        }
        assert_eq!(medida, RecepcaoMedida::default());
    }

    /// A pior perda e o menor jitter, e só de quem tocou na janela: o jitter
    /// congelado de quem calou há tempo não decide o de agora.
    #[test]
    fn a_pior_perda_e_o_menor_jitter_entre_quem_esta_falando() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut limpa = Falando::nova(1, 0, 0);
        let mut ruim = Falando::nova(2, 0, 0);
        let calada = ContagemDaFonte {
            ssrc: 3,
            lugares: 9_000,
            faltaram: 0,
            jitter_ms: 0.1,
        };
        limpa.contagem.jitter_ms = 4.0;
        ruim.contagem.jitter_ms = 30.0;
        let mut agora = 0.0;
        janela.observar(agora, &[limpa.contagem, ruim.contagem, calada]);
        let mut medida = RecepcaoMedida::default();
        for _ in 0..3 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[limpa.um_segundo(1), ruim.um_segundo(6), calada]);
        }
        assert!(perto(medida.perda, 0.12), "{medida:?}");
        assert!(perto(medida.jitter_ms, 4.0), "{medida:?}");
    }

    /// O buffer refeito recomeça as contagens do zero: é um ponto de partida
    /// novo, e não uma perda negativa nem um salto.
    #[test]
    fn o_buffer_refeito_recomeca_do_ponto_de_partida() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 0, 0);
        let mut agora = 0.0;
        janela.observar(agora, &[fonte.contagem]);
        for _ in 0..3 {
            agora += 1_000.0;
            janela.observar(agora, &[fonte.um_segundo(25)]);
        }
        // O mesmo `ssrc`, com as contagens de volta ao começo.
        let mut refeita = Falando::nova(7, 20, 0);
        agora += 1_000.0;
        assert_eq!(
            janela.observar(agora, &[refeita.contagem]),
            RecepcaoMedida::default(),
            "os 50% do buffer velho não são do novo"
        );
        let mut medida = RecepcaoMedida::default();
        for _ in 0..2 {
            agora += 1_000.0;
            medida = janela.observar(agora, &[refeita.um_segundo(0)]);
        }
        assert!(perto(medida.perda, 0.0), "{medida:?}");
    }

    /// Quem saiu da tabela leva a medida junto.
    #[test]
    fn a_fonte_que_sumiu_e_esquecida() {
        let mut janela = JanelaDeRecepcao::nova();
        let mut fonte = Falando::nova(7, 0, 0);
        janela.observar(0.0, &[fonte.contagem]);
        janela.observar(1_000.0, &[fonte.um_segundo(10)]);
        janela.observar(2_000.0, &[fonte.um_segundo(10)]);
        assert_eq!(janela.observar(3_000.0, &[]), RecepcaoMedida::default());
        // De volta com o mesmo `ssrc`: ponto de partida novo.
        assert_eq!(
            janela.observar(4_000.0, &[fonte.um_segundo(10)]),
            RecepcaoMedida::default()
        );
    }

    /// A contagem tirada do buffer tem a definição de perda do buffer.
    #[test]
    fn a_contagem_do_buffer_bate_com_a_fracao_do_buffer() {
        let metricas = JitterMetrics {
            frames_played: 180,
            frames_concealed: 15,
            frames_silenced: 5,
            frames_comfort: 400,
            jitter_ms: 2.5,
            ..JitterMetrics::default()
        };
        let contagem = ContagemDaFonte::do_buffer(9, &metricas);
        assert_eq!(contagem.lugares, 200);
        assert_eq!(contagem.faltaram, 20);
        let fracao = contagem.faltaram as f64 / contagem.lugares as f64;
        assert!((fracao - metricas.loss_fraction()).abs() < 1e-12);
        assert!((contagem.jitter_ms - 2.5).abs() < 1e-12);
    }
}
