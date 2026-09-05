# MODs — núcleo local: o MOD existe em disco e a janela o carrega

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Um MOD colocado em `$SEELE_HOME/mods/<autor>/<nome>/` pode ser lido, validado, habilitado no servidor hospedado nesta máquina, e o JavaScript dele carrega na janela.

**Architecture:** O manifesto e o hash de conteúdo moram em `seele-proto`, porque a regra de dependência (`xtask/src/check_deps.rs`) proíbe `seele-server` de depender de `seele-core` e as duas pontas precisam do mesmo tipo. O `seele-core` lê o disco. O `seele-server` guarda o que está habilitado, na migração 11. A janela carrega o JS por um protocolo próprio do Tauri, `mod://`, com o hash conferido antes de servir cada byte.

**Tech Stack:** Rust 1.97, `serde_json` 1.0.151 (já na árvore em três crates), `sha2` 0.11 (já em `seele-proto`), Tauri 2, JavaScript sem framework.

**Spec:** `docs/superpowers/specs/2026-09-05-mods-design.md` · ADRs [0044](../../adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md) e [0045](../../adr/0045-toda-versao-continua-de-pe.md)

## Escopo deste plano

Este é o **plano 1 de 5**, e cobre as etapas 1 e 2 da ordem de entrega do spec.

**Entra:** manifesto, validação, hash de conteúdo, leitura do diretório, migração 11, comandos de habilitar/desabilitar, `mod://`, mudança de CSP, carga na janela.

**Não entra, e cada um ganha plano próprio:** o spike do interpretador (etapa 3), o runtime no servidor e a API congelada (4), os verbos de protocolo e a tela de aceite (5), multi-versão (6), indexador (7).

**O que está funcionando ao fim deste plano:** alguém põe um diretório de MOD em disco, habilita no servidor que hospeda nesta máquina, e o JS daquele MOD roda na janela com acesso ao DOM inteiro — que é, pelo ADR 0044, exatamente o alcance decidido. Um MOD de cor ou de layout já funciona. Nada atravessa o fio e nada vem do indexador.

## Global Constraints

Copiados do spec e das convenções do repositório. Valem para toda tarefa.

- **`unsafe_code = "forbid"`** no workspace. Nenhuma tarefa aqui precisa de exceção.
- **`unwrap_used` e `expect_used` são `deny`** fora de teste (`specs/10-convencoes.md`). Em teste, o módulo relaxa com `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]` — é como os módulos existentes fazem.
- **`missing_docs = "warn"`**: todo item público leva doc.
- **Código, identificadores e comentários em inglês; documentação, specs e ADRs em português** (`specs/10-convencoes.md`, ADR 0013). Módulos recentes do `seele-core` seguem inglês (ADR 0023).
- **Chaves do `mod.json` em inglês** (ADR 0044).
- **Nenhuma razão de erro em texto livre chega à interface** (`specs/02-protocolo.md`): cada recusa é uma variante com os números dentro, e a casca decide como escrever. É o padrão de `seele_proto::version::Incompatible`.
- **Regra de dependência**: `proto` não depende de ninguém; `core` depende de `proto` e `audio`; `server` depende de `proto` e **nunca** de `core` nem de `audio`; cascas dependem só de `core`. Conferido por `cargo xtask check-deps`.
- **Migrações são append-only e sem passo de descida.** `no_migration_contains_a_down_step` reprova `DROP TABLE` e `DROP COLUMN`.
- **Nenhuma dependência nova de terceiro.** `serde_json` já está declarado em `seele-app`, `seele-ffi` e `seele-server` na versão `1.0.151`; acrescentá-lo a `seele-proto` não muda o `deny.toml`. Versões são declaradas no crate membro, como os três já fazem.
- **Vocabulário**: `docs/glossario.md` é a autoridade — **servidor**, **sala de voz**, **canal de texto**, **pessoa**; `Server`, `VoiceRoom`, `Channel`, `Person` no código. A palavra na tela para este recurso é **MOD**.

## Estrutura de arquivos

| arquivo | responsabilidade |
|---|---|
| `crates/seele-proto/src/mods.rs` | **criar** — o manifesto, as recusas, e o hash de conteúdo. Sem I/O. |
| `crates/seele-proto/src/lib.rs` | modificar — declarar o módulo |
| `crates/seele-proto/Cargo.toml` | modificar — `serde_json` |
| `crates/seele-core/src/mods.rs` | **criar** — ler o diretório `mods/`, com I/O e nada de validação própria |
| `crates/seele-core/src/lib.rs` | modificar — declarar o módulo |
| `crates/seele-server/src/persistence/schema.rs` | modificar — migração 11 |
| `crates/seele-server/src/persistence/mods.rs` | **criar** — ler e escrever a tabela de MODs |
| `crates/seele-server/src/persistence/mod.rs` | modificar — declarar o módulo |
| `apps/seele-app/src/mods.rs` | **criar** — o handler do `mod://` e a conferência de hash |
| `apps/seele-app/src/main.rs` | modificar — três comandos e o registro do protocolo |
| `apps/seele-app/tauri.conf.json` | modificar — a CSP |
| `apps/seele-app/ui/base.js` | modificar — carregar os MODs habilitados |
| `apps/seele-app/tests/frontend.rs` | modificar — emendar o escopo de um guarda, acrescentar dois |

