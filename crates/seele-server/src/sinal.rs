//! O sinal que esta sessão anuncia: com que medidas ele é feito.
//!
//! # O defeito que isto conserta
//!
//! Da 0.17 em diante o `signal` do `PersonState` ficou parado em 100 para quase
//! todo mundo. A sessão só alimentava a conta quando tinha ida e volta **e** a
//! perda de saída do RTCP, juntas:
//!
//! ```text
//! match (rtt_ms, loss_fraction) {
//!     (Some(rtt_ms), Some(loss_fraction)) => sync.update(..),
//!     _ => sync.value(),          // «sem medida» — que vale 100
//! }
//! ```
//!
//! e a perda de saída só existe para quem está **recebendo** mídia RTP deste
//! servidor. Quem só fala, quem só ouve, quem transmite a própria tela e todo
//! navegador ficavam no segundo braço para sempre. A ida e volta do ICE estava
//! medida o tempo todo e era jogada fora por falta de companhia.
//!
//! # A regra
//!
//! **Cada parcela entra com o que foi medido; a que ninguém mediu não
//! penaliza; sem medida nenhuma, a conta não é alimentada.**
//!
//! - Ida e volta: a do ICE, que existe para todo par depois do primeiro check
//!   respondido — com mídia RTP ou sem, navegador ou não.
//! - Perda: a **maior** das que este servidor mede sobre o caminho desta
//!   pessoa, cada uma com o seu alcance:
//!   - a de saída do RTP (o que mandamos a ela e não chegou) — só enquanto ela
//!     recebe vídeo;
//!   - a de entrada do RTP (o que ela mandou e não chegou) — só enquanto ela
//!     transmite vídeo;
//!   - a de subida da voz ([`crate::perda_de_subida`]) — só enquanto ela fala.
//! - Jitter: medido no receptor; o servidor não tem, e não inventa.
//!
//! # O que este número **não** sabe, dito aqui para ninguém supor o contrário
//!
//! Quem está calado e sem vídeo nenhum não tem perda medida por este servidor:
//! não há tráfego de mídia dele para contar, e o SCTP do str0m não expõe
//! retransmissão. O sinal dessa pessoa reflete o atraso do caminho e nada diz
//! da perda — a perda do que ela **ouve** é medida no cliente dela, e não
//! atravessa o fio. Fechar isso pede uma mensagem nova de protocolo (o cliente
//! relatando a própria recepção), que não foi feita aqui.

use std::time::{Duration, Instant};

use seele_enlace::Leitura;
use seele_proto::signal::{Signal, SyncInputs};

/// Por quanto tempo o último relato de perda da voz ainda vale como medida.
///
/// A sala relata a cada segundo **enquanto chegam pacotes**; com DTX, calar-se
/// é parar de mandar, e o relato para junto. O relato descreve uma janela de
/// [`crate::perda_de_subida::JANELA`], então vale por esse tempo: mais curto e
/// o sinal piscaria a cada pausa entre duas frases; mais longo e ele afirmaria
/// uma perda que ninguém mais está medindo.
pub const VALIDADE_DA_PERDA_DA_VOZ: Duration = crate::perda_de_subida::JANELA;

/// O que se mediu do caminho de uma pessoa neste instante. `None` é «não
/// medido», e nunca zero.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Medidas {
    /// A ida e volta do enlace (ICE, ou RTCP quando há mídia).
    pub ida_e_volta: Option<Duration>,
    /// Do RTP que mandamos a ela, a fração que se perdeu.
    pub perda_na_saida: Option<f32>,
    /// Do RTP que ela mandou, a fração que se perdeu.
    pub perda_na_entrada: Option<f32>,
    /// Da voz que ela mandou, a fração que se perdeu.
    pub perda_da_voz: Option<f32>,
}

impl Medidas {
    /// A maior perda medida, ou `None` se nenhuma fonte mediu.
    ///
    /// A maior e não a média: as três olham o mesmo caminho por tráfegos
    /// diferentes, e a que viu mais perda é a que a pessoa está sentindo.
    #[must_use]
    pub fn perda(&self) -> Option<f32> {
        [
            self.perda_na_saida,
            self.perda_na_entrada,
            self.perda_da_voz,
        ]
        .into_iter()
        .flatten()
        .filter(|fracao| fracao.is_finite())
        .map(|fracao| fracao.clamp(0.0, 1.0))
        .reduce(f32::max)
    }

    /// As entradas da conta, ou `None` se nada foi medido.
    #[must_use]
    pub fn entradas(&self) -> Option<SyncInputs> {
        let rtt_ms = self.ida_e_volta.map(|volta| volta.as_secs_f32() * 1000.0);
        let perda = self.perda();
        if rtt_ms.is_none() && perda.is_none() {
            return None;
        }
        Some(SyncInputs {
            // A parcela sem medida não penaliza. Não é afirmar zero: é não
            // descontar pontos por algo que ninguém viu.
            rtt_ms: rtt_ms.unwrap_or(0.0),
            // Medido no receptor; o servidor não tem.
            jitter_ms: 0.0,
            loss_fraction: perda.unwrap_or(0.0),
        })
    }
}

/// O sinal de uma sessão, e o que ele lembra entre um tique e outro.
#[derive(Debug)]
pub struct SinalDaSessao {
    sync: Signal,
    perda_da_voz: Option<(f32, Instant)>,
}

impl Default for SinalDaSessao {
    fn default() -> Self {
        Self::novo()
    }
}

