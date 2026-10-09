//! Os casos do link do navegador da F1, pela ponte do celular.
//!
//! A tabela `tests/links-da-f1.json` tem um caso por asserção do módulo
//! `a_forma_do_navegador` de `seele-proto/src/uri.rs`, com o esperado escrito à
//! mão a partir deles. Este teste a passa por `link_do_navegador`, no nativo; o
//! `cargo xtask web` a passa pela fronteira, dentro do `.wasm`
//! (`xtask/web/links.mjs`).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::Path;

use seele_web::link_do_navegador;

#[test]
fn os_casos_da_f1_passam_pela_ponte() {
    let tabela: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/links-da-f1.json"),
        )
        .expect("a tabela"),
    )
    .expect("a tabela é JSON");
    let casos = tabela["casos"].as_array().expect("os casos");
    assert!(
        casos.len() >= 16,
        "a tabela encolheu para {} casos",
        casos.len()
    );

    for caso in casos {
        let de = caso["de"].as_str().unwrap();
        let lido = link_do_navegador(
            caso["texto"].as_str().unwrap(),
            caso["origem"].as_str().unwrap(),
        );
        let esperado = &caso["esperado"];
        if let Some(erro) = esperado.get("erro") {
            assert_eq!(&lido["erro"], erro, "{de}: veio {lido}");
            continue;
        }
        // Só os campos que o caso escreveu: o resto o caso não afirma.
        for (campo, valor) in esperado["link"].as_object().unwrap() {
            assert_eq!(
                &lido["link"][campo], valor,
                "{de}: o campo {campo}; veio {lido}"
            );
        }
    }
}

#[test]
fn com_a_origem_do_produto_vazia_nenhum_link_e_do_navegador() {
    // Até o D-web-2, `ORIGEM_DO_NAVEGADOR` é vazia, e a página não tem como
    // aceitar link nenhum. A fronteira usa sempre esta constante.
    assert_eq!(seele_proto::uri::ORIGEM_DO_NAVEGADOR, "");
    let texto = format!(
        "https://origem.invalid/e#198.51.100.7:8383/?fp={}&wh={}",
        "3cbcfb02".repeat(8),
        "ab".repeat(32)
    );
    assert_eq!(
        link_do_navegador(&texto, seele_proto::uri::ORIGEM_DO_NAVEGADOR)["erro"],
        "OutraOrigem"
    );
}