O handler do `mod://` fica em `apps/seele-app/src/mods.rs` e não no `main.rs` de propósito: aquele arquivo já passa de 3.000 linhas, e uma conferência de hash que decide se bytes de terceiro entram na página é a última coisa que deve ficar difícil de achar.

---

### Task 1: O manifesto, e o que ele recusa

**Files:**
- Create: `crates/seele-proto/src/mods.rs`
- Modify: `crates/seele-proto/src/lib.rs`
- Modify: `crates/seele-proto/Cargo.toml`

**Interfaces:**
- Consumes: nada.
- Produces: `seele_proto::mods::{Manifest, Refused, MANIFEST_SCHEMA, MOD_API_VERSION, read_manifest}`.
  - `pub fn read_manifest(text: &str) -> Result<Manifest, Refused>`
  - `Manifest { schema: u32, id: String, version: String, api: u32, repo: String, reach: Vec<String>, state: Option<u32>, client: Option<String>, server: Option<String> }`

- [ ] **Step 1: Declarar a dependência**

Em `crates/seele-proto/Cargo.toml`, na seção `[dependencies]`, em ordem alfabética entre `postcard` e `serde`:

```toml
# O manifesto de um MOD é JSON, e não postcard: ele é escrito à mão por
# terceiros e lido por uma pessoa antes de instalar (ADR 0044). Já está na
# árvore em `seele-app`, `seele-ffi` e `seele-server`, na mesma versão, então
# não há exceção nova no `deny.toml`.
serde_json = "1.0.151"
```

- [ ] **Step 2: Escrever o teste que falha**

Criar `crates/seele-proto/src/mods.rs` com **apenas** o bloco de teste abaixo mais os `use` que ele precisa. O corpo do módulo vem no passo 4.

```rust
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
        assert!(matches!(read_manifest(&texto), Err(Refused::Malformed { .. })));
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

    /// A primeira coisa que toca texto de terceiro. Totalidade é o ponto —
    /// mesma disciplina de `version::negotiate`.
    #[test]
    fn nenhuma_entrada_causa_panico() {
        for texto in ["", "{", "null", "[]", "{\"schema\":}", "\u{0}"] {
            let _ = read_manifest(texto);
        }
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p seele-proto mods`
Expected: FAIL na compilação — `cannot find function 'read_manifest'`, `cannot find type 'Manifest'`.

- [ ] **Step 4: Escrever o módulo**

No topo de `crates/seele-proto/src/mods.rs`, antes do `mod tests`:

```rust
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
```

Em `crates/seele-proto/src/lib.rs`, junto das outras declarações de módulo, em ordem alfabética:

```rust
pub mod mods;
```

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test -p seele-proto mods`
Expected: PASS, nove testes.

- [ ] **Step 6: Conferir lints e a regra de dependência**

Run: `cargo clippy -p seele-proto --all-targets -- -D warnings && cargo xtask check-deps`
Expected: sem saída de erro nos dois.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-proto/src/mods.rs crates/seele-proto/src/lib.rs crates/seele-proto/Cargo.toml
git commit -m "feat(mods): o manifesto existe, e cada recusa carrega os números

ADR 0044. Chave desconhecida é recusada porque um \`vesion\` com um s a menos
instalaria calado um MOD sem o que o autor achava essencial, e recusar é a
única forma de retorno que um autor tem.

Mora em seele-proto e não no core porque check_deps proíbe o servidor de
depender do core, e as duas pontas precisam do mesmo tipo — a mesma razão que
já pôs \`uri\` e \`attachment\` ali."
```

---

### Task 2: O hash do conteúdo

**Files:**
- Modify: `crates/seele-proto/src/mods.rs`

**Interfaces:**
- Consumes: `Manifest` da Task 1.
- Produces: `pub fn content_hash(files: &mut [(String, Vec<u8>)]) -> [u8; 32]`

- [ ] **Step 1: Escrever o teste que falha**

Acrescentar dentro do `mod tests` existente:

```rust
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto mods::tests::o_mesmo_conteudo`
Expected: FAIL — `cannot find function 'content_hash'`.

- [ ] **Step 3: Implementar**

Acrescentar a `crates/seele-proto/src/mods.rs`, antes do `mod tests`:

```rust
use sha2::{Digest, Sha256};

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
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-proto mods`
Expected: PASS, treze testes.

- [ ] **Step 5: Commit**

```bash
git add crates/seele-proto/src/mods.rs
git commit -m "feat(mods): a identidade de um MOD são os bytes dele, e ela não depende da máquina

Caminhos ordenados porque ordem de diretório não é promessa de nenhum sistema
de arquivos, e cada comprimento entra antes dos bytes dele para que (ab, c) e
(a, bc) não colidam — dois MODs diferentes com a mesma identidade seriam a
falha mais silenciosa possível desta peça."
```

---

### Task 3: Ler o diretório `mods/`

**Files:**
- Create: `crates/seele-core/src/mods.rs`
- Modify: `crates/seele-core/src/lib.rs`

