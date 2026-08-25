//! PERSISTENCE — persistent state.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > SQLite, single file, WAL on. Tables: `pessoas`, `papeis`, `persono_papeis`,
//! > `cages`, `linhas`, `mensagens`, `banimentos`, `config`, `schema_version`.
//! >
//! > - Migrations embedded in the binary, applied at boot, versioned and
//! >   irreversible.
//! > - Message history with configurable retention (default: unlimited).
//! > - Index on `(linha_id, criado_em)` for cursor pagination.
//! > - Batched message writes with a time flush (~200 ms), so there is no fsync
//! >   per message.
//!
//! # Why a single connection behind a mutex, and not a pool
//!
//! SQLite in WAL mode allows one writer and many readers, and the target is a
//! Dogma of ~50 people on 1 vCPU (`specs/00-visao-geral.md`). A pool would add
//! contention management for a workload that is a few writes a second. What
//! matters far more is not calling `fsync` per message. `specs/04` asks for
//! batched writes with a ~200 ms flush, which arrives with the text channel in
//! M3.3 — the schema and the index it needs are already here.

use anyhow::{Context, Result};
use rusqlite::Connection;

pub mod aparencia;
pub mod attachments;
pub mod channels;
pub mod messages;
pub mod schema;

/// O banco de um cliente que hospeda, na pasta dele, com o nome de hoje.
///
/// Faz a mudança de nome no caminho: até a 0.7.7 o arquivo se chamava
/// `dogma.db`, e o nome vinha de Evangelion. O `seeled` já usava `seele.db`;
/// eram os dois clientes que não.
///
/// # Por que renomear em vez de só passar a abrir o novo
///
/// Porque o arquivo antigo não guarda só conversa: ele guarda o **certificado
/// TLS do servidor**, na tabela `configuracao`. Abrir um `seele.db` vazio ao
/// lado de um `dogma.db` cheio geraria uma chave nova, e todo mundo que já
/// entrou naquele servidor tomaria o alarme de pino trocado do ADR 0003 — o
/// alarme reservado a ataque, disparando por causa de uma mudança de nome.
///
/// # Os três arquivos, e não um
///
/// O `-wal` e o `-shm` andam com o banco. Renomear só o principal deixaria um
/// diário órfão, e o SQLite abriria o novo nome **sem** ele: o que estivesse no
/// diário e ainda não tivesse sido incorporado sumiria em silêncio. Ou os três
/// vão juntos, ou nenhum vai.
///
/// Não falha: se o novo já existe, ou se o antigo não existe, ou se o sistema
/// recusar a renomeação, devolve o caminho novo do mesmo jeito. Um cliente que
/// não sobe porque não conseguiu trocar um nome é pior que um cliente que sobe
/// com o banco vazio — e o antigo continua no disco, intacto, para quem quiser
/// mover à mão.
#[must_use]
pub fn banco_do_cliente(pasta: &std::path::Path) -> std::path::PathBuf {
    let novo = pasta.join("seele.db");
    let antigo = pasta.join("dogma.db");
    if novo.exists() || !antigo.exists() {
        return novo;
    }
    for sufixo in ["", "-wal", "-shm"] {
        let de = pasta.join(format!("dogma.db{sufixo}"));
        if de.exists() {
            let para = pasta.join(format!("seele.db{sufixo}"));
            if let Err(erro) = std::fs::rename(&de, &para) {
                tracing::warn!(%erro, ?de, ?para, "não deu para renomear o banco");
            }
        }
    }
    novo
}

/// Where the database lives.
#[derive(Debug, Clone)]
pub enum Location {
    /// A file on disk. `specs/04-servidor-seele.md` defaults to
    /// `/var/lib/seeled/seele.db`.
    File(std::path::PathBuf),
    /// Memory only. Tests, and nothing else: a Dogma that forgets everything on
    /// restart fails its own acceptance criterion.
    Memory,
}

