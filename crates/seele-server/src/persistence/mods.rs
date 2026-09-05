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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::schema::MIGRATIONS;

    fn banco() -> Connection {
        let conn = Connection::open_in_memory().expect("abrir memória");
        for migration in MIGRATIONS {
            conn.execute_batch(migration.sql)
                .unwrap_or_else(|error| panic!("{}: {error}", migration.description));
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