**Interfaces:**
- Consumes: `seele_proto::mods::{Manifest, Refused, content_hash, read_manifest}`.
- Produces:
  - `pub struct Installed { pub manifest: Manifest, pub hash: [u8; 32], pub dir: PathBuf }`
  - `pub enum Found { Ok(Box<Installed>), Refused { id: String, why: Refused } }`
  - `pub fn list(config_dir: &Path) -> Vec<Found>`
  - `pub fn read_one(dir: &Path) -> Result<Installed, Refused>`

- [ ] **Step 1: Escrever o teste que falha**

Criar `crates/seele-core/src/mods.rs` com só o bloco de teste:

```rust
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
        assert!(matches!(&achados[0], Found::Refused { why: Refused::MalformedId, .. }));
    }

    #[test]
    fn um_diretorio_de_mods_que_nao_existe_da_lista_vazia() {
        let raiz = temporario("vazio");
        assert!(list(&raiz).is_empty());
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core mods`
Expected: FAIL na compilação — `cannot find function 'list'`.

- [ ] **Step 3: Implementar**

No topo de `crates/seele-core/src/mods.rs`:

```rust
//! MODs installed on this machine.
//!
//! ADR 0044. This module does the I/O and nothing else: what a manifest *is*,
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

use seele_proto::mods::{content_hash, read_manifest, Manifest, Refused};

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
    let text = std::fs::read_to_string(dir.join("mod.json")).map_err(|_| Refused::Malformed {
        line: 1,
        column: 1,
    })?;
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
        if path.is_dir() {
            files.extend(collect(root, &path));
        } else if let (Ok(relative), Ok(bytes)) =
            (path.strip_prefix(root), std::fs::read(&path))
        {
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

/// The identifier of an entry, however it turned out.
fn name_of(found: &Found) -> &str {
    match found {
        Found::Ok(installed) => &installed.manifest.id,
        Found::Refused { id, .. } => id,
    }
}
```

Em `crates/seele-core/src/lib.rs`, junto das outras declarações, em ordem alfabética:

```rust
pub mod mods;
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-core mods`
Expected: PASS, quatro testes.

- [ ] **Step 5: Lints e dependências**

Run: `cargo clippy -p seele-core --all-targets -- -D warnings && cargo xtask check-deps`
Expected: sem erro.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-core/src/mods.rs crates/seele-core/src/lib.rs
git commit -m "feat(mods): o disco é lido, e um MOD recusado continua na lista

Seria menos código pular o que não valida. Seria também a falha que este
repositório mais paga: quem pôs o diretório ali não recebe resposta nenhuma e
pergunta dias depois sem dado junto.

Um id que discorda do lugar onde o MOD está em disco é recusado, senão dois
MODs disputariam o mesmo diretório sob mod://."
```

---

### Task 4: Migração 11 — o servidor guarda o que está habilitado

**Files:**
- Modify: `crates/seele-server/src/persistence/schema.rs`
- Create: `crates/seele-server/src/persistence/mods.rs`
- Modify: `crates/seele-server/src/persistence/mod.rs`

**Interfaces:**
- Consumes: nada das tarefas anteriores — a tabela guarda `id` e `hash` como texto.
- Produces:
  - `pub fn enable(conn: &Connection, id: &str, version: &str, hash: &str) -> rusqlite::Result<()>`
  - `pub fn disable(conn: &Connection, id: &str) -> rusqlite::Result<()>`
  - `pub fn enabled(conn: &Connection) -> rusqlite::Result<Vec<EnabledMod>>`
  - `pub struct EnabledMod { pub id: String, pub version: String, pub hash: String }`

- [ ] **Step 1: Escrever a migração**

Em `crates/seele-server/src/persistence/schema.rs`, **ao fim** de `MIGRATIONS`, depois da versão 10:

```rust
    Migration {
        version: 11,
        description: "MODs habilitados neste servidor, e o quintal de dados de cada um",
        sql: r#"
            -- ADR 0044. O que este servidor exige de quem entra.
            --
            -- `hash` é texto e não BLOB porque ele é lido por uma pessoa
            -- comparando com o que o indexador publica — a mesma razão pela
            -- qual o `pins` do ADR 0017 é texto puro.
            --
            -- `enabled` é coluna e não a ausência da linha porque desabilitar
            -- preserva os dados: apagar a linha ao desabilitar tornaria o ato
            -- destrutivo, e ninguém desabilita para testar o que não volta.
            CREATE TABLE mods (
                id         TEXT PRIMARY KEY NOT NULL,
                version    TEXT NOT NULL,
                hash       TEXT NOT NULL,
                enabled    INTEGER NOT NULL DEFAULT 1,
                config     TEXT NOT NULL DEFAULT '{}'
            );

            -- O quintal do ADR 0044: cada MOD escreve só sob a chave dele, com
            -- teto conferido no código. Tabela e não coluna porque são muitas
            -- linhas por MOD e nenhuma é lida junto do resto.
            CREATE TABLE mod_data (
                mod_id     TEXT NOT NULL REFERENCES mods(id),
                key        TEXT NOT NULL,
                value      BLOB NOT NULL,
                PRIMARY KEY (mod_id, key)
            );
        "#,
    },
