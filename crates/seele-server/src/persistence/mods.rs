//! Which MODs this server requires, and whether each is on.
//!
//! ADR 0045. The rows say what a joining client has to fetch and verify; the
//! bytes never live here.
//!
//! Free functions taking `&Persistence`, and not methods over a `Connection`,
//! for the reason the neighbours already follow — `admissao::criar_convite`,
//! `icone_da_pessoa`: `Persistence::connection` is `pub(crate)`, so a signature
//! over a raw connection cannot be called from the desktop shell, which is
//! exactly who calls these.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::params;

use super::Persistence;

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
/// Fails if the row cannot be written.
pub fn enable(persistence: &Persistence, id: &str, version: &str, hash: &str) -> Result<()> {
    persistence.connection().execute(
        "INSERT INTO mods (id, version, hash, enabled) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(id) DO UPDATE SET version = ?2, hash = ?3, enabled = 1",
        params![id, version, hash],
    )?;
    Ok(())
}

/// Turns a MOD off, keeping its row and its data.
///
/// # Errors
///
/// Fails if the row cannot be written.
pub fn disable(persistence: &Persistence, id: &str) -> Result<()> {
    persistence
        .connection()
        .execute("UPDATE mods SET enabled = 0 WHERE id = ?1", [id])?;
    Ok(())
}

