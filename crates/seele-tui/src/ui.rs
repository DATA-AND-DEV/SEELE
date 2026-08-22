//! Drawing. Takes an [`App`] and produces cells.
//!
//! Nothing here decides anything: if a function in this module has to ask what
//! a keystroke means or whether a Sync Ratio is bad, the decision belongs in
//! [`crate::app`] or in `seele-core` and has leaked.
//!
//! # Width is measured, never counted
//!
//! `specs/05-cliente-tui.md` is explicit, and blunt about the consequence:
//! "Kanji ocupa duas células — calcular largura com `unicode-width`, nunca com
//! `.len()`. Isso vai quebrar o layout se esquecido." Every truncation and
//! every pad in this file goes through [`width`] or [`truncate`]. `.len()` on a
//! display string is a bug even when the string happens to be ASCII, because
//! the next string through that code path will not be.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use seele_core::search::{occurrences, Search};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Bar, ChatLine, Mode, Node, Panel, Screen};
use crate::theme::Theme;

/// The smallest terminal `specs/05-cliente-tui.md` supports.
pub const MIN_WIDTH: u16 = 80;
/// The smallest terminal height `specs/05-cliente-tui.md` supports.
pub const MIN_HEIGHT: u16 = 24;

/// The tallest the alert band is ever allowed to get, in rows.
///
/// Four rows is one more than the tallest notice this client generates — an
/// invite verdict is a sentence and two fingerprints — and the fourth row is
/// there so the `[enter]` hint of a blocking alert always has somewhere to go.
///
/// The cap is a bound on *remote* input, not a layout preference.
/// `Alert.text` is `notice.operator_text` straight off the wire
/// (`crate::view`), and the only thing between a server and this band is
/// `seele_proto::control::MAX_ALERT_TEXT_LEN` — 512 bytes, with no filter on
/// newlines or control characters anywhere along the way. Forty short
/// `\n`-separated lines is a legal notice, and a band that sized itself to it
/// would take eighteen of the twenty-two rows a minimum terminal has, leave
/// MENSAGENS one line of content, and push the `[enter]` hint off the bottom —
/// turning the one alert `specs/08-seguranca.md` says must be impossible to
/// miss into one with no visible way out. An absolute cap is what makes the
/// band's height a property of this client instead of a property of whoever is
/// on the other end.
pub const MAX_ALERT_ROWS: usize = 4;

/// A marca em texto puro: o nó cheio, o enlace, o nó vazio.
///
/// `docs/marca.md` dá esta variação à TUI porque aqui não há SVG: o glifo tem
/// de ser feito de caracteres que qualquer terminal já tem. Substitui o
/// katakana e a assinatura do plug de entrada em toda a interface.
///
/// **Nove bytes, três caracteres, três células.** As três são de largura
/// ambígua no leste asiático (`■` U+25A0, `—` U+2014, `□` U+25A1): a medida
/// que vale aqui é a que [`width`] dá, porque é a mesma que o ratatui usa para
/// posicionar. Um terminal configurado para desenhar ambíguo em célula dupla
/// vai render seis, e é por isso que a marca só entra em títulos e cabeçalhos
/// — nunca numa linha cujo alinhamento outra coluna dependa.
pub const MARCA: &str = "■—□";

/// A assinatura, quando cabe: marca mais o wordmark. **Quinze bytes, nove
/// caracteres, nove células** pela mesma medida de [`MARCA`].
pub const ASSINATURA: &str = "■—□ SEELE";

/// Display width of a string, in terminal cells.
///
/// The only correct way to ask. See the module docs.
#[must_use]
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncates to a cell budget, marking the cut with `…`.
///
/// Cuts on character boundaries and accounts for wide characters, so a kanji
/// never gets split into half a cell — which does not render as half a glyph,
/// it renders as a corrupted row for the rest of the line.
#[must_use]
pub fn truncate(text: &str, budget: usize) -> String {
    if width(text) <= budget {
        return text.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    // Leave a cell for the ellipsis, which is itself one cell wide.
    let allowance = budget - 1;
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let cell = UnicodeWidthStr::width(character.encode_utf8(&mut [0u8; 4]) as &str);
        if used + cell > allowance {
            break;
        }
        out.push(character);
        used += cell;
    }
    out.push('…');
    out
}

/// Pads to a cell budget on the right.
#[must_use]
pub fn pad(text: &str, budget: usize) -> String {
    let text = truncate(text, budget);
    let mut out = text.clone();
    for _ in width(&text)..budget {
        out.push(' ');
    }
    out
}

