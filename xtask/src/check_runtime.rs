//! Roda as bancadas de JavaScript do produto.
//!
//! Duas, e as duas carregam o arquivo de verdade em vez de uma cópia:
//!
//! - `ciclo-do-executor.cjs` põe `ui/mods-runtime.js` num contexto de VM e
//!   controla a **ordem dos eventos** — que é a única coisa que separa um
//!   caminho correto de um que só parece correto;
//! - `regiao-do-mod.cjs` põe `ui/mods-regiao.js` num DOM mínimo e mede o que um
//!   guarda de texto não alcança: que atualizar não tira do documento quem tem
//!   foco, e que sair **para o som** em vez de só tirar o nó da tela;
//! - `continuacao-de-midia.cjs` extrai de `ui/base.js` o laço que busca uma
//!   mídia grande do servidor de um MOD e o roda contra um servidor de mentira:
//!   a ordem dos pedaços, o teto de voltas e a falha nomeada.
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
    // **Todas, e não até a primeira que reprova.** Saber que duas quebraram é
    // uma informação diferente de saber que uma quebrou, e é a que diz se a
    // mudança foi num lugar ou na fronteira entre os dois.
    let mut falhou = false;
    for nome in [
        "ciclo-do-executor.cjs",
        "regiao-do-mod.cjs",
        "continuacao-de-midia.cjs",
    ] {
        let bancada = raiz.join("apps/seele-app/bancada").join(nome);
        if !bancada.is_file() {
            eprintln!("check-runtime: a bancada sumiu de {}", bancada.display());
            return ExitCode::FAILURE;
        }
        match std::process::Command::new("node").arg(&bancada).status() {
            Ok(estado) if estado.success() => {}
            Ok(estado) => {
                eprintln!("check-runtime: {nome} reprovou ({estado})");
                falhou = true;
            }
            // **Nomeado, e não engolido.** Sem Node não dá para rodar, e dizer
            // «passou» aqui seria a pior resposta possível: quem chamou acharia
            // que as corridas foram provadas.
            Err(erro) => {
                eprintln!("check-runtime: não consegui rodar o `node`: {erro}");
                eprintln!("  a bancada precisa dele, e o produto não — ver o topo de xtask/src/check_runtime.rs");
                return ExitCode::FAILURE;
            }
        }
    }
    if falhou {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
