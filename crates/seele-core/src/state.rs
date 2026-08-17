//! What is true about the session right now.
//!
//! A fold over [`ServerMessage`]: which pilots are in which Cage, what they are
//! called, what state they announced, what has been said. Nothing here draws
//! anything or decides what a human should read.
//!
//! # Why this is in the core and not in the shells
//!
//! `specs/01-arquitetura.md` is blunt about the consequence of getting this
//! wrong: "Se uma funcionalidade precisa ser implementada duas vezes em duas
//! interfaces diferentes, ela está no lugar errado."
//!
//! The M4 terminal client kept this bookkeeping itself, and it worked. Then
//! `specs/06-clientes-gui.md` asked `seele-ffi` for `estado_atual() -> Snapshot`
//! — the same map from pilot to nickname, the same seats, the same "who is
//! speaking". Building that separately would have been the second
//! implementation, with the third waiting in M6.
//!
//! So it moved here. What is left in a shell is the part that genuinely differs
//! between a terminal and a window: how to flatten this into rows, how to
//! format a timestamp, how wide a name may be.
//!
//! # What a shell still owns
//!
//! Ordering for display, truncation, colour, and anything involving a clock or
//! a locale. This module has no opinion about what time it is.

use std::collections::HashMap;

use seele_proto::control::{CageInfo, LineInfo, Permission, PilotState};
use seele_proto::ids::{CageId, LineId, MessageId, PilotId, Ssrc};
use seele_proto::sync_ratio::SyncBand;
use seele_proto::ServerMessage;

use crate::client::SessionInfo;

/// One pilot, as far as this client knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Pilot {
    /// Account identifier.
    pub id: PilotId,
    /// Display name.
    pub nickname: String,
    /// Their media source, for per-talker volume.
    pub ssrc: Option<Ssrc>,
    /// Microphone muted — A.T. Field.
    pub at_field: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
    /// Transmitting right now.
    pub speaking: bool,
    /// Sync Ratio, 0 to 100.
    pub sync_ratio: u8,
}

impl Pilot {
    fn new(id: PilotId, nickname: String, ssrc: Option<Ssrc>) -> Self {
        Self {
            id,
            nickname,
            ssrc,
            at_field: false,
            total_isolation: false,
            speaking: false,
            // Zero rather than a hopeful hundred: an unmeasured Sync Ratio that
            // reads as perfect is worse than one that reads as unknown, because
            // it looks like an answer.
            sync_ratio: 0,
        }
    }
}

/// One thing somebody said.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Server-assigned identifier, ordered.
    pub id: MessageId,
    /// Which Line it was said in.
    pub line: LineId,
    /// Who said it.
    pub author: PilotId,
    /// Their name at the time it arrived.
    pub author_nickname: String,
    /// When the server accepted it, in **seconds** since the Unix epoch.
    ///
    /// The server's clock. Turning it into something a person reads — a local
    /// time, a locale, a relative "há 3 min" — is a shell's job, and this
    /// module has no opinion about what time it is.
    pub at_seconds: i64,
    /// The body.
    pub body: String,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// Whether this client sent it.
    pub own: bool,
    /// Whether it has been edited since.
    pub edited: bool,
}

/// Something the interface should surface, already carrying its severity.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// How loud.
    pub severity: seele_proto::control::AlertSeverity,
    /// What about. Enumerated, so each shell writes its own sentence.
    pub reason: seele_proto::control::AlertReason,
    /// The operator's own words, when they have any.
    pub operator_text: Option<String>,
}

/// The average Sync Ratio of a Cage, already banded.
///
/// The comp (`design/Entry Plug v2.dc.html`) draws this as **MÉDIA DO CAGE**, a
/// number in the band's colour with the sample size beside it, and computes
/// both in the shell. Here it is computed once, in the core, for the same
/// reason `seele_ffi::types` gives for carrying a band beside every pilot's
/// number: a threshold known by two shells is a threshold two shells disagree
/// about the day one of them is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CageSync {
    /// The mean of the seated pilots' ratios, 0 to 100.
    ///
    /// Rounded to the nearest point, ties up. The comp prints `82.4`, but the
    /// datum is a `u8` at every point it exists — on the wire, in
    /// [`Pilot::sync_ratio`], in the smoothing — so a decimal here would be
    /// precision invented at the last step. `82` is what is known.
    pub ratio: u8,
    /// Which band that mean falls into.
    pub band: SyncBand,
    /// How many pilots it is the mean of — the comp's `5 PLUGS`.
    ///
    /// Carried so a shell can say what the number is an average *of* without
    /// counting the roster a second time and getting a different answer.
    pub pilots: usize,
}

/// Why the session ended, if it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    /// Enumerated reason.
    pub reason: seele_proto::control::DisconnectReason,
}

