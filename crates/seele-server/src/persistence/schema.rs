//! Embedded migrations.
//!
//! `specs/04-servidor-seele.md`: "migrations embedded in the binary, applied at
//! boot, versioned and irreversible."
//!
//! **Irreversible** is the load-bearing word. There is no `down` step and there
//! never will be: a rollback that runs against a database somebody has already
//! written to loses their messages, and the safe recovery from a bad migration
//! is a backup, not a reverse migration nobody tested.
//!
//! # Table names are in English
//!
//! `specs/04-servidor-seele.md` lists them in Portuguese — `pilotos`, `papeis`,
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
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema: pilots, roles, cages, lines, messages, bans",
        sql: r#"
        -- specs/04-servidor-seele.md, domain model:
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
            -- specs/04-servidor-seele.md: "permissões negadas vencem concedidas".
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

        -- specs/04-servidor-seele.md asks for an index on (linha_id, criado_em)
        -- for cursor pagination. Two indexes rather than one, because they serve
        -- different questions:
        --
        --   by_line_id    pagination. The cursor is a message id, and ids are
        --                 monotonic while created_at is a wall clock that can
        --                 tie or step backwards. Paginating by time would drop
        --                 or repeat messages whenever it did.
        --   by_line_time  retention sweeps, which genuinely ask about time.
        CREATE INDEX messages_by_line_id ON messages (line_id, id DESC);
        CREATE INDEX messages_by_line_time ON messages (line_id, created_at);

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

        -- specs/04-servidor-seele.md names the four defaults.
        INSERT INTO roles (id, name, permissions, denials) VALUES
            (1, 'Commander', '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine","RemoveMessage","MovePilot","Kick","Ban","ManageCages","ManageRoles","AdministerDogma"]', '[]'),
            (2, 'Operator',  '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine","RemoveMessage","MovePilot","Kick","Ban"]', '[]'),
            (3, 'Pilot',     '["ViewCage","InsertPlug","Speak","ReadLine","WriteLine"]', '[]'),
            -- specs/04: Observador is "só ouvir e ler". The denial is explicit
            -- rather than merely absent, so that granting somebody Observer
            -- alongside Pilot silences them instead of quietly doing nothing.
            (4, 'Observer',  '["ViewCage","InsertPlug","ReadLine"]', '["Speak","WriteLine"]');
    "#,
    },
    Migration {
        version: 2,
        description: "admissão no Dogma: senha e convites de uso único",
        sql: r#"
            -- specs/08-seguranca.md fechava com [EM ABERTO — escolher em M2]:
            -- "chave pública como mecanismo primário, com convite por token de
            -- uso único para entrada em um Dogma. Senha como fallback opcional
            -- configurável pelo operador." É isto.

            -- Configuração do Dogma que não cabe num arquivo, porque muda em
            -- tempo de execução e precisa sobreviver a reinício.
            -- `ANY` porque aqui cabem tanto o hash da senha (texto) quanto o
            -- certificado e a chave TLS (bytes). STRICT permite `ANY`, e é
            -- melhor que uma segunda tabela só pela diferença de tipo.
            CREATE TABLE configuracao (
                chave TEXT PRIMARY KEY,
                valor ANY  NOT NULL
            ) STRICT;

            CREATE TABLE convites (
                token      TEXT PRIMARY KEY,
                criado_em  INTEGER NOT NULL,
                expira_em  INTEGER NOT NULL,
                -- NULL enquanto não usado. O UPDATE de consumo condiciona a
                -- esta coluna ser NULL, e é isso que impede dois clientes de
                -- gastarem o mesmo convite ao mesmo tempo.
                usado_em   INTEGER,
                -- Para quem o operador mandou. Só para ele se lembrar.
                observacao TEXT NOT NULL DEFAULT ''
            ) STRICT;

            -- A varredura de convites vencidos e a listagem para o operador
            -- olham por prazo, não por token.
            CREATE INDEX convites_por_prazo ON convites(expira_em) WHERE usado_em IS NULL;
        "#,
    },
    Migration {
        version: 3,
        description: "attachments: rows that outlive their bytes (ADR 0027)",
        sql: r#"
            -- ADR 0027. The bytes live in `anexos/`, beside the database and
            -- not inside it; this table is the index over them, and it is the
            -- **truth**: a file missing from the directory reads exactly as an
            -- expired one, which is a state the design already has.
            CREATE TABLE attachments (
                id            INTEGER PRIMARY KEY,
                message_id    INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
                -- SHA-256 of the content, lowercase hex. This is also the file
                -- name on disk: no byte anybody else chose ever reaches the
                -- filesystem. Not UNIQUE — two pilots sending the same picture
                -- get two rows and one blob, which is the whole of the
                -- deduplication.
                content_hash  TEXT    NOT NULL,
                -- The name the sender gave it. A column, never a path.
                file_name     TEXT    NOT NULL,
                -- The type the sender claimed. A claim, not a fact: nothing
                -- decides what to decode from this alone.
                declared_type TEXT    NOT NULL,
                byte_size     INTEGER NOT NULL,
                created_at    INTEGER NOT NULL,
                -- NULL while the bytes exist. Expiring stamps this and deletes
                -- the blob; the row stays so the message can still say that a
                -- file was here, with its name and its size. A row deleted
                -- instead would render as a message with nothing in it, and
                -- nobody would know there had been a file at all.
                --
                -- The consequence is written in the ADR and accepted: this
                -- table never loses rows, only bytes, so it grows forever.
                expired_at    INTEGER
            ) STRICT;

            -- Eviction asks one question — "which live attachment is the
            -- oldest" — and asks it on the way in to every transfer.
            CREATE INDEX attachments_oldest_live
                ON attachments (created_at, id) WHERE expired_at IS NULL;
            -- "How many live rows still point at these bytes." The answer is
            -- what decides whether deleting a row may delete the blob.
            CREATE INDEX attachments_by_hash ON attachments (content_hash);
            -- Drawing a page of history joins this per message.
            CREATE INDEX attachments_by_message ON attachments (message_id);

            -- `Permission::AttachFile` is the thirteenth variant, and the
            -- thirteenth variant is mechanical in the code and **not**
            -- mechanical in the database: the seeded roles are JSON inside a
            -- column, and a Dogma that already exists needs its rows brought
            -- forward or nobody there may ever send a file.
            --
            -- Not folded into WriteLine. "May write" and "may put a gigabyte on
            -- my laptop" are separate questions, and answering the second one
            -- separately is most of the point of hosting for your own friends.
            --
            -- Written against the roles that still carry their seeded names
            -- rather than against identifiers, and guarded by `NOT EXISTS`, so
            -- that this is idempotent and so that a role somebody rebuilt by
            -- hand is not overwritten by a migration.
            UPDATE roles SET permissions = json_insert(permissions, '$[#]', 'AttachFile')
             WHERE name IN ('Commander', 'Operator', 'Pilot')
               AND NOT EXISTS (
                   SELECT 1 FROM json_each(roles.permissions) WHERE value = 'AttachFile'
               );

            -- Denied on purpose, and not merely absent. Migration 1 wrote the
            -- reason on the Observer's line and it holds here word for word:
            -- an explicit denial is what makes granting Observer to somebody
            -- who is also a Pilot take the ability away, instead of quietly
            -- doing nothing. `specs/04-servidor-seele.md`: negadas vencem
            -- concedidas — a sentence that is empty without a denial to win
            -- with.
            UPDATE roles SET denials = json_insert(denials, '$[#]', 'AttachFile')
             WHERE name = 'Observer'
               AND NOT EXISTS (
                   SELECT 1 FROM json_each(roles.denials) WHERE value = 'AttachFile'
               );
        "#,
    },
    Migration {
        version: 4,
        description: "portaria: TOFU aplicado a gente, uma linha por impressão digital (ADR 0030)",
        sql: r#"
            -- ADR 0030. A terceira camada de admissão, e a única que decide
            -- sobre **gente** em vez de sobre um segredo: quem hospeda vê quem
            -- bateu e escolhe.

            -- A chave é a impressão digital, e não `pilot_id`, de propósito.
            --
            -- A conta é consequência da chave — `register_or_find` a cria a
            -- partir dela — e uma decisão sobre quem entra tem que sobreviver à
            -- conta ser apagada, renomeada ou recriada. É também o que a pessoa
            -- que aprova está olhando na tela, então é o que fica gravado: o que
            -- se decide e o que se guarda são a mesma string.
            --
            -- Sem FOREIGN KEY para `pilots` pelo mesmo motivo. Um pedido de
            -- alguém que nunca virou conta é um estado normal aqui, não órfão.
            CREATE TABLE portaria (
                impressao   TEXT PRIMARY KEY,

                -- 'pendente' | 'admitido' | 'recusado'.
                --
                -- Texto e não inteiro porque este banco é lido à mão pelo dono
                -- da máquina no dia em que a tela não bastar, e é ele quem tem a
                -- autoridade final sobre a própria porta.
                veredito    TEXT    NOT NULL,

                -- O apelido **pedido** na batida, guardado como foi digitado.
                --
                -- Não é identidade e esta coluna não finge que seja: ela existe
                -- para quem hospeda reconhecer o pedido, e a linha de cima do
                -- cartão continua sendo a impressão. Congelado no momento da
                -- batida em vez de lido de `pilots` na hora de mostrar, porque o
                -- que se decidiu foi sobre o que estava escrito ali.
                apelido     TEXT    NOT NULL DEFAULT '',

                -- Com que segredo chegou: 'aberto' | 'senha' | 'convite'.
                -- Prova exibida a quem decide, nunca decisão por si — ADR 0030
                -- recusa aprovar sozinho quem traz convite válido.
                segredo     TEXT    NOT NULL DEFAULT 'aberto',

                -- A observação que quem hospeda escreveu ao gerar o convite.
                -- `criar_convite` já guardava este campo e nada o lia; «chegou
                -- com o convite *para o Rafael*» é a melhor prova que existe.
                observacao  TEXT    NOT NULL DEFAULT '',

                bateu_em    INTEGER NOT NULL,
                -- Quantas vezes bateu. Tentar de novo é o caminho normal quando
                -- o pedido está pendente, e não deve virar uma fila de linhas.
                batidas     INTEGER NOT NULL DEFAULT 1,
                -- NULL enquanto ninguém decidiu.
                decidido_em INTEGER
            ) STRICT;

            -- A fila que a tela desenha: pendentes, mais antigo primeiro.
            CREATE INDEX portaria_pendentes
                ON portaria (bateu_em) WHERE decidido_em IS NULL;

            -- O interruptor. Ausente = desligada, que é o comportamento de
            -- antes desta migração — um Dogma que já existe não muda de
            -- comportamento por ter sido migrado.
            --
            -- O `seeled` continua subindo sem portaria (ADR 0021 mantém o padrão
            -- aberto, e este ADR não mexe nele). Quem liga é o botão HOSPEDAR
            -- AQUI, na primeira vez que sobe um Dogma, porque quem apertou um
            -- botão não aceitou cerimônia nenhuma.
        "#,
    },
    Migration {
        version: 5,
        description: "vocabulário: Pilot vira Person em tabela, coluna, papel e permissão",
        sql: r#"
            -- O vocabulário de Evangelion sai do código — o ADR 0033 já o tinha
            -- tirado da tela —, e `Pilot` era o nome da conta de uma pessoa. O
            -- lado Rust já mudou; esta migração é o outro lado do mesmo rename,
            -- para que um banco que já existe continue casando com as consultas.
            --
            -- **Migração nova em vez de editar a 1**, que é a regra escrita no
            -- alto deste arquivo: a 1 já chegou a banco de verdade, e editá-la
            -- faria duas instalações reivindicarem a mesma versão com formas
            -- diferentes. Uma varredura de renomeação a editou por acidente em
            -- 2026-08-24; foi revertida, e o que ela queria fazer está aqui.

            ALTER TABLE pilots RENAME TO people;
            ALTER TABLE pilot_roles RENAME TO person_roles;
            ALTER TABLE person_roles RENAME COLUMN pilot_id TO person_id;
            ALTER TABLE bans RENAME COLUMN pilot_id TO person_id;

            -- SQLite não renomeia índice; o caminho é derrubar e refazer.
            -- Barato: `bans` é pequena por construção. `DROP INDEX` não é passo
            -- de volta — não perde linha nenhuma —, e por isso não cai na regra
            -- que `no_migration_contains_a_down_step` cobra.
            DROP INDEX IF EXISTS bans_by_pilot;
            CREATE INDEX bans_by_person ON bans (person_id, expires_at);

            -- Dados semeados pela migração 1, gravados em todo banco existente.
            -- `Pilot` é o papel de quem só conversa, e o nome dele aparece na
            -- tela de papéis.
            UPDATE roles SET name = 'Person' WHERE name = 'Pilot';

            -- `permissions` e `denials` são arrays JSON de **nomes**, então o
            -- rename do enum em Rust não chega sozinho até aqui: sem isto um
            -- banco antigo guarda "MovePilot", o código procura "MovePerson", e
            -- a permissão de mover alguém some sem erro nenhum.
            UPDATE roles SET permissions = replace(permissions, 'MovePilot', 'MovePerson');
            UPDATE roles SET denials     = replace(denials,     'MovePilot', 'MovePerson');
        "#,
    },
    Migration {
        version: 6,
        description: "vocabulário: Cage vira VoiceRoom em tabela e permissão",
        sql: r#"
            -- Segundo lado do mesmo rename da migração 5, agora para o `Cage`,
            -- que era o termo de Evangelion para a sala de voz. A tela já dizia
            -- "sala de voz" desde o ADR 0033.

            ALTER TABLE cages RENAME TO voice_rooms;

            -- Os nomes de permissão são gravados como **texto** dentro dos
            -- arrays JSON de `roles`, então o rename do enum em Rust não chega
            -- aqui sozinho — foi assim que "MovePilot" quase sumiu na migração
            -- anterior. `ManageCages` antes de `ViewCage`: trocar o mais curto
            -- primeiro deixaria `ManageVoiceRooms` impossível de casar depois.
            UPDATE roles SET permissions = replace(permissions, 'ManageCages', 'ManageVoiceRooms');
            UPDATE roles SET denials     = replace(denials,     'ManageCages', 'ManageVoiceRooms');
            UPDATE roles SET permissions = replace(permissions, 'ViewCage', 'ViewVoiceRoom');
            UPDATE roles SET denials     = replace(denials,     'ViewCage', 'ViewVoiceRoom');
        "#,
    },
    Migration {
        version: 7,
        description: "vocabulário: Dogma vira Server na permissão gravada",
        sql: r#"
            -- Terceiro lado do rename das migrações 5 e 6. `Dogma` era o termo
            -- de Evangelion para o próprio servidor, e a única aparição dele em
            -- **dado gravado** é o nome da permissão dentro dos arrays JSON de
            -- `roles` — o resto era tipo, campo e comentário, que o Rust
            -- resolveu sozinho.
            UPDATE roles SET permissions = replace(permissions, 'AdministerDogma', 'AdministerServer');
            UPDATE roles SET denials     = replace(denials,     'AdministerDogma', 'AdministerServer');
        "#,
    },
];

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
        // specs/04-servidor-seele.md: irreversible. A rollback run against a
        // database somebody has written to loses their messages, and the safe
        // recovery from a bad migration is a backup.
        for migration in MIGRATIONS {
            let sql = migration.sql.to_ascii_uppercase();
            assert!(!sql.contains("DROP TABLE"), "{}", migration.description);
            assert!(!sql.contains("DROP COLUMN"), "{}", migration.description);
        }
    }
}
