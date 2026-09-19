//! Roda a bancada de corridas do ciclo de vida de um MOD.
//!
//! `apps/seele-app/bancada/ciclo-do-executor.cjs` carrega `ui/mods-runtime.js`
//! de verdade e controla a **ordem dos eventos** — que é a única coisa que
//! separa um caminho correto de um que só parece correto.
//!
//! # Por que aqui, e não em `cargo test`
//!
//! Porque ela precisa de Node, e a bateria do produto não pode precisar: quem
//! roda `cargo test` é quem compila o SEELE, e o SEELE não usa Node para nada.
//! Uma bateria que reprova por falta de uma ferramenta que o produto não usa é
//! uma bateria que ensina a ignorá-la.
//!
//! Em `xtask` ela é obrigatória e explícita: `cargo xtask check-runtime`
//! reprova sem Node, porque quem chama esta ferramenta pediu por ela.

use std::process::ExitCode;

pub(crate) fn run() -> ExitCode {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let bancada = raiz.join("apps/seele-app/bancada/ciclo-do-executor.cjs");
    if !bancada.is_file() {
        eprintln!("check-runtime: a bancada sumiu de {}", bancada.display());
        return ExitCode::FAILURE;
    }

    match std::process::Command::new("node").arg(&bancada).status() {
        Ok(estado) if estado.success() => ExitCode::SUCCESS,
        Ok(estado) => {
            eprintln!("check-runtime: a bancada reprovou ({estado})");
            ExitCode::FAILURE
        }
        // **Nomeado, e não engolido.** Sem Node não dá para rodar, e dizer
        // «passou» aqui seria a pior resposta possível: quem chamou acharia que
        // as corridas foram provadas.
        Err(erro) => {
            eprintln!("check-runtime: não consegui rodar o `node`: {erro}");
            eprintln!("  a bancada precisa dele, e o produto não — ver o topo de xtask/src/check_runtime.rs");
            ExitCode::FAILURE
        }
    }
}
