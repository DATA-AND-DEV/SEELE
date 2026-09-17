//! MODs installed on this machine.
//!
//! ADR 0045. This module does the I/O and nothing else: what a manifest *is*,
//! and what makes one invalid, lives in `seele_proto::mods`. Splitting them is
//! what lets the server share the validation without depending on this crate,
//! which `xtask/src/check_deps.rs` forbids.
//!
//! # Layout on disk
//!
//! Under the ADR 0017 directory, next to `identity.key`, `pins`, `conhecidos`
//! and `preferences`:
//!
//! ```text
//! mods/<author>/<name>/mod.json
//! mods/<author>/<name>/cliente/main.js
//! ```
//!
//! # A refused MOD stays in the list
//!
//! It would be less code to skip one that fails validation. It would also be
//! the failure this repository pays for most: whoever put the directory there
//! gets no answer, and asks days later with nothing to go on.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

pub use seele_proto::mods::content_hash;
use seele_proto::mods::read_manifest;

// Republicados de propósito, e não por conveniência. `xtask/src/check_deps.rs`
// impede `seele-ffi` — e portanto a casca — de nomear `seele-proto`: «reaching
// past it would put protocol knowledge in a Tauri command». Quem consome MOD
// precisa destes dois tipos, então esta camada, que é a fronteira, os
// republica. É a mesma disciplina, não uma brecha nela.
pub use seele_proto::mods::{inner_path, Manifest, ModAnunciado, Refused};

/// A MOD that read cleanly.
#[derive(Debug, Clone)]
pub struct Installed {
    /// What its `mod.json` declares.
    pub manifest: Manifest,
    /// The identity of its bytes, from [`seele_proto::mods::content_hash`].
    pub hash: [u8; 32],
    /// Where it lives, so the `mod://` handler can serve out of it.
    pub dir: PathBuf,
}

/// One entry of the installed list.
///
/// `Installed` is boxed because it carries a `Manifest` and a path while the
/// refusal is two small fields, and clippy's `large_enum_variant` is right
/// about the difference.
#[derive(Debug, Clone)]
pub enum Found {
    /// It read cleanly.
    Ok(Box<Installed>),
    /// It did not, and this is what to tell the person.
    Refused {
        /// The directory it was found under, which is the only name we have
        /// when the manifest itself is unreadable.
        id: String,
        /// Why.
        why: Refused,
    },
}

/// Every MOD under `<config_dir>/mods`, valid or not.
///
/// A missing `mods/` directory is an empty list and not an error: it is the
/// state of every installation that has never had a MOD.
#[must_use]
pub fn list(config_dir: &Path) -> Vec<Found> {
    let root = config_dir.join("mods");
    let mut found = Vec::new();

    let Ok(authors) = std::fs::read_dir(&root) else {
        return found;
    };
    for author in authors.flatten() {
        let Ok(names) = std::fs::read_dir(author.path()) else {
            continue;
        };
        for name in names.flatten() {
            let dir = name.path();
            let Some(id) = identifier_of(&root, &dir) else {
                continue;
            };
            match read_one(&dir) {
                Ok(installed) if installed.manifest.id == id => {
                    found.push(Found::Ok(Box::new(installed)));
                }
                // A manifest whose `id` disagrees with where it sits would let
                // two MODs claim one directory under `mod://`.
                Ok(_) => found.push(Found::Refused {
                    id,
                    why: Refused::MalformedId,
                }),
                Err(why) => found.push(Found::Refused { id, why }),
            }
        }
    }
    found.sort_by(|left, right| name_of(left).cmp(name_of(right)));
    found
}

/// Reads one MOD directory.
///
/// # Errors
///
/// Returns [`Refused`] when the manifest is missing, unreadable, or invalid.
pub fn read_one(dir: &Path) -> Result<Installed, Refused> {
    // An unreadable manifest is reported at the position a reader would stop
    // at, which for "there is no file" is the beginning.
    let text = std::fs::read_to_string(dir.join("mod.json"))
        .map_err(|_| Refused::Malformed { line: 1, column: 1 })?;
    let manifest = read_manifest(&text)?;

    let mut files = collect(dir, dir);
    let hash = content_hash(&mut files);

    Ok(Installed {
        manifest,
        hash,
        dir: dir.to_path_buf(),
    })
}

