//! PERMISSIONS — identity, roles, permissions and bans.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > Modelo simples e enumerado, sem sistema de expressão. Cada Papel carrega um
//! > conjunto: `ver_voice_room`, `inserir_plug`, `falar`, …
//! >
//! > Papéis padrão: **Comandante** (tudo), **Operador** (moderação), **Pessoa**
//! > (uso normal), **Observador** (só ouvir e ler).
//! >
//! > Regra: permissões negadas vencem concedidas. Sem herança em árvore — a
//! > complexidade não se paga na escala alvo.
//!
//! # "Denied beats granted" needs something to deny
//!
//! A model of grants alone makes that sentence vacuous: absence is already
//! denial, so there is nothing for a denial to win against. A role here can
//! therefore both **grant** and **deny**, and a denial in any role the person
//! holds beats a grant in any other.
//!
//! It matters in exactly the case the spec's four defaults suggest: giving
//! somebody Observer alongside Person should silence them, and with grants alone
//! it would do nothing at all.
//!
//! # Every check happens here
//!
//! `specs/08-seguranca.md`: "Toda ação é verificada no servidor, sempre, mesmo
//! que o cliente já esconda o botão. A interface esconder é conveniência; o
//! servidor negar é a segurança."

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use seele_proto::control::Permission;
use seele_proto::ids::{PersonId, RoleId};

use crate::persistence::{now_seconds, Persistence};

/// The Comandante role, seeded by migration 1.
pub const COMMANDER_ROLE: RoleId = RoleId(1);
/// The Operador role.
pub const OPERATOR_ROLE: RoleId = RoleId(2);
/// The Pessoa role, which every account after the first arrives with.
pub const PERSON_ROLE: RoleId = RoleId(3);
/// The Observador role: may listen and read, and nothing else.
pub const OBSERVER_ROLE: RoleId = RoleId(4);

/// A person as PERMISSIONS knows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// Account identifier.
    pub id: PersonId,
    /// Display name.
    pub nickname: String,
    /// Roles held.
    pub roles: Vec<RoleId>,
}

/// Why an action was refused.
///
/// Enumerated, per `specs/02-protocolo.md`. A shell matches on the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The person holds no role granting it, or a role denying it.
    #[error("permission denied")]
    PermissionDenied,
    /// The person is barred from the server.
    #[error("banned")]
    Banned,
    /// No such account.
    #[error("no such person")]
    UnknownPerson,
    /// The nickname is taken by a different key.
    ///
    /// `specs/08-seguranca.md` wants uniform login failures, so this is only
    /// ever surfaced to an operator — never to the peer who triggered it.
    #[error("nickname belongs to a different identity")]
    NicknameTaken,
}

/// Parses the JSON permission arrays the schema stores.
///
/// Public because the handshake builds the role list for `Sessao` out of the
/// same column.
#[must_use]
pub fn permissions_from_json(json: &str) -> Vec<Permission> {
    parse_permissions(json)
}

/// Parses the JSON permission arrays the schema stores.
fn parse_permissions(json: &str) -> Vec<Permission> {
    serde_json::from_str::<Vec<String>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name_to_permission(name))
        .collect()
}

/// Maps a stored name to a [`Permission`].
///
/// Explicit rather than derived, so that renaming a variant does not silently
/// change what an existing database means.
fn name_to_permission(name: &str) -> Option<Permission> {
    Some(match name {
        "ViewVoiceRoom" => Permission::ViewVoiceRoom,
        "EnterVoiceRoom" => Permission::EnterVoiceRoom,
        "Speak" => Permission::Speak,
        "ReadChannel" => Permission::ReadChannel,
        "WriteChannel" => Permission::WriteChannel,
        "RemoveMessage" => Permission::RemoveMessage,
        "MovePerson" => Permission::MovePerson,
        "Kick" => Permission::Kick,
        "Ban" => Permission::Ban,
        "ManageVoiceRooms" => Permission::ManageVoiceRooms,
        "ManageRoles" => Permission::ManageRoles,
        "AdministerServer" => Permission::AdministerServer,
        "AttachFile" => Permission::AttachFile,
        _ => return None,
    })
}

/// Identity and authorisation, over PERSISTENCE.
pub struct Permissions<'a> {
    connection: &'a Connection,
}

