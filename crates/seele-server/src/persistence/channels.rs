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
//! On purpose, and it is the opposite of the choice [`crate::permissions::Permissions::ban`]
//! makes. A ban is a single verb with a single caller; a room is written by the
//! session handler, by [`crate::seed`] at boot, and by tests, and only the first
//! of those has a person to check. The check therefore lives at the one call site
//! that has an asker — and `specs/08-seguranca.md` still holds, because that
//! call site is the only path a client can reach.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use seele_proto::control::{CageInfo, LineInfo};
use seele_proto::ids::{CageId, LineId};

/// The channel tree, over PERSISTENCE.
pub struct Channels<'a> {
    connection: &'a Connection,
}

impl<'a> Channels<'a> {
    /// Borrows a store.
    #[must_use]
    pub fn new(persistence: &'a super::Persistence) -> Self {
        Self {
            connection: persistence.connection(),
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

    /// What destroying a Line would cost, counted now.
    ///
    /// The three numbers the confirmation in the app is built out of, and they
    /// are read here rather than estimated anywhere else: a client holds one
    /// page of history and would guess low by the whole of the Line's past.
    ///
    /// Messages already taken off the Line by `remover_mensagem` are left out.
    /// [`super::messages::Messages::remove`] is soft — it clears the body and
    /// stamps `deleted_at`, and `history` filters those rows out — so they are
    /// gone from every screen already. Counting them would inflate what the
    /// reader is told they are about to lose by a number only the database can
    /// see.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Line, or a database error.
    pub fn weigh_line(&self, line: LineId) -> Result<LineWeight> {
        if !self.line_exists(line)? {
            return Err(NoSuchChannel.into());
        }
        // One statement for the three numbers, and not three. Two of them would
        // be counted a moment apart otherwise, and a Line being written to
        // while somebody weighs it could answer "1.847 messages by 7 people"
        // with the seventh person's only message in neither count.
        let (messages, authors, oldest) = self.connection.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT author_id), MIN(created_at)
             FROM messages WHERE line_id = ?1 AND deleted_at IS NULL",
            params![i64::from(line.get())],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )?;
        Ok(LineWeight {
            messages: messages.max(0) as u32,
            authors: authors.max(0) as u32,
            oldest_at_seconds: oldest,
        })
    }