/// The persistent store.
pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    /// Opens the database and brings the schema up to date.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be opened, the pragmas cannot be set, or a
    /// migration fails.
    pub fn open(location: &Location) -> Result<Self> {
        let connection = match location {
            Location::File(path) => Connection::open(path)
                .with_context(|| format!("could not open {}", path.display()))?,
            Location::Memory => Connection::open_in_memory()?,
        };

        // WAL, per specs/04. Readers do not block the writer, and a crash leaves
        // a recoverable log rather than a truncated file.
        //
        // `synchronous = NORMAL` is the WAL-appropriate setting: it fsyncs at
        // checkpoints rather than at every commit. A power cut can lose the last
        // transactions, which is the trade specs/04 already makes by batching
        // writes at all.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        // Without this a second writer fails instantly instead of waiting.
        connection.busy_timeout(std::time::Duration::from_secs(5))?;

        let mut persistence = Self { connection };
        persistence.migrate()?;
        Ok(persistence)
    }

    /// Borrows the connection.
    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Borrows the connection mutably, for a transaction.
    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// The schema version currently on disk.
    ///
    /// # Errors
    ///
    /// Fails if the version table cannot be read.
    pub fn schema_version(&self) -> Result<i64> {
        let version: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(version)
    }

    /// Applies every migration above the current version.
    ///
    /// Each runs in its own transaction, so a failure leaves the database at the
    /// last version that fully succeeded rather than half-way through one.
    fn migrate(&mut self) -> Result<()> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version    INTEGER PRIMARY KEY,
                 applied_at INTEGER NOT NULL
             ) STRICT;",
        )?;

        let current = self.schema_version()?;
        for migration in schema::MIGRATIONS {
            if migration.version <= current {
                continue;
            }
            let transaction = self.connection.transaction()?;
            transaction
                .execute_batch(migration.sql)
                .with_context(|| format!("migration {} failed", migration.version))?;
            transaction.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![migration.version, now_seconds()],
            )?;
            transaction.commit()?;
            tracing::info!(
                version = migration.version,
                description = migration.description,
                "migration applied"
            );
        }
        Ok(())
    }
}

