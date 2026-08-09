//! Projecting a [`Room`] onto what a terminal shows.
//!
//! This is what is left of the shell once the bookkeeping moved into
//! `seele_core::state`: flattening a tree into rows, turning a server timestamp
//! into a local `12:04`, and choosing which of the six screens is showing.
//!
//! Every one of those is genuinely terminal-shaped. The desktop app will
//! flatten differently, format differently and have different screens, and it
//! reads the same [`Room`] to do it. If something in this file would be
//! identical in the app, it is in the wrong crate.

use seele_core::{AlertSeverity, DisconnectReason, Link, Room};

use crate::app::{Alert, App, ChatLine, Node, RosterEntry, Screen};

/// Rebuilds everything the interface draws from the room.
///
/// Wholesale rather than patched in place: the panels are small, and a tree
/// that drifts out of step with the roster is a bug that only shows up after an
/// hour of use.
pub fn project(room: &Room, app: &mut App) {
    project_channels(room, app);
    project_messages(room, app);
    project_bar(room, app);
    project_screen(room, app);
}

/// The Cages/Lines panel: Cages, their pilots nested under the open one, Lines.
fn project_channels(room: &Room, app: &mut App) {
    app.dogmas = if room.dogma.is_empty() {
        Vec::new()
    } else {
        vec![room.dogma.clone()]
    };

    let mut tree = Vec::new();
    for cage in &room.cages {
        let open = Some(cage.id) == room.current_cage;
        tree.push(Node::Cage {
            name: cage.name.clone(),
            open,
        });
        if !open {
            continue;
        }
        for pilot in room.roster(cage.id) {
            tree.push(Node::Pilot(RosterEntry {
                nickname: pilot.nickname.clone(),
                sync: pilot.sync_ratio,
                speaking: pilot.speaking,
                at_field: pilot.at_field,
                total_isolation: pilot.total_isolation,
            }));
        }
    }
    for line in &room.lines {
        tree.push(Node::Line {
            name: line.name.clone(),
        });
    }

    app.selected = app.selected.min(tree.len().saturating_sub(1));
    app.tree = tree;
}

fn project_messages(room: &Room, app: &mut App) {
    app.messages = room
        .messages
        .iter()
        .map(|message| ChatLine {
            at: clock(message.at_seconds),
            author: message.author_nickname.clone(),
            body: if message.edited {
                format!("{} (editada)", message.body)
            } else {
                message.body.clone()
            },
            own: message.own,
        })
        .collect();
}

fn project_bar(room: &Room, app: &mut App) {
    if let Some(telemetry) = &room.telemetry {
        app.bar.rtt_ms = telemetry.rtt_ms;
        // Jitter and loss from the server are what the *server* sees; the
        // receiver's own numbers replace these when there is audio running.
        app.bar.jitter_ms = telemetry.jitter_ms;
        app.bar.loss = telemetry.loss_fraction;
    }
}

fn project_screen(room: &Room, app: &mut App) {
    if let Some(ended) = room.ended {
        app.screen = Screen::Lost {
            reason: crate::text::disconnect(ended.reason).to_owned(),
        };
        return;
    }

    app.alert = room.notice.as_ref().map(|notice| Alert {
        text: notice
            .operator_text
            .clone()
            .unwrap_or_else(|| crate::text::alert(notice.reason).to_owned()),
        // Only the loudest severity takes the keyboard hostage.
        // `specs/08-seguranca.md` wants that for what cannot be missed; using it
        // for a mention would teach people to dismiss without reading, which is
        // how the one that matters gets dismissed too.
        blocking: matches!(notice.severity, AlertSeverity::Critical),
    });
}

/// Folds the link state in, which the room does not know about.
///
/// The internal battery is a property of the *connection*, not of the room:
/// the room's last known contents are exactly what stays on screen while it
/// runs down.
pub fn project_link(link: Link, remaining_seconds: u64, app: &mut App) {
    app.set_link(link, remaining_seconds);
}

/// Turns a server timestamp into a local wall clock.
///
/// The shell owns this. `seele-core` deals in the server's seconds and has no
/// opinion about what time it is where the pilot is sitting.
#[must_use]
pub fn clock(at_seconds: i64) -> String {
    use chrono::TimeZone;

    chrono::Local
        .timestamp_opt(at_seconds, 0)
        .single()
        // A timestamp outside the representable range is somebody's clock being
        // wrong, not a reason to lose the message it came with.
        .map_or_else(|| "--:--".to_owned(), |at| at.format("%H:%M").to_string())
}