impl<'a> Permissions<'a> {
    /// Borrows a store.
    #[must_use]
    pub fn new(persistence: &'a Persistence) -> Self {
        Self {
            connection: persistence.connection(),
        }
    }

    /// Finds the account for a public key, creating it on first sight.
    ///
    /// ADR 0004 makes the key the identity, and the nickname a label attached to
    /// it. A returning person therefore keeps their **account** whatever name
    /// they ask for, and the name they ask for becomes the label — the account
    /// survives a rename, which is the opposite of the name owning the account.
    ///
    /// A **different** key asking for a taken name is refused rather than
    /// silently given somebody else's history, and that refusal is what stops
    /// renaming from being the way to take somebody's name.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::NicknameTaken`] if the name belongs to another key, or
    /// a database error.
    pub fn register_or_find(&self, public_key: &[u8], nickname: &str) -> Result<Person> {
        if let Some(id) = self
            .connection
            .query_row(
                "SELECT id FROM people WHERE public_key = ?1",
                [public_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            self.connection.execute(
                "UPDATE people SET last_seen_at = ?1 WHERE id = ?2",
                params![now_seconds(), id],
            )?;
            // O apelido pedido passa a valer, e isto é conserto e não recurso.
            //
            // Antes, uma conta que voltava mantinha o nome com que foi criada e
            // o pedido era descartado em silêncio: quem entrou uma vez como
            // `pessoa` era `pessoa` para sempre, para todo mundo, sem nada na
            // tela dizendo por quê. Encontrado num teste entre duas máquinas —
            // a pessoa trocou de nome e a outra continuou vendo o antigo.
            //
            // O histórico acompanha sozinho: `persistence::messages` resolve o autor
            // por `JOIN people` e lê o nome de agora, em vez de guardar uma
            // cópia por mensagem. Uma mensagem antiga passa a ser exibida com o
            // nome novo, que é o que uma pessoa espera de «mudei meu nome».
            //
            // O que **não** muda é a proteção do ADR 0017: o nome continua
            // pertencendo a uma chave. Pedir um nome que é de outra pessoa é a
            // mesma recusa de sempre — a de baixo —, e não passa a ser
            // permitida por a conta já existir.
            self.rename(PersonId(id as u64), nickname)?;
            // Uma conta que já existe assume o comando **se ele nunca foi de
            // ninguém** — e essa condição não é a mesma que «está vago agora».
            //
            // O problema: contas criadas antes de o comando existir nunca
            // passaram por `seat_the_arrival`, então o assento ficava vazio para
            // sempre. Num servidor real, com histórico real, o formulário de criar
            // sala não aparecia para ninguém e não havia como fazer aparecer; a
            // única saída era entrar com um apelido nunca usado, que é uma
            // resposta absurda para «administre o seu próprio servidor».
            //
            // Por que não «vago agora»: um operador revoga o comando de alguém,
            // essa pessoa reconecta, e recuperaria o papel sozinha. A revogação
            // viraria decoração. Foi um teste deste arquivo que pegou isso.
            //
            // A marca em `config` é o que separa as duas: ela é escrita na
            // primeira vez que o assento é ocupado e nunca sai. Sem marca, o
            // servidor nunca teve Comandante; com marca, quem não tem o papel não
            // o tem por decisão de alguém.
            self.claim_never_held_commandership(id)?;
            return self.person(PersonId(id as u64));
        }

        let taken: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM people WHERE nickname = ?1",
                [nickname],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() {
            return Err(Refusal::NicknameTaken.into());
        }

        self.connection.execute(
            "INSERT INTO people (nickname, public_key, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![nickname, public_key, now_seconds()],
        )?;
        let id = self.connection.last_insert_rowid();
        self.seat_the_arrival(id)?;

        self.person(PersonId(id as u64))
    }

    /// Gives a freshly created account its opening role.
    ///
    /// # The first account created on a server becomes the Comandante
    ///
    /// Every account used to arrive as a Pessoa, which meant **nobody ever
    /// became a Comandante**. Migration 1 seeds the role with `ManageVoiceRooms`,
    /// `ManageRoles` and `AdministerServer`, and nothing granted it: a server
    /// shipped with three permissions no account in it could ever hold, so
    /// every verb behind them was unreachable by construction.
    ///
    /// Whoever hosts is whoever connects to their own server first — they press
    /// the button and then they connect, in that order, and nobody else has the
    /// address yet. That fact is enough to answer "who hosts" with no
    /// configuration file, no flag, and no question on screen, all three of
    /// which would have to be answered by the person least equipped to answer
    /// them: someone who just wants to talk to their friends tonight.
    ///
    /// # The race
    ///
    /// Two clients reaching a virgin server at the same moment must not both
    /// become Comandante, and must not both miss. The claim is therefore **one
    /// statement**, conditional on nobody holding the role yet — the same shape
    /// `crate::admissao` uses to spend an invite exactly once with
    /// `UPDATE … WHERE usado_em IS NULL`. SQLite serialises writers, so the
    /// second claim to run sees the first one's row, inserts nothing, and
    /// reports zero rows changed; that account then takes the Pessoa role like
    /// anybody else.
    ///
    /// Deliberately keyed on *the role being unheld*, not on "is this the first
    /// person row". A server whose Comandante account was deleted has no
    /// Comandante again, and the next arrival should be able to take the seat
    /// rather than leave the server permanently unadministrable.
    /// Points an account at a different display name.
    ///
    /// Does nothing when the name is already the one held — the common case, on
    /// every reconnect, and not worth a write.
    ///
    /// # Errors
    ///
    /// [`Refusal::NicknameTaken`] when the name belongs to a **different** key.
    /// ADR 0017 makes the name property of the key that claimed it, and that is
    /// the whole protection: without this check, renaming would be the way to
    /// take somebody else's name and inherit how they are addressed.
    fn rename(&self, person: PersonId, nickname: &str) -> Result<()> {
        let atual: String = self.connection.query_row(
            "SELECT nickname FROM people WHERE id = ?1",
            [person.get() as i64],
            |row| row.get(0),
        )?;
        if atual == nickname {
            return Ok(());
        }

        let dono: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM people WHERE nickname = ?1",
                [nickname],
                |row| row.get(0),
            )
            .optional()?;
        if dono.is_some_and(|dono| dono != person.get() as i64) {
            return Err(Refusal::NicknameTaken.into());
        }

        self.connection.execute(
            "UPDATE people SET nickname = ?1 WHERE id = ?2",
            params![nickname, person.get() as i64],
        )?;
        tracing::info!(person = person.get(), "this account changed its name");
        Ok(())
    }

