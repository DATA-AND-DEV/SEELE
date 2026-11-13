/// `Event::MessagesChanged` may only be raised by `Shared::messages_changed`.
///
/// The two halves — bump the revision, then tell the shell — have to move
/// together, and they did not: `Command::OpenChannel` clears the room's
/// messages and raised the event **without** the bump. A shell that
/// refetches only when the number moves, which is the entire point of the
/// number, swallowed it, and switching Channel left the previous Channel's
/// conversation on screen under the new heading.
///
/// Nothing about that failed. The event fired, the listener ran, the guard
/// compared two equal numbers and correctly concluded there was nothing to
/// do. Every part behaved; the pair was wrong.
///
/// Reading the source is the only way to state "one door and no other" —
/// a behavioural test would have to know every future caller in advance.
#[test]
fn nobody_raises_the_event_without_moving_the_revision() {
    let fonte = include_str!("../lib.rs");
    let Some(depois) = fonte.split("fn messages_changed(&self)").nth(1) else {
        panic!("`messages_changed` is gone, and with it the only place the two halves are tied");
    };
    let Some(corpo) = depois.split("\n    }").next() else {
        panic!("`messages_changed` is never closed");
    };
    assert!(
        corpo.contains("fetch_add") && corpo.contains("Event::MessagesChanged"),
        "the one function that ties the revision to the notice no longer does \
             both:\n{corpo}"
    );

    // Everything else may only reach it through that function.
    //
    // Scanned up to the first `#[cfg(test)]` and no further, and comments
    // dropped. Both exclusions are load-bearing rather than tidy: this very
    // test names the event three times in its own assertions, and the
    // paragraphs above name it because that is what they are about. A guard
    // its own text can trip is as broken as one its own text can satisfy.
    let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
    let sem_comentarios: String = producao
        .lines()
        .filter(|channel| !channel.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mencoes = sem_comentarios.matches("Event::MessagesChanged").count();
    assert_eq!(
        mencoes, 1,
        "`Event::MessagesChanged` is raised somewhere other than \
             `messages_changed`, which is how the revision and the notice came \
             apart the first time"
    );
}
