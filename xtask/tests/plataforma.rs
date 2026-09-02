//! Import de plataforma sem guarda de plataforma.
//!
//! # O defeito que este guarda existe para não deixar acontecer de novo
//!
//! `xtask/tests/empacotamento.rs` usava `std::os::unix::fs::PermissionsExt`
//! dentro de um `if`, sem `cfg` nenhum. Compila em macOS e Linux, e derruba a
//! compilação do Windows com «cannot find `unix` in `os`» — um erro que
//! **nenhuma máquina Unix vê**. Ficou assim de 2026-08-17 até 2026-08-25, e só
//! apareceu quando alguém foi compilar num Windows de verdade.
//!
//! Este teste roda em qualquer sistema e reprova ali mesmo, na máquina de quem
//! escreveu.
//!
//! # O que ele **não** pega, e é importante dizer
//!
//! Ele confere se o import está **gateado**, e não se ele está **certo**. No
//! mesmo dia, `crates/seele-audio/src/device.rs` estava escrito
//! `winreg::enums::HKEY` — dentro de um `#[cfg(windows)]` impecável — e o tipo
//! mora na raiz do crate, não em `enums`. Aquele bloco passa por aqui e falha no
//! compilador do Windows, porque **só um compilador de verdade sabe se um
//! caminho existe**.
//!
//! A conclusão vale escrita: este guarda encurta a distância até o erro, e não
//! substitui compilar nos três sistemas. O job `windows-2022` de
//! `empacotar/publicar.sh` é quem faz isso — rodando a bateria aqui e no
//! Windows antes de empacotar —, e o `seele-audio` quebrado
//! atravessou um release inteiro — o que quer dizer que ele não estava sendo
//! olhado. Ver a pendência 7.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};

/// Um caminho que só existe numa plataforma, e qual `cfg` o autoriza.
///
/// A segunda coluna é o que o `#[cfg(...)]` tem de **conter**, como texto. É
/// grosseiro de propósito: `cfg(unix)`, `cfg(not(windows))` e
/// `cfg(any(unix, target_os = "macos"))` todos contêm `unix`, e todos servem.
/// Um guarda que exigisse a forma exata reprovaria código correto, que é o jeito
/// mais rápido de alguém aprender a ignorá-lo.
const SO_NESTA_PLATAFORMA: &[(&str, &str)] = &[
    ("std::os::unix::", "unix"),
    ("std::os::windows::", "windows"),
    ("winreg", "windows"),
    ("windows_sys", "windows"),
    ("windows_capture", "windows"),
    ("core_graphics", "macos"),
    ("screencapturekit", "macos"),
    ("objc2", "macos"),
];

/// Arquivos cujo **nome** já é a guarda.
///
/// `captura/windows.rs` e `captura/macos.rs` são declarados com `#[cfg]` no
/// `mod` que os traz, então tudo dentro deles já está gateado e exigir `cfg` em
/// cada `use` seria ruído. A lista é por nome de arquivo e não por caminho para
/// que um módulo novo com a mesma convenção entre coberto sozinho.
const NOME_JA_E_GUARDA: &[&str] = &["windows", "macos", "linux", "unix"];

/// Quantas linhas acima do `use` o `cfg` pode estar.
///
/// Dez porque o padrão real deste repositório é
/// `#[cfg(unix)]` + `#[test]` + assinatura + comentário + `use`, e o comentário
/// às vezes tem quatro linhas. Mais que isso e o `cfg` provavelmente é de outra
/// coisa.
const ALCANCE: usize = 10;

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("o xtask mora dentro do repositório")
        .to_path_buf()
}

fn arquivos_rust(pasta: &Path, achados: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        let nome = entrada.file_name();
        let nome = nome.to_string_lossy();
        // `target` é artefato e `.claude` são cópias do repositório: varrer os
        // dois faria este teste acusar código que já foi consertado aqui.
        if nome == "target" || nome == "target-linux" || nome.starts_with('.') {
            continue;
        }
        if caminho.is_dir() {
            arquivos_rust(&caminho, achados);
        } else if caminho.extension().is_some_and(|e| e == "rs") {
            achados.push(caminho);
        }
    }
}

#[test]
fn todo_import_de_plataforma_esta_dentro_de_um_cfg() {
    let raiz = raiz();
    let mut arquivos = Vec::new();
    for pasta in ["crates", "apps", "xtask"] {
        arquivos_rust(&raiz.join(pasta), &mut arquivos);
    }
    assert!(
        arquivos.len() > 50,
        "a varredura achou {} arquivos, o que é pouco demais para este \
         repositório — provavelmente o caminho está errado e este teste está \
         passando por não olhar nada",
        arquivos.len()
    );

    let mut sem_guarda: Vec<String> = Vec::new();

    for arquivo in &arquivos {
        let nome = arquivo
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if NOME_JA_E_GUARDA.contains(&nome.as_str()) {
            continue;
        }
        let Ok(corpo) = std::fs::read_to_string(arquivo) else {
            continue;
        };
        let linhas: Vec<&str> = corpo.lines().collect();

        for (numero, linha) in linhas.iter().enumerate() {
            let cortada = linha.trim_start();
            if !cortada.starts_with("use ") {
                continue;
            }
            let Some((caminho, plataforma)) = SO_NESTA_PLATAFORMA
                .iter()
                .find(|(caminho, _)| cortada.contains(caminho))
            else {
                continue;
            };

            let inicio = numero.saturating_sub(ALCANCE);
            let coberto = linhas
                .get(inicio..numero)
                .unwrap_or_default()
                .iter()
                .any(|acima| acima.contains("#[cfg(") && acima.contains(plataforma));

            if !coberto {
                let relativo = arquivo.strip_prefix(&raiz).unwrap_or(arquivo);
                sem_guarda.push(format!(
                    "{}:{}: `{}` sem `#[cfg(...{}...)]` nas {} linhas acima\n      {}",
                    relativo.display(),
                    numero + 1,
                    caminho,
                    plataforma,
                    ALCANCE,
                    cortada
                ));
            }
        }
    }

    assert!(
        sem_guarda.is_empty(),
        "import que só existe numa plataforma, sem guarda de plataforma — isto \
         compila aqui e derruba a compilação dos outros sistemas:\n\n{}\n\n\
         Ponha o `#[cfg(...)]` na função que o contém, ou mova o código para um \
         arquivo com nome de plataforma.",
        sem_guarda.join("\n")
    );
}
