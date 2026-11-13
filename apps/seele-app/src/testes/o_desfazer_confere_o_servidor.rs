#[test]
fn um_servidor_que_esta_maquina_nao_guarda_e_recusado() {
    let casa = tempfile::tempdir().expect("a casa");
    let config = casa.path().to_string_lossy().into_owned();
    for torto in ["nunca-existiu", "../sentinela", "/tmp/x"] {
        assert_eq!(
            super::desfazer_escolha_de_mod_em(&config, Some(torto), "x/y"),
            Err(super::mods::FalhaAoResolverModAntigo::ServidorDesconhecido),
            "o DESFAZER aceitou {torto:?}"
        );
    }
    assert!(
        !casa
            .path()
            .join(super::escolhas_de_mods::PASTA_DAS_ESCOLHAS)
            .exists(),
        "um servidor recusado ainda tocou a pasta das escolhas"
    );
    let guardado = super::servidores::criar(&config, "Casa", "0.17.0");
    assert_eq!(
        super::desfazer_escolha_de_mod_em(&config, Some(&guardado.id), "x/y"),
        Ok(()),
        "o DESFAZER recusou um guardado de verdade"
    );
}
