//! `cargo xtask web`: os dois crates do celular, dentro do `.wasm`.
//!
//! O que os testes nativos não provam, porque só existe no `wasm`:
//!
//! 1. o `seele-web` compila para `wasm32-unknown-unknown`, e os bytes-ouro da
//!    F1 cruzam a fronteira de verdade com os mesmos bytes, com o JSON passando
//!    pelo JavaScript (`xtask/web/ouro.mjs`);
//! 2. **sem `WASI_SDK`, o build do motor para, e não baixa nada**: o
//!    `.cargo/config.toml` aponta a variável para uma pasta que não existe;
//! 3. o wasi-sdk vem conferido ([`crate::wasi_sdk`]), e o motor compila com ele;
//! 4. o clippy passa no `wasm32-wasip1`, que é o único alvo onde a
//!    `fronteira.rs` do motor existe, e com ela o `// SAFETY:` cobrado;
//! 5. o motor, dentro do `.wasm`, só importa as funções da lista, fala a API do
//!    desktop, corta o laço sem fim e **para no teto de memória**, que só vale
//!    com o `RustAllocator` (`xtask/web/motor.mjs`).
//!
//! Precisa de `node`, `curl`, `tar` e dos alvos `wasm32-unknown-unknown` e
//! `wasm32-wasip1`. Roda todos os passos e diz cada um, como o `check-runtime`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Um passo: o nome, e `None` se passou ou o motivo se não.
type Passo = (&'static str, Option<String>);

fn rodar(comando: &mut Command) -> Result<std::process::Output, String> {
    comando
        .current_dir(raiz())
        .output()
        .map_err(|e| format!("não rodou: {e}"))
}

fn deu_certo(saida: Result<std::process::Output, String>) -> Option<String> {
    match saida {
        Ok(s) if s.status.success() => None,
        Ok(s) => Some(format!(
            "{}\n{}{}",
            s.status,
            String::from_utf8_lossy(&s.stdout),
            ultimas_linhas(&String::from_utf8_lossy(&s.stderr), 25)
        )),
        Err(e) => Some(e),
    }
}

fn ultimas_linhas(texto: &str, n: usize) -> String {
    let linhas: Vec<&str> = texto.lines().collect();
    linhas
        .get(linhas.len().saturating_sub(n)..)
        .unwrap_or_default()
        .join("\n")
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// O passo 2: sem `WASI_SDK`, o build tem de falhar dizendo o conserto, e
/// nenhum wasi-sdk pode aparecer baixado.
///
/// Num `target/` próprio: o `build.rs` do `rquickjs-sys` não declara
/// `rerun-if-env-changed=WASI_SDK`, e no `target/` de sempre o Cargo
/// reaproveitaria o build feito com o SDK conferido, e este passo passaria sem
/// provar nada.
fn sem_wasi_sdk() -> Option<String> {
    let alvo = raiz().join("target/xtask-sem-wasi-sdk");
    // Do zero, sempre: um build que tenha passado aqui (sem a linha do
    // `.cargo/config.toml`, por exemplo) seria reaproveitado na próxima vez.
    let _ = std::fs::remove_dir_all(&alvo);
    let saida = rodar(
        cargo()
            .args([
                "build",
                "--release",
                "-p",
                "seele-motor-web",
                "--target",
                "wasm32-wasip1",
            ])
            .env("CARGO_TARGET_DIR", &alvo)
            .env_remove("WASI_SDK"),
    );
    let saida = match saida {
        Ok(s) => s,
        Err(e) => return Some(e),
    };
    let erro = String::from_utf8_lossy(&saida.stderr);
    let baixado = std::fs::read_dir(alvo.join("wasm32-wasip1/release/build"))
        .map(|pastas| {
            pastas
                .filter_map(Result::ok)
                .any(|p| p.path().join("out/wasi-sdk").exists())
        })
        .unwrap_or(false);
    if saida.status.success() {
        Some("o motor compilou sem WASI_SDK: o .cargo/config.toml deixou de apontar para a pasta que não existe".into())
    } else if baixado {
        Some("o build.rs do rquickjs-sys baixou um wasi-sdk sem conferir".into())
    } else if !erro.contains("FALTA-rode-cargo-xtask-wasi-sdk") {
        Some(format!(
            "o build falhou, mas não pela falta do wasi-sdk conferido:\n{}",
            ultimas_linhas(&erro, 25)
        ))
    } else {
        None
    }
}

pub(crate) fn run() -> ExitCode {
    let raiz = raiz();
    let mut passos: Vec<Passo> = Vec::new();

    let web = raiz.join("target/wasm32-unknown-unknown/release/seele_web.wasm");
    passos.push((
        "seele-web para wasm32-unknown-unknown",
        deu_certo(rodar(cargo().args([
            "build",
            "--release",
            "-p",
            "seele-web",
            "--target",
            "wasm32-unknown-unknown",
            "--features",
            "prova",
        ]))),
    ));
    passos.push((
        "os bytes-ouro dentro do .wasm (xtask/web/ouro.mjs)",
        deu_certo(rodar(
            Command::new("node")
                .arg("xtask/web/ouro.mjs")
                .arg(&web)
                .arg("crates/seele-proto/tests/fio_1_0.txt"),
        )),
    ));

    passos.push(("sem WASI_SDK, o motor para e não baixa", sem_wasi_sdk()));

    match crate::wasi_sdk::caminho() {
        Err(e) => passos.push(("o wasi-sdk conferido", Some(e))),
        Ok(sdk) => {
            passos.push(("o wasi-sdk conferido", None));
            passos.push((
                "seele-motor-web para wasm32-wasip1, com o SDK conferido",
                deu_certo(rodar(
                    cargo()
                        .args([
                            "build",
                            "--release",
                            "-p",
                            "seele-motor-web",
                            "--target",
                            "wasm32-wasip1",
                        ])
                        .env("WASI_SDK", &sdk),
                )),
            ));
            passos.push((
                "clippy do motor no wasm32-wasip1 (a fronteira e o SAFETY)",
                deu_certo(rodar(
                    cargo()
                        .args([
                            "clippy",
                            "-p",
                            "seele-motor-web",
                            "--target",
                            "wasm32-wasip1",
                            "--",
                            "-D",
                            "warnings",
                        ])
                        .env("WASI_SDK", &sdk),
                )),
            ));
            passos.push((
                "o motor dentro do .wasm (xtask/web/motor.mjs)",
                deu_certo(rodar(
                    Command::new("node")
                        .arg("xtask/web/motor.mjs")
                        .arg(raiz.join("target/wasm32-wasip1/release/seele_motor_web.wasm"))
                        .arg("crates/seele-motor-web/tests/mod-comum.js"),
                )),
            ));
        }
    }

    let mut passou = true;
    for (nome, falha) in &passos {
        match falha {
            None => eprintln!("web: ok      {nome}"),
            Some(motivo) => {
                passou = false;
                eprintln!("web: FALHOU  {nome}\n{motivo}\n");
            }
        }
    }
    if passou {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
