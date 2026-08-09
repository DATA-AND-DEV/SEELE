//! The desktop client — a Tauri shell over [`seele_ffi`].
//!
//! `specs/06-clientes-gui.md` sets the shape of this file in one sentence:
//! "Nenhuma lógica de protocolo em JavaScript. Se o frontend precisa saber o
//! que é um `ssrc`, algo está errado." So the frontend gets a `Snapshot` and
//! sends back verbs — enter this Cage, say this, mute — and every one of them
//! is a call straight through to the FFI.
//!
//! Nothing here decides anything either. If a command in this file grows a
//! judgement, it belongs in `seele-core`, and the terminal client would have had
//! to grow the same one.
//!
//! # Threading
//!
//! [`seele_ffi::Plug::connect`] blocks, so it runs on a blocking thread. Events
//! arrive on the FFI's driver thread; [`Bridge`] is what marshals them onto the
//! webview, which is the "a casca marshala para sua thread de UI" the spec asks
//! for.

// A desktop shell with no window is not a desktop shell. The attribute keeps
// the console from opening behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use seele_ffi::{ConnectConfig, Event, EventListener, Plug, PlugError, Snapshot, VoiceMode};
use tauri::{AppHandle, Emitter, Manager, State};

/// The name the webview listens on.
///
/// One channel rather than one per variant: the payload already says which
/// [`Event`] it is, and a frontend subscribing to seven names would drift from
/// this list the first time one is added.
const EVENT_CHANNEL: &str = "seele://event";

/// Everything the commands share.
#[derive(Default)]
struct Session {
    plug: Mutex<Option<Arc<Plug>>>,
}

impl Session {
    /// The live handle, or the reason there is none.
    fn plug(&self) -> Result<Arc<Plug>, PlugError> {
        self.plug
            .lock()
            .map_err(|_| PlugError::NotConnected)?
            .clone()
            .ok_or(PlugError::NotConnected)
    }
}

/// Carries FFI events onto the webview.
struct Bridge {
    app: AppHandle,
}

impl EventListener for Bridge {
    fn on_event(&self, event: Event) {
        // A failed emit means the window is gone, which is not worth a log line
        // per event during shutdown.
        let _ = self.app.emit(EVENT_CHANNEL, &event);
    }
}

/// Where this client keeps its identity and its pins. ADR 0017.
///
/// The FFI takes a path because the shell knows where its platform keeps
/// configuration and the core knows how to persist an identity. `$SEELE_HOME`
/// comes first so the desktop app and `plug` can be told to be the same pilot —
/// which is what makes a session resumable between them.
fn config_dir(app: &AppHandle) -> String {
    if let Ok(home) = std::env::var("SEELE_HOME") {
        return home;
    }
    // The same `~/.config/seele` the terminal client uses, deliberately: two
    // clients on one machine should be one pilot unless told otherwise.
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return format!("{xdg}/seele");
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.config/seele");
    }
    app.path()
        .app_config_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".seele".to_owned())
}

#[tauri::command]
async fn connect(
    app: AppHandle,
    session: State<'_, Session>,
    server: String,
    nickname: String,
    audio: bool,
) -> Result<Snapshot, PlugError> {
    if session.plug().is_ok() {
        return Err(PlugError::AlreadyConnected);
    }

    let home = config_dir(&app);
    let config = ConnectConfig {
        server,
        nickname,
        home,
        audio,
    };

    // `connect` blocks on a QUIC handshake. Running it on the async runtime's
    // worker would stall every other command until it finished or timed out.
    let plug = tauri::async_runtime::spawn_blocking(move || Plug::connect(config))
        .await
        .map_err(|_| PlugError::Unreachable)??;

    plug.subscribe(Arc::new(Bridge { app }) as Arc<dyn EventListener>);
    let snapshot = plug.snapshot();

    if let Ok(mut slot) = session.plug.lock() {
        *slot = Some(plug);
    }
    Ok(snapshot)
}

#[tauri::command]
fn disconnect(session: State<'_, Session>) {
    if let Ok(mut slot) = session.plug.lock() {
        // Dropping the handle is what ends the session; taking it out of the
        // slot is what makes the next `connect` allowed.
        slot.take();
    }
}

#[tauri::command]
fn snapshot(session: State<'_, Session>) -> Result<Snapshot, PlugError> {
    Ok(session.plug()?.snapshot())
}

#[tauri::command]
fn insert_plug(session: State<'_, Session>, cage: u32) -> Result<(), PlugError> {
    session.plug()?.insert_plug(cage)
}

#[tauri::command]
fn eject_plug(session: State<'_, Session>) -> Result<(), PlugError> {
    session.plug()?.eject_plug()
}

#[tauri::command]
fn open_line(session: State<'_, Session>, line: u32) -> Result<(), PlugError> {
    session.plug()?.open_line(line)
}

#[tauri::command]
fn send_message(session: State<'_, Session>, line: u32, body: String) -> Result<(), PlugError> {
    session.plug()?.send_message(line, body)
}

#[tauri::command]
fn set_at_field(session: State<'_, Session>, on: bool) -> Result<(), PlugError> {
    session.plug()?.set_at_field(on)
}

#[tauri::command]
fn set_total_isolation(session: State<'_, Session>, on: bool) -> Result<(), PlugError> {
    session.plug()?.set_total_isolation(on)
}

/// Push-to-talk, reported as it happens.
///
/// Not fallible on purpose: a key coming *up* must never be refused. Returning
/// an error here would give the frontend a path where the microphone was opened
/// and the close was rejected.
#[tauri::command]
fn set_talking(session: State<'_, Session>, talking: bool) {
    if let Ok(plug) = session.plug() {
        plug.set_talking(talking);
    }
}

#[tauri::command]
fn set_voice_mode(session: State<'_, Session>, mode: VoiceMode) -> Result<(), PlugError> {
    session.plug()?.set_voice_mode(mode);
    Ok(())
}

#[tauri::command]
fn set_volume(
    session: State<'_, Session>,
    nickname: String,
    percent: u16,
) -> Result<(), PlugError> {
    session.plug()?.set_volume(nickname, percent)
}

fn main() {
    // Marca de arranque. `specs/06-clientes-gui.md` aceita M5 com inicialização
    // abaixo de 2 s, e um critério que ninguém mede é um critério que passa a
    // valer o que a lembrança de alguém sobre "pareceu rápido" valer.
    let arranque = std::time::Instant::now();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "seele_app=info,seele_ffi=info,seele_core=info".into()),
        )
        .init();

    // A window that cannot open is not a case with a graceful path: there is
    // nowhere left to show the reason. It goes to the log and to the exit code.
    let started = tauri::Builder::default()
        .manage(Session::default())
        .setup(move |_app| {
            tracing::info!(millis = arranque.elapsed().as_millis(), "janela pronta");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            snapshot,
            insert_plug,
            eject_plug,
            open_line,
            send_message,
            set_at_field,
            set_total_isolation,
            set_talking,
            set_voice_mode,
            set_volume,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = started {
        tracing::error!(%error, "the desktop shell could not start");
        std::process::exit(1);
    }
}
