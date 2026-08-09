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

use seele_proto::control::{CageInfo, LineInfo, PilotState};
use seele_proto::ids::{CageId, LineId, MessageId, PilotId, Ssrc};
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
                ..
            } => {
                self.me = Some(*pilot);
                self.ssrc = Some(*ssrc);
                self.dogma.clone_from(dogma);
                self.cages.clone_from(cages);
                self.lines.clone_from(lines);
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
}
