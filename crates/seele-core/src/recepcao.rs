//! O relato de recepção: o que esta ponta mede da voz que ouve, e quando é a
//! vez de dizê-lo ao servidor (`ClientMessage::RelatoDeRecepcao`, protocolo 13).
//!
//! # Por que existe
//!
//! O sinal que os outros veem de cada pessoa é composto no servidor, com o que
//! ele mede do caminho dela. De quem está calado e sem vídeo ele não mede
//! perda nenhuma, e jitter não mede de ninguém: as duas grandezas só existem
//! no receptor. Esta ponta já as media para a própria tela
//! (`seele_ffi::Snapshot::telemetry`) e não as dizia a ninguém.
//!
//! # O que é relatado
//!
//! A janela de cinco segundos de [`seele_audio::recepcao`] sobre os buffers de
//! jitter da voz: a pior perda e o menor jitter de chegada entre as pessoas
//! ouvidas (o porquê de cada escolha está lá). **Não** é a perda que a
//! telemetria local mostra, que é cumulativa desde que cada buffer nasceu.
//!
//! Sem voz chegando — fora de toda sala, ou numa sala em que ninguém fala —
//! não há medida, e **nada é relatado**: o servidor deixa o último relato
//! envelhecer, e a parcela sai da conta. Um zero mandado no lugar afirmaria
//! uma recepção perfeita que ninguém mediu.
//!
//! # Onde mora
//!
//! Aqui, e não na casca: o ADR 0002 deixa na `seele-ffi` a decisão já tomada,
//! e o que vai num relato é decisão de protocolo. O portão de versão é do
//! enlace ([`crate::enlace::Enlace::relatar_recepcao`]).

use std::time::{Duration, Instant};

use seele_audio::recepcao::JanelaDeRecepcao;
use seele_audio::telemetry::AudioTelemetry;
use seele_proto::signal::{Recepcao, INTERVALO_DO_RELATO_MS};

/// De quanto em quanto tempo esta ponta relata.
pub const INTERVALO: Duration = Duration::from_millis(INTERVALO_DO_RELATO_MS);

/// Quem diz quando é a vez do relato, e o que ele leva.
#[derive(Debug)]
pub struct Relator {
    janela: JanelaDeRecepcao,
    /// De onde o relógio da janela conta.
    origem: Instant,
    proximo: Instant,
}

impl Relator {
    /// Um relator cuja primeira vez é um [`INTERVALO`] depois de `agora`.
    #[must_use]
    pub fn novo(agora: Instant) -> Self {
        Self {
            janela: JanelaDeRecepcao::nova(),
            origem: agora,
            proximo: agora + INTERVALO,
        }
    }

    /// A vez do relato.
    ///
    /// Antes de passar o [`INTERVALO`] devolve `None` **sem ler a
    /// telemetria** (quem chama a tira de trás de um cadeado, e isto é
    /// chamado a cada volta do laço). Na vez, lê as contagens da voz — `None`
    /// de `telemetria` é «sem voz aberta» —, anda a janela e devolve o relato
    /// a mandar, ou `None` se nada foi medido.
    pub fn vez(
        &mut self,
        agora: Instant,
        telemetria: impl FnOnce() -> Option<AudioTelemetry>,
    ) -> Option<Recepcao> {
        if agora < self.proximo {
            return None;
        }
        // A partir de agora, e não do compasso anterior: um laço que atrasou
        // não manda dois relatos seguidos para compensar.
        self.proximo = agora + INTERVALO;
        let contagens = telemetria()
            .map(|telemetria| telemetria.contagens_da_recepcao())
            .unwrap_or_default();
        let agora_ms = agora.saturating_duration_since(self.origem).as_secs_f64() * 1000.0;
        let medida = self.janela.observar(agora_ms, &contagens);
        Recepcao::medida(medida.perda, medida.jitter_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_audio::telemetry::{LocalTelemetry, SourceTelemetry};

    /// Uma voz com uma pessoa falando: `segundos` de fala, com `faltaram` de
    /// cada cinquenta lugares sem quadro.
    fn voz(segundos: u64, faltaram: u64, jitter_ms: f64) -> Option<AudioTelemetry> {
        Some(AudioTelemetry {
            local: LocalTelemetry::default(),
            sources: vec![SourceTelemetry {
                ssrc: 7,
                expected_frames: segundos * 50,
                missing_frames: segundos * faltaram,
                jitter_ms,
                ..SourceTelemetry::default()
            }],
        })
    }

    #[test]
    fn antes_da_vez_a_telemetria_nem_e_lida() {
        let inicio = Instant::now();
        let mut relator = Relator::novo(inicio);
        let relato = relator.vez(inicio + Duration::from_millis(999), || {
            panic!("a telemetria foi lida fora da vez")
        });
        assert_eq!(relato, None);
    }

    /// Dez por cento de perda no que se ouve viram dez por cento no relato,
    /// uma vez por segundo, depois de a janela ter fala bastante.
    #[test]
    fn a_perda_da_voz_ouvida_vira_relato() {
        let inicio = Instant::now();
        let mut relator = Relator::novo(inicio);
        let mut relatos = Vec::new();
        for segundo in 1..=6_u64 {
            let agora = inicio + Duration::from_secs(segundo);
            relatos.push(relator.vez(agora, || voz(segundo, 5, 3.0)));
            // No mesmo segundo, de novo: não é a vez.
            assert_eq!(
                relator.vez(agora + Duration::from_millis(250), || voz(segundo, 5, 3.0)),
                None
            );
        }
        // A primeira volta marca o ponto de partida e a segunda ainda tem
        // pouca fala; da terceira em diante há medida.
        assert_eq!(relatos[0], None);
        assert_eq!(relatos[1], None);
        for relato in &relatos[2..] {
            let relato = relato.expect("com fala bastante há relato");
            assert!(relato.perda.is_some_and(|p| (p - 0.1).abs() < 1e-6));
            assert_eq!(relato.jitter_ms, Some(3.0));
            assert_eq!(relato.confere(), Ok(()));
        }
    }

    /// **Sem voz, nada é relatado** — nem zero. E quando a voz fecha no meio,
    /// o relato para: o servidor deixa o último envelhecer.
    #[test]
    fn sem_voz_chegando_nao_ha_relato() {
        let inicio = Instant::now();
        let mut relator = Relator::novo(inicio);
        for segundo in 1..=4_u64 {
            assert_eq!(
                relator.vez(inicio + Duration::from_secs(segundo), || None),
                None
            );
        }
        // Uma voz aberta sem ninguém falando também não mede nada.
        let vazia = AudioTelemetry::default();
        assert_eq!(
            relator.vez(inicio + Duration::from_secs(5), || Some(vazia)),
            None
        );

        let mut relator = Relator::novo(inicio);
        for segundo in 1..=4_u64 {
            relator.vez(inicio + Duration::from_secs(segundo), || {
                voz(segundo, 5, 3.0)
            });
        }
        assert_eq!(
            relator.vez(inicio + Duration::from_secs(5), || None),
            None,
            "a voz fechou e o relato continuou"
        );
    }

    /// Um laço que atrasou não manda dois relatos seguidos para compensar.
    #[test]
    fn um_laco_atrasado_nao_relata_em_rajada() {
        let inicio = Instant::now();
        let mut relator = Relator::novo(inicio);
        let tarde = inicio + Duration::from_secs(10);
        let _ = relator.vez(tarde, || voz(10, 0, 1.0));
        assert_eq!(
            relator.vez(tarde + Duration::from_millis(10), || {
                panic!("segunda leitura em rajada")
            }),
            None
        );
    }
}
