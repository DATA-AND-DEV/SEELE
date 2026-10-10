PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE schema_version (
                 version    INTEGER PRIMARY KEY,
                 applied_at INTEGER NOT NULL
             ) STRICT;
INSERT INTO schema_version VALUES(1,1791646837);
INSERT INTO schema_version VALUES(2,1791646837);
INSERT INTO schema_version VALUES(3,1791646837);
INSERT INTO schema_version VALUES(4,1791646837);
INSERT INTO schema_version VALUES(5,1791646837);
INSERT INTO schema_version VALUES(6,1791646837);
INSERT INTO schema_version VALUES(7,1791646837);
INSERT INTO schema_version VALUES(8,1791646837);
INSERT INTO schema_version VALUES(9,1791646837);
INSERT INTO schema_version VALUES(10,1791646837);
INSERT INTO schema_version VALUES(11,1791646837);
INSERT INTO schema_version VALUES(12,1791646837);
INSERT INTO schema_version VALUES(13,1791646837);
CREATE TABLE IF NOT EXISTS "people" (
            id           INTEGER PRIMARY KEY,
            nickname     TEXT    NOT NULL UNIQUE,
            -- ADR 0004: identity is an Ed25519 public key, 32 bytes.
            public_key   BLOB    NOT NULL UNIQUE,
            created_at   INTEGER NOT NULL,
            last_seen_at INTEGER
        , icon BLOB) STRICT;
INSERT INTO people VALUES(1,'dono',x'5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e',1791646837,1791646837,NULL);
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
INSERT INTO roles VALUES(1,'Commander','["ViewVoiceRoom","EnterVoiceRoom","Speak","ReadChannel","WriteChannel","RemoveMessage","MovePerson","Kick","Ban","ManageVoiceRooms","ManageRoles","AdministerServer","AttachFile"]','[]');
INSERT INTO roles VALUES(2,'Operator','["ViewVoiceRoom","EnterVoiceRoom","Speak","ReadChannel","WriteChannel","RemoveMessage","MovePerson","Kick","Ban","AttachFile"]','[]');
INSERT INTO roles VALUES(3,'Person','["ViewVoiceRoom","EnterVoiceRoom","Speak","ReadChannel","WriteChannel","AttachFile"]','[]');
INSERT INTO roles VALUES(4,'Observer','["ViewVoiceRoom","EnterVoiceRoom","ReadChannel"]','["Speak","WriteChannel","AttachFile"]');
CREATE TABLE IF NOT EXISTS "person_roles" (
            person_id INTEGER NOT NULL REFERENCES "people"(id) ON DELETE CASCADE,
            role_id  INTEGER NOT NULL REFERENCES roles(id)  ON DELETE CASCADE,
            PRIMARY KEY (person_id, role_id)
        ) STRICT;
INSERT INTO person_roles VALUES(1,1);
CREATE TABLE IF NOT EXISTS "voice_rooms" (
            id             INTEGER PRIMARY KEY,
            name           TEXT    NOT NULL,
            member_limit   INTEGER NOT NULL,
            -- specs/08-seguranca.md forbids home-made cryptography, so a Cage
            -- password is stored hashed by the same primitive as anything else.
            password_hash  TEXT,
            minimum_role   INTEGER REFERENCES roles(id),
            -- specs/04: a Cage may have an associated Line, but need not.
            channel_id        INTEGER REFERENCES "channels"(id),
            position       INTEGER NOT NULL DEFAULT 0
        ) STRICT;
CREATE TABLE IF NOT EXISTS "channels" (
            id                 INTEGER PRIMARY KEY,
            name               TEXT    NOT NULL,
            minimum_read_role  INTEGER REFERENCES roles(id),
            minimum_write_role INTEGER REFERENCES roles(id),
            position           INTEGER NOT NULL DEFAULT 0
        ) STRICT;
CREATE TABLE messages (
            id                INTEGER PRIMARY KEY,
            channel_id           INTEGER NOT NULL REFERENCES "channels"(id) ON DELETE CASCADE,
            author_id         INTEGER NOT NULL REFERENCES "people"(id),
            body              TEXT    NOT NULL,
            created_at        INTEGER NOT NULL,
            edited_at         INTEGER,
            deleted_at        INTEGER,
            replies_to        INTEGER REFERENCES messages(id),
            -- specs/02-protocolo.md makes the send idempotent by this. Gap G9:
            -- the field was named in the notes and missing from the payload.
            client_message_id INTEGER
        ) STRICT;
CREATE TABLE bans (
            id          INTEGER PRIMARY KEY,
            person_id    INTEGER NOT NULL REFERENCES "people"(id) ON DELETE CASCADE,
            issued_by   INTEGER NOT NULL REFERENCES "people"(id),
            reason      TEXT,
            created_at  INTEGER NOT NULL,
            -- NULL means permanent.
            expires_at  INTEGER
        ) STRICT;
CREATE TABLE config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) STRICT;
INSERT INTO config VALUES('commandership_claimed','1');
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
CREATE TABLE mods (
                id         TEXT PRIMARY KEY NOT NULL,
                version    TEXT NOT NULL,
                hash       TEXT NOT NULL,
                enabled    INTEGER NOT NULL DEFAULT 1,
                config     TEXT NOT NULL DEFAULT '{}'
            , repo        TEXT    NOT NULL DEFAULT '', reach       TEXT    NOT NULL DEFAULT '[]', server_half INTEGER NOT NULL DEFAULT 0);
INSERT INTO mods VALUES('seele/estilo','3.1.0','ca0b79565bbb41a79403ce69b500c62f908a080efbb9bcbd4e3486a302d21682',1,'{}','https://github.com/DATA-AND-DEV/ESTILO','["tema do servidor"]',1);
CREATE TABLE mod_data (
                mod_id     TEXT NOT NULL REFERENCES mods(id),
                key        TEXT NOT NULL,
                value      BLOB NOT NULL,
                PRIMARY KEY (mod_id, key)
            );
INSERT INTO mod_data VALUES('seele/estilo','theme',x'7b227265766973696f6e223a312c227468656d65223a7b22616363656e74223a2223326638306564222c226261636b67726f756e64223a2223306230643130222c2270616e656c223a2223313231363162222c2274657874223a2223653865646632222c226d75746564223a2223396161366232222c22626f72646572223a2223326133323363222c22726164697573223a342c2264656e73697479223a22636f6d70616374222c22666f6e74223a2273616e73222c22676c6f77223a66616c73657d2c22656e61626c6564223a747275657d');
CREATE TABLE instancia (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                identidade   TEXT    NOT NULL,
                nascida_em   INTEGER NOT NULL
            );
CREATE UNIQUE INDEX messages_idempotency
            ON messages (author_id, client_message_id)
            WHERE client_message_id IS NOT NULL;
CREATE INDEX convites_por_prazo ON convites(expira_em) WHERE usado_em IS NULL;
CREATE INDEX attachments_oldest_live
                ON attachments (created_at, id) WHERE expired_at IS NULL;
CREATE INDEX attachments_by_hash ON attachments (content_hash);
CREATE INDEX attachments_by_message ON attachments (message_id);
CREATE INDEX portaria_pendentes
                ON portaria (bateu_em) WHERE decidido_em IS NULL;
CREATE INDEX bans_by_person ON bans (person_id, expires_at);
CREATE INDEX messages_by_channel_id ON messages (channel_id, id DESC);
CREATE INDEX messages_by_channel_time ON messages (channel_id, created_at);
COMMIT;
