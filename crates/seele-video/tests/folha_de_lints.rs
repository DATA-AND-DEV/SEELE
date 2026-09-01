//! A exceção do ADR 0041 não pode virar uma folha de lints à deriva.
//!
//! Este crate é o único que não herda `[workspace.lints]`, e o motivo é uma
//! linha só: `unsafe_code` precisa ser `deny` em vez de `forbid` para o módulo
//! do codificador por hardware poder existir. Todo o resto foi **copiado**, e
//! cópia é o que apodrece: uma regra acrescentada ao workspace amanhã não chega
//! aqui, e ninguém percebe — a exceção de uma linha vira, com o tempo, um crate
//! que segue outras regras.
//!
//! Ler os dois arquivos é o único jeito de prender isso: o Cargo não tem como
//! dizer «herde tudo menos uma».

#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;

/// As regras de um bloco `[...lints.rust]` e `[...lints.clippy]`, achatadas.
///
/// O prefixo separa os dois arquivos: no workspace as seções são
/// `[workspace.lints.rust]`, e aqui são `[lints.rust]`.
fn folha(texto: &str, prefixo: &str) -> BTreeMap<String, String> {
    let mut regras = BTreeMap::new();
    let mut dentro = false;
    for linha in texto.lines() {
        let cortada = linha.trim();
        if cortada.starts_with('[') {
            dentro = cortada.starts_with(&format!("[{prefixo}lints."));
            continue;
        }
        if !dentro || cortada.starts_with('#') {
            continue;
        }
        if let Some((chave, valor)) = cortada.split_once('=') {
            regras.insert(chave.trim().to_owned(), valor.trim().to_owned());
        }
    }
    regras
}

fn ler(caminho: &str) -> String {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"));
    let alvo = raiz.join(caminho);
    std::fs::read_to_string(&alvo).unwrap_or_else(|erro| panic!("não li {caminho}: {erro}"))
}

#[test]
fn a_folha_deste_crate_so_difere_do_workspace_no_unsafe() {
    let do_workspace = folha(&ler("../../Cargo.toml"), "workspace.");
    let daqui = folha(&ler("Cargo.toml"), "");

    assert!(
        !do_workspace.is_empty(),
        "não achei `[workspace.lints.*]` na raiz; este guarda ficou cego"
    );

    let faltando: Vec<&String> = do_workspace
        .keys()
        .filter(|chave| !daqui.contains_key(*chave))
        .collect();
    assert!(
        faltando.is_empty(),
        "o workspace tem regras que este crate não copiou: {faltando:?}.\n\
         A exceção do ADR 0041 é de **uma** linha; tudo o mais tem de valer \
         aqui igual. Copie as que faltam para `crates/seele-video/Cargo.toml`."
    );

    let divergentes: Vec<(&String, &String, &String)> = do_workspace
        .iter()
        .filter_map(|(chave, valor)| {
            daqui
                .get(chave)
                .filter(|aqui| *aqui != valor)
                .map(|aqui| (chave, valor, aqui))
        })
        .collect();

    let esperada = ("unsafe_code", "\"forbid\"", "\"deny\"");
    assert_eq!(
        divergentes.len(),
        1,
        "esperava exatamente uma divergência — o `unsafe_code` do ADR 0041 — e \
         vieram {divergentes:?}"
    );
    let (chave, la, aqui) = divergentes[0];
    assert_eq!(
        (chave.as_str(), la.as_str(), aqui.as_str()),
        esperada,
        "a única divergência permitida é `unsafe_code`: `forbid` no workspace, \
         `deny` aqui. Qualquer outra afrouxa em silêncio uma regra que não tem \
         nada a ver com o codec por hardware."
    );
}
