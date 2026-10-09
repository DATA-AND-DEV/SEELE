//! O prelúdio do desktop, lido do fonte e não copiado.
//!
//! Extrai o texto de `const PRELUDIO: &str = r#"…"#;` de
//! `crates/seele-motor/src/preludio.rs` (antes de 09/10/2026, do
//! `apps/seele-app/src/executor.rs`) e o escreve em `$OUT_DIR/preludio.js`. Se o
//! desktop mudar o prelúdio, o motor do navegador muda junto no próximo build;
//! se a constante sumir ou mudar de forma, o build falha aqui, em vez de rodar
//! um prelúdio velho.

use std::path::PathBuf;

fn main() {
    let raiz = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let executor = raiz.join("../../../crates/seele-motor/src/preludio.rs");
    println!("cargo:rerun-if-changed={}", executor.display());
    let fonte = std::fs::read_to_string(&executor).expect("crates/seele-motor/src/preludio.rs");
    let marca = "const PRELUDIO: &str = r#\"";
    let inicio = fonte.find(marca).expect("o executor não tem mais `const PRELUDIO`") + marca.len();
    let fim = inicio + fonte[inicio..].find("\"#;").expect("o PRELUDIO não fecha com \"#;");
    let saida = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("preludio.js");
    std::fs::write(saida, &fonte[inicio..fim]).unwrap();
}
