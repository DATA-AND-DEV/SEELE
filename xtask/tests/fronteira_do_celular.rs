//! Nos dois crates do celular, o `unsafe` mora num arquivo só.
//!
//! O `seele-web` e o `seele-motor-web` falam com o JavaScript por funções
//! exportadas e importadas de um `.wasm`, e isso pede `unsafe`. A exceção ao
//! `forbid` do workspace segue o molde do `seele-video` (ADR 0041) e do
//! `seele-instalador` (ADR 0043): `unsafe_code = "deny"` na folha (o
//! `folha_de_lints` confere), e o `allow` **só** num `fronteira.rs` (emenda de
//! 08/10/2026 ao ADR 0057).
//!
//! O `deny` já reprova um `unsafe` posto noutro arquivo, na compilação. O que
//! ele não pega é alguém pôr, junto, um `#[allow(unsafe_code)]`: é isso que
//! este guarda pega. E ele confere que cada `fronteira.rs` existe e exige o
//! comentário `// SAFETY:` pelo clippy, para o guarda não ficar cego.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};

/// Os crates com a exceção, a partir da raiz.
const CRATES: [&str; 2] = ["crates/seele-web", "crates/seele-motor-web"];

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("o xtask mora um nível abaixo da raiz")
        .to_path_buf()
}

/// Todo `.rs` do crate, fora de `target/`: `src/`, `tests/`, `build.rs`.
fn fontes(pasta: &Path) -> Vec<PathBuf> {
    let mut achados = Vec::new();
    let mut pendentes = vec![pasta.to_path_buf()];
    while let Some(atual) = pendentes.pop() {
        for entrada in std::fs::read_dir(&atual).expect("legível") {
            let caminho = entrada.expect("entrada").path();
            if caminho.is_dir() {
                if caminho.file_name().is_some_and(|n| n != "target") {
                    pendentes.push(caminho);
                }
            } else if caminho.extension().is_some_and(|e| e == "rs") {
                achados.push(caminho);
            }
        }
    }
    achados.sort();
    achados
}

/// Um `allow` ou `expect` que nomeia `unsafe_code`, em qualquer forma:
/// `#[allow(unsafe_code)]`, `#![allow(unsafe_code)]`,
/// `#[allow(dead_code, unsafe_code)]`, `#[expect(unsafe_code)]`.
fn libera_unsafe(texto: &str) -> bool {
    texto.lines().any(|linha| {
        let codigo = linha.split("//").next().unwrap_or("");
        (codigo.contains("allow(") || codigo.contains("expect(")) && codigo.contains("unsafe_code")
    })
}

#[test]
fn o_unsafe_dos_crates_do_celular_mora_so_na_fronteira() {
    for krate in CRATES {
        let pasta = raiz().join(krate);
        let fronteira = pasta.join("src/fronteira.rs");
        let texto = std::fs::read_to_string(&fronteira)
            .unwrap_or_else(|e| panic!("{krate}: sem src/fronteira.rs ({e}); o guarda ficou cego"));
        assert!(
            texto.contains("#![allow(unsafe_code)]"),
            "{krate}: a fronteira.rs não libera o unsafe; ou o crate não precisa mais da \
             exceção (e a folha volta a herdar), ou o allow foi parar noutro lugar"
        );
        assert!(
            texto.contains("#![deny(clippy::undocumented_unsafe_blocks)]"),
            "{krate}: a fronteira.rs tem de exigir o `// SAFETY:` de cada bloco pelo clippy"
        );

        let fora: Vec<String> = fontes(&pasta)
            .into_iter()
            .filter(|f| f != &fronteira)
            .filter(|f| libera_unsafe(&std::fs::read_to_string(f).expect("legível")))
            .map(|f| f.strip_prefix(raiz()).unwrap_or(&f).display().to_string())
            .collect();
        assert!(
            fora.is_empty(),
            "{krate}: `unsafe_code` liberado fora da fronteira.rs: {fora:?}.\n\
             A exceção do ADR 0057 é de um arquivo só por crate."
        );
    }
}

/// O que só o `.wasm` prova (os bytes-ouro pela fronteira, o teto de memória
/// do motor, as importações fixas) só é guarda se alguém o roda. O
/// `cargo xtask web` roda no job de MOD do CI, com os dois alvos instalados.
#[test]
fn o_ci_roda_os_crates_do_celular_dentro_do_wasm() {
    let ci = std::fs::read_to_string(raiz().join(".github/workflows/ci.yml")).expect("o ci.yml");
    let comandos: Vec<&str> = ci
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- run:").or_else(|| l.strip_prefix("run:")))
        .map(str::trim)
        .collect();
    assert!(
        comandos.iter().any(|c| *c == "cargo xtask web"),
        "o ci.yml não roda `cargo xtask web`: o teto de memória do motor do celular e os \
         bytes-ouro no .wasm viram guardas que ninguém roda"
    );
    assert!(
        comandos.iter().any(|c| c.starts_with("rustup target add")
            && c.contains("wasm32-unknown-unknown")
            && c.contains("wasm32-wasip1")),
        "o ci.yml roda `cargo xtask web` sem instalar os dois alvos wasm"
    );
}