/// Every file under `dir`, with paths relative to `root`, in whatever order the
/// filesystem gives them — [`content_hash`] sorts.
fn collect(root: &Path, dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Runtime data is not part of the immutable, consented package.
        if dir == root && entry.file_name() == "dados" {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
            continue;
        }
        if path.is_dir() {
            files.extend(collect(root, &path));
        } else if let (Ok(relative), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path)) {
            files.push((relative.to_string_lossy().replace('\\', "/"), bytes));
        }
    }
    files
}

/// `author/name` from the path, or `None` if it is not two levels under `root`.
fn identifier_of(root: &Path, dir: &Path) -> Option<String> {
    let relative = dir.strip_prefix(root).ok()?;
    let mut parts = relative.components();
    let author = parts.next()?.as_os_str().to_str()?;
    let name = parts.next()?.as_os_str().to_str()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{author}/{name}"))
}

/// The name of a refusal, from a closed list.
///
/// Exists here and not in the shell for the reason above: the shell cannot name
/// `Refused`'s variants. ADR 0012 keeps the wording in the shell — this is an
/// identifier, not a sentence, and `ui/frases.js` turns it into one.
#[must_use]
pub fn refusal_name(why: &Refused) -> &'static str {
    match why {
        Refused::Malformed { .. } => "malformed",
        Refused::SchemaTooNew { .. } => "schema-too-new",
        Refused::ApiTooNew { .. } => "api-too-new",
        Refused::MalformedId => "malformed-id",
        Refused::Empty => "empty",
    }
}

/// Lowercase hex of a content hash.
///
/// Here rather than in the shell because it is what a person compares by eye
/// against what the indexer publishes, and two spellings of the same hash would
/// make that comparison fail for no reason.
#[must_use]
pub fn hex(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    hash.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// The identifier of an entry, however it turned out.
fn name_of(found: &Found) -> &str {
    match found {
        Found::Ok(installed) => &installed.manifest.id,
        Found::Refused { id, .. } => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um MOD de mentira em disco, sob um diretório temporário do teste.
    fn semear(raiz: &Path, id: &str, manifesto: &str) -> PathBuf {
        let dir = raiz.join("mods").join(id);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(dir.join("mod.json"), manifesto).expect("escrever manifesto");
        std::fs::write(dir.join("cliente/main.js"), "/* nada */").expect("escrever script");
        dir
    }

    fn manifesto(id: &str) -> String {
        format!(
            r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":1,
                "repo":"https://example.invalid/x","reach":["dom"],
                "client":"cliente/main.js"}}"#
        )
    }

    fn temporario(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seele-mods-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criar temporário");
        dir
    }

    #[test]
    fn um_mod_em_disco_e_lido_com_hash() {
        let raiz = temporario("um");
        semear(&raiz, "seele/exemplo", &manifesto("seele/exemplo"));

        let achados = list(&raiz);
        assert_eq!(achados.len(), 1);
        let Found::Ok(instalado) = &achados[0] else {
            panic!("MOD válido veio recusado");
        };
        assert_eq!(instalado.manifest.id, "seele/exemplo");
        assert_ne!(instalado.hash, [0u8; 32]);
    }

    /// O defeito mais caro deste repositório, segundo o CLAUDE.md, é «o produto
    /// sabe e não conta». Um MOD que não passa na validação não pode
    /// desaparecer da lista: quem o pôs ali precisa ver por quê.
    #[test]
    fn um_mod_invalido_aparece_recusado_em_vez_de_sumir() {
        let raiz = temporario("invalido");
        semear(&raiz, "seele/quebrado", "{ isto não é json");
        semear(&raiz, "seele/bom", &manifesto("seele/bom"));

        let achados = list(&raiz);
        assert_eq!(achados.len(), 2, "um dos dois sumiu da lista");
        assert!(achados.iter().any(|f| matches!(f, Found::Refused { .. })));
        assert!(achados.iter().any(|f| matches!(f, Found::Ok(_))));
    }

    /// Um `id` que não corresponde ao lugar onde o MOD está em disco deixaria
    /// dois MODs disputarem o mesmo diretório no `mod://`.
    #[test]
    fn um_id_que_nao_bate_com_o_diretorio_e_recusado() {
        let raiz = temporario("mentiroso");
        semear(&raiz, "seele/pasta", &manifesto("outro/nome"));

        let achados = list(&raiz);
        assert!(matches!(
            &achados[0],
            Found::Refused {
                why: Refused::MalformedId,
                ..
            }
        ));
    }

    #[test]
    fn um_diretorio_de_mods_que_nao_existe_da_lista_vazia() {
        let raiz = temporario("vazio");
        assert!(list(&raiz).is_empty());
    }
}