/// Wraps text to a cell budget, breaking between words where it can.
///
/// Falls back to breaking mid-word for anything longer than the budget, because
/// a URL that overflows the panel is worse than a URL split in two.
#[must_use]
pub fn wrap(text: &str, budget: usize) -> Vec<String> {
    if budget == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        if width(&current) + separator + width(word) <= budget {
            if separator == 1 {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }

        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }

        // A single word wider than the panel gets split by cells, not by bytes.
        let mut rest = word;
        while width(rest) > budget {
            let mut used = 0;
            let mut cut = 0;
            for (index, character) in rest.char_indices() {
                let cell = UnicodeWidthStr::width(character.encode_utf8(&mut [0u8; 4]) as &str);
                if used + cell > budget {
                    break;
                }
                used += cell;
                cut = index + character.len_utf8();
            }
            if cut == 0 {
                break;
            }
            lines.push(rest[..cut].to_string());
            rest = &rest[cut..];
        }
        current = rest.to_string();
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Draws the whole client.
pub fn render(frame: &mut Frame<'_>, app: &App, theme: Theme) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_cramped(frame, app, theme, area);
        return;
    }

    if matches!(app.screen, Screen::Boot) {
        render_boot(frame, app, theme, area);
        return;
    }

    // The session is over. Nothing on the normal layout is live any more, and
    // showing a frozen roster beside a dead connection reads as a hang rather
    // than an ending — so the reason takes the screen.
    if let Screen::Lost { reason } = &app.screen {
        render_lost(frame, app, theme, area, reason);
        return;
    }

    let outer = title_block(app, theme);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // The telemetry bar is permanent. specs/05-cliente-tui.md calls it "a
    // diferença de caráter em relação a um cliente de chat comum", so it gets
    // its row before the panels get theirs, not out of whatever is left.
    // The battery gets a row of its own rather than sharing the alert banner:
    // specs/05-cliente-tui.md wants the countdown and the attempts both visible,
    // and an alert can arrive while the link is down.
    let battery_rows = u16::from(matches!(app.screen, Screen::InternalBattery { .. }));
    // The alert asks for as many rows as its text needs at this width, and the
    // rows are laid out here because the layout is what decides how tall the
    // band is. A one-row band was fine while every alert was a sentence; the
    // invite verdicts are a sentence *and two sixty-four-character
    // fingerprints*, and at the 80 columns `specs/05-cliente-tui.md` supports a
    // single row shows the first of the two and none of the second — which is
    // the one shape in which a comparison cannot be made.
    // Bounded at the source by `MAX_ALERT_ROWS`, so what comes back is at most
    // four rows however much text a server sent.
    let alert = alert_rows(app, theme, inner.width);
    // Never at the cost of the panels: three rows of session and one of
    // telemetry come first, and an alert taller than what is left gets cut
    // instead of pushing the conversation off the screen. With the cap in place
    // this can no longer bind — at the 24 rows `specs/05-cliente-tui.md`
    // supports there are seventeen to spare — and it stays as the guarantee
    // that the arithmetic, not the alert, is what the panels answer to.
    let ceiling = inner.height.saturating_sub(battery_rows + 4);
    let alert_height = u16::try_from(alert.len()).unwrap_or(u16::MAX).min(ceiling);
    let [battery, banner, panels, bar] = Layout::vertical([
        Constraint::Length(battery_rows),
        Constraint::Length(alert_height),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);

    if let Screen::InternalBattery {
        remaining,
        attempts,
    } = app.screen
    {
        render_battery(frame, theme, battery, remaining, attempts);
    }
    if alert_height > 0 {
        frame.render_widget(
            Paragraph::new(
                alert
                    .into_iter()
                    .take(alert_height as usize)
                    .collect::<Vec<_>>(),
            ),
            banner,
        );
    }

    let [dogma, tree, messages] = Layout::horizontal([
        Constraint::Length(18),
        Constraint::Length(24),
        Constraint::Min(20),
    ])
    .areas(panels);

    render_dogma(frame, app, theme, dogma);
    render_tree(frame, app, theme, tree);
    render_messages(frame, app, theme, messages);
    render_bar(frame, app, theme, bar);

    if app.help {
        render_help(frame, theme, area);
    } else if app.convite_visivel {
        if let Some(convite) = &app.convite {
            render_convite(frame, theme, area, convite, app.alcance.as_deref());
        }
    }
}

/// O link de convite, numa caixa larga o bastante para ele caber inteiro.
///
/// Largura inteira de propósito. O painel de mensagens tem cinquenta colunas e
/// o link tem uns noventa; ali ele quebraria em duas linhas, e um link quebrado
/// é um link que ninguém copia — que é a única coisa que se faz com ele.
fn render_convite(
    frame: &mut Frame<'_>,
    theme: Theme,
    area: Rect,
    convite: &str,
    alcance: Option<&str>,
) {
    let largura = area.width.saturating_sub(4).max(20);
    // A caixa cresce com a frase de alcance em vez de cortá-la. Um aviso de
    // "este link só funciona na sua rede" que não cabe na caixa é um aviso que
    // não foi dado — e é justamente o que o ADR 0022 manda não deixar acontecer.
    let alcance_linhas: Vec<String> = alcance
        .into_iter()
        .flat_map(|frase| frase.split('\n'))
        .flat_map(|linha| wrap(linha, largura.saturating_sub(2).max(20) as usize))
        .collect();
    let altura = 9 + u16::try_from(alcance_linhas.len()).unwrap_or(0) + 1;
    let caixa = centred(area, largura, altura);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border(true))
        .title(Span::styled(" CONVITE ", theme.accent()));
    let inner = block.inner(caixa);

    frame.render_widget(Clear, caixa);
    frame.render_widget(block, caixa);

    let budget = inner.width as usize;
    let mut lines = vec![
        Line::from(Span::styled(
            "Mande este link para quem você quer no servidor:",
            theme.label(),
        )),
        Line::from(""),
    ];
    for pedaco in wrap(convite, budget) {
        lines.push(Line::from(Span::styled(pedaco, theme.accent())));
    }
    if !alcance_linhas.is_empty() {
        lines.push(Line::from(""));
        for linha in &alcance_linhas {
            lines.push(Line::from(Span::styled(linha.clone(), theme.label())));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Vale para quem chegar primeiro. O servidor acaba quando você sair.",
        theme.label(),
    )));
    lines.push(Line::from(Span::styled(
        "`:convite` mostra de novo · qualquer tecla fecha",
        theme.label(),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The outer frame: `■—□ SEELE ─ 12:04:33`.
fn title_block(app: &App, theme: Theme) -> Block<'static> {
    let mut spans = vec![
        Span::styled(format!(" {ASSINATURA} "), theme.accent()),
        Span::styled("─ ", theme.fg(crate::theme::RULE)),
    ];

    spans.push(Span::styled(format!("{} ", app.clock), theme.body()));

    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme.border(false))
        .title(Line::from(spans))
}

fn panel(title: &str, focused: bool, theme: Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border(focused))
        .title(Span::styled(
            format!(" {title} "),
            if focused {
                theme.label().add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
}

fn render_dogma(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let block = panel("SERVIDOR", app.focus == Panel::Dogma, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let budget = inner.width as usize;
    let lines: Vec<Line<'_>> = app
        .dogmas
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let selected = index == app.selected_dogma;
            let marker = if selected { "▸ " } else { "  " };
            let style = if selected {
                theme.accent()
            } else {
                theme.body()
            };
            Line::from(Span::styled(
                truncate(&format!("{marker}{name}"), budget),
                style,
            ))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).style(dim(theme, app)), inner);
}

fn render_tree(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let block = panel("SALAS / CANAIS", app.focus == Panel::Channels, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let budget = inner.width as usize;
    let mut lines = Vec::new();

    for (index, node) in app.tree.iter().enumerate() {
        let selected = index == app.selected && app.focus == Panel::Channels;
        lines.push(match node {
            Node::Cage { name, open, sync } => {
                cage_line(name, *open, *sync, selected, budget, theme)
            }
            Node::Line { name } => Line::from(Span::styled(
                truncate(&format!("─ CANAL {name}"), budget),
                if selected {
                    theme.accent()
                } else {
                    theme.body()
                },
            )),
            Node::Pilot(pilot) => pilot_line(pilot, budget, theme),
        });
    }

    frame.render_widget(Paragraph::new(lines).style(dim(theme, app)), inner);
}

/// One Cage row: the name, and the room's average Sync Ratio when it has one.
///
/// The comp labels this **MÉDIA DO CAGE**. There is no room for the label in a
/// panel this narrow, so what identifies it is its place: the same column as
/// every pilot's number, on the row the pilots hang under. An empty Cage prints
/// nothing rather than a zero — see [`seele_core::Room::cage_sync`].
fn cage_line(
    name: &str,
    open: bool,
    sync: Option<seele_core::CageSync>,
    selected: bool,
    budget: usize,
    theme: Theme,
) -> Line<'static> {
    let arrow = if open { "▼ " } else { "▶ " };
    let style = if selected {
        theme.accent()
    } else {
        theme.body()
    };

    let Some(sync) = sync else {
        return Line::from(Span::styled(
            truncate(&format!("{arrow}{name}"), budget),
            style,
        ));
    };

    // Mark and number, as everywhere else: specs/05-cliente-tui.md forbids
    // carrying this by colour alone, and an average is no exception.
    let right = format!("{}{:>3}%", Theme::sync_mark(sync.band), sync.ratio);
    let left_budget = budget.saturating_sub(width(&right) + 1);
    let left = format!("{arrow}{}", truncate(name, left_budget));
    let gap = budget.saturating_sub(width(&left) + width(&right));

    Line::from(vec![
        Span::styled(left, style),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, theme.sync(sync.band)),
    ])
}

/// One pilot row: presence, name, and the Sync Ratio with its mark.
///
/// `specs/05-cliente-tui.md`: "Nenhuma informação transmitida **só** por cor: a
/// Taxa de Sincronização é sempre acompanhada do número". So the number is
/// always printed, the band mark is always printed, and colour is the third
/// channel rather than the only one.
fn pilot_line(pilot: &crate::app::RosterEntry, budget: usize, theme: Theme) -> Line<'static> {
    let presence = if pilot.speaking { "●" } else { "○" };
    // Mudo e isolamento total ganham texto, e não só uma cor, pela mesma
    // razão: esta interface é usada por quem não separa vermelho de verde.
    let flag = if pilot.at_field {
        "MUDO"
    } else if pilot.total_isolation {
        "SURDO"
    } else {
        ""
    };

    let sync = format!("{}{:>3}%", Theme::sync_mark(pilot.band()), pilot.sync);
    let right = if flag.is_empty() {
        sync.clone()
    } else {
        format!("{flag} {sync}")
    };

    let left_budget = budget.saturating_sub(width(&right) + 3);
    let left = format!("  {presence} {}", truncate(&pilot.nickname, left_budget));
    let gap = budget.saturating_sub(width(&left) + width(&right));

    Line::from(vec![
        Span::styled(
            left,
            if pilot.speaking {
                theme.body().add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, theme.sync(pilot.band())),
    ])
}

fn render_messages(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let block = panel("MENSAGENS", app.focus == Panel::Messages, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The composition line is part of the panel, not floating over it.
    let [history, compose] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let budget = history.width as usize;

    let (mut lines, current_line) = history_lines(app, budget, theme);

    let visible = history.height as usize;
    let skip = scroll(lines.len(), visible, current_line, app.busca.is_some());

    frame.render_widget(
        Paragraph::new(lines.split_off(skip)).style(dim(theme, app)),
        history,
    );
    frame.render_widget(compose_line(app, theme, budget), compose);
}

/// The whole history as drawn lines, and which line the current match fell on.
///
/// Shared by the full layout and the cramped one so a search behaves the same
/// in both: a term that only lights up on a wide terminal is a term somebody on
/// a narrow one cannot find.
fn history_lines(app: &App, budget: usize, theme: Theme) -> (Vec<Line<'static>>, Option<usize>) {
    let current = app.busca.as_ref().and_then(Search::current);
    let ordinal = app.busca.as_ref().and_then(Search::ordinal_in_message);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_line = None;
    for (index, message) in app.messages.iter().chain(&app.local).enumerate() {
        let here = current.filter(|candidate| candidate.message == index);
        if here.is_some() {
            // Recorded before the message is laid out, so it is the row the
            // message starts on rather than wherever the last one ended.
            current_line = Some(lines.len());
        }
        lines.extend(message_lines(
            message,
            budget,
            theme,
            &app.termo,
            here.and(ordinal),
        ));
    }
    (lines, current_line)
}

/// Which line the history starts drawing at.
///
/// Without a search this shows the tail: a chat that scrolls off the top is
/// normal, a chat that shows the first thing anybody ever said is broken. With
/// a search the tail stops being what matters and the occurrence does — an
/// occurrence off screen that nobody scrolls to is an occurrence that was not
/// found. Never past the tail either way, because scrolling below the last line
/// trades conversation for blank rows.
fn scroll(total: usize, visible: usize, current_line: Option<usize>, searching: bool) -> usize {
    let tail = total.saturating_sub(visible);
    match current_line {
        Some(line) if searching => line.saturating_sub(visible / 2).min(tail),
        _ => tail,
    }
}

/// One message: its header, then its body wrapped and lit where the term hits.
///
/// `current` is the ordinal *within this message* of the occurrence the cursor
/// is on, or `None` when the cursor is in some other message.
fn message_lines(
    message: &ChatLine,
    budget: usize,
    theme: Theme,
    term: &str,
    current: Option<usize>,
) -> Vec<Line<'static>> {
    let header = format!("{} {}", message.at, message.author);
    let mut lines = vec![Line::from(Span::styled(
        truncate(&header, budget),
        if message.own {
            theme.accent()
        } else {
            theme.label()
        },
    ))];

    // Counts occurrences in order, exactly the way the core counts them, so the
    // k-th hit here is the k-th hit there. Both passes go left to right over the
    // same collapsed text, so the ordinals line up.
    let mut seen = 0usize;
    for wrapped in wrap(&message.body, budget.saturating_sub(2)) {
        lines.push(Line::from(highlight(
            &wrapped, term, theme, current, &mut seen,
        )));
    }
    lines
}

/// An already-wrapped segment, split into lit and unlit spans.
///
/// The highlight is applied per segment, and not by offset into the whole body,
/// because [`wrap`] collapses whitespace with `split_whitespace` and an offset
/// computed on the raw body would point at the wrong character after a double
/// space.
///
/// # A match across a wrap boundary is missed, and the ordinals then drift
///
/// This is the price of the per-segment scan, and it is a real one. The core
/// searches the whole body; this searches one wrapped segment at a time. A
/// match split by the line break belongs to neither segment, so nothing lights
/// up for it — and because `seen` never counts it, every later occurrence in
/// that message is off by one and `REVERSED` can land on the wrong neighbour.
/// Two ordinary cases reach it: a term with a space in it (`"sync caiu"`) that
/// falls on the break, and a word wider than the panel, which [`wrap`] splits
/// mid-word.
///
/// The counter in the compose line comes from the core and stays correct
/// regardless, so the half of `specs/05-cliente-tui.md:144` that is not colour
/// survives this. Closing it properly needs either the offset mapping this
/// design exists to avoid, or lighting cells rather than spans.
///
/// The indent span keeps the width budget intact: the segment was wrapped to
/// `budget - 2`, and these two cells are the other two.
fn highlight(
    segment: &str,
    term: &str,
    theme: Theme,
    current: Option<usize>,
    seen: &mut usize,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("  ".to_owned(), theme.body())];
    if term.trim().is_empty() {
        spans.push(Span::styled(segment.to_owned(), theme.body()));
        return spans;
    }

    let characters: Vec<char> = segment.chars().collect();
    let mut cursor = 0usize;
    for (start, end) in occurrences(segment, term) {
        let ordinal = *seen;
        *seen += 1;
        // Overlapping hits ("aa" twice in "aaa") are counted, because the core
        // counts them and the ordinals have to agree, but drawn once: those
        // cells are already lit, and re-emitting them would duplicate text.
        // Skipping is also what keeps `start - cursor` below from going
        // negative, which in a debug build is a panic and not a wrong colour.
        // The cost: when the cursor sits on the overlapped hit, nothing on
        // screen is emphasised while the counter still reads `[2/2]`. The
        // counter is the half that has to be right, and it is.
        if start < cursor {
            continue;
        }
        let before: String = characters
            .iter()
            .skip(cursor)
            .take(start - cursor)
            .collect();
        if !before.is_empty() {
            spans.push(Span::styled(before, theme.body()));
        }
        let lit: String = characters.iter().skip(start).take(end - start).collect();
        // Both states are visible without colour — bold against inverse — so a
        // 16-colour terminal and `NO_COLOR` still separate "a hit" from "the
        // hit you are on". The counter in the compose line is the other half.
        spans.push(Span::styled(
            lit,
            if current == Some(ordinal) {
                theme.accent().add_modifier(Modifier::REVERSED)
            } else {
                theme.accent()
            },
        ));
        cursor = end;
    }
    let rest: String = characters.iter().skip(cursor).collect();
    if !rest.is_empty() {
        spans.push(Span::styled(rest, theme.body()));
    }
    spans
}

