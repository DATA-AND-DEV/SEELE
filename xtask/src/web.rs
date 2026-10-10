//! `cargo xtask web`: os crates do celular, dentro do `.wasm`.
//!
//! O que os testes nativos não provam, porque só existe no `wasm`:
//!
//! 1. o `seele-web` compila para `wasm32-unknown-unknown`, e os bytes-ouro da
//!    F1 cruzam a fronteira de verdade com os mesmos bytes, com o JSON passando
//!    pelo JavaScript (`xtask/web/ouro.mjs`); e os casos do link do navegador
//!    da F1 passam pela fronteira (`xtask/web/links.mjs`);
//! 2. o `seele-sessao` (o `Room` que o navegador dobra, Tarefa 2.2) compila
//!    para `wasm32-unknown-unknown`: uma dependência nova dele que não compile
//!    no navegador para aqui, e não no dia em que o `seele-web` a usar;
//!    O mesmo vale para o `seele-voz` (o portão, o jitter e o mixer da voz,
//!    Tarefa 3.3);
//! 3. **sem `WASI_SDK`, o build do motor para, e não baixa nada**: o
//!    `.cargo/config.toml` aponta a variável para uma pasta que não existe;
//! 4. o wasi-sdk vem conferido ([`crate::wasi_sdk`]), e o motor compila com ele;
//! 5. o clippy passa no `wasm32-wasip1`, que é o único alvo onde a
//!    `fronteira.rs` do motor existe, e com ela o `// SAFETY:` cobrado;
//! 6. o motor, dentro do `.wasm`, só importa as funções da lista, fala a API do
//!    desktop e **passa a tabela da volta do `seele-motor`**
//!    (`crates/seele-motor/tests/casos.json`), a mesma que o nativo passa: o
//!    laço cortado, as rejeições, o balde do `console`, os temporizadores, a
//!    sonda das globais e o teto de memória, que só vale com o
//!    `RustAllocator` (`xtask/web/motor.mjs`).
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

/// O que falta nesta máquina para os passos rodarem, uma frase por item, com o
/// conserto.
///
/// Sem isto, uma máquina sem os alvos reprovava com o erro do `cargo` no meio do
/// primeiro passo, e uma sem hash fixado do wasi-sdk (Windows, Mac Intel) só no
/// quarto. As duas razões são da máquina, e não do código, e quem publica
/// precisa ler isso primeiro.
///
/// `alvos_instalados` é a saída de `rustup target list --installed`, ou `None`
/// sem `rustup`: um toolchain sem ele pode ter os alvos, e aí quem diz é o build.
fn o_que_falta(
    alvos_instalados: Option<&str>,
    tem_node: bool,
    sufixo: Option<&str>,
) -> Vec<String> {
    let mut falta = Vec::new();
    if let Some(instalados) = alvos_instalados {
        let tem = |alvo: &str| instalados.lines().any(|l| l.trim() == alvo);
        if !tem("wasm32-unknown-unknown") || !tem("wasm32-wasip1") {
            falta.push(
                "faltam os alvos wasm: rode «rustup target add wasm32-unknown-unknown wasm32-wasip1»".to_owned(),
            );
        }
    }
    if !tem_node {
        falta.push("não há node nesta máquina: os dois guardas do .wasm rodam nele".to_owned());
    }
    match sufixo {
        None => falta.push(
            "o wasi-sdk não tem pacote para esta plataforma; os crates do celular se provam no Mac arm64 \
             e no Linux x86_64"
                .to_owned(),
        ),
        Some(s) if !crate::wasi_sdk::FIXADOS.iter().any(|(f, _)| *f == s) => falta.push(format!(
            "esta máquina ({s}) não tem SHA-256 fixado do wasi-sdk, e o motor de MODs não compila com um \
             SDK que ninguém conferiu. Rode numa máquina com hash fixado (Mac arm64 ou Linux x86_64), ou \
             baixe, confira por outro meio e acrescente a linha em `FIXADOS` (xtask/src/wasi_sdk.rs)"
        )),
        Some(_) => {}
    }
    falta
}