/// Everything this client knows about the Dogma it is attached to.
#[derive(Debug, Default, Clone)]
pub struct Room {
    /// Which pilot this connection is.
    pub me: Option<PilotId>,
    /// The media source the server assigned — gap G1.
    pub ssrc: Option<Ssrc>,
    /// What the Dogma is called.
    pub dogma: String,
    /// Voice channels visible to this pilot.
    pub cages: Vec<CageInfo>,
    /// Text channels visible to this pilot.
    pub lines: Vec<LineInfo>,
    /// What this pilot may do, as MELCHIOR resolved it.
    ///
    /// Here so a shell can ask "should this control exist at all" without
    /// re-deriving `specs/04-servidor-seele.md`'s "negadas vencem concedidas"
    /// for itself. **Convenience, never enforcement**: the server checks again.
    pub permissions: Vec<Permission>,
    /// The Cage this pilot's plug is in.
    pub current_cage: Option<CageId>,
    /// The Line being read.
    pub current_line: Option<LineId>,
    /// Everybody this client has heard of, by id.
    pub pilots: HashMap<PilotId, Pilot>,
    /// Who is seated in which Cage, in arrival order.
    pub seats: HashMap<CageId, Vec<PilotId>>,
    /// What has been said, oldest first.
    pub messages: Vec<Message>,
    /// Connection quality as the server reports it.
    pub telemetry: Option<seele_proto::control::Telemetry>,
    /// The last thing worth surfacing.
    pub notice: Option<Notice>,
    /// Set once the session is over.
    pub ended: Option<Ended>,
}

/// What changed, so a shell can redraw only what it must.
///
/// Returned rather than inferred: a shell that has to diff two snapshots to
/// find out a message arrived is a shell doing the core's work again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    /// Somebody joined, left, or changed state.
    pub roster: bool,
    /// A message arrived, changed, or went away.
    pub messages: bool,
    /// Cages or Lines changed.
    pub channels: bool,
    /// New measurements.
    pub telemetry: bool,
    /// A notice was raised.
    pub notice: bool,
    /// The session ended.
    pub ended: bool,
}

impl Changed {
    /// Whether anything at all changed.
    #[must_use]
    pub fn any(self) -> bool {
        self.roster || self.messages || self.channels || self.telemetry || self.notice || self.ended
    }
}

impl Room {
    /// A room that has not heard anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds from what the handshake already established.
    ///
    /// The handshake consumes the `Session` message while establishing PADRÃO:
    /// AZUL, so no shell ever sees it go by. Without this a client that
    /// connected perfectly shows an empty Dogma — which looks exactly like a
    /// server with no Cages.
    ///
    /// `nickname` is what this client asked to be called. It has to be passed
    /// in because the server announces arrivals to everybody *else*: nothing on
    /// the wire ever tells this client who it is.
    pub fn adopt(&mut self, info: &SessionInfo, nickname: &str) {
        self.me = Some(info.pilot);
        self.ssrc = Some(info.ssrc);
        self.dogma = info.dogma.clone();
        self.cages = info.cages.clone();
        self.lines = info.lines.clone();
        self.permissions = info.permissions.clone();
        self.pilots.insert(
            info.pilot,
            Pilot::new(info.pilot, nickname.to_owned(), Some(info.ssrc)),
        );
    }

    /// Records that this pilot's plug is now in a Cage, and seats them in it.
    ///
    /// Called on the way *out*, when the client asks — not on the way in. The
    /// server confirms a Cage entry by silence, and a roster that waits for a
    /// message that never comes shows an empty room to the person sitting in it.
    pub fn enter_cage(&mut self, cage: CageId) {
        if let Some(me) = self.me {
            if let Some(previous) = self.current_cage {
                if let Some(seats) = self.seats.get_mut(&previous) {
                    seats.retain(|seated| *seated != me);
                }
            }
            let seats = self.seats.entry(cage).or_default();
            if !seats.contains(&me) {
                seats.push(me);
            }
        }
        self.current_cage = Some(cage);
    }

    /// Records that the client is now reading a Line.
    ///
    /// Clears the messages, because a new Line is a new conversation and keeping
    /// the old one under a new heading misattributes every line of it.
    pub fn open_line(&mut self, line: LineId) {
        if self.current_line != Some(line) {
            self.messages.clear();
        }
        self.current_line = Some(line);
    }

    /// The pilots seated in a Cage, in arrival order.
    pub fn roster(&self, cage: CageId) -> impl Iterator<Item = &Pilot> {
        self.seats
            .get(&cage)
            .into_iter()
            .flatten()
            .filter_map(|id| self.pilots.get(id))
    }

    /// The pilots in whichever Cage this client is in.
    pub fn current_roster(&self) -> impl Iterator<Item = &Pilot> {
        self.current_cage
            .into_iter()
            .flat_map(|cage| self.roster(cage))
    }

