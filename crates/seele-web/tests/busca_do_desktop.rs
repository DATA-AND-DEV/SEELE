//! **A busca do navegador responde na forma do desktop** (Tarefa 2.6 da F4).
//!
//! O `BuscaEstado` mora no binário do app (`apps/seele-app/src/main.rs`), que
//! nenhum teste daqui alcança. Os campos dele são lidos do texto, e a resposta
//! do `seele_web::comandos` tem de ter os mesmos, nem um a mais: a casca lê
//! esses nomes (`tela-sessao.js`), e um campo renomeado de um lado só deixaria
//! a busca do celular muda sem nada reprovar.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;

use seele_web::comandos::{atender, DaPagina};

#[test]
fn o_estado_da_busca_tem_os_campos_do_buscaestado_do_desktop() {
    let main = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/seele-app/src/main.rs"
    ))
    .expect("o main.rs do app");
    let corpo = main
        .split_once("struct BuscaEstado {")
        .and_then(|(_, depois)| depois.split_once("\n}"))
        .map(|(corpo, _)| corpo)
        .expect("o `BuscaEstado` do desktop");
    let do_desktop: BTreeSet<String> = corpo
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.starts_with("//") && !linha.starts_with('#'))
        .filter_map(|linha| {
            linha
                .split_once(':')
                .map(|(nome, _)| nome.trim().to_owned())
        })
        .filter(|nome| !nome.is_empty() && nome.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    assert!(
        do_desktop.len() >= 5,
        "li só {do_desktop:?} do `BuscaEstado`"
    );

    let mut pagina = DaPagina::nova(1);
    let estado = atender(
        None,
        "busca_andar",
        &serde_json::json!({ "adiante": true }),
        &mut pagina,
        0,
    )
    .expect("um comando da sessão")
    .expect("respondido sem sessão")
    .resposta;
    let do_navegador: BTreeSet<String> = estado
        .as_object()
        .expect("um objeto")
        .keys()
        .cloned()
        .collect();
    assert_eq!(do_navegador, do_desktop);
}
