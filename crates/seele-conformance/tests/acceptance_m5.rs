//! The `seele-ffi` surface, exercised against a real server.
//!
//! `specs/06-clientes-gui.md` specifies this surface and the rules it must
//! keep. The unit tests in `seele-ffi` pin the shapes; what is here is the part
//! they cannot check — that a graphical shell holding one of these handles can
//! actually connect, see the room, say something, and be heard.
//!
//! The desktop client is a thin adapter over exactly these calls. If a test
//! here needs something the surface does not offer, the surface is incomplete
//! and the app would have reached around it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::{
    ConnectConfig, Connection, ConnectionError, Event, EventListener, LinkTrust, Trust,
};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

const VOICE_ROOM: u32 = 1;
const CHANNEL: u32 = 1;
const WAIT: Duration = Duration::from_secs(5);

async fn start() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let server = Arc::new(Daemon::bind(config).await?);
    let address = server.local_addr()?;
    let accepting = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = accepting.run().await;
    });
    Ok((address, server))
}

/// A scratch directory, so each test is a different person.
///
/// ADR 0017 binds a nickname to the identity that claimed it, which is exactly
/// what makes two tests sharing a home directory fight over a name.
fn home(name: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("seele-ffi-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path.to_string_lossy().into_owned()
}

fn connect(address: SocketAddr, nickname: &str) -> Result<Arc<Connection>, ConnectionError> {
    // `seele-ffi`'s unit tests cover the two ends of this — what goes into
    // `Destino`, and how a `Verdict` maps to a `Trust` — and neither of them
    // touches the one channel that puts the second inside the value `connect`
    // hands back. Replacing that channel with a constant `Trust::Known` left the
    // whole workspace green: the shell would announce nothing on first contact
    // and nothing on a disagreeing link, pinning every key in silence, which is
    // the defect this branch exists to remove.
    //
    // `home()` is wiped per nickname, so every connection here is a genuine
    // first contact with a server that was just born. Saying so out loud is what
    // makes the constant impossible.
    Connection::connect(ConnectConfig {
        server: address.to_string(),
        alternate_servers: Vec::new(),
        nickname: nickname.to_owned(),
        home: home(nickname),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // No sound card on a CI box, and the text half needs none.
        audio: false,
        capture_device: None,
        playback_device: None,
    })
    .map(|(connection, trust)| {
        assert!(
            matches!(trust, Trust::FirstContact { .. }),
            "a fresh home against a fresh Server is first contact, and the shell \
             was told {trust:?}"
        );
        connection
    })
}

/// Records what the shell would have been woken for.
#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl EventListener for Recorder {
    fn on_event(&self, event: Event) {
        if let Ok(mut seen) = self.0.lock() {
            seen.push(event);
        }
    }
}

impl Recorder {
    fn saw<F: Fn(&Event) -> bool>(&self, matches: F) -> bool {
        self.0
            .lock()
            .map(|seen| seen.iter().any(matches))
            .unwrap_or(false)
    }
}

