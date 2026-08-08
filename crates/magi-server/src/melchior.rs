//! MELCHIOR — identity, roles, permissions and bans.
//!
//! `specs/04-servidor-magi.md`:
//!
//! > Modelo simples e enumerado, sem sistema de expressão. Cada Papel carrega um
//! > conjunto: `ver_cage`, `inserir_plug`, `falar`, …
//! >
//! > Papéis padrão: **Comandante** (tudo), **Operador** (moderação), **Piloto**
//! > (uso normal), **Observador** (só ouvir e ler).
//! >
//! > Regra: permissões negadas vencem concedidas. Sem herança em árvore — a
//! > complexidade não se paga na escala alvo.
//!
//! # "Denied beats granted" needs something to deny
//!
//! A model of grants alone makes that sentence vacuous: absence is already
//! denial, so there is nothing for a denial to win against. A role here can
//! therefore both **grant** and **deny**, and a denial in any role the pilot
//! holds beats a grant in any other.
//!
//! It matters in exactly the case the spec's four defaults suggest: giving
//! somebody Observer alongside Pilot should silence them, and with grants alone
//! it would do nothing at all.
//!
//! # Every check happens here
//!
//! `specs/08-seguranca.md`: "Toda ação é verificada no servidor, sempre, mesmo
//! que o cliente já esconda o botão. A interface esconder é conveniência; o
//! servidor negar é a segurança."

use anyhow::{Context, Result};
use magi_proto::control::Permission;
use magi_proto::ids::{PilotId, RoleId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::casper::{now_seconds, Casper};

/// The Comandante role, seeded by migration 1.
pub const COMMANDER_ROLE: RoleId = RoleId(1);
/// The Operador role.
pub const OPERATOR_ROLE: RoleId = RoleId(2);
/// The Piloto role, which every new account arrives with.
pub const PILOT_ROLE: RoleId = RoleId(3);
/// The Observador role: may listen and read, and nothing else.
pub const OBSERVER_ROLE: RoleId = RoleId(4);

/// A pilot as MELCHIOR knows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pilot {
    /// Account identifier.
    pub id: PilotId,
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
    /// The pilot holds no role granting it, or a role denying it.
    #[error("permission denied")]
    PermissionDenied,
    /// The pilot is barred from the Dogma.
    #[error("banned")]
    Banned,
    /// No such account.
    #[error("no such pilot")]
    UnknownPilot,
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
        "ViewCage" => Permission::ViewCage,
        "InsertPlug" => Permission::InsertPlug,
        "Speak" => Permission::Speak,
        "ReadLine" => Permission::ReadLine,
        "WriteLine" => Permission::WriteLine,
        "RemoveMessage" => Permission::RemoveMessage,
        "MovePilot" => Permission::MovePilot,
        "Kick" => Permission::Kick,
        "Ban" => Permission::Ban,
        "ManageCages" => Permission::ManageCages,
        "ManageRoles" => Permission::ManageRoles,
        "AdministerDogma" => Permission::AdministerDogma,
        _ => return None,
    })
}

/// Identity and authorisation, over CASPER.
pub struct Melchior<'a> {
    connection: &'a Connection,
}

impl<'a> Melchior<'a> {
    /// Borrows a store.
    #[must_use]
    pub fn new(casper: &'a Casper) -> Self {
        Self {
            connection: casper.connection(),
        }
    }

