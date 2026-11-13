/// **Fora da build de teste, o diagnóstico não existe.** O guarda lê o
/// fonte, como o `fora_da_build_de_teste_e_constante` do app: a chamada no
/// `measure` e o módulo inteiro estão sob o `cfg` do recurso com debug.
/// Roda sem o recurso, que é a build de todo dia.
#[test]
fn fora_da_build_de_teste_nao_existe() {
    let fonte = include_str!("../lib.rs");
    let porta = "#[cfg(all(debug_assertions, feature = \"aparelho-de-mentira\"))]";
    assert!(
        fonte.contains(&format!(
            "{porta}\n    let _ = diagnostico_da_voz::relatar(shared.carimbo(), \
                 telemetria.as_ref(), Instant::now());\n    medir_a_volta(sync, shared, \
                 telemetria.as_ref(), rtt_micros)\n}}"
        )),
        "a chamada do diagnóstico no `measure` saiu de baixo do cfg da build de teste"
    );
    assert!(
        fonte.contains(&format!("{porta}\nmod diagnostico_da_voz {{")),
        "o módulo do diagnóstico saiu de baixo do cfg da build de teste"
    );
    // A produção chama num lugar só, o `measure`. A conta era a de tudo o que
    // estava acima deste módulo de testes, quando ele era texto do `lib.rs`:
    // a produção e os módulos de teste de cima. Hoje é o `lib.rs` inteiro e os
    // outros módulos de teste dele, que moram nesta pasta. Este arquivo fica
    // de fora: o `com_o_recurso`, abaixo, chama o diagnóstico de propósito.
    let pasta = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/testes");
    let mut lido = fonte.to_owned();
    let mut outros = 0;
    for entrada in std::fs::read_dir(&pasta).expect("a pasta dos módulos de teste") {
        let caminho = entrada.expect("uma entrada de src/testes").path();
        let este = caminho
            .file_name()
            .is_some_and(|nome| nome == "o_diagnostico_da_voz.rs");
        if caminho.extension().is_some_and(|extensao| extensao == "rs") && !este {
            lido.push('\n');
            lido.push_str(&std::fs::read_to_string(&caminho).expect("um módulo de teste legível"));
            outros += 1;
        }
    }
    assert!(
        outros > 0,
        "{} não tem mais os outros módulos de teste do `lib.rs`: a conta ficou cega para eles",
        pasta.display()
    );
    assert_eq!(
        lido.matches(concat!("diagnostico_da_voz", "::relatar("))
            .count(),
        1,
        "o diagnóstico ganhou outra chamada fora do `measure`"
    );
}

/// O resto pede o recurso: sem ele o módulo não compila.
#[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
mod com_o_recurso {
    use super::super::{diagnostico_da_voz, Carimbo};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Instant;

    /// A variável é do processo: um teste destes por vez.
    static VEZ: Mutex<()> = Mutex::new(());

    /// Um assinante mínimo que guarda a mensagem de cada evento: o teste
    /// afirma o que o `tracing::info!` de fato emitiu, e não só a função pura.
    struct Captura(Arc<Mutex<Vec<String>>>);

