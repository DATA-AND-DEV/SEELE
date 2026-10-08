//! O prelúdio do desktop, lido do fonte, e não copiado.
//!
//! Extrai o texto de `const PRELUDIO: &str = r#"…"#;` de
//! `apps/seele-app/src/executor.rs` e o escreve em `$OUT_DIR/preludio.js`. Se o
//! desktop mudar o prelúdio, o motor do celular muda junto no próximo build; se
//! a constante sumir ou mudar de forma, o build para aqui, em vez de rodar um
//! prelúdio velho. O teste `o_preludio_e_o_texto_do_executor_do_desktop` confere
//! o resultado.

use std::path::PathBuf;

fn main() {
    let raiz = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let executor = raiz.join("../../apps/seele-app/src/executor.rs");
    println!("cargo:rerun-if-changed={}", executor.display());
    let fonte = std::fs::read_to_string(&executor)
        .unwrap_or_else(|e| panic!("{}: {e}", executor.display()));
    let marca = "const PRELUDIO: &str = r#\"";
    let inicio = fonte
        .find(marca)
        .map(|i| i + marca.len())
        .unwrap_or_else(|| panic!("o executor do desktop não tem mais `{marca}`"));
    let tamanho = fonte
        .get(inicio..)
        .and_then(|resto| resto.find("\"#;"))
        .unwrap_or_else(|| panic!("o PRELUDIO do executor não fecha com \"#;"));
    let saida = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("preludio.js");
    std::fs::write(
        &saida,
        fonte.get(inicio..inicio + tamanho).unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{}: {e}", saida.display()));
}