```

- [ ] **Step 2: Rodar os guardas de migração que já existem**

Run: `cargo test -p seele-server persistence::schema`
Expected: PASS — `versions_start_at_one_and_never_skip`, `every_migration_says_what_it_is_for` e `no_migration_contains_a_down_step` continuam passando com a 11 no lugar.

- [ ] **Step 3: Escrever o teste de acesso que falha**

Criar `crates/seele-server/src/persistence/mods.rs` com só o bloco de teste:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::schema::MIGRATIONS;

    fn banco() -> Connection {
        let conn = Connection::open_in_memory().expect("abrir memória");
        for migration in MIGRATIONS {
            conn.execute_batch(migration.sql).expect(migration.description);
        }
        conn
    }

    #[test]
    fn habilitar_e_depois_listar_devolve_o_mod() {
        let conn = banco();
        enable(&conn, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");

        let lista = enabled(&conn).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, "seele/exemplo");
        assert_eq!(lista[0].hash, "abc123");
    }

    /// ADR 0044: desabilitar preserva. Se a linha sumisse, desabilitar seria
    /// destrutivo e ninguém desabilitaria para testar.
    #[test]
    fn desabilitar_tira_da_lista_e_preserva_a_linha() {
        let conn = banco();
        enable(&conn, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");
        disable(&conn, "seele/exemplo").expect("desabilitar");

        assert!(enabled(&conn).expect("listar").is_empty());

        let sobrou: i64 = conn
            .query_row("SELECT count(*) FROM mods WHERE id = ?1", ["seele/exemplo"], |row| row.get(0))
            .expect("contar");
        assert_eq!(sobrou, 1, "desabilitar apagou a linha");
    }

    /// Um MOD que sobe de versão não vira uma segunda linha disputando o mesmo
    /// identificador.
    #[test]
    fn habilitar_de_novo_atualiza_em_vez_de_duplicar() {
        let conn = banco();
        enable(&conn, "seele/exemplo", "1.0.0", "abc123").expect("primeira");
        enable(&conn, "seele/exemplo", "2.0.0", "def456").expect("segunda");

        let lista = enabled(&conn).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].version, "2.0.0");
        assert_eq!(lista[0].hash, "def456");
    }

    /// O quintal de dados sobrevive a desabilitar, porque é o que faz religar
    /// um MOD devolver a sala como ela estava.
    #[test]
    fn o_dado_do_mod_sobrevive_a_desabilitar() {
        let conn = banco();
        enable(&conn, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");
        conn.execute(
            "INSERT INTO mod_data (mod_id, key, value) VALUES (?1, ?2, ?3)",
            rusqlite::params!["seele/exemplo", "placar", b"7".as_slice()],
        )
        .expect("escrever dado");

        disable(&conn, "seele/exemplo").expect("desabilitar");

        let linhas: i64 = conn
            .query_row("SELECT count(*) FROM mod_data", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(linhas, 1);
    }
}
```

- [ ] **Step 4: Rodar e ver falhar**

Run: `cargo test -p seele-server persistence::mods`
Expected: FAIL na compilação — `cannot find function 'enable'`.

- [ ] **Step 5: Implementar**

No topo de `crates/seele-server/src/persistence/mods.rs`:

```rust
//! Which MODs this server requires, and whether each is on.
//!
//! ADR 0044. The rows say what a joining client has to fetch and verify; the
//! bytes never live here.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use rusqlite::Connection;

/// One MOD this server currently requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnabledMod {
    /// `author/name`.
    pub id: String,
    /// The author's version string, as published.
    pub version: String,
    /// Hex of the content hash, so a person can compare it by eye against what
    /// the indexer publishes — the same reason `pins` is plain text (ADR 0017).
    pub hash: String,
}

/// Turns a MOD on, or updates the version and hash of one already known.
///
/// # Errors
///
/// Returns any `rusqlite` failure.
pub fn enable(conn: &Connection, id: &str, version: &str, hash: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO mods (id, version, hash, enabled) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(id) DO UPDATE SET version = ?2, hash = ?3, enabled = 1",
        rusqlite::params![id, version, hash],
    )?;
    Ok(())
}

/// Turns a MOD off, keeping its row and its data.
///
/// # Errors
///
/// Returns any `rusqlite` failure.
pub fn disable(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE mods SET enabled = 0 WHERE id = ?1", [id])?;
    Ok(())
}

/// Every MOD currently on, by identifier.
///
/// # Errors
///
/// Returns any `rusqlite` failure.
pub fn enabled(conn: &Connection) -> rusqlite::Result<Vec<EnabledMod>> {
    let mut statement =
        conn.prepare("SELECT id, version, hash FROM mods WHERE enabled = 1 ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok(EnabledMod {
            id: row.get(0)?,
            version: row.get(1)?,
            hash: row.get(2)?,
        })
    })?;
    rows.collect()
}
```

Em `crates/seele-server/src/persistence/mod.rs`, junto das outras declarações:

```rust
pub mod mods;
```

- [ ] **Step 6: Rodar e ver passar**

Run: `cargo test -p seele-server persistence`
Expected: PASS — os quatro novos mais os três guardas de migração.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-server/src/persistence/
git commit -m "feat(mods): migração 11, e desabilitar deixa de ser destrutivo

