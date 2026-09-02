//! Toda folha de lints própria é uma cópia, e cópia apodrece.
//!
//! O workspace declara as regras uma vez, em `[workspace.lints]`, e cada crate
//! herda com `[lints] workspace = true`. Dois crates não podem herdar: eles
//! precisam de `unsafe_code = "deny"` em vez de `forbid`, porque `forbid` não se
//! afrouxa com `allow` nem dentro de um módulo — e os dois falam com o sistema
//! operacional por FFI.
//!
//! - `seele-video`, pelo codificador por hardware (ADR 0041);
//! - `seele-instalador`, pela janela em Win32 (ADR 0043).
//!
//! O Cargo não tem como dizer «herde tudo menos uma linha», então cada um copia
//! a folha inteira. É aí que mora o defeito que este arquivo existe para pegar:
//! uma regra acrescentada ao workspace amanhã não chega às cópias, ninguém
//! percebe, e a exceção de **uma** linha vira, com o tempo, um crate que segue
//! outras regras.
//!
//! Este guarda morava em `crates/seele-video/tests/`, onde dizia de si mesmo que
//! aquele era «o único crate que não herda». Deixou de ser verdade quando o
//! instalador chegou — e um guarda que descreve errado o que guarda é o próximo
//! a envelhecer. Aqui ele não precisa saber quantos são: **acha sozinho**.

#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// As regras de um bloco `[...lints.rust]` e `[...lints.clippy]`, achatadas.
///
/// O prefixo separa os dois arquivos: no workspace as seções são
/// `[workspace.lints.rust]`, e num crate são `[lints.rust]`.
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

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("o xtask mora um nível abaixo da raiz")
        .to_path_buf()
}

/// Os `Cargo.toml` dos membros do workspace, lidos da lista de `members`.
///
/// Da lista, e não de uma varredura do disco: um diretório com `Cargo.toml` que
/// não está nos `members` não é compilado por ninguém, e cobrá-lo aqui seria
/// cobrar de código que o workspace não conhece.
fn membros() -> Vec<(String, String)> {
    let raiz = raiz();
    let manifesto = std::fs::read_to_string(raiz.join("Cargo.toml")).expect("a raiz é legível");
    let lista = manifesto
        .split("members = [")
        .nth(1)
        .and_then(|resto| resto.split(']').next())
        .expect("o workspace declara `members`");

    lista
        .lines()
        .filter_map(|linha| {
            let caminho = linha.trim().trim_end_matches(',').trim_matches('"');
            if caminho.is_empty() {
                return None;
            }
            let arquivo = raiz.join(caminho).join("Cargo.toml");
            std::fs::read_to_string(&arquivo)
                .ok()
                .map(|texto| (caminho.to_owned(), texto))
        })
        .collect()
}

#[test]
fn toda_folha_propria_so_difere_do_workspace_no_unsafe() {
    let do_workspace = folha(
        &std::fs::read_to_string(raiz().join("Cargo.toml")).expect("a raiz é legível"),
        "workspace.",
    );
    assert!(
        !do_workspace.is_empty(),
        "não achei `[workspace.lints.*]` na raiz; este guarda ficou cego"
    );

    let mut proprias = 0_usize;
    for (caminho, manifesto) in membros() {
        let daqui = folha(&manifesto, "");
        if daqui.is_empty() {
            // Herda, que é o caso comum e o desejável.
            continue;
        }
        proprias += 1;

        let faltando: Vec<&String> = do_workspace
            .keys()
            .filter(|chave| !daqui.contains_key(*chave))
            .collect();
        assert!(
            faltando.is_empty(),
            "{caminho} tem folha própria e não copiou: {faltando:?}.\n\
             A exceção é de **uma** linha; tudo o mais vale igual. Copie as que \
             faltam para o `Cargo.toml` de lá."
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

        assert_eq!(
            divergentes.len(),
            1,
            "{caminho}: esperava exatamente uma divergência — o `unsafe_code` — \
             e vieram {divergentes:?}"
        );
        let Some((chave, la, aqui)) = divergentes.first() else {
            unreachable!("a asserção acima já garantiu exatamente uma divergência")
        };
        assert_eq!(
            (chave.as_str(), la.as_str(), aqui.as_str()),
            ("unsafe_code", "\"forbid\"", "\"deny\""),
            "{caminho}: a única divergência permitida é `unsafe_code`: `forbid` \
             no workspace, `deny` no crate. Qualquer outra afrouxa em silêncio \
             uma regra que não tem nada a ver com falar com o sistema."
        );
    }

    assert!(
        proprias >= 2,
        "achei {proprias} folha(s) própria(s), e são dois os crates que não \
         herdam: `seele-video` e `seele-instalador`.\n\
         Se um deles voltou a herdar, ótimo — apague esta asserção junto. Se a \
         leitura dos `members` quebrou, este guarda está cego e passando."
    );
}
