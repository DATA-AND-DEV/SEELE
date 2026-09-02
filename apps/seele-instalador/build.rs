//! Embute o ícone no executável.
//!
//! # Por que um `build.rs` para isto
//!
//! No Windows o ícone que o Explorer mostra não é um arquivo ao lado do
//! programa: é um **recurso dentro do `.exe`**, e recurso se compila junto. Sem
//! isso o instalador aparece com o ícone genérico de aplicativo — na tela em que
//! alguém decide se confia no arquivo que acabou de baixar.
//!
//! # Por que `embed-resource`
//!
//! Porque ele **já está na árvore**: o `tauri-build` o traz para embutir o
//! ícone do próprio SEELE. Usá-lo aqui não acrescenta uma linha ao grafo de
//! dependências nem uma superfície nova para auditar — que é o critério do ADR
//! 0019 para o que entra.

fn main() {
    // O recurso só existe no Windows. Fora dele o `build.rs` não faz nada, e a
    // crate continua compilando no Mac — que é onde a bateria roda primeiro.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icone.rc");
        println!("cargo:rerun-if-changed=../seele-app/icons/icon.ico");
        // O `expect` é negado pela folha de lints, e aqui a alternativa é
        // melhor mesmo: um `panic!` de `build.rs` sai como rastro de pânico do
        // Rust no meio da compilação, e `panic!` com a frase certa sai como a
        // frase certa.
        if let Err(erro) =
            embed_resource::compile("icone.rc", embed_resource::NONE).manifest_optional()
        {
            panic!(
                "o ícone não entrou no executável: {erro}\n\
                 Sem ele o instalador aparece com o ícone genérico de \
                 aplicativo — na tela em que alguém decide se confia no arquivo \
                 que acabou de baixar."
            );
        }
    }
}
