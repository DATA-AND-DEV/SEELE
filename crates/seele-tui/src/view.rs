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

/// The voice_rooms/Lines panel: voice_rooms, their people nested under the open one, Lines.
fn project_channels(room: &Room, app: &mut App) {
    app.servers = if room.server.is_empty() {
        Vec::new()
    } else {
        vec![room.server.clone()]
    };

    let mut tree = Vec::new();
    for voice_room in &room.voice_rooms {
        let open = Some(voice_room.id) == room.current_voice_room;
        tree.push(Node::VoiceRoom {
            name: voice_room.name.clone(),
            open,
            // The core's average, not one folded here. `seele-ffi` hands the
            // desktop shell the same value from the same method: two shells
            // averaging the same roster separately is the second implementation
            // `specs/01-arquitetura.md` says is in the wrong place.
            sync: room.voice_room_sync(voice_room.id),
        });
        if !open {
            continue;
        }
        for person in room.roster(voice_room.id) {
            tree.push(Node::Person(RosterEntry {
                nickname: person.nickname.clone(),
                sync: person.sync_ratio,
                speaking: person.speaking,
                at_field: person.at_field,
                total_isolation: person.total_isolation,
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
            body: {
                let mut body = if message.edited {
                    format!("{} (editada)", message.body)
                } else {
                    message.body.clone()
                };
                // ADR 0027. Drawn on the line and never opened: this terminal
                // has no button that opens a file and is not going to grow one.
                // The word that matters is the one that says whether the bytes
                // are still there — a message whose picture was evicted must
                // not read as a message with nothing in it.
                if let Some(anexo) = &message.attachment {
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(&anexo_line(anexo));
                }
                body
            },
            own: message.own,
        })
        .collect();
}

/// The one line a terminal draws for a file.
///
/// Name, size, and whether it is still there. No preview, because a terminal
/// has none to give, and no way to open it, because ADR 0027 gives no client of
/// the SEELE one.
fn anexo_line(anexo: &seele_core::AttachmentInfo) -> String {
    let estado = if anexo.state == seele_core::AttachmentState::Expired {
        " — ESTE ARQUIVO EXPIROU"
    } else {
        ""
    };
    format!(
        "[ARQUIVO] {} · {}{estado}",
        anexo.file_name,
        tamanho(anexo.byte_size)
    )
}

/// A byte count the way somebody reads one.
fn tamanho(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= MIB {
        format!("{:.1} MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.0} KB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
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
/// opinion about what time it is where the person is sitting.
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
        AlertReason, VoiceRoomId, VoiceRoomInfo, LineId, LineInfo, MessageId, PersonId, PersonProfile,
        PersonState, Presence, ServerMessage, SessionId, Ssrc,
    };

    const VOICE_ROOM: VoiceRoomId = VoiceRoomId(1);
    const LINE: LineId = LineId(1);

    fn room() -> Room {
        let mut room = Room::new();
        // Via `apply` rather than `adopt`, so the self entry falls back to an
        // id — which is what happens to any shell that never says its own name.
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Terceira Tóquio".into(),
            voice_rooms: vec![
                VoiceRoomInfo {
                    id: VOICE_ROOM,
                    name: "VOICE_ROOM-01 CENTRAL".into(),
                    limit: 20,
                    password_required: false,
                    line: Some(LINE),
                },
                VoiceRoomInfo {
                    id: VoiceRoomId(2),
                    name: "VOICE_ROOM-02 TESTE".into(),
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
            permissions: Vec::new(),
        });
        room.enter_voice_room(VOICE_ROOM);
        room.open_line(LINE);
        room
    }

    fn joined(id: u64, nickname: &str) -> ServerMessage {
        ServerMessage::PersonJoined {
            voice_room: VOICE_ROOM,
            profile: PersonProfile {
                id: PersonId(id),
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
            author: PersonId(author),
            at_seconds: 1_700_000_000,
            author_nickname: "pessoa".into(),
            body: body.into(),
            replies_to: None,
            client_message_id: None,
            attachment: None,
        }
    }

    fn com_anexo(id: u64, body: &str, expirado: bool) -> ServerMessage {
        let ServerMessage::MessageReceived {
            line,
            author,
            at_seconds,
            author_nickname,
            replies_to,
            client_message_id,
            ..
        } = said(id, 1, body)
        else {
            unreachable!("said builds a MessageReceived")
        };
        ServerMessage::MessageReceived {
            line,
            id: MessageId(id),
            author,
            at_seconds,
            author_nickname,
            body: body.into(),
            replies_to,
            client_message_id,
            attachment: Some(seele_core::AttachmentInfo {
                id: seele_core::AttachmentId(7),
                file_name: "harmonicos.png".into(),
                declared_type: "image/png".into(),
                byte_size: 2 * 1024 * 1024,
                state: if expirado {
                    seele_core::AttachmentState::Expired
                } else {
                    seele_core::AttachmentState::Available
                },
            }),
        }
    }

    #[test]
    fn a_message_with_a_file_says_the_name_and_the_size() {
        // ADR 0027. The terminal draws it and never opens it: there is no
        // button that opens a file in any client of the SEELE.
        let mut room = room();
        room.apply(&com_anexo(1, "olha isto", false));
        let mut app = App::new();
        project(&room, &mut app);

        let linha = &app.messages[0].body;
        assert!(linha.contains("olha isto"), "o texto sumiu: {linha}");
        assert!(linha.contains("harmonicos.png"), "sem o nome: {linha}");
        assert!(linha.contains("2.0 MB"), "sem o tamanho: {linha}");
        assert!(
            !linha.contains("EXPIROU"),
            "um arquivo que está lá foi anunciado como ido: {linha}"
        );
    }

    #[test]
    fn a_message_whose_file_expired_says_so_instead_of_drawing_nothing() {
        // A mensagem com o arquivo ido não pode ficar igual a uma mensagem sem
        // arquivo nenhum: é para isso que o servidor guarda a linha depois de
        // apagar os bytes, e é aqui que essa decisão vira ou não vira uma
        // frase na tela.
        let mut room = room();
        room.apply(&com_anexo(1, "recibo", true));
        let mut app = App::new();
        project(&room, &mut app);

        let linha = &app.messages[0].body;
        assert!(linha.contains("EXPIROU"), "não disse que expirou: {linha}");
        assert!(
            linha.contains("harmonicos.png") && linha.contains("2.0 MB"),
            "o nome e o tamanho foram embora com os bytes: {linha}"
        );
    }

    #[test]
    fn a_message_with_no_file_gains_no_line() {
        let mut room = room();
        room.apply(&said(1, 1, "só texto"));
        let mut app = App::new();
        project(&room, &mut app);
        assert_eq!(app.messages[0].body, "só texto");
    }

    #[test]
    fn persons_are_nested_under_the_open_voice_room_and_nowhere_else() {
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        let mut app = App::new();

        project(&room, &mut app);

        // Ourselves first, then who arrived. The person reading the roster has
        // to be on it — that was the M4 bug.
        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["pessoa 7", "ayanami"]);

        // The closed voice room is a row, not a container.
        let voice_rooms = app
            .tree
            .iter()
            .filter(|node| matches!(node, Node::VoiceRoom { .. }))
            .count();
        assert_eq!(voice_rooms, 2);
    }

    #[test]
    fn the_voice_room_row_carries_the_core_s_average_and_not_one_of_its_own() {
        // The terminal must not average anything: the desktop shell reads the
        // same value from `Room::voice_room_sync` through `seele-ffi`, and a mean
        // computed twice is a mean that will disagree once.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(3),
            at_field: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            sync_ratio: 90,
        }));
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(7),
            at_field: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            sync_ratio: 60,
        }));

        let mut app = App::new();
        project(&room, &mut app);

        let occupied = app
            .tree
            .iter()
            .find_map(|node| match node {
                Node::VoiceRoom { name, sync, .. } if name.contains("VOICE_ROOM-01") => Some(*sync),
                _ => None,
            })
            .expect("the open voice room");
        assert_eq!(occupied, room.voice_room_sync(VOICE_ROOM));
        assert_eq!(occupied.map(|sync| sync.ratio), Some(75));

        // And the voice room nobody is in has nothing, rather than a zero.
        let empty = app
            .tree
            .iter()
            .find_map(|node| match node {
                Node::VoiceRoom { name, sync, .. } if name.contains("VOICE_ROOM-02") => Some(*sync),
                _ => None,
            })
            .expect("the closed voice room");
        assert_eq!(empty, None);
    }

    #[test]
    fn lines_come_after_every_voice_room() {
        // The composition specs/05-cliente-tui.md draws: voice_rooms with their
        // people, then Lines. A Line floating between two voice_rooms reads as
        // belonging to the one above it.
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        let mut app = App::new();
        project(&room, &mut app);

        let last_voice_room = app
            .tree
            .iter()
            .rposition(|node| matches!(node, Node::VoiceRoom { .. }))
            .expect("a voice room");
        let first_line = app
            .tree
            .iter()
            .position(|node| matches!(node, Node::Line { .. }))
            .expect("a line");
        assert!(first_line > last_voice_room);
    }

    #[test]
    fn a_selection_past_the_end_is_pulled_back_rather_than_left_dangling() {
        // Somebody selects the last row, a voice room disappears, and the index now
        // points at nothing. Drawing that panics or silently shows the wrong
        // row depending on how it is read.
        let mut room = room();
        let mut app = App::new();
        project(&room, &mut app);
        app.selected = app.tree.len() - 1;

        room.voice_rooms.clear();
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
    fn person_state_reaches_the_roster_row() {
        let mut room = room();
        room.apply(&joined(3, "ayanami"));
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(3),
            at_field: true,
            total_isolation: false,
            speaking: true,
            presence: Presence::Available,
            sync_ratio: 42,
        }));

        let mut app = App::new();
        project(&room, &mut app);

        let person = app
            .roster()
            .find(|person| person.nickname == "ayanami")
            .expect("roster");
        assert!(person.at_field);
        assert!(person.speaking);
        assert_eq!(person.sync, 42);
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
        assert!(app.servers.is_empty());
    }
}
