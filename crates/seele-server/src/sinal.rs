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
//!   - a de subida da voz ([`crate::perda_de_subida`]) — só enquanto ela fala;
//!   - a que **ela mesma relata** da voz que ouve
//!     ([`seele_proto::signal::Recepcao`], protocolo 13) — enquanto alguém
//!     fala para ela.
//! - Jitter: medido no receptor; o servidor não tem, e não inventa. O que
//!   entra é o do relato dela, na parcela que o [`seele_proto::signal`] já
//!   previa para ele.
//!
//! # O relato de recepção, e o buraco que ele fecha
//!
//! Quem está calado e sem vídeo nenhum não tem perda medida por este servidor:
//! não há tráfego de mídia dele para contar, e o SCTP do str0m não expõe
//! retransmissão. Até o protocolo 13 o sinal dessa pessoa refletia o atraso do
//! caminho e nada dizia da perda — a perda do que ela **ouve** é medida no
//! cliente dela, e não atravessava o fio.
//!
//! Agora atravessa: o cliente relata a cada segundo a perda e o jitter da voz
//! que chega a ele (`ClientMessage::RelatoDeRecepcao`), e
//! [`SinalDaSessao::relatou`] guarda o último. Três regras:
//!
//! - **A forma é conferida aqui** ([`seele_proto::signal::Recepcao::confere`]):
//!   um número que não é medida (`NaN`, perda fora de 0..=1, jitter negativo
//!   ou acima do teto) não entra, e o relato anterior continua valendo até
//!   envelhecer. A sessão não cai por isso.
//! - **O relato envelhece** ([`VALIDADE_DO_RELATO`]): o cliente só relata
//!   enquanto ouve voz, e um relato que parou de chegar deixa de ser medida.
//! - **É do próprio cliente sobre si.** O relato só entra no sinal de quem o
//!   mandou: um cliente que minta piora ou melhora o próprio indicador, e o de
//!   mais ninguém. Não é fronteira de segurança, é a honestidade do indicador.
//!
//! A frequência não é contada aqui: todo quadro de controle passa pelo
//! [`crate::taxa::Vigia`] antes de chegar a um tratador, e guardar um relato é
//! uma atribuição — mil por segundo custam o que o limitador deixa passar, e
//! só o último é lido, uma vez por anúncio.
//!
//! # O que este número **não** sabe, dito aqui para ninguém supor o contrário
//!
//! - Um cliente do protocolo 12 não relata: o sinal de quem está calado e sem
//!   vídeo, nele, continua refletindo só o atraso.
//! - Numa sala em que ninguém fala não há voz chegando a ninguém, e portanto
//!   não há perda de recepção a medir: o relato cessa, e a parcela sai.

use std::time::{Duration, Instant};

use seele_enlace::Leitura;
use seele_proto::signal::{Recepcao, RecepcaoRecusada, Signal, SyncInputs};

/// Por quanto tempo o último relato de perda da voz ainda vale como medida.
///
/// A sala relata a cada segundo **enquanto chegam pacotes**; com DTX, calar-se
/// é parar de mandar, e o relato para junto. O relato descreve uma janela de
/// [`crate::perda_de_subida::JANELA`], então vale por esse tempo: mais curto e
/// o sinal piscaria a cada pausa entre duas frases; mais longo e ele afirmaria
/// uma perda que ninguém mais está medindo.
pub const VALIDADE_DA_PERDA_DA_VOZ: Duration = crate::perda_de_subida::JANELA;

