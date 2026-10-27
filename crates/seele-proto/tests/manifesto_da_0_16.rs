//! **O que uma 0.16 já instalada faz com um manifesto da API 7** (E8 da 0.17).
//!
//! A 0.16 publicada (`v0.16.0`, do commit `8faa3379b`) não se altera. O
//! analisador de manifesto dela está congelado no binário, e a pergunta é o que
//! ele faz com um `mod.json` que a 0.17 aceita. Não se finge que um MOD da 7
//! equivale a um da 6: é preciso saber se a 0.16 o recusa, e com que nome.
//!
//! Este arquivo traz uma **réplica** do `Manifest` e do começo de
//! `read_manifest` da 0.16. São os campos, o `deny_unknown_fields` e a ordem
//! das conferências: o JSON, o `schema`, e a API contra `[6]`. A réplica é
//! conferida contra o código publicado por `git show 8faa3379b:…`, quando o
//! histórico está disponível; sem ele, a conferência é pulada **em voz alta**.
//!
//! O que se mede:
//! - um manifesto da 7 **com** `permissoes` é recusado pela 0.16 como
//!   `Malformed` (chave desconhecida), antes de olhar a API. A tela dela diz
//!   «manifesto malformado», e não «API mais nova»;
//! - um manifesto da 7 **sem** `permissoes` é recusado como `ApiTooNew {7, 6}`;
//! - um manifesto da 6 sem `permissoes` continua aceito por ela.
//!
//! Por isso o catálogo não pode oferecer a uma 0.16 uma versão da 7. A 0.16
//! escolhe a versão do catálogo pela `api` de cada entrada
//! (`ui/camada-mods.js`, `ultimaQueEsteBuildEntende` e `versaoParaEsteApp`,
//! iguais no `8faa3379b`), e o indexador tem de escrever a `api` que o
//! `mod.json` declara.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use serde::Deserialize;

/// O commit da v0.16.0, conferido pela API de releases.
const COMMIT_DA_0_16: &str = "8faa3379b";

/// A réplica do `Manifest` da 0.16 (`crates/seele-proto/src/mods.rs:396-458`
/// no `8faa3379b`). Só o que decide aceitar ou recusar: os campos e o
/// `deny_unknown_fields`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "os campos existem para o serde recusar o que não está aqui"
)]
struct ManifestDa016 {
    schema: u32,
    id: String,
    version: String,
    api: u32,
    repo: String,
    #[serde(default)]
    reach: Vec<String>,
    #[serde(default)]
    state: Option<u32>,
    #[serde(default)]
    client: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    arquivos: Vec<String>,
    #[serde(default)]
    tema: bool,
}

/// Os campos da réplica, na ordem do código.
const CAMPOS_DA_REPLICA: [&str; 11] = [
    "schema", "id", "version", "api", "repo", "reach", "state", "client", "server", "arquivos",
    "tema",
];

#[derive(Debug, PartialEq, Eq)]
enum RecusaDa016 {
    Malformed,
    SchemaTooNew,
    ApiTooNew { wanted: u32, ours: u32 },
    ApiTooOld,
}

/// O começo de `read_manifest` da 0.16 (`:617-642` no `8faa3379b`): o JSON
/// pelo serde, o `schema` contra 1, e a API contra `MOD_API_VERSION = 6` e
/// `APIS_ACEITAS = [6]`.
fn ler_como_a_0_16(texto: &str) -> Result<ManifestDa016, RecusaDa016> {
    let manifesto: ManifestDa016 =
        serde_json::from_str(texto).map_err(|_| RecusaDa016::Malformed)?;
    if manifesto.schema > 1 {
        return Err(RecusaDa016::SchemaTooNew);
    }
    if manifesto.api > 6 {
        return Err(RecusaDa016::ApiTooNew {
            wanted: manifesto.api,
            ours: 6,
        });
    }
    if manifesto.api != 6 {
        return Err(RecusaDa016::ApiTooOld);
    }
    Ok(manifesto)
}

