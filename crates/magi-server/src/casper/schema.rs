//! Embedded migrations.
//!
//! `specs/04-servidor-magi.md`: "migrations embedded in the binary, applied at
//! boot, versioned and irreversible."
//!
//! **Irreversible** is the load-bearing word. There is no `down` step and there
//! never will be: a rollback that runs against a database somebody has already
//! written to loses their messages, and the safe recovery from a bad migration
//! is a backup, not a reverse migration nobody tested.
//!
//! # Table names are in English
//!
//! `specs/04-servidor-magi.md` lists them in Portuguese — `pilotos`, `papeis`,
//! `linhas`. `specs/10-convencoes.md` says "code, identifiers, types and
//! comments: English", and a schema is code. The names below follow the
//! bilingual glossary in `docs/glossario.md`, which `specs/10` makes normative:
//! `Piloto` → `Pilot`, `Linha` → `Line`, and `Cage` stays `Cage`.
//!
//! Worth correcting in `specs/04` so the two documents stop disagreeing.

/// One migration: a version and the SQL that reaches it.
pub struct Migration {
    /// Schema version this migration produces.
    pub version: i64,
    /// What it is for, in one line. Ends up in the log at boot.
    pub description: &'static str,
    /// The statements. Applied inside a transaction.
    pub sql: &'static str,
}

