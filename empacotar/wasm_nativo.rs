//! Shared by xtask and the native build: do not ship absent or stale WASM.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) fn soma(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn listar(pasta: &Path, saida: &mut Vec<PathBuf>) -> Result<(), String> {
    for entrada in std::fs::read_dir(pasta).map_err(|e| e.to_string())? {
        let entrada = entrada.map_err(|e| e.to_string())?;
        let path = entrada.path();
        if entrada.file_type().map_err(|e| e.to_string())?.is_dir() {
            listar(&path, saida)?;
        } else if path.extension().is_some_and(|e| e == "rs")
            || path.file_name().is_some_and(|n| n == "Cargo.toml")
        {
            saida.push(path);
        }
    }
    Ok(())
}

pub(crate) fn fontes(raiz: &Path) -> Result<String, String> {
    let mut caminhos = Vec::new();
    listar(&raiz.join("crates"), &mut caminhos)?;
    for nome in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "xtask/src/pacote_web.rs",
        "empacotar/wasm_nativo.rs",
        "apps/seele-app/chaves/mods.pub",
    ] {
        caminhos.push(raiz.join(nome));
    }
    caminhos.sort();
    let mut hash = Sha256::new();
    hash.update(std::env::var("SEELE_VERSAO").unwrap_or_default().as_bytes());
    hash.update([0]);
    for path in caminhos {
        let nome = path
            .strip_prefix(raiz)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        hash.update(nome.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn conferir(raiz: &Path) -> Result<(), String> {
    let app = raiz.join("apps/seele-app");
    let marca = std::fs::read_to_string(app.join("wasm-nativo.sha256"))
        .map_err(|e| format!("WASM nativo ainda não preparado: {e}"))?;
    let mut linhas = marca.lines();
    if linhas.next() != Some(fontes(raiz)?.as_str()) {
        return Err("o WASM nativo foi preparado com outras fontes".into());
    }
    let bytes = std::fs::read(app.join("ui/seele_web.wasm")).map_err(|e| e.to_string())?;
    if !bytes.starts_with(b"\0asm") || linhas.next() != Some(soma(&bytes).as_str()) {
        return Err("o WASM nativo mudou depois da conferência".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rejeita_modulo_trocado_fontes_e_chave_de_catalogo_antigas() {
        let raiz =
            std::env::temp_dir().join(format!("seele-wasm-proveniencia-{}", std::process::id()));
        let gravar = |nome: &str, bytes: &[u8]| {
            let path = raiz.join(nome);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        for nome in [
            "crates/seele-web/src/lib.rs",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "xtask/src/pacote_web.rs",
            "empacotar/wasm_nativo.rs",
            "apps/seele-app/chaves/mods.pub",
        ] {
            gravar(nome, b"original");
        }
        let wasm = b"\0asm\x01\0\0\0";
        gravar("apps/seele-app/ui/seele_web.wasm", wasm);
        assert!(conferir(&raiz).is_err());
        let marca = format!("{}\n{}\n", fontes(&raiz).unwrap(), soma(wasm));
        gravar("apps/seele-app/wasm-nativo.sha256", marca.as_bytes());
        assert!(conferir(&raiz).is_ok());
        gravar("apps/seele-app/ui/seele_web.wasm", b"\0asmoutro");
        assert!(conferir(&raiz).is_err());
        gravar("apps/seele-app/ui/seele_web.wasm", wasm);
        gravar("crates/seele-web/src/lib.rs", b"nova fonte");
        assert!(conferir(&raiz).is_err());
        gravar("crates/seele-web/src/lib.rs", b"original");
        gravar("apps/seele-app/chaves/mods.pub", b"outra chave");
        assert!(conferir(&raiz).is_err());
        std::fs::remove_dir_all(raiz).unwrap();
    }
}