/// Seconds since the Unix epoch.
///
/// One helper rather than a call at each site, so every timestamp in the
/// database means the same thing.
#[must_use]
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Persistence {
        Persistence::open(&Location::Memory).expect("in-memory database")
    }

    #[test]
    fn a_fresh_database_reaches_the_latest_version() {
        let persistence = memory();
        let expected = schema::MIGRATIONS
            .last()
            .map_or(0, |migration| migration.version);
        assert_eq!(persistence.schema_version().unwrap(), expected);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        // The boot path runs this every start. A second run must not fail and
        // must not apply anything again.
        let path = tempfile::tempdir().unwrap();
        let file = Location::File(path.path().join("dogma.db"));

        let first = Persistence::open(&file).unwrap();
        let version = first.schema_version().unwrap();
        drop(first);

        let second = Persistence::open(&file).unwrap();
        assert_eq!(second.schema_version().unwrap(), version);
    }

    #[test]
    fn the_four_default_roles_exist() {
        // specs/04-servidor-seele.md: Comandante, Operador, Pessoa, Observador.
        let persistence = memory();
        let mut statement = persistence
            .connection()
            .prepare("SELECT name FROM roles ORDER BY id")
            .unwrap();
        let names: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(names, vec!["Commander", "Operator", "Person", "Observer"]);
    }

    #[test]
    fn an_observer_cannot_speak_and_a_person_can() {
        // The role definitions are data, so they deserve the same scrutiny as
        // code. specs/04 gives Observador "só ouvir e ler".
        let persistence = memory();
        let permissions = |name: &str| -> String {
            persistence
                .connection()
                .query_row(
                    "SELECT permissions FROM roles WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert!(permissions("Person").contains("Speak"));
        assert!(!permissions("Observer").contains("Speak"));
        assert!(permissions("Observer").contains("ReadLine"));
        assert!(!permissions("Observer").contains("WriteLine"));
    }

    #[test]
    fn the_three_roles_that_may_speak_may_also_attach_and_the_observer_is_denied() {
        // ADR 0027 puts `AttachFile` on Commander, Operator and Person, and
        // denies it on Observer **explicitly**. The difference between denying
        // and omitting is the whole of "negadas vencem concedidas": an omission
        // does nothing when the same person also holds Person.
        let persistence = memory();
        let column = |role: &str, column: &str| -> String {
            persistence
                .connection()
                .query_row(
                    &format!("SELECT {column} FROM roles WHERE name = ?1"),
                    [role],
                    |row| row.get(0),
                )
                .unwrap()
        };
        for role in ["Commander", "Operator", "Person"] {
            assert!(
                column(role, "permissions").contains("AttachFile"),
                "{role} may write and may not attach"
            );
        }
        assert!(!column("Observer", "permissions").contains("AttachFile"));
        assert!(
            column("Observer", "denials").contains("AttachFile"),
            "the Observer is merely missing the permission, so holding Person \
             alongside would grant it back"
        );
    }

    #[test]
    fn a_dogma_that_predates_attachments_gets_the_permission_at_the_next_boot() {
        // The upgrade path, which is the case that matters: the roles are JSON
        // inside a column, so the thirteenth variant is free in the code and
        // costs a migration in the database. Without this, nobody on a Dogma
        // that already exists could ever send a file.
        let directory = tempfile::tempdir().unwrap();
        let file = Location::File(directory.path().join("dogma.db"));

        {
            let persistence = Persistence::open(&file).unwrap();
            // Rewind to exactly what migration 1 left behind, and forget that
            // migration 3 ever ran.
            //
            // `>= 3`, and not `= 3`, and this cost a green test once. `migrate`
            // asks the database for its version with `MAX(version)` and skips
            // everything at or below it, so deleting the row for 3 while 4 sits
            // beside it rewinds nothing at all: the maximum is still 4 and
            // migration 3 is skipped exactly as before. The rewind has to forget
            // **every** version above the one being replayed, or this test goes
            // on passing by never running the thing it is about.
            //
            // Found when migration 4 landed (ADR 0030). Every migration after 3
            // has to drop its own tables here too, for the same reason
            // `attachments` is dropped: the replay re-runs their `CREATE TABLE`.
            persistence
                .connection()
                .execute_batch(
                    "UPDATE roles SET permissions = replace(permissions, ',\"AttachFile\"', ''),
                                      denials     = replace(denials,     ',\"AttachFile\"', '');
                     DELETE FROM schema_version WHERE version >= 3;
                     DROP TABLE attachments;
                     DROP TABLE portaria;

                     -- A parte da migração 5, desfeita pela mesma regra que o
                     -- comentário acima escreve. Ela não cria tabela: renomeia.
                     -- Então o que a rebobinagem tem de desfazer é o rename, ou
                     -- o replay encontra `people` onde procura `pilots` e para
                     -- com «no such table» — que foi exatamente o que aconteceu
                     -- quando a 5 entrou.
                     ALTER TABLE people RENAME TO pilots;
                     ALTER TABLE person_roles RENAME TO pilot_roles;
                     ALTER TABLE pilot_roles RENAME COLUMN person_id TO pilot_id;
                     ALTER TABLE bans RENAME COLUMN person_id TO pilot_id;
                     DROP INDEX IF EXISTS bans_by_person;
                     UPDATE roles SET name = 'Pilot' WHERE name = 'Person';
                     UPDATE roles SET permissions = replace(permissions, 'MovePerson', 'MovePilot');
                     UPDATE roles SET denials     = replace(denials,     'MovePerson', 'MovePilot');",
                )
                .unwrap();
            // `'Pilot'` e não `'Person'`: aqui o banco está rebobinado para
            // antes da 5, e o papel ainda carrega o nome velho.
            assert!(!persistence
                .connection()
                .query_row::<String, _, _>(
                    "SELECT permissions FROM roles WHERE name = 'Pilot'",
                    [],
                    |row| row.get(0)
                )
                .unwrap()
                .contains("AttachFile"));
        }

        let persistence = Persistence::open(&file).unwrap();
        let person: String = persistence
            .connection()
            .query_row(
                "SELECT permissions FROM roles WHERE name = 'Person'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            person.contains("AttachFile"),
            "the old Dogma was left behind"
        );
    }

    #[test]
    fn the_permission_is_not_added_twice_by_a_second_boot() {
        // `migrate` skips applied versions, so this can only happen if somebody
        // reruns the statement — but a migration that is not idempotent is a
        // migration nobody may ever replay, and the guard is one clause.
        let persistence = memory();
        persistence
            .connection()
            .execute_batch(
                schema::MIGRATIONS[2]
                    .sql
                    .rsplit_once("CREATE INDEX attachments_by_message ON attachments (message_id);")
                    .map_or("", |(_, rest)| rest),
            )
            .unwrap();
        let person: String = persistence
            .connection()
            .query_row(
                "SELECT permissions FROM roles WHERE name = 'Person'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(person.matches("AttachFile").count(), 1);
    }

    #[test]
    fn write_ahead_logging_is_on() {
        // specs/04-servidor-seele.md asks for it by name. Without WAL a reader
        // blocks the writer, and the Cage tasks read while messages are written.
        let path = tempfile::tempdir().unwrap();
        let persistence = Persistence::open(&Location::File(path.path().join("dogma.db"))).unwrap();
        let mode: String = persistence
            .connection()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn foreign_keys_are_enforced() {
        // Off by default in SQLite. A message pointing at a deleted Line would
        // otherwise survive happily and surface as a crash somewhere else.
        let persistence = memory();
        let result = persistence.connection().execute(
            "INSERT INTO messages (line_id, author_id, body, created_at)
             VALUES (999, 999, 'orphan', 0)",
            [],
        );
        assert!(result.is_err(), "an orphan message was accepted");
    }

    #[test]
    fn a_nickname_cannot_be_taken_twice() {
        let persistence = memory();
        let insert = |nick: &str, key: &[u8]| {
            persistence.connection().execute(
                "INSERT INTO people (nickname, public_key, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![nick, key, now_seconds()],
            )
        };
        assert!(insert("ayanami", &[1; 32]).is_ok());
        assert!(insert("ayanami", &[2; 32]).is_err(), "duplicate nickname");
        assert!(insert("shinji", &[1; 32]).is_err(), "duplicate public key");
    }
}