/// Every migration, in order.
///
/// **Append only once shipped.** Editing a migration that has reached a real
/// database means two installations claiming the same version with different
/// shapes, which is worse than any mistake the edit would fix. Before the first
/// release there is no such database, so migration 1 is still editable — and
/// saying so precisely is better than a rule nobody believes.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "initial schema: pilots, roles, cages, lines, messages, bans",
    sql: r#"
        -- specs/04-servidor-magi.md, domain model:
        --   Dogma (the instance)
        --    ├─ Cage    — voice channel
        --    ├─ Line    — text channel
        --    ├─ Pilot   — user account
        --    └─ Role    — set of permissions

        CREATE TABLE pilots (
            id           INTEGER PRIMARY KEY,
            nickname     TEXT    NOT NULL UNIQUE,
            -- ADR 0004: identity is an Ed25519 public key, 32 bytes.
            public_key   BLOB    NOT NULL UNIQUE,
            created_at   INTEGER NOT NULL,
            last_seen_at INTEGER
        ) STRICT;

        CREATE TABLE roles (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL UNIQUE,
            -- Permissions as JSON arrays of names rather than bitmasks.
            -- specs/04 has twelve today and expects more; a bitmask would make
            -- every addition a migration and every dump unreadable.
            permissions TEXT    NOT NULL,
            -- specs/04-servidor-magi.md: "permissões negadas vencem concedidas".
            -- That sentence is empty without an explicit denial to win with:
            -- a model of grants alone has nothing for a denial to beat. A role
            -- may therefore both grant and deny, and denial wins across every
            -- role a pilot holds.
            denials     TEXT    NOT NULL DEFAULT '[]'
        ) STRICT;

        CREATE TABLE pilot_roles (
            pilot_id INTEGER NOT NULL REFERENCES pilots(id) ON DELETE CASCADE,
            role_id  INTEGER NOT NULL REFERENCES roles(id)  ON DELETE CASCADE,
            PRIMARY KEY (pilot_id, role_id)
        ) STRICT;

        CREATE TABLE cages (
            id             INTEGER PRIMARY KEY,
            name           TEXT    NOT NULL,
            member_limit   INTEGER NOT NULL,
            -- specs/08-seguranca.md forbids home-made cryptography, so a Cage
            -- password is stored hashed by the same primitive as anything else.
            password_hash  TEXT,
            minimum_role   INTEGER REFERENCES roles(id),
            -- specs/04: a Cage may have an associated Line, but need not.
            line_id        INTEGER REFERENCES lines(id),
            position       INTEGER NOT NULL DEFAULT 0
        ) STRICT;

        CREATE TABLE lines (
            id                 INTEGER PRIMARY KEY,
            name               TEXT    NOT NULL,
            minimum_read_role  INTEGER REFERENCES roles(id),
            minimum_write_role INTEGER REFERENCES roles(id),
            position           INTEGER NOT NULL DEFAULT 0
        ) STRICT;

        CREATE TABLE messages (
            id                INTEGER PRIMARY KEY,
            line_id           INTEGER NOT NULL REFERENCES lines(id) ON DELETE CASCADE,
            author_id         INTEGER NOT NULL REFERENCES pilots(id),
            body              TEXT    NOT NULL,
            created_at        INTEGER NOT NULL,
            edited_at         INTEGER,
            deleted_at        INTEGER,
            replies_to        INTEGER REFERENCES messages(id),
            -- specs/02-protocolo.md makes the send idempotent by this. Gap G9:
            -- the field was named in the notes and missing from the payload.
            client_message_id INTEGER
        ) STRICT;

        -- specs/04-servidor-magi.md: 'index on (linha_id, criado_em) for cursor
        -- pagination'. Descending, because history is read newest-first.
        CREATE INDEX messages_by_line_time ON messages (line_id, created_at DESC, id DESC);

        -- The idempotency key only has to be unique per author, and only when
        -- present: a partial index keeps NULLs out of the way.
        CREATE UNIQUE INDEX messages_idempotency
            ON messages (author_id, client_message_id)
            WHERE client_message_id IS NOT NULL;

        CREATE TABLE bans (
            id          INTEGER PRIMARY KEY,
            pilot_id    INTEGER NOT NULL REFERENCES pilots(id) ON DELETE CASCADE,
            issued_by   INTEGER NOT NULL REFERENCES pilots(id),
            reason      TEXT,
            created_at  INTEGER NOT NULL,
            -- NULL means permanent.
            expires_at  INTEGER
        ) STRICT;

        CREATE INDEX bans_by_pilot ON bans (pilot_id, expires_at);

        CREATE TABLE config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) STRICT;

        -- specs/04-servidor-magi.md names the four defaults.
        INSERT INTO roles (id, name, permissions, denials) VALUES
            (1, 'Commander', '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine","RemoveMessage","MovePilot","Kick","Ban","ManageCages","ManageRoles","AdministerDogma"]', '[]'),
            (2, 'Operator',  '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine","RemoveMessage","MovePilot","Kick","Ban"]', '[]'),
            (3, 'Pilot',     '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine"]', '[]'),
            -- specs/04: Observador is "só ouvir e ler". The denial is explicit
            -- rather than merely absent, so that granting somebody Observer
            -- alongside Pilot silences them instead of quietly doing nothing.
            (4, 'Observer',  '["ViewCage","InsertPlug","ReadLine"]', '["Speak","WriteLine"]');
    "#,
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_start_at_one_and_never_skip() {
        // A gap would make the runner's 'apply everything above the current
        // version' silently correct and impossible to reason about.
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                migration.version,
                index as i64 + 1,
                "migration {} is out of order",
                migration.description
            );
        }
    }

    #[test]
    fn every_migration_says_what_it_is_for() {
        assert!(MIGRATIONS.iter().all(|m| !m.description.is_empty()));
        assert!(MIGRATIONS.iter().all(|m| !m.sql.trim().is_empty()));
    }

    #[test]
    fn no_migration_contains_a_down_step() {
        // specs/04-servidor-magi.md: irreversible. A rollback run against a
        // database somebody has written to loses their messages, and the safe
        // recovery from a bad migration is a backup.
        for migration in MIGRATIONS {
            let sql = migration.sql.to_ascii_uppercase();
            assert!(!sql.contains("DROP TABLE"), "{}", migration.description);
            assert!(!sql.contains("DROP COLUMN"), "{}", migration.description);
        }
    }
}
