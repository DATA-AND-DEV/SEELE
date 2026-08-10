//! What the client is showing and what a keystroke means.
//!
//! Pure: no terminal, no sockets, no clock. `specs/05-cliente-tui.md` describes
//! a modal interface with four modes and six visual states, and all of that is
//! decidable without drawing anything — so it is decided here and tested here.
//! [`crate::ui`] turns the result into cells.
//!
//! # Modal, in the spirit of Vim
//!
//! `specs/05-cliente-tui.md`: "porque o público é esse". The consequence that
//! matters is that a keystroke means different things in different modes, and
//! the one place that gets decided is [`App::on_key`].

use seele_core::{Link, Pattern, SyncBand};

/// Which input mode the client is in. `specs/05-cliente-tui.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigation by single keys.
    Normal,
    /// Typing a message.
    Insert,
    /// Typing a command after `:`.
    Command,
    /// Typing a search after `/`.
    Search,
}

impl Mode {
    /// The word shown in the telemetry bar.
    ///
    /// A mode you cannot see is a mode you press the wrong key in.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERÇÃO",
            Self::Command => "COMANDO",
            Self::Search => "BUSCA",
        }
    }
}

/// Which panel has focus. `specs/05-cliente-tui.md` has three, plus the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// The Dogma list.
    Dogma,
    /// Cages and Lines.
    Channels,
    /// Messages.
    Messages,
}

impl Panel {
    /// The next panel, for `Tab`.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Dogma => Self::Channels,
            Self::Channels => Self::Messages,
            Self::Messages => Self::Dogma,
        }
    }
}

/// The six visual states `specs/05-cliente-tui.md` requires to exist.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// Boot: the three subsystems reporting.
    ///
    /// `specs/05` is emphatic that this lasts exactly as long as connecting
    /// takes: "se conectar em 200 ms, não inventar espera artificial. Animação
    /// decorativa que atrasa o usuário é falha de design."
    Boot,
    /// PADRÃO: LARANJA — connected, not verified.
    PatternOrange,
    /// PADRÃO: AZUL — normal operation.
    PatternBlue,
    /// Internal battery: disconnected, counting down, still readable.
    InternalBattery {
        /// Seconds left of the five minutes.
        remaining: u64,
        /// Reconnection attempts so far.
        attempts: u32,
    },
    /// Something went wrong and the session is over.
    Lost {
        /// What happened, in the interface's own words.
        reason: String,
    },
}

/// One pilot in the roster.
#[derive(Debug, Clone, PartialEq)]
pub struct RosterEntry {
    /// Display name.
    pub nickname: String,
    /// Their Sync Ratio, 0 to 100.
    pub sync: u8,
    /// Whether they are transmitting.
    pub speaking: bool,
    /// Microphone muted — A.T. Field.
    pub at_field: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
}

impl RosterEntry {
    /// Which band their Sync Ratio falls into.
    #[must_use]
    pub fn band(&self) -> SyncBand {
        SyncBand::of(self.sync)
    }
}

/// One row of the Cages/Lines panel.
///
/// `specs/05-cliente-tui.md` draws pilots nested under their Cage rather than
/// in a panel of their own, so the panel is a flattened tree and not a list.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A Cage — a voice room.
    Cage {
        /// Its name.
        name: String,
        /// Whether its pilots are showing.
        open: bool,
    },
    /// A pilot inside the Cage above.
    Pilot(RosterEntry),
    /// A Line — a text channel.
    Line {
        /// Its name, including the `#`.
        name: String,
    },
}

impl Node {
    /// Whether the selection is allowed to land here.
    ///
    /// Pilots are shown, not entered: `Enter` on a pilot means nothing, so
    /// stopping there on the way down just costs keystrokes.
    #[must_use]
    pub fn selectable(&self) -> bool {
        !matches!(self, Self::Pilot(_))
    }
}

/// One line of conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatLine {
    /// Wall-clock time, already formatted. The core does not format; the shell
    /// does, and this is the shell.
    pub at: String,
    /// Who said it.
    pub author: String,
    /// What they said.
    pub body: String,
    /// Whether it was this pilot.
    pub own: bool,
}