    /// Finds the account for a public key, creating it on first sight.
    ///
    /// ADR 0004 makes the key the identity. The nickname is a label attached to
    /// it, so a returning pilot keeps their account even if they ask for a
    /// different name — and a **different** key asking for a taken name is
    /// refused rather than silently given somebody else's history.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::NicknameTaken`] if the name belongs to another key, or
    /// a database error.
    pub fn register_or_find(&self, public_key: &[u8], nickname: &str) -> Result<Pilot> {
        if let Some(id) = self
            .connection
            .query_row(
                "SELECT id FROM pilots WHERE public_key = ?1",
                [public_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            self.connection.execute(
                "UPDATE pilots SET last_seen_at = ?1 WHERE id = ?2",
                params![now_seconds(), id],
            )?;
            return self.pilot(PilotId(id as u64));
        }

        let taken: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM pilots WHERE nickname = ?1",
                [nickname],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() {
            return Err(Refusal::NicknameTaken.into());
        }

        self.connection.execute(
            "INSERT INTO pilots (nickname, public_key, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![nickname, public_key, now_seconds()],
        )?;
        let id = self.connection.last_insert_rowid();

        // Everybody arrives as a Pilot. specs/04 makes that the normal-use role;
        // an operator promotes from there.
        self.connection.execute(
            "INSERT INTO pilot_roles (pilot_id, role_id) VALUES (?1, 3)",
            params![id],
        )?;

        self.pilot(PilotId(id as u64))
    }

    /// Loads one account.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::UnknownPilot`] if there is no such account.
    pub fn pilot(&self, id: PilotId) -> Result<Pilot> {
        let nickname: String = self
            .connection
            .query_row(
                "SELECT nickname FROM pilots WHERE id = ?1",
                [id.get() as i64],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(Refusal::UnknownPilot)?;

        let mut statement = self
            .connection
            .prepare("SELECT role_id FROM pilot_roles WHERE pilot_id = ?1 ORDER BY role_id")?;
        let roles = statement
            .query_map([id.get() as i64], |row| row.get::<_, i64>(0))?
            .filter_map(Result::ok)
            .map(|role| RoleId(role as u32))
            .collect();

        Ok(Pilot {
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
    pub fn grant_role(&self, pilot: PilotId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO pilot_roles (pilot_id, role_id) VALUES (?1, ?2)",
            params![pilot.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Revokes a role.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn revoke_role(&self, pilot: PilotId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "DELETE FROM pilot_roles WHERE pilot_id = ?1 AND role_id = ?2",
            params![pilot.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Whether a pilot may do something.
    ///
    /// Denial in any role beats a grant in any other, and a ban beats
    /// everything. `specs/08-seguranca.md` requires this to be consulted for
    /// every action, whether or not the client already hid the button.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn may(&self, pilot: PilotId, permission: Permission) -> Result<bool> {
        if self.is_banned(pilot)? {
            return Ok(false);
        }

        let mut statement = self.connection.prepare(
            "SELECT r.permissions, r.denials
             FROM roles r
             JOIN pilot_roles pr ON pr.role_id = r.id
             WHERE pr.pilot_id = ?1",
        )?;
        let rows: Vec<(String, String)> = statement
            .query_map([pilot.get() as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
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

    /// Every permission a pilot currently holds.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn permissions(&self, pilot: PilotId) -> Result<Vec<Permission>> {
        let all = [
            Permission::ViewCage,
            Permission::InsertPlug,
            Permission::Speak,
            Permission::ReadLine,
            Permission::WriteLine,
            Permission::RemoveMessage,
            Permission::MovePilot,
            Permission::Kick,
            Permission::Ban,
            Permission::ManageCages,
            Permission::ManageRoles,
            Permission::AdministerDogma,
        ];
        let mut held = Vec::new();
        for permission in all {
            if self.may(pilot, permission)? {
                held.push(permission);
            }
        }
        Ok(held)
    }

    /// Bars a pilot. `expires_at` of `None` is permanent.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::PermissionDenied`] if the issuer lacks
    /// [`Permission::Ban`].
    pub fn ban(
        &self,
        pilot: PilotId,
        issued_by: PilotId,
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
                "INSERT INTO bans (pilot_id, issued_by, reason, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    pilot.get() as i64,
                    issued_by.get() as i64,
                    reason,
                    now_seconds(),
                    expires_at
                ],
            )
            .context("could not record the ban")?;
        Ok(())
    }

    /// Lifts every ban on a pilot.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::PermissionDenied`] if the issuer lacks
    /// [`Permission::Ban`].
    pub fn unban(&self, pilot: PilotId, issued_by: PilotId) -> Result<()> {
        if !self.may(issued_by, Permission::Ban)? {
            return Err(Refusal::PermissionDenied.into());
        }
        self.connection
            .execute("DELETE FROM bans WHERE pilot_id = ?1", [pilot.get() as i64])?;
        Ok(())
    }

    /// Whether a pilot is currently barred.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn is_banned(&self, pilot: PilotId) -> Result<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM bans
             WHERE pilot_id = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            params![pilot.get() as i64, now_seconds()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casper::Location;

    const COMMANDER: RoleId = COMMANDER_ROLE;
    const OPERATOR: RoleId = OPERATOR_ROLE;
    const PILOT: RoleId = PILOT_ROLE;
    const OBSERVER: RoleId = OBSERVER_ROLE;

    /// Every permission `specs/04-servidor-magi.md` enumerates.
    const ALL: &[Permission] = &[
        Permission::ViewCage,
        Permission::InsertPlug,
        Permission::Speak,
        Permission::ReadLine,
        Permission::WriteLine,
        Permission::RemoveMessage,
        Permission::MovePilot,
        Permission::Kick,
        Permission::Ban,
        Permission::ManageCages,
        Permission::ManageRoles,
        Permission::AdministerDogma,
    ];

    fn store() -> Casper {
        Casper::open(&Location::Memory).unwrap()
    }

    fn pilot_with(casper: &Casper, nickname: &str, key: u8, role: RoleId) -> PilotId {
        let melchior = Melchior::new(casper);
        let pilot = melchior.register_or_find(&[key; 32], nickname).unwrap();
        if role != PILOT {
            melchior.revoke_role(pilot.id, PILOT).unwrap();
            melchior.grant_role(pilot.id, role).unwrap();
        }
        pilot.id
    }

    #[test]
    fn a_key_gets_the_same_account_every_time() {
        // ADR 0004 makes the key the identity, so a returning pilot must find
        // their own history rather than a new empty account.
        let casper = store();
        let melchior = Melchior::new(&casper);
        let first = melchior.register_or_find(&[7; 32], "ayanami").unwrap();
        let second = melchior.register_or_find(&[7; 32], "ayanami").unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn a_different_key_cannot_take_a_nickname() {
        // Otherwise anybody could claim somebody else's name and inherit how the
        // room reads their messages.
        let casper = store();
        let melchior = Melchior::new(&casper);
        melchior.register_or_find(&[7; 32], "ayanami").unwrap();
        assert!(melchior.register_or_find(&[8; 32], "ayanami").is_err());
    }

    #[test]
    fn everybody_arrives_as_a_pilot() {
        let casper = store();
        let melchior = Melchior::new(&casper);
        let pilot = melchior.register_or_find(&[1; 32], "shinji").unwrap();
        assert_eq!(pilot.roles, vec![PILOT]);
    }

    // ---- the permission matrix ----
    //
    // specs/10-convencoes.md: "Permissões | Um teste por permissão negada,
    // obrigatório." specs/08-seguranca.md: "para cada permissão, um teste de
    // cliente sem ela tentando a ação."

    #[test]
    fn an_observer_is_denied_every_permission_it_should_not_have() {
        // specs/04: Observador is "só ouvir e ler".
        let casper = store();
        let observer = pilot_with(&casper, "observador", 4, OBSERVER);
        let melchior = Melchior::new(&casper);

        let allowed = [
            Permission::ViewCage,
            Permission::InsertPlug,
            Permission::ReadLine,
        ];
        for permission in ALL {
            let expected = allowed.contains(permission);
            assert_eq!(
                melchior.may(observer, *permission).unwrap(),
                expected,
                "observer and {permission:?}"
            );
        }
    }

    #[test]
    fn a_pilot_is_denied_every_moderation_permission() {
        let casper = store();
        let pilot = pilot_with(&casper, "ayanami", 1, PILOT);
        let melchior = Melchior::new(&casper);

        let denied = [
            Permission::RemoveMessage,
            Permission::MovePilot,
            Permission::Kick,
            Permission::Ban,
            Permission::ManageCages,
            Permission::ManageRoles,
            Permission::AdministerDogma,
        ];
        for permission in denied {
            assert!(
                !melchior.may(pilot, permission).unwrap(),
                "a pilot should not have {permission:?}"
            );
        }
    }

    #[test]
    fn an_operator_moderates_but_does_not_administer() {
        // The line specs/04 draws between Operador and Comandante.
        let casper = store();
        let operator = pilot_with(&casper, "operador", 2, OPERATOR);
        let melchior = Melchior::new(&casper);

        assert!(melchior.may(operator, Permission::Kick).unwrap());
        assert!(melchior.may(operator, Permission::Ban).unwrap());
        assert!(!melchior.may(operator, Permission::ManageCages).unwrap());
        assert!(!melchior.may(operator, Permission::ManageRoles).unwrap());
        assert!(!melchior.may(operator, Permission::AdministerDogma).unwrap());
    }

    #[test]
    fn a_commander_has_everything() {
        let casper = store();
        let commander = pilot_with(&casper, "comandante", 9, COMMANDER);
        let melchior = Melchior::new(&casper);
        for permission in ALL {
            assert!(
                melchior.may(commander, *permission).unwrap(),
                "commander lacks {permission:?}"
            );
        }
    }

    #[test]
    fn denial_beats_a_grant_from_another_role() {
        // specs/04-servidor-magi.md: "permissões negadas vencem concedidas".
        // Without an explicit denial the sentence has nothing to mean, and
        // giving somebody Observer alongside Pilot would quietly do nothing.
        let casper = store();
        let melchior = Melchior::new(&casper);
        let pilot = melchior.register_or_find(&[3; 32], "silenciado").unwrap();
        assert!(melchior.may(pilot.id, Permission::Speak).unwrap());

        melchior.grant_role(pilot.id, OBSERVER).unwrap();

        assert!(
            !melchior.may(pilot.id, Permission::Speak).unwrap(),
            "the Observer denial did not beat the Pilot grant"
        );
        // And the permissions the two roles agree on survive.
        assert!(melchior.may(pilot.id, Permission::ReadLine).unwrap());
    }

    #[test]
    fn a_ban_beats_every_permission() {
        let casper = store();
        let commander = pilot_with(&casper, "comandante", 9, COMMANDER);
        let target = pilot_with(&casper, "ayanami", 1, PILOT);
        let melchior = Melchior::new(&casper);

        assert!(melchior.may(target, Permission::Speak).unwrap());
        melchior
            .ban(target, commander, Some("flooding"), None)
            .unwrap();

        for permission in ALL {
            assert!(
                !melchior.may(target, *permission).unwrap(),
                "a banned pilot kept {permission:?}"
            );
        }
    }

    #[test]
    fn banning_needs_the_permission_to_ban() {
        // specs/08-seguranca.md: every action verified on the server. The check
        // lives inside `ban` so no future caller can forget it.
        let casper = store();
        let ordinary = pilot_with(&casper, "ayanami", 1, PILOT);
        let target = pilot_with(&casper, "shinji", 2, PILOT);
        let melchior = Melchior::new(&casper);

        assert!(melchior.ban(target, ordinary, None, None).is_err());
        assert!(!melchior.is_banned(target).unwrap());
    }

    #[test]
    fn an_expired_ban_stops_applying() {
        let casper = store();
        let commander = pilot_with(&casper, "comandante", 9, COMMANDER);
        let target = pilot_with(&casper, "ayanami", 1, PILOT);
        let melchior = Melchior::new(&casper);

        melchior
            .ban(target, commander, None, Some(now_seconds() - 1))
            .unwrap();
        assert!(!melchior.is_banned(target).unwrap());
        assert!(melchior.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_ban_can_be_lifted() {
        let casper = store();
        let commander = pilot_with(&casper, "comandante", 9, COMMANDER);
        let target = pilot_with(&casper, "ayanami", 1, PILOT);
        let melchior = Melchior::new(&casper);

        melchior.ban(target, commander, None, None).unwrap();
        assert!(melchior.is_banned(target).unwrap());

        melchior.unban(target, commander).unwrap();
        assert!(!melchior.is_banned(target).unwrap());
        assert!(melchior.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_pilot_with_no_roles_can_do_nothing() {
        // The default has to be denial. A pilot whose roles were all revoked
        // must not fall through to some implicit baseline.
        let casper = store();
        let melchior = Melchior::new(&casper);
        let pilot = melchior.register_or_find(&[5; 32], "sem-papel").unwrap();
        melchior.revoke_role(pilot.id, PILOT).unwrap();

        for permission in ALL {
            assert!(
                !melchior.may(pilot.id, *permission).unwrap(),
                "a roleless pilot had {permission:?}"
            );
        }
        assert!(melchior.permissions(pilot.id).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_pilot_is_refused_rather_than_defaulted() {
        let casper = store();
        let melchior = Melchior::new(&casper);
        assert!(melchior.pilot(PilotId(9999)).is_err());
        assert!(!melchior.may(PilotId(9999), Permission::Speak).unwrap());
    }
}
