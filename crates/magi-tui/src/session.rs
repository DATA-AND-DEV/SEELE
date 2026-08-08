//! Turning what the server said into what the screen shows.
//!
//! `specs/01-arquitetura.md` has the core emit events and the shell turn them
//! into pixels. This is that turn, and it is the whole of it: every
//! [`ServerMessage`] the client acts on passes through [`Session::apply`].
//!
//! It holds no logic of its own. Deciding *whether* a pilot may speak, *what* a
//! Sync Ratio means, or *when* to reconnect all happen in `magi-core`. What
//! happens here is bookkeeping — which name goes with which id, which pilots
//! are in which Cage — because that is presentation state and the core has no
//! business remembering it for three different interfaces.

use std::collections::HashMap;

use magi_core::{AlertSeverity, CageId, CageInfo, LineId, LineInfo, PilotId, ServerMessage, Ssrc};

use crate::app::{Alert, App, ChatLine, Node, RosterEntry, Screen};

/// Everything the client knows about the Dogma it is attached to.
#[derive(Debug, Default)]
pub struct Session {
    /// Which pilot this connection is.
    pub me: Option<PilotId>,
    /// The media source the server assigned — gap G1.
    pub ssrc: Option<Ssrc>,
    /// Voice channels visible to this pilot.
    pub cages: Vec<CageInfo>,
    /// Text channels visible to this pilot.
    pub lines: Vec<LineInfo>,
    /// The Cage the plug is inserted in.
    pub current_cage: Option<CageId>,
    /// The Line being read.
    pub current_line: Option<LineId>,
    /// Names by id, so a message can be attributed without asking again.
    names: HashMap<PilotId, String>,
    /// Media sources by pilot, so per-user volume can be addressed by name.
    ssrcs: HashMap<PilotId, Ssrc>,
    /// Who is in which Cage.
    members: HashMap<CageId, Vec<PilotId>>,
    /// Last announced state per pilot.
    states: HashMap<PilotId, magi_core::PilotState>,
}

impl Session {
    /// A session that has not heard anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds from what the handshake already established.
    ///
    /// The core consumes the `Session` message while establishing PADRÃO: AZUL,
    /// so the interface never sees it go by. Without this, a client that
    /// connected perfectly would show an empty Dogma — which is how a wiring
    /// mistake looks exactly like a server with no Cages.
    pub fn adopt(&mut self, info: &magi_core::SessionInfo, nickname: &str, app: &mut App) {
        self.me = Some(info.pilot);
        self.ssrc = Some(info.ssrc);
        self.cages = info.cages.clone();
        self.lines = info.lines.clone();
        self.ssrcs.insert(info.pilot, info.ssrc);
        // Our own name comes from what we asked to be called: the server
        // announces arrivals to everybody *else*, so nothing on the wire ever
        // tells this client who it is. Without this the pilot is the one person
        // missing from their own roster.
        self.names.insert(info.pilot, nickname.to_owned());
        app.dogmas = vec![info.dogma.clone()];
        self.rebuild(app);
    }

    /// Records that this pilot's plug is now in a Cage, and seats them in it.
    pub fn enter_cage(&mut self, cage: CageId, app: &mut App) {
        if let Some(me) = self.me {
            if let Some(previous) = self.current_cage {
                if let Some(seats) = self.members.get_mut(&previous) {
                    seats.retain(|seated| *seated != me);
                }
            }
            let seats = self.members.entry(cage).or_default();
            if !seats.contains(&me) {
                seats.push(me);
            }
        }
        self.current_cage = Some(cage);
        self.rebuild(app);
    }