/// What the telemetry bar shows. `specs/05-cliente-tui.md` keeps it permanent:
/// "A telemetria é permanente, não escondida em menu — é a diferença de caráter
/// em relação a um cliente de chat comum."
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bar {
    /// Sync Ratio, 0 to 100.
    pub sync: u8,
    /// Round trip, milliseconds.
    pub rtt_ms: f32,
    /// Arrival jitter, milliseconds.
    pub jitter_ms: f32,
    /// Packet loss, 0.0 to 1.0.
    pub loss: f32,
    /// Encoder bitrate, bits per second.
    pub bitrate: u32,
}

/// An alert banner. `specs/07-tema-evangelion.md`: `Alerta · 警告`.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    /// The line to show.
    pub text: String,
    /// Whether it blocks until dismissed.
    ///
    /// `specs/08-seguranca.md` requires the certificate-change warning to be
    /// "impossível de ignorar". A banner that scrolls away is not that.
    pub blocking: bool,
}

/// What a keystroke asked for that the app cannot do alone.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Leave. `:q` — "ejetar e sair".
    Quit,
    /// Post the composed message.
    Send(String),
    /// Run a command typed after `:`.
    Command(String),
    /// Start transmitting. Space held, in Normal mode.
    StartTalking,
    /// Stop transmitting.
    StopTalking,
    /// Toggle the microphone mute.
    ToggleAtField,
    /// Toggle the speaker mute.
    ToggleTotalIsolation,
    /// Enter the selected Cage or open the selected Line.
    Activate,
}