    /// The average Sync Ratio of a Cage, banded, or `None` if nobody is in it.
    ///
    /// # An empty Cage has no average
    ///
    /// Not zero. Zero is a measurement, and by the bands it is a critical one:
    /// a Dogma with four idle Cages would show four red rooms nobody is in,
    /// and the one Cage that is genuinely in trouble would stop standing out.
    /// `None` says "nothing to average", which is the truth, and leaves the
    /// shell to draw the absence — the comp draws `——` for a plug with no
    /// number, and this is the same case one level up.
    ///
    /// # An ejected pilot does not count
    ///
    /// Ejecting a plug leaves the Cage: the pilot comes out of `seats` — via
    /// [`Self::enter_cage`] for this client, or `PilotLeft` for anybody else —
    /// while staying in `pilots`, because their name is still needed for what
    /// they already said. This averages the *seats*, so somebody who left stops
    /// counting the moment they leave and cannot drag the room's number down
    /// from outside it. There is no third state: this client has no notion of a
    /// pilot who is in the Cage but ejected.
    #[must_use]
    pub fn cage_sync(&self, cage: CageId) -> Option<CageSync> {
        let mut total: u32 = 0;
        let mut pilots: usize = 0;
        for pilot in self.roster(cage) {
            total += u32::from(pilot.sync_ratio);
            pilots += 1;
        }

        let count = u32::try_from(pilots).ok()?;
        if count == 0 {
            return None;
        }

        // Integer arithmetic, ties up: `(2a + n) / 2n` is the mean rounded to
        // the nearest point without a float ever holding the value. A float
        // would round the same way here and invite a `.1` onto the screen
        // later, which is precision the datum does not carry.
        let mean = (2 * total + count) / (2 * count);
        let ratio = u8::try_from(mean).unwrap_or(u8::MAX);

        Some(CageSync {
            ratio,
            band: SyncBand::of(ratio),
            pilots,
        })
    }

    /// The average for whichever Cage this client is in.
    #[must_use]
    pub fn current_cage_sync(&self) -> Option<CageSync> {
        self.current_cage.and_then(|cage| self.cage_sync(cage))
    }

    /// A pilot's media source, addressed by nickname.
    #[must_use]
    pub fn ssrc_of(&self, nickname: &str) -> Option<Ssrc> {
        self.pilots
            .values()
            .find(|pilot| pilot.nickname.eq_ignore_ascii_case(nickname))
            .and_then(|pilot| pilot.ssrc)
    }

    /// The Cage matching a name or a number.
    #[must_use]
    pub fn find_cage(&self, which: &str) -> Option<CageId> {
        if let Ok(number) = which.parse::<u32>() {
            if let Some(cage) = self.cages.iter().find(|cage| cage.id.0 == number) {
                return Some(cage.id);
            }
        }
        let wanted = which.to_lowercase();
        self.cages
            .iter()
            .find(|cage| cage.name.to_lowercase().contains(&wanted))
            .map(|cage| cage.id)
    }

    /// The Line matching a name or a number, with or without its `#`.
    #[must_use]
    pub fn find_line(&self, which: &str) -> Option<LineId> {
        let wanted = which.trim_start_matches('#').to_lowercase();
        if let Ok(number) = wanted.parse::<u32>() {
            if let Some(line) = self.lines.iter().find(|line| line.id.0 == number) {
                return Some(line.id);
            }
        }
        self.lines
            .iter()
            .find(|line| line.name.to_lowercase().contains(&wanted))
            .map(|line| line.id)
    }

    /// The name to show for a pilot, even one never introduced.
    ///
    /// Joining a Line mid-conversation means the first thing said can come from
    /// somebody whose arrival was never seen. Showing an id beats dropping it.
    #[must_use]
    pub fn name_of(&self, pilot: PilotId) -> String {
        self.pilots
            .get(&pilot)
            .map_or_else(|| format!("piloto {}", pilot.0), |p| p.nickname.clone())
    }

