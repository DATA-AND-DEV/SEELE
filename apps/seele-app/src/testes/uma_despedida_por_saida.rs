use super::ao_fechar_a_ultima_janela;
use std::sync::atomic::{AtomicBool, Ordering};

/// Sem a conferência no `ExitRequested`, a janela que fecha com a
/// despedida de um sinal correndo começava outra, e este teste reprova.
#[test]
fn a_janela_que_fecha_durante_a_despedida_nao_despede_de_novo() {
    let feita = AtomicBool::new(false);
    let mut comecou = 0;
    ao_fechar_a_ultima_janela(&feita, || comecou += 1);
    assert_eq!(comecou, 1, "a primeira saída não despediu");
    assert!(feita.load(Ordering::SeqCst));
    ao_fechar_a_ultima_janela(&feita, || comecou += 1);
    assert_eq!(
        comecou, 1,
        "a saída com a despedida correndo despediu de novo"
    );
}

/// Com a despedida da janela correndo, o sinal não chama o `exit` (o dela
/// vem). Sem o `return`, o sinal saía na hora, e este teste reprova.
#[cfg(unix)]
#[tokio::test]
async fn o_sinal_durante_a_despedida_da_janela_nao_sai_por_cima() {
    let feita = AtomicBool::new(true);
    let (mut despediu, mut saiu) = (false, false);
    super::despedir_no_sinal(
        &feita,
        "SIGTERM",
        async { despediu = true },
        std::future::pending(),
        || saiu = true,
    )
    .await;
    assert!(!saiu, "o sinal saiu por cima da despedida que já corria");
    assert!(!despediu, "o sinal despediu de novo");

    // Sem despedida correndo, ele despede e sai.
    let feita = AtomicBool::new(false);
    let (mut despediu, mut saiu) = (false, false);
    super::despedir_no_sinal(
        &feita,
        "SIGTERM",
        async { despediu = true },
        std::future::pending(),
        || saiu = true,
    )
    .await;
    assert!(despediu && saiu, "o sinal não despediu e saiu");
    assert!(feita.load(Ordering::SeqCst));
}