/// The counter that goes beside a search, `  [1/3]`, or nothing without one.
///
/// `specs/05-cliente-tui.md:144` forbids information carried by colour alone.
/// "Which of the three occurrences am I on" is exactly such information, and
/// this is the half of it that survives `NO_COLOR` and sixteen colours by SSH.
/// It is not decoration.
fn search_counter(app: &App) -> Option<String> {
    app.busca.as_ref().map(|search| {
        let (position, total) = search.position();
        format!("  [{position}/{total}]")
    })
}

fn compose_line(app: &App, theme: Theme, budget: usize) -> Paragraph<'static> {
    let (prefix, style) = match app.mode {
        Mode::Insert => ("▸ ", theme.body()),
        Mode::Command => (": ", theme.accent()),
        Mode::Search => ("/ ", theme.accent()),
        Mode::Normal => ("▸ ", theme.label()),
    };

    let counter = search_counter(app);
    // The counter is reserved rather than appended: this panel is about thirty
    // cells wide at 80 columns, so a long term would otherwise push the counter
    // off the edge, and the counter is the part that may not be lost.
    let reserved = counter.as_deref().map_or(0, width);

    let shown = if app.mode == Mode::Normal {
        String::new()
    } else {
        // Keep the caret visible in a long line by showing the tail.
        let room = budget.saturating_sub(width(prefix) + 1 + reserved);
        let text = &app.input;
        if width(text) <= room {
            text.clone()
        } else {
            let mut start = 0;
            for (index, _) in text.char_indices() {
                if width(&text[index..]) <= room {
                    start = index;
                    break;
                }
            }
            text[start..].to_string()
        }
    };

    let mut spans = vec![
        Span::styled(prefix, style),
        Span::styled(shown, theme.body()),
        Span::styled("_", theme.accent()),
    ];
    if let Some(counter) = counter {
        spans.push(Span::styled(counter, theme.label()));
    }

    Paragraph::new(Line::from(spans))
}