/// Whether losing the link this way is worth reconnecting over.
#[must_use]
pub fn worth_retrying(reason: DisconnectReason) -> bool {
    crate::text::worth_retrying(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_core::{
        AlertReason, CageId, CageInfo, LineId, LineInfo, MessageId, PilotId, PilotProfile,
        PilotState, Presence, ServerMessage, SessionId, Ssrc,
    };

    const CAGE: CageId = CageId(1);
    const LINE: LineId = LineId(1);

    fn room() -> Room {
        let mut room = Room::new();
        // Via `apply` rather than `adopt`, so the self entry falls back to an
        // id — which is what happens to any shell that never says its own name.
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![
                CageInfo {
                    id: CAGE,
                    name: "CAGE-01 CENTRAL".into(),
                    limit: 20,
                    password_required: false,
                    line: Some(LINE),
                },
                CageInfo {
                    id: CageId(2),
                    name: "CAGE-02 TESTE".into(),
                    limit: 20,
                    password_required: false,
                    line: None,
                },
            ],
            lines: vec![LineInfo {
                id: LINE,
                name: "geral".into(),
            }],
            roles: Vec::new(),
        });
        room.enter_cage(CAGE);
        room.open_line(LINE);
        room
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
            at_seconds: 1_700_000_000,
            author_nickname: "piloto".into(),
            body: body.into(),
            replies_to: None,
            client_message_id: None,
        }
    }

    #[test]
    fn pilots_are_nested_under_the_open_cage_and_nowhere_else() {
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        let mut app = App::new();

        project(&room, &mut app);

        // Ourselves first, then who arrived. The pilot reading the roster has
        // to be on it — that was the M4 bug.
        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["piloto 7", "ayanami"]);

        // The closed Cage is a row, not a container.
        let cages = app
            .tree
            .iter()
            .filter(|node| matches!(node, Node::Cage { .. }))
            .count();
        assert_eq!(cages, 2);
    }

    #[test]
    fn lines_come_after_every_cage() {
        // The composition specs/05-cliente-tui.md draws: Cages with their
        // pilots, then Lines. A Line floating between two Cages reads as
        // belonging to the one above it.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        let mut app = App::new();
        project(&room, &mut app);

        let last_cage = app
            .tree
            .iter()
            .rposition(|node| matches!(node, Node::Cage { .. }))
            .expect("a cage");
        let first_line = app
            .tree
            .iter()
            .position(|node| matches!(node, Node::Line { .. }))
            .expect("a line");
        assert!(first_line > last_cage);
    }

    #[test]
    fn a_selection_past_the_end_is_pulled_back_rather_than_left_dangling() {
        // Somebody selects the last row, a Cage disappears, and the index now
        // points at nothing. Drawing that panics or silently shows the wrong
        // row depending on how it is read.
        let mut room = room();
        let mut app = App::new();
        project(&room, &mut app);
        app.selected = app.tree.len() - 1;

        room.cages.clear();
        room.lines.clear();
        project(&room, &mut app);

        assert!(app.selected < app.tree.len().max(1));
    }

    #[test]
    fn an_edited_message_says_so_instead_of_changing_under_the_reader() {
        // A line that quietly becomes different text is worse than one that
        // admits it: the reader has no way to know the conversation moved.
        let mut room = room();
        room.apply(&said(1, 7, "sync caiu"));
        room.apply(&ServerMessage::MessageEdited {
            line: LINE,
            id: MessageId(1),
            body: "sync voltou".into(),
        });

        let mut app = App::new();
        project(&room, &mut app);

        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].body.contains("sync voltou"));
        assert!(app.messages[0].body.contains("editada"));
    }

    #[test]
    fn a_message_shows_the_time_the_server_stamped_it() {
        let mut room = room();
        room.apply(&said(1, 7, "olá"));
        let mut app = App::new();
        project(&room, &mut app);

        assert_eq!(app.messages[0].at, clock(1_700_000_000));
        assert_ne!(app.messages[0].at, "--:--");
    }

    #[test]
    fn an_impossible_timestamp_costs_the_clock_and_not_the_message() {
        assert_eq!(clock(i64::MAX), "--:--");
        assert_eq!(clock(i64::MIN), "--:--");
    }

    #[test]
    fn only_a_critical_notice_takes_the_keyboard() {
        let mut room = room();
        let mut app = App::new();

        room.apply(&ServerMessage::Alert {
            severity: AlertSeverity::Info,
            reason: AlertReason::Mentioned,
            operator_text: None,
        });
        project(&room, &mut app);
        assert!(!app.alert.as_ref().expect("alert").blocking);

        room.apply(&ServerMessage::Alert {
            severity: AlertSeverity::Critical,
            reason: AlertReason::PermissionDenied,
            operator_text: None,
        });
        project(&room, &mut app);
        assert!(app.alert.as_ref().expect("alert").blocking);
    }

    #[test]
    fn the_operators_own_words_win_over_the_canned_sentence() {
        let mut room = room();
        room.apply(&ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::OperatorNotice,
            operator_text: Some("reiniciando em 5 min".into()),
        });

        let mut app = App::new();
        project(&room, &mut app);
        assert_eq!(app.alert.expect("alert").text, "reiniciando em 5 min");
    }

    #[test]
    fn a_disconnect_ends_in_the_lost_screen_with_a_sentence() {
        let mut room = room();
        room.apply(&ServerMessage::Disconnecting {
            reason: DisconnectReason::Banned,
        });

        let mut app = App::new();
        project(&room, &mut app);

        let Screen::Lost { reason } = &app.screen else {
            panic!("the session ended and the screen did not");
        };
        assert!(!reason.is_empty());
        assert!(
            !reason.contains("Banned"),
            "a variant name leaked: {reason}"
        );
    }

    #[test]
    fn pilot_state_reaches_the_roster_row() {
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

        let mut app = App::new();
        project(&room, &mut app);

        let pilot = app
            .roster()
            .find(|pilot| pilot.nickname == "ayanami")
            .expect("roster");
        assert!(pilot.at_field);
        assert!(pilot.speaking);
        assert_eq!(pilot.sync, 42);
    }

    #[test]
    fn telemetry_lands_in_the_bar() {
        let mut room = room();
        room.apply(&ServerMessage::Telemetry(seele_core::Telemetry {
            rtt_ms: 38.0,
            jitter_ms: 12.0,
            loss_fraction: 0.002,
            subsystems: Vec::new(),
        }));

        let mut app = App::new();
        project(&room, &mut app);

        assert!((app.bar.rtt_ms - 38.0).abs() < f32::EPSILON);
    }

    #[test]
    fn an_empty_room_projects_an_empty_interface_and_not_a_panic() {
        let mut app = App::new();
        project(&Room::new(), &mut app);

        assert!(app.tree.is_empty());
        assert!(app.messages.is_empty());
        assert!(app.dogmas.is_empty());
    }
}
