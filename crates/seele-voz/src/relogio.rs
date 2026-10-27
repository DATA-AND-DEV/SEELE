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

/// **De onde um caminho de voz novo começa a contar**: a sequência em zero e o
/// carimbo no relógio da máquina, `desde_a_epoca` em quadros de 20 ms, módulo
/// 2³². Um caminho que **substitui** outro na mesma conexão continua dele
/// ([`RelogioDeMidia::salto`]); este é o de uma voz que nasce sem anterior.
///
/// # Por que não do zero (o CUA de 25/10, 03:39)
///
/// Quem ouve guarda, por `ssrc`, até que carimbo já tocou, e descarta como
/// atrasado tudo o que vier antes dele. O `ssrc` sobrevive à conexão: quem sai
/// do servidor sentado e volta em cinco minutos recebe o mesmo, da reserva da
/// graça (`seele_server::server::Slots::reclaim`, e o resgate do assento em
/// `seele-sessao` exige o mesmo `ssrc`). A voz da conexão nova contava do zero,
/// e quem ficou a descartava por tanto tempo quanto a voz velha tinha vivido:
/// medido, 76 s de voz e ~77 s de mudo, com a tela dizendo NO AR.
///
/// Do relógio da máquina, o carimbo novo fica à frente do velho pelo tempo que
/// a pessoa passou fora, e quem ouve lê o vão como a pausa que já tocou. Não é
/// uma heurística de quem recebe: o buffer não tem como separar «a fonte
/// recomeçou» de «chegou a cauda velha atrasada», e quem manda tem.
///
/// # Os limites, declarados
///
/// - **A volta dos 32 bits** (24,8 h) não importa: quem ouve compara pela
///   diferença circular (`jitter::ts_delta`), e a diferença é o tempo fora.
/// - **O cristal do aparelho**: o carimbo da voz velha andou no ritmo da placa
///   de som, e não do relógio da máquina. Uma placa adiantada `p` ppm por uma
///   voz de `t` segundos deixa o carimbo velho `t·p` à frente; o novo só fica
///   atrás se a pessoa voltou em menos que isso (100 ppm por 3 h: 1 s), e então
///   fica mudo por essa diferença, e não pela vida da voz.
/// - **O relógio de parede que recua** (NTP): quem chama ancora o relógio de
///   parede uma vez por processo e soma o tempo monotônico que passou
///   (`seele-core`, `partida_desta_maquina`; na página, `performance`). Dentro
///   do processo a partida nunca recua. **Entre processos** (o app reaberto) e
///   entre máquinas com a mesma identidade, um recuo ainda deixa a voz nova
///   atrás pelo tamanho dele, e muda por esse tempo.
/// - **A sequência** fica em zero: quem recebe conta a perda pela sequência, e
///   uma sequência que pulasse junto com o relógio leria o tempo fora como
///   perda.
/// - **Quem manda de uma versão que conta do zero** (a 0.16) continua com o
///   defeito do lado dela; quem ouve não muda. O navegador da 0.17 parte pela
///   hora da página (`Envio::partindo_de`, no `seele-web`).
#[must_use]
pub fn partida(desde_a_epoca: std::time::Duration) -> (u16, u32) {
    let quadros = desde_a_epoca.as_millis() / u128::from(FRAME_MS);
    let amostras = quadros.wrapping_mul(FRAME_SAMPLES as u128);
    // Módulo 2³²: o carimbo é um contador que dá a volta.
    (0, (amostras & u128::from(u32::MAX)) as u32)
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

#[cfg(test)]
mod a_partida {
    use super::partida;
    use crate::jitter::ts_delta;
    use std::time::Duration;

    /// Duas vozes da mesma máquina, a segunda 22 s depois de a primeira ter
    /// vivido 76 s: a segunda parte à frente do fim da primeira pelo tempo
    /// fora, e não atrás dela pela vida inteira (o CUA de 25/10).
    #[test]
    fn a_voz_nova_parte_a_frente_da_velha_pelo_tempo_fora() {
        let agora = Duration::from_secs(1_792_900_000);
        let (seq, velha) = partida(agora);
        assert_eq!(seq, 0, "a sequência parte do zero: o vão não é perda");
        let fim_da_velha = velha.wrapping_add(76 * 48_000);
        let (_, nova) = partida(agora + Duration::from_secs(76 + 22));
        assert_eq!(ts_delta(nova, fim_da_velha), 22 * 48_000);
    }

    /// Através da volta dos 32 bits do carimbo (a cada 24,8 h de relógio), a
    /// diferença circular continua sendo o tempo que passou.
    #[test]
    fn a_volta_dos_32_bits_nao_muda_a_conta() {
        let quadro_ms = 20_u64;
        let volta_ms = (1_u64 << 32) / 960 * quadro_ms;
        let antes = Duration::from_millis(volta_ms - 1_000);
        let depois = Duration::from_millis(volta_ms + 1_000);
        let (_, a) = partida(antes);
        let (_, b) = partida(depois);
        assert!(b < a, "o carimbo não deu a volta entre os dois");
        assert_eq!(ts_delta(b, a), 2 * 48_000);
    }

    /// O carimbo anda em quadros inteiros de 20 ms do relógio.
    #[test]
    fn a_partida_anda_em_quadros() {
        let (_, a) = partida(Duration::from_millis(1_000_000));
        let (_, b) = partida(Duration::from_millis(1_000_019));
        let (_, c) = partida(Duration::from_millis(1_000_020));
        assert_eq!(a, b);
        assert_eq!(c.wrapping_sub(a), 960);
    }
}
