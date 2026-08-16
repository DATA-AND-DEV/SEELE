//! Cages and Lines — the rooms a Dogma is made of.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > ```text
//! >  ├─ Cage       — canal de voz     (id, nome, limite, senha?, papel mínimo)
//! >  └─ Linha      — canal de texto   (id, nome, papel mínimo de leitura/escrita)
//! > ```
//! >
//! > Cages e Linhas são independentes; um Cage pode ter uma Linha associada, mas
//! > não é obrigatório.
//!
//! The tables have been in [`super::schema`] since migration 1, with every
//! column the spec names. What was missing was any way to put a row in one:
//! outside test modules, the whole repository could read the channel tree and
//! could not write to it. A Dogma therefore had exactly the rooms
//! [`crate::seed`] gave it at boot and could never have another.
//!
//! # Why this reads as well as writes
//!
//! The handshake needs the tree to fill `Sessao`, and creating a room needs to
//! send back the row it just made. Two copies of the same `SELECT` is one copy
//! that will be updated and one that will not — and the one that is not is the
//! handshake, so the divergence would show up as "the room is there until you
//! reconnect".
//!
//! # Nothing here checks a permission
//!
//! On purpose, and it is the opposite of the choice [`crate::melchior::Melchior::ban`]
//! makes. A ban is a single verb with a single caller; a room is written by the
//! session handler, by [`crate::seed`] at boot, and by tests, and only the first
//! of those has a pilot to check. The check therefore lives at the one call site
//! that has an asker — and `specs/08-seguranca.md` still holds, because that
//! call site is the only path a client can reach.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use seele_proto::control::{CageInfo, LineInfo};
use seele_proto::ids::{CageId, LineId};

/// The channel tree, over CASPER.
pub struct Channels<'a> {
    connection: &'a Connection,
}

impl<'a> Channels<'a> {
    /// Borrows a store.
    #[must_use]
    pub fn new(casper: &'a super::Casper) -> Self {
        Self {
            connection: casper.connection(),
        }
    }

