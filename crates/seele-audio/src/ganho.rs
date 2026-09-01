//! Um ganho automático para o microfone, escrito aqui.
//!
//! # Por que existe
//!
//! Porque não havia nenhum. A mistura soma as fontes com ganho mestre em 1,0 e
//! só um `soft_clip` no fim: o que sai é exatamente o que o microfone capturou.
//! Microfone de notebook e headset barato entregam bem abaixo do que uma
//! conversa pede, e o pedido de campo foi esse — «o volume do áudio na conversa
//! parece bem baixo; dá para pôr um ganho natural do app?».
//!
//! # E o ADR 0007
//!
//! Ele continua valendo. O que aquele ADR recusa é a **dependência** de DSP em
//! C++ — `webrtc-audio-processing`, e com ela AEC, AGC e supressão de ruído numa
//! decisão só. Isto aqui não é aquilo: são quarenta linhas de Rust, sem
//! dependência nova, e não fazem cancelamento de eco nem supressão. O seam de
//! `--features aec` continua de pé para quando a hora dele chegar.
//!
//! # Na captura, e não na reprodução
//!
//! Ganho na saída deixaria **cada ouvinte** consertando sozinho um microfone que
//! não é dele, e não ajudaria quem escuta pelo celular de alguém. Na captura,
//! quem fala baixo passa a ser ouvido por todos de uma vez.
//!
//! # Depois do portão de voz, e nunca antes
//!
//! O portão decide se há fala olhando a energia do quadro. Multiplicar antes
//! dele faria ruído de sala virar fala, e o §3 já paga caro por um portão que
//! abre à toa: cada abertura é banda gasta e é a voz de alguém sendo cortada
//! para dar lugar a um ventilador.

/// Onde o pico de um quadro deve chegar depois do ganho.
///
/// 0,5 é −6 dBFS: alto o bastante para uma conversa e com meia escala de folga
/// para o que vier depois — a mistura de quem ouve soma várias pessoas antes do
/// `soft_clip`, e chegar já colado no teto faria duas pessoas falando ao mesmo
/// tempo distorcerem.
const ALVO: f32 = 0.5;

/// Quanto este ganho pode amplificar, no máximo.
///
/// 8× são +18 dB. Acima disso o que sobe é o ruído de fundo junto com a voz, e o
/// resultado é uma sala que chia — pior que uma voz baixa, porque a voz baixa
/// quem escuta compensa no volume do sistema e o chiado não sai mais.
const TETO: f32 = 8.0;

/// Abaixo deste pico, o quadro é silêncio e o ganho não se mexe.
///
/// Sem este piso, um quadro de sala vazia pediria os 8× inteiros, e a primeira
/// sílaba depois do silêncio sairia estourada — o efeito que se ouve como
/// «bombeamento» e que denuncia um ganho automático mal feito.
const PISO: f32 = 0.01;

/// Quanto do caminho até o ganho desejado se anda por quadro, ao **subir**.
///
/// Um quadro são 20 ms, então 0,02 leva o ganho a cerca de 63% do caminho em um
/// segundo e a quase todo ele em dois. Subir devagar é o que faz a mudança não
/// ser ouvida; descer é outra história, e é imediata.
const SUBIDA: f32 = 0.02;

/// O ganho automático de um caminho de captura.
#[derive(Debug)]
pub struct Ganho {
    atual: f32,
}

impl Default for Ganho {
    fn default() -> Self {
        Self::novo()
    }
}

impl Ganho {
    /// Um ganho que começa neutro.
    #[must_use]
    pub const fn novo() -> Self {
        Self { atual: 1.0 }
    }

    /// Quanto está sendo aplicado agora. Para telemetria e para os testes.
    #[must_use]
    pub const fn atual(&self) -> f32 {
        self.atual
    }