/// The permanent telemetry bar.
///
/// `SINAL 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ MUDO OFF`
fn render_bar(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let mut spans = vec![Span::styled(
        format!(" {} ", app.mode.label()),
        theme.accent(),
    )];

    let separator = Span::styled("│ ", theme.fg(crate::theme::RULE));
    for (index, segment) in bar_segments(&app.bar, app.at_field, app.total_isolation)
        .into_iter()
        .enumerate()
    {
        spans.push(separator.clone());
        // The Sync Ratio is the one segment that changes colour, because it is
        // the one the pilot is meant to react to.
        let style = if index == 0 {
            theme.sync(seele_core::SyncBand::of(app.bar.sync))
        } else {
            theme.body()
        };
        spans.push(Span::styled(format!("{segment} "), style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The bar's text, without styling. Pulled out so its width can be tested.
#[must_use]
pub fn bar_segments(bar: &Bar, at_field: bool, total_isolation: bool) -> Vec<String> {
    let mut segments = vec![
        format!(
            "SINAL {}{:>3}%",
            Theme::sync_mark(seele_core::SyncBand::of(bar.sync)),
            bar.sync
        ),
        format!("RTT {:.0}ms", bar.rtt_ms),
        format!("JIT {:.0}ms", bar.jitter_ms),
        format!("LOSS {:.1}%", bar.loss * 100.0),
        format!("OPUS {}k", bar.bitrate / 1000),
        format!("MUDO {}", if at_field { "ON" } else { "OFF" }),
    ];
    if total_isolation {
        segments.push("SURDO".to_string());
    }
    segments
}

/// Boot: o enlace sendo estabelecido.
///
/// This lasts exactly as long as connecting takes. `specs/05-cliente-tui.md`:
/// "se conectar em 200 ms, não inventar espera artificial. Animação decorativa
/// que atrasa o usuário é falha de design." So nothing here sleeps or animates
/// on a timer — the rows reflect state that actually changed.
fn render_boot(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let block = title_block(app, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(format!("  {ASSINATURA}"), theme.accent())),
        Line::from(""),
        Line::from(Span::styled("  estabelecendo enlace…", theme.label())),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Below 80×24: one panel and an honest warning.
///
/// `specs/05-cliente-tui.md` asks to "degradar para painel único com aviso".
/// The messages are what survives, because a client that cannot show the
/// conversation is not degraded, it is broken.
fn render_cramped(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let [warning, history] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

    // There is no compose line down here to hang the counter off, and a
    // highlight without its counter would be colour on its own — so it shares
    // the warning row, which is the only chrome this layout has.
    //
    // Reserved out of the warning's budget rather than appended for ratatui to
    // clip, exactly as `compose_line` does it: the warning alone is 22 cells,
    // so below about 29 columns an appended counter would fall off the edge and
    // leave the highlight carrying the position on its own, which is the
    // `specs/05-cliente-tui.md:144` failure this row exists to prevent. The
    // warning is the one that gives ground, because its own text is the thing
    // the terminal's size already says.
    let counter = search_counter(app);
    let reserved = counter.as_deref().map_or(0, width);
    let mut spans = vec![Span::styled(
        truncate(
            &format!("TERMINAL {}×{} < 80×24", area.width, area.height),
            (area.width as usize).saturating_sub(reserved),
        ),
        theme.alert(),
    )];
    if let Some(counter) = counter {
        spans.push(Span::styled(counter, theme.label()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), warning);

    let budget = history.width as usize;
    let (mut lines, current_line) = history_lines(app, budget, theme);
    let visible = history.height as usize;
    let skip = scroll(lines.len(), visible, current_line, app.busca.is_some());

    frame.render_widget(Paragraph::new(lines.split_off(skip)), history);
}

/// The internal battery: how long is left, and what is being tried.
///
/// `specs/05-cliente-tui.md` asks for "contagem 04:59 regressiva, interface
/// esmaecida mas legível, tentativas listadas". The countdown is the part that
/// tells somebody whether to wait or to go and fix their wifi, so it is a
/// number and not a spinner.
fn render_battery(frame: &mut Frame<'_>, theme: Theme, area: Rect, remaining: u64, attempts: u32) {
    let mut spans = Vec::new();
    spans.push(Span::styled("BATERIA INTERNA ", theme.alert()));
    spans.push(Span::styled(
        format!("{:02}:{:02} ", remaining / 60, remaining % 60),
        theme.alert().add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(
            "· {attempts} tentativa{}",
            if attempts == 1 { "" } else { "s" }
        ),
        theme.label(),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The end of a session, with the reason.
///
/// A client that closes without saying why turns every disconnection into a
/// support question. `specs/02-protocolo.md` carries enumerated reasons for
/// exactly this moment; [`crate::text`] turns them into sentences.
///
/// The reason is wrapped **line by line**, and that is not tidiness. Some
/// reasons are written with newlines because the newline is the information:
/// the invite check prints the expected fingerprint over the offered one so the
/// two can be read against each other, and `docs/pendencias.md` #12 promises
/// exactly that. Handing the whole string to [`wrap`], which splits on
/// whitespace, reflowed two sixty-four-character hex strings into one greedy
/// paragraph — and comparing two unaligned hex strings is the one thing a human
/// cannot do, on the screen whose only job it is.
fn render_lost(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect, reason: &str) {
    let block = title_block(app, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let budget = inner.width as usize;
    let mut lines = vec![
        Line::from(Span::styled("  ENLACE ENCERRADO", theme.alert())),
        Line::from(""),
    ];
    for paragraph in reason.split('\n') {
        // A blank line in the reason is a blank line on screen: it is what
        // separates the sentence from the two values it is about.
        if paragraph.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        for wrapped in wrap(paragraph, budget.saturating_sub(2)) {
            lines.push(Line::from(Span::styled(
                format!("  {wrapped}"),
                theme.body(),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  qualquer tecla para sair",
        theme.label(),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The band's last row, carrying `…` when there was something after it.
///
/// Separate from the band because it is asked twice: once to measure the row
/// the `[enter]` hint wants to share, and once to write it. Measuring the
/// unmarked row and writing the marked one is how a hint gets promised a seat
/// that the mark had already taken.
fn marked(row: &str, cut: bool, budget: usize) -> String {
    if !cut {
        return row.to_string();
    }
    let base = row.trim_end();
    let with_mark = if base.is_empty() {
        "…".to_string()
    } else {
        format!("{base} …")
    };
    truncate(&with_mark, budget)
}

/// Lays the alert out into screen rows, one [`Line`] each.
///
/// Returns empty when there is no alert, which is what makes the band take no
/// rows at all rather than an empty one.
///
/// The band grew a second and a third row for one reason: `ADR 0006`'s two
/// invite verdicts carry two sixty-four-character fingerprints, and the whole
/// point of showing them is that they be read against each other. On one row at
/// 80 columns the sentence plus `esperada:` already spends the budget, so the
/// offered fingerprint — the half that says what the Dogma actually is — was
/// never on the screen. `render_lost` reached the same conclusion for the
/// refusal, and the app's band carries `white-space: pre-line`; this is the
/// same rule in the third shell.
///
/// `\n` in the text is a row break, exactly as in [`render_lost`], and anything
/// longer than the width wraps on whitespace — so a fingerprint, being one
/// unbroken word of 64 cells, always lands whole on a row of its own.
///
/// And it stops at [`MAX_ALERT_ROWS`], because the text is not ours.
fn alert_rows(app: &App, theme: Theme, width: u16) -> Vec<Line<'static>> {
    let Some(alert) = &app.alert else {
        return Vec::new();
    };
    let budget = width as usize;

    let text = alert.text.clone();

    let mut rows: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            rows.push(String::new());
            continue;
        }
        rows.extend(wrap(paragraph, budget));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }

    // Every row this band drops has to be accounted for before anything is
    // marked, because there are two ways to lose one and they compound. The
    // cap is the obvious one. The other is the `[enter]` row: at the cap, a
    // last row too wide to share with the hint pays for the hint's row with
    // itself. Marking after the first and before the second is what put a `…`
    // on a row that was then deleted — the band came back reading like the
    // whole notice, twenty of sixty-four words and a way out.
    let mut cut = rows.len() > MAX_ALERT_ROWS;
    rows.truncate(MAX_ALERT_ROWS);

    // Does the way out fit beside the text, or does it need a row? Asked
    // against the marked row, because the mark is two cells the hint no longer
    // has. `specs/08-seguranca.md` wants the blocking alert impossible to
    // ignore, and a hint clipped off the right edge is ignorable.
    let mark = "  [enter]";
    let mut hint_of_its_own = false;
    if alert.blocking {
        let last = rows
            .last()
            .map_or(0, |row| self::width(&marked(row, cut, budget)));
        if last + self::width(mark) > budget {
            hint_of_its_own = true;
            // The text can be cut, the way out cannot. Dropping a row of a
            // notice already at the cap is the whole reason `MAX_ALERT_ROWS`
            // is one taller than anything this client writes — and the row it
            // drops is a cut like any other, so it is marked like one. Below
            // the cap the hint's row is free and nothing is lost.
            if rows.len() >= MAX_ALERT_ROWS {
                rows.truncate(MAX_ALERT_ROWS - 1);
                cut = true;
            }
        }
    }

    // The cut is marked. A band that silently drops the tail of a notice reads
    // as the whole notice, and the reader has no way to tell that a server sent
    // more than this — `…` is the difference between short and truncated.
    if let Some(last) = rows.last_mut() {
        *last = marked(last, cut, budget);
    }

    let mut lines: Vec<Line<'static>> = rows
        .into_iter()
        .map(|row| Line::from(Span::styled(row, theme.alert())))
        .collect();

    if alert.blocking {
        if hint_of_its_own {
            lines.push(Line::from(Span::styled(mark, theme.label())));
        } else if let Some(line) = lines.last_mut() {
            line.spans.push(Span::styled(mark, theme.label()));
        }
    }

    debug_assert!(lines.len() <= MAX_ALERT_ROWS);
    lines
}

/// The help overlay. `specs/09-roadmap.md` accepts M4 on somebody outside the
/// project connecting and talking with only `?`, so this is a deliverable and
/// not a courtesy.
fn render_help(frame: &mut Frame<'_>, theme: Theme, area: Rect) {
    let rows = [
        ("h j k l / setas", "navegar"),
        ("Tab / Shift+Tab", "alternar painel"),
        ("Enter", "entrar na sala / abrir canal"),
        ("s", "sair da sala de voz"),
        ("i", "escrever mensagem"),
        ("Espaço (segurar)", "falar"),
        ("m", "mudo (microfone fechado)"),
        ("d", "isolamento total (surdo)"),
        ("g / G", "topo / fim"),
        ("/", "buscar no histórico"),
        ("n / N", "ocorrência seguinte / anterior"),
        ("?", "esta ajuda"),
        (":conectar <host>", "conectar a um servidor"),
        (":cage <nome>", "entrar numa sala de voz"),
        (":sync", "diagnóstico detalhado"),
        (":audio", "dispositivos"),
        (":ejetar", "sair do servidor e escolher outro"),
        (":q", "sair do programa"),
    ];

    let height = rows.len() as u16 + 2;
    let inner_width = 54u16;
    let box_area = centred(area, inner_width, height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border(true))
        .title(Span::styled(" AJUDA ", theme.accent()));
    let inner = block.inner(box_area);

    frame.render_widget(Clear, box_area);
    frame.render_widget(block, box_area);

    let lines: Vec<Line<'_>> = rows
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(pad(key, 18), theme.accent()),
                Span::styled((*what).to_string(), theme.body()),
            ])
        })
        .collect();

    // Deliberately not wrapped: a description that spilled onto a second line
    // would push `:q` off the bottom of the box, and `:q` is how somebody
    // leaves. Clipping a description is the cheaper failure.
    frame.render_widget(Paragraph::new(lines), inner);
}

fn centred(area: Rect, want_width: u16, want_height: u16) -> Rect {
    let width = want_width.min(area.width);
    let height = want_height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// The dim style used while the internal battery runs down.
///
/// `specs/07-tema-evangelion.md` wants the interface "esmaecida mas ainda
/// legível — o histórico continua ali para leitura". Dim, not hidden.
fn dim(theme: Theme, app: &App) -> Style {
    if app.dimmed() {
        theme.label().add_modifier(Modifier::DIM)
    } else {
        Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Alert, ChatLine, RosterEntry};
    use crate::theme::Palette;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(app: &App, palette: Palette, size: (u16, u16)) -> String {
        let backend = TestBackend::new(size.0, size.1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, app, Theme::with_palette(palette)))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..size.1)
            .map(|y| {
                // A wide character occupies one cell and leaves the next one as
                // filler. Reading every cell would count kanji twice and make a
                // correct 80-cell row measure 88 — so skip what the glyph
                // already covers, and the row that comes back is what the
                // terminal actually shows.
                let mut row = String::new();
                let mut x = 0u16;
                while x < size.0 {
                    let symbol = buffer[(x, y)].symbol();
                    row.push_str(symbol);
                    x += u16::try_from(width(symbol).max(1)).unwrap_or(1);
                }
                row
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn populated() -> App {
        let mut app = App::new();
        app.screen = Screen::PatternBlue;
        app.clock = "12:04:33".into();
        app.dogmas = vec!["Terceira Tóquio".into(), "Geofront".into()];
        app.tree = vec![
            Node::Cage {
                name: "CAGE-01 CENTRAL".into(),
                open: true,
                sync: None,
            },
            Node::Pilot(RosterEntry {
                nickname: "ayanami".into(),
                sync: 98,
                speaking: true,
                at_field: false,
                total_isolation: false,
            }),
            Node::Pilot(RosterEntry {
                nickname: "asuka".into(),
                sync: 44,
                speaking: false,
                at_field: true,
                total_isolation: false,
            }),
            Node::Line {
                name: "#geral".into(),
            },
        ];
        app.messages = vec![ChatLine {
            at: "12:01".into(),
            author: "ayanami".into(),
            body: "verificando harmônicos".into(),
            own: false,
        }];
        app.bar = Bar {
            sync: 94,
            rtt_ms: 38.0,
            jitter_ms: 12.0,
            loss: 0.002,
            bitrate: 32_000,
        };
        app
    }

    /// The alert band's own rows, cut out of the screen.
    ///
    /// `screen.contains('…')` is not an assertion about the band: `populated()`
    /// draws `▸ Terceira Tóqu…` in DOGMA on every single frame, so the whole
    /// screen always has an ellipsis in it and the check passes with a band
    /// that marked nothing. The band lives between the frame's top border and
    /// the first row of the panels, and that is the only region worth asking.
    fn band_rows(screen: &str) -> Vec<&str> {
        screen
            .lines()
            .skip(1)
            .take_while(|row| !row.contains('┌'))
            .collect()
    }

    #[test]
    fn a_cages_row_carries_the_average_of_the_room() {
        // MÉDIA DO CAGE, in the same column as every pilot's number. The mark
        // travels with it: specs/05-cliente-tui.md forbids carrying a band by
        // colour alone, and an average is not an exception to that.
        let mut app = populated();
        app.tree[0] = Node::Cage {
            name: "CAGE-01 CENTRAL".into(),
            open: true,
            // 98 and 44, which is what the two pilots below it read.
            sync: Some(seele_core::CageSync {
                ratio: 71,
                band: seele_core::SyncBand::of(71),
                pilots: 2,
            }),
        };
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));

        let row = screen
            .lines()
            .find(|row| row.contains("CAGE-01"))
            .expect("the Cage row");
        assert!(row.contains("▒ 71%"), "no average on the Cage row: {row:?}");
        assert!(row.contains("CAGE-01"), "the name was eaten: {row:?}");
    }

    #[test]
    fn a_cage_nobody_is_in_shows_no_number_at_all() {
        // An empty Cage has no average — not a zero, which the bands would
        // paint red and which would read as a room in trouble.
        let screen = draw(&populated(), Palette::True, (MIN_WIDTH, MIN_HEIGHT));

        let row = screen
            .lines()
            .find(|row| row.contains("CAGE-01"))
            .expect("the Cage row");
        assert!(
            !row.contains('%'),
            "an empty Cage was given a number: {row:?}"
        );
    }

    #[test]
    fn width_counts_cells_and_not_bytes() {
        // The bug specs/05-cliente-tui.md warns about, pinned. 同期率 is nine
        // bytes, three characters, and six cells; only the last is a layout.
        assert_eq!("同期率".len(), 9);
        assert_eq!("同期率".chars().count(), 3);
        assert_eq!(width("同期率"), 6);
    }

    #[test]
    fn a_marca_em_texto_puro_mede_o_que_o_comentario_diz() {
        // Mesma armadilha do teste acima, agora na marca: `■`, `—` e `□` são
        // de largura ambígua, e o número que o layout usa é o de células, não
        // o de bytes. Medido aqui para que ninguém troque o glifo por um mais
        // largo sem ver a conta mudar.
        assert_eq!(MARCA.len(), 9);
        assert_eq!(MARCA.chars().count(), 3);
        assert_eq!(width(MARCA), 3);

        assert_eq!(ASSINATURA.len(), 15);
        assert_eq!(ASSINATURA.chars().count(), 9);
        assert_eq!(width(ASSINATURA), 9);
    }

    #[test]
    fn a_marca_abre_o_quadro_e_a_tela_de_conexao() {
        // A marca desenhada, não só a constante: o katakana e o plug de
        // entrada saíram, e o que abre o quadro agora é `■—□ SEELE`.
        let screen = draw(&populated(), Palette::True, (MIN_WIDTH, MIN_HEIGHT));
        assert!(screen.contains(ASSINATURA), "{screen}");
        assert!(!screen.contains("ゼーレ"), "{screen}");
        assert!(!screen.contains("ENTRY PLUG"), "{screen}");
    }

    #[test]
    fn truncation_never_splits_a_wide_character() {
        // Half a kanji is not half a glyph. It corrupts the rest of the row.
        for budget in 1..=12 {
            let cut = truncate("第3新東京市本部", budget);
            assert!(
                width(&cut) <= budget,
                "budget {budget} overflowed to {} cells: {cut:?}",
                width(&cut)
            );
        }
    }

    #[test]
    fn truncation_leaves_short_text_alone() {
        assert_eq!(truncate("ayanami", 20), "ayanami");
        assert_eq!(truncate("同期率", 6), "同期率");
    }

    #[test]
    fn padding_measures_in_cells() {
        assert_eq!(width(&pad("同期率", 10)), 10);
        assert_eq!(width(&pad("sync", 10)), 10);
    }

    #[test]
    fn wrapping_respects_the_budget_even_for_unbroken_words() {
        let long = "a".repeat(200);
        for line in wrap(&long, 24) {
            assert!(width(&line) <= 24);
        }

        let mixed = "第3新東京市 verificando harmônicos no núcleo do geofront agora";
        for line in wrap(mixed, 20) {
            assert!(width(&line) <= 20, "{line:?} is {} cells", width(&line));
        }
    }

    #[test]
    fn wrapping_keeps_every_word() {
        let text = "sync caiu aqui e o padrão voltou para laranja";
        let joined = wrap(text, 15).join(" ");
        assert_eq!(
            joined.split_whitespace().count(),
            text.split_whitespace().count()
        );
    }

    #[test]
    fn the_full_layout_draws_all_three_panels_and_the_bar() {
        let screen = draw(&populated(), Palette::True, (80, 24));

        assert!(screen.contains("SERVIDOR"), "{screen}");
        assert!(screen.contains("SALAS / CANAIS"), "{screen}");
        assert!(screen.contains("MENSAGENS"), "{screen}");
        assert!(screen.contains("SINAL"), "{screen}");
        assert!(screen.contains("RTT 38ms"), "{screen}");
    }

    #[test]
    fn nothing_overflows_the_minimum_terminal() {
        // Every row is exactly 80 cells or the terminal wrapped something.
        let screen = draw(&populated(), Palette::True, (80, 24));
        for line in screen.lines() {
            assert_eq!(width(line), 80, "row is {} cells: {line:?}", width(line));
        }
    }

    #[test]
    fn sixteen_colours_lose_no_information() {
        // specs/05-cliente-tui.md acceptance: "Funciona por SSH em terminal de
        // 16 cores sem perder informação." So the ANSI-16 screen must carry the
        // same facts as the truecolor one, since colour is what got taken away.
        let app = populated();
        let rich = draw(&app, Palette::True, (80, 24));
        let poor = draw(&app, Palette::Ansi16, (80, 24));
        assert_eq!(rich, poor, "16 colours changed which characters are drawn");
    }

    #[test]
    fn no_palette_draws_japanese_decoration_and_none_loses_a_number() {
        // O japonês decorativo saiu da tela em toda paleta — ele nunca disse
        // nada que a frase ao lado já não dissesse. O número, esse, é
        // informação, e é o que nenhuma paleta pode perder.
        let app = populated();
        let rica = draw(&app, Palette::True, (80, 24));
        assert!(!rica.contains("同期率"), "{rica}");
        assert!(!rica.contains("第3新東京市"), "{rica}");

        let mono = draw(&app, Palette::Mono, (80, 24));
        assert!(!mono.contains("同期率"), "{mono}");
        assert!(mono.contains("94"), "the Sync Ratio vanished:\n{mono}");
        assert!(
            mono.contains("98"),
            "a pilot's Sync Ratio vanished:\n{mono}"
        );
        assert!(mono.contains("MUDO"), "the mute marker vanished:\n{mono}");
    }

    #[test]
    fn a_sync_ratio_is_never_shown_by_colour_alone() {
        // specs/05-cliente-tui.md: "Nenhuma informação transmitida **só** por
        // cor". Mono is the honest test — whatever survives here is the part
        // that was never carried by hue.
        let mono = draw(&populated(), Palette::Mono, (80, 24));
        assert!(mono.contains('█') || mono.contains('▓'), "{mono}");
        assert!(mono.contains("44"), "{mono}");
    }

    #[test]
    fn below_the_minimum_it_degrades_to_one_panel_with_a_warning() {
        let screen = draw(&populated(), Palette::True, (60, 18));

        assert!(screen.contains("60×18"), "no warning:\n{screen}");
        assert!(screen.contains("harmônicos"), "history was lost:\n{screen}");
        assert!(
            !screen.contains("SALAS"),
            "three panels at 60 cells:\n{screen}"
        );
        for line in screen.lines() {
            assert!(width(line) <= 60);
        }
    }

    #[test]
    fn resizing_never_overflows_at_any_size() {
        // specs/05-cliente-tui.md acceptance: "Sem tremulação ao redimensionar."
        // Flicker is a frame-timing property, but a row that overflows its
        // width is the failure that looks like flicker and is testable here.
        let app = populated();
        for w in [40u16, 60, 79, 80, 100, 200] {
            for h in [10u16, 23, 24, 40] {
                let screen = draw(&app, Palette::True, (w, h));
                for line in screen.lines() {
                    assert!(
                        width(line) <= w as usize,
                        "{w}×{h} produced a {}-cell row",
                        width(line)
                    );
                }
            }
        }
    }

    #[test]
    fn the_boot_screen_says_the_link_is_being_established() {
        // As três luzes saíram: nenhuma delas media coisa nenhuma, e uma luz
        // que não mede é cenário se passando por instrumento. O que fica é a
        // única linha desta tela que diz o que está acontecendo.
        let mut app = populated();
        app.screen = Screen::Boot;
        let screen = draw(&app, Palette::True, (80, 24));

        assert!(screen.contains("estabelecendo enlace"), "{screen}");
        assert!(!screen.contains("MELCHIOR"), "{screen}");
        assert!(!screen.contains("BALTHASAR"), "{screen}");
        assert!(!screen.contains("CASPER"), "{screen}");
    }

    #[test]
    fn the_battery_counts_down_where_it_can_be_seen() {
        // specs/05-cliente-tui.md asks for the countdown and the attempts, not a
        // spinner: the number is what tells somebody whether to wait or to go
        // and fix their wifi.
        let mut app = populated();
        app.screen = Screen::InternalBattery {
            remaining: 287,
            attempts: 3,
        };
        let screen = draw(&app, Palette::True, (80, 24));

        assert!(screen.contains("04:47"), "no countdown:\n{screen}");
        assert!(screen.contains("3 tentativas"), "no attempts:\n{screen}");
        assert!(screen.contains("BATERIA INTERNA"), "{screen}");
    }

    #[test]
    fn one_attempt_is_not_reported_as_one_attempts() {
        let mut app = populated();
        app.screen = Screen::InternalBattery {
            remaining: 60,
            attempts: 1,
        };
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("1 tentativa "), "{screen}");
    }

    #[test]
    fn a_lost_session_says_why_instead_of_freezing_the_roster() {
        // A client that closes without a reason turns every disconnection into
        // a support question, and a frozen layout reads as a hang.
        let mut app = populated();
        app.screen = Screen::Lost {
            reason: "ACESSO BARRADO POR UM OPERADOR".into(),
        };
        let screen = draw(&app, Palette::True, (80, 24));

        assert!(
            screen.contains("ACESSO BARRADO POR UM OPERADOR"),
            "{screen}"
        );
        assert!(
            screen.contains("qualquer tecla"),
            "no way out shown:\n{screen}"
        );
        assert!(
            !screen.contains("SALAS / CANAIS"),
            "a dead session still shows a live roster:\n{screen}"
        );
    }

    #[test]
    fn the_reason_keeps_the_line_breaks_that_carry_its_meaning() {
        // `docs/pendencias.md` #12 promises the refused Dogma is shown "com a
        // esperada e a ofertada lado a lado". `wrap` splits on whitespace, so
        // handing it the whole reason reflowed two sixty-four-character hex
        // strings into one paragraph — the one shape in which they cannot be
        // compared, on the screen whose only job is comparing them.
        let mut app = populated();
        let expected = "a".repeat(64);
        let offered = "b".repeat(64);
        app.screen = Screen::Lost {
            reason: format!(
                "ESTE NÃO É O SERVIDOR DO CONVITE.\n\nesperada:  {expected}\nofertada:  {offered}"
            ),
        };
        let screen = draw(&app, Palette::True, (80, 24));

        // The rows carry the panel border, so this asks which row a label is on
        // and not what a row begins with.
        let row_with = |needle: &str| screen.lines().position(|line| line.contains(needle));
        let (Some(esperada), Some(ofertada)) = (row_with("esperada:"), row_with("ofertada:"))
        else {
            panic!("the reason lost one of the two fingerprints:\n{screen}");
        };
        assert_ne!(
            esperada, ofertada,
            "both fingerprints reflowed onto one row, which is the shape they cannot be \
             compared in:\n{screen}"
        );
        assert!(
            ofertada > esperada,
            "the offered fingerprint did not stay below the expected one:\n{screen}"
        );
        let esperada_line = screen.lines().nth(esperada);
        let ofertada_line = screen.lines().nth(ofertada);
        // If the fingerprint wrapped onto the next row, "aaa"/"bbb" is not on
        // the label's own row at all, and comparing two `None`s below would
        // pass without ever looking at either fingerprint.
        assert!(
            esperada_line.is_some_and(|line| line.find("aaa").is_some()),
            "the expected fingerprint did not stay on its own label's row:\n{screen}"
        );
        assert!(
            ofertada_line.is_some_and(|line| line.find("bbb").is_some()),
            "the offered fingerprint did not stay on its own label's row:\n{screen}"
        );
        // Same prefix, same width, same column: that is what "side by side"
        // means when the values themselves are unreadable strings.
        assert_eq!(
            esperada_line.and_then(|line| line.find("aaa")),
            ofertada_line.and_then(|line| line.find("bbb")),
            "the two fingerprints do not start in the same column:\n{screen}"
        );
    }

    #[test]
    fn the_battery_keeps_the_history_on_screen() {
        // specs/07-tema-evangelion.md: "o histórico continua ali para leitura".
        let mut app = populated();
        app.screen = Screen::InternalBattery {
            remaining: 287,
            attempts: 3,
        };
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("harmônicos"), "history vanished:\n{screen}");
    }

    #[test]
    fn a_blocking_alert_is_drawn_where_it_cannot_be_missed() {
        let mut app = populated();
        app.alert = Some(Alert {
            text: "A CHAVE DO SERVIDOR MUDOU".into(),
            blocking: true,
        });
        let screen = draw(&app, Palette::True, (80, 24));

        assert!(screen.contains("A CHAVE DO SERVIDOR MUDOU"), "{screen}");
        assert!(screen.contains("[enter]"), "no way out shown:\n{screen}");
    }

    #[test]
    fn an_invite_verdict_shows_both_fingerprints_whole_at_eighty_columns() {
        // The alert exists so the two values can be compared, and a comparison
        // needs both halves on the screen. On one row the offered fingerprint
        // never appeared at all: label plus the expected value already spend
        // the 80 columns `specs/05-cliente-tui.md` supports.
        let expected = "a".repeat(64);
        let offered = "b".repeat(64);
        let mut app = populated();
        app.alert = Some(Alert {
            text: format!(
                "O CONVITE NÃO CORRESPONDE A ESTE SERVIDOR.\nesperada:  {expected}\nofertada:  {offered}"
            ),
            blocking: false,
        });
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));

        // Whole, and on a row each — a fingerprint split across two rows is as
        // uncomparable as one that is missing.
        for fingerprint in [&expected, &offered] {
            assert!(
                screen.lines().any(|row| row.contains(fingerprint.as_str())),
                "a fingerprint was cut or missing:\n{screen}"
            );
        }
        assert!(screen.contains("O CONVITE NÃO CORRESPONDE"), "{screen}");
        // And the conversation is still there: the band takes rows, it does not
        // take the screen.
        assert!(
            screen.contains("harmônicos"),
            "the panels went away:\n{screen}"
        );
    }

    #[test]
    fn a_long_operator_notice_cannot_take_the_screen_from_the_panels() {
        // `Alert.text` is `notice.operator_text` off the wire, capped only at
        // 512 bytes and filtered for nothing. Forty short lines is a legal
        // notice, and a band that sized itself to the text gave it eighteen of
        // the twenty-two rows: MENSAGENS down to one line of content and the
        // `[enter]` hint of a *blocking* alert clipped off the bottom, which is
        // the one thing `specs/08-seguranca.md` says cannot be missed.
        let mut app = populated();
        app.alert = Some(Alert {
            text: (0..40)
                .map(|n| format!("linha{n:02}"))
                .collect::<Vec<_>>()
                .join("\n"),
            blocking: true,
        });
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));

        let banda = screen.lines().filter(|row| row.contains("linha")).count();
        assert!(
            banda <= MAX_ALERT_ROWS,
            "the band took {banda} rows of the {MAX_ALERT_ROWS} it is allowed:\n{screen}"
        );
        // The way out survives the cap. Enter and Esc dismiss either way; a
        // hint nobody can see is the same as no hint.
        assert!(
            screen.contains("[enter]"),
            "the way out was cut off the screen:\n{screen}"
        );
        // And the panels kept their rows: the roster, the tree and the history
        // are all still readable.
        for kept in ["harmônicos", "ayanami", "#geral"] {
            assert!(
                screen.contains(kept),
                "`{kept}` was pushed off by the alert:\n{screen}"
            );
        }
        // Cut, and saying so — asked of the band and not of the screen.
        assert!(
            band_rows(&screen).iter().any(|row| row.contains('…')),
            "the band dropped the tail of the notice without marking it:\n{screen}"
        );
    }

    #[test]
    fn a_blocking_notice_that_fills_the_band_keeps_its_mark() {
        // The blocking path used to undo the mark it had just written: the `…`
        // went onto row four, and then the hint — which no longer fitted on a
        // row that wide — took row four for itself by dropping it. What was
        // left was twenty of sixty-four words and a `[enter]`, reading exactly
        // like the whole notice. 512 bytes of unbroken prose is a legal notice:
        // `seele_proto::control::MAX_ALERT_TEXT_LEN` is the only filter there
        // is, and `crate::view` takes both the text and `blocking` off the wire.
        let mut app = populated();
        app.alert = Some(Alert {
            text: vec!["palavra"; 64].join(" "),
            blocking: true,
        });
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));
        let band = band_rows(&screen);

        assert!(
            band.iter().any(|row| row.contains('…')),
            "the band cut sixty-four words down to a bandful and said nothing:\n{screen}"
        );
        assert!(
            screen.contains("[enter]"),
            "the way out was cut off the screen:\n{screen}"
        );
        assert!(
            band.len() <= MAX_ALERT_ROWS,
            "the band took {} rows of the {MAX_ALERT_ROWS} it is allowed:\n{screen}",
            band.len()
        );
    }

    #[test]
    fn a_blocking_notice_of_exactly_four_rows_marks_the_row_the_hint_costs() {
        // The case that never got a marker at all: at exactly `MAX_ALERT_ROWS`
        // the cap does not fire, so nothing was marked — and then the hint,
        // needing a row of its own because row four is wide, deleted row four
        // outright. A whole row of the notice disappeared from a band that
        // still looked complete.
        let wide = "x".repeat(70);
        let mut app = populated();
        app.alert = Some(Alert {
            text: format!("primeira\nsegunda\nterceira\n{wide}"),
            blocking: true,
        });
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));
        let band = band_rows(&screen);

        assert!(
            band.iter().any(|row| row.contains('…')),
            "a row of the notice was dropped for the hint with nothing to say so:\n{screen}"
        );
        assert!(
            screen.contains("[enter]"),
            "the way out was cut off the screen:\n{screen}"
        );
    }

    #[test]
    fn a_one_line_alert_still_takes_a_single_row() {
        // The band grows with the text and no further: nothing that fits gets a
        // blank row underneath it.
        let mut app = populated();
        app.alert = Some(Alert {
            text: "A CHAVE DO SERVIDOR MUDOU".into(),
            blocking: false,
        });
        let screen = draw(&app, Palette::True, (MIN_WIDTH, MIN_HEIGHT));

        let rows: Vec<&str> = screen.lines().collect();
        let Some(index) = rows
            .iter()
            .position(|row| row.contains("A CHAVE DO SERVIDOR MUDOU"))
        else {
            panic!("the alert never made it to the screen:\n{screen}");
        };
        let Some(next) = rows.get(index + 1) else {
            panic!("the alert was the last row on the screen:\n{screen}");
        };
        assert!(
            next.contains('┌') || next.contains('│') || next.contains('─'),
            "a blank row under a one-line alert — the band grew for nothing:\n{screen}"
        );
    }

    #[test]
    fn the_help_overlay_lists_the_essential_keys() {
        // The M4 acceptance criterion is that an outsider gets by with `?`.
        let mut app = populated();
        app.help = true;
        let screen = draw(&app, Palette::True, (80, 24));

        for key in ["Tab", "Shift+Tab", "Enter", ":q", "?"] {
            assert!(screen.contains(key), "`{key}` missing from help:\n{screen}");
        }
        assert!(screen.contains("falar"), "{screen}");
    }

    #[test]
    fn the_help_overlay_names_the_keys_that_have_no_other_hint() {
        // Searching, leaving the voice room and ejecting are reachable and
        // announced nowhere else: `/`, `n`, `N`, `s` and `:ejetar` have no
        // button, no label and no prompt. A key that works and is written down
        // nowhere is as good as missing.
        let mut app = populated();
        app.help = true;
        let screen = draw(&app, Palette::True, (80, 24));

        for what in [
            "buscar no histórico",
            "ocorrência seguinte",
            ":ejetar",
            "sair da sala de voz",
            "sair do programa",
        ] {
            assert!(screen.contains(what), "`{what}` missing:\n{screen}");
        }
    }

    #[test]
    fn the_bar_shows_the_mode_it_is_in() {
        let mut app = populated();
        app.mode = Mode::Insert;
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("INSERÇÃO"), "{screen}");
    }

    #[test]
    fn typing_shows_up_in_the_compose_line() {
        let mut app = populated();
        app.mode = Mode::Command;
        app.input = "conectar localhost".into();
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("conectar localhost"), "{screen}");
    }

    /// A message with one body, so a test can say what it is searching.
    fn said(body: &str) -> ChatLine {
        ChatLine {
            at: "12:03".into(),
            author: "shinji".into(),
            body: body.into(),
            own: false,
        }
    }

    /// An app in Search mode with `term` already run over `bodies`.
    fn searching(bodies: &[&str], term: &str) -> App {
        let mut app = populated();
        app.messages = bodies.iter().map(|body| said(body)).collect();
        app.mode = Mode::Search;
        app.input = term.to_owned();
        app.termo = term.to_owned();
        app.refazer_busca();
        app
    }

    #[test]
    fn the_search_shows_the_counter_and_marks_the_current_line() {
        // specs/05-cliente-tui.md:144: nothing may be conveyed by colour alone.
        // The counter is the highlight's textual companion, and it is what
        // survives NO_COLOR and a 16-colour SSH terminal.
        let app = searching(&["o sync caiu aqui"], "sync");
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("[1/1]"), "{screen}");
    }

    #[test]
    fn a_search_with_no_results_says_zero_instead_of_disappearing() {
        let app = searching(&["o sync caiu aqui"], "harmônicos");
        let screen = draw(&app, Palette::True, (80, 24));
        assert!(screen.contains("[0/0]"), "{screen}");
    }

    #[test]
    fn the_counter_counts_the_whole_history_and_moves_with_n() {
        let mut app = searching(&["o sync caiu", "sync de novo e sync"], "sync");
        assert!(draw(&app, Palette::True, (80, 24)).contains("[1/3]"));

        if let Some(search) = app.busca.as_mut() {
            search.next_match();
            search.next_match();
        }
        assert!(draw(&app, Palette::True, (80, 24)).contains("[3/3]"));
    }

    #[test]
    fn the_counter_survives_every_palette() {
        // The point of the counter is that it is not colour. Mono is where a
        // decoration would give itself away by vanishing.
        let app = searching(&["o sync caiu aqui"], "sync");
        for palette in [Palette::True, Palette::Ansi16, Palette::Mono] {
            let screen = draw(&app, palette, (80, 24));
            assert!(screen.contains("[1/1]"), "{palette:?} lost it:\n{screen}");
        }
    }

    #[test]
    fn both_occurrences_are_drawn_and_only_the_current_one_is_lit() {
        let theme = Theme::with_palette(Palette::True);
        let mut seen = 0;
        let spans = highlight("o sync e o sync", "sync", theme, Some(1), &mut seen);

        assert_eq!(seen, 2, "the two hits were not both counted");
        let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "  o sync e o sync", "the segment was not kept whole");

        let mut hits = spans.iter().filter(|span| span.content == "sync");
        assert_eq!(hits.next().map(|span| span.style), Some(theme.accent()));
        assert_eq!(
            hits.next().map(|span| span.style),
            Some(theme.accent().add_modifier(Modifier::REVERSED)),
            "the current occurrence is not told apart from its neighbour"
        );
        assert!(hits.next().is_none(), "a third hit appeared from nowhere");
    }

    #[test]
    fn a_double_space_does_not_shift_the_highlight() {
        // This is the case the per-segment design exists for. `wrap` collapses
        // runs of whitespace, so an offset taken over the raw body would light
        // `aiu ` — one character to the right — instead of `caiu`.
        let theme = Theme::with_palette(Palette::True);
        let lines = message_lines(&said("o  sync  caiu"), 40, theme, "caiu", Some(0));

        let body: Vec<&Span<'_>> = lines
            .iter()
            .skip(1)
            .flat_map(|line| line.spans.iter())
            .collect();
        let text: String = body.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "  o sync caiu");

        let lit: Vec<&str> = body
            .iter()
            .filter(|span| span.style != theme.body())
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(lit, ["caiu"], "the highlight landed off the term");
    }

    #[test]
    fn an_empty_term_draws_exactly_what_it_drew_before() {
        let theme = Theme::with_palette(Palette::True);
        let lines = message_lines(&said("o sync caiu"), 40, theme, "", None);

        let body: Vec<&Span<'_>> = lines
            .iter()
            .skip(1)
            .flat_map(|line| line.spans.iter())
            .collect();
        let text: String = body.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "  o sync caiu");
        assert!(
            body.iter().all(|span| span.style == theme.body()),
            "an empty term still styled something"
        );

        // And clearing a search that was really live puts the screen back
        // exactly as it was. `populated()` never sets `termo`, so emptying it
        // there and redrawing would only prove that `draw` is deterministic.
        let mut app = populated();
        let quiet = draw(&app, Palette::True, (80, 24));

        app.mode = Mode::Search;
        app.input = "harmônicos".into();
        app.termo = "harmônicos".into();
        app.refazer_busca();
        assert!(app.busca.is_some(), "the search never went live");
        assert_ne!(
            draw(&app, Palette::True, (80, 24)),
            quiet,
            "the live search changed nothing, so clearing it proves nothing"
        );

        app.mode = Mode::Normal;
        app.input.clear();
        app.termo.clear();
        app.refazer_busca();
        assert!(app.busca.is_none(), "an empty term must erase the search");
        assert_eq!(
            draw(&app, Palette::True, (80, 24)),
            quiet,
            "clearing the search left something behind"
        );
    }

    #[test]
    fn history_lines_lights_the_right_occurrence_in_the_right_message() {
        // The wiring `draw()` cannot see. It returns symbols and throws styles
        // away, so passing `""` as the term, dropping the ordinal, or comparing
        // against the wrong message index would leave every screen test green.
        let theme = Theme::with_palette(Palette::True);
        let reversed = theme.accent().add_modifier(Modifier::REVERSED);

        let mut app = populated();
        app.messages = vec![said("sync no começo"), said("o sync e o sync")];
        app.mode = Mode::Search;
        app.input = "sync".into();
        app.termo = "sync".into();
        app.refazer_busca();

        // (hits drawn, which one is emphasised, how many are, first line of the
        // message the cursor is in).
        let read = |app: &App| {
            let (lines, current_line) = history_lines(app, 40, theme);
            let spans: Vec<Span<'static>> = lines.into_iter().flat_map(|line| line.spans).collect();
            let drawn = spans.iter().filter(|span| span.content == "sync").count();
            let which = spans
                .iter()
                .filter(|span| span.content == "sync")
                .position(|span| span.style == reversed);
            let emphasised = spans.iter().filter(|span| span.style == reversed).count();
            (drawn, which, emphasised, current_line)
        };

        // Three hits over two messages. All three are drawn whatever the cursor
        // is doing; exactly one of them is the one you are on.
        assert_eq!(read(&app), (3, Some(0), 1, Some(0)));

        // Message 0 is a header plus one wrapped body line, so message 1 starts
        // on line 2 — and the hit in message 0 goes back to being an ordinary
        // one, which is `current.filter(|c| c.message == index)` doing its job.
        if let Some(search) = app.busca.as_mut() {
            search.next_match();
        }
        assert_eq!(
            read(&app),
            (3, Some(1), 1, Some(2)),
            "the emphasis did not follow the cursor into the next message"
        );

        // Both hits live in the same message now, so only `ordinal_in_message`
        // can tell them apart.
        if let Some(search) = app.busca.as_mut() {
            search.next_match();
        }
        assert_eq!(
            read(&app),
            (3, Some(2), 1, Some(2)),
            "the ordinal within the message was not threaded through"
        );
    }

    #[test]
    fn an_overlapping_match_neither_panics_nor_draws_twice() {
        // `occurrences("aaa", "aa")` returns (0,2) and (1,3): the second starts
        // before the first ends. Subtracting the cursor from that start goes
        // negative, which in a debug build is a panic and not a wrong colour —
        // and a term of `"aa"` or `"ss"` is an ordinary thing to type.
        let theme = Theme::with_palette(Palette::True);
        let reversed = theme.accent().add_modifier(Modifier::REVERSED);

        let mut seen = 0;
        let spans = highlight("aaa", "aa", theme, Some(0), &mut seen);
        assert_eq!(seen, 2, "the core counts both, so the ordinals must too");
        let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "  aaa", "the overlap was drawn twice");
        assert_eq!(
            spans.iter().filter(|span| span.style == reversed).count(),
            1
        );

        // And the cost the comment admits to: sitting on the overlapped hit
        // emphasises nothing, while the counter still reads [2/2].
        let mut seen = 0;
        let spans = highlight("aaa", "aa", theme, Some(1), &mut seen);
        let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "  aaa");
        assert_eq!(
            spans.iter().filter(|span| span.style == reversed).count(),
            0
        );
    }

    #[test]
    fn the_cramped_layout_keeps_the_counter_when_the_warning_will_not_fit() {
        // Below 80×24 there is no compose line, so the warning row carries the
        // counter. `TERMINAL 24×20 < 80×24` is 22 cells on its own: appended
        // rather than reserved, the counter would be clipped off and the
        // highlight left carrying the position alone — specs/05:144 failing in
        // the very layout the extra row was added to honour.
        let app = searching(&["o sync caiu aqui"], "sync");
        for columns in [24u16, 28, 40, 60] {
            let screen = draw(&app, Palette::True, (columns, 20));
            assert!(
                screen.contains("[1/1]"),
                "{columns} columns lost the counter:\n{screen}"
            );
            for line in screen.lines() {
                assert!(
                    width(line) <= columns as usize,
                    "{columns} columns produced a {}-cell row: {line:?}",
                    width(line)
                );
            }
        }
    }

    #[test]
    fn a_wide_term_lights_whole_cells_and_never_half_a_kanji() {
        let theme = Theme::with_palette(Palette::True);
        let mut seen = 0;
        let spans = highlight("em 第3新東京市 agora", "新東京", theme, Some(0), &mut seen);

        let lit: Vec<&str> = spans
            .iter()
            .filter(|span| span.style != theme.body())
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(lit, ["新東京"]);
        // The spans still add up to the segment's own cell width plus indent.
        let cells: usize = spans.iter().map(|span| width(&span.content)).sum();
        assert_eq!(cells, width("em 第3新東京市 agora") + 2);
    }

    #[test]
    fn the_scroll_stays_on_the_tail_without_an_occurrence() {
        assert_eq!(scroll(100, 10, None, true), 90);
        assert_eq!(scroll(100, 10, Some(4), false), 90);
        assert_eq!(scroll(4, 10, None, false), 0);
    }

    #[test]
    fn the_scroll_centres_on_the_occurrence_without_passing_the_tail() {
        assert_eq!(scroll(100, 10, Some(50), true), 45);
        assert_eq!(scroll(100, 10, Some(2), true), 0);
        // Past the tail would trade conversation for blank rows.
        assert_eq!(scroll(100, 10, Some(99), true), 90);
    }

    #[test]
    fn an_occurrence_above_the_tail_is_scrolled_to() {
        let mut app = populated();
        let mut messages = vec![said("o sync caiu aqui")];
        messages.extend((0..40).map(|index| said(&format!("ruído {index}"))));
        app.messages = messages;

        let quiet = draw(&app, Palette::True, (80, 24));
        assert!(
            !quiet.contains("o sync caiu"),
            "the occurrence was already on the tail, so this proves nothing:\n{quiet}"
        );

        app.mode = Mode::Search;
        app.input = "sync".into();
        app.termo = "sync".into();
        app.refazer_busca();

        let screen = draw(&app, Palette::True, (80, 24));
        assert!(
            screen.contains("o sync caiu"),
            "the history never scrolled to the occurrence:\n{screen}"
        );
    }

    #[test]
    fn a_search_that_matches_nothing_moves_nothing() {
        let mut app = populated();
        app.messages = (0..40)
            .map(|index| said(&format!("ruído {index}")))
            .collect();
        let quiet = draw(&app, Palette::True, (80, 24));

        app.mode = Mode::Search;
        app.input = "harmônicos".into();
        app.termo = "harmônicos".into();
        app.refazer_busca();
        let screen = draw(&app, Palette::True, (80, 24));

        // The compose line and the mode in the bar changed, and nothing else:
        // with no occurrence to go to, the history stays where it was.
        let history = |screen: &str| {
            screen
                .lines()
                .filter(|line| line.contains("ruído"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert!(!history(&quiet).is_empty(), "nothing was drawn:\n{quiet}");
        assert_eq!(
            history(&screen),
            history(&quiet),
            "the history moved:\n{quiet}\n---\n{screen}"
        );
    }

    #[test]
    fn a_search_never_overflows_the_panel() {
        let mut app = searching(&["o sync caiu aqui"], "sync");
        app.input = "sync".repeat(40);
        for size in [(80u16, 24u16), (100, 40), (60, 18)] {
            let screen = draw(&app, Palette::True, size);
            for line in screen.lines() {
                assert!(
                    width(line) <= size.0 as usize,
                    "{size:?} produced a {}-cell row: {line:?}",
                    width(line)
                );
            }
            assert!(
                screen.contains("[1/1]"),
                "a long draft pushed the counter off screen at {size:?}:\n{screen}"
            );
        }
    }

    #[test]
    fn a_long_draft_shows_its_tail_so_the_caret_stays_visible() {
        let mut app = populated();
        app.mode = Mode::Insert;
        app.input = "x".repeat(300);
        let screen = draw(&app, Palette::True, (80, 24));
        for line in screen.lines() {
            assert_eq!(width(line), 80);
        }
        assert!(screen.contains("x_"), "the caret was pushed off screen");
    }
}
