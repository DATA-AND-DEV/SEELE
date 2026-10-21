//! O wasi-sdk do motor de MODs do celular, baixado e conferido.
//!
//! O motor (o QuickJS do `rquickjs`) é C, e no `wasm32-wasip1` o `build.rs` do
//! `rquickjs-sys` 0.12.2 baixa o wasi-sdk com `curl` e o descompacta **sem
//! conferir nada** (`rquickjs-sys-0.12.2/build.rs:13-85`). O `cargo deny` não vê
//! o que um `build.rs` baixa. Com a variável `WASI_SDK` apontando para um SDK
//! já instalado, ele não baixa (`:88-91`). Este módulo é quem baixa, confere e
//! dá o caminho.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

/// A versão que o `rquickjs-sys` 0.12.2 espera (`build.rs:10-11`).
const VERSAO: &str = "24.0";
/// A etiqueta do release em `github.com/WebAssembly/wasi-sdk`.
const ETIQUETA: &str = "wasi-sdk-24";

/// Os SHA-256 fixados, por plataforma.
///
/// **O wasi-sdk não publica checksum**: o release 24 não tem arquivo de somas,
/// e a API do GitHub não dá o `digest` dos arquivos dele. Estes foram
/// calculados na primeira descarga, em 08/10/2026, e o tamanho de cada um bate
/// com o que a API informa (108 199 177 e 118 879 731 bytes). Eles não provam
/// que a primeira descarga era a boa; provam que, daqui em diante, um arquivo
/// trocado é recusado. Uma plataforma sem linha aqui falha, em vez de baixar
/// sem conferir.
pub(crate) const FIXADOS: &[(&str, &str)] = &[
    (
        "arm64-macos",
        "aeae999396d5f5caa5ce419f52e83c35869d5fd21d40af80acba2c80f51b0b3a",
    ),
    (
        "x86_64-linux",
        "c6c38aab56e5de88adf6c1ebc9c3ae8da72f88ec2b656fb024eda8d4167a0bc5",
    ),
];

/// O sufixo do arquivo desta máquina, com o mapa do `build.rs` do
/// `rquickjs-sys` (`:28-36`).
pub(crate) fn sufixo_desta_maquina() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-linux"),
        ("linux", "aarch64") => Some("arm64-linux"),
        ("macos", "x86_64") => Some("x86_64-macos"),
        ("macos", "aarch64") => Some("arm64-macos"),
        ("windows", "x86_64") => Some("x86_64-windows"),
        _ => None,
    }
}

/// `cargo xtask wasi-sdk`: baixa, confere e descompacta, e imprime o caminho
/// para `WASI_SDK`.
///
/// ```text
/// WASI_SDK="$(cargo xtask wasi-sdk)" cargo build --release --target wasm32-wasip1 …
/// ```
pub(crate) fn run() -> ExitCode {
    match caminho() {
        Ok(pasta) => {
            println!("{}", pasta.display());
            ExitCode::SUCCESS
        }
        Err(erro) => {
            eprintln!("wasi-sdk: {erro}");
            ExitCode::FAILURE
        }
    }
}

/// A pasta do wasi-sdk conferido desta máquina, baixando se preciso.
///
/// # Errors
///
/// A plataforma sem pacote ou sem hash fixado, a descarga, a conferência ou a
/// descompactação.
pub(crate) fn caminho() -> Result<PathBuf, String> {
    sufixo_desta_maquina()
        .ok_or_else(|| "o wasi-sdk não tem pacote para esta plataforma".to_owned())
        .and_then(|sufixo| {
            let esperado = FIXADOS
                .iter()
                .find(|(s, _)| *s == sufixo)
                .map(|(_, h)| *h)
                .ok_or_else(|| {
                    format!(
                        "nenhum SHA-256 fixado para {sufixo}: baixe, confira por outro meio \
                         e acrescente a linha em FIXADOS"
                    )
                })?;
            preparar_com(&destino(), sufixo, esperado, &|para| baixar(sufixo, para))
        })
}

/// A pasta do wasi-sdk **já conferido**, sem baixar nunca: sem ele no
/// `target/`, para e diz o que fazer. É a do `cargo xtask pacote-web`, que
/// monta o que vai para a origem e não pode trazer nada de fora sem aprovação.
///
/// # Errors
///
/// A plataforma sem pacote ou sem hash fixado, e o SDK que não está aqui ou
/// não confere.
pub(crate) fn caminho_sem_baixar() -> Result<PathBuf, String> {
    let sufixo = sufixo_desta_maquina()
        .ok_or_else(|| "o wasi-sdk não tem pacote para esta plataforma".to_owned())?;
    let esperado = FIXADOS
        .iter()
        .find(|(s, _)| *s == sufixo)
        .map(|(_, h)| *h)
        .ok_or_else(|| format!("nenhum SHA-256 fixado para {sufixo}"))?;
    preparar_com(&destino(), sufixo, esperado, &|_| {
        Err(format!(
            "o wasi-sdk conferido não está em {} (ou não confere), e este passo não baixa: \
             copie o conferido de outro `target/` (`cp -c -R`) ou rode `cargo xtask wasi-sdk` \
             com a descarga aprovada",
            destino().display()
        ))
    })
}

