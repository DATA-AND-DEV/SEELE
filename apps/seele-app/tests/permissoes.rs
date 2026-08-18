//! Guards for the half of the shell that only the ACL can answer.
//!
//! `tests/frontend.rs` reads the scripts and can prove that a listener is
//! *written*. It cannot prove that the listener is ever *registered*, and the
//! difference is not academic: `listen()` is not a JavaScript call, it is an
//! IPC call to the `event` core plugin, and every core-plugin call in Tauri v2
//! goes through the capability system before it reaches any code. With no
//! capability file the call is rejected, the returned promise rejects into
//! nothing, and the page keeps running with a listener that will never fire.
//!
//! That is the shape of the bug these guards exist for. They do not read the
//! configuration and agree with it — they build the real app from the real
//! `tauri.conf.json`, put a webview in front of it, and make the same IPC call
//! the frontend makes.

#![allow(
    clippy::expect_used,
    reason = "a test that cannot build the app has nothing left to assert"
)]

use tauri::ipc::CallbackFn;
use tauri::test::{mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindowBuilder;

/// The real app: the real `tauri.conf.json`, the real generated ACL.
///
/// The runtime is the mock one — there is no window server in CI, and none is
/// needed. What is being asked here is decided before any pixel: the capability
/// resolution happens in `Webview::on_message`, which the mock runtime reaches
/// exactly as the real one does.
fn app() -> tauri::App<MockRuntime> {
    mock_builder()
        .build(contexto())
        .expect("the desktop shell must build from its own configuration")
}

/// This app's own `tauri.conf.json`, and the ACL generated beside it.
///
/// Behind a function, and it has to be: `generate_context!` embeds `Info.plist`
/// under a fixed symbol name, so a second expansion anywhere in the same crate
/// is a duplicate-symbol error at link time. One expansion, called as often as
/// a test needs a fresh context.
fn contexto() -> tauri::Context<MockRuntime> {
    tauri::generate_context!()
}

/// Makes the same IPC call the frontend makes, and reports what came back.
fn call(
    webview: &tauri::WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<(), String> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a constant URL parses"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

/// The window the frontend actually runs in.
///
/// The label matters and is not decoration: a capability lists the windows it
/// applies to, so a capability written for `main` grants nothing to a window
/// called anything else. `tauri.conf.json` names no label, and Tauri's default
/// is `main`.
fn main_webview(app: &tauri::App<MockRuntime>) -> tauri::WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, "main", Default::default())
        .build()
        .expect("the mock runtime always yields a webview")
}

#[test]
fn the_page_may_register_an_event_listener() {
    // The whole of `listen()`. Three screens call it — the session's snapshot
    // pump, the call screen's, and the updater's — and the file drop that
    // attaches a file is a fourth. Denied, every one of them is a listener that
    // exists in the source and never receives anything.
    //
    // The two snapshot pumps have a 500 ms `setInterval` beside them and go on
    // looking alive without this; the file drop has nothing beside it, which is
    // why it is the one that was noticed.
    let app = app();
    let webview = main_webview(&app);
    if let Err(erro) = call(
        &webview,
        "plugin:event|listen",
        serde_json::json!({
            "event": "tauri://drag-drop",
            "target": { "kind": "Any" },
            "handler": 1_u32,
        }),
    ) {
        panic!(
            "the page cannot listen for events at all: {erro}\n\n\
             Every `listen(...)` in `ui/` is dead, and the file drop that \
             attaches a file is dead with them."
        );
    }
}

/// A command of the app's own, standing in for the forty-odd in `main.rs`.
#[tauri::command]
fn um_comando_do_proprio_app() -> &'static str {
    "pong"
}

#[test]
fn the_shells_own_commands_stay_reachable_from_the_page() {
    // The other side of the same wall. A capability file is what the ACL reads,
    // and the day one is written wrong — an app-level manifest turned on, a
    // window label that does not match — every `invoke("…")` in `ui/` dies at
    // once, which is a far larger outage than the one that brought the file
    // here.
    //
    // The command below is a stand-in and says so: the real ones live in the
    // `main.rs` binary, which an integration test cannot link against. What is
    // being asked is not whether `pasta_de_downloads` exists — `frontend.rs`
    // already ties every `invoke("…")` to a registered command, both ways — but
    // whether a command that is *not* a plugin command still gets through this
    // capability set. That answer does not depend on which command it is.
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![um_comando_do_proprio_app])
        .build(contexto())
        .expect("the desktop shell must build from its own configuration");
    let webview = main_webview(&app);
    let erro = call(&webview, "um_comando_do_proprio_app", serde_json::json!({}));
    assert!(
        erro.is_ok(),
        "the page can no longer call the shell's own commands: {erro:?}"
    );
}

#[test]
fn the_page_may_stop_listening() {
    // The other half of the pair. `listen()` resolves to an unlisten function,
    // and a screen that can subscribe but not unsubscribe leaks a handler per
    // call — and, worse, fails at the moment a screen is torn down rather than
    // at the moment it is built, which is far from the cause.
    let app = app();
    let webview = main_webview(&app);
    let erro = call(
        &webview,
        "plugin:event|unlisten",
        serde_json::json!({ "event": "tauri://drag-drop", "eventId": 1_u32 }),
    );
    assert!(
        erro.is_ok(),
        "the page may subscribe but never unsubscribe: {erro:?}"
    );
}