/// Polls the snapshot until it says what the test is waiting for.
fn until<F: Fn(&Connection) -> bool>(connection: &Connection, done: F) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(connection) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    done(connection)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_connects_and_the_snapshot_describes_the_server() -> Result<()> {
    let (address, server) = start().await?;

    // `connect` blocks, and a shell must call it off the thread that draws.
    // Here that is `spawn_blocking`, which is what the Tauri command does too.
    let connection = tokio::task::spawn_blocking(move || connect(address, "marcela")).await??;

    let snapshot = connection.snapshot();
    assert_eq!(snapshot.link_state, LinkTrust::Verified);
    assert_eq!(snapshot.server, "Casa");
    assert_eq!(snapshot.nickname, "marcela");
    assert!(snapshot.me.is_some());
    assert!(
        !snapshot.voice_rooms.is_empty(),
        "no voice_rooms in the snapshot"
    );
    assert!(!snapshot.audio_available, "audio was off for this session");
    assert!(snapshot.ended.is_none());

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn entering_a_voice_room_puts_us_on_our_own_roster() -> Result<()> {
    let (address, server) = start().await?;
    let connection = tokio::task::spawn_blocking(move || connect(address, "rei")).await??;

    connection.insert_plug(VOICE_ROOM)?;

    assert!(
        until(&connection, |connection| {
            let snapshot = connection.snapshot();
            snapshot.voice_rooms.iter().any(|voice_room| {
                voice_room.occupied_by_us && voice_room.people.iter().any(|p| p.is_self)
            })
        }),
        "we entered a voice room and are not on its roster"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_a_voice_room_takes_us_off_our_own_roster() -> Result<()> {
    // The mirror of the test above, and it was missing.
    //
    // The server does not echo `PersonLeft` to the person who caused it — "they
    // already know" — so this side of the roster is the shell's own
    // bookkeeping. `insert_plug` did it and `eject_plug` did not, and the
    // asymmetry is invisible from inside `Room`, which both halves are correct
    // about: the seat that never got cleared was cleared on the *server* and on
    // every *other* client.
    //
    // Reported from a real session twice over, as two complaints that turn out
    // to be one: "não dá pra sair de uma jaula e deixá-la vazia" — the sala de voz
    // never empties on the screen of the person who left it — and "o usuário
    // está numa jaula com outro, mas o segundo não consegue ver esse usuário" —
    // which is the same picture from the other chair, where the server is right
    // and the leaver's own screen is the one lying.
    let (address, server) = start().await?;
    let connection = tokio::task::spawn_blocking(move || connect(address, "rei")).await??;

    connection.insert_plug(VOICE_ROOM)?;
    assert!(
        until(&connection, |connection| {
            connection.snapshot().voice_rooms.iter().any(|voice_room| {
                voice_room.occupied_by_us && voice_room.people.iter().any(|p| p.is_self)
            })
        }),
        "we entered a voice room and are not on its roster"
    );

    connection.eject_plug()?;

    assert!(
        until(&connection, |connection| {
            let snapshot = connection.snapshot();
            !snapshot.voice_rooms.iter().any(|voice_room| {
                voice_room.occupied_by_us || voice_room.people.iter().any(|p| p.is_self)
            })
        }),
        "we left the voice room and our own screen still draws us in it: {:?}",
        connection
            .snapshot()
            .voice_rooms
            .iter()
            .map(|voice_room| (
                voice_room.occupied_by_us,
                voice_room.people.iter().filter(|p| p.is_self).count()
            ))
            .collect::<Vec<_>>()
    );

    // And the way back in still works, which is the part of this that was a
    // trap rather than a wrong picture. The screen draws `SAIR DA JAULA` when
    // `occupied_by_us` is true and sends `eject_plug` when it is pressed. With
    // the seat never clearing, that button stayed on `SAIR DA JAULA` for the
    // rest of the session and every press ejected again: the voice room could not be
    // left on screen **and could not be re-entered**.
    connection.insert_plug(VOICE_ROOM)?;
    assert!(
        until(&connection, |connection| {
            connection.snapshot().voice_rooms.iter().any(|voice_room| {
                voice_room.occupied_by_us && voice_room.people.iter().any(|p| p.is_self)
            })
        }),
        "we could not walk back into the voice room we had just left"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn two_shells_hold_a_conversation() -> Result<()> {
    let (address, server) = start().await?;

    let speaker = tokio::task::spawn_blocking(move || connect(address, "rafael")).await??;
    let listener = tokio::task::spawn_blocking(move || connect(address, "carla")).await??;

    let heard = Arc::new(Recorder::default());
    listener.subscribe(Arc::clone(&heard) as Arc<dyn EventListener>);

    for connection in [&speaker, &listener] {
        connection.insert_plug(VOICE_ROOM)?;
        connection.open_channel(CHANNEL)?;
    }

    speaker.send_message(CHANNEL, "sync caiu aqui".into())?;

    assert!(
        until(&listener, |connection| {
            connection
                .messages()
                .iter()
                .any(|m| m.body == "sync caiu aqui")
        }),
        "the message never reached the other shell"
    );
    assert!(
        heard.saw(|event| matches!(event, Event::MessagesChanged)),
        "the shell was never woken to redraw"
    );

    // Attribution, and the server's own clock rather than the arrival time.
    let mensagens = listener.messages();
    let message = mensagens
        .iter()
        .find(|m| m.body == "sync caiu aqui")
        .expect("message");
    assert_eq!(message.author_nickname, "rafael");
    assert!(!message.own);
    assert!(message.at_seconds > 0, "no timestamp: {message:?}");

    server.shutdown();
    Ok(())
}

/// Muting is announced, not just applied.
///
/// `specs/07-estetica.md` gives the A.T. Field a marker in the roster,
/// which only means something if the other side ever sees it. Before this the
/// server ignored the message outright and the marker could never light up.
#[tokio::test(flavor = "multi_thread")]
async fn a_muted_mic_is_visible_to_everybody_else() -> Result<()> {
    let (address, server) = start().await?;

    let muted = tokio::task::spawn_blocking(move || connect(address, "helena")).await??;
    let watcher = tokio::task::spawn_blocking(move || connect(address, "daniel")).await??;

    muted.insert_plug(VOICE_ROOM)?;
    watcher.insert_plug(VOICE_ROOM)?;
    assert!(
        until(&watcher, |connection| {
            let snapshot = connection.snapshot();
            snapshot.voice_rooms.iter().any(|voice_room| {
                voice_room
                    .people
                    .iter()
                    .any(|person| person.nickname == "helena")
            })
        }),
        "the other person never appeared"
    );

    muted.set_muted(true)?;

    assert!(
        until(&watcher, |connection| {
            let snapshot = connection.snapshot();
            snapshot.voice_rooms.iter().any(|voice_room| {
                voice_room
                    .people
                    .iter()
                    .any(|person| person.nickname == "helena" && person.muted)
            })
        }),
        "a mute was applied locally and never announced"
    );

    server.shutdown();
    Ok(())
}

/// A shell arriving late reads what was already said.
///
/// The M5 acceptance criterion in `specs/06-clientes-gui.md`: "mesma sessão
/// pode ser retomada em outro cliente sem perda de histórico."
#[tokio::test(flavor = "multi_thread")]
async fn a_second_client_resumes_the_conversation_with_its_history() -> Result<()> {
    let (address, server) = start().await?;

    let first = tokio::task::spawn_blocking(move || connect(address, "maya")).await??;
    first.insert_plug(VOICE_ROOM)?;
    first.open_channel(CHANNEL)?;
    first.send_message(CHANNEL, "primeira coisa dita".into())?;
    assert!(
        until(&first, |connection| !connection.messages().is_empty()),
        "the message was never committed"
    );
    // Ending the first session is what makes this a resumption rather than two
    // clients watching at once.
    first.disconnect();
    drop(first);

    let second = tokio::task::spawn_blocking(move || connect(address, "makoto")).await??;
    second.open_channel(CHANNEL)?;

    assert!(
        until(&second, |connection| {
            connection
                .messages()
                .iter()
                .any(|m| m.body == "primeira coisa dita")
        }),
        "a client that arrived late saw an empty room"
    );

    let mensagens = second.messages();
    let message = mensagens
        .iter()
        .find(|m| m.body == "primeira coisa dita")
        .expect("message");
    assert_eq!(message.author_nickname, "maya");
    assert!(
        message.at_seconds > 1_600_000_000,
        "history with no time on it — or with the wrong unit, which reads as \
         1970 and is the same loss with extra confusion: {message:?}"
    );

    server.shutdown();
    Ok(())
}

/// O critério de aceite de M5, entre as duas cascas de verdade.
///
/// `specs/06-clientes-gui.md`: "mesma sessão pode ser retomada em outro cliente
/// sem perda de histórico." Os outros testes deste arquivo usam dois handles da
/// FFI, o que prova a retomada mas não a travessia — este usa o caminho do
/// `connection` de um lado e o do app do outro, que é o que a frase quer dizer.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_started_in_the_terminal_resumes_in_the_desktop() -> Result<()> {
    use seele_core::{Client, MemoryPinStore, Room};
    use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};

    let (address, server) = start().await?;

    // ---- o lado do `connection`: seele-core cru, com o mesmo Room que a TUI projeta.
    let mut terminal = Client::connect(
        address,
        "localhost",
        &address.to_string(),
        "marcela",
        &ed25519_dalek::SigningKey::from_bytes(&[42; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;

    let mut room = Room::new();
    room.adopt(terminal.session(), "marcela");
    terminal.insert_plug(VoiceRoomId(VOICE_ROOM)).await?;
    room.enter_voice_room(VoiceRoomId(VOICE_ROOM));
    terminal.join_channel(ChannelId(CHANNEL)).await?;
    room.open_channel(ChannelId(CHANNEL));
    terminal
        .send_message(ChannelId(CHANNEL), "dito no terminal", ClientMessageId(1))
        .await?;

    // Espera a mensagem voltar, que é quando ela está durável.
    //
    // O prazo já foi de vinte segundos, quando eu achava que o problema era
    // lentidão do runner. Não era: o `select!` da sessão cancelava a leitura no
    // meio do quadro e o `SendMessage` sumia — nenhum prazo resolve uma
    // mensagem que o servidor nunca recebeu. Com a tarefa leitora dedicada, o
    // caminho todo (lote, transação, difusão) leva centenas de milissegundos.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && room.messages.is_empty() {
        match tokio::time::timeout(Duration::from_millis(500), terminal.next_event()).await {
            Ok(Ok(message)) => {
                room.apply(&message);
            }
            // O enlace caiu: é informação, e engoli-la — que era o que este
            // `if let Ok(Ok(_))` fazia — troca a causa real por um prazo
            // esgotado dizendo outra coisa. `docs/pendencias.md` #1 aponta este
            // padrão como um dos motivos de ela ter ficado sem diagnóstico.
            Ok(Err(erro)) => panic!("o enlace caiu antes da confirmação: {erro}"),
            Err(_) => {}
        }
    }
    assert_eq!(room.messages.len(), 1, "a mensagem não foi confirmada");
    terminal.disconnect();
    drop(terminal);

    // ---- o lado do app: a mesma superfície que o Tauri chama.
    let desktop = tokio::task::spawn_blocking(move || connect(address, "rafael")).await??;
    desktop.open_channel(CHANNEL)?;

    assert!(
        until(&desktop, |connection| {
            connection
                .messages()
                .iter()
                .any(|m| m.body == "dito no terminal")
        }),
        "o app abriu a Linha e não viu o que o terminal disse"
    );

    let mensagens = desktop.messages();
    let mensagem = mensagens
        .iter()
        .find(|m| m.body == "dito no terminal")
        .expect("mensagem");

    // Sem perda: quem escreveu, e quando — não só o corpo.
    assert_eq!(mensagem.author_nickname, "marcela");
    assert!(!mensagem.own, "atribuída ao pessoa errado");
    assert!(
        mensagem.at_seconds > 1_600_000_000,
        "sem horário do servidor: {mensagem:?}"
    );

    server.shutdown();
    Ok(())
}

/// Every failure is an enum a shell can write its own sentence for.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_server_is_an_enum_and_not_a_message() -> Result<()> {
    // Port 1 on the loopback: nothing listens, and nothing will.
    let nowhere = SocketAddr::from(([127, 0, 0, 1], 1));
    let failure = tokio::task::spawn_blocking(move || connect(nowhere, "ninguem"))
        .await?
        .expect_err("connecting to nothing succeeded");

    assert!(
        matches!(
            failure,
            ConnectionError::Unreachable | ConnectionError::HandshakeTimeout
        ),
        "unexpected failure: {failure:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_that_does_not_resolve_says_so_specifically() -> Result<()> {
    let failure = tokio::task::spawn_blocking(|| {
        Connection::connect(ConnectConfig {
            server: "nao-existe.invalid:8383".into(),
            alternate_servers: Vec::new(),
            nickname: "ninguem".into(),
            home: home("unresolvable"),
            join_secret: None,
            expected_fingerprint: None,
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        })
    })
    .await?
    .expect_err("an impossible host resolved");

    assert_eq!(failure, ConnectionError::UnresolvableHost);
    Ok(())
}

/// Adjusting the volume of somebody who is not here is a named failure.
#[tokio::test(flavor = "multi_thread")]
async fn the_volume_of_a_stranger_is_refused_by_name() -> Result<()> {
    let (address, server) = start().await?;
    let connection = tokio::task::spawn_blocking(move || connect(address, "hyuga")).await??;

    assert_eq!(
        connection.set_volume("ninguem".into(), 50),
        Err(ConnectionError::UnknownPerson)
    );

    server.shutdown();
    Ok(())
}

/// Dropping the handle ends the session.
#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_handle_disconnects() -> Result<()> {
    let (address, server) = start().await?;
    let connection = tokio::task::spawn_blocking(move || connect(address, "aoba")).await??;
    connection.insert_plug(VOICE_ROOM)?;
    assert!(until(&connection, |connection| connection
        .snapshot()
        .link_state
        == LinkTrust::Verified));

    drop(connection);
    // Nothing to assert beyond not hanging: a handle whose driver thread
    // outlives it would keep a QUIC connection and an audio thread alive for
    // the life of the process.
    tokio::time::sleep(Duration::from_millis(200)).await;

    server.shutdown();
    Ok(())
}

/// A shell asks for a room and the server makes one.
///
/// The bridge's own unit tests stop at the command queue: they prove the right
/// `Command` is enqueued and that a blank name is swallowed, and they cannot
/// see the arm on the driver thread that turns it into a frame. Gutting that
/// arm — keeping the `match` branch and dropping the `await` inside it — left
/// `seele-ffi` and `seele-conformance` entirely green until this test existed.
/// The button would have done nothing at all, silently, in the shipped app.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_asks_for_a_room_and_the_server_makes_it() -> Result<()> {
    let (address, server) = start().await?;
    let connection = tokio::task::spawn_blocking(move || connect(address, "anfitria")).await??;

    // The first account on a server is its Comandante, which is what makes this
    // shell the one that may ask. The field exists so a screen can decide
    // whether to draw the control at all.
    assert!(
        connection.snapshot().may_manage_voice_rooms,
        "the shell that hosted this server was not told it may make rooms"
    );

    let recorder = Arc::new(Recorder::default());
    connection.subscribe(Arc::clone(&recorder) as Arc<dyn EventListener>);

    connection.create_channel("planejamento".into())?;
    connection.create_voice_room("SALA-02 SALA DOS FUNDOS".into(), 8, None)?;

    assert!(
        until(&connection, |connection| {
            connection
                .snapshot()
                .voice_rooms
                .iter()
                .any(|voice_room| voice_room.name == "SALA-02 SALA DOS FUNDOS")
        }),
        "the room never reached the snapshot the screen reads"
    );
    assert!(
        until(&connection, |connection| connection
            .snapshot()
            .channels
            .iter()
            .any(|channel| channel.name == "planejamento")),
        "the Channel never reached the snapshot"
    );
    assert!(
        recorder.saw(|event| matches!(event, Event::ChannelsChanged)),
        "nothing woke the shell to redraw the channel list"
    );

    // And the room is a room: somebody can walk into it. A voice room that exists in
    // a list and cannot be entered is a row, not a channel.
    connection.insert_plug(2)?;
    assert!(until(&connection, |connection| connection
        .snapshot()
        .voice_rooms
        .iter()
        .any(
            |voice_room| voice_room.id == 2 && voice_room.occupied_by_us
        )));

    drop(connection);
    server.shutdown();
    Ok(())
}

/// A shell that asks without the permission is refused by the server.
///
/// `may_manage_voice_rooms` is convenience — `specs/08-seguranca.md` puts the
/// security in the server refusing — so this shell ignores it and asks anyway,
/// which is exactly what a hostile client would do. What comes back is an
/// enumerated notice and no room.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_without_the_permission_is_refused_by_the_server() -> Result<()> {
    let (address, server) = start().await?;

    // Whoever connects first hosts. This one is the guest.
    let anfitria = tokio::task::spawn_blocking(move || connect(address, "anfitria2")).await??;
    let convidado = tokio::task::spawn_blocking(move || connect(address, "convidado")).await??;

    assert!(
        !convidado.snapshot().may_manage_voice_rooms,
        "the guest was told it may make rooms, and this test measures nothing"
    );

    let recorder = Arc::new(Recorder::default());
    convidado.subscribe(Arc::clone(&recorder) as Arc<dyn EventListener>);

    convidado.create_voice_room("SALA-DO-INTRUSO".into(), 8, None)?;

    assert!(
        until(&convidado, |_| {
            recorder.saw(|event| matches!(
            event,
            Event::NoticeRaised { notice } if notice.reason == seele_ffi::NoticeReason::PermissionDenied
        ))
        }),
        "the server refused in silence, which looks exactly like a server that is broken"
    );

    // The half that proves the refusal was the server's: the host is connected
    // the whole time and never sees the room appear. A shell that had merely
    // hidden its own button would produce the same screen for the guest and no
    // difference at all here.
    let antes = anfitria.snapshot().voice_rooms.len();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        anfitria.snapshot().voice_rooms.len(),
        antes,
        "the intruder's room was announced to the host"
    );
    assert!(!convidado
        .snapshot()
        .voice_rooms
        .iter()
        .any(|voice_room| voice_room.name == "SALA-DO-INTRUSO"));

    drop(anfitria);
    drop(convidado);
    server.shutdown();
    Ok(())
}