impl SinalDaSessao {
    /// Um sinal sem medida nenhuma.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            sync: Signal::new(),
            perda_da_voz: None,
        }
    }

    /// A sala relatou a perda de subida da voz desta pessoa.
    pub fn a_voz_perdeu(&mut self, fracao: f32, agora: Instant) {
        self.perda_da_voz = Some((fracao, agora));
    }

    /// O que há de medido agora, juntando a leitura do enlace ao último relato
    /// da voz que ainda vale.
    #[must_use]
    pub fn medidas(&self, leitura: &Leitura, agora: Instant) -> Medidas {
        Medidas {
            ida_e_volta: leitura.ida_e_volta,
            perda_na_saida: leitura.fracao_de_perda_na_saida,
            perda_na_entrada: leitura.fracao_de_perda_na_entrada,
            perda_da_voz: self.perda_da_voz.and_then(|(fracao, quando)| {
                (agora.saturating_duration_since(quando) <= VALIDADE_DA_PERDA_DA_VOZ)
                    .then_some(fracao)
            }),
        }
    }

    /// Um tique: dobra o que foi medido na média e devolve o sinal a anunciar.
    ///
    /// Sem medida nenhuma a média não anda, e o que sai é o valor anterior (ou
    /// o «ainda sem medida» do [`Signal`], se nunca houve uma).
    pub fn tique(&mut self, leitura: &Leitura, agora: Instant) -> u8 {
        match self.medidas(leitura, agora).entradas() {
            Some(entradas) => self.sync.update(entradas),
            None => self.sync.value(),
        }
    }

    /// Se alguma medida já entrou na conta.
    #[must_use]
    pub fn medido(&self) -> bool {
        self.sync.has_measurement()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::signal::{SignalBand, NOMINAL_FLOOR};

    fn leitura(rtt_ms: Option<u64>, saida: Option<f32>, entrada: Option<f32>) -> Leitura {
        Leitura {
            ida_e_volta: rtt_ms.map(Duration::from_millis),
            fracao_de_perda_na_saida: saida,
            fracao_de_perda_na_entrada: entrada,
            ..Leitura::default()
        }
    }

    /// Assenta a média: o mesmo tique repetido até ela chegar ao regime.
    fn regime(sinal: &mut SinalDaSessao, leitura: &Leitura, agora: Instant) -> u8 {
        let mut valor = 0;
        for _ in 0..40 {
            valor = sinal.tique(leitura, agora);
        }
        valor
    }

    /// O defeito, na forma em que ele existia: ida e volta medida, perda de
    /// saída sem medida (ninguém recebendo RTP), e o sinal em 100.
    #[test]
    fn a_ida_e_volta_sozinha_ja_move_o_sinal() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        let valor = sinal.tique(&leitura(Some(300), None, None), agora);
        assert!(sinal.medido());
        assert!(
            valor < NOMINAL_FLOOR,
            "300 ms de ida e volta e o sinal em {valor}"
        );
    }

    #[test]
    fn a_perda_sozinha_ja_move_o_sinal() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        let valor = sinal.tique(&leitura(None, None, Some(0.10)), agora);
        assert_eq!(valor, 70, "10% de perda são os 30 pontos da parcela");
    }

    #[test]
    fn sem_medida_nenhuma_a_conta_nao_e_alimentada() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        sinal.tique(&leitura(None, None, None), agora);
        assert!(
            !sinal.medido(),
            "uma leitura vazia foi contada como uma rede perfeita"
        );
        assert_eq!(Medidas::default().entradas(), None);
    }

    #[test]
    fn um_caminho_bom_fica_nominal() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        let valor = regime(&mut sinal, &leitura(Some(12), Some(0.0), Some(0.0)), agora);
        assert_eq!(valor, 100);
    }

    /// Quem transmite não recebe RTP: a de saída é `None`, e a de entrada é a
    /// única que vê a perda do vídeo dele. E vice-versa para quem assiste.
    #[test]
    fn cada_sentido_do_rtp_basta_sozinho_e_vale_o_pior() {
        let agora = Instant::now();
        for (saida, entrada) in [(None, Some(0.10)), (Some(0.10), None)] {
            let mut sinal = SinalDaSessao::novo();
            let valor = regime(&mut sinal, &leitura(Some(10), saida, entrada), agora);
            assert_eq!(valor, 70, "saída {saida:?}, entrada {entrada:?}");
        }
        let medidas = Medidas {
            perda_na_saida: Some(0.01),
            perda_na_entrada: Some(0.04),
            perda_da_voz: Some(0.02),
            ..Medidas::default()
        };
        assert_eq!(medidas.perda(), Some(0.04));
    }

    #[test]
    fn a_perda_da_voz_entra_e_envelhece() {
        let inicio = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        let limpa = leitura(Some(10), None, None);
        sinal.a_voz_perdeu(0.10, inicio);
        assert_eq!(regime(&mut sinal, &limpa, inicio), 70);
        assert_eq!(SignalBand::of(70), SignalBand::Degraded);

        // No limite da validade ainda vale; passado dele, a perda deixa de ser
        // uma medida e o sinal volta ao que o resto diz.
        let no_limite = inicio + VALIDADE_DA_PERDA_DA_VOZ;
        assert_eq!(sinal.medidas(&limpa, no_limite).perda_da_voz, Some(0.10));
        let depois = no_limite + Duration::from_millis(1);
        assert_eq!(sinal.medidas(&limpa, depois).perda_da_voz, None);
        assert_eq!(regime(&mut sinal, &limpa, depois), 100);
    }

    #[test]
    fn uma_fracao_que_nao_e_numero_nao_entra() {
        let medidas = Medidas {
            perda_na_saida: Some(f32::NAN),
            perda_na_entrada: Some(7.0),
            ..Medidas::default()
        };
        assert_eq!(medidas.perda(), Some(1.0));
        let so_lixo = Medidas {
            perda_na_saida: Some(f32::INFINITY),
            ..Medidas::default()
        };
        assert_eq!(so_lixo.perda(), None);
    }
}