/// `target/wasi-sdk`, ou o mesmo sob `CARGO_TARGET_DIR`.
fn destino() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"),
            PathBuf::from,
        )
        .join("wasi-sdk")
}

fn baixar(sufixo: &str, para: &Path) -> Result<(), String> {
    let url = format!(
        "https://github.com/WebAssembly/wasi-sdk/releases/download/{ETIQUETA}/wasi-sdk-{VERSAO}-{sufixo}.tar.gz"
    );
    let status = Command::new("curl")
        .args(["-fSL", "--retry", "2", "-o"])
        .arg(para)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("curl {url}: {status}"))
    }
}

/// Baixa (se o arquivo guardado não confere), **confere antes de tudo** e só
/// então descompacta. Devolve a pasta do SDK.
///
/// Uma pasta já descompactada é reaproveitada se a marca dela tem o mesmo
/// SHA-256 fixado.
fn preparar_com(
    destino: &Path,
    sufixo: &str,
    esperado: &str,
    baixar: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let nome = format!("wasi-sdk-{VERSAO}-{sufixo}");
    let pasta = destino.join(&nome);
    let marca = pasta.join(".conferido");
    if std::fs::read_to_string(&marca).is_ok_and(|m| m.trim() == esperado) {
        return Ok(pasta);
    }
    std::fs::create_dir_all(destino).map_err(|e| format!("{}: {e}", destino.display()))?;
    let arquivo = destino.join(format!("{nome}.tar.gz"));
    if conferir(&arquivo, esperado).is_err() {
        let parcial = destino.join(format!("{nome}.tar.gz.parcial"));
        baixar(&parcial)?;
        conferir(&parcial, esperado)?;
        std::fs::rename(&parcial, &arquivo).map_err(|e| format!("{}: {e}", arquivo.display()))?;
    }
    let _ = std::fs::remove_dir_all(&pasta);
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&arquivo)
        .arg("-C")
        .arg(destino)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !status.success() || !pasta.is_dir() {
        return Err(format!(
            "{} não descompactou em {}",
            arquivo.display(),
            pasta.display()
        ));
    }
    std::fs::write(&marca, esperado).map_err(|e| format!("{}: {e}", marca.display()))?;
    Ok(pasta)
}

/// Confere o SHA-256 de um arquivo contra o fixado, em hex minúsculo.
///
/// # Errors
///
/// Se o arquivo não abrir ou o hash não bater, com os dois hashes no texto.
pub(crate) fn conferir(arquivo: &Path, esperado: &str) -> Result<(), String> {
    let mut entrada =
        std::fs::File::open(arquivo).map_err(|e| format!("{}: {e}", arquivo.display()))?;
    let mut hash = Sha256::new();
    let mut bloco = vec![0_u8; 1 << 16];
    loop {
        let n = entrada
            .read(&mut bloco)
            .map_err(|e| format!("{}: {e}", arquivo.display()))?;
        if n == 0 {
            break;
        }
        hash.update(bloco.get(..n).unwrap_or_default());
    }
    let visto: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if visto == esperado {
        Ok(())
    } else {
        Err(format!(
            "{}: o SHA-256 é {visto}, e o fixado é {esperado}",
            arquivo.display()
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]
mod tests {
    use super::*;

    /// O SHA-256 de `abc`, o vetor da FIPS 180-2, calculado fora daqui.
    const SHA256_DE_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn arquivo_com(nome: &str, conteudo: &[u8]) -> std::path::PathBuf {
        let caminho =
            std::env::temp_dir().join(format!("xtask-wasi-sdk-{}-{nome}", std::process::id()));
        std::fs::write(&caminho, conteudo).expect("escrever");
        caminho
    }

    #[test]
    fn o_arquivo_com_o_hash_fixado_passa() {
        let a = arquivo_com("certo", b"abc");
        assert!(conferir(&a, SHA256_DE_ABC).is_ok());
    }

    /// O erro que este módulo existe para impedir: descompactar o que veio
    /// antes de conferir. Uma descarga que não bate não vira pasta.
    #[test]
    fn uma_descarga_trocada_nao_e_descompactada() {
        let destino =
            std::env::temp_dir().join(format!("xtask-wasi-sdk-destino-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&destino);
        let baixar =
            |para: &std::path::Path| std::fs::write(para, b"abd").map_err(|e| e.to_string());
        let r = preparar_com(&destino, "teste", SHA256_DE_ABC, &baixar);
        assert!(r.is_err(), "uma descarga com outro SHA-256 passou: {r:?}");
        assert!(
            !destino.join("wasi-sdk-24.0-teste").exists(),
            "o que não bateu foi descompactado"
        );
        assert!(
            !destino.join("wasi-sdk-24.0-teste.tar.gz").exists(),
            "o que não bateu ficou guardado como bom"
        );
    }

    #[test]
    fn um_byte_trocado_e_recusado() {
        let a = arquivo_com("trocado", b"abd");
        assert!(conferir(&a, SHA256_DE_ABC).is_err());
    }
}
