//! The M4 acceptance criteria from `specs/09-roadmap.md` and
//! `specs/05-cliente-tui.md`, executed against a real Dogma.
//!
//! > **Aceite:** alguém de fora do projeto consegue conectar e conversar só com
//! > `?`. Funciona por SSH em terminal de 16 cores sem perder informação. Do
//! > lançamento até pronto para falar em menos de 1,5 s.
//!
//! The rendering rules — cell widths, truncation, degradation — are pinned in
//! `seele-tui`'s own tests, where they run without a network. What is here is
//! the part those cannot check: that the interface, wired to an actual server,
//! shows what actually happened. A layout that is correct about invented data
//! is not evidence of anything.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use seele_core::{Client, MemoryPinStore, Room};
use seele_proto::ids::{VoiceRoomId, ClientMessageId, LineId};
use seele_proto::ServerMessage;
use seele_server::persistence::Location;
use seele_server::{DogmaConfig, Server};
use seele_tui::app::{App, Key, Mode, Screen};
use seele_tui::theme::{Palette, Theme};
use seele_tui::{ui, view};

const VOICE_ROOM: VoiceRoomId = VoiceRoomId(1);
const LINE: LineId = LineId(1);
const WAIT: Duration = Duration::from_secs(5);

/// The smallest terminal the spec supports, which is the one that has to work.
const SIZE: (u16, u16) = (80, 24);

async fn start() -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..DogmaConfig::default()
    };
    let server = Arc::new(Server::bind(config).await?);
    let address = server.local_addr()?;
    let accepting = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = accepting.run().await;
    });
    Ok((address, server))
}

