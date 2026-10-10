//! O relógio de mídia da voz que sai: a sequência e o carimbo de cada pacote.
//!
//! **A sequência conta o que sai, e o carimbo conta as amostras que passaram,
//! saia algo ou não.** É essa diferença que deixa quem recebe separar o
//! silêncio do DTX da perda de verdade (M1.9): um vão no carimbo com a
//! sequência seguida é silêncio; um vão na sequência é perda.
//!
//! Morava dentro do laço da voz do desktop (`seele-core/src/voice.rs`), e saiu
//! para cá para o navegador usar a mesma conta (Tarefa 3.3 da F4; o ADR 0057
//! proíbe uma segunda implementação). O teste gravado da sequência fixa
//! (`seele-core`, `voice::relogio_gravado`) é o mesmo de antes de sair.

use crate::{FRAME_MS, FRAME_SAMPLES};

/// O relógio de quem manda: a sequência do último pacote que saiu e o carimbo
/// do quadro de agora, em amostras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelogioDeMidia {
    seq: u16,
    carimbo: u32,
}

impl RelogioDeMidia {
    /// Um relógio que continua de `seq` e `carimbo`: de onde o caminho anterior
    /// parou, e não de zero.
    #[must_use]
    pub const fn de(seq: u16, carimbo: u32) -> Self {
        Self { seq, carimbo }
    }

    /// A sequência do último pacote que saiu.
    #[must_use]
    pub const fn seq(self) -> u16 {
        self.seq
    }

    /// O carimbo do quadro de agora, em amostras.
    #[must_use]
    pub const fn carimbo(self) -> u32 {
        self.carimbo
    }

    /// Um quadro de amostras passou, saia algo dele ou não. Devolve o carimbo
    /// de agora.
    pub fn quadro_passou(&mut self) -> u32 {
        self.carimbo = self
            .carimbo
            .wrapping_add(u32::try_from(FRAME_SAMPLES).unwrap_or(FRAME_MS));
        self.carimbo
    }

    /// Um pacote vai sair: a sequência anda um, e esta é a dele.
    pub fn pacote_saiu(&mut self) -> u16 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    /// Onde o relógio do caminho que substitui este recomeça, depois de trocar
    /// de aparelho ou de reconectar.
    ///
    /// # Dois quadros, e a sequência junto
    ///
    /// Os dois caminhos existem ao mesmo tempo por um instante: o novo nasce
    /// **antes** de o velho ser largado, de propósito — um microfone que sumiu
    /// deixa a pessoa falando pelo antigo em vez de muda. Depois de o relógio ser
    /// lido, o velho ainda pode mandar um quadro antes de parar. Começar dois
    /// quadros à frente passa por cima dele sem repetir número.
    ///
    /// **Até a 0.15.0 o pulo era de um segundo, e foi o defeito.** A ideia era
    /// que abrir um aparelho leva esse tempo e o buffer de jitter de quem ouve
    /// leria o pulo como pausa e reacertaria. Não reacertava: com a sequência
    /// andando um e o carimbo cinquenta quadros, quem ouve agendava ~900 ms de
    /// silêncio enquanto a voz nova já chegava, e o atraso ficava — cada troca
    /// de microfone somava outro (relato de 05/10/2026, medido 50 → 950 → 1850
    /// ms). O relógio agora é lido **depois** de o aparelho novo abrir, então o
    /// tempo de abrir já está nele, e o pulo só cobre o quadro em voo.
    ///
    /// A sequência anda o mesmo que o carimbo, em quadros: quem recebe usa a
    /// diferença entre os dois para separar silêncio de perda (M1.9), e o vão
    /// de dois quadros chega como um quadro perdido — que o Opus esconde — e
    /// não como pausa de quem fala.
    #[must_use]
    pub fn salto(self) -> Self {
        let quadro = u32::try_from(FRAME_SAMPLES).unwrap_or(960);
        Self {
            seq: self.seq.wrapping_add(2),
            carimbo: self.carimbo.wrapping_add(2 * quadro),
        }
    }
}

/// O carimbo de tempo de um quadro numa entrega de vários. F01.
///
/// `carimbo` é o do quadro de **agora**, `quantos` é o tamanho da entrega e
/// `indice` a posição dentro dela. O último quadro fica com `carimbo`; cada um
/// antes dele recua um quadro de amostras.
///
/// # Por que não bastava carimbar todos igual
///
/// Porque um carimbo é *quando aquele áudio aconteceu*, e os quadros retidos da
/// primeira sílaba aconteceram **antes**. Carimbados todos com o mesmo instante,
/// quem recebe vê três quadros no mesmo tempo: o `playout` trata carimbo repetido
/// como um salto do relógio e ressincroniza, e uma ressincronização no primeiro
/// quadro de cada fala é exatamente onde ela mais se ouve — é o começo da palavra
/// que F01 foi recuperar.
///
/// Auditoria de 22/09/2026, A05. Recuar é a conta certa e não uma aproximação: o
/// quadro retido *é* de 20 ms antes.
///
/// `wrapping_sub` porque o carimbo é um contador de 32 bits que dá a volta, e uma
/// volta no meio de uma abertura não é um erro — é a mesma aritmética que o resto
/// do relógio usa para andar para a frente.
pub fn carimbo_do_quadro(carimbo: u32, quantos: usize, indice: usize) -> u32 {
    let atras = quantos.saturating_sub(indice + 1);
    let amostras = u32::try_from(atras.saturating_mul(FRAME_SAMPLES)).unwrap_or(0);
    carimbo.wrapping_sub(amostras)
}