fn manifesto(api: u32, permissoes: bool) -> String {
    let extra = if permissoes {
        r#""permissoes": [{"acao": "fechar-mesa", "nome": "Fechar a mesa"}],"#
    } else {
        ""
    };
    format!(
        r#"{{"schema": 1, "id": "seele/mesa", "version": "5.0.0", "api": {api}, {extra}
            "repo": "https://github.com/seele/mesa", "server": "servidor/main.js"}}"#
    )
}

/// **A 0.16 recusa um MOD da 7 com permissões como malformado**, e não como
/// API mais nova. E a 0.17 o aceita.
#[test]
fn a_0_16_recusa_um_mod_da_7_com_permissoes_como_malformado() {
    let texto = manifesto(7, true);
    assert_eq!(
        ler_como_a_0_16(&texto).map(|_| ()),
        Err(RecusaDa016::Malformed)
    );
    let lido = seele_proto::mods::read_manifest(&texto).expect("a 0.17 recusou um MOD da 7");
    assert_eq!(lido.permissoes.len(), 1);
}

/// Sem `permissoes`, a 0.16 diz o nome certo: `ApiTooNew { 7, 6 }`.
#[test]
fn a_0_16_recusa_um_mod_da_7_sem_permissoes_como_api_mais_nova() {
    assert_eq!(
        ler_como_a_0_16(&manifesto(7, false)).map(|_| ()),
        Err(RecusaDa016::ApiTooNew { wanted: 7, ours: 6 })
    );
}

/// O MOD da 6 sem permissões roda nas duas: o alargamento não tira a 6 de
/// ninguém.
#[test]
fn um_mod_da_6_roda_na_0_16_e_na_0_17() {
    let texto = manifesto(6, false);
    assert!(ler_como_a_0_16(&texto).is_ok());
    assert!(seele_proto::mods::read_manifest(&texto).is_ok());
    assert_eq!(
        ler_como_a_0_16(&manifesto(5, false)).map(|_| ()),
        Err(RecusaDa016::ApiTooOld)
    );
}

/// **A réplica é a do código publicado.** Lê o `mods.rs` do `8faa3379b` pelo
/// `git` e confere três coisas: os campos do `Manifest`, o
/// `deny_unknown_fields` e as constantes da API. Sem o histórico (um clone
/// raso, um pacote de fontes), a conferência é pulada e diz que pulou.
#[test]
fn a_replica_e_o_manifesto_da_0_16_publicada() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let saida = std::process::Command::new("git")
        .arg("-C")
        .arg(&raiz)
        .args([
            "show",
            &format!("{COMMIT_DA_0_16}:crates/seele-proto/src/mods.rs"),
        ])
        .output();
    let Some(saida) = saida.ok().filter(|saida| saida.status.success()) else {
        eprintln!(
            "PULADO: sem o histórico do {COMMIT_DA_0_16}, a réplica do manifesto da 0.16 não \
             foi conferida contra o código publicado"
        );
        return;
    };
    let fonte = String::from_utf8(saida.stdout).unwrap();
    let corpo = fonte
        .split_once("#[serde(deny_unknown_fields)]\npub struct Manifest {")
        .expect("o Manifest da 0.16 não tem `deny_unknown_fields` logo antes dele")
        .1
        .split_once("\n}\n")
        .expect("o Manifest da 0.16 não fecha")
        .0;
    let campos: Vec<&str> = corpo
        .lines()
        .filter_map(|linha| linha.strip_prefix("    pub "))
        .filter_map(|resto| resto.split_once(':').map(|(nome, _)| nome))
        .collect();
    assert_eq!(
        campos, CAMPOS_DA_REPLICA,
        "a réplica não tem os campos da 0.16"
    );
    for constante in [
        "pub const MOD_API_VERSION: u32 = 6;",
        "pub const MANIFEST_SCHEMA: u32 = 1;",
        "pub const APIS_POR_LINHA: [(&str, &[u32]); 2] = [(\"1\", &[6]), (\"0.15\", &[5, 4, 3])];",
    ] {
        assert!(
            fonte.contains(constante),
            "a 0.16 publicada não tem «{constante}»"
        );
    }
}