    /// The media source belonging to a nickname, for `:volume`.
    #[must_use]
    pub fn ssrc_of(&self, nickname: &str) -> Option<Ssrc> {
        let pilot = self
            .names
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(nickname))
            .map(|(pilot, _)| *pilot)?;
        self.ssrcs.get(&pilot).copied()
    }

    /// The Cage matching a name or a number, for `:cage`.
    #[must_use]
    pub fn find_cage(&self, which: &str) -> Option<CageId> {
        if let Ok(number) = which.parse::<u32>() {
            if let Some(cage) = self.cages.iter().find(|cage| cage.id.0 == number) {
                return Some(cage.id);
            }
        }
        self.cages
            .iter()
            .find(|cage| cage.name.to_lowercase().contains(&which.to_lowercase()))
            .map(|cage| cage.id)
    }

    /// The Line matching a name or a number, for `:linha`.
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

    /// Folds one server message into the interface state.
    #[allow(
        clippy::too_many_lines,
        reason = "one match over the protocol; splitting it hides which messages are handled"
    )]
    pub fn apply(&mut self, message: &ServerMessage, app: &mut App, now: &str) {
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
                self.cages = cages.clone();
                self.lines = lines.clone();
                self.ssrcs.insert(*pilot, *ssrc);
                app.dogmas = vec![dogma.clone()];
                app.screen = Screen::PatternBlue;
                self.rebuild(app);
            }

            ServerMessage::PilotJoined {
                cage,
                profile,
                ssrc,
            } => {
                self.names.insert(profile.id, profile.nickname.clone());
                self.ssrcs.insert(profile.id, *ssrc);
                let seats = self.members.entry(*cage).or_default();
                if !seats.contains(&profile.id) {
                    seats.push(profile.id);
                }
                self.rebuild(app);
            }

            ServerMessage::PilotLeft { cage, pilot } => {
                if let Some(seats) = self.members.get_mut(cage) {
                    seats.retain(|seated| seated != pilot);
                }
                self.states.remove(pilot);
                self.rebuild(app);
            }

            ServerMessage::PilotState(state) => {
                self.states.insert(state.pilot, *state);
                self.rebuild(app);
            }

            ServerMessage::MessageReceived {
                line, author, body, ..
            } => {
                // History for a Line nobody is reading is still kept: switching
                // to it should not show an empty room somebody was just talking
                // in. Filtering happens at draw time, not here.
                if Some(*line) == self.current_line || self.current_line.is_none() {
                    app.messages.push(ChatLine {
                        at: now.to_owned(),
                        author: self.name_of(*author),
                        body: body.clone(),
                        own: Some(*author) == self.me,
                    });
                }
            }

            ServerMessage::MessageEdited { id, body, .. } => {
                // No local id map yet, so an edit appends rather than rewrites.
                // Registered as debt rather than dropped: a message that changes
                // and does not change on screen is worse than a duplicated one.
                app.messages.push(ChatLine {
                    at: now.to_owned(),
                    author: format!("editado #{}", id.0),
                    body: body.clone(),
                    own: false,
                });
            }

            ServerMessage::MessageRemoved { .. } => {}

            ServerMessage::Telemetry(telemetry) => {
                app.bar.rtt_ms = telemetry.rtt_ms;
                app.bar.jitter_ms = telemetry.jitter_ms;
                app.bar.loss = telemetry.loss_fraction;
            }

            ServerMessage::Alert {
                severity,
                reason,
                operator_text,
            } => {
                let text = operator_text
                    .clone()
                    .unwrap_or_else(|| crate::text::alert(*reason).to_owned());
                app.alert = Some(Alert {
                    text,
                    // Only the loudest severity takes the keyboard hostage.
                    // specs/08-seguranca.md wants that for things that cannot be
                    // missed; using it for a mention would train people to
                    // dismiss without reading, which is how the important one
                    // gets dismissed too.
                    blocking: matches!(severity, AlertSeverity::Critical),
                });
            }

            ServerMessage::Disconnecting { reason } => {
                app.screen = Screen::Lost {
                    reason: crate::text::disconnect(*reason).to_owned(),
                };
            }

            // The handshake is the core's business and is over before the
            // interface sees anything; Pong is consumed by the core to measure
            // the round trip.
            ServerMessage::Challenge { .. } | ServerMessage::Pong { .. } => {}
        }
    }

    fn name_of(&self, pilot: PilotId) -> String {
        self.names
            .get(&pilot)
            .cloned()
            .unwrap_or_else(|| format!("piloto {}", pilot.0))
    }

    /// Rebuilds the Cages/Lines panel from what is known.
    ///
    /// Rebuilt wholesale rather than patched in place, because the panel is
    /// small and a tree that drifts out of step with the roster is a bug that
    /// only shows up after an hour of use.
    pub fn rebuild(&self, app: &mut App) {
        let mut tree = Vec::new();

        for cage in &self.cages {
            let open = Some(cage.id) == self.current_cage;
            tree.push(Node::Cage {
                name: cage.name.clone(),
                open,
            });
            if !open {
                continue;
            }
            for pilot in self.members.get(&cage.id).into_iter().flatten() {
                let state = self.states.get(pilot);
                tree.push(Node::Pilot(RosterEntry {
                    nickname: self.name_of(*pilot),
                    sync: state.map_or(0, |state| state.sync_ratio),
                    speaking: state.is_some_and(|state| state.speaking),
                    at_field: state.is_some_and(|state| state.at_field),
                    total_isolation: state.is_some_and(|state| state.total_isolation),
                }));
            }
        }

        for line in &self.lines {
            tree.push(Node::Line {
                name: line.name.clone(),
            });
        }

        app.selected = app.selected.min(tree.len().saturating_sub(1));
        app.tree = tree;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magi_core::{PilotProfile, PilotState, Presence, SessionId};

    fn session_message() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![CageInfo {
                id: CageId(1),
                name: "CAGE-01 CENTRAL".into(),
                limit: 20,
                password_required: false,
                line: Some(LineId(1)),
            }],
            lines: vec![LineInfo {
                id: LineId(1),
                name: "#geral".into(),
            }],
            roles: Vec::new(),
        }
    }

    fn joined(id: u64, nickname: &str) -> ServerMessage {
        ServerMessage::PilotJoined {
            cage: CageId(1),
            profile: PilotProfile {
                id: PilotId(id),
                nickname: nickname.into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
        }
    }

    #[test]
    fn the_session_message_populates_the_dogma_and_the_channels() {
        let mut app = App::new();
        let mut session = Session::new();

        session.apply(&session_message(), &mut app, "12:00");

        assert_eq!(app.dogmas, ["Terceira Tóquio"]);
        assert_eq!(app.screen, Screen::PatternBlue);
        assert!(app
            .tree
            .iter()
            .any(|node| matches!(node, Node::Cage { name, .. } if name == "CAGE-01 CENTRAL")));
        assert!(app
            .tree
            .iter()
            .any(|node| matches!(node, Node::Line { name } if name == "#geral")));
    }

    #[test]
    fn pilots_only_show_inside_the_cage_the_plug_is_in() {
        // A roster listing every pilot on the server under a Cage nobody is in
        // is not a roster, it is a directory.
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");
        session.apply(&joined(3, "ayanami"), &mut app, "12:00");

        assert_eq!(app.roster().count(), 0, "pilots showed in a closed Cage");

        session.current_cage = Some(CageId(1));
        session.rebuild(&mut app);

        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["ayanami"]);
    }

    #[test]
    fn the_pilot_sees_themself_in_their_own_roster() {
        // The server announces arrivals to everybody else, so nothing on the
        // wire ever names this client to itself. Without seating ourselves, the
        // one person missing from the roster is the person reading it.
        let mut app = App::new();
        let mut session = Session::new();

        let info = magi_core::SessionInfo {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![CageInfo {
                id: CageId(1),
                name: "CAGE-01 CENTRAL".into(),
                limit: 20,
                password_required: false,
                line: Some(LineId(1)),
            }],
            lines: Vec::new(),
        };
        session.adopt(&info, "ayanami", &mut app);
        session.enter_cage(CageId(1), &mut app);

        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["ayanami"]);
    }

    #[test]
    fn moving_between_cages_does_not_leave_a_copy_behind() {
        let mut app = App::new();
        let mut session = Session::new();
        let info = magi_core::SessionInfo {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![
                CageInfo {
                    id: CageId(1),
                    name: "CAGE-01".into(),
                    limit: 20,
                    password_required: false,
                    line: None,
                },
                CageInfo {
                    id: CageId(2),
                    name: "CAGE-02".into(),
                    limit: 20,
                    password_required: false,
                    line: None,
                },
            ],
            lines: Vec::new(),
        };
        session.adopt(&info, "ayanami", &mut app);
        session.enter_cage(CageId(1), &mut app);
        session.enter_cage(CageId(2), &mut app);

        assert_eq!(app.roster().count(), 1, "the pilot is in two Cages at once");
    }

    #[test]
    fn a_pilot_leaving_leaves_the_roster() {
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");
        session.current_cage = Some(CageId(1));
        session.apply(&joined(3, "ayanami"), &mut app, "12:00");
        session.apply(&joined(4, "shinji"), &mut app, "12:00");

        session.apply(
            &ServerMessage::PilotLeft {
                cage: CageId(1),
                pilot: PilotId(3),
            },
            &mut app,
            "12:00",
        );

        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["shinji"]);
    }

    #[test]
    fn state_updates_reach_the_roster() {
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");
        session.current_cage = Some(CageId(1));
        session.apply(&joined(3, "ayanami"), &mut app, "12:00");

        session.apply(
            &ServerMessage::PilotState(PilotState {
                pilot: PilotId(3),
                at_field: true,
                total_isolation: false,
                speaking: false,
                presence: Presence::Available,
                sync_ratio: 42,
            }),
            &mut app,
            "12:00",
        );

        let pilot = app.roster().next().expect("roster");
        assert!(pilot.at_field);
        assert_eq!(pilot.sync, 42);
    }

    #[test]
    fn a_message_is_attributed_by_name_and_marked_when_it_is_ours() {
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");
        session.apply(&joined(3, "ayanami"), &mut app, "12:00");

        session.apply(
            &ServerMessage::MessageReceived {
                line: LineId(1),
                id: magi_core::MessageId(1),
                author: PilotId(3),
                body: "verificando harmônicos".into(),
                replies_to: None,
                client_message_id: None,
            },
            &mut app,
            "12:01",
        );
        session.apply(
            &ServerMessage::MessageReceived {
                line: LineId(1),
                id: magi_core::MessageId(2),
                author: PilotId(7),
                body: "recebido".into(),
                replies_to: None,
                client_message_id: None,
            },
            &mut app,
            "12:02",
        );

        assert_eq!(app.messages[0].author, "ayanami");
        assert!(!app.messages[0].own);
        assert!(app.messages[1].own, "our own message was not marked");
    }

    #[test]
    fn a_message_from_a_stranger_is_still_shown() {
        // Joining a Line mid-conversation means the first thing said may come
        // from somebody whose join was never seen. Dropping it would be worse
        // than showing an id.
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");

        session.apply(
            &ServerMessage::MessageReceived {
                line: LineId(1),
                id: magi_core::MessageId(1),
                author: PilotId(99),
                body: "olá".into(),
                replies_to: None,
                client_message_id: None,
            },
            &mut app,
            "12:01",
        );

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].author, "piloto 99");
    }

    #[test]
    fn only_a_critical_alert_takes_the_keyboard() {
        // specs/08-seguranca.md wants the unmissable warning unmissable. Using
        // the same treatment for a mention teaches people to dismiss without
        // reading, which is how the unmissable one gets missed.
        let mut app = App::new();
        let mut session = Session::new();

        session.apply(
            &ServerMessage::Alert {
                severity: AlertSeverity::Info,
                reason: magi_core::AlertReason::Mentioned,
                operator_text: None,
            },
            &mut app,
            "12:00",
        );
        assert!(!app.alert.as_ref().expect("alert").blocking);

        session.apply(
            &ServerMessage::Alert {
                severity: AlertSeverity::Critical,
                reason: magi_core::AlertReason::SubsystemChanged,
                operator_text: None,
            },
            &mut app,
            "12:00",
        );
        assert!(app.alert.as_ref().expect("alert").blocking);
    }

    #[test]
    fn telemetry_lands_in_the_bar() {
        let mut app = App::new();
        let mut session = Session::new();

        session.apply(
            &ServerMessage::Telemetry(magi_core::Telemetry {
                rtt_ms: 38.0,
                jitter_ms: 12.0,
                loss_fraction: 0.002,
                subsystems: Vec::new(),
            }),
            &mut app,
            "12:00",
        );

        assert!((app.bar.rtt_ms - 38.0).abs() < f32::EPSILON);
        assert!((app.bar.loss - 0.002).abs() < f32::EPSILON);
    }

    #[test]
    fn a_cage_can_be_found_by_number_or_by_part_of_its_name() {
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");

        assert_eq!(session.find_cage("1"), Some(CageId(1)));
        assert_eq!(session.find_cage("central"), Some(CageId(1)));
        assert_eq!(session.find_cage("CENTRAL"), Some(CageId(1)));
        assert_eq!(session.find_cage("geofront"), None);
    }

    #[test]
    fn a_line_can_be_found_with_or_without_its_hash() {
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");

        assert_eq!(session.find_line("#geral"), Some(LineId(1)));
        assert_eq!(session.find_line("geral"), Some(LineId(1)));
    }

    #[test]
    fn a_pilots_media_source_can_be_looked_up_by_nickname() {
        // What `:volume ayanami 40` needs, and the reason the shell is allowed
        // to know an ssrc at all: it is an address, not protocol logic.
        let mut app = App::new();
        let mut session = Session::new();
        session.apply(&session_message(), &mut app, "12:00");
        session.apply(&joined(3, "ayanami"), &mut app, "12:00");

        assert_eq!(session.ssrc_of("ayanami"), Some(Ssrc(30)));
        assert_eq!(session.ssrc_of("AYANAMI"), Some(Ssrc(30)));
        assert_eq!(session.ssrc_of("ninguém"), None);
    }

    #[test]
    fn a_disconnect_ends_in_the_lost_screen_with_a_reason() {
        let mut app = App::new();
        let mut session = Session::new();

        session.apply(
            &ServerMessage::Disconnecting {
                reason: magi_core::DisconnectReason::Banned,
            },
            &mut app,
            "12:00",
        );

        assert!(matches!(app.screen, Screen::Lost { ref reason } if !reason.is_empty()));
    }
}
