//! What a MOD declares about itself, and what the product refuses.
//!
//! ADR 0044. A MOD is a directory with a `mod.json`, an optional client half
//! and an optional server half. This module is the manifest and nothing else:
//! reading disk belongs to `seele-core`, storing state belongs to
//! `seele-server`.
//!
//! # Why it lives here and not in `seele-core`
//!
//! `xtask/src/check_deps.rs` forbids `seele-server` from depending on
//! `seele-core`, and both ends need the same type. `seele-proto` is the only
//! crate both are allowed to reach, and it already holds two non-wire parsers
//! for the same reason — `uri` and `attachment`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Version of the `mod.json` format this build reads.
///
/// An integer and not semver, for the reason ADR 0029 gave and 0044 keeps:
/// semver invites an argument about what is compatible, and here there is none
/// — the schema only grows.
pub const MANIFEST_SCHEMA: u32 = 1;

/// Version of the MOD API this build offers.
///
/// Separate from [`MANIFEST_SCHEMA`] because they move for different reasons:
/// the schema changes when the *manifest* gains a field, the API when what a
/// MOD can *call* changes. ADR 0044 freezes each API version in its own file,
/// never edited once shipped.
pub const MOD_API_VERSION: u32 = 1;

/// What a MOD declares about itself.
///
/// `deny_unknown_fields` is the decision, not the default: a misspelled key
/// installs a MOD missing the thing its author thought was there, and the
/// author never finds out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Manifest format this file is written against.
    pub schema: u32,
    /// `author/name`. The identity a server announces; the hash is what it
    /// proves.
    pub id: String,
    /// The author's own version string. Never parsed by us.
    pub version: String,
    /// MOD API version this MOD was written against.
    pub api: u32,
    /// The public repository. Required — ADR 0044 makes it a condition of
    /// publication, and this is where it is stated.
    pub repo: String,
    /// What this MOD asks to reach, for the acceptance screen to show before
    /// anything is downloaded.
    ///
    /// **Read and validated, and nothing consumes it yet.** The acceptance
    /// screen is a later plan, and the runtime that would enforce it is later
    /// still. It is in schema 1 anyway for the reason ADR 0029 gave about
    /// `ansi256`: a manifest written today has to still parse on the day the
    /// consumer arrives, and `deny_unknown_fields` would refuse it otherwise.
    #[serde(default)]
    pub reach: Vec<String>,
    /// Version of this MOD's own data schema, for its own migrations.
    ///
    /// Read and not consumed, for the same reason as [`Self::reach`].
    #[serde(default)]
    pub state: Option<u32>,
    /// Path, relative to the MOD directory, of the script the window loads.
    #[serde(default)]
    pub client: Option<String>,
    /// Path, relative to the MOD directory, of the script the server runs.
    /// Read but not executed until the runtime lands.
    #[serde(default)]
    pub server: Option<String>,
}

/// Why a manifest was refused.
///
/// One variant per refusal, with the numbers inside. `specs/02-protocolo.md`:
/// "no free-form string reaches the interface — the shell decides how to
/// present each variant". The `Display` text is for `tracing`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refused {
    /// Not JSON, or a key this build does not know.
    ///
    /// Carries the position rather than `serde_json`'s message, so no
    /// free-form string crosses into a shell.
    #[error("manifest is malformed at line {line}, column {column}")]
    Malformed {
        /// 1-based line where parsing stopped.
        line: usize,
        /// 1-based column where parsing stopped.
        column: usize,
    },

    /// The manifest is written against a newer format than this build reads.
    #[error("manifest schema {found}, this build reads {ours}")]
    SchemaTooNew {
        /// Schema the file declares.
        found: u32,
        /// Schema this build reads.
        ours: u32,
    },

    /// The MOD targets a newer API than this build offers.
    #[error("mod targets API {wanted}, this build offers {ours}")]
    ApiTooNew {
        /// API the MOD asks for.
        wanted: u32,
        /// API this build offers.
        ours: u32,
    },

    /// The identifier is not `author/name`.
    #[error("mod id is not `author/name`")]
    MalformedId,

    /// Neither half is declared, so the MOD does nothing.
    #[error("mod declares neither a client nor a server half")]
    Empty,
}