/// Por quanto tempo o último relato de recepção ainda vale como medida.
///
/// O número é do protocolo ([`seele_proto::signal::VALIDADE_DO_RELATO_MS`]),
/// porque os dois lados contam com ele: o cliente relata a cada segundo uma
/// janela deste tamanho, e para de relatar quando não há voz chegando.
pub const VALIDADE_DO_RELATO: Duration =
    Duration::from_millis(seele_proto::signal::VALIDADE_DO_RELATO_MS);

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
    /// Da voz que ela **ouve**, a fração que faltou. Relatada por ela.
    pub perda_relatada: Option<f32>,
    /// O jitter de chegada da voz que ela ouve, em milissegundos. Relatado
    /// por ela: é a única fonte de jitter que este servidor tem.
    pub jitter_relatado_ms: Option<f32>,
}

impl Medidas {
    /// A maior perda medida, ou `None` se nenhuma fonte mediu.
    ///
    /// A maior e não a média: todas olham o mesmo caminho por tráfegos
    /// diferentes, e a que viu mais perda é a que a pessoa está sentindo.
    #[must_use]
    pub fn perda(&self) -> Option<f32> {
        [
            self.perda_na_saida,
            self.perda_na_entrada,
            self.perda_da_voz,
            self.perda_relatada,
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
        let jitter_ms = self
            .jitter_relatado_ms
            .filter(|jitter| jitter.is_finite() && *jitter >= 0.0);
        if rtt_ms.is_none() && perda.is_none() && jitter_ms.is_none() {
            return None;
        }
        Some(SyncInputs {
            // A parcela sem medida não penaliza. Não é afirmar zero: é não
            // descontar pontos por algo que ninguém viu.
            rtt_ms: rtt_ms.unwrap_or(0.0),
            // Medido no receptor: só existe se o cliente relatou.
            jitter_ms: jitter_ms.unwrap_or(0.0),
            loss_fraction: perda.unwrap_or(0.0),
        })
    }
}

/// O sinal de uma sessão, e o que ele lembra entre um tique e outro.
#[derive(Debug)]
pub struct SinalDaSessao {
    sync: Signal,
    perda_da_voz: Option<(f32, Instant)>,
    /// O último relato de recepção que passou na conferência, e quando.
    relato: Option<(Recepcao, Instant)>,
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
            relato: None,
        }
    }

    /// A sala relatou a perda de subida da voz desta pessoa.
    pub fn a_voz_perdeu(&mut self, fracao: f32, agora: Instant) {
        self.perda_da_voz = Some((fracao, agora));
    }

    /// O cliente desta sessão relatou a própria recepção.
    ///
    /// Só o que passa em [`Recepcao::confere`] é guardado. Um relato recusado
    /// não apaga o anterior: ele não é uma medida nova, e o anterior segue
    /// valendo até envelhecer ([`VALIDADE_DO_RELATO`]).
    ///
    /// # Errors
    ///
    /// [`RecepcaoRecusada`], para quem chama dizer no registro o que veio.
    pub fn relatou(&mut self, relato: Recepcao, agora: Instant) -> Result<(), RecepcaoRecusada> {
        relato.confere()?;
        self.relato = Some((relato, agora));
        Ok(())
    }

    /// O relato de recepção que ainda vale em `agora`.
    fn relato_valido(&self, agora: Instant) -> Option<Recepcao> {
        self.relato.and_then(|(relato, quando)| {
            (agora.saturating_duration_since(quando) <= VALIDADE_DO_RELATO).then_some(relato)
        })
    }

    /// O que há de medido agora, juntando a leitura do enlace ao último relato
    /// da voz e ao último relato de recepção que ainda valem.
    #[must_use]
    pub fn medidas(&self, leitura: &Leitura, agora: Instant) -> Medidas {
        let relato = self.relato_valido(agora);
        Medidas {
            perda_relatada: relato.and_then(|relato| relato.perda),
            jitter_relatado_ms: relato.and_then(|relato| relato.jitter_ms),
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

    fn relato(perda: Option<f32>, jitter_ms: Option<f32>) -> Recepcao {
        Recepcao { perda, jitter_ms }
    }

    /// **O buraco, fechado**: calado e sem vídeo, o enlace só tem a ida e
    /// volta. Sem relato o sinal fica alto; com 10% de perda relatada ele
    /// perde os 30 pontos da parcela.
    #[test]
    fn a_perda_relatada_move_o_sinal_de_quem_esta_calado() {
        let agora = Instant::now();
        let calado = leitura(Some(10), None, None);

        let mut sem_relato = SinalDaSessao::novo();
        assert_eq!(regime(&mut sem_relato, &calado, agora), 100);

        let mut com_relato = SinalDaSessao::novo();
        assert_eq!(com_relato.relatou(relato(Some(0.10), None), agora), Ok(()));
        assert_eq!(regime(&mut com_relato, &calado, agora), 70);
    }

    /// A relatada é mais uma entre as perdas, e vale a maior.
    #[test]
    fn a_perda_relatada_entra_no_maior_entre_as_perdas() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        sinal.a_voz_perdeu(0.04, agora);
        assert_eq!(sinal.relatou(relato(Some(0.02), None), agora), Ok(()));
        let medidas = sinal.medidas(&leitura(Some(10), Some(0.01), None), agora);
        assert_eq!(medidas.perda_relatada, Some(0.02));
        assert_eq!(medidas.perda(), Some(0.04), "a maior é a da voz");

        assert_eq!(sinal.relatou(relato(Some(0.08), None), agora), Ok(()));
        let medidas = sinal.medidas(&leitura(Some(10), Some(0.01), None), agora);
        assert_eq!(medidas.perda(), Some(0.08), "a maior é a relatada");
    }

    /// O jitter entra com a parcela que o `seele_proto::signal` já tinha para
    /// ele, e com os pesos de lá: nenhum número novo nesta conta.
    #[test]
    fn o_jitter_relatado_entra_com_a_parcela_do_protocolo() {
        use seele_proto::signal::{jitter_penalty, JITTER_FULL_MS, JITTER_MAX_PENALTY};

        let agora = Instant::now();
        let calado = leitura(Some(10), None, None);

        let mut sinal = SinalDaSessao::novo();
        assert_eq!(
            sinal.relatou(relato(None, Some(JITTER_FULL_MS)), agora),
            Ok(())
        );
        let entradas = sinal.medidas(&calado, agora).entradas();
        assert_eq!(entradas.map(|e| e.jitter_ms), Some(JITTER_FULL_MS));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let esperado = 100 - JITTER_MAX_PENALTY as u8;
        assert_eq!(regime(&mut sinal, &calado, agora), esperado);

        // Um valor do meio, contra a função do protocolo.
        let mut meio = SinalDaSessao::novo();
        assert_eq!(meio.relatou(relato(None, Some(40.0)), agora), Ok(()));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let esperado = (100.0 - jitter_penalty(40.0)).round() as u8;
        assert_eq!(regime(&mut meio, &calado, agora), esperado);

        // Sem relato, a parcela não penaliza e não é afirmada.
        let sem = SinalDaSessao::novo().medidas(&calado, agora);
        assert_eq!(sem.jitter_relatado_ms, None);
        assert_eq!(sem.entradas().map(|e| e.jitter_ms), Some(0.0));
    }

    /// Só um relato já alimenta a conta: é uma medida como as outras.
    #[test]
    fn o_relato_sozinho_ja_e_uma_medida() {
        let agora = Instant::now();
        let mut sinal = SinalDaSessao::novo();
        assert_eq!(sinal.relatou(relato(None, Some(2.0)), agora), Ok(()));
        sinal.tique(&leitura(None, None, None), agora);
        assert!(sinal.medido());
    }

    /// **Fora da faixa não entra, e não apaga o que valia.** A sessão não cai:
    /// quem chama recebe o motivo e segue.
    #[test]
    fn um_relato_fora_da_faixa_e_recusado_e_o_anterior_continua() {
        let agora = Instant::now();
        let calado = leitura(Some(10), None, None);
        let mut sinal = SinalDaSessao::novo();
        assert_eq!(sinal.relatou(relato(Some(0.10), Some(3.0)), agora), Ok(()));

        for (ruim, motivo) in [
            (relato(Some(f32::NAN), Some(3.0)), RecepcaoRecusada::Perda),
            (relato(Some(-0.5), None), RecepcaoRecusada::Perda),
            (relato(Some(7.0), None), RecepcaoRecusada::Perda),
            (relato(Some(0.0), Some(-1.0)), RecepcaoRecusada::Jitter),
            (
                relato(Some(0.0), Some(f32::INFINITY)),
                RecepcaoRecusada::Jitter,
            ),
            (relato(None, Some(1.0e9)), RecepcaoRecusada::Jitter),
            (relato(None, None), RecepcaoRecusada::Vazia),
        ] {
            assert_eq!(sinal.relatou(ruim, agora), Err(motivo), "{ruim:?}");
            let medidas = sinal.medidas(&calado, agora);
            assert_eq!(medidas.perda_relatada, Some(0.10), "depois de {ruim:?}");
            assert_eq!(medidas.jitter_relatado_ms, Some(3.0), "depois de {ruim:?}");
        }

        // E um sinal que só recebeu lixo não tem relato nenhum.
        let mut so_lixo = SinalDaSessao::novo();
        assert!(so_lixo
            .relatou(relato(Some(f32::NAN), None), agora)
            .is_err());
        assert_eq!(so_lixo.medidas(&calado, agora).perda_relatada, None);
        assert_eq!(regime(&mut so_lixo, &calado, agora), 100);
    }

    /// **O relato que cessa deixa de valer.** O cliente só relata enquanto
    /// ouve voz; passada a validade, a perda relatada não é mais medida e o
    /// sinal volta ao que o resto diz.
    #[test]
    fn o_relato_que_cessa_deixa_de_valer_depois_da_janela() {
        let inicio = Instant::now();
        let calado = leitura(Some(10), None, None);
        let mut sinal = SinalDaSessao::novo();
        assert_eq!(
            sinal.relatou(relato(Some(0.10), Some(50.0)), inicio),
            Ok(())
        );
        assert!(regime(&mut sinal, &calado, inicio) < 70);

        let no_limite = inicio + VALIDADE_DO_RELATO;
        let medidas = sinal.medidas(&calado, no_limite);
        assert_eq!(medidas.perda_relatada, Some(0.10));
        assert_eq!(medidas.jitter_relatado_ms, Some(50.0));

        let depois = no_limite + Duration::from_millis(1);
        let medidas = sinal.medidas(&calado, depois);
        assert_eq!(medidas.perda_relatada, None);
        assert_eq!(medidas.jitter_relatado_ms, None);
        assert_eq!(regime(&mut sinal, &calado, depois), 100);

        // Um relato novo volta a valer, a partir de quando chegou.
        assert_eq!(sinal.relatou(relato(Some(0.10), None), depois), Ok(()));
        assert_eq!(regime(&mut sinal, &calado, depois), 70);
    }

    /// Mil relatos no mesmo instante custam mil atribuições, e a conta lê o
    /// último, uma vez por anúncio. (Quem limita a chegada é o `Vigia`, antes
    /// de qualquer tratador: ver `tests/relato_de_recepcao.rs`.)
    #[test]
    fn mil_relatos_valem_o_ultimo() {
        let agora = Instant::now();
        let calado = leitura(Some(10), None, None);
        let mut sinal = SinalDaSessao::novo();
        for _ in 0..1000 {
            assert_eq!(sinal.relatou(relato(Some(1.0), None), agora), Ok(()));
        }
        assert_eq!(sinal.relatou(relato(Some(0.0), None), agora), Ok(()));
        assert_eq!(sinal.medidas(&calado, agora).perda_relatada, Some(0.0));
        assert_eq!(sinal.tique(&calado, agora), 100);
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