    /// The key that remembers the seat was taken once, whoever holds it now.
    const SEAT_TAKEN: &'static str = "commandership_claimed";

    /// Takes the Comandante seat only if it has **never** been held.
    ///
    /// Runs on the reconnect path, where "empty right now" is the wrong
    /// question. An operator who revokes somebody's Comandante would see the
    /// role come back the next time that person connected, and the revocation
    /// would be decoration — a test in this file says so, and it is right.
    ///
    /// What this asks instead is whether the seat was *ever* occupied. A server
    /// that gained accounts before the commandership existed answers no, and
    /// the next arrival may take it. A server whose Comandante was demoted
    /// answers yes, forever, and nobody takes it back by reconnecting.
    ///
    /// The mark is written inside the same transaction as the claim: a claim
    /// that succeeded without leaving the mark would let the next reconnect
    /// claim again.
    fn claim_never_held_commandership(&self, person_row: i64) -> Result<bool> {
        let ja: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                [Self::SEAT_TAKEN],
                |row| row.get(0),
            )
            .optional()?;
        if ja.is_some() {
            return Ok(false);
        }
        self.claim_vacant_commandership(person_row)
    }

    /// Takes the Comandante seat if nobody holds it. Says whether it did.
    ///
    /// One statement, conditional on the role being unheld — the same shape
    /// `crate::admissao` uses to spend an invite exactly once. SQLite serialises
    /// writers, so of two clients racing for a virgin server the second sees the
    /// first's row, inserts nothing, and reports zero rows changed. Neither a
    /// check-then-insert in Rust nor two statements would survive that.
    fn claim_vacant_commandership(&self, person_row: i64) -> Result<bool> {
        let claimed = self.connection.execute(
            "INSERT INTO person_roles (person_id, role_id)
             SELECT ?1, ?2
             WHERE NOT EXISTS (SELECT 1 FROM person_roles WHERE role_id = ?2)",
            params![person_row, i64::from(COMMANDER_ROLE.get())],
        )?;
        if claimed > 0 {
            // A marca, e não só o papel: ela é o que distingue «nunca teve
            // Comandante» de «teve e não tem mais», e o caminho de reconexão
            // depende dessa diferença.
            self.connection.execute(
                "INSERT OR IGNORE INTO config (key, value) VALUES (?1, '1')",
                [Self::SEAT_TAKEN],
            )?;
            tracing::info!(person = person_row, "this account took the commandership");
        }
        Ok(claimed > 0)
    }

    fn seat_the_arrival(&self, person_row: i64) -> Result<()> {
        if self.claim_vacant_commandership(person_row)? {
            return Ok(());
        }

        // Everybody after the first arrives as a Person. specs/04 makes that the
        // normal-use role; a Comandante promotes from there.
        self.connection.execute(
            "INSERT INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
            params![person_row, i64::from(PERSON_ROLE.get())],
        )?;
        Ok(())
    }

    /// Loads one account.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::UnknownPerson`] if there is no such account.
    pub fn person(&self, id: PersonId) -> Result<Person> {
        let nickname: String = self
            .connection
            .query_row(
                "SELECT nickname FROM people WHERE id = ?1",
                [id.get() as i64],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(Refusal::UnknownPerson)?;

        let mut statement = self
            .connection
            .prepare("SELECT role_id FROM person_roles WHERE person_id = ?1 ORDER BY role_id")?;
        let roles = statement
            .query_map([id.get() as i64], |row| row.get::<_, i64>(0))?
            .filter_map(Result::ok)
            .map(|role| RoleId(role as u32))
            .collect();

        Ok(Person {
            id,
            nickname,
            roles,
        })
    }

    /// Grants a role.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn grant_role(&self, person: PersonId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
            params![person.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Revokes a role.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn revoke_role(&self, person: PersonId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "DELETE FROM person_roles WHERE person_id = ?1 AND role_id = ?2",
            params![person.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Whether a person may do something.
    ///
    /// Denial in any role beats a grant in any other, and a ban beats
    /// everything. `specs/08-seguranca.md` requires this to be consulted for
    /// every action, whether or not the client already hid the button.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn may(&self, person: PersonId, permission: Permission) -> Result<bool> {
        if self.is_banned(person)? {
            return Ok(false);
        }

        let mut statement = self.connection.prepare(
            "SELECT r.permissions, r.denials
             FROM roles r
             JOIN person_roles pr ON pr.role_id = r.id
             WHERE pr.person_id = ?1",
        )?;
        let rows: Vec<(String, String)> = statement
            .query_map([person.get() as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(Result::ok)
            .collect();

        let mut granted = false;
        for (permissions, denials) in rows {
            // A denial anywhere ends it. specs/04: "negadas vencem concedidas".
            if parse_permissions(&denials).contains(&permission) {
                return Ok(false);
            }
            if parse_permissions(&permissions).contains(&permission) {
                granted = true;
            }
        }
        Ok(granted)
    }

    /// Every permission a person currently holds.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn permissions(&self, person: PersonId) -> Result<Vec<Permission>> {
        let all = [
            Permission::ViewVoiceRoom,
            Permission::EnterVoiceRoom,
            Permission::Speak,
            Permission::ReadChannel,
            Permission::WriteChannel,
            Permission::RemoveMessage,
            Permission::MovePerson,
            Permission::Kick,
            Permission::Ban,
            Permission::ManageVoiceRooms,
            Permission::ManageRoles,
            Permission::AdministerServer,
            Permission::AttachFile,
        ];
        let mut held = Vec::new();
        for permission in all {
            if self.may(person, permission)? {
                held.push(permission);
            }
        }
        Ok(held)
    }

    /// Bars a person. `expires_at` of `None` is permanent.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::PermissionDenied`] if the issuer lacks
    /// [`Permission::Ban`].
    pub fn ban(
        &self,
        person: PersonId,
        issued_by: PersonId,
        reason: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<()> {
        // The check is here rather than at the call site, so no future caller
        // can forget it. specs/08-seguranca.md: the server denying is the
        // security; the interface hiding the button is convenience.
        if !self.may(issued_by, Permission::Ban)? {
            return Err(Refusal::PermissionDenied.into());
        }
        self.connection
            .execute(
                "INSERT INTO bans (person_id, issued_by, reason, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    person.get() as i64,
                    issued_by.get() as i64,
                    reason,
                    now_seconds(),
                    expires_at
                ],
            )
            .context("could not record the ban")?;
        Ok(())
    }

    /// Lifts every ban on a person.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::PermissionDenied`] if the issuer lacks
    /// [`Permission::Ban`].
    pub fn unban(&self, person: PersonId, issued_by: PersonId) -> Result<()> {
        if !self.may(issued_by, Permission::Ban)? {
            return Err(Refusal::PermissionDenied.into());
        }
        self.connection
            .execute("DELETE FROM bans WHERE person_id = ?1", [person.get() as i64])?;
        Ok(())
    }

    /// Whether a person is currently barred.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn is_banned(&self, person: PersonId) -> Result<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM bans
             WHERE person_id = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            params![person.get() as i64, now_seconds()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    const COMMANDER: RoleId = COMMANDER_ROLE;
    const OPERATOR: RoleId = OPERATOR_ROLE;
    const PERSON: RoleId = PERSON_ROLE;
    const OBSERVER: RoleId = OBSERVER_ROLE;

    /// Every permission `specs/04-servidor-seele.md` enumerates.
    const ALL: &[Permission] = &[
        Permission::ViewVoiceRoom,
        Permission::EnterVoiceRoom,
        Permission::Speak,
        Permission::ReadChannel,
        Permission::WriteChannel,
        Permission::RemoveMessage,
        Permission::MovePerson,
        Permission::Kick,
        Permission::Ban,
        Permission::ManageVoiceRooms,
        Permission::ManageRoles,
        Permission::AdministerServer,
        Permission::AttachFile,
    ];

    fn store() -> Persistence {
        Persistence::open(&Location::Memory).unwrap()
    }

    /// An account holding exactly one named role, whatever it arrived with.
    ///
    /// Normalising rather than assuming: the first account on a server arrives as
    /// a Comandante and every one after it as a Pessoa, so a fixture that only
    /// added a role would hand back a Comandante whenever it happened to be
    /// called first — and the permission matrix below would pass for the wrong
    /// reason, which is the worst way for it to pass.
    fn person_with(persistence: &Persistence, nickname: &str, key: u8, role: RoleId) -> PersonId {
        let permissions = Permissions::new(persistence);
        let person = permissions.register_or_find(&[key; 32], nickname).unwrap();
        for held in [COMMANDER, OPERATOR, PERSON, OBSERVER] {
            if held != role {
                permissions.revoke_role(person.id, held).unwrap();
            }
        }
        permissions.grant_role(person.id, role).unwrap();
        person.id
    }

    #[test]
    fn a_key_gets_the_same_account_every_time() {
        // ADR 0004 makes the key the identity, so a returning person must find
        // their own history rather than a new empty account.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let first = permissions.register_or_find(&[7; 32], "ayanami").unwrap();
        let second = permissions.register_or_find(&[7; 32], "ayanami").unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn a_different_key_cannot_take_a_nickname() {
        // Otherwise anybody could claim somebody else's name and inherit how the
        // room reads their messages.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        permissions.register_or_find(&[7; 32], "ayanami").unwrap();
        assert!(permissions.register_or_find(&[8; 32], "ayanami").is_err());
    }

    #[test]
    fn the_first_account_becomes_the_commander() {
        // Whoever hosts is whoever connects to their own server first. Without
        // this, migration 1 seeds a Comandante role that no account can ever
        // hold, and ManageVoiceRooms / ManageRoles / AdministerServer are unreachable
        // by construction — every verb behind them dead on arrival.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let anfitriao = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert_eq!(anfitriao.roles, vec![COMMANDER]);
        assert!(permissions.may(anfitriao.id, Permission::ManageVoiceRooms).unwrap());
    }

    #[test]
    fn a_returning_key_takes_the_name_it_asks_for() {
        // Encontrado num teste entre duas máquinas: a pessoa entrou como
        // `pessoa`, trocou o nome, e a outra continuou vendo `pessoa` — nas
        // mensagens e no roster. O pedido era descartado em silêncio, e não
        // havia nada na tela dizendo por quê.
        //
        // O histórico acompanha sem trabalho nenhum: `persistence::messages` resolve
        // o autor por `JOIN people`, então o nome exibido é o de agora e não uma
        // cópia congelada por mensagem.
        let persistence = store();
        let permissions = Permissions::new(&persistence);

        let antes = permissions.register_or_find(&[1; 32], "pessoa").unwrap();
        let depois = permissions.register_or_find(&[1; 32], "ikari").unwrap();

        assert_eq!(depois.id, antes.id, "trocar de nome criou uma conta nova");
        assert_eq!(
            depois.nickname, "ikari",
            "o nome pedido foi descartado, e a pessoa continua sendo chamada do \
             nome antigo para todo mundo"
        );
    }

    #[test]
    fn renaming_is_not_a_way_to_take_somebody_elses_name() {
        // A outra metade, e a razão de o ADR 0017 existir. Sem esta recusa,
        // renomear seria o caminho para herdar como outra pessoa é chamada — e
        // a proteção que existe na criação de conta seria contornável por
        // qualquer um que já tivesse uma.
        let persistence = store();
        let permissions = Permissions::new(&persistence);

        permissions.register_or_find(&[1; 32], "ikari").unwrap();
        let outra = permissions.register_or_find(&[2; 32], "ayanami").unwrap();

        assert!(
            permissions.register_or_find(&[2; 32], "ikari").is_err(),
            "uma conta renomeou-se para o nome de outra pessoa"
        );
        let ainda = permissions.register_or_find(&[2; 32], "ayanami").unwrap();
        assert_eq!(
            ainda.nickname, outra.nickname,
            "a tentativa recusada mexeu no nome de quem tentou"
        );
    }

    #[test]
    fn an_account_older_than_the_commandership_can_still_take_it() {
        // O caso que este conserto existe para resolver, e ele veio de um servidor
        // de verdade: contas criadas **antes** de o comando existir nunca
        // passaram por `seat_the_arrival`, então o assento ficou vazio e nunca
        // marcado. O formulário de criar sala não aparecia para ninguém e não
        // havia como fazer aparecer — a única saída era entrar com um apelido
        // nunca usado, que é uma resposta absurda para «administre o seu
        // próprio server».
        //
        // O cenário é encenado como o banco antigo de verdade era: a linha do
        // pessoa escrita à mão, com o papel de Pessoa e nada mais. Encená-lo
        // revogando o comando de uma conta nova seria outro caso — aquele deixa
        // a marca, e a marca é justamente o que impede a revogação de ser
        // desfeita.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        persistence
            .connection()
            .execute(
                "INSERT INTO people (nickname, public_key, created_at, last_seen_at)
                 VALUES ('anfitriao', ?1, 0, 0)",
                [&[1u8; 32][..]],
            )
            .unwrap();
        let antigo = persistence.connection().last_insert_rowid();
        persistence
            .connection()
            .execute(
                "INSERT INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
                params![antigo, i64::from(PERSON.get())],
            )
            .unwrap();

        let de_volta = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert!(
            de_volta.roles.contains(&COMMANDER),
            "uma conta anterior ao comando não conseguiu assumi-lo, e o servidor fica \
             inadministrável para sempre: {de_volta:?}"
        );
        assert!(permissions.may(de_volta.id, Permission::ManageVoiceRooms).unwrap());
    }

    #[test]
    fn a_revoked_commander_does_not_get_it_back_by_reconnecting() {
        // A outra metade, e a razão de a marca existir. «Assume se estiver
        // vago» desfaria toda revogação na reconexão seguinte, e a revogação
        // viraria decoração. Um teste vizinho já dizia isso; este diz por que a
        // marca é o que separa os dois casos.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let dono = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert!(dono.roles.contains(&COMMANDER));

        permissions.revoke_role(dono.id, COMMANDER).unwrap();
        let de_novo = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert!(
            !de_novo.roles.contains(&COMMANDER),
            "reconectar desfez a revogação: {de_novo:?}"
        );
    }

    #[test]
    fn the_second_account_is_only_a_person() {
        // The half that makes the rule a rule. "First account is Comandante"
        // implemented as "every account is Comandante" passes the test above and
        // hands the server to whoever walks in.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        permissions.register_or_find(&[1; 32], "anfitriao").unwrap();

        let convidado = permissions.register_or_find(&[2; 32], "shinji").unwrap();
        assert_eq!(convidado.roles, vec![PERSON]);
        for permission in [
            Permission::ManageVoiceRooms,
            Permission::ManageRoles,
            Permission::AdministerServer,
        ] {
            assert!(
                !permissions.may(convidado.id, permission).unwrap(),
                "the second account arrived holding {permission:?}"
            );
        }
    }

    #[test]
    fn two_arrivals_at_once_produce_exactly_one_commander() {
        // Not "neither", which leaves the server unadministrable forever, and not
        // "both", which hands a stranger every permission there is. Two real
        // connections on the same file, because the claim's whole correctness is
        // that SQLite serialises the two writers and the second sees the first's
        // row — a single-connection test could not tell that apart from a check
        // done in Rust before the insert, which would race.
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("dogma.db");
        Persistence::open(&Location::File(file.clone())).unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let chegantes: Vec<_> = [(1_u8, "ayanami"), (2_u8, "shinji")]
            .into_iter()
            .map(|(key, nickname)| {
                let file = file.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let persistence = Persistence::open(&Location::File(file)).unwrap();
                    barrier.wait();
                    Permissions::new(&persistence)
                        .register_or_find(&[key; 32], nickname)
                        .unwrap()
                        .roles
                })
            })
            .collect();

        let papeis: Vec<Vec<RoleId>> = chegantes
            .into_iter()
            .map(|chegante| chegante.join().unwrap())
            .collect();

        let comandantes = papeis
            .iter()
            .filter(|roles| roles.contains(&COMMANDER))
            .count();
        assert_eq!(comandantes, 1, "roles handed out: {papeis:?}");
        assert_eq!(
            papeis.iter().filter(|roles| roles.contains(&PERSON)).count(),
            1,
            "roles handed out: {papeis:?}"
        );
    }

    #[test]
    fn a_returning_account_does_not_claim_the_commandership_again() {
        // `register_or_find` runs on every handshake. Seating on the *find* path
        // as well as the create path would let the second person take the seat
        // the moment the first one's account was deleted mid-session — and, more
        // ordinarily, would re-grant a role an operator had just revoked.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let anfitriao = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        permissions.revoke_role(anfitriao.id, COMMANDER).unwrap();

        let de_novo = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert!(de_novo.roles.is_empty(), "roles came back: {de_novo:?}");
    }

    // ---- the permission matrix ----
    //
    // specs/10-convencoes.md: "Permissões | Um teste por permissão negada,
    // obrigatório." specs/08-seguranca.md: "para cada permissão, um teste de
    // cliente sem ela tentando a ação."

    #[test]
    fn an_observer_is_denied_every_permission_it_should_not_have() {
        // specs/04: Observador is "só ouvir e ler".
        let persistence = store();
        let observer = person_with(&persistence, "observador", 4, OBSERVER);
        let permissions = Permissions::new(&persistence);

        let allowed = [
            Permission::ViewVoiceRoom,
            Permission::EnterVoiceRoom,
            Permission::ReadChannel,
        ];
        for permission in ALL {
            let expected = allowed.contains(permission);
            assert_eq!(
                permissions.may(observer, *permission).unwrap(),
                expected,
                "observer and {permission:?}"
            );
        }
    }

    #[test]
    fn a_person_is_denied_every_moderation_permission() {
        let persistence = store();
        let person = person_with(&persistence, "ayanami", 1, PERSON);
        let permissions = Permissions::new(&persistence);

        let denied = [
            Permission::RemoveMessage,
            Permission::MovePerson,
            Permission::Kick,
            Permission::Ban,
            Permission::ManageVoiceRooms,
            Permission::ManageRoles,
            Permission::AdministerServer,
        ];
        for permission in denied {
            assert!(
                !permissions.may(person, permission).unwrap(),
                "a person should not have {permission:?}"
            );
        }
    }

    #[test]
    fn an_operator_moderates_but_does_not_administer() {
        // The channel specs/04 draws between Operador and Comandante.
        let persistence = store();
        let operator = person_with(&persistence, "operador", 2, OPERATOR);
        let permissions = Permissions::new(&persistence);

        assert!(permissions.may(operator, Permission::Kick).unwrap());
        assert!(permissions.may(operator, Permission::Ban).unwrap());
        assert!(!permissions.may(operator, Permission::ManageVoiceRooms).unwrap());
        assert!(!permissions.may(operator, Permission::ManageRoles).unwrap());
        assert!(!permissions.may(operator, Permission::AdministerServer).unwrap());
    }

    #[test]
    fn a_commander_has_everything() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let permissions = Permissions::new(&persistence);
        for permission in ALL {
            assert!(
                permissions.may(commander, *permission).unwrap(),
                "commander lacks {permission:?}"
            );
        }
    }

    #[test]
    fn denial_beats_a_grant_from_another_role() {
        // specs/04-servidor-seele.md: "permissões negadas vencem concedidas".
        // Without an explicit denial the sentence has nothing to mean, and
        // giving somebody Observer alongside Person would quietly do nothing.
        let persistence = store();
        let person = person_with(&persistence, "silenciado", 3, PERSON);
        let permissions = Permissions::new(&persistence);
        assert!(permissions.may(person, Permission::Speak).unwrap());

        permissions.grant_role(person, OBSERVER).unwrap();

        assert!(
            !permissions.may(person, Permission::Speak).unwrap(),
            "the Observer denial did not beat the Person grant"
        );
        // And the permissions the two roles agree on survive.
        assert!(permissions.may(person, Permission::ReadChannel).unwrap());
    }

    #[test]
    fn a_ban_beats_every_permission() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "ayanami", 1, PERSON);
        let permissions = Permissions::new(&persistence);

        assert!(permissions.may(target, Permission::Speak).unwrap());
        permissions
            .ban(target, commander, Some("flooding"), None)
            .unwrap();

        for permission in ALL {
            assert!(
                !permissions.may(target, *permission).unwrap(),
                "a banned person kept {permission:?}"
            );
        }
    }

    #[test]
    fn banning_needs_the_permission_to_ban() {
        // specs/08-seguranca.md: every action verified on the server. The check
        // lives inside `ban` so no future caller can forget it.
        let persistence = store();
        let ordinary = person_with(&persistence, "ayanami", 1, PERSON);
        let target = person_with(&persistence, "shinji", 2, PERSON);
        let permissions = Permissions::new(&persistence);

        assert!(permissions.ban(target, ordinary, None, None).is_err());
        assert!(!permissions.is_banned(target).unwrap());
    }

    #[test]
    fn an_expired_ban_stops_applying() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "ayanami", 1, PERSON);
        let permissions = Permissions::new(&persistence);

        permissions
            .ban(target, commander, None, Some(now_seconds() - 1))
            .unwrap();
        assert!(!permissions.is_banned(target).unwrap());
        assert!(permissions.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_ban_can_be_lifted() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "ayanami", 1, PERSON);
        let permissions = Permissions::new(&persistence);

        permissions.ban(target, commander, None, None).unwrap();
        assert!(permissions.is_banned(target).unwrap());

        permissions.unban(target, commander).unwrap();
        assert!(!permissions.is_banned(target).unwrap());
        assert!(permissions.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_person_with_no_roles_can_do_nothing() {
        // The default has to be denial. A person whose roles were all revoked
        // must not fall through to some implicit baseline.
        let persistence = store();
        let person = person_with(&persistence, "sem-papel", 5, PERSON);
        let permissions = Permissions::new(&persistence);
        permissions.revoke_role(person, PERSON).unwrap();

        for permission in ALL {
            assert!(
                !permissions.may(person, *permission).unwrap(),
                "a roleless person had {permission:?}"
            );
        }
        assert!(permissions.permissions(person).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_person_is_refused_rather_than_defaulted() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        assert!(permissions.person(PersonId(9999)).is_err());
        assert!(!permissions.may(PersonId(9999), Permission::Speak).unwrap());
    }
}
