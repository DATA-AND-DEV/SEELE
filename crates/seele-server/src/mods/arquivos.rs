//! A MOD's own folder, and nothing outside it.
//!
//! ADR 0044. A MOD reaches the network and everything the server can do — the
//! product's rules do not reach it. **The disk is the one exception**, and the
//! reason is not taste: `identity.key`, `pins`, `conhecidos` and the database
//! with every conversation sit beside `mods/`, and handing those over is not
//! "the rules do not apply to MODs" — it is the ADR 0004 and the 0017 falling,
//! which are what guarantee a person is themself.
//!
//! Network and disk are not the same degree, and that is what makes this line
//! drawable: **network lets a MOD send out what it already sees; disk decides
//! what it sees.** On its own, the network never reaches `identity.key`.
//!
//! # How the scope is held
//!
//! Every path goes through [`seele_proto::mods::inner_path`] — the same rule the
//! window's `mod://` handler uses, and not a second copy of it. A `..` anywhere
//! is refused rather than resolved, because resolving is where a path that
//! looks contained stops being contained.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

/// The most one file a MOD writes may hold, in bytes.
///
/// 4 MiB: enough for the bulk data the folder exists for — a MOD with a
/// thousand small images — and bounded so that one MOD cannot fill the disk of
/// whoever hosts it in a single call.
pub const TETO_DE_ARQUIVO: usize = 4 * 1024 * 1024;

/// Resolves a MOD-supplied path against its own folder, or nothing.
///
/// The whole guard, and it is one line of logic on purpose: everything else in
/// this module is `std::fs` doing what it is told.
#[must_use]
pub fn dentro(pasta: &Path, caminho: &str) -> Option<PathBuf> {
    // Um caminho absoluto é **recusado**, e não contido.
    //
    // Contê-lo seria seguro — `/etc/passwd` viraria `<pasta>/etc/passwd`, que
    // não sai de lugar nenhum — e foi o que a primeira versão fazia, por
    // acidente e não por escolha. Recusar é melhor pela razão que o `CLAUDE.md`
    // deste repositório nomeia: quem escreveu `/etc/passwd` quis o arquivo do
    // sistema, e um produto que escreve calado noutro lugar sabe e não conta.
    if caminho.starts_with('/') || caminho.starts_with('\\') {
        return None;
    }
    let partes: Vec<&str> = caminho.split('/').filter(|p| !p.is_empty()).collect();
    if partes.is_empty() {
        return None;
    }
    let relativo = seele_proto::mods::inner_path(&partes)?;
    Some(pasta.join(relativo))
}

/// Reads one file from a MOD's folder.
#[must_use]
pub fn ler(pasta: &Path, caminho: &str) -> Option<String> {
    std::fs::read_to_string(dentro(pasta, caminho)?).ok()
}

/// Writes one file into a MOD's folder, creating the folders it needs.
///
/// Returns whether it landed. `false` for a path that climbs out, for content
/// past [`TETO_DE_ARQUIVO`], and for a filesystem that refused — deliberately
/// the same answer, because a MOD that could tell them apart could ask
/// questions about the disk of whoever runs it.
#[must_use]
pub fn escrever(pasta: &Path, caminho: &str, conteudo: &str) -> bool {
    if conteudo.len() > TETO_DE_ARQUIVO {
        return false;
    }
    let Some(alvo) = dentro(pasta, caminho) else {
        return false;
    };
    if let Some(pai) = alvo.parent() {
        if std::fs::create_dir_all(pai).is_err() {
            return false;
        }
    }
    std::fs::write(alvo, conteudo).is_ok()
}

/// Every file in a MOD's folder, by path relative to it, sorted.
#[must_use]
pub fn listar(pasta: &Path) -> Vec<String> {
    let mut achados = Vec::new();
    colher(pasta, pasta, &mut achados);
    achados.sort();
    achados
}

/// Removes one file from a MOD's folder. Returns whether it is gone.
#[must_use]
pub fn apagar(pasta: &Path, caminho: &str) -> bool {
    let Some(alvo) = dentro(pasta, caminho) else {
        return false;
    };
    std::fs::remove_file(alvo).is_ok()
}

fn colher(raiz: &Path, dir: &Path, destino: &mut Vec<String>) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            colher(raiz, &caminho, destino);
        } else if let Ok(relativo) = caminho.strip_prefix(raiz) {
            destino.push(relativo.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seele-quintal-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    #[test]
    fn um_mod_escreve_e_le_na_pasta_dele() {
        let p = pasta("ida-e-volta");
        assert!(escrever(&p, "ficha.json", "{\"forca\":18}"));
        assert_eq!(ler(&p, "ficha.json").as_deref(), Some("{\"forca\":18}"));
    }

    #[test]
    fn subpastas_sao_criadas() {
        let p = pasta("subpastas");
        assert!(escrever(&p, "fichas/coelho.json", "{}"));
        assert_eq!(listar(&p), vec!["fichas/coelho.json".to_owned()]);
    }

    /// **O guarda inteiro desta página.** Sem ele, um MOD lê a chave privada de
    /// identidade de quem hospeda, que fica dois níveis acima da pasta dele.
    #[test]
    fn nenhum_caminho_sai_da_pasta_do_mod() {
        let p = pasta("fuga");
        for tentativa in [
            "../../../identity.key",
            "../../identity.key",
            "..",
            "fichas/../../../pins",
            "/etc/passwd",
            "",
        ] {
            assert_eq!(dentro(&p, tentativa), None, "`{tentativa}` escapou");
            assert!(!escrever(&p, tentativa, "x"), "`{tentativa}` escreveu fora");
            assert_eq!(ler(&p, tentativa), None, "`{tentativa}` leu fora");
            assert!(!apagar(&p, tentativa), "`{tentativa}` apagou fora");
        }
    }

    /// A prova de que o guarda acima não passa por acidente: o vizinho existe,
    /// tem conteúdo, e mesmo assim não é alcançado.
    #[test]
    fn o_arquivo_do_vizinho_existe_e_continua_fora_de_alcance() {
        let raiz = pasta("vizinho");
        let minha = raiz.join("meu-mod");
        std::fs::create_dir_all(&minha).expect("minha pasta");
        std::fs::write(raiz.join("identity.key"), "SEGREDO").expect("chave de mentira");

        assert_eq!(ler(&minha, "../identity.key"), None);
        assert!(!listar(&minha).iter().any(|f| f.contains("identity")));
        // E o arquivo continua lá, intacto.
        assert_eq!(
            std::fs::read_to_string(raiz.join("identity.key")).expect("ler"),
            "SEGREDO"
        );
    }

    #[test]
    fn um_arquivo_grande_demais_e_recusado() {
        let p = pasta("grande");
        let enorme = "x".repeat(TETO_DE_ARQUIVO + 1);
        assert!(!escrever(&p, "grande.bin", &enorme));
        assert_eq!(ler(&p, "grande.bin"), None, "o arquivo entrou pela metade");
    }

    #[test]
    fn apagar_tira_o_arquivo_da_listagem() {
        let p = pasta("apagar");
        assert!(escrever(&p, "some.txt", "aqui"));
        assert!(apagar(&p, "some.txt"));
        assert!(listar(&p).is_empty());
    }
}