\`enabled\` é coluna e não a ausência da linha porque o ADR 0044 decidiu que
desabilitar preserva e remover apaga. Se desabilitar apagasse, ninguém
desabilitaria para testar — e o teste que cobra isso confere que a linha e o
quintal de dados continuam lá depois."
```

---

### Task 5: Os comandos da janela

**Files:**
- Modify: `apps/seele-app/src/main.rs`
- Modify: `apps/seele-app/tests/frontend.rs`

**Interfaces:**
- Consumes: `seele_core::mods::{list, Found}`, `seele_server::persistence::mods::{enable, disable, enabled}`.
- Produces, para o `base.js` da Task 6:
  - comando `mods_instalados() -> Vec<ModNaTela>`
  - comando `habilitar_mod(id: String) -> Result<(), String>`
  - comando `desabilitar_mod(id: String) -> Result<(), String>`
  - `pub struct ModNaTela { pub id: String, pub version: String, pub hash: String, pub client: Option<String>, pub enabled: bool, pub refused: Option<String> }`

- [ ] **Step 1: Escrever o guarda de frontend que falha**

Em `apps/seele-app/tests/frontend.rs`, ao fim do arquivo:

```rust
/// Os três comandos de MOD existem no Rust com o nome que a página invoca.
///
/// ADR 0019 escolheu frontend sem checagem de tipo, e este arquivo é a
/// mitigação nomeada: um `invoke` com nome errado não aparece em build nenhum,
/// só em execução, e só quando alguém clica.
#[test]
fn the_mod_commands_the_page_invokes_exist_in_rust() {
    let rust = read("src/main.rs");
    for command in ["mods_instalados", "habilitar_mod", "desabilitar_mod"] {
        assert!(
            rust.contains(&format!("fn {command}(")),
            "a página invoca `{command}` e o Rust não o define"
        );
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-app --test frontend the_mod_commands`
Expected: FAIL — "a página invoca `mods_instalados` e o Rust não o define".

- [ ] **Step 3: Escrever os comandos**

Em `apps/seele-app/src/main.rs`, junto dos outros comandos (depois de `tirar_icone_do_server`):

```rust
/// One MOD as the window draws it.
///
/// A refused MOD arrives with `refused` filled rather than being left out:
/// whoever dropped the directory there is owed the reason. The string is the
/// variant's name and not a sentence — `ui/frases.js` writes the sentence, per
/// ADR 0012.
#[derive(serde::Serialize)]
struct ModNaTela {
    id: String,
    version: String,
    hash: String,
    client: Option<String>,
    enabled: bool,
    refused: Option<String>,
}

/// Every MOD in `mods/`, with whether this machine's server has it on.
#[tauri::command]
fn mods_instalados(app: AppHandle, session: State<'_, Session>) -> Vec<ModNaTela> {
    let ligados = mods_ligados(&session);
    seele_core::mods::list(std::path::Path::new(&config_dir(&app)))
        .into_iter()
        .map(|found| match found {
            seele_core::mods::Found::Ok(installed) => ModNaTela {
                enabled: ligados.iter().any(|id| *id == installed.manifest.id),
                id: installed.manifest.id.clone(),
                version: installed.manifest.version.clone(),
                hash: hex(&installed.hash),
                client: installed.manifest.client.clone(),
                refused: None,
            },
            seele_core::mods::Found::Refused { id, why } => ModNaTela {
                id,
                version: String::new(),
                hash: String::new(),
                client: None,
                enabled: false,
                refused: Some(nome_da_recusa(&why).to_owned()),
            },
        })
        .collect()
}

/// Turns a MOD on for the server this process hosts.
///
/// # Errors
///
/// Fails when this process is not hosting, when the MOD is not installed, or
/// when it does not validate.
#[tauri::command]
fn habilitar_mod(app: AppHandle, session: State<'_, Session>, id: String) -> Result<(), String> {
    let dir = std::path::Path::new(&config_dir(&app)).join("mods").join(&id);
    let installed = seele_core::mods::read_one(&dir).map_err(|why| nome_da_recusa(&why).to_owned())?;
    com_o_banco(&session, |conn| {
        seele_server::persistence::mods::enable(
            conn,
            &installed.manifest.id,
            &installed.manifest.version,
            &hex(&installed.hash),
        )
    })
}

/// Turns a MOD off. Its data stays (ADR 0044).
///
/// # Errors
///
/// Fails when this process is not hosting.
#[tauri::command]
fn desabilitar_mod(session: State<'_, Session>, id: String) -> Result<(), String> {
    com_o_banco(&session, |conn| {
        seele_server::persistence::mods::disable(conn, &id)
    })
}

/// The variant's name, for `ui/frases.js` to turn into a sentence.
///
/// ADR 0012 puts the wording in the shell: the core never formats a message.
fn nome_da_recusa(why: &seele_proto::mods::Refused) -> &'static str {
    use seele_proto::mods::Refused;
    match why {
        Refused::Malformed { .. } => "malformed",
        Refused::SchemaTooNew { .. } => "schema-too-new",
        Refused::ApiTooNew { .. } => "api-too-new",
        Refused::MalformedId => "malformed-id",
        Refused::Empty => "empty",
    }
}

/// Lowercase hex, which is what a person compares against the indexer by eye.
fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
```

