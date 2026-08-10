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
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Bar, ChatLine, Mode, Node, Panel, Screen};
use crate::theme::Theme;

/// The smallest terminal `specs/05-cliente-tui.md` supports.
pub const MIN_WIDTH: u16 = 80;
/// The smallest terminal height `specs/05-cliente-tui.md` supports.
pub const MIN_HEIGHT: u16 = 24;

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
    let alert_rows = u16::from(app.alert.is_some());
    let [battery, banner, panels, bar] = Layout::vertical([
        Constraint::Length(battery_rows),
        Constraint::Length(alert_rows),
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
    if app.alert.is_some() {
        render_alert(frame, app, theme, banner);
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
            render_convite(frame, theme, area, convite);
        }
    }
}

/// O link de convite, numa caixa larga o bastante para ele caber inteiro.
///
/// Largura inteira de propósito. O painel de mensagens tem cinquenta colunas e
/// o link tem uns noventa; ali ele quebraria em duas linhas, e um link quebrado
/// é um link que ninguém copia — que é a única coisa que se faz com ele.
fn render_convite(frame: &mut Frame<'_>, theme: Theme, area: Rect, convite: &str) {
    let largura = area.width.saturating_sub(4).max(20);
    let altura = 9;
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
            "Mande este link para quem você quer no Dogma:",
            theme.label(),
        )),
        Line::from(""),
    ];
    for pedaco in wrap(convite, budget) {
        lines.push(Line::from(Span::styled(pedaco, theme.accent())));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Vale para quem chegar primeiro. O Dogma acaba quando você sair.",
        theme.label(),
    )));
    lines.push(Line::from(Span::styled(
        "`:convite` mostra de novo · qualquer tecla fecha",
        theme.label(),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The outer frame: `SEELE ─ 同期率 ─ 第3新東京市 ─ 12:04:33`.
fn title_block(app: &App, theme: Theme) -> Block<'static> {
    let mut spans = vec![
        Span::styled(" SEELE ", theme.accent()),
        Span::styled("─ ", theme.fg(crate::theme::RULE)),
    ];

    // Kanji are a typographic accent that never carries information
    // (specs/07-tema-evangelion.md), so a terminal that cannot draw them loses
    // nothing by not being shown them.
    if theme.kanji() {
        spans.push(Span::styled("同期率 ", theme.label()));
        spans.push(Span::styled("─ ", theme.fg(crate::theme::RULE)));
        spans.push(Span::styled("第3新東京市 ", theme.label()));
        spans.push(Span::styled("─ ", theme.fg(crate::theme::RULE)));
    }

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
    let block = panel("DOGMA", app.focus == Panel::Dogma, theme);
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
    let block = panel("CAGES / LINHAS", app.focus == Panel::Channels, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let budget = inner.width as usize;
    let mut lines = Vec::new();

    for (index, node) in app.tree.iter().enumerate() {
        let selected = index == app.selected && app.focus == Panel::Channels;
        lines.push(match node {
            Node::Cage { name, open } => {
                let arrow = if *open { "▼ " } else { "▶ " };
                Line::from(Span::styled(
                    truncate(&format!("{arrow}{name}"), budget),
                    if selected {
                        theme.accent()
                    } else {
                        theme.body()
                    },
                ))
            }
            Node::Line { name } => Line::from(Span::styled(
                truncate(&format!("─ LINHA {name}"), budget),
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

/// One pilot row: presence, name, and the Sync Ratio with its mark.
///
/// `specs/05-cliente-tui.md`: "Nenhuma informação transmitida **só** por cor: a
/// Taxa de Sincronização é sempre acompanhada do número". So the number is
/// always printed, the band mark is always printed, and colour is the third
/// channel rather than the only one.
fn pilot_line(pilot: &crate::app::RosterEntry, budget: usize, theme: Theme) -> Line<'static> {
    let presence = if pilot.speaking { "●" } else { "○" };
    // A.T. Field and Isolamento total get text, not just a hue, for the same
    // reason: this interface is used by people who cannot tell red from green.
    let flag = if pilot.at_field {
        "A.T."
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

    let mut lines: Vec<Line<'_>> = Vec::new();
    for message in app.messages.iter().chain(&app.local) {
        lines.extend(message_lines(message, budget, theme));
    }

    // Show the tail: a chat that scrolls off the top is normal, a chat that
    // shows the first thing anybody ever said is broken.
    let visible = history.height as usize;
    let skip = lines.len().saturating_sub(visible);

    frame.render_widget(
        Paragraph::new(lines.split_off(skip)).style(dim(theme, app)),
        history,
    );
    frame.render_widget(compose_line(app, theme, budget), compose);
}

fn message_lines(message: &ChatLine, budget: usize, theme: Theme) -> Vec<Line<'static>> {
    let header = format!("{} {}", message.at, message.author);
    let mut lines = vec![Line::from(Span::styled(
        truncate(&header, budget),
        if message.own {
            theme.accent()
        } else {
            theme.label()
        },
    ))];

    for wrapped in wrap(&message.body, budget.saturating_sub(2)) {
        lines.push(Line::from(Span::styled(
            format!("  {wrapped}"),
            theme.body(),
        )));
    }
    lines
}

fn compose_line(app: &App, theme: Theme, budget: usize) -> Paragraph<'static> {
    let (prefix, style) = match app.mode {
        Mode::Insert => ("▸ ", theme.body()),
        Mode::Command => (": ", theme.accent()),
        Mode::Search => ("/ ", theme.accent()),
        Mode::Normal => ("▸ ", theme.label()),
    };

    let shown = if app.mode == Mode::Normal {
        String::new()
    } else {
        // Keep the caret visible in a long line by showing the tail.
        let room = budget.saturating_sub(width(prefix) + 1);
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

    Paragraph::new(Line::from(vec![
        Span::styled(prefix, style),
        Span::styled(shown, theme.body()),
        Span::styled("_", theme.accent()),
    ]))
}

/// The permanent telemetry bar.
///
/// `SYNC 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ A.T. OFF`
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
            "SYNC {}{:>3}%",
            Theme::sync_mark(seele_core::SyncBand::of(bar.sync)),
            bar.sync
        ),
        format!("RTT {:.0}ms", bar.rtt_ms),
        format!("JIT {:.0}ms", bar.jitter_ms),
        format!("LOSS {:.1}%", bar.loss * 100.0),
        format!("OPUS {}k", bar.bitrate / 1000),
        format!("A.T. {}", if at_field { "ON" } else { "OFF" }),
    ];
    if total_isolation {
        segments.push("SURDO".to_string());
    }
    segments
}

/// Boot: the three subsystems reporting.
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
        Line::from(Span::styled("  SEELE SYSTEM", theme.accent())),
        Line::from(""),
        Line::from(Span::styled("  MELCHIOR ...... ok", theme.body())),
        Line::from(Span::styled("  BALTHASAR ..... ok", theme.body())),
        Line::from(Span::styled("  CASPER ........ ok", theme.body())),
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

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(
                &format!("TERMINAL {}×{} < 80×24", area.width, area.height),
                area.width as usize,
            ),
            theme.alert(),
        ))),
        warning,
    );

    let budget = history.width as usize;
    let mut lines = Vec::new();
    for message in app.messages.iter().chain(&app.local) {
        lines.extend(message_lines(message, budget, theme));
    }
    let visible = history.height as usize;
    let skip = lines.len().saturating_sub(visible);

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
    if theme.kanji() {
        spans.push(Span::styled("警告 ", theme.alert()));
    }
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
fn render_lost(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect, reason: &str) {
    let block = title_block(app, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let budget = inner.width as usize;
    let mut lines = vec![
        Line::from(Span::styled("  ENLACE ENCERRADO", theme.alert())),
        Line::from(""),
    ];
    for wrapped in wrap(reason, budget.saturating_sub(2)) {
        lines.push(Line::from(Span::styled(
            format!("  {wrapped}"),
            theme.body(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  qualquer tecla para ejetar",
        theme.label(),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_alert(frame: &mut Frame<'_>, app: &App, theme: Theme, area: Rect) {
    let Some(alert) = &app.alert else { return };

    let mut spans = Vec::new();
    if theme.kanji() {
        spans.push(Span::styled("警告 ", theme.alert()));
    }
    spans.push(Span::styled(
        truncate(&alert.text, area.width as usize),
        theme.alert(),
    ));
    if alert.blocking {
        spans.push(Span::styled("  [enter]", theme.label()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The help overlay. `specs/09-roadmap.md` accepts M4 on somebody outside the
/// project connecting and talking with only `?`, so this is a deliverable and
/// not a courtesy.
fn render_help(frame: &mut Frame<'_>, theme: Theme, area: Rect) {
    let rows = [
        ("h j k l / setas", "navegar"),
        ("Tab", "alternar painel"),
        ("Enter", "entrar no Cage / abrir Linha"),
        ("i", "escrever mensagem"),
        ("Espaço (segurar)", "falar"),
        ("m", "A.T. Field (mudo)"),
        ("d", "isolamento total (surdo)"),
        ("g / G", "topo / fim"),
        ("?", "esta ajuda"),
        (":conectar <host>", "conectar a um Dogma"),
        (":cage <nome>", "entrar num Cage"),
        (":sync", "diagnóstico detalhado"),
        (":audio", "dispositivos"),
        (":q", "ejetar e sair"),
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

    #[test]
    fn width_counts_cells_and_not_bytes() {
        // The bug specs/05-cliente-tui.md warns about, pinned. 同期率 is nine
        // bytes, three characters, and six cells; only the last is a layout.
        assert_eq!("同期率".len(), 9);
        assert_eq!("同期率".chars().count(), 3);
        assert_eq!(width("同期率"), 6);
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

        assert!(screen.contains("DOGMA"), "{screen}");
        assert!(screen.contains("CAGES / LINHAS"), "{screen}");
        assert!(screen.contains("MENSAGENS"), "{screen}");
        assert!(screen.contains("SYNC"), "{screen}");
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
    fn mono_drops_kanji_but_keeps_every_number() {
        // Kanji are decoration (specs/07); Sync Ratios are not.
        let app = populated();
        let mono = draw(&app, Palette::Mono, (80, 24));

        assert!(!mono.contains("同期率"), "{mono}");
        assert!(mono.contains("94"), "the Sync Ratio vanished:\n{mono}");
        assert!(
            mono.contains("98"),
            "a pilot's Sync Ratio vanished:\n{mono}"
        );
        assert!(mono.contains("A.T."), "the mute marker vanished:\n{mono}");
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
            !screen.contains("CAGES"),
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
    fn the_boot_screen_reports_the_three_subsystems() {
        let mut app = populated();
        app.screen = Screen::Boot;
        let screen = draw(&app, Palette::True, (80, 24));

        assert!(screen.contains("MELCHIOR"), "{screen}");
        assert!(screen.contains("BALTHASAR"), "{screen}");
        assert!(screen.contains("CASPER"), "{screen}");
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
        assert!(screen.contains("ejetar"), "no way out shown:\n{screen}");
        assert!(
            !screen.contains("CAGES / LINHAS"),
            "a dead session still shows a live roster:\n{screen}"
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
    fn the_help_overlay_lists_the_essential_keys() {
        // The M4 acceptance criterion is that an outsider gets by with `?`.
        let mut app = populated();
        app.help = true;
        let screen = draw(&app, Palette::True, (80, 24));

        for key in ["Tab", "Enter", ":q", "?"] {
            assert!(screen.contains(key), "`{key}` missing from help:\n{screen}");
        }
        assert!(screen.contains("falar"), "{screen}");
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