/// Everything on screen.
#[derive(Debug, Clone)]
pub struct App {
    /// Input mode.
    pub mode: Mode,
    /// Focused panel.
    pub focus: Panel,
    /// Which of the six states is showing.
    pub screen: Screen,
    /// The Dogmas this pilot knows about.
    pub dogmas: Vec<String>,
    /// Which Dogma is selected.
    pub selected_dogma: usize,
    /// Cages, their pilots, and Lines — flattened for drawing.
    pub tree: Vec<Node>,
    /// Which tree row is selected.
    pub selected: usize,
    /// The conversation, projected from the room.
    ///
    /// Rebuilt wholesale on every change, so nothing local may live here.
    pub messages: Vec<ChatLine>,
    /// The client's own answers — `:sync`, `:audio`, an unknown command.
    ///
    /// Kept apart from the conversation because they are not part of it: they
    /// were never sent anywhere and nobody else can see them. They are drawn
    /// after the last message, which is where an answer to something just typed
    /// belongs.
    pub local: Vec<ChatLine>,
    /// What the bar shows.
    pub bar: Bar,
    /// The composition buffer, shared by Insert, Command and Search.
    pub input: String,
    /// A banner, if there is one.
    pub alert: Option<Alert>,
    /// Whether this pilot is transmitting.
    pub speaking: bool,
    /// Microphone muted.
    pub at_field: bool,
    /// Speakers muted.
    pub total_isolation: bool,
    /// Whether the help overlay is up. `?` in Normal mode.
    pub help: bool,
    /// O link de convite, quando este cliente é o anfitrião.
    ///
    /// Fica numa sobreposição de largura inteira e não no painel de mensagens:
    /// o link tem uns noventa caracteres e o painel tem cinquenta. Um convite
    /// quebrado em duas linhas é um convite que ninguém consegue copiar, e
    /// copiar é a única coisa que se faz com ele.
    pub convite: Option<String>,
    /// Se a sobreposição do convite está aberta.
    pub convite_visivel: bool,
    /// Wall-clock time for the title bar, already formatted.
    ///
    /// The shell owns the clock. `seele-core` deals in monotonic durations and
    /// has no opinion about what time it is where the pilot is sitting.
    pub clock: String,
    /// Set when the app should exit.
    pub quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// A client that has not connected yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            focus: Panel::Channels,
            screen: Screen::Boot,
            dogmas: Vec::new(),
            selected_dogma: 0,
            tree: Vec::new(),
            selected: 0,
            messages: Vec::new(),
            local: Vec::new(),
            bar: Bar::default(),
            input: String::new(),
            alert: None,
            speaking: false,
            at_field: false,
            total_isolation: false,
            help: false,
            convite: None,
            convite_visivel: false,
            clock: "--:--:--".into(),
            quit: false,
        }
    }

    /// Folds in the connection state from the core.
    pub fn set_pattern(&mut self, pattern: Pattern) {
        self.screen = match pattern {
            Pattern::Offline => Screen::Boot,
            Pattern::Orange => Screen::PatternOrange,
            Pattern::Blue => Screen::PatternBlue,
        };
    }

    /// Folds in the link state from [`seele_core::Battery`].
    pub fn set_link(&mut self, link: Link, remaining_seconds: u64) {
        self.screen = match link {
            Link::Online => Screen::PatternBlue,
            Link::InternalBattery { attempts } => Screen::InternalBattery {
                remaining: remaining_seconds,
                attempts,
            },
            Link::Discharged => Screen::Lost {
                reason: "BATERIA INTERNA ESGOTADA".into(),
            },
        };
    }

    /// Whether the interface should be drawn dimmed but legible.
    ///
    /// `specs/07-tema-evangelion.md`: during the internal battery the interface
    /// is "esmaecida mas ainda legível — o histórico continua ali para leitura".
    /// Blanking it would be the simplification the spec warns against.
    #[must_use]
    pub fn dimmed(&self) -> bool {
        matches!(
            self.screen,
            Screen::InternalBattery { .. } | Screen::Lost { .. }
        )
    }

    /// Handles one keystroke, returning anything the caller must act on.
    ///
    /// Every mode is handled here, because a key that means one thing in Normal
    /// and another in Insert is exactly the bug that a scattered handler
    /// produces.
    pub fn on_key(&mut self, key: Key) -> Option<Action> {
        // A blocking alert swallows everything but its dismissal.
        // specs/08-seguranca.md wants the certificate warning impossible to
        // ignore, and a banner you can type past is not that.
        if let Some(alert) = &self.alert {
            if alert.blocking {
                if matches!(key, Key::Enter | Key::Esc) {
                    self.alert = None;
                }
                return None;
            }
        }

        if self.help {
            self.help = false;
            return None;
        }

        if self.convite_visivel {
            self.convite_visivel = false;
            return None;
        }

        match self.mode {
            Mode::Normal => self.on_normal(key),
            Mode::Insert => self.on_typing(key, Action::Send),
            Mode::Command => self.on_typing(key, Action::Command),
            // Search filters the message list as it is typed, so there is
            // nothing to do on submit but leave the mode — which on_typing
            // already did by the time the discarded action comes back.
            Mode::Search => {
                let _ = self.on_typing(key, Action::Command);
                None
            }
        }
    }

    fn on_normal(&mut self, key: Key) -> Option<Action> {
        match key {
            Key::Char('i') => {
                self.mode = Mode::Insert;
                self.input.clear();
            }
            Key::Char(':') => {
                self.mode = Mode::Command;
                self.input.clear();
            }
            Key::Char('/') => {
                self.mode = Mode::Search;
                self.input.clear();
            }
            Key::Char('?') => self.help = true,
            Key::Tab => self.focus = self.focus.next(),
            Key::Char('j') | Key::Down => self.move_selection(1),
            Key::Char('k') | Key::Up => self.move_selection(-1),
            Key::Char('g') => self.jump(Edge::First),
            Key::Char('G') => self.jump(Edge::Last),
            Key::Char('m') => return Some(Action::ToggleAtField),
            Key::Char('d') => return Some(Action::ToggleTotalIsolation),
            Key::Enter => return Some(Action::Activate),
            // specs/05-cliente-tui.md marks the space bar as open: it collides
            // with typing. Decision D19 keeps push-to-talk in Normal mode only,
            // where there is nothing to collide with.
            Key::SpaceDown => return Some(Action::StartTalking),
            Key::SpaceUp => return Some(Action::StopTalking),
            _ => {}
        }
        None
    }

    fn on_typing<F>(&mut self, key: Key, finish: F) -> Option<Action>
    where
        F: FnOnce(String) -> Action,
    {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            Key::Enter => {
                let text = std::mem::take(&mut self.input);
                self.mode = Mode::Normal;
                if !text.trim().is_empty() {
                    return Some(finish(text));
                }
            }
            Key::Backspace => {
                self.input.pop();
            }
            Key::Char(character) => self.input.push(character),
            _ => {}
        }
        None
    }

    /// Moves the selection inside whichever panel has focus.
    ///
    /// Deliberately clamps instead of wrapping: the tree is usually shorter
    /// than the screen, and a list that wraps makes `j` and `G` do the same
    /// thing often enough that neither can be trusted.
    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Panel::Dogma => {
                let Some(last) = self.dogmas.len().checked_sub(1) else {
                    return;
                };
                let next = self.selected_dogma as isize + delta;
                self.selected_dogma = next.clamp(0, last as isize) as usize;
            }
            Panel::Channels => self.step_tree(delta),
            // The message panel scrolls rather than selects, and scrolling is
            // the viewport's business, not the model's.
            Panel::Messages => {}
        }
    }

    /// Steps to the next selectable tree row in a direction, skipping pilots.
    fn step_tree(&mut self, delta: isize) {
        let mut index = self.selected as isize;
        loop {
            index += delta;
            let Ok(candidate) = usize::try_from(index) else {
                return;
            };
            match self.tree.get(candidate) {
                None => return,
                Some(node) if node.selectable() => {
                    self.selected = candidate;
                    return;
                }
                Some(_) => {}
            }
        }
    }

    fn jump(&mut self, edge: Edge) {
        match self.focus {
            Panel::Dogma => {
                self.selected_dogma = match edge {
                    Edge::First => 0,
                    Edge::Last => self.dogmas.len().saturating_sub(1),
                };
            }
            Panel::Channels => {
                self.selected = match edge {
                    Edge::First => 0,
                    Edge::Last => self.tree.len().saturating_sub(1),
                };
                if self
                    .tree
                    .get(self.selected)
                    .is_some_and(|node| !node.selectable())
                {
                    self.step_tree(match edge {
                        Edge::First => 1,
                        Edge::Last => -1,
                    });
                }
            }
            Panel::Messages => {}
        }
    }

    /// The pilots currently in the tree, in drawing order.
    pub fn roster(&self) -> impl Iterator<Item = &RosterEntry> {
        self.tree.iter().filter_map(|node| match node {
            Node::Pilot(entry) => Some(entry),
            _ => None,
        })
    }

    /// Records that the app should exit. `:q` — "ejetar e sair".
    pub fn quit(&mut self) {
        self.quit = true;
    }
}