    struct Mensagem(String);
    impl tracing::field::Visit for Mensagem {
        fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
            if campo.name() == "message" {
                self.0 = format!("{valor:?}");
            }
        }
    }

    impl tracing::Subscriber for Captura {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, evento: &tracing::Event<'_>) {
            let mut mensagem = Mensagem(String::new());
            evento.record(&mut mensagem);
            if *evento.metadata().level() == tracing::Level::INFO {
                self.0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(mensagem.0);
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn telemetria() -> seele_core::AudioTelemetry {
        seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry {
                playback_underruns: 11,
                capture_overruns: 2,
                playout_catchup_frames: 33,
                playout_resyncs: 4,
                playout_worst_lateness_ms: 27.5,
                pacing_ppm: -12.0,
                pacing_depth_ms: 41.0,
                pacing_target_ms: 40.0,
                pacing_clamps: 5,
                pacing_primes: 6,
                ..seele_core::LocalTelemetry::default()
            },
            sources: vec![
                seele_core::SourceTelemetry {
                    ssrc: 7,
                    jitter_depth_ms: 60.0,
                    jitter_ms: 1.25,
                    loss_fraction: 0.5,
                    expected_frames: 520,
                    missing_frames: 260,
                    concealed_frames: 260,
                    comfort_frames: 240,
                    late_frames: 9,
                    descartes_por_rajada: 12,
                    buraco_pulado: 3,
                    rtt_ms: None,
                },
                seele_core::SourceTelemetry {
                    ssrc: 8,
                    ..seele_core::SourceTelemetry::default()
                },
            ],
        }
    }

    /// Roda `corpo` com a variável no valor pedido, e a devolve depois.
    fn com_a_variavel<T>(valor: Option<&str>, corpo: impl FnOnce() -> T) -> T {
        let _vez = VEZ.lock().unwrap_or_else(PoisonError::into_inner);
        let antes = std::env::var_os("SEELE_AUDIO_DE_MENTIRA");
        match valor {
            Some(valor) => std::env::set_var("SEELE_AUDIO_DE_MENTIRA", valor),
            None => std::env::remove_var("SEELE_AUDIO_DE_MENTIRA"),
        }
        let feito = corpo();
        match antes {
            Some(antes) => std::env::set_var("SEELE_AUDIO_DE_MENTIRA", antes),
            None => std::env::remove_var("SEELE_AUDIO_DE_MENTIRA"),
        }
        feito
    }

    #[test]
    fn com_a_variavel_a_linha_sai_com_os_campos_uma_vez_a_cada_5_s() {
        crate::trilha_no_log::segundo_despachante();
        let capturado = Arc::new(Mutex::new(Vec::new()));
        let assinante = Captura(Arc::clone(&capturado));
        let telemetria = telemetria();
        let carimbo = Carimbo {
            sessao: 9_001,
            geracao: 2,
        };
        let agora = Instant::now();
        let (primeira, logo_depois, cinco_s_depois, outra_sessao) =
            com_a_variavel(Some("1"), || {
                tracing::subscriber::with_default(assinante, || {
                    (
                        diagnostico_da_voz::relatar(carimbo, Some(&telemetria), agora),
                        diagnostico_da_voz::relatar(
                            carimbo,
                            Some(&telemetria),
                            agora + std::time::Duration::from_millis(4_999),
                        ),
                        diagnostico_da_voz::relatar(
                            Carimbo {
                                sessao: 9_001,
                                geracao: 3,
                            },
                            None,
                            agora + diagnostico_da_voz::A_CADA,
                        ),
                        diagnostico_da_voz::relatar(
                            Carimbo {
                                sessao: 9_002,
                                geracao: 1,
                            },
                            Some(&telemetria),
                            agora,
                        ),
                    )
                })
            });
        let primeira = primeira.expect("com a variável, a primeira linha da sessão sai");
        assert_eq!(
            primeira,
            "diagnóstico da voz: sessao=9001 geracao=2 underruns=11 overruns=2 erros=0 \
                 eventos=0 catchup=33 resyncs=4 atraso_ms=27.5 pacing_ppm=-12.0 \
                 pacing_prof_ms=41.0 pacing_alvo_ms=40.0 pacing_grampos=5 pacing_primes=6 \
                 fontes=2 | ssrc=7 prof_ms=60.0 jitter_ms=1.25 perda=0.5000 plc=260 \
                 comfort=240 late=9 rajada=12 buraco_pulado=3 | ssrc=8 prof_ms=0.0 jitter_ms=0.00 \
                 perda=0.0000 plc=0 comfort=0 late=0 rajada=0 buraco_pulado=0"
        );
        assert_eq!(
            logo_depois, None,
            "duas linhas da mesma sessão em menos de 5 s"
        );
        assert_eq!(
            cinco_s_depois.as_deref(),
            Some("diagnóstico da voz: sessao=9001 geracao=3 voz=nenhuma"),
            "passados 5 s a sessão diz de novo, com a geração nova"
        );
        assert!(
            outra_sessao.is_some(),
            "o limite é por sessão, não do processo"
        );
        let emitido = capturado
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(
            emitido,
            vec![
                primeira,
                cinco_s_depois.unwrap_or_default(),
                outra_sessao.unwrap_or_default()
            ],
            "o `tracing::info!` não emitiu exatamente as linhas devolvidas"
        );
    }

    #[test]
    fn sem_a_variavel_nao_sai() {
        crate::trilha_no_log::segundo_despachante();
        for valor in [None, Some("0"), Some("")] {
            let capturado = Arc::new(Mutex::new(Vec::new()));
            let assinante = Captura(Arc::clone(&capturado));
            let disse = com_a_variavel(valor, || {
                tracing::subscriber::with_default(assinante, || {
                    diagnostico_da_voz::relatar(
                        Carimbo {
                            sessao: 9_100,
                            geracao: 1,
                        },
                        Some(&telemetria()),
                        Instant::now(),
                    )
                })
            });
            assert_eq!(
                disse, None,
                "com SEELE_AUDIO_DE_MENTIRA={valor:?} a linha saiu"
            );
            assert!(
                capturado
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_empty(),
                "com SEELE_AUDIO_DE_MENTIRA={valor:?} o log recebeu algo"
            );
        }
    }
}