async fn connect(address: SocketAddr, nickname: &str, seed: u8) -> Result<Client> {
    Client::connect(
        address,
        "localhost",
        &address.to_string(),
        nickname,
        &SigningKey::from_bytes(&[seed; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await
    .map_err(Into::into)
}

/// Draws the interface and returns the screen as text.
///
/// Continuation cells after a wide character are skipped, so the rows measure
/// the width the terminal shows rather than double-counting every kanji.
fn draw(app: &App, palette: Palette) -> String {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).unwrap();
    terminal
        .draw(|frame| ui::render(frame, app, Theme::with_palette(palette)))
        .unwrap();
    let buffer = terminal.backend().buffer();

    (0..SIZE.1)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < SIZE.0 {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += u16::try_from(ui::width(symbol).max(1)).unwrap_or(1);
            }
            row
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Pumps events into the interface until it shows what the test is waiting for,
/// or the wait runs out.
async fn pump<F>(client: &mut Client, app: &mut App, room: &mut Room, mut done: F) -> bool
where
    F: FnMut(&App) -> bool,
{
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(app) {
            return true;
        }
        let Ok(Ok(message)) =
            tokio::time::timeout(Duration::from_millis(500), client.next_event()).await
        else {
            continue;
        };
        if room.apply(&message).any() {
            view::project(room, app);
        }
    }
    done(app)
}

/// Brings the interface up against a live connection, the way `plug` does.
fn attach(client: &Client, nickname: &str, voice_room: VoiceRoomId, line: Option<LineId>) -> (App, Room) {
    let mut app = App::new();
    app.screen = Screen::PatternBlue;
    let mut room = Room::new();
    room.adopt(client.session(), nickname);
    room.enter_voice_room(voice_room);
    if let Some(line) = line {
        room.open_line(line);
    }
    view::project(&room, &mut app);
    (app, room)
}

/// Somebody outside the project connects, sees the room, and reads what is said.
#[tokio::test]
async fn an_outsider_connects_and_the_screen_shows_the_conversation() -> Result<()> {
    let (address, server) = start().await?;

    let mut watcher = connect(address, "ayanami", 1).await?;
    watcher.insert_plug(VOICE_ROOM).await?;
    watcher.join_line(LINE).await?;

    let (mut app, mut room) = attach(&watcher, "ayanami", VOICE_ROOM, Some(LINE));

    let mut talker = connect(address, "shinji", 2).await?;
    talker.insert_plug(VOICE_ROOM).await?;
    talker.join_line(LINE).await?;
    talker
        .send_message(LINE, "sync caiu aqui", ClientMessageId(1))
        .await?;

    let arrived = pump(&mut watcher, &mut app, &mut room, |app| {
        app.messages.iter().any(|m| m.body == "sync caiu aqui")
    })
    .await;
    assert!(arrived, "the message never reached the interface");

    let screen = draw(&app, Palette::True);
    assert!(screen.contains("sync caiu aqui"), "{screen}");
    assert!(screen.contains("shinji"), "unattributed:\n{screen}");
    // Truncated, not missing: the Dogma panel is 18 cells and the name is
    // longer. Asserting the full string here would be asserting that truncation
    // does not work.
    assert!(screen.contains("Terceira"), "no Dogma:\n{screen}");

    server.shutdown();
    Ok(())
}

/// The roster fills in from real joins, not from invented state.
#[tokio::test]
async fn the_roster_shows_who_actually_entered_the_voice_room() -> Result<()> {
    let (address, server) = start().await?;

    let mut watcher = connect(address, "ayanami", 3).await?;
    watcher.insert_plug(VOICE_ROOM).await?;

    let (mut app, mut room) = attach(&watcher, "ayanami", VOICE_ROOM, None);

    let mut talker = connect(address, "asuka", 4).await?;
    talker.insert_plug(VOICE_ROOM).await?;

    let seen = pump(&mut watcher, &mut app, &mut room, |app| {
        app.roster().any(|person| person.nickname == "asuka")
    })
    .await;
    assert!(seen, "a person entered the voice room and never appeared");

    // The Sync Ratio is shown as a number beside a mark, in every palette —
    // specs/05-cliente-tui.md forbids carrying it by colour alone.
    let mono = draw(&app, Palette::Mono);
    assert!(mono.contains("asuka"), "{mono}");
    assert!(
        mono.contains('█') || mono.contains('▓') || mono.contains('▒') || mono.contains('░'),
        "no band mark survived without colour:\n{mono}"
    );

    server.shutdown();
    Ok(())
}

/// Walking into an occupied voice room shows the people already in it.
///
/// Gap G15. `specs/02-protocolo.md` announces arrivals going forward and says
/// nothing about who is already seated, so the second person to arrive saw an
/// empty room until somebody else moved. Found by starting two clients in
/// sequence rather than at once — which is what everybody actually does.
#[tokio::test]
async fn the_second_person_to_arrive_sees_the_first_one() -> Result<()> {
    let (address, server) = start().await?;

    // The first person sits down and stops being interesting.
    let mut early = connect(address, "shinji", 10).await?;
    early.insert_plug(VOICE_ROOM).await?;

    // The second arrives afterwards, and has never seen an announcement.
    let mut late = connect(address, "asuka", 11).await?;
    late.insert_plug(VOICE_ROOM).await?;

    let (mut app, mut room) = attach(&late, "asuka", VOICE_ROOM, None);
    let seen = pump(&mut late, &mut app, &mut room, |app| {
        app.roster().any(|person| person.nickname == "shinji")
    })
    .await;

    assert!(seen, "walked into an occupied voice room and saw an empty room");

    let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
    assert!(
        names.contains(&"asuka"),
        "we are not on our own roster: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| **name == "shinji").count(),
        1,
        "the same person is seated twice: {names:?}"
    );

    server.shutdown();
    Ok(())
}

/// Sixteen colours is what an SSH session gets, and it must lose nothing.
#[tokio::test]
async fn sixteen_colours_over_ssh_lose_no_information() -> Result<()> {
    let (address, server) = start().await?;

    let mut watcher = connect(address, "ayanami", 5).await?;
    watcher.insert_plug(VOICE_ROOM).await?;
    watcher.join_line(LINE).await?;

    let (mut app, mut room) = attach(&watcher, "ayanami", VOICE_ROOM, Some(LINE));

    let mut talker = connect(address, "shinji", 6).await?;
    talker.insert_plug(VOICE_ROOM).await?;
    talker.join_line(LINE).await?;
    talker
        .send_message(LINE, "verificando harmônicos", ClientMessageId(1))
        .await?;
    pump(&mut watcher, &mut app, &mut room, |app| {
        !app.messages.is_empty()
    })
    .await;

    let rich = draw(&app, Palette::True);
    let ssh = draw(&app, Palette::Ansi16);
    assert_eq!(
        rich, ssh,
        "the 16-colour screen does not carry the same characters"
    );

    // And it still fits, which is the other half of "works over SSH".
    for row in ssh.lines() {
        assert!(ui::width(row) <= SIZE.0 as usize, "row overflows: {row:?}");
    }

    server.shutdown();
    Ok(())
}

/// From launch to ready to speak in under 1.5 s.
///
/// Measured from before the connection to after the first frame is drawn, which
/// is when there is something to read and a key can be pressed. Process spawn
/// and dynamic linking are outside this measurement and are what the remaining
/// margin is for — noted rather than quietly claimed.
#[tokio::test]
async fn boot_to_ready_is_under_a_second_and_a_half() -> Result<()> {
    let (address, server) = start().await?;

    let started = Instant::now();
    let mut client = connect(address, "ayanami", 7).await?;
    client.insert_plug(VOICE_ROOM).await?;
    client.join_line(LINE).await?;

    let (app, _room) = attach(&client, "ayanami", VOICE_ROOM, Some(LINE));
    let screen = draw(&app, Palette::True);
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(1500),
        "boot to ready took {elapsed:?}"
    );
    assert!(!matches!(app.screen, Screen::Boot), "still booting");
    assert!(screen.contains("MENSAGENS"), "{screen}");

    server.shutdown();
    Ok(())
}

/// The whole path an outsider takes with nothing but `?`.
///
/// `specs/09-roadmap.md` accepts M4 on somebody outside the project connecting
/// and conversing knowing only that key. So the test presses only keys that
/// help screen names, and checks that a message really reaches another person.
#[tokio::test]
async fn a_newcomer_can_hold_a_conversation_with_only_the_help_screen() -> Result<()> {
    let (address, server) = start().await?;

    let mut newcomer = connect(address, "ayanami", 8).await?;
    newcomer.insert_plug(VOICE_ROOM).await?;
    newcomer.join_line(LINE).await?;

    let mut listener = connect(address, "shinji", 9).await?;
    listener.insert_plug(VOICE_ROOM).await?;
    listener.join_line(LINE).await?;

    let mut app = App::new();
    app.screen = Screen::PatternBlue;

    // `?` — and everything the newcomer needs is named on it.
    app.on_key(Key::Char('?'));
    let help = draw(&app, Palette::True);
    for promise in ["i", "escrever mensagem", ":q"] {
        assert!(
            help.contains(promise),
            "`{promise}` is not on the help:\n{help}"
        );
    }
    app.on_key(Key::Esc);
    assert!(!app.help);

    // `i`, type, Enter — exactly what the help said.
    app.on_key(Key::Char('i'));
    assert_eq!(app.mode, Mode::Insert);
    for character in "olá".chars() {
        app.on_key(Key::Char(character));
    }
    let action = app.on_key(Key::Enter);

    let seele_tui::app::Action::Send(body) = action.expect("nothing to send") else {
        panic!("`i` then Enter did not produce a message");
    };
    newcomer
        .send_message(LINE, &body, ClientMessageId(1))
        .await?;

    // And it arrives at somebody else, which is the part that makes it a
    // conversation rather than a text box.
    let deadline = Instant::now() + WAIT;
    let mut heard = false;
    while Instant::now() < deadline && !heard {
        match tokio::time::timeout(Duration::from_millis(500), listener.next_event()).await {
            Ok(Ok(ServerMessage::MessageReceived { body, .. })) => heard = body == "olá",
            Ok(Ok(_)) => {}
            // A dead link is information, and swallowing it — which is what an
            // `if let Ok(Ok(_))` here did — turns the real cause into an expired
            // deadline reporting the wrong thing. `docs/pendencias.md` #1 names
            // this pattern as one of the reasons it stayed undiagnosed.
            Ok(Err(error)) => panic!("the link died before the message arrived: {error}"),
            Err(_) => {}
        }
    }
    assert!(heard, "the message never reached the other person");

    // `:q` leaves, which is the last thing the help promised.
    app.on_key(Key::Char(':'));
    for character in "q".chars() {
        app.on_key(Key::Char(character));
    }
    let quit = app.on_key(Key::Enter);
    assert_eq!(
        quit,
        Some(seele_tui::app::Action::Command("q".into())),
        "`:q` did not reach the command handler"
    );

    server.shutdown();
    Ok(())
}
