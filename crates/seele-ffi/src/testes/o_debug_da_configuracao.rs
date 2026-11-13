use super::ConnectConfig;

/// O `join_secret` é o convite de uso único ou a senha do servidor, e o
/// `Debug` é por onde um `tracing::info!(?config)` no `connect` do app o
/// levaria ao `seele.log` (o filtro padrão, `seele_app=info`, escreve lá).
/// Lote R da F5, rodada 1.
#[test]
fn o_debug_da_configuracao_esconde_o_segredo_e_mostra_o_resto() {
    let segredo = "segredo-do-teste-do-debug-da-configuracao";
    let config = ConnectConfig {
        server: "a.exemplo:8384".into(),
        alternate_servers: Vec::new(),
        nickname: "bia".into(),
        home: "/tmp/casa-de-teste".into(),
        join_secret: Some(segredo.to_owned()),
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    for impresso in [format!("{config:?}"), format!("{config:#?}")] {
        assert!(
            !impresso.contains(segredo),
            "o `Debug` de `ConnectConfig` imprime o `join_secret` — o convite de uso \
                 único ou a senha do servidor —, e um `tracing::info!(?config)` o levaria \
                 ao `seele.log`:\n{impresso}"
        );
        assert!(
            impresso.contains("<oculto>"),
            "o `Debug` de `ConnectConfig` não diz que há segredo, e quem investiga \
                 uma recusa não sabe se a entrada levou um:\n{impresso}"
        );
        assert!(
            impresso.contains("a.exemplo:8384") && impresso.contains("bia"),
            "o `Debug` de `ConnectConfig` escondeu mais que o segredo:\n{impresso}"
        );
    }
    let sem = ConnectConfig {
        join_secret: None,
        ..config
    };
    let impresso = format!("{sem:?}");
    assert!(
        impresso.contains("join_secret: None"),
        "sem segredo, o `Debug` de `ConnectConfig` não diz `None`:\n{impresso}"
    );
}