    /// Destroys a Cage.
    ///
    /// # The last one is refused
    ///
    /// A Dogma with no Cage has nowhere to speak, and speaking is what this
    /// product is. Somebody looking at a channel list with no voice room in it
    /// cannot tell a working Dogma from a broken one — which is the exact
    /// condition [`crate::seed`] exists to prevent on the first boot, and it
    /// would be strange to spend a paragraph avoiding it there and then let a
    /// button walk into it.
    ///
    /// Refused by name rather than by foreign key, so the shell can say why
    /// instead of showing the sentence it shows when the disk is full.
    ///
    /// # The Line bound to it survives
    ///
    /// `specs/04-servidor-seele.md` makes Cages and Lines independent and the
    /// association optional. Destroying a voice room says nothing about the
    /// writing that happened to hang off it, and taking the Line down with it
    /// would destroy history through a verb whose confirmation never mentioned
    /// any.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Cage, [`LastCage`] if it
    /// is the only one, or a database error.
    pub fn delete_cage(&self, cage: CageId) -> Result<()> {
        let remaining: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM cages", [], |row| row.get(0))?;
        if remaining <= 1 {
            // Checked before "does it exist", deliberately: with one Cage left,
            // the answer is the same whether the identifier names it or names
            // nothing, and the useful half of the answer is the reason.
            return Err(LastCage.into());
        }
        let changed = self.connection.execute(
            "DELETE FROM cages WHERE id = ?1",
            params![i64::from(cage.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(())
    }

    /// Destroys a Line, and everything written in it.
    ///
    /// Really destroys it. `remover_mensagem` is soft — it keeps the row so
    /// replies do not dangle and an operator can still answer "what was removed
    /// and by whom" — and this is the verb that takes the Line those rows hang
    /// from, so there is nothing left for either purpose to be about. The
    /// confirmation in front of it says so in the same words.
    ///
    /// # Three writes, one transaction
    ///
    /// The messages go by `ON DELETE CASCADE`, which migration 1 already
    /// declares. The other two are the ones a cascade cannot do:
    ///
    /// - a Cage bound to this Line keeps existing and loses the binding.
    ///   `cages.line_id` has no `ON DELETE` clause, so without this the delete
    ///   fails on the foreign key and reaches the shell as a database error —
    ///   "could not destroy it", about a Line whose only sin is being useful to
    ///   a room.
    /// - a reply **from another Line** pointing at a message in this one is
    ///   unhooked first. `messages.replies_to` references `messages(id)` with
    ///   no `ON DELETE` either, so one cross-Line reply is enough to make the
    ///   cascade fail — and nothing stops a client sending one.
    ///
    /// One transaction because a Line half destroyed is worse than one not
    /// destroyed at all: rooms pointing at nothing, replies pointing at
    /// nothing, and a confirmation that already promised it was over.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Line, or a database error.
    pub fn delete_line(&self, line: LineId) -> Result<()> {
        let id = i64::from(line.get());
        // `unchecked_transaction` because [`Channels`] borrows the connection
        // immutably, like every other method here. The nesting it does not
        // check for cannot happen: PERSISTENCE is one connection behind one mutex,
        // and this is the only place that opens a transaction on it outside the
        // migration runner, which runs before anybody is connected.
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE messages SET replies_to = NULL
             WHERE replies_to IN (SELECT id FROM messages WHERE line_id = ?1)",
            params![id],
        )?;
        transaction.execute(
            "UPDATE cages SET line_id = NULL WHERE line_id = ?1",
            params![id],
        )?;
        let changed = transaction.execute("DELETE FROM lines WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        transaction.commit().context("could not destroy the Line")?;
        Ok(())
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

/// The Cage named is the only one this Dogma has.
///
/// Its own refusal rather than [`NoSuchChannel`], because the two ask different
/// things of whoever reads them: one means "check the identifier", this one
/// means "make another room first". `specs/02-protocolo.md` keeps the sentence
/// out of the protocol; the shell writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is the only Cage in the Dogma")]
pub struct LastCage;

/// What a Line holds, as the confirmation in front of destroying it needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineWeight {
    /// How many messages are in it that anybody can read.
    pub messages: u32,
    /// How many distinct people wrote them.
    pub authors: u32,
    /// When the oldest was written, in seconds since the Unix epoch. `None`
    /// when the Line is empty.
    pub oldest_at_seconds: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{Persistence, Location};

    fn store() -> Persistence {
        Persistence::open(&Location::Memory).unwrap()
    }

    #[test]
    fn a_fresh_store_has_no_rooms_until_somebody_makes_one() {
        // The starting point, and the reason this module exists: the tables were
        // there and nothing outside a test block ever wrote to them.
        let persistence = store();
        let channels = Channels::new(&persistence);
        assert!(channels.cages().unwrap().is_empty());
        assert!(channels.lines().unwrap().is_empty());
    }

    #[test]
    fn a_created_cage_reads_back_the_way_it_was_asked_for() {
        let persistence = store();
        let channels = Channels::new(&persistence);
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
        let persistence = store();
        let channels = Channels::new(&persistence);
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
        let persistence = store();
        let channels = Channels::new(&persistence);
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
        let persistence = store();
        let channels = Channels::new(&persistence);
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
        let persistence = store();
        let channels = Channels::new(&persistence);
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

    // ---- unmaking a room ----

    /// A person to hang messages on, since `messages.author_id` is a real key.
    fn person(persistence: &Persistence, nickname: &str, key: u8) -> i64 {
        persistence
            .connection()
            .execute(
                "INSERT INTO people (nickname, public_key, created_at) VALUES (?1, ?2, 0)",
                params![nickname, [key; 32]],
            )
            .unwrap();
        persistence.connection().last_insert_rowid()
    }

    fn say(persistence: &Persistence, line: LineId, author: i64, body: &str, at: i64) -> i64 {
        persistence
            .connection()
            .execute(
                "INSERT INTO messages (line_id, author_id, body, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![i64::from(line.get()), author, body, at],
            )
            .unwrap();
        persistence.connection().last_insert_rowid()
    }

    #[test]
    fn the_weight_of_a_line_is_counted_and_never_guessed() {
        // The number in the confirmation is this number. Three writers, five
        // messages, and the oldest one is the date the sentence gives.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("sync-geral").unwrap();
        let outra = channels.create_line("avisos").unwrap();

        let rei = person(&persistence, "rei", 1);
        let shinji = person(&persistence, "shinji", 2);
        let asuka = person(&persistence, "asuka", 3);
        say(&persistence, line.id, rei, "primeira", 1_678_600_000);
        say(&persistence, line.id, rei, "segunda", 1_678_600_060);
        say(&persistence, line.id, shinji, "terceira", 1_678_600_120);
        say(&persistence, line.id, asuka, "quarta", 1_678_600_180);
        // Noutra Linha, e portanto em nenhuma destas contas.
        say(&persistence, outra.id, asuka, "noutra sala", 1_600_000_000);

        let peso = channels.weigh_line(line.id).unwrap();
        assert_eq!(peso.messages, 4);
        assert_eq!(peso.authors, 3);
        assert_eq!(peso.oldest_at_seconds, Some(1_678_600_000));
    }

    #[test]
    fn a_message_already_taken_off_the_line_is_not_counted_again() {
        // `remover_mensagem` is soft: the row survives so replies do not dangle
        // and an operator can still answer what was removed. It is gone from
        // every screen, though, so counting it would tell somebody they are
        // about to destroy writing that nobody can read.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("geral").unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, line.id, rei, "fica", 100);
        let removida = say(&persistence, line.id, rei, "removida", 50);
        persistence
            .connection()
            .execute(
                "UPDATE messages SET body = '', deleted_at = 1 WHERE id = ?1",
                params![removida],
            )
            .unwrap();

        let peso = channels.weigh_line(line.id).unwrap();
        assert_eq!(peso.messages, 1);
        assert_eq!(peso.authors, 1);
        // E a data é a da mais antiga que ainda dá para ler, não a da removida.
        assert_eq!(peso.oldest_at_seconds, Some(100));
    }

    #[test]
    fn an_empty_line_weighs_nothing_and_has_no_date_to_give() {
        // The one case the sentence cannot be written the usual way: there is
        // no "written since" when nobody wrote.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("nova").unwrap();
        let peso = channels.weigh_line(line.id).unwrap();
        assert_eq!(peso.messages, 0);
        assert_eq!(peso.authors, 0);
        assert_eq!(peso.oldest_at_seconds, None);
    }

    #[test]
    fn weighing_something_that_is_not_there_says_so() {
        let persistence = store();
        assert!(Channels::new(&persistence)
            .weigh_line(LineId(404))
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
    }

    #[test]
    fn destroying_a_line_destroys_what_was_written_in_it() {
        // Really destroys it, which is the decision this whole path is built
        // around: not archived, not hidden from a list. A row left behind would
        // make the confirmation's last sentence false.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("geral").unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, line.id, rei, "some junto", 100);

        channels.delete_line(line.id).unwrap();
        assert!(channels.lines().unwrap().is_empty());
        let left: i64 = persistence
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE line_id = ?1",
                params![i64::from(line.id.get())],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "the Line went and its messages stayed");
    }

    #[test]
    fn a_cage_bound_to_a_destroyed_line_keeps_existing_without_it() {
        // `specs/04-servidor-seele.md` makes the association optional, so the
        // room outlives the Line it pointed at. Without the unbinding, the
        // foreign key refuses the delete and the shell shows the sentence it
        // shows when the disk is full.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("geral").unwrap();
        let cage = channels.create_cage("CAGE-01", 8, Some(line.id)).unwrap();

        channels.delete_line(line.id).unwrap();
        let cages = channels.cages().unwrap();
        assert_eq!(cages.len(), 1, "the Cage went with the Line");
        assert_eq!(cages[0].id, cage.id);
        assert_eq!(cages[0].line, None);
    }

    #[test]
    fn a_reply_from_another_line_does_not_block_the_destruction() {
        // `messages.replies_to` has no `ON DELETE`, so one cross-Line reply is
        // enough to make the cascade fail — and nothing on the wire stops a
        // client sending one. Found here rather than in front of somebody.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let condenada = channels.create_line("condenada").unwrap();
        let outra = channels.create_line("outra").unwrap();
        let rei = person(&persistence, "rei", 1);
        let alvo = say(&persistence, condenada.id, rei, "original", 100);
        persistence
            .connection()
            .execute(
                "INSERT INTO messages (line_id, author_id, body, created_at, replies_to)
                 VALUES (?1, ?2, 'resposta', 200, ?3)",
                params![i64::from(outra.id.get()), rei, alvo],
            )
            .unwrap();

        channels.delete_line(condenada.id).unwrap();
        let pendurada: Option<i64> = persistence
            .connection()
            .query_row(
                "SELECT replies_to FROM messages WHERE line_id = ?1",
                params![i64::from(outra.id.get())],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pendurada, None, "a reply is left pointing at nothing");
    }

    #[test]
    fn the_last_cage_is_refused_by_name() {
        // A Dogma with no Cage has nowhere to speak, and somebody looking at a
        // channel list with no voice room in it cannot tell a working Dogma from
        // a broken one. Refused with its own error, so the shell can say "make
        // another room first" instead of "check the identifier".
        let persistence = store();
        let channels = Channels::new(&persistence);
        let unica = channels.create_cage("CAGE-01", 8, None).unwrap();

        assert!(channels
            .delete_cage(unica.id)
            .unwrap_err()
            .downcast_ref::<LastCage>()
            .is_some());
        assert_eq!(channels.cages().unwrap().len(), 1);

        // A segunda sala é o que destrava a primeira.
        let segunda = channels.create_cage("CAGE-02", 8, None).unwrap();
        channels.delete_cage(unica.id).unwrap();
        let restantes = channels.cages().unwrap();
        assert_eq!(restantes.len(), 1);
        assert_eq!(restantes[0].id, segunda.id);
    }

    #[test]
    fn destroying_a_cage_leaves_the_line_it_was_bound_to_alone() {
        // The other half of "Cages and Lines are independent". A voice room
        // going away is no statement about the writing hanging off it, and
        // taking the Line with it would destroy history through a verb whose
        // confirmation never mentioned any.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let line = channels.create_line("geral").unwrap();
        let cage = channels.create_cage("CAGE-01", 8, Some(line.id)).unwrap();
        channels.create_cage("CAGE-02", 8, None).unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, line.id, rei, "sobrevive", 100);

        channels.delete_cage(cage.id).unwrap();
        assert_eq!(channels.weigh_line(line.id).unwrap().messages, 1);
        assert_eq!(channels.lines().unwrap(), vec![line]);
    }

    #[test]
    fn destroying_a_cage_that_is_not_there_says_so() {
        let persistence = store();
        let channels = Channels::new(&persistence);
        channels.create_cage("CAGE-01", 8, None).unwrap();
        channels.create_cage("CAGE-02", 8, None).unwrap();
        assert!(channels
            .delete_cage(CageId(404))
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(channels
            .delete_line(LineId(404))
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
    }

    #[test]
    fn the_padding_around_a_name_is_not_part_of_it() {
        // A name is trimmed once, here, rather than by each shell that draws it:
        // " geral" and "geral" sorting apart in a list is the kind of thing
        // nobody can see and everybody trips over.
        let persistence = store();
        let channels = Channels::new(&persistence);
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