/// Which end of a list `g` and `G` mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    First,
    Last,
}

/// A keystroke, abstracted from the terminal library.
///
/// Small on purpose: the state machine is the thing worth testing, and a test
/// that has to build a `crossterm::event::KeyEvent` tests crossterm too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// The space bar going down. Push-to-talk.
    SpaceDown,
    /// The space bar coming up.
    SpaceUp,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
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
            Node::Cage {
                name: "CAGE-02 TESTE".into(),
                open: false,
            },
            Node::Line {
                name: "#geral".into(),
            },
        ];
        app.screen = Screen::PatternBlue;
        app
    }

    #[test]
    fn typing_i_starts_a_message_and_escape_abandons_it() {
        let mut app = app();
        assert_eq!(app.on_key(Key::Char('i')), None);
        assert_eq!(app.mode, Mode::Insert);

        for character in "olá".chars() {
            app.on_key(Key::Char(character));
        }
        assert_eq!(app.input, "olá");

        app.on_key(Key::Esc);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.input.is_empty(), "an abandoned draft lingered");
    }

    #[test]
    fn enter_in_insert_mode_sends_and_returns_to_normal() {
        let mut app = app();
        app.on_key(Key::Char('i'));
        for character in "sync caiu aqui".chars() {
            app.on_key(Key::Char(character));
        }

        assert_eq!(
            app.on_key(Key::Enter),
            Some(Action::Send("sync caiu aqui".into()))
        );
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.input.is_empty());
    }

    #[test]
    fn an_empty_message_is_not_sent() {
        // Pressing enter on an empty line is how somebody leaves insert mode,
        // not how they post nothing.
        let mut app = app();
        app.on_key(Key::Char('i'));
        assert_eq!(app.on_key(Key::Enter), None);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn letters_that_are_commands_in_normal_mode_are_text_in_insert_mode() {
        // The whole point of being modal, and the bug a scattered key handler
        // produces: typing "diga" would otherwise deafen, insert, and quit.
        let mut app = app();
        app.on_key(Key::Char('i'));
        for character in "diga".chars() {
            assert_eq!(app.on_key(Key::Char(character)), None);
        }
        assert_eq!(app.input, "diga");
        assert!(!app.total_isolation);
    }

    #[test]
    fn colon_opens_a_command_and_enter_runs_it() {
        let mut app = app();
        app.on_key(Key::Char(':'));
        assert_eq!(app.mode, Mode::Command);
        for character in "sync".chars() {
            app.on_key(Key::Char(character));
        }
        assert_eq!(app.on_key(Key::Enter), Some(Action::Command("sync".into())));
    }

    #[test]
    fn tab_cycles_through_all_three_panels_and_returns() {
        let mut app = app();
        let first = app.focus;
        app.on_key(Key::Tab);
        app.on_key(Key::Tab);
        app.on_key(Key::Tab);
        assert_eq!(app.focus, first, "the cycle does not close");
    }

    #[test]
    fn navigation_stops_at_the_ends_instead_of_wrapping() {
        // Wrapping a short list makes `G` and `j` indistinguishable, and the
        // roster is usually shorter than the screen.
        let mut app = app();
        app.focus = Panel::Channels;
        for _ in 0..10 {
            app.on_key(Key::Char('j'));
        }
        assert_eq!(app.selected, app.tree.len() - 1);

        for _ in 0..10 {
            app.on_key(Key::Char('k'));
        }
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn g_and_shift_g_jump_to_the_ends() {
        let mut app = app();
        app.focus = Panel::Channels;
        app.on_key(Key::Char('G'));
        assert_eq!(app.selected, app.tree.len() - 1);
        app.on_key(Key::Char('g'));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn push_to_talk_only_works_in_normal_mode() {
        // specs/05-cliente-tui.md flags the collision between the space bar and
        // typing; decision D19 resolves it by keeping PTT out of insert mode.
        let mut app = app();
        assert_eq!(app.on_key(Key::SpaceDown), Some(Action::StartTalking));
        assert_eq!(app.on_key(Key::SpaceUp), Some(Action::StopTalking));

        app.on_key(Key::Char('i'));
        assert_eq!(
            app.on_key(Key::SpaceDown),
            None,
            "the space bar opened the microphone mid-sentence"
        );
    }

    #[test]
    fn a_blocking_alert_swallows_everything_until_dismissed() {
        // specs/08-seguranca.md: the certificate-change warning must be
        // impossible to ignore. A banner somebody can type past is not that.
        let mut app = app();
        app.alert = Some(Alert {
            text: "A CHAVE DO SERVIDOR MUDOU".into(),
            blocking: true,
        });

        assert_eq!(app.on_key(Key::Char('i')), None);
        assert_eq!(app.mode, Mode::Normal, "a blocking alert was typed past");
        assert_eq!(app.on_key(Key::Char('d')), None);

        app.on_key(Key::Enter);
        assert!(app.alert.is_none());
        assert_eq!(app.on_key(Key::Char('i')), None);
        assert_eq!(app.mode, Mode::Insert, "the alert did not release");
    }

    #[test]
    fn a_non_blocking_alert_does_not_get_in_the_way() {
        let mut app = app();
        app.alert = Some(Alert {
            text: "menção direta".into(),
            blocking: false,
        });
        app.on_key(Key::Char('i'));
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn the_help_overlay_closes_on_any_key() {
        // specs/09-roadmap.md accepts M4 on somebody outside the project being
        // able to connect and talk "só com `?`", so `?` has to be cheap to open
        // and cheap to leave.
        let mut app = app();
        app.on_key(Key::Char('?'));
        assert!(app.help);
        app.on_key(Key::Char('x'));
        assert!(!app.help);
    }

    #[test]
    fn the_battery_dims_the_interface_without_clearing_it() {
        // specs/07-tema-evangelion.md: "interface esmaecida mas ainda legível —
        // o histórico continua ali para leitura". Blanking it is the
        // simplification the spec asks to be protected from.
        let mut app = app();
        app.messages.push(ChatLine {
            at: "12:01".into(),
            author: "ayanami".into(),
            body: "verificando harmônicos".into(),
            own: false,
        });

        app.set_link(Link::InternalBattery { attempts: 3 }, 287);

        assert!(app.dimmed());
        assert_eq!(app.messages.len(), 1, "history was cleared");
        assert!(matches!(
            app.screen,
            Screen::InternalBattery {
                remaining: 287,
                attempts: 3
            }
        ));
    }

    #[test]
    fn every_mode_says_its_own_name() {
        // A mode you cannot see is a mode you press the wrong key in.
        let labels = [
            Mode::Normal.label(),
            Mode::Insert.label(),
            Mode::Command.label(),
            Mode::Search.label(),
        ];
        let unique: std::collections::HashSet<&&str> = labels.iter().collect();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn the_pattern_follows_the_core() {
        let mut app = App::new();
        app.set_pattern(Pattern::Orange);
        assert_eq!(app.screen, Screen::PatternOrange);
        app.set_pattern(Pattern::Blue);
        assert_eq!(app.screen, Screen::PatternBlue);
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;

    fn tree() -> Vec<Node> {
        vec![
            Node::Cage {
                name: "CAGE-01".into(),
                open: true,
            },
            Node::Pilot(RosterEntry {
                nickname: "ayanami".into(),
                sync: 98,
                speaking: false,
                at_field: false,
                total_isolation: false,
            }),
            Node::Pilot(RosterEntry {
                nickname: "shinji".into(),
                sync: 71,
                speaking: false,
                at_field: false,
                total_isolation: false,
            }),
            Node::Line {
                name: "#geral".into(),
            },
        ]
    }

    #[test]
    fn navigation_skips_over_pilots() {
        // Pilots are shown, not entered. Stopping on them on the way from a
        // Cage to a Line is two wasted keystrokes per pilot in the room.
        let mut app = App::new();
        app.tree = tree();
        app.focus = Panel::Channels;

        app.on_key(Key::Char('j'));

        assert_eq!(app.selected, 3, "the selection landed on a pilot");
    }

    #[test]
    fn jumping_to_the_end_lands_somewhere_selectable() {
        let mut app = App::new();
        app.tree = vec![
            Node::Cage {
                name: "CAGE-01".into(),
                open: true,
            },
            Node::Pilot(RosterEntry {
                nickname: "ayanami".into(),
                sync: 98,
                speaking: false,
                at_field: false,
                total_isolation: false,
            }),
        ];
        app.focus = Panel::Channels;

        app.on_key(Key::Char('G'));

        assert!(app.tree[app.selected].selectable());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn the_roster_is_the_pilots_in_drawing_order() {
        let mut app = App::new();
        app.tree = tree();
        let names: Vec<&str> = app.roster().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, ["ayanami", "shinji"]);
    }

    #[test]
    fn focus_decides_which_list_moves() {
        // Tab and j share the same j. Moving the wrong list is the bug.
        let mut app = App::new();
        app.tree = tree();
        app.dogmas = vec!["Terceira Tóquio".into(), "Geofront".into()];
        app.focus = Panel::Dogma;

        app.on_key(Key::Char('j'));

        assert_eq!(app.selected_dogma, 1);
        assert_eq!(app.selected, 0, "the tree moved while Dogma had focus");
    }
}