    /// Aplica o ganho ao quadro, no lugar.
    ///
    /// **Nunca aumenta o pico além de [`ALVO`], e nunca atenua.** As duas
    /// metades são a garantia inteira, e ela é aritmética e não estatística: o
    /// pico que sai é no máximo o maior entre o pico que entrou e o alvo.
    ///
    /// Nunca atenuar é decisão: quem já fala alto tem o volume que escolheu, no
    /// sistema dele, e abaixá-lo aqui seria este código discordando de uma
    /// pessoa sobre o volume da própria voz.
    ///
    /// Não estourar depende de a **descida valer na hora**. Depois de dois
    /// segundos de sussurro o ganho está perto de 8×; se a queda fosse gradual
    /// como a subida, a primeira sílaba alta sairia multiplicada por oito.
    pub fn aplicar(&mut self, quadro: &mut [f32]) {
        let pico = quadro.iter().fold(0.0_f32, |maior, a| maior.max(a.abs()));
        if pico < PISO {
            // Silêncio não move o ganho: ele fica onde a última fala o deixou,
            // e a próxima começa no volume certo em vez de escalar do zero.
            for amostra in quadro.iter_mut() {
                *amostra *= self.atual;
            }
            return;
        }

        let desejado = (ALVO / pico).clamp(1.0, TETO);
        self.atual = if desejado < self.atual {
            // Descida imediata: é o que impede o estouro.
            desejado
        } else {
            self.atual + (desejado - self.atual) * SUBIDA
        };

        for amostra in quadro.iter_mut() {
            *amostra *= self.atual;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um quadro com um seno de amplitude escolhida.
    fn seno(amplitude: f32, quantas: usize) -> Vec<f32> {
        (0..quantas)
            .map(|i| {
                let fase = i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU;
                fase.sin() * amplitude
            })
            .collect()
    }

    fn pico(quadro: &[f32]) -> f32 {
        quadro.iter().fold(0.0_f32, |maior, a| maior.max(a.abs()))
    }

    #[test]
    fn uma_voz_baixa_sobe_ate_perto_do_alvo() {
        // O caso que gerou isto: microfone entregando um vigésimo da escala.
        let mut ganho = Ganho::novo();
        let mut ultimo = 0.0;
        // Dois segundos de fala, a 50 quadros por segundo.
        for _ in 0..100 {
            let mut quadro = seno(0.05, 960);
            ganho.aplicar(&mut quadro);
            ultimo = pico(&quadro);
        }
        // O alcançável, e não o alvo: com [`TETO`] em 8×, um sinal de 0,05
        // chega no máximo a 0,4. Exigir o alvo aqui seria exigir o impossível —
        // foi o que a primeira versão deste teste fez.
        let alcancavel = (0.05 * TETO).min(ALVO);
        assert!(
            ultimo > alcancavel * 0.85,
            "depois de dois segundos o pico é {ultimo}, longe dos {alcancavel} que o teto permite"
        );
    }

    #[test]
    fn a_subida_e_lenta_o_bastante_para_ninguem_ouvir() {
        // Um ganho que salta é pior que um volume baixo: quem escuta ouve a
        // sala respirando. Um quadro só não pode fazer quase nada.
        let mut ganho = Ganho::novo();
        let mut quadro = seno(0.05, 960);
        ganho.aplicar(&mut quadro);
        assert!(
            ganho.atual() < 1.2,
            "um quadro só levou o ganho a {}, que é um salto audível",
            ganho.atual()
        );
    }

    #[test]
    fn uma_voz_alta_nunca_e_amplificada() {
        // Quem já chega no alvo não recebe nada: amplificar aqui seria empurrar
        // para o `soft_clip` da mistura, que é distorção.
        let mut ganho = Ganho::novo();
        let mut quadro = seno(0.9, 960);
        ganho.aplicar(&mut quadro);
        assert!((pico(&quadro) - 0.9).abs() < 1e-3, "a voz alta foi mexida");
        assert!((ganho.atual() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn um_grito_depois_do_sussurro_nao_estoura() {
        // **A prova que justifica a descida imediata.** Dois segundos de sussurro
        // levam o ganho perto de 8×; se a queda fosse gradual como a subida, a
        // primeira sílaba alta sairia oito vezes acima da escala.
        let mut ganho = Ganho::novo();
        for _ in 0..100 {
            let mut quadro = seno(0.05, 960);
            ganho.aplicar(&mut quadro);
        }
        assert!(ganho.atual() > 4.0, "o sussurro não levantou o ganho");

        let mut grito = seno(0.9, 960);
        ganho.aplicar(&mut grito);
        // O grito sai como entrou: o ganho volta a 1,0 no mesmo quadro. O que
        // este teste prende é que ele **não** sai multiplicado pelo ganho do
        // sussurro, que seria 8 × 0,9 e estouraria a escala inteira.
        assert!(
            (pico(&grito) - 0.9).abs() < 1e-3,
            "o grito saiu com pico {} em vez de 0,9 — a descida não foi imediata",
            pico(&grito)
        );
        assert!(
            (ganho.atual() - 1.0).abs() < 1e-6,
            "o ganho não voltou a 1,0"
        );
    }

    #[test]
    fn o_silencio_nao_e_amplificado() {
        // Sem o piso, a sala vazia pediria os 8× e a primeira sílaba estouraria.
        let mut ganho = Ganho::novo();
        for _ in 0..100 {
            let mut quadro = seno(0.001, 960);
            ganho.aplicar(&mut quadro);
        }
        assert!(
            (ganho.atual() - 1.0).abs() < 1e-6,
            "o silêncio moveu o ganho para {}",
            ganho.atual()
        );
    }
}