    /// Folds one message from the server into this room.
    #[allow(
        clippy::too_many_lines,
        reason = "one match over the protocol; splitting it hides which messages are handled"
    )]
    pub fn apply(&mut self, message: &ServerMessage) -> Changed {
        let mut changed = Changed::default();

        match message {
            ServerMessage::Session {
                pilot,
                ssrc,
                dogma,
                cages,
                lines,
                permissions,
                ..
            } => {
                self.me = Some(*pilot);
                self.ssrc = Some(*ssrc);
                self.dogma.clone_from(dogma);
                self.cages.clone_from(cages);
                self.lines.clone_from(lines);
                self.permissions.clone_from(permissions);
                self.pilots
                    .entry(*pilot)
                    .or_insert_with(|| Pilot::new(*pilot, format!("piloto {}", pilot.0), None))
                    .ssrc = Some(*ssrc);
                changed.channels = true;
                changed.roster = true;
            }

            ServerMessage::PilotJoined {
                cage,
                profile,
                ssrc,
            } => {
                let pilot = self
                    .pilots
                    .entry(profile.id)
                    .or_insert_with(|| Pilot::new(profile.id, profile.nickname.clone(), None));
                pilot.nickname.clone_from(&profile.nickname);
                pilot.ssrc = Some(*ssrc);

                let seats = self.seats.entry(*cage).or_default();
                if !seats.contains(&profile.id) {
                    seats.push(profile.id);
                }
                changed.roster = true;
            }

            ServerMessage::PilotLeft { cage, pilot } => {
                if let Some(seats) = self.seats.get_mut(cage) {
                    seats.retain(|seated| seated != pilot);
                }
                // The pilot stays in `pilots`: their name is still needed to
                // attribute everything they said before leaving.
                if let Some(known) = self.pilots.get_mut(pilot) {
                    known.speaking = false;
                }
                changed.roster = true;
            }

            ServerMessage::PilotState(state) => {
                self.absorb_state(state);
                changed.roster = true;
            }

            ServerMessage::MessageReceived {
                line,
                id,
                author,
                at_seconds,
                author_nickname,
                body,
                replies_to,
                ..
            } => {
                // Idempotent by server id: a history fetch that overlaps what is
                // already on screen must not double every line in the overlap.
                if self.messages.iter().any(|known| known.id == *id) {
                    return changed;
                }
                self.messages.push(Message {
                    id: *id,
                    line: *line,
                    author: *author,
                    // The server's name for them wins over ours only when we
                    // have none: somebody we watched arrive is somebody whose
                    // name we already learned, and history for a stranger is
                    // the case this field exists for.
                    author_nickname: self
                        .pilots
                        .get(author)
                        .map_or_else(|| author_nickname.clone(), |pilot| pilot.nickname.clone()),
                    at_seconds: *at_seconds,
                    body: body.clone(),
                    replies_to: *replies_to,
                    own: Some(*author) == self.me,
                    edited: false,
                });
                // History arrives oldest-first per page, but a page fetched
                // after live messages have landed would otherwise sit at the
                // bottom. Sorting by the server's own ordering is the only
                // ordering every client agrees on.
                self.messages.sort_by_key(|message| message.id);
                changed.messages = true;
            }

            ServerMessage::MessageEdited { id, body, .. } => {
                if let Some(message) = self.messages.iter_mut().find(|known| known.id == *id) {
                    message.body.clone_from(body);
                    message.edited = true;
                    changed.messages = true;
                }
            }

            ServerMessage::MessageRemoved { id, .. } => {
                let before = self.messages.len();
                self.messages.retain(|message| message.id != *id);
                changed.messages = self.messages.len() != before;
            }

            ServerMessage::Telemetry(telemetry) => {
                self.telemetry = Some(telemetry.clone());
                changed.telemetry = true;
            }

            ServerMessage::Alert {
                severity,
                reason,
                operator_text,
            } => {
                self.notice = Some(Notice {
                    severity: *severity,
                    reason: *reason,
                    operator_text: operator_text.clone(),
                });
                changed.notice = true;
            }

            ServerMessage::Disconnecting { reason } => {
                self.ended = Some(Ended { reason: *reason });
                changed.ended = true;
            }

            // A room somebody made while this client was already connected.
            //
            // Appended rather than re-sorted: the server sends these in the
            // order it made them and lists them in that same order at the next
            // handshake, so appending is what keeps the list this client is
            // looking at and the list it would get on reconnecting identical.
            //
            // Idempotent by identifier, like `MessageReceived` above. Nothing
            // sends one twice today; a client that reconnects and folds an old
            // event in would otherwise show the same room in the list twice, and
            // a duplicated room is one nobody can tell their friends to join.
            ServerMessage::CageCreated { cage } => {
                if !self.cages.iter().any(|known| known.id == cage.id) {
                    self.cages.push(cage.clone());
                    changed.channels = true;
                }
            }

            ServerMessage::LineCreated { line } => {
                if !self.lines.iter().any(|known| known.id == line.id) {
                    self.lines.push(line.clone());
                    changed.channels = true;
                }
            }

            ServerMessage::CageRenamed { cage, name } => {
                if let Some(known) = self.cages.iter_mut().find(|known| known.id == *cage) {
                    known.name.clone_from(name);
                    changed.channels = true;
                }
            }

            ServerMessage::LineRenamed { line, name } => {
                if let Some(known) = self.lines.iter_mut().find(|known| known.id == *line) {
                    known.name.clone_from(name);
                    changed.channels = true;
                }
            }

            // Somebody with the permission moved this plug.
            //
            // The same bookkeeping [`Self::enter_cage`] does when *this* client
            // asks, and it has to be here as well as there: entering a Cage is
            // recorded on the way out, when the client sends the request, and a
            // move is a Cage this client never asked for. Without this the
            // person is drawn in the room they left, sending voice into it,
            // reading its roster.
            //
            // The sentence that says it happened arrives separately, as an
            // `Alert` carrying `MovedByOperator`. Two frames rather than one
            // because they are two different things: this is where the plug is,
            // that is what the person should be told, and only the shell knows
            // how to say the second.
            ServerMessage::MovedToCage { cage } => {
                self.enter_cage(*cage);
                changed.roster = true;
            }

            // Consumed by the handshake and by the round-trip measurement, both
            // of which are over before any shell is watching.
            ServerMessage::Challenge { .. } | ServerMessage::Pong { .. } => {}
        }

        changed
    }

    fn absorb_state(&mut self, state: &PilotState) {
        let pilot = self
            .pilots
            .entry(state.pilot)
            .or_insert_with(|| Pilot::new(state.pilot, format!("piloto {}", state.pilot.0), None));
        pilot.at_field = state.at_field;
        pilot.total_isolation = state.total_isolation;
        pilot.speaking = state.speaking;
        pilot.sync_ratio = state.sync_ratio;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::control::{
        AlertReason, AlertSeverity, DisconnectReason, PilotProfile, Presence, Telemetry,
    };
    use seele_proto::ids::SessionId;

    const CAGE: CageId = CageId(1);
    const LINE: LineId = LineId(1);

    fn session() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![CageInfo {
                id: CAGE,
                name: "CAGE-01 CENTRAL".into(),
                limit: 20,
                password_required: false,
                line: Some(LINE),
            }],
            lines: vec![LineInfo {
                id: LINE,
                name: "geral".into(),
            }],
            roles: Vec::new(),
            permissions: Vec::new(),
        }
    }

    fn joined(id: u64, nickname: &str) -> ServerMessage {
        ServerMessage::PilotJoined {
            cage: CAGE,
            profile: PilotProfile {
                id: PilotId(id),
                nickname: nickname.into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
        }
    }

    fn said(id: u64, author: u64, body: &str) -> ServerMessage {
        ServerMessage::MessageReceived {
            line: LINE,
            id: MessageId(id),
            author: PilotId(author),
            author_nickname: format!("piloto {author}"),
            // Deliberately unrelated to the id: the server's clock can tie or
            // go backwards across a restart, and ordering must not depend on it.
            at_seconds: 1_700_000_000 - i64::try_from(id).expect("at"),
            body: body.into(),
            replies_to: None,
            client_message_id: None,
        }
    }

    fn room() -> Room {
        let mut room = Room::new();
        room.apply(&session());
        room.enter_cage(CAGE);
        room.open_line(LINE);
        room
    }

    #[test]
    fn the_session_message_fills_in_the_dogma_and_the_channels() {
        let mut room = Room::new();
        let changed = room.apply(&session());

        assert!(changed.channels);
        assert_eq!(room.dogma, "Terceira Tóquio");
        assert_eq!(room.cages.len(), 1);
        assert_eq!(room.lines.len(), 1);
        assert_eq!(room.me, Some(PilotId(7)));
    }

    #[test]
    fn the_pilot_is_seated_in_their_own_roster() {
        // The server announces arrivals to everybody else, so nothing on the
        // wire ever names this client to itself. The one person missing from
        // the roster would be the person reading it.
        let mut room = Room::new();
        let info = SessionInfo {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: Vec::new(),
            lines: Vec::new(),
            permissions: Vec::new(),
        };
        room.adopt(&info, "ayanami");
        room.enter_cage(CAGE);

        let names: Vec<&str> = room
            .current_roster()
            .map(|pilot| pilot.nickname.as_str())
            .collect();
        assert_eq!(names, ["ayanami"]);
    }

    #[test]
    fn moving_between_cages_does_not_leave_a_copy_behind() {
        let mut room = room();
        room.enter_cage(CageId(2));

        assert_eq!(room.roster(CAGE).count(), 0);
        assert_eq!(room.roster(CageId(2)).count(), 1);
    }

    #[test]
    fn a_pilot_who_leaves_keeps_their_name_for_what_they_already_said() {
        // Dropping the name with the pilot would turn every line they wrote
        // into "piloto 3" the moment they close their client.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        room.apply(&said(1, 3, "verificando harmônicos"));
        room.apply(&ServerMessage::PilotLeft {
            cage: CAGE,
            pilot: PilotId(3),
        });

        assert_eq!(room.current_roster().count(), 1, "only us should be left");
        assert_eq!(room.name_of(PilotId(3)), "ayanami");
        assert_eq!(room.messages[0].author_nickname, "ayanami");
    }

    #[test]
    fn the_same_message_arriving_twice_is_stored_once() {
        // A history fetch that overlaps what is already on screen must not
        // double every line in the overlap.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));

        let first = room.apply(&said(1, 3, "olá"));
        let second = room.apply(&said(1, 3, "olá"));

        assert!(first.messages);
        assert!(!second.messages, "a duplicate was reported as a change");
        assert_eq!(room.messages.len(), 1);
    }

    #[test]
    fn messages_end_up_in_the_servers_order_however_they_arrive() {
        // A page of history fetched after live messages landed would otherwise
        // sit underneath them, which reads as the past happening last.
        let mut room = room();
        room.apply(&said(10, 7, "recente"));
        room.apply(&said(2, 7, "antigo"));
        room.apply(&said(5, 7, "meio"));

        let bodies: Vec<&str> = room.messages.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["antigo", "meio", "recente"]);
    }

    #[test]
    fn an_edit_rewrites_in_place_instead_of_appending() {
        let mut room = room();
        room.apply(&said(1, 7, "sync caiu"));
        let changed = room.apply(&ServerMessage::MessageEdited {
            line: LINE,
            id: MessageId(1),
            body: "sync voltou".into(),
        });

        assert!(changed.messages);
        assert_eq!(room.messages.len(), 1, "the edit appended a second line");
        assert_eq!(room.messages[0].body, "sync voltou");
        assert!(room.messages[0].edited);
    }

    #[test]
    fn editing_something_not_on_screen_changes_nothing() {
        let mut room = room();
        let changed = room.apply(&ServerMessage::MessageEdited {
            line: LINE,
            id: MessageId(99),
            body: "fantasma".into(),
        });

        assert!(!changed.messages);
        assert!(room.messages.is_empty());
    }

    #[test]
    fn a_removal_takes_the_message_off_the_screen() {
        let mut room = room();
        room.apply(&said(1, 7, "apagar isto"));
        let changed = room.apply(&ServerMessage::MessageRemoved {
            line: LINE,
            id: MessageId(1),
        });

        assert!(changed.messages);
        assert!(room.messages.is_empty());
    }

    #[test]
    fn our_own_messages_are_marked_as_ours() {
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        room.apply(&said(1, 3, "deles"));
        room.apply(&said(2, 7, "nosso"));

        assert!(!room.messages[0].own);
        assert!(room.messages[1].own);
    }

    #[test]
    fn a_message_carries_the_servers_clock_and_not_the_arrival_time() {
        // A page of history whose lines all claim to have been written the
        // moment the app opened has lost what makes it history.
        let mut room = room();
        room.apply(&said(1, 7, "olá"));
        assert_eq!(room.messages[0].at_seconds, 1_700_000_000 - 1);
    }

    #[test]
    fn ordering_follows_the_id_and_not_the_clock() {
        // The server's wall clock can tie, and can go backwards across a
        // restart or an NTP step. Ids are monotonic; that is why they exist.
        let mut room = room();
        room.apply(&said(3, 7, "terceiro"));
        room.apply(&said(1, 7, "primeiro"));
        room.apply(&said(2, 7, "segundo"));

        let bodies: Vec<&str> = room.messages.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["primeiro", "segundo", "terceiro"]);
    }

    #[test]
    fn a_message_from_somebody_never_introduced_is_still_kept() {
        let mut room = room();
        room.apply(&said(1, 99, "olá"));

        assert_eq!(room.messages.len(), 1);
        assert_eq!(room.messages[0].author_nickname, "piloto 99");
    }

    #[test]
    fn state_updates_reach_the_pilot_they_are_about() {
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        room.apply(&ServerMessage::PilotState(PilotState {
            pilot: PilotId(3),
            at_field: true,
            total_isolation: false,
            speaking: true,
            presence: Presence::Available,
            sync_ratio: 42,
        }));

        let pilot = room.pilots.get(&PilotId(3)).expect("pilot");
        assert!(pilot.at_field);
        assert!(pilot.speaking);
        assert_eq!(pilot.sync_ratio, 42);
    }

    #[test]
    fn an_unmeasured_sync_ratio_reads_as_zero_and_not_as_perfect() {
        // A hundred that nobody measured looks like an answer. A zero looks
        // like a question, which is what it is.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        assert_eq!(room.pilots[&PilotId(3)].sync_ratio, 0);
    }

    #[test]
    fn changing_the_line_clears_what_belonged_to_the_old_one() {
        let mut room = room();
        room.apply(&said(1, 7, "na linha 1"));
        room.open_line(LineId(2));

        assert!(room.messages.is_empty());
        assert_eq!(room.current_line, Some(LineId(2)));
    }

    #[test]
    fn opening_the_line_already_open_keeps_the_conversation() {
        // Re-entering the same Line — a reconnection, a redraw — must not throw
        // the conversation away.
        let mut room = room();
        room.apply(&said(1, 7, "ainda aqui"));
        room.open_line(LINE);

        assert_eq!(room.messages.len(), 1);
    }

    #[test]
    fn telemetry_and_notices_are_reported_as_separate_changes() {
        // A shell that redraws the roster because the round trip moved is a
        // shell redrawing thirty times a second for nothing.
        let mut room = room();

        let changed = room.apply(&ServerMessage::Telemetry(Telemetry {
            rtt_ms: 38.0,
            jitter_ms: 12.0,
            loss_fraction: 0.002,
            subsystems: Vec::new(),
        }));
        assert!(changed.telemetry);
        assert!(!changed.roster);
        assert!(!changed.messages);

        let changed = room.apply(&ServerMessage::Alert {
            severity: AlertSeverity::Critical,
            reason: AlertReason::PermissionDenied,
            operator_text: None,
        });
        assert!(changed.notice);
        assert!(!changed.telemetry);
    }

    #[test]
    fn the_end_of_a_session_is_recorded_with_its_reason() {
        let mut room = room();
        let changed = room.apply(&ServerMessage::Disconnecting {
            reason: DisconnectReason::Banned,
        });

        assert!(changed.ended);
        assert_eq!(
            room.ended.map(|end| end.reason),
            Some(DisconnectReason::Banned)
        );
    }

    #[test]
    fn a_handshake_message_changes_nothing_a_shell_would_redraw() {
        let mut room = room();
        assert!(!room.apply(&ServerMessage::Pong { timestamp: 1 }).any());
        assert!(!room
            .apply(&ServerMessage::Challenge { nonce: vec![1, 2] })
            .any());
    }

    #[test]
    fn a_cage_is_found_by_number_or_by_part_of_its_name() {
        let room = room();
        assert_eq!(room.find_cage("1"), Some(CAGE));
        assert_eq!(room.find_cage("central"), Some(CAGE));
        assert_eq!(room.find_cage("CENTRAL"), Some(CAGE));
        assert_eq!(room.find_cage("geofront"), None);
    }

    #[test]
    fn a_line_is_found_with_or_without_its_hash() {
        let room = room();
        assert_eq!(room.find_line("#geral"), Some(LINE));
        assert_eq!(room.find_line("geral"), Some(LINE));
        assert_eq!(room.find_line("1"), Some(LINE));
    }

    #[test]
    fn a_media_source_is_addressable_by_nickname() {
        // What `:volume ayanami 40` needs, in the one place that knows both.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));

        assert_eq!(room.ssrc_of("ayanami"), Some(Ssrc(30)));
        assert_eq!(room.ssrc_of("AYANAMI"), Some(Ssrc(30)));
        assert_eq!(room.ssrc_of("ninguém"), None);
    }

    /// Seats a pilot in `CAGE` and gives them a Sync Ratio.
    fn seated_with(room: &mut Room, id: u64, nickname: &str, sync: u8) {
        room.apply(&joined(id, nickname));
        room.apply(&ServerMessage::PilotState(PilotState {
            pilot: PilotId(id),
            at_field: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            sync_ratio: sync,
        }));
    }

    /// The Cage seen by somebody whose own plug is elsewhere, so the fixture's
    /// own zero does not have to be reasoned about in every average.
    fn watching() -> Room {
        let mut room = Room::new();
        room.apply(&session());
        room
    }

    #[test]
    fn an_empty_cage_has_no_average_rather_than_a_zero() {
        // Zero is a measurement, and by the bands a critical one. Four idle
        // Cages drawn in red is four alarms nobody set, and the Cage that is
        // genuinely in trouble stops standing out among them.
        let room = room();
        assert_eq!(room.cage_sync(CageId(9)), None, "a Cage nobody is in");
        assert_eq!(watching().cage_sync(CAGE), None, "a Cage nobody has sat in");

        // And the pilot's own Cage, before anybody is in it, has no average
        // either — including no average of nobody.
        let mut alone = watching();
        assert_eq!(alone.current_cage_sync(), None, "no Cage entered yet");
        alone.enter_cage(CAGE);
        assert_eq!(
            alone.current_cage_sync().map(|sync| sync.pilots),
            Some(1),
            "the pilot sitting in it is one pilot"
        );
    }

    #[test]
    fn the_average_is_the_seated_pilots_rounded_to_a_whole_point() {
        // 90 + 81 + 78 = 249, over three, is 83 exactly. The band is read off
        // the average and not off the members: two of these three are nominal
        // on their own, and together the room is degraded.
        let mut room = watching();
        seated_with(&mut room, 3, "ayanami", 90);
        seated_with(&mut room, 4, "asuka", 81);
        seated_with(&mut room, 5, "shinji", 78);

        let average = room.cage_sync(CAGE).expect("three pilots are seated");
        assert_eq!(average.ratio, 83);
        assert_eq!(average.band, SyncBand::Degraded);
        assert_eq!(average.pilots, 3);
    }

    #[test]
    fn a_half_point_rounds_up_and_never_lands_between_two_numbers() {
        // 84 and 85 average to 84.5. The comp would print 84.5; the datum is a
        // u8 everywhere it exists, so the answer is 85 — and 85 is the nominal
        // floor, which is exactly the tie where guessing changes the colour of
        // the room.
        let mut room = watching();
        seated_with(&mut room, 3, "ayanami", 84);
        seated_with(&mut room, 4, "asuka", 85);

        let average = room.cage_sync(CAGE).expect("two pilots are seated");
        assert_eq!(average.ratio, 85);
        assert_eq!(average.band, SyncBand::Nominal);
        assert_eq!(average.pilots, 2);
        assert_eq!(room.current_cage_sync(), None, "our plug is elsewhere");
    }

    #[test]
    fn a_pilot_who_ejected_stops_counting_towards_the_room() {
        // Ejecting leaves the seats and keeps the name. Somebody who walked out
        // with a dying connection must not go on dragging the room's number
        // down from outside it — and this client has no third state where a
        // pilot is in the Cage but ejected.
        let mut room = watching();
        seated_with(&mut room, 3, "ayanami", 90);
        seated_with(&mut room, 4, "asuka", 20);

        assert_eq!(room.cage_sync(CAGE).map(|sync| sync.ratio), Some(55));

        room.apply(&ServerMessage::PilotLeft {
            cage: CAGE,
            pilot: PilotId(4),
        });

        let average = room.cage_sync(CAGE).expect("ayanami is still seated");
        assert_eq!(average.ratio, 90, "somebody who left is still counted");
        assert_eq!(average.pilots, 1);
        assert_eq!(
            room.name_of(PilotId(4)),
            "asuka",
            "the name left with the seat"
        );
    }

    // ---- rooms made while this client was already connected ----

    #[test]
    fn a_cage_made_now_joins_the_list_without_a_reconnection() {
        // The whole point of announcing it. Folding this into the next
        // handshake only would mean the person who made the room has to tell
        // everybody to reconnect before they can walk into it.
        let mut room = Room::new();
        room.apply(&session());
        assert_eq!(room.cages.len(), 1);

        let changed = room.apply(&ServerMessage::CageCreated {
            cage: CageInfo {
                id: CageId(2),
                name: "CAGE-02 SALA DOS FUNDOS".into(),
                limit: 8,
                password_required: false,
                line: None,
            },
        });

        assert!(
            changed.channels,
            "nothing told the shell to redraw the list"
        );
        assert_eq!(room.cages.len(), 2);
        assert_eq!(room.cages[1].id, CageId(2));
        assert_eq!(room.cages[1].name, "CAGE-02 SALA DOS FUNDOS");
    }

    #[test]
    fn a_line_made_now_joins_the_list_without_a_reconnection() {
        let mut room = Room::new();
        room.apply(&session());

        let changed = room.apply(&ServerMessage::LineCreated {
            line: LineInfo {
                id: LineId(2),
                name: "planejamento".into(),
            },
        });

        assert!(changed.channels);
        assert_eq!(room.lines.len(), 2);
        assert_eq!(room.lines[1].name, "planejamento");
    }

    #[test]
    fn the_same_room_announced_twice_appears_once() {
        // Idempotent by identifier, like `MessageReceived`. A room listed twice
        // is a room nobody can tell a friend to join by name.
        let mut room = Room::new();
        room.apply(&session());
        let made = ServerMessage::CageCreated {
            cage: CageInfo {
                id: CageId(2),
                name: "CAGE-02".into(),
                limit: 8,
                password_required: false,
                line: None,
            },
        };

        room.apply(&made);
        let again = room.apply(&made);

        assert_eq!(room.cages.len(), 2);
        assert!(
            !again.channels,
            "a repeat told the shell to redraw a list that did not change"
        );
    }

    #[test]
    fn a_rename_keeps_the_room_in_its_place() {
        // The alternative — remove and re-append — reads, to everybody
        // watching, as the room being destroyed and a new one made, and moves
        // it under the cursor of whoever was about to click it.
        let mut room = Room::new();
        room.apply(&session());
        room.apply(&ServerMessage::CageCreated {
            cage: CageInfo {
                id: CageId(2),
                name: "CAGE-02".into(),
                limit: 8,
                password_required: false,
                line: None,
            },
        });

        let changed = room.apply(&ServerMessage::CageRenamed {
            cage: CAGE,
            name: "CAGE-01 PONTE".into(),
        });

        assert!(changed.channels);
        assert_eq!(room.cages.len(), 2, "the rename made a second room");
        assert_eq!(room.cages[0].id, CAGE);
        assert_eq!(room.cages[0].name, "CAGE-01 PONTE");
        assert_eq!(room.cages[0].limit, 20, "the rename rewrote the rest of it");
    }

    #[test]
    fn renaming_a_room_this_client_never_heard_of_changes_nothing() {
        let mut room = Room::new();
        room.apply(&session());

        let changed = room.apply(&ServerMessage::LineRenamed {
            line: LineId(404),
            name: "fantasma".into(),
        });

        assert!(!changed.channels);
        assert_eq!(room.lines.len(), 1);
        assert_eq!(room.lines[0].name, "geral");
    }

    // ---- moved by somebody else's hand ----

    #[test]
    fn being_moved_takes_the_plug_with_it() {
        // `enter_cage` is called on the way out, when this client asks. A move
        // is a Cage nobody here asked for, so without an arm of its own the
        // person stays drawn in the room they left — sending voice into it and
        // reading its roster — while everybody else sees them somewhere new.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        assert_eq!(room.current_cage, Some(CAGE));

        let changed = room.apply(&ServerMessage::MovedToCage { cage: CageId(2) });

        assert!(changed.roster, "nothing told the shell to redraw");
        assert_eq!(room.current_cage, Some(CageId(2)));
        assert_eq!(
            room.current_roster().count(),
            1,
            "the moved pilot is not in the room they were moved to"
        );
        assert_eq!(
            room.roster(CAGE)
                .map(|p| p.nickname.as_str())
                .collect::<Vec<_>>(),
            ["ayanami"],
            "a copy was left behind in the old Cage"
        );
    }

    #[test]
    fn the_session_carries_what_this_pilot_may_do() {
        // A shell asking "should this control exist" must not have to intersect
        // the role catalogue itself: "negadas vencem concedidas" is one rule and
        // it lives on the server.
        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: Vec::new(),
            lines: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::ManageCages, Permission::Speak],
        });

        assert!(room.permissions.contains(&Permission::ManageCages));
        assert!(!room.permissions.contains(&Permission::Ban));
    }
}
