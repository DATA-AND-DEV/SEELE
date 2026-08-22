//! The `seele-ffi` surface, exercised against a real Dogma.
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
use seele_ffi::{ConnectConfig, Event, EventListener, Pattern, Plug, PlugError, Trust};
use seele_server::casper::Location;
use seele_server::{DogmaConfig, Server};

const CAGE: u32 = 1;
const LINE: u32 = 1;
const WAIT: Duration = Duration::from_secs(5);

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

/// A scratch directory, so each test is a different pilot.
///
/// ADR 0017 binds a nickname to the identity that claimed it, which is exactly
/// what makes two tests sharing a home directory fight over a name.
fn home(name: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("seele-ffi-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path.to_string_lossy().into_owned()
}

fn connect(address: SocketAddr, nickname: &str) -> Result<Arc<Plug>, PlugError> {
    // `seele-ffi`'s unit tests cover the two ends of this — what goes into
    // `Destino`, and how a `Verdict` maps to a `Trust` — and neither of them
    // touches the one line that puts the second inside the value `connect`
    // hands back. Replacing that line with a constant `Trust::Known` left the
    // whole workspace green: the shell would announce nothing on first contact
    // and nothing on a disagreeing link, pinning every key in silence, which is
    // the defect this branch exists to remove.
    //
    // `home()` is wiped per nickname, so every connection here is a genuine
    // first contact with a Dogma that was just born. Saying so out loud is what
    // makes the constant impossible.
    Plug::connect(ConnectConfig {
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
    .map(|(plug, trust)| {
        assert!(
            matches!(trust, Trust::FirstContact { .. }),
            "a fresh home against a fresh Dogma is first contact, and the shell \
             was told {trust:?}"
        );
        plug
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
fn until<F: Fn(&Plug) -> bool>(plug: &Plug, done: F) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(plug) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    done(plug)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_connects_and_the_snapshot_describes_the_dogma() -> Result<()> {
    let (address, server) = start().await?;

    // `connect` blocks, and a shell must call it off the thread that draws.
    // Here that is `spawn_blocking`, which is what the Tauri command does too.
    let plug = tokio::task::spawn_blocking(move || connect(address, "ayanami")).await??;

    let snapshot = plug.snapshot();
    assert_eq!(snapshot.pattern, Pattern::Blue);
    assert_eq!(snapshot.dogma, "Terceira Tóquio");
    assert_eq!(snapshot.nickname, "ayanami");
    assert!(snapshot.me.is_some());
    assert!(!snapshot.cages.is_empty(), "no Cages in the snapshot");
    assert!(!snapshot.audio_available, "audio was off for this session");
    assert!(snapshot.ended.is_none());

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn entering_a_cage_puts_us_on_our_own_roster() -> Result<()> {
    let (address, server) = start().await?;
    let plug = tokio::task::spawn_blocking(move || connect(address, "rei")).await??;

    plug.insert_plug(CAGE)?;

    assert!(
        until(&plug, |plug| {
            let snapshot = plug.snapshot();
            snapshot
                .cages
                .iter()
                .any(|cage| cage.occupied_by_us && cage.pilots.iter().any(|p| p.is_self))
        }),
        "we entered a Cage and are not on its roster"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_a_cage_takes_us_off_our_own_roster() -> Result<()> {
    // The mirror of the test above, and it was missing.
    //
    // The Dogma does not echo `PilotLeft` to the pilot who caused it — "they
    // already know" — so this side of the roster is the shell's own
    // bookkeeping. `insert_plug` did it and `eject_plug` did not, and the
    // asymmetry is invisible from inside `Room`, which both halves are correct
    // about: the seat that never got cleared was cleared on the *server* and on
    // every *other* client.
    //
    // Reported from a real session twice over, as two complaints that turn out
    // to be one: "não dá pra sair de uma jaula e deixá-la vazia" — the Cage
    // never empties on the screen of the person who left it — and "o usuário
    // está numa jaula com outro, mas o segundo não consegue ver esse usuário" —
    // which is the same picture from the other chair, where the Dogma is right
    // and the leaver's own screen is the one lying.
    let (address, server) = start().await?;
    let plug = tokio::task::spawn_blocking(move || connect(address, "rei")).await??;

    plug.insert_plug(CAGE)?;
    assert!(
        until(&plug, |plug| {
            plug.snapshot()
                .cages
                .iter()
                .any(|cage| cage.occupied_by_us && cage.pilots.iter().any(|p| p.is_self))
        }),
        "we entered a Cage and are not on its roster"
    );

    plug.eject_plug()?;

    assert!(
        until(&plug, |plug| {
            let snapshot = plug.snapshot();
            !snapshot
                .cages
                .iter()
                .any(|cage| cage.occupied_by_us || cage.pilots.iter().any(|p| p.is_self))
        }),
        "we left the Cage and our own screen still draws us in it: {:?}",
        plug.snapshot()
            .cages
            .iter()
            .map(|cage| (
                cage.occupied_by_us,
                cage.pilots.iter().filter(|p| p.is_self).count()
            ))
            .collect::<Vec<_>>()
    );

    // And the way back in still works, which is the part of this that was a
    // trap rather than a wrong picture. The screen draws `SAIR DA JAULA` when
    // `occupied_by_us` is true and sends `eject_plug` when it is pressed. With
    // the seat never clearing, that button stayed on `SAIR DA JAULA` for the
    // rest of the session and every press ejected again: the Cage could not be
    // left on screen **and could not be re-entered**.
    plug.insert_plug(CAGE)?;
    assert!(
        until(&plug, |plug| {
            plug.snapshot()
                .cages
                .iter()
                .any(|cage| cage.occupied_by_us && cage.pilots.iter().any(|p| p.is_self))
        }),
        "we could not walk back into the Cage we had just left"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn two_shells_hold_a_conversation() -> Result<()> {
    let (address, server) = start().await?;

    let speaker = tokio::task::spawn_blocking(move || connect(address, "shinji")).await??;
    let listener = tokio::task::spawn_blocking(move || connect(address, "asuka")).await??;

    let heard = Arc::new(Recorder::default());
    listener.subscribe(Arc::clone(&heard) as Arc<dyn EventListener>);

    for plug in [&speaker, &listener] {
        plug.insert_plug(CAGE)?;
        plug.open_line(LINE)?;
    }

    speaker.send_message(LINE, "sync caiu aqui".into())?;

    assert!(
        until(&listener, |plug| {
            plug.messages().iter().any(|m| m.body == "sync caiu aqui")
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
    assert_eq!(message.author_nickname, "shinji");
    assert!(!message.own);
    assert!(message.at_seconds > 0, "no timestamp: {message:?}");

    server.shutdown();
    Ok(())
}

/// Muting is announced, not just applied.
///
/// `specs/07-tema-evangelion.md` gives the A.T. Field a marker in the roster,
/// which only means something if the other side ever sees it. Before this the
/// server ignored the message outright and the marker could never light up.
#[tokio::test(flavor = "multi_thread")]
async fn an_at_field_is_visible_to_everybody_else() -> Result<()> {
    let (address, server) = start().await?;

    let muted = tokio::task::spawn_blocking(move || connect(address, "kaworu")).await??;
    let watcher = tokio::task::spawn_blocking(move || connect(address, "misato")).await??;

    muted.insert_plug(CAGE)?;
    watcher.insert_plug(CAGE)?;
    assert!(
        until(&watcher, |plug| {
            let snapshot = plug.snapshot();
            snapshot
                .cages
                .iter()
                .any(|cage| cage.pilots.iter().any(|pilot| pilot.nickname == "kaworu"))
        }),
        "the other pilot never appeared"
    );

    muted.set_at_field(true)?;

    assert!(
        until(&watcher, |plug| {
            let snapshot = plug.snapshot();
            snapshot.cages.iter().any(|cage| {
                cage.pilots
                    .iter()
                    .any(|pilot| pilot.nickname == "kaworu" && pilot.at_field)
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
    first.insert_plug(CAGE)?;
    first.open_line(LINE)?;
    first.send_message(LINE, "primeira coisa dita".into())?;
    assert!(
        until(&first, |plug| !plug.messages().is_empty()),
        "the message was never committed"
    );
    // Ending the first session is what makes this a resumption rather than two
    // clients watching at once.
    first.disconnect();
    drop(first);

    let second = tokio::task::spawn_blocking(move || connect(address, "makoto")).await??;
    second.open_line(LINE)?;

    assert!(
        until(&second, |plug| {
            plug.messages()
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
/// `plug` de um lado e o do app do outro, que é o que a frase quer dizer.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_started_in_the_terminal_resumes_in_the_desktop() -> Result<()> {
    use seele_core::{Client, MemoryPinStore, Room};
    use seele_proto::ids::{CageId, ClientMessageId, LineId};

    let (address, server) = start().await?;

    // ---- o lado do `plug`: seele-core cru, com o mesmo Room que a TUI projeta.
    let mut terminal = Client::connect(
        address,
        "localhost",
        &address.to_string(),
        "ayanami",
        &ed25519_dalek::SigningKey::from_bytes(&[42; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;

    let mut room = Room::new();
    room.adopt(terminal.session(), "ayanami");
    terminal.insert_plug(CageId(CAGE)).await?;
    room.enter_cage(CageId(CAGE));
    terminal.join_line(LineId(LINE)).await?;
    room.open_line(LineId(LINE));
    terminal
        .send_message(LineId(LINE), "dito no terminal", ClientMessageId(1))
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
    let desktop = tokio::task::spawn_blocking(move || connect(address, "shinji")).await??;
    desktop.open_line(LINE)?;

    assert!(
        until(&desktop, |plug| {
            plug.messages().iter().any(|m| m.body == "dito no terminal")
        }),
        "o app abriu a Linha e não viu o que o terminal disse"
    );

    let mensagens = desktop.messages();
    let mensagem = mensagens
        .iter()
        .find(|m| m.body == "dito no terminal")
        .expect("mensagem");

    // Sem perda: quem escreveu, e quando — não só o corpo.
    assert_eq!(mensagem.author_nickname, "ayanami");
    assert!(!mensagem.own, "atribuída ao piloto errado");
    assert!(
        mensagem.at_seconds > 1_600_000_000,
        "sem horário do servidor: {mensagem:?}"
    );

    server.shutdown();
    Ok(())
}

/// Every failure is an enum a shell can write its own sentence for.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_dogma_is_an_enum_and_not_a_message() -> Result<()> {
    // Port 1 on the loopback: nothing listens, and nothing will.
    let nowhere = SocketAddr::from(([127, 0, 0, 1], 1));
    let failure = tokio::task::spawn_blocking(move || connect(nowhere, "ninguem"))
        .await?
        .expect_err("connecting to nothing succeeded");

    assert!(
        matches!(
            failure,
            PlugError::Unreachable | PlugError::HandshakeTimeout
        ),
        "unexpected failure: {failure:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_that_does_not_resolve_says_so_specifically() -> Result<()> {
    let failure = tokio::task::spawn_blocking(|| {
        Plug::connect(ConnectConfig {
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

    assert_eq!(failure, PlugError::UnresolvableHost);
    Ok(())
}

/// Adjusting the volume of somebody who is not here is a named failure.
#[tokio::test(flavor = "multi_thread")]
async fn the_volume_of_a_stranger_is_refused_by_name() -> Result<()> {
    let (address, server) = start().await?;
    let plug = tokio::task::spawn_blocking(move || connect(address, "hyuga")).await??;

    assert_eq!(
        plug.set_volume("ninguem".into(), 50),
        Err(PlugError::UnknownPilot)
    );

    server.shutdown();
    Ok(())
}

/// Dropping the handle ends the session.
#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_handle_disconnects() -> Result<()> {
    let (address, server) = start().await?;
    let plug = tokio::task::spawn_blocking(move || connect(address, "aoba")).await??;
    plug.insert_plug(CAGE)?;
    assert!(until(&plug, |plug| plug.snapshot().pattern == Pattern::Blue));

    drop(plug);
    // Nothing to assert beyond not hanging: a handle whose driver thread
    // outlives it would keep a QUIC connection and an audio thread alive for
    // the life of the process.
    tokio::time::sleep(Duration::from_millis(200)).await;

    server.shutdown();
    Ok(())
}

/// A shell asks for a room and the Dogma makes one.
///
/// The bridge's own unit tests stop at the command queue: they prove the right
/// `Command` is enqueued and that a blank name is swallowed, and they cannot
/// see the arm on the driver thread that turns it into a frame. Gutting that
/// arm — keeping the `match` branch and dropping the `await` inside it — left
/// `seele-ffi` and `seele-conformance` entirely green until this test existed.
/// The button would have done nothing at all, silently, in the shipped app.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_asks_for_a_room_and_the_dogma_makes_it() -> Result<()> {
    let (address, server) = start().await?;
    let plug = tokio::task::spawn_blocking(move || connect(address, "anfitria")).await??;

    // The first account on a Dogma is its Comandante, which is what makes this
    // shell the one that may ask. The field exists so a screen can decide
    // whether to draw the control at all.
    assert!(
        plug.snapshot().may_manage_cages,
        "the shell that hosted this Dogma was not told it may make rooms"
    );

    let recorder = Arc::new(Recorder::default());
    plug.subscribe(Arc::clone(&recorder) as Arc<dyn EventListener>);

    plug.create_line("planejamento".into())?;
    plug.create_cage("CAGE-02 SALA DOS FUNDOS".into(), 8, None)?;

    assert!(
        until(&plug, |plug| plug
            .snapshot()
            .cages
            .iter()
            .any(|cage| cage.name == "CAGE-02 SALA DOS FUNDOS")),
        "the room never reached the snapshot the screen reads"
    );
    assert!(
        until(&plug, |plug| plug
            .snapshot()
            .lines
            .iter()
            .any(|line| line.name == "planejamento")),
        "the Line never reached the snapshot"
    );
    assert!(
        recorder.saw(|event| matches!(event, Event::ChannelsChanged)),
        "nothing woke the shell to redraw the channel list"
    );

    // And the room is a room: somebody can walk into it. A Cage that exists in
    // a list and cannot be entered is a row, not a channel.
    plug.insert_plug(2)?;
    assert!(until(&plug, |plug| plug
        .snapshot()
        .cages
        .iter()
        .any(|cage| cage.id == 2 && cage.occupied_by_us)));

    drop(plug);
    server.shutdown();
    Ok(())
}

/// A shell that asks without the permission is refused by the Dogma.
///
/// `may_manage_cages` is convenience — `specs/08-seguranca.md` puts the
/// security in the server refusing — so this shell ignores it and asks anyway,
/// which is exactly what a hostile client would do. What comes back is an
/// enumerated notice and no room.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_without_the_permission_is_refused_by_the_dogma() -> Result<()> {
    let (address, server) = start().await?;

    // Whoever connects first hosts. This one is the guest.
    let anfitria = tokio::task::spawn_blocking(move || connect(address, "anfitria2")).await??;
    let convidado = tokio::task::spawn_blocking(move || connect(address, "convidado")).await??;

    assert!(
        !convidado.snapshot().may_manage_cages,
        "the guest was told it may make rooms, and this test measures nothing"
    );

    let recorder = Arc::new(Recorder::default());
    convidado.subscribe(Arc::clone(&recorder) as Arc<dyn EventListener>);

    convidado.create_cage("CAGE-DO-INTRUSO".into(), 8, None)?;

    assert!(
        until(&convidado, |_| {
            recorder.saw(|event| matches!(
            event,
            Event::NoticeRaised { notice } if notice.reason == seele_ffi::NoticeReason::PermissionDenied
        ))
        }),
        "the Dogma refused in silence, which looks exactly like a Dogma that is broken"
    );

    // The half that proves the refusal was the server's: the host is connected
    // the whole time and never sees the room appear. A shell that had merely
    // hidden its own button would produce the same screen for the guest and no
    // difference at all here.
    let antes = anfitria.snapshot().cages.len();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        anfitria.snapshot().cages.len(),
        antes,
        "the intruder's room was announced to the host"
    );
    assert!(!convidado
        .snapshot()
        .cages
        .iter()
        .any(|cage| cage.name == "CAGE-DO-INTRUSO"));

    drop(anfitria);
    drop(convidado);
    server.shutdown();
    Ok(())
}
