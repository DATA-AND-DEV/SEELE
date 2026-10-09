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
/// `cargo xtask web` roda no job de MOD do `ci.yml` e no `validar` do
/// `release.yml`, com os dois alvos instalados. A bateria do `publicar.sh` o
/// roda também, e quem prende isso é `xtask/tests/empacotamento.rs`.
#[test]
fn o_ci_e_o_validar_rodam_os_crates_do_celular_dentro_do_wasm() {
    for arquivo in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let texto = std::fs::read_to_string(raiz().join(arquivo)).expect("o workflow");
        let comandos: Vec<&str> = texto
            .lines()
            .map(str::trim)
            .filter_map(|l| l.strip_prefix("- run:").or_else(|| l.strip_prefix("run:")))
            .map(str::trim)
            .collect();
        assert!(
            comandos.contains(&"cargo xtask web"),
            "{arquivo} não roda `cargo xtask web`: o teto de memória do motor do celular e os \
             bytes-ouro no .wasm viram guardas que ninguém roda"
        );
        assert!(
            comandos.iter().any(|c| c.starts_with("rustup target add")
                && c.contains("wasm32-unknown-unknown")
                && c.contains("wasm32-wasip1")),
            "{arquivo} roda `cargo xtask web` sem instalar os dois alvos wasm"
        );
    }
}

/// **Os dois embrulham a mesma volta, e nenhum a reimplementa.**
///
/// Até 09/10/2026 o motor do celular tinha uma cópia menor da volta do
/// desktop, e ela não sabia das rejeições, do balde do `console` nem dos tetos
/// de temporizador: um MOD com uma `async` sem `catch` sumia calado no celular.
/// A volta passou para o `seele-motor`, e o guarda é de dependência, que é o
/// que não se contorna com cuidado: **sem `rquickjs`, o embrulho não tem como
/// avaliar JavaScript**, e portanto não tem como ter outra volta.
///
/// E a prova de que a volta é a mesma nos dois alvos é uma tabela só
/// (`crates/seele-motor/tests/casos.json`), que o nativo roda em `casos.rs` e
/// o `cargo xtask web` roda dentro do `.wasm`.
#[test]
fn o_desktop_e_o_celular_embrulham_a_mesma_volta_sem_reimplementar_nada() {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(raiz().join("Cargo.toml"))
        .no_deps()
        .exec()
        .expect("o cargo metadata");
    let dependencias = |nome: &str| -> Vec<String> {
        metadata
            .packages
            .iter()
            .find(|p| p.name.as_str() == nome)
            .unwrap_or_else(|| panic!("o pacote {nome}"))
            .dependencies
            .iter()
            .map(|d| d.name.clone())
            .collect()
    };
    assert!(
        dependencias("seele-motor").iter().any(|d| d == "rquickjs"),
        "o seele-motor deixou de ter o motor: o guarda abaixo ficaria vazio"
    );
    for embrulho in ["seele-app", "seele-motor-web"] {
        let deps = dependencias(embrulho);
        assert!(
            deps.iter().any(|d| d == "seele-motor"),
            "{embrulho} não usa o seele-motor: de onde vem a volta dele?"
        );
        assert!(
            !deps.iter().any(|d| d == "rquickjs"),
            "{embrulho} depende do rquickjs direto: com ele, a volta do seele-motor \
             pode ganhar uma segunda versão, e o celular e o desktop voltam a divergir"
        );
    }
    assert!(
        !raiz().join("crates/seele-motor-web/build.rs").exists(),
        "o seele-motor-web voltou a ter build.rs: o prelúdio é o do seele-motor, e não uma cópia"
    );
    let web = std::fs::read_to_string(raiz().join("xtask/src/web.rs")).expect("o web.rs");
    assert!(
        web.contains("\"crates/seele-motor/tests/casos.json\""),
        "o cargo xtask web não passa a tabela da volta ao .wasm: a prova de que os \
         dois alvos rodam a mesma volta deixa de existir"
    );
}