/// Reads a `mod.json`.
///
/// # Errors
///
/// Returns [`Refused`] for malformed JSON, an unknown key, a schema or API
/// this build cannot serve, a malformed identifier, or a MOD with no halves.
pub fn read_manifest(text: &str) -> Result<Manifest, Refused> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|error| Refused::Malformed {
        line: error.line(),
        column: error.column(),
    })?;

    if manifest.schema > MANIFEST_SCHEMA {
        return Err(Refused::SchemaTooNew {
            found: manifest.schema,
            ours: MANIFEST_SCHEMA,
        });
    }
    if manifest.api > MOD_API_VERSION {
        return Err(Refused::ApiTooNew {
            wanted: manifest.api,
            ours: MOD_API_VERSION,
        });
    }
    if !is_well_formed_id(&manifest.id) {
        return Err(Refused::MalformedId);
    }
    if manifest.client.is_none() && manifest.server.is_none() {
        return Err(Refused::Empty);
    }
    Ok(manifest)
}

/// `author/name`, both halves non-empty, and nothing that could climb out of a
/// directory.
fn is_well_formed_id(id: &str) -> bool {
    let mut halves = id.split('/');
    let (Some(author), Some(name), None) = (halves.next(), halves.next(), halves.next()) else {
        return false;
    };
    [author, name].iter().all(|half| {
        !half.is_empty()
            && half
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

/// The identity of a MOD's bytes.
///
/// A server announces `author/name` and a version; this is what proves the
/// bytes are the ones that were reviewed. ADR 0026, alternative 5, is the
/// reason it exists at all: "TLS says which server the file came from, not who
/// produced it".
///
/// Deterministic across machines, which is the whole requirement:
///
/// - **paths are sorted**, because directory order is not a promise any
///   filesystem makes;
/// - **every length is fed in before its bytes**, so `("ab", "c")` and
///   `("a", "bc")` cannot collide;
/// - lengths go in as fixed 8-byte big-endian, so a length is never itself
///   ambiguous.
///
/// Takes `&mut` so the sort happens in place and no caller has to remember to
/// sort first — a caller that forgot would produce a hash that is right on one
/// machine and wrong on the next.
#[must_use]
pub fn content_hash(files: &mut [(String, Vec<u8>)]) -> [u8; 32] {
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    hasher.update((files.len() as u64).to_be_bytes());
    for (path, bytes) in files.iter() {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifesto_minimo() -> String {
        format!(
            r#"{{
                "schema": {MANIFEST_SCHEMA},
                "id": "seele/exemplo",
                "version": "1.0.0",
                "api": {MOD_API_VERSION},
                "repo": "https://github.com/seele/exemplo",
                "reach": ["dom"],
                "state": 1,
                "client": "cliente/main.js"
            }}"#
        )
    }

    #[test]
    fn um_manifesto_completo_e_lido() {
        let lido = read_manifest(&manifesto_minimo()).expect("manifesto válido recusado");
        assert_eq!(lido.id, "seele/exemplo");
        assert_eq!(lido.client.as_deref(), Some("cliente/main.js"));
        assert_eq!(lido.server, None);
    }

    /// Chave desconhecida é recusada, e o motivo é do ADR 0029 e sobrevive no 0044.
    ///
    /// `vesion` com um `s` a menos instalaria calado um MOD sem versão, e o
    /// autor nunca ficaria sabendo. Recusar é a única forma de retorno que um
    /// autor de MOD tem.
    #[test]
    fn uma_chave_desconhecida_e_recusada() {
        let texto = manifesto_minimo().replace("\"version\"", "\"vesion\"");
        assert!(matches!(
            read_manifest(&texto),
            Err(Refused::Malformed { .. })
        ));
    }

    #[test]
    fn um_esquema_do_futuro_e_recusado_com_os_dois_numeros() {
        let texto = manifesto_minimo().replace(
            &format!("\"schema\": {MANIFEST_SCHEMA}"),
            &format!("\"schema\": {}", MANIFEST_SCHEMA + 1),
        );
        assert_eq!(
            read_manifest(&texto),
            Err(Refused::SchemaTooNew {
                found: MANIFEST_SCHEMA + 1,
                ours: MANIFEST_SCHEMA,
            })
        );
    }

    /// ADR 0044: «o MOD não carrega, e diz» — nomeando os dois números.
    #[test]
    fn uma_api_do_futuro_e_recusada_com_os_dois_numeros() {
        let texto = manifesto_minimo().replace(
            &format!("\"api\": {MOD_API_VERSION}"),
            &format!("\"api\": {}", MOD_API_VERSION + 1),
        );
        assert_eq!(
            read_manifest(&texto),
            Err(Refused::ApiTooNew {
                wanted: MOD_API_VERSION + 1,
                ours: MOD_API_VERSION,
            })
        );
    }

    #[test]
    fn um_identificador_sem_autor_e_recusado() {
        let texto = manifesto_minimo().replace("\"seele/exemplo\"", "\"exemplo\"");
        assert!(matches!(read_manifest(&texto), Err(Refused::MalformedId)));
    }

    /// Um MOD que não declara nenhuma das duas metades não faz nada, e um MOD
    /// que não faz nada instalado em silêncio é a instalação parcial silenciosa
    /// que o ADR 0029 nomeia e o 0044 herda.
    #[test]
    fn um_mod_sem_nenhuma_das_duas_metades_e_recusado() {
        let texto = format!(
            r#"{{"schema":{MANIFEST_SCHEMA},"id":"seele/exemplo","version":"1.0.0",
                "api":{MOD_API_VERSION},"repo":"https://example.invalid/x"}}"#
        );
        assert_eq!(read_manifest(&texto), Err(Refused::Empty));
    }

    /// `reach` e `state` são lidos e ainda não valem nada, e é de propósito.
    ///
    /// ADR 0029, sobre `ansi256` e `ansi16`: «o arquivo que alguém escrever
    /// hoje já está completo no dia em que o produto os ler». Sem eles no
    /// esquema 1, `deny_unknown_fields` recusaria amanhã os manifestos escritos
    /// corretamente hoje — e o esquema só cresce, então não há conserto barato.
    #[test]
    fn reach_e_state_sao_lidos_mesmo_sem_consumidor() {
        let lido = read_manifest(&manifesto_minimo()).expect("manifesto válido recusado");
        assert_eq!(lido.reach, vec!["dom".to_owned()]);
        assert_eq!(lido.state, Some(1));
    }

    /// Um MOD que não pede alcance nenhum é legítimo — um MOD de cor não
    /// alcança nada além do que a janela já lhe dá.
    #[test]
    fn um_manifesto_sem_reach_nem_state_e_lido() {
        let texto = format!(
            r#"{{"schema":{MANIFEST_SCHEMA},"id":"seele/exemplo","version":"1.0.0",
                "api":{MOD_API_VERSION},"repo":"https://example.invalid/x",
                "client":"cliente/main.js"}}"#
        );
        let lido = read_manifest(&texto).expect("manifesto sem os opcionais recusado");
        assert!(lido.reach.is_empty());
        assert_eq!(lido.state, None);
    }

    fn arquivos() -> Vec<(String, Vec<u8>)> {
        vec![
            ("mod.json".to_owned(), b"{}".to_vec()),
            ("cliente/main.js".to_owned(), b"console.log(1)".to_vec()),
        ]
    }

    #[test]
    fn o_mesmo_conteudo_da_o_mesmo_hash() {
        let mut a = arquivos();
        let mut b = arquivos();
        assert_eq!(content_hash(&mut a), content_hash(&mut b));
    }

    /// A ordem em que o disco devolve os arquivos não pode mudar a identidade
    /// de um MOD: um mesmo diretório em duas máquinas tem de dar o mesmo
    /// número, ou a conferência do `mod://` recusa o MOD certo.
    #[test]
    fn a_ordem_dos_arquivos_nao_muda_o_hash() {
        let mut direta = arquivos();
        let mut invertida = arquivos();
        invertida.reverse();
        assert_eq!(content_hash(&mut direta), content_hash(&mut invertida));
    }

    #[test]
    fn um_byte_diferente_da_um_hash_diferente() {
        let mut original = arquivos();
        let mut mexido = arquivos();
        mexido[1].1 = b"console.log(2)".to_vec();
        assert_ne!(content_hash(&mut original), content_hash(&mut mexido));
    }

    /// Sem separador contado, `a/bc` + `d` e `a/b` + `cd` colidiriam, e dois
    /// MODs diferentes teriam a mesma identidade.
    #[test]
    fn mover_bytes_do_nome_para_o_conteudo_muda_o_hash() {
        let mut um = vec![("ab".to_owned(), b"c".to_vec())];
        let mut outro = vec![("a".to_owned(), b"bc".to_vec())];
        assert_ne!(content_hash(&mut um), content_hash(&mut outro));
    }

    /// A primeira coisa que toca texto de terceiro. Totalidade é o ponto —
    /// mesma disciplina de `version::negotiate`.
    #[test]
    fn nenhuma_entrada_causa_panico() {
        for texto in ["", "{", "null", "[]", "{\"schema\":}", "\u{0}"] {
            let _ = read_manifest(texto);
        }
    }
}