    /// Every Cage, in the order a shell should draw them.
    ///
    /// `position` first and `id` as the tiebreak, so a Dogma that has never
    /// reordered anything still lists its rooms in the order they were made
    /// rather than in whatever order SQLite finds them.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn cages(&self) -> Result<Vec<CageInfo>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, member_limit, password_hash IS NOT NULL, line_id
             FROM cages ORDER BY position, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(CageInfo {
                    id: CageId(row.get::<_, i64>(0)? as u32),
                    name: row.get(1)?,
                    limit: row.get::<_, i64>(2)? as u16,
                    password_required: row.get(3)?,
                    line: row.get::<_, Option<i64>>(4)?.map(|id| LineId(id as u32)),
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Every Line, in the order a shell should draw them.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn lines(&self) -> Result<Vec<LineInfo>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM lines ORDER BY position, id")?;
        let rows = statement
            .query_map([], |row| {
                Ok(LineInfo {
                    id: LineId(row.get::<_, i64>(0)? as u32),
                    name: row.get(1)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Makes a Cage, and returns it as the wire will carry it.
    ///
    /// `line` binds a text channel to the room. It is checked against the
    /// `lines` table rather than trusted: SQLite enforces the foreign key, but
    /// the error it raises is a database error rather than something a caller
    /// can tell apart from the disk being full, and the difference matters to
    /// the person who mistyped a number.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if `line` names a Line that is not there, or a
    /// database error.
    pub fn create_cage(&self, name: &str, limit: u16, line: Option<LineId>) -> Result<CageInfo> {
        let name = name.trim();
        if let Some(line) = line {
            if !self.line_exists(line)? {
                return Err(NoSuchChannel.into());
            }
        }

        self.connection
            .execute(
                "INSERT INTO cages (name, member_limit, line_id, position)
                 VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM cages))",
                params![
                    name,
                    i64::from(limit),
                    line.map(|line| i64::from(line.get()))
                ],
            )
            .context("could not create the Cage")?;

        Ok(CageInfo {
            id: CageId(self.connection.last_insert_rowid() as u32),
            name: name.to_owned(),
            limit,
            // Nothing sets a password at creation. A room born locked is a room
            // whose maker has to tell everybody a secret before anybody can use
            // it, which is a separate decision taken later, with
            // `admissao::definir_senha_cage`.
            password_required: false,
            line,
        })
    }

    /// Makes a Line, and returns it as the wire will carry it.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn create_line(&self, name: &str) -> Result<LineInfo> {
        let name = name.trim();
        self.connection
            .execute(
                "INSERT INTO lines (name, position)
                 VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM lines))",
                params![name],
            )
            .context("could not create the Line")?;

        Ok(LineInfo {
            id: LineId(self.connection.last_insert_rowid() as u32),
            name: name.to_owned(),
        })
    }

    /// Renames a Cage. Returns the trimmed name that was stored.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Cage, or a database error.
    pub fn rename_cage(&self, cage: CageId, name: &str) -> Result<String> {
        let name = name.trim();
        let changed = self.connection.execute(
            "UPDATE cages SET name = ?1 WHERE id = ?2",
            params![name, i64::from(cage.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(name.to_owned())
    }

    /// Renames a Line. Returns the trimmed name that was stored.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Line, or a database error.
    pub fn rename_line(&self, line: LineId, name: &str) -> Result<String> {
        let name = name.trim();
        let changed = self.connection.execute(
            "UPDATE lines SET name = ?1 WHERE id = ?2",
            params![name, i64::from(line.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(name.to_owned())
    }

    fn line_exists(&self, line: LineId) -> Result<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM lines WHERE id = ?1",
            params![i64::from(line.get())],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

/// The Cage or Line named does not exist.
///
/// Enumerated rather than a sentence, like every other refusal that can reach a
/// client: the shell decides how to say it. `specs/02-protocolo.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no such Cage or Line")]
pub struct NoSuchChannel;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casper::{Casper, Location};

    fn store() -> Casper {
        Casper::open(&Location::Memory).unwrap()
    }

    #[test]
    fn a_fresh_store_has_no_rooms_until_somebody_makes_one() {
        // The starting point, and the reason this module exists: the tables were
        // there and nothing outside a test block ever wrote to them.
        let casper = store();
        let channels = Channels::new(&casper);
        assert!(channels.cages().unwrap().is_empty());
        assert!(channels.lines().unwrap().is_empty());
    }

    #[test]
    fn a_created_cage_reads_back_the_way_it_was_asked_for() {
        let casper = store();
        let channels = Channels::new(&casper);
        let line = channels.create_line("geral").unwrap();
        let cage = channels
            .create_cage("CAGE-01 CENTRAL", 15, Some(line.id))
            .unwrap();

        // What the creator is told, and what everybody else will read out of the
        // table, have to be the same thing — otherwise the room the maker sees
        // is not the room that exists.
        assert_eq!(channels.cages().unwrap(), vec![cage.clone()]);
        assert_eq!(cage.limit, 15);
        assert_eq!(cage.line, Some(line.id));
        assert!(!cage.password_required);
    }

    #[test]
    fn rooms_come_back_in_the_order_they_were_made() {
        // Without an explicit `position` this is whatever the query planner
        // feels like, and a channel list that reshuffles between two sessions is
        // a channel list nobody can build a habit around.
        let casper = store();
        let channels = Channels::new(&casper);
        for name in ["geral", "avisos", "planejamento"] {
            channels.create_line(name).unwrap();
        }
        let names: Vec<String> = channels
            .lines()
            .unwrap()
            .into_iter()
            .map(|line| line.name)
            .collect();
        assert_eq!(names, ["geral", "avisos", "planejamento"]);
    }

    #[test]
    fn a_cage_bound_to_a_line_that_is_not_there_is_refused_by_name() {
        // The foreign key would stop it too, but it would stop it as a database
        // error — indistinguishable from the disk being full, and useless to the
        // person who mistyped a number.
        let casper = store();
        let channels = Channels::new(&casper);
        let refused = channels.create_cage("CAGE-02", 8, Some(LineId(404)));
        assert!(refused
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(
            channels.cages().unwrap().is_empty(),
            "the Cage was made anyway"
        );
    }

    #[test]
    fn renaming_something_that_is_not_there_says_so() {
        let casper = store();
        let channels = Channels::new(&casper);
        assert!(channels
            .rename_cage(CageId(404), "fantasma")
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(channels
            .rename_line(LineId(404), "fantasma")
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
    }

    #[test]
    fn a_rename_keeps_the_identifier_and_the_place_in_the_list() {
        // A rename that moved the room to the end of the list would look, to
        // everybody watching, like the room was destroyed and a new one made.
        let casper = store();
        let channels = Channels::new(&casper);
        channels.create_line("geral").unwrap();
        let segunda = channels.create_line("avisos").unwrap();

        assert_eq!(
            channels.rename_line(segunda.id, "recados").unwrap(),
            "recados"
        );
        let lines = channels.lines().unwrap();
        assert_eq!(lines[1].id, segunda.id);
        assert_eq!(lines[1].name, "recados");
    }

    #[test]
    fn the_padding_around_a_name_is_not_part_of_it() {
        // A name is trimmed once, here, rather than by each shell that draws it:
        // " geral" and "geral" sorting apart in a list is the kind of thing
        // nobody can see and everybody trips over.
        let casper = store();
        let channels = Channels::new(&casper);
        let line = channels.create_line("  geral \n").unwrap();
        assert_eq!(line.name, "geral");
        assert_eq!(channels.lines().unwrap()[0].name, "geral");

        let cage = channels.create_cage("\tCAGE-01  ", 4, None).unwrap();
        assert_eq!(cage.name, "CAGE-01");
        assert_eq!(
            channels.rename_cage(cage.id, " CAGE-02 ").unwrap(),
            "CAGE-02"
        );
        assert_eq!(channels.cages().unwrap()[0].name, "CAGE-02");
    }
}