/// Every MOD currently on, by identifier.
///
/// # Errors
///
/// Fails if the table cannot be read.
pub fn enabled(persistence: &Persistence) -> Result<Vec<EnabledMod>> {
    let connection = persistence.connection();
    let mut statement =
        connection.prepare("SELECT id, version, hash FROM mods WHERE enabled = 1 ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok(EnabledMod {
            id: row.get(0)?,
            version: row.get(1)?,
            hash: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Everything one MOD has in its yard.
///
/// The whole yard at once, and not key by key, because that is the shape
/// `mods::Anfitriao` wants: it loads before a call and stores after, which is
/// what makes a call transactional without this module knowing what a call is.
///
/// A yard that does not fit in memory is a yard past `TETO_DO_QUINTAL`, and
/// that ceiling is enforced where the writing happens.
///
/// Values are UTF-8 out of a `BLOB` column. Non-UTF-8 rows are skipped rather
/// than failing the read: a yard that refuses to load because of one bad row
/// takes the whole MOD down with it, and the MOD cannot fix a row it cannot
/// read.
///
/// # Errors
///
/// Fails if the table cannot be read.
pub fn ler_quintal(persistence: &Persistence, mod_id: &str) -> Result<BTreeMap<String, String>> {
    let connection = persistence.connection();
    let mut statement =
        connection.prepare("SELECT key, value FROM mod_data WHERE mod_id = ?1 ORDER BY key")?;
    let linhas = statement.query_map([mod_id], |row| {
        let chave: String = row.get(0)?;
        let valor: Vec<u8> = row.get(1)?;
        Ok((chave, valor))
    })?;

    let mut quintal = BTreeMap::new();
    for linha in linhas {
        let (chave, valor) = linha?;
        if let Ok(texto) = String::from_utf8(valor) {
            quintal.insert(chave, texto);
        }
    }
    Ok(quintal)
}

/// Replaces one MOD's yard with what it holds now.
///
/// Replace and not merge, inside one transaction: the map that comes back from
/// a call **is** the yard, so a key the MOD deleted has to disappear here too.
/// Merging would make `delete` impossible to express from JavaScript.
///
/// # Errors
///
/// Fails if the transaction cannot be committed. Nothing is written when it
/// fails, which is what keeps a call transactional end to end.
pub fn gravar_quintal(
    persistence: &mut Persistence,
    mod_id: &str,
    quintal: &BTreeMap<String, String>,
) -> Result<()> {
    let transacao = persistence.connection_mut().transaction()?;
    transacao.execute("DELETE FROM mod_data WHERE mod_id = ?1", [mod_id])?;
    {
        let mut inserir =
            transacao.prepare("INSERT INTO mod_data (mod_id, key, value) VALUES (?1, ?2, ?3)")?;
        for (chave, valor) in quintal {
            inserir.execute(params![mod_id, chave, valor.as_bytes()])?;
        }
    }
    transacao.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    #[test]
    fn o_quintal_ida_e_volta() {
        let mut persistence = banco();
        enable(&persistence, "seele/x", "1.0.0", "h").expect("habilitar");

        let quintal = BTreeMap::from([
            ("pontos".to_owned(), "7".to_owned()),
            ("nome".to_owned(), "coelho".to_owned()),
        ]);
        gravar_quintal(&mut persistence, "seele/x", &quintal).expect("gravar");

        assert_eq!(ler_quintal(&persistence, "seele/x").expect("ler"), quintal);
    }

    /// Gravar substitui em vez de misturar, senão apagar uma chave seria
    /// impossível de expressar do JavaScript.
    #[test]
    fn gravar_substitui_e_uma_chave_apagada_some() {
        let mut persistence = banco();
        enable(&persistence, "seele/x", "1.0.0", "h").expect("habilitar");

        gravar_quintal(
            &mut persistence,
            "seele/x",
            &BTreeMap::from([
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ]),
        )
        .expect("primeira");
        gravar_quintal(
            &mut persistence,
            "seele/x",
            &BTreeMap::from([("a".to_owned(), "1".to_owned())]),
        )
        .expect("segunda");

        let lido = ler_quintal(&persistence, "seele/x").expect("ler");
        assert_eq!(lido.len(), 1);
        assert!(!lido.contains_key("b"), "a chave apagada sobreviveu");
    }

    /// Dois MODs não leem o quintal um do outro.
    #[test]
    fn o_quintal_de_um_mod_nao_aparece_no_do_outro() {
        let mut persistence = banco();
        enable(&persistence, "seele/um", "1.0.0", "h").expect("um");
        enable(&persistence, "seele/dois", "1.0.0", "h").expect("dois");

        gravar_quintal(
            &mut persistence,
            "seele/um",
            &BTreeMap::from([("segredo".to_owned(), "meu".to_owned())]),
        )
        .expect("gravar");

        assert!(ler_quintal(&persistence, "seele/dois")
            .expect("ler")
            .is_empty());
    }

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("abrir memória")
    }

    #[test]
    fn habilitar_e_depois_listar_devolve_o_mod() {
        let persistence = banco();
        enable(&persistence, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, "seele/exemplo");
        assert_eq!(lista[0].hash, "abc123");
    }

    /// ADR 0045: desabilitar preserva. Se a linha sumisse, desabilitar seria
    /// destrutivo e ninguém desabilitaria para testar.
    #[test]
    fn desabilitar_tira_da_lista_e_preserva_a_linha() {
        let persistence = banco();
        enable(&persistence, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");
        disable(&persistence, "seele/exemplo").expect("desabilitar");

        assert!(enabled(&persistence).expect("listar").is_empty());

        let sobrou: i64 = persistence
            .connection()
            .query_row(
                "SELECT count(*) FROM mods WHERE id = ?1",
                ["seele/exemplo"],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(sobrou, 1, "desabilitar apagou a linha");
    }

    /// Um MOD que sobe de versão não vira uma segunda linha disputando o mesmo
    /// identificador.
    #[test]
    fn habilitar_de_novo_atualiza_em_vez_de_duplicar() {
        let persistence = banco();
        enable(&persistence, "seele/exemplo", "1.0.0", "abc123").expect("primeira");
        enable(&persistence, "seele/exemplo", "2.0.0", "def456").expect("segunda");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].version, "2.0.0");
        assert_eq!(lista[0].hash, "def456");
    }

    /// O quintal de dados sobrevive a desabilitar, porque é o que faz religar
    /// um MOD devolver a sala como ela estava.
    #[test]
    fn o_dado_do_mod_sobrevive_a_desabilitar() {
        let persistence = banco();
        enable(&persistence, "seele/exemplo", "1.0.0", "abc123").expect("habilitar");
        persistence
            .connection()
            .execute(
                "INSERT INTO mod_data (mod_id, key, value) VALUES (?1, ?2, ?3)",
                params!["seele/exemplo", "placar", b"7".as_slice()],
            )
            .expect("escrever dado");

        disable(&persistence, "seele/exemplo").expect("desabilitar");

        let linhas: i64 = persistence
            .connection()
            .query_row("SELECT count(*) FROM mod_data", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(linhas, 1);
    }
}