**Nota para quem implementa:** `mods_ligados` e `com_o_banco` são os dois pontos em que esta tarefa toca o servidor hospedado neste processo. O `main.rs` já guarda o servidor local em `State` para `renomear_server` e `escolher_icone_do_server` — **siga o mesmo acessor que aqueles dois usam**, em vez de abrir uma conexão nova. Leia `renomear_server` (`apps/seele-app/src/main.rs`, procure por `fn renomear_server`) antes de escrever estas duas, e nomeie-as com o mesmo padrão que encontrar lá. Se o acessor devolver erro quando não há servidor hospedado, propague-o: habilitar MOD sem hospedar é uma recusa legítima e nomeável, não um `unwrap`.

Registrar os três em `tauri::generate_handler![...]`, junto dos outros.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-app --test frontend the_mod_commands`
Expected: PASS.

- [ ] **Step 5: Compilar e conferir lints**

Run: `cargo clippy -p seele-app --all-targets -- -D warnings`
Expected: sem erro. Em particular, nenhum `unwrap` ou `expect` fora de teste.

- [ ] **Step 6: Commit**

```bash
git add apps/seele-app/src/main.rs apps/seele-app/tests/frontend.rs
git commit -m "feat(mods): a janela lista, habilita e desabilita

Um MOD recusado chega à tela com o motivo em vez de ficar de fora da lista, e
o motivo é o nome da variante e não uma frase — quem escreve a frase é o
frases.js, como o ADR 0012 decidiu.

O guarda novo cobra que os três nomes que a página invoca existam no Rust:
ADR 0019 abriu mão de checagem de tipo no frontend e nomeou esta mitigação."
```

---

### Task 6: `mod://`, a CSP, e a carga na janela

**Files:**
- Create: `apps/seele-app/src/mods.rs`
- Modify: `apps/seele-app/src/main.rs`
- Modify: `apps/seele-app/tauri.conf.json`
- Modify: `apps/seele-app/ui/base.js`
- Modify: `apps/seele-app/tests/frontend.rs`

**Interfaces:**
- Consumes: `seele_core::mods::read_one`, `ModNaTela` da Task 5.
- Produces: `pub fn serve(config_dir: &Path, url_path: &str) -> Option<Vec<u8>>` — o corpo do handler, testável sem Tauri.

- [ ] **Step 1: Escrever os testes do handler**

Criar `apps/seele-app/src/mods.rs` com só o bloco de teste, reusando as mesmas funções de semeadura da Task 3 (repetidas aqui de propósito — quem implementa esta tarefa pode não ter lido aquela):

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
        let dir = std::env::temp_dir().join(format!("seele-modserve-{nome}-{}", std::process::id()));
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

    /// `..` não sobe. Sem isto, `mod://` seria leitura de disco arbitrária a
    /// partir de um caminho que um terceiro escolhe.
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-app mods::tests`
Expected: FAIL na compilação — `cannot find function 'serve'`.

- [ ] **Step 3: Implementar o handler**

No topo de `apps/seele-app/src/mods.rs`:

```rust
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
//! - **nothing climbs out.** The path is rebuilt from components and compared
//!   against the MOD's own directory after resolution, so `..` cannot reach a
//!   byte outside it. Without this, `mod://` would be arbitrary disk reads from
//!   a path a third party chooses.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Component, Path, PathBuf};

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
pub fn serve(config_dir: &Path, url_path: &str) -> Option<Vec<u8>> {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let author = parts.next()?;
    let name = parts.next()?;
    let inside: Vec<&str> = parts.collect();
    if inside.is_empty() || author.is_empty() || name.is_empty() {
        return None;
    }

    // Rebuilt from components: a `..` anywhere is refused rather than resolved.
    let mut relative = PathBuf::new();
    for part in &inside {
        let mut components = Path::new(part).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(piece)), None) => relative.push(piece),
            _ => return None,
        }
    }

    let dir = config_dir.join("mods").join(author).join(name);
    let installed = seele_core::mods::read_one(&dir).ok()?;

    // Only what the manifest declares. Today that is the client script; when
    // a MOD may ship more than one file, this list grows and this guard does
    // not change shape.
    let declared = installed.manifest.client.as_deref()?;
    if Path::new(declared) != relative.as_path() {
        return None;
    }

    std::fs::read(dir.join(&relative)).ok()
}
```

Em `apps/seele-app/src/main.rs`, junto das outras declarações de módulo:

```rust
mod mods;
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-app mods::tests`
Expected: PASS, cinco testes.

- [ ] **Step 5: Registrar o protocolo no Tauri**

Em `apps/seele-app/src/main.rs`, no `tauri::Builder`, antes de `.invoke_handler(...)`:

```rust
        // ADR 0044. O único esquema além de `self` que a CSP admite, e ele só
        // devolve o que `mods::serve` aprovou.
        .register_uri_scheme_protocol("mod", |ctx, request| {
            let caminho = request.uri().path().to_owned();
            let bytes = mods::serve(std::path::Path::new(&config_dir(ctx.app_handle())), &caminho);
            match bytes {
                Some(corpo) => tauri::http::Response::builder()
                    .header("Content-Type", "text/javascript; charset=utf-8")
                    .body(corpo)
                    .unwrap_or_else(|_| vazio()),
                None => vazio(),
            }
        })
