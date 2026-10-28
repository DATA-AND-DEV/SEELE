//! Generates the Tauri context from `tauri.conf.json`.
#[path = "../../empacotar/wasm_nativo.rs"]
mod wasm_nativo;

fn main() {
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let raiz = app
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or_else(|| panic!("o app deve estar em apps/seele-app"));
    println!("cargo:rerun-if-env-changed=SEELE_VERSAO");
    // Direct Cargo checks/tests need no browser runtime. Optimized product builds
    // must have the exact production module prepared by the Tauri hook.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        for nome in [
            "crates",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "xtask/src/pacote_web.rs",
            "empacotar/wasm_nativo.rs",
            "apps/seele-app/chaves/mods.pub",
            "apps/seele-app/wasm-nativo.sha256",
            "apps/seele-app/ui/seele_web.wasm",
        ] {
            println!("cargo:rerun-if-changed={}", raiz.join(nome).display());
        }
        wasm_nativo::conferir(raiz).unwrap_or_else(|erro| {
            panic!("{erro}. Rode `cargo xtask wasm-nativo` antes de compilar o desktop.")
        });
    }
    tauri_build::build();
}
