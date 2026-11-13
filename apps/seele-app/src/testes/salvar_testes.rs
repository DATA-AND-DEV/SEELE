#![allow(clippy::unwrap_used)]

/// O MOD sugere, e o produto limpa: sem barra (nada de caminho), sem
/// arquivo escondido, com teto.
#[test]
fn o_nome_sugerido_nao_vira_caminho() {
    assert_eq!(
        super::nome_sugerido_ao_salvar("Casa do SEELE.mrpack"),
        "Casa do SEELE.mrpack"
    );
    assert_eq!(super::nome_sugerido_ao_salvar("../../.zshrc"), "zshrc");
    assert_eq!(
        super::nome_sugerido_ao_salvar("a/b\\c.mrpack"),
        "abc.mrpack"
    );
    assert_eq!(super::nome_sugerido_ao_salvar("..."), "arquivo");
    assert_eq!(super::nome_sugerido_ao_salvar(&"x".repeat(200)).len(), 80);
}

#[test]
fn o_salvo_entra_de_uma_vez_e_sem_parcial() {
    let pasta = std::env::temp_dir().join(format!("seele-salvar-{}", std::process::id()));
    std::fs::create_dir_all(&pasta).unwrap();
    let destino = pasta.join("pack.mrpack");
    super::gravar_o_salvo(&destino, b"PK pack").unwrap();
    assert_eq!(std::fs::read(&destino).unwrap(), b"PK pack");
    assert!(!pasta.join("pack.mrpack.seele-parcial").exists());
    assert!(super::gravar_o_salvo(&pasta.join("nao/existe/x"), b"x").is_err());
    let _ = std::fs::remove_dir_all(&pasta);
}