```

E, junto de `hex` na Task 5:

```rust
/// A 404 with no body, which is the only answer `mod://` gives to anything it
/// did not approve.
fn vazio() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Vec::new())
        .unwrap_or_default()
}
```

**Nota para quem implementa:** a assinatura exata de `register_uri_scheme_protocol` mudou entre versões do Tauri 2. Se o fechamento acima não compilar, **não invente**: rode `cargo doc -p tauri --open` e leia a assinatura da versão travada no `Cargo.lock`, e ajuste só a forma do fechamento. O corpo — chamar `mods::serve` e devolver 404 para `None` — não muda.

- [ ] **Step 6: Abrir a CSP para `mod:`**

Em `apps/seele-app/tauri.conf.json`, linha 23, trocar só a diretiva `script-src`:

```json
      "csp": "default-src 'self'; style-src 'self'; script-src 'self' mod:; img-src 'self' data: mod:; font-src 'self'; connect-src ipc: http://ipc.localhost"
```

`img-src` ganha `mod:` junto porque um MOD de aparência sem imagem própria não é um MOD de aparência. `style-src` **não** ganha: uma folha de terceiro passaria a existir e os guardas do vermelho pedem folha, então isso é decisão de outra tarefa e de outro dia. Um MOD que quer pintar usa CSSOM, que é o que o ADR 0029 já descrevia.

- [ ] **Step 7: Escrever o guarda da CSP e emendar o guarda de escopo**

Em `apps/seele-app/tests/frontend.rs`, ao fim:

```rust
/// A CSP admite `mod:` e nada mais que isso.
///
/// O ADR 0044 escreveu como critério e não como coincidência: se aplicar um
/// MOD exigisse `unsafe-inline` ou `unsafe-eval`, o desenho estaria errado.
#[test]
fn the_csp_admits_mods_and_never_loosens_further() {
    let conf = read("tauri.conf.json");
    let csp = conf
        .lines()
        .find(|line| line.contains("\"csp\""))
        .expect("a CSP sumiu do tauri.conf.json");

    assert!(csp.contains("script-src 'self' mod:"), "`mod:` saiu do script-src");
    assert!(!csp.contains("unsafe-inline"), "a CSP ganhou unsafe-inline");
    assert!(!csp.contains("unsafe-eval"), "a CSP ganhou unsafe-eval");
    assert!(
        !csp.contains("style-src 'self' mod:"),
        "folha de terceiro entrou pela CSP sem ADR que a autorize"
    );
}
```

E emendar o escopo do guarda existente. Localize `the_page_loads_only_files_that_are_shipped` (`frontend.rs:520`) e acrescente ao doc-comment dele, sem mudar o corpo:

```rust
/// **Escopo, desde o ADR 0044:** este guarda vale sobre `self` — os arquivos
/// embutidos em `ui/`. Bytes servidos sob `mod://` não passam por aqui e têm o
/// guarda deles em `apps/seele-app/src/mods.rs`: nada é servido que o manifesto
/// do MOD não declare, e nada sobe de diretório. A emenda é de alcance e não de
/// rigor — nenhum arquivo de MOD entra em `ui/`.
```

- [ ] **Step 8: Rodar os dois**

Run: `cargo test -p seele-app --test frontend`
Expected: PASS — o novo, o emendado e todos os que já existiam.

- [ ] **Step 9: Carregar os MODs habilitados na janela**

Em `apps/seele-app/ui/base.js`, acrescentar a função abaixo e chamá-la **depois** do primeiro `desenhar(await invoke("snapshot"))` (linha 100), para que a página exista antes de um MOD tentar mexer nela:

```js
// ADR 0044. Um MOD roda com acesso à janela inteira, por decisão: as regras do
// produto base — palheta congelada, movimento diagnóstico, os guardas do
// vermelho — protegem o produto e não alcançam MOD. A defesa é o repositório
// público e a revisão de código de cada versão, e ela não mora aqui.
//
// Carregado depois do primeiro desenho de propósito: um MOD que roda antes de a
// página existir não acha o que veio mexer, e falharia de um jeito que parece
// defeito do MOD sem ser.
async function carregarMods() {
    let instalados;
    try {
        instalados = await invoke("mods_instalados");
    } catch (erro) {
        console.error("MODs: não deu para listar", erro);
        return;
    }

    for (const mod of instalados) {
        if (!mod.enabled || !mod.client) continue;

        const script = document.createElement("script");
        script.type = "module";
        script.src = `mod://localhost/${mod.id}/${mod.client}`;
        // Um MOD que quebra não leva a janela junto — ADR 0044, «falha
        // isolada». Sem isto, um erro de sintaxe num MOD de terceiro é uma tela
        // preta que ninguém sabe explicar.
        script.addEventListener("error", () => {
            console.error(`MOD ${mod.id}: não carregou`);
        });
        document.head.appendChild(script);
    }
}
```

**Nota para quem implementa:** a autoridade da URL (`localhost` acima) depende de como o Tauri monta o esquema no sistema em que roda — no Windows os esquemas próprios ficam sob `http://mod.localhost/`. Confira o que `request.uri()` recebe de fato, imprimindo-o uma vez no handler da Step 5, e use a forma que chegar. Não adivinhe: um caminho errado aqui falha em silêncio, que é exatamente o modo de falha que este plano tenta não construir.

