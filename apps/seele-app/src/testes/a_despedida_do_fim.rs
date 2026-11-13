use super::{despedir_no_fim, dizer_a_despedida_do_fim, DespedidaNoFim};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// **A saída nunca trava.** Com as duas metades presas, a despedida volta
/// no teto, e diz que ele venceu.
#[test]
fn a_despedida_do_fim_nunca_passa_do_teto() {
    let feita = AtomicBool::new(false);
    let teto = Duration::from_millis(300);
    let comeco = Instant::now();
    let (desfecho, levou) = despedir_no_fim(
        &feita,
        teto,
        || std::thread::sleep(Duration::from_secs(3)),
        async { tokio::time::sleep(Duration::from_secs(3)).await },
    );
    let medido = comeco.elapsed();
    assert_eq!(desfecho, DespedidaNoFim::TetoVenceu);
    assert!(
        medido < teto + Duration::from_millis(200),
        "a despedida do fim segurou a saída {medido:?}, com teto de {teto:?}: o \
             AppKit espera, e o app não fecha"
    );
    assert!(levou >= teto, "o teto venceu e a conta diz {levou:?}");
}

/// **As duas metades, qualquer que acabe primeiro.** A despedida só diz
/// que rodou depois de a sessão **e** as hospedagens acabarem.
#[test]
fn a_despedida_do_fim_espera_as_duas_metades() {
    for (da_sessao, das_hospedagens) in [(100, 250), (250, 100)] {
        let feita = AtomicBool::new(false);
        let sessao = Arc::new(AtomicBool::new(false));
        let hospedagens = Arc::new(AtomicBool::new(false));
        let (marca_a_sessao, marca_as_hospedagens) =
            (Arc::clone(&sessao), Arc::clone(&hospedagens));
        let (desfecho, _) = despedir_no_fim(
            &feita,
            Duration::from_secs(2),
            move || {
                std::thread::sleep(Duration::from_millis(da_sessao));
                marca_a_sessao.store(true, Ordering::SeqCst);
            },
            async move {
                tokio::time::sleep(Duration::from_millis(das_hospedagens)).await;
                marca_as_hospedagens.store(true, Ordering::SeqCst);
            },
        );
        assert_eq!(desfecho, DespedidaNoFim::Rodou);
        assert!(
            sessao.load(Ordering::SeqCst) && hospedagens.load(Ordering::SeqCst),
            "a despedida do fim voltou antes de uma metade acabar (sessão em \
                 {da_sessao} ms, hospedagens em {das_hospedagens} ms)"
        );
        assert!(
            feita.load(Ordering::SeqCst),
            "a despedida rodou e não ficou marcada"
        );
    }
}

/// **Uma vez por processo.** Depois da saída comum, que já despediu e
/// marcou, o `RunEvent::Exit` não despede de novo.
#[test]
fn a_despedida_do_fim_nao_repete_a_da_janela() {
    let feita = AtomicBool::new(true);
    let chamou = Arc::new(AtomicBool::new(false));
    let (pela_sessao, pelas_hospedagens) = (Arc::clone(&chamou), Arc::clone(&chamou));
    let (desfecho, _) = despedir_no_fim(
        &feita,
        Duration::from_secs(1),
        move || pela_sessao.store(true, Ordering::SeqCst),
        async move { pelas_hospedagens.store(true, Ordering::SeqCst) },
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(desfecho, DespedidaNoFim::JaFeita);
    assert!(
        !chamou.load(Ordering::SeqCst),
        "a despedida do fim despediu de novo uma saída que já tinha se despedido"
    );
}

/// **Uma metade que cai não vira «rodou».**
#[test]
fn a_despedida_do_fim_diz_quando_uma_metade_cai() {
    let feita = AtomicBool::new(false);
    let (desfecho, _) = despedir_no_fim(
        &feita,
        Duration::from_secs(1),
        || panic!("a metade da sessão caiu (de propósito, no teste)"),
        async {},
    );
    assert_eq!(desfecho, DespedidaNoFim::Caiu);
}

/// **O `seele.log` diz o que aconteceu**: se rodou e quanto levou, ou que
/// o teto venceu. Um silêncio aqui é a pergunta de quem usa, dias depois,
/// sem dado nenhum.
#[test]
fn a_despedida_do_fim_diz_no_log_o_que_conseguiu() {
    for (desfecho, agulhas) in [
        (DespedidaNoFim::JaFeita, &["já tinha rodado"][..]),
        (
            DespedidaNoFim::Rodou,
            &["se despediram", "levou_ms=420"][..],
        ),
        (
            DespedidaNoFim::TetoVenceu,
            &[
                "WARN",
                "o teto da despedida venceu",
                "levou_ms=420",
                "teto_ms=1000",
            ][..],
        ),
        (DespedidaNoFim::Caiu, &["WARN", "caiu antes de acabar"][..]),
    ] {
        let ((), rastro) = crate::rastro_de_teste::capturar(|| {
            dizer_a_despedida_do_fim(desfecho, Duration::from_millis(420));
        });
        for agulha in agulhas {
            assert!(
                rastro.contains(agulha),
                "a linha de {desfecho:?} não diz `{agulha}`: {rastro}"
            );
        }
    }
}
