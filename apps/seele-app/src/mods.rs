//! Serving a MOD's own files to the window, under `mod://`.
//!
//! ADR 0044. `script-src 'self'` refuses any script not compiled into the
//! binary, and loosening it to `unsafe-eval` would open the widest door in the
//! building to solve the narrowest problem. So the files stay on disk and a
//! scheme of our own serves them, with the CSP widened by exactly one scheme.
//!
//! # What replaces what the CSP stops guaranteeing
//!
//! Two guards, and neither is politeness:
//!
//! - **only what the manifest declares.** A file sitting in the directory that
//!   `mod.json` never names is not served. A MOD is what it declared it was.
//! - **nothing climbs out.** The path is rebuilt from components, so a `..`
//!   anywhere is refused rather than resolved. Without this, `mod://` would be
//!   arbitrary disk reads from a path a third party chooses.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

/// Serves one path under `mod://`, or nothing.
///
/// `url_path` is the path component of the URL, e.g.
/// `/seele/exemplo/cliente/main.js` — author, name, then the file inside the
/// MOD.
///
/// Returns `None` for anything that is not a file this MOD declared, which is
/// deliberately the same answer for "does not exist", "not declared" and
/// "tried to climb out": a scheme that distinguishes them is a scheme that
/// answers questions about the disk of whoever is running it.
#[must_use]
pub(crate) fn serve(config_dir: &Path, url_path: &str) -> Option<Vec<u8>> {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let author = parts.next()?;
    let name = parts.next()?;
    let inside: Vec<&str> = parts.collect();
    if inside.is_empty() || author.is_empty() || name.is_empty() {
        return None;
    }

    let relative = seele_ffi::mods::caminho_interno(&inside)?;

    let id = format!("{author}/{name}");
    let declared = seele_ffi::mods::ler_um(&config_dir.to_string_lossy(), &id)
        .ok()?
        .client?;
    // Only what the manifest declares. Today that is the client script; when a
    // MOD may ship more than one file, this list grows and this guard does not
    // change shape.
    if Path::new(&declared) != relative.as_path() {
        return None;
    }

    std::fs::read(
        config_dir
            .join("mods")
            .join(author)
            .join(name)
            .join(&relative),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn semear(raiz: &Path, id: &str) -> PathBuf {
        let dir = raiz.join("mods").join(id);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(
            dir.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":1,
                    "repo":"https://example.invalid/x","reach":["dom"],
                    "client":"cliente/main.js"}}"#
            ),
        )
        .expect("manifesto");
        std::fs::write(dir.join("cliente/main.js"), "globalThis.MOD_RODOU = true;")
            .expect("script");
        dir
    }

    fn temporario(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("seele-modserve-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    #[test]
    fn um_arquivo_declarado_no_manifesto_e_servido() {
        let raiz = temporario("ok");
        semear(&raiz, "seele/exemplo");
        let bytes = serve(&raiz, "/seele/exemplo/cliente/main.js").expect("devia servir");
        assert_eq!(bytes, b"globalThis.MOD_RODOU = true;");
    }

    /// O guarda que substitui o que a CSP deixa de garantir: um arquivo que o
    /// manifesto não declara não é servido, mesmo estando no diretório.
    #[test]
    fn um_arquivo_que_o_manifesto_nao_declara_nao_e_servido() {
        let raiz = temporario("nao-declarado");
        let dir = semear(&raiz, "seele/exemplo");
        std::fs::write(dir.join("cliente/extra.js"), "// clandestino").expect("extra");
        assert_eq!(serve(&raiz, "/seele/exemplo/cliente/extra.js"), None);
    }

    /// Caminho inteiro por `serve`, e **este teste não é a prova da travessia**.
    ///
    /// Vale dizer, porque a suposição contrária já custou uma vez: com a regra
    /// de `seele_proto::mods::inner_path` inteiramente quebrada, este teste
    /// continua verde — a conferência do manifesto recusa estes caminhos por
    /// outro motivo. A prova de verdade é
    /// `nenhum_caminho_com_dois_pontos_vira_caminho_interno`, lá, com os dois
    /// lados. Aqui é integração: o `serve` recusa, seja qual for a razão.
    #[test]
    fn um_caminho_que_tenta_subir_nao_e_servido() {
        let raiz = temporario("subir");
        semear(&raiz, "seele/exemplo");
        for caminho in [
            "/seele/exemplo/../../../etc/passwd",
            "/seele/exemplo/cliente/../../mod.json",
            "/../mods/seele/exemplo/cliente/main.js",
        ] {
            assert_eq!(serve(&raiz, caminho), None, "subiu com `{caminho}`");
        }
    }

    #[test]
    fn um_mod_que_nao_existe_nao_e_servido() {
        let raiz = temporario("ausente");
        assert_eq!(serve(&raiz, "/seele/fantasma/cliente/main.js"), None);
    }

    #[test]
    fn nenhum_caminho_causa_panico() {
        let raiz = temporario("panico");
        semear(&raiz, "seele/exemplo");
        for caminho in ["", "/", "//", "/a", "/a/b", "\u{0}", "/seele/exemplo/"] {
            let _ = serve(&raiz, caminho);
        }
    }
}