- [ ] **Step 10: Guarda de que a página carrega MOD e o faz depois do primeiro desenho**

Em `apps/seele-app/tests/frontend.rs`, ao fim:

```rust
/// A página carrega MOD, e só depois de haver página.
#[test]
fn the_page_loads_mods_after_the_first_draw() {
    let base = read("ui/base.js");
    assert!(base.contains("mods_instalados"), "base.js não lista MODs");
    assert!(base.contains("mod://"), "base.js não carrega nada sob mod://");

    let primeiro_desenho = base.find("desenhar(await invoke(\"snapshot\"))");
    let carga = base.find("carregarMods()");
    let (Some(desenho), Some(carga)) = (primeiro_desenho, carga) else {
        panic!("não achei o primeiro desenho ou a chamada de carregarMods");
    };
    assert!(
        carga > desenho,
        "os MODs carregam antes de a página existir, e um MOD que roda cedo \
         falha de um jeito que parece defeito dele"
    );
}
```

- [ ] **Step 11: Rodar a suíte inteira do app**

Run: `cargo test -p seele-app`
Expected: PASS.

- [ ] **Step 12: Provar o guarda contra a regressão de verdade**

O `CLAUDE.md` deste repositório é explícito: *«Provar um guarda contra a regressão de verdade — revertendo o conserto e vendo-o falhar — é o que separa os dois.»* Faça, e não pule:

1. Em `apps/seele-app/src/mods.rs`, comente a conferência `if Path::new(declared) != relative.as_path()`.
2. Run: `cargo test -p seele-app mods::tests::um_arquivo_que_o_manifesto_nao_declara_nao_e_servido`
3. Expected: **FAIL**. Se passar, o guarda não guarda nada e a tarefa não está pronta.
4. Desfaça o comentário e rode de novo. Expected: PASS.
5. Repita com a reconstrução por componentes e `um_caminho_que_tenta_subir_nao_e_servido`.

- [ ] **Step 13: Commit**

```bash
git add apps/seele-app/src/mods.rs apps/seele-app/src/main.rs \
        apps/seele-app/tauri.conf.json apps/seele-app/ui/base.js \
        apps/seele-app/tests/frontend.rs
git commit -m "feat(mods): mod:// existe, e a CSP abre um esquema e nada mais

script-src 'self' recusa qualquer script que não esteja embutido, e afrouxar
para unsafe-eval seria abrir a porta mais larga do prédio para resolver o
problema mais estreito. Os arquivos ficam em disco e um esquema próprio os
serve, com dois guardas no lugar do que a CSP deixa de garantir: nada que o
manifesto não declare, e nada que suba de diretório.

O guarda de escopo do the_page_loads_only_files_that_are_shipped ganha a
emenda por escrito: ele vale sobre self, e mod:// tem o guarda dele."
```

---

## Ao fim deste plano

- [ ] **Rodar tudo**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && cargo xtask check-deps`
Expected: PASS nos quatro.

- [ ] **Provar à mão, com um MOD de verdade**

Um plano que só passa em teste não provou que o recurso existe. Crie
`$SEELE_HOME/mods/seele/primeiro/` com:

`mod.json`:
```json
{
  "schema": 1,
  "id": "seele/primeiro",
  "version": "1.0.0",
  "api": 1,
  "repo": "https://example.invalid/primeiro",
  "reach": ["dom"],
  "client": "cliente/main.js"
}
```

`cliente/main.js`:
```js
document.documentElement.style.setProperty("--seele-laranja-nerv", "#00b7ff");
console.log("MOD seele/primeiro: entrou");
```

Abra o app, hospede, habilite o MOD, e confira as três coisas:

1. o MOD aparece na lista;
2. depois de habilitar e reabrir, o laranja da interface está azul;
3. o console diz «entrou».

Se qualquer uma falhar, o plano não terminou — e o que falhou é informação, não
motivo para contornar.

## O que este plano deliberadamente não faz

Nomeado para não parecer esquecimento:

- **Nenhum MOD roda no servidor.** A metade `server/` do manifesto é lida e
  guardada, e nunca executada. O runtime é o plano 3, e ele começa por um spike
  que mede o interpretador em vez de escolher por intuição.
- **Nada atravessa o fio.** Habilitar MOD aqui é local, pelo banco do servidor
  que este processo hospeda. Os verbos com `AdministerServer` são o plano 4.
- **Nada vem do indexador.** `mods.seele.app.br` é o plano 5. Até lá, MOD entra
  copiando diretório à mão — o que é o suficiente para provar o formato antes de
  ele virar contrato com terceiros, que é a razão de esta etapa vir primeiro.
- **Não há tela de aceite**, porque não há MOD chegando de outra pessoa.
- **Não há isolamento de falha do lado do servidor.** Do lado do cliente há o
  mínimo: um script que não carrega é registrado e não derruba a janela.
