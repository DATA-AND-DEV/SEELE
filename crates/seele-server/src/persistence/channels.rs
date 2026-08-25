//! voice_rooms and Channels — the rooms a server is made of.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > ```text
//! >  ├─ voice room       — canal de voz     (id, nome, limite, senha?, papel mínimo)
//! >  └─ Linha      — canal de texto   (id, nome, papel mínimo de leitura/escrita)
//! > ```
//! >
//! > salas de voz e Linhas são independentes; uma sala de voz pode ter uma Linha associada, mas
//! > não é obrigatório.
//!
//! The tables have been in [`super::schema`] since migration 1, with every
//! column the spec names. What was missing was any way to put a row in one:
//! outside test modules, the whole repository could read the channel tree and
//! could not write to it. A server therefore had exactly the rooms
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
use seele_proto::control::{VoiceRoomInfo, ChannelInfo};
use seele_proto::ids::{VoiceRoomId, ChannelId};

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

    /// Every voice room, in the order a shell should draw them.
    ///
    /// `position` first and `id` as the tiebreak, so a server that has never
    /// reordered anything still lists its rooms in the order they were made
    /// rather than in whatever order SQLite finds them.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn voice_rooms(&self) -> Result<Vec<VoiceRoomInfo>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, member_limit, password_hash IS NOT NULL, channel_id
             FROM voice_rooms ORDER BY position, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(VoiceRoomInfo {
                    id: VoiceRoomId(row.get::<_, i64>(0)? as u32),
                    name: row.get(1)?,
                    limit: row.get::<_, i64>(2)? as u16,
                    password_required: row.get(3)?,
                    channel: row.get::<_, Option<i64>>(4)?.map(|id| ChannelId(id as u32)),
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Every Channel, in the order a shell should draw them.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn channels(&self) -> Result<Vec<ChannelInfo>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM channels ORDER BY position, id")?;
        let rows = statement
            .query_map([], |row| {
                Ok(ChannelInfo {
                    id: ChannelId(row.get::<_, i64>(0)? as u32),
                    name: row.get(1)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Makes a voice room, and returns it as the wire will carry it.
    ///
    /// `channel` binds a text channel to the room. It is checked against the
    /// `channels` table rather than trusted: SQLite enforces the foreign key, but
    /// the error it raises is a database error rather than something a caller
    /// can tell apart from the disk being full, and the difference matters to
    /// the person who mistyped a number.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if `channel` names a Channel that is not there, or a
    /// database error.
    pub fn create_voice_room(&self, name: &str, limit: u16, channel: Option<ChannelId>) -> Result<VoiceRoomInfo> {
        let name = name.trim();
        if let Some(channel) = channel {
            if !self.channel_exists(channel)? {
                return Err(NoSuchChannel.into());
            }
        }

        self.connection
            .execute(
                "INSERT INTO voice_rooms (name, member_limit, channel_id, position)
                 VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM voice_rooms))",
                params![
                    name,
                    i64::from(limit),
                    channel.map(|channel| i64::from(channel.get()))
                ],
            )
            .context("could not create the voice room")?;

        Ok(VoiceRoomInfo {
            id: VoiceRoomId(self.connection.last_insert_rowid() as u32),
            name: name.to_owned(),
            limit,
            // Nothing sets a password at creation. A room born locked is a room
            // whose maker has to tell everybody a secret before anybody can use
            // it, which is a separate decision taken later, with
            // `admissao::definir_senha_voice_room`.
            password_required: false,
            channel,
        })
    }

    /// Makes a Channel, and returns it as the wire will carry it.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn create_channel(&self, name: &str) -> Result<ChannelInfo> {
        let name = name.trim();
        self.connection
            .execute(
                "INSERT INTO channels (name, position)
                 VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM channels))",
                params![name],
            )
            .context("could not create the Channel")?;

        Ok(ChannelInfo {
            id: ChannelId(self.connection.last_insert_rowid() as u32),
            name: name.to_owned(),
        })
    }

    /// Renames a voice room. Returns the trimmed name that was stored.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such voice room, or a database error.
    pub fn rename_voice_room(&self, voice_room: VoiceRoomId, name: &str) -> Result<String> {
        let name = name.trim();
        let changed = self.connection.execute(
            "UPDATE voice_rooms SET name = ?1 WHERE id = ?2",
            params![name, i64::from(voice_room.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(name.to_owned())
    }

    /// Renames a Channel. Returns the trimmed name that was stored.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Channel, or a database error.
    pub fn rename_channel(&self, channel: ChannelId, name: &str) -> Result<String> {
        let name = name.trim();
        let changed = self.connection.execute(
            "UPDATE channels SET name = ?1 WHERE id = ?2",
            params![name, i64::from(channel.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(name.to_owned())
    }

    /// What destroying a Channel would cost, counted now.
    ///
    /// The three numbers the confirmation in the app is built out of, and they
    /// are read here rather than estimated anywhere else: a client holds one
    /// page of history and would guess low by the whole of the Channel's past.
    ///
    /// Messages already taken off the Channel by `remover_mensagem` are left out.
    /// [`super::messages::Messages::remove`] is soft — it clears the body and
    /// stamps `deleted_at`, and `history` filters those rows out — so they are
    /// gone from every screen already. Counting them would inflate what the
    /// reader is told they are about to lose by a number only the database can
    /// see.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Channel, or a database error.
    pub fn weigh_channel(&self, channel: ChannelId) -> Result<ChannelWeight> {
        if !self.channel_exists(channel)? {
            return Err(NoSuchChannel.into());
        }
        // One statement for the three numbers, and not three. Two of them would
        // be counted a moment apart otherwise, and a Channel being written to
        // while somebody weighs it could answer "1.847 messages by 7 people"
        // with the seventh person's only message in neither count.
        let (messages, authors, oldest) = self.connection.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT author_id), MIN(created_at)
             FROM messages WHERE channel_id = ?1 AND deleted_at IS NULL",
            params![i64::from(channel.get())],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )?;
        Ok(ChannelWeight {
            messages: messages.max(0) as u32,
            authors: authors.max(0) as u32,
            oldest_at_seconds: oldest,
        })
    }

    /// Destroys a voice room.
    ///
    /// # The last one is refused
    ///
    /// A server with na sala de voz has nowhere to speak, and speaking is what this
    /// product is. Somebody looking at a channel list with na sala de voz in it
    /// cannot tell a working server from a broken one — which is the exact
    /// condition [`crate::seed`] exists to prevent on the first boot, and it
    /// would be strange to spend a paragraph avoiding it there and then let a
    /// button walk into it.
    ///
    /// Refused by name rather than by foreign key, so the shell can say why
    /// instead of showing the sentence it shows when the disk is full.
    ///
    /// # The Channel bound to it survives
    ///
    /// `specs/04-servidor-seele.md` makes voice_rooms and Channels independent and the
    /// association optional. Destroying a voice room says nothing about the
    /// writing that happened to hang off it, and taking the Channel down with it
    /// would destroy history through a verb whose confirmation never mentioned
    /// any.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such voice room, [`LastVoiceRoom`] if it
    /// is the only one, or a database error.
    pub fn delete_voice_room(&self, voice_room: VoiceRoomId) -> Result<()> {
        let remaining: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM voice_rooms", [], |row| row.get(0))?;
        if remaining <= 1 {
            // Checked before "does it exist", deliberately: with one voice room left,
            // the answer is the same whether the identifier names it or names
            // nothing, and the useful half of the answer is the reason.
            return Err(LastVoiceRoom.into());
        }
        let changed = self.connection.execute(
            "DELETE FROM voice_rooms WHERE id = ?1",
            params![i64::from(voice_room.get())],
        )?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        Ok(())
    }

    /// Destroys a Channel, and everything written in it.
    ///
    /// Really destroys it. `remover_mensagem` is soft — it keeps the row so
    /// replies do not dangle and an operator can still answer "what was removed
    /// and by whom" — and this is the verb that takes the Channel those rows hang
    /// from, so there is nothing left for either purpose to be about. The
    /// confirmation in front of it says so in the same words.
    ///
    /// # Three writes, one transaction
    ///
    /// The messages go by `ON DELETE CASCADE`, which migration 1 already
    /// declares. The other two are the ones a cascade cannot do:
    ///
    /// - a voice room bound to this Channel keeps existing and loses the binding.
    ///   `voice_rooms.channel_id` has no `ON DELETE` clause, so without this the delete
    ///   fails on the foreign key and reaches the shell as a database error —
    ///   "could not destroy it", about a Channel whose only sin is being useful to
    ///   a room.
    /// - a reply **from another Channel** pointing at a message in this one is
    ///   unhooked first. `messages.replies_to` references `messages(id)` with
    ///   no `ON DELETE` either, so one cross-Channel reply is enough to make the
    ///   cascade fail — and nothing stops a client sending one.
    ///
    /// One transaction because a Channel half destroyed is worse than one not
    /// destroyed at all: rooms pointing at nothing, replies pointing at
    /// nothing, and a confirmation that already promised it was over.
    ///
    /// # Errors
    ///
    /// Returns [`NoSuchChannel`] if there is no such Channel, or a database error.
    pub fn delete_channel(&self, channel: ChannelId) -> Result<()> {
        let id = i64::from(channel.get());
        // `unchecked_transaction` because [`Channels`] borrows the connection
        // immutably, like every other method here. The nesting it does not
        // check for cannot happen: PERSISTENCE is one connection behind one mutex,
        // and this is the only place that opens a transaction on it outside the
        // migration runner, which runs before anybody is connected.
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE messages SET replies_to = NULL
             WHERE replies_to IN (SELECT id FROM messages WHERE channel_id = ?1)",
            params![id],
        )?;
        transaction.execute(
            "UPDATE voice_rooms SET channel_id = NULL WHERE channel_id = ?1",
            params![id],
        )?;
        let changed = transaction.execute("DELETE FROM channels WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(NoSuchChannel.into());
        }
        transaction.commit().context("could not destroy the Channel")?;
        Ok(())
    }

    fn channel_exists(&self, channel: ChannelId) -> Result<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM channels WHERE id = ?1",
            params![i64::from(channel.get())],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

/// The voice room or Channel named does not exist.
///
/// Enumerated rather than a sentence, like every other refusal that can reach a
/// client: the shell decides how to say it. `specs/02-protocolo.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no such voice room or Channel")]
pub struct NoSuchChannel;

/// The voice room named is the only one this server has.
///
/// Its own refusal rather than [`NoSuchChannel`], because the two ask different
/// things of whoever reads them: one means "check the identifier", this one
/// means "make another room first". `specs/02-protocolo.md` keeps the sentence
/// out of the protocol; the shell writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is the only voice room in the server")]
pub struct LastVoiceRoom;

/// What a Channel holds, as the confirmation in front of destroying it needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelWeight {
    /// How many messages are in it that anybody can read.
    pub messages: u32,
    /// How many distinct people wrote them.
    pub authors: u32,
    /// When the oldest was written, in seconds since the Unix epoch. `None`
    /// when the Channel is empty.
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
        assert!(channels.voice_rooms().unwrap().is_empty());
        assert!(channels.channels().unwrap().is_empty());
    }

    #[test]
    fn a_created_voice_room_reads_back_the_way_it_was_asked_for() {
        let persistence = store();
        let channels = Channels::new(&persistence);
        let channel = channels.create_channel("geral").unwrap();
        let voice_room = channels
            .create_voice_room("VOICE_ROOM-01 CENTRAL", 15, Some(channel.id))
            .unwrap();

        // What the creator is told, and what everybody else will read out of the
        // table, have to be the same thing — otherwise the room the maker sees
        // is not the room that exists.
        assert_eq!(channels.voice_rooms().unwrap(), vec![voice_room.clone()]);
        assert_eq!(voice_room.limit, 15);
        assert_eq!(voice_room.channel, Some(channel.id));
        assert!(!voice_room.password_required);
    }

    #[test]
    fn rooms_come_back_in_the_order_they_were_made() {
        // Without an explicit `position` this is whatever the query planner
        // feels like, and a channel list that reshuffles between two sessions is
        // a channel list nobody can build a habit around.
        let persistence = store();
        let channels = Channels::new(&persistence);
        for name in ["geral", "avisos", "planejamento"] {
            channels.create_channel(name).unwrap();
        }
        let names: Vec<String> = channels
            .channels()
            .unwrap()
            .into_iter()
            .map(|channel| channel.name)
            .collect();
        assert_eq!(names, ["geral", "avisos", "planejamento"]);
    }

    #[test]
    fn a_voice_room_bound_to_a_line_that_is_not_there_is_refused_by_name() {
        // The foreign key would stop it too, but it would stop it as a database
        // error — indistinguishable from the disk being full, and useless to the
        // person who mistyped a number.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let refused = channels.create_voice_room("VOICE_ROOM-02", 8, Some(ChannelId(404)));
        assert!(refused
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(
            channels.voice_rooms().unwrap().is_empty(),
            "the voice room was made anyway"
        );
    }

    #[test]
    fn renaming_something_that_is_not_there_says_so() {
        let persistence = store();
        let channels = Channels::new(&persistence);
        assert!(channels
            .rename_voice_room(VoiceRoomId(404), "fantasma")
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(channels
            .rename_channel(ChannelId(404), "fantasma")
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
        channels.create_channel("geral").unwrap();
        let segunda = channels.create_channel("avisos").unwrap();

        assert_eq!(
            channels.rename_channel(segunda.id, "recados").unwrap(),
            "recados"
        );
        let channels = channels.channels().unwrap();
        assert_eq!(channels[1].id, segunda.id);
        assert_eq!(channels[1].name, "recados");
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

    fn say(persistence: &Persistence, channel: ChannelId, author: i64, body: &str, at: i64) -> i64 {
        persistence
            .connection()
            .execute(
                "INSERT INTO messages (channel_id, author_id, body, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![i64::from(channel.get()), author, body, at],
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
        let channel = channels.create_channel("sync-geral").unwrap();
        let outra = channels.create_channel("avisos").unwrap();

        let rei = person(&persistence, "rei", 1);
        let shinji = person(&persistence, "shinji", 2);
        let asuka = person(&persistence, "asuka", 3);
        say(&persistence, channel.id, rei, "primeira", 1_678_600_000);
        say(&persistence, channel.id, rei, "segunda", 1_678_600_060);
        say(&persistence, channel.id, shinji, "terceira", 1_678_600_120);
        say(&persistence, channel.id, asuka, "quarta", 1_678_600_180);
        // Noutra Linha, e portanto em nenhuma destas contas.
        say(&persistence, outra.id, asuka, "noutra sala", 1_600_000_000);

        let peso = channels.weigh_channel(channel.id).unwrap();
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
        let channel = channels.create_channel("geral").unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, channel.id, rei, "fica", 100);
        let removida = say(&persistence, channel.id, rei, "removida", 50);
        persistence
            .connection()
            .execute(
                "UPDATE messages SET body = '', deleted_at = 1 WHERE id = ?1",
                params![removida],
            )
            .unwrap();

        let peso = channels.weigh_channel(channel.id).unwrap();
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
        let channel = channels.create_channel("nova").unwrap();
        let peso = channels.weigh_channel(channel.id).unwrap();
        assert_eq!(peso.messages, 0);
        assert_eq!(peso.authors, 0);
        assert_eq!(peso.oldest_at_seconds, None);
    }

    #[test]
    fn weighing_something_that_is_not_there_says_so() {
        let persistence = store();
        assert!(Channels::new(&persistence)
            .weigh_channel(ChannelId(404))
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
        let channel = channels.create_channel("geral").unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, channel.id, rei, "some junto", 100);

        channels.delete_channel(channel.id).unwrap();
        assert!(channels.channels().unwrap().is_empty());
        let left: i64 = persistence
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE channel_id = ?1",
                params![i64::from(channel.id.get())],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "the Channel went and its messages stayed");
    }

    #[test]
    fn a_voice_room_bound_to_a_destroyed_line_keeps_existing_without_it() {
        // `specs/04-servidor-seele.md` makes the association optional, so the
        // room outlives the Channel it pointed at. Without the unbinding, the
        // foreign key refuses the delete and the shell shows the sentence it
        // shows when the disk is full.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let channel = channels.create_channel("geral").unwrap();
        let voice_room = channels.create_voice_room("VOICE_ROOM-01", 8, Some(channel.id)).unwrap();

        channels.delete_channel(channel.id).unwrap();
        let voice_rooms = channels.voice_rooms().unwrap();
        assert_eq!(voice_rooms.len(), 1, "the voice room went with the Channel");
        assert_eq!(voice_rooms[0].id, voice_room.id);
        assert_eq!(voice_rooms[0].channel, None);
    }

    #[test]
    fn a_reply_from_another_line_does_not_block_the_destruction() {
        // `messages.replies_to` has no `ON DELETE`, so one cross-Channel reply is
        // enough to make the cascade fail — and nothing on the wire stops a
        // client sending one. Found here rather than in front of somebody.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let condenada = channels.create_channel("condenada").unwrap();
        let outra = channels.create_channel("outra").unwrap();
        let rei = person(&persistence, "rei", 1);
        let alvo = say(&persistence, condenada.id, rei, "original", 100);
        persistence
            .connection()
            .execute(
                "INSERT INTO messages (channel_id, author_id, body, created_at, replies_to)
                 VALUES (?1, ?2, 'resposta', 200, ?3)",
                params![i64::from(outra.id.get()), rei, alvo],
            )
            .unwrap();

        channels.delete_channel(condenada.id).unwrap();
        let pendurada: Option<i64> = persistence
            .connection()
            .query_row(
                "SELECT replies_to FROM messages WHERE channel_id = ?1",
                params![i64::from(outra.id.get())],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pendurada, None, "a reply is left pointing at nothing");
    }

    #[test]
    fn the_last_voice_room_is_refused_by_name() {
        // A server with na sala de voz has nowhere to speak, and somebody looking at a
        // channel list with na sala de voz in it cannot tell a working server from
        // a broken one. Refused with its own error, so the shell can say "make
        // another room first" instead of "check the identifier".
        let persistence = store();
        let channels = Channels::new(&persistence);
        let unica = channels.create_voice_room("VOICE_ROOM-01", 8, None).unwrap();

        assert!(channels
            .delete_voice_room(unica.id)
            .unwrap_err()
            .downcast_ref::<LastVoiceRoom>()
            .is_some());
        assert_eq!(channels.voice_rooms().unwrap().len(), 1);

        // A segunda sala é o que destrava a primeira.
        let segunda = channels.create_voice_room("VOICE_ROOM-02", 8, None).unwrap();
        channels.delete_voice_room(unica.id).unwrap();
        let restantes = channels.voice_rooms().unwrap();
        assert_eq!(restantes.len(), 1);
        assert_eq!(restantes[0].id, segunda.id);
    }

    #[test]
    fn destroying_a_voice_room_leaves_the_line_it_was_bound_to_alone() {
        // The other half of "voice_rooms and Channels are independent". A voice room
        // going away is no statement about the writing hanging off it, and
        // taking the Channel with it would destroy history through a verb whose
        // confirmation never mentioned any.
        let persistence = store();
        let channels = Channels::new(&persistence);
        let channel = channels.create_channel("geral").unwrap();
        let voice_room = channels.create_voice_room("VOICE_ROOM-01", 8, Some(channel.id)).unwrap();
        channels.create_voice_room("VOICE_ROOM-02", 8, None).unwrap();
        let rei = person(&persistence, "rei", 1);
        say(&persistence, channel.id, rei, "sobrevive", 100);

        channels.delete_voice_room(voice_room.id).unwrap();
        assert_eq!(channels.weigh_channel(channel.id).unwrap().messages, 1);
        assert_eq!(channels.channels().unwrap(), vec![channel]);
    }

    #[test]
    fn destroying_a_voice_room_that_is_not_there_says_so() {
        let persistence = store();
        let channels = Channels::new(&persistence);
        channels.create_voice_room("VOICE_ROOM-01", 8, None).unwrap();
        channels.create_voice_room("VOICE_ROOM-02", 8, None).unwrap();
        assert!(channels
            .delete_voice_room(VoiceRoomId(404))
            .unwrap_err()
            .downcast_ref::<NoSuchChannel>()
            .is_some());
        assert!(channels
            .delete_channel(ChannelId(404))
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
        let channel = channels.create_channel("  geral \n").unwrap();
        assert_eq!(channel.name, "geral");
        assert_eq!(channels.channels().unwrap()[0].name, "geral");

        let voice_room = channels.create_voice_room("\tVOICE_ROOM-01  ", 4, None).unwrap();
        assert_eq!(voice_room.name, "VOICE_ROOM-01");
        assert_eq!(
            channels.rename_voice_room(voice_room.id, " VOICE_ROOM-02 ").unwrap(),
            "VOICE_ROOM-02"
        );
        assert_eq!(channels.voice_rooms().unwrap()[0].name, "VOICE_ROOM-02");
    }
}