pub(crate) fn run() -> ExitCode {
    let raiz = raiz();
    let alvos = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .filter(|s| s.status.success())
        .map(|s| String::from_utf8_lossy(&s.stdout).into_owned());
    let tem_node = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|s| s.status.success());
    let falta = o_que_falta(
        alvos.as_deref(),
        tem_node,
        crate::wasi_sdk::sufixo_desta_maquina(),
    );
    if !falta.is_empty() {
        for motivo in &falta {
            eprintln!("web: FALHOU antes de começar: {motivo}");
        }
        return ExitCode::FAILURE;
    }
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
        "seele-sessao para wasm32-unknown-unknown (o Room que o navegador dobra)",
        deu_certo(rodar(cargo().args([
            "build",
            "--release",
            "-p",
            "seele-sessao",
            "--target",
            "wasm32-unknown-unknown",
        ]))),
    ));
    passos.push((
        "seele-voz para wasm32-unknown-unknown (o portão, o jitter e o mixer da voz)",
        deu_certo(rodar(cargo().args([
            "build",
            "--release",
            "-p",
            "seele-voz",
            "--target",
            "wasm32-unknown-unknown",
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

    passos.push((
        "os links da F1 dentro do .wasm (xtask/web/links.mjs)",
        deu_certo(rodar(
            Command::new("node")
                .arg("xtask/web/links.mjs")
                .arg(&web)
                .arg("crates/seele-web/tests/links-da-f1.json"),
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
                        .arg("crates/seele-motor-web/tests/mod-comum.js")
                        .arg("crates/seele-motor/tests/casos.json")
                        .arg("crates/seele-motor/src/lib.rs"),
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

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "num teste, o pânico é o relatório")]
mod tests {
    use super::*;

    const COM_OS_DOIS: &str = "aarch64-apple-darwin\nwasm32-unknown-unknown\nwasm32-wasip1\n";

    #[test]
    fn com_tudo_no_lugar_nada_falta() {
        assert!(o_que_falta(Some(COM_OS_DOIS), true, Some("arm64-macos")).is_empty());
    }

    #[test]
    fn sem_os_alvos_wasm_a_recusa_diz_o_rustup() {
        let falta = o_que_falta(Some("aarch64-apple-darwin\n"), true, Some("arm64-macos"));
        assert_eq!(falta.len(), 1, "{falta:?}");
        assert!(
            falta
                .iter()
                .any(|f| f.contains("rustup target add wasm32-unknown-unknown wasm32-wasip1")),
            "{falta:?}"
        );
    }

    #[test]
    fn uma_maquina_sem_hash_fixado_e_recusada_dizendo_o_porque() {
        let falta = o_que_falta(Some(COM_OS_DOIS), true, Some("x86_64-macos"));
        assert_eq!(falta.len(), 1, "{falta:?}");
        assert!(
            falta[0].contains("x86_64-macos") && falta[0].contains("SHA-256"),
            "{falta:?}"
        );
    }

    #[test]
    fn uma_plataforma_sem_pacote_do_wasi_sdk_e_recusada() {
        let falta = o_que_falta(Some(COM_OS_DOIS), true, None);
        assert_eq!(falta.len(), 1, "{falta:?}");
        assert!(falta[0].contains("wasi-sdk"), "{falta:?}");
    }

    #[test]
    fn sem_node_a_recusa_diz_o_node() {
        let falta = o_que_falta(Some(COM_OS_DOIS), false, Some("arm64-macos"));
        assert_eq!(falta.len(), 1, "{falta:?}");
        assert!(falta[0].contains("node"), "{falta:?}");
    }

    #[test]
    fn sem_rustup_os_alvos_nao_sao_cobrados_daqui() {
        // Um toolchain sem `rustup` pode ter os alvos; quem diz é o build.
        assert!(o_que_falta(None, true, Some("arm64-macos")).is_empty());
    }
}
