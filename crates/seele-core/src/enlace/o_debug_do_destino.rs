use super::Destino;

/// O segredo de um destino é o convite de uso único ou a senha do servidor,
/// e o `Debug` é por onde um `tracing::debug!(?destino)` o levaria ao
/// `seele.log`. Lote R da F5, rodada 1.
#[test]
fn o_debug_do_destino_esconde_o_segredo_e_mostra_o_resto() {
    let segredo = "segredo-do-teste-do-debug-do-destino";
    let destino = Destino {
        servidor: "127.0.0.1:8384".parse().expect("endereço"),
        nome_tls: "localhost".into(),
        chave_do_pin: "casa".into(),
        apelido: "bia".into(),
        segredo: Some(segredo.to_owned()),
        esperada: None,
        o_pino_prova_o_servidor: true,
        aceito: None,
    };
    for impresso in [format!("{destino:?}"), format!("{destino:#?}")] {
        assert!(
            !impresso.contains(segredo),
            "o `Debug` de `Destino` imprime o segredo — o convite de uso único ou a \
                 senha do servidor —, e um `tracing::debug!(?destino)` o levaria ao \
                 `seele.log`:\n{impresso}"
        );
        assert!(
            impresso.contains("<oculto>"),
            "o `Debug` de `Destino` não diz que há segredo, e quem investiga uma \
                 recusa não sabe se a entrada levou um:\n{impresso}"
        );
        assert!(
            impresso.contains("bia") && impresso.contains("casa"),
            "o `Debug` de `Destino` escondeu mais que o segredo:\n{impresso}"
        );
    }
    let sem = Destino {
        segredo: None,
        ..destino
    };
    let impresso = format!("{sem:?}");
    assert!(
        impresso.contains("segredo: None"),
        "sem segredo, o `Debug` de `Destino` não diz `None`:\n{impresso}"
    );
}
