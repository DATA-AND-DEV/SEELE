//! The theme, as `specs/07-tema-evangelion.md` defines it.
//!
//! > O tema é **vocabulário de produto**, não skin. É definido aqui uma vez e
//! > aplicado em todo lugar: interface, mensagens de erro, logs, documentação,
//! > nome de binário. Não se redesenha por tela.
//!
//! The values come from `design/seele-tokens.json`, frozen in task M0.12. That
//! file is the source; this is the translation, and the numbers must match it.
//!
//! # Degradation is not an afterthought
//!
//! `specs/05-cliente-tui.md` requires the client to be "utilizável por SSH em
//! 80×24 e 16 cores sem perda de informação". So a colour is not one value but
//! three — truecolor, 256, and 16 — chosen when the palette was frozen rather
//! than derived at runtime by rounding, which is how a careful palette turns
//! into mud over SSH.
//!
//! # Nothing is conveyed by colour alone
//!
//! `specs/05-cliente-tui.md` is explicit, and gives the reason: "daltonismo é
//! comum no público e a paleta depende muito de vermelho/verde". Every helper
//! here that returns a colour has a sibling that returns a mark or a word, and
//! the widgets use both. [`Palette::Mono`] then costs nothing to support,
//! because nothing was ever leaning on the colour.

use ratatui::style::{Color, Modifier, Style};
use seele_core::SignalBand;

/// How much colour the terminal can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    /// 24-bit. What a modern local terminal does.
    True,
    /// The xterm 256 cube. What most SSH sessions do.
    Ansi256,
    /// The sixteen the user has themed themselves.
    Ansi16,
    /// No colour at all: `NO_COLOR`, a pipe, or a terminal that cannot.
    Mono,
}

impl Palette {
    /// Works out what this terminal can do.
    ///
    /// `specs/05-cliente-tui.md` requires `NO_COLOR` to be respected, and the
    /// convention is that its mere presence counts — not its value.
    #[must_use]
    pub fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::Mono;
        }
        match std::env::var("COLORTERM").as_deref() {
            Ok("truecolor" | "24bit") => Self::True,
            _ => match std::env::var("TERM").as_deref() {
                Ok(term) if term.contains("256") => Self::Ansi256,
                Ok("dumb") | Err(_) => Self::Mono,
                _ => Self::Ansi16,
            },
        }
    }
}

/// One colour, in every palette it has to survive.
///
/// The 256 and 16 values are chosen, not computed. Rounding a careful palette
/// at runtime is how it turns to mud over SSH.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    truecolor: (u8, u8, u8),
    ansi256: u8,
    ansi16: Color,
}

impl Ink {
    /// The colour for a given palette. [`Palette::Mono`] yields none at all.
    #[must_use]
    pub fn resolve(self, palette: Palette) -> Option<Color> {
        match palette {
            Palette::True => {
                let (r, g, b) = self.truecolor;
                Some(Color::Rgb(r, g, b))
            }
            Palette::Ansi256 => Some(Color::Indexed(self.ansi256)),
            Palette::Ansi16 => Some(self.ansi16),
            Palette::Mono => None,
        }
    }
}

/// Background. Near-absolute black, never neutral charcoal (`specs/07`).
pub const BLACK: Ink = Ink {
    truecolor: (0x05, 0x04, 0x03),
    ansi256: 232,
    ansi16: Color::Black,
};

/// Panel surface. One step above the background.
///
/// In 256 colours this collapses onto [`BLACK`] — both are the nearest
/// neighbour of the same grey. That is not a rounding bug: it is why
/// `specs/07-tema-evangelion.md` defines a surface as "linha 1px + vazio". Over
/// SSH the panel is its border, never its fill. Recorded in
/// `docs/tokens-achados.md`.
pub const SURFACE: Ink = Ink {
    truecolor: (0x0A, 0x08, 0x06),
    ansi256: 232,
    ansi16: Color::Black,
};

/// Default 1px border.
pub const RULE: Ink = Ink {
    truecolor: (0x24, 0x1F, 0x19),
    ansi256: 234,
    ansi16: Color::DarkGray,
};

/// Border of the focused panel, and scale marks.
pub const RULE_STRONG: Ink = Ink {
    truecolor: (0x3A, 0x32, 0x2A),
    ansi256: 236,
    ansi16: Color::DarkGray,
};

/// Body text. Off-white, slightly yellow. Pure white is wrong (`specs/07`).
pub const BONE: Ink = Ink {
    truecolor: (0xEA, 0xE3, 0xCF),
    ansi256: 254,
    ansi16: Color::White,
};

/// Secondary labels and inactive log channels.
///
/// 4.11:1 against the surface, which passes WCAG only as large text — see
/// `docs/tokens-achados.md`. Never used alone for anything that must be read.
pub const BONE_DIM: Ink = Ink {
    truecolor: (0x7A, 0x70, 0x61),
    ansi256: 243,
    ansi16: Color::DarkGray,
};

/// NERV orange — the institutional accent. **Not** a success colour
/// (`specs/07`).
pub const NERV: Ink = Ink {
    truecolor: (0xF2, 0x52, 0x1F),
    ansi256: 202,
    ansi16: Color::Yellow,
};

/// Alert red. Reserved for error and disconnection; if it appears, something is
/// wrong (`specs/07`).
///
/// In sixteen colours this must not collide with [`NERV`], or the exclusivity
/// the theme promises disappears exactly where it matters most.
pub const ALERT: Ink = Ink {
    truecolor: (0xFF, 0x1A, 0x1A),
    ansi256: 196,
    ansi16: Color::LightRed,
};

/// Phosphor green — telemetry and nominal values.
pub const PHOSPHOR: Ink = Ink {
    truecolor: (0x6B, 0xFF, 0xB6),
    ansi256: 85,
    ansi16: Color::LightGreen,
};

/// Verified identity — PADRÃO: AZUL.
pub const PATTERN_BLUE: Ink = Ink {
    truecolor: (0x2F, 0x8F, 0xE0),
    ansi256: 32,
    ansi16: Color::LightBlue,
};

/// The theme, resolved for one terminal.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// What this terminal can show.
    pub palette: Palette,
    /// Whether to lean on bold and inverse instead of hue.
    ///
    /// True in [`Palette::Mono`], and also in [`Palette::Ansi16`] where the
    /// user's own theme decides what "red" looks like and it might be anything.
    pub high_contrast: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::detect()
    }
}

impl Theme {
    /// The theme for this terminal.
    #[must_use]
    pub fn detect() -> Self {
        let palette = Palette::detect();
        Self {
            palette,
            high_contrast: palette == Palette::Mono,
        }
    }

    /// A theme for a specific palette. Tests and `:tema`.
    #[must_use]
    pub fn with_palette(palette: Palette) -> Self {
        Self {
            palette,
            high_contrast: palette == Palette::Mono,
        }
    }

    /// Foreground style for one ink.
    #[must_use]
    pub fn fg(self, ink: Ink) -> Style {
        ink.resolve(self.palette)
            .map_or_else(Style::default, |colour| Style::default().fg(colour))
    }

    /// The app background.
    #[must_use]
    pub fn background(self) -> Style {
        BLACK
            .resolve(self.palette)
            .map_or_else(Style::default, |colour| Style::default().bg(colour))
    }

    /// Style for a panel border, focused or not.
    #[must_use]
    pub fn border(self, focused: bool) -> Style {
        let style = self.fg(if focused { RULE_STRONG } else { RULE });
        if focused && self.high_contrast {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    /// Style for a small uppercase label.
    #[must_use]
    pub fn label(self) -> Style {
        self.fg(BONE_DIM)
    }

    /// Style for body text.
    #[must_use]
    pub fn body(self) -> Style {
        self.fg(BONE)
    }

    /// Style for the institutional accent.
    #[must_use]
    pub fn accent(self) -> Style {
        self.fg(NERV).add_modifier(Modifier::BOLD)
    }

    /// Style for an alert.
    ///
    /// In mono this becomes inverse video, because "something is wrong" is the
    /// one thing that must survive every degradation.
    #[must_use]
    pub fn alert(self) -> Style {
        if self.palette == Palette::Mono {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            self.fg(ALERT).add_modifier(Modifier::BOLD)
        }
    }

    /// Style for a Sync Ratio in a given band.
    ///
    /// `design/SEELE v2.dc.html` gives each band a colour — phosphor,
    /// orange, red — and bone appears in no sync scale at all. The caller is
    /// expected to print [`Self::sync_mark`] beside it, because the number and
    /// the mark are what survive when the colour does not.
    #[must_use]
    pub fn sync(self, band: SignalBand) -> Style {
        match band {
            SignalBand::Nominal => self.fg(PHOSPHOR),
            SignalBand::Degraded => self.fg(NERV),
            SignalBand::Critical => self.alert(),
        }
    }

    /// A one-character mark for a Sync Ratio band.
    ///
    /// `specs/05-cliente-tui.md`: "Nenhuma informação transmitida **só** por
    /// cor". This is the half that is not colour, and it is drawn in every
    /// palette rather than only in mono — a mark that appears when things
    /// degrade is a mark nobody has learned to read.
    #[must_use]
    pub fn sync_mark(band: SignalBand) -> &'static str {
        match band {
            SignalBand::Nominal => "█",
            SignalBand::Degraded => "▒",
            SignalBand::Critical => "░",
        }
    }

    /// Style for verified identity — PADRÃO: AZUL.
    #[must_use]
    pub fn pattern_blue(self) -> Style {
        self.fg(PATTERN_BLUE).add_modifier(Modifier::BOLD)
    }

    /// Whether kanji accents should be drawn at all.
    ///
    /// `specs/07-tema-evangelion.md` makes kanji a typographic accent that never
    /// carries information: "um usuário que não lê japonês não perde nada". A
    /// terminal that cannot render it therefore loses nothing either, and a
    /// mojibake box in a status bar is worse than an empty one.
    #[must_use]
    pub fn kanji(self) -> bool {
        self.palette != Palette::Mono
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ink_survives_every_palette() {
        let inks = [
            BLACK,
            SURFACE,
            RULE,
            RULE_STRONG,
            BONE,
            BONE_DIM,
            NERV,
            ALERT,
            PHOSPHOR,
            PATTERN_BLUE,
        ];
        for ink in inks {
            assert!(ink.resolve(Palette::True).is_some());
            assert!(ink.resolve(Palette::Ansi256).is_some());
            assert!(ink.resolve(Palette::Ansi16).is_some());
            assert!(
                ink.resolve(Palette::Mono).is_none(),
                "mono must yield no colour at all"
            );
        }
    }

    #[test]
    fn the_values_match_the_frozen_tokens() {
        // design/seele-tokens.json is the source; this is the translation. A
        // silent drift between them is how a design stops being the design.
        assert_eq!(NERV.truecolor, (0xF2, 0x52, 0x1F));
        assert_eq!(NERV.ansi256, 202);
        assert_eq!(ALERT.truecolor, (0xFF, 0x1A, 0x1A));
        assert_eq!(ALERT.ansi256, 196);
        assert_eq!(PHOSPHOR.truecolor, (0x6B, 0xFF, 0xB6));
        assert_eq!(PHOSPHOR.ansi256, 85);
        assert_eq!(BONE.truecolor, (0xEA, 0xE3, 0xCF));
        assert_eq!(BONE.ansi256, 254);
        assert_eq!(PATTERN_BLUE.ansi256, 32);
    }

    #[test]
    fn alert_and_accent_stay_distinct_in_sixteen_colours() {
        // specs/07-tema-evangelion.md reserves red for error and disconnection:
        // "Se aparece, algo está errado". Collapsing orange onto red over SSH
        // would break that promise exactly where the user is least able to
        // check anything.
        assert_ne!(
            NERV.resolve(Palette::Ansi16),
            ALERT.resolve(Palette::Ansi16)
        );
        assert_ne!(
            PHOSPHOR.resolve(Palette::Ansi16),
            ALERT.resolve(Palette::Ansi16)
        );
    }

    #[test]
    fn the_panel_surface_collapses_onto_the_background_in_256() {
        // Not a bug, and worth pinning so nobody "fixes" it: both are the
        // nearest neighbour of the same grey, which is why specs/07 defines a
        // surface as a 1px channel around emptiness rather than a fill.
        assert_eq!(
            SURFACE.resolve(Palette::Ansi256),
            BLACK.resolve(Palette::Ansi256)
        );
        assert_ne!(SURFACE.truecolor, BLACK.truecolor);
    }

    #[test]
    fn every_sync_band_has_a_mark_of_its_own() {
        // specs/05-cliente-tui.md: nothing conveyed by colour alone. Two bands
        // sharing a mark would put the no-colour mode back where it started.
        let marks = [
            Theme::sync_mark(SignalBand::Nominal),
            Theme::sync_mark(SignalBand::Degraded),
            Theme::sync_mark(SignalBand::Critical),
        ];
        let unique: std::collections::HashSet<&&str> = marks.iter().collect();
        assert_eq!(unique.len(), 3, "two bands share a mark: {marks:?}");
    }

    #[test]
    fn an_alert_is_still_loud_without_colour() {
        // "Algo está errado" is the one thing that must survive every
        // degradation, including a monochrome terminal.
        let mono = Theme::with_palette(Palette::Mono);
        let style = mono.alert();
        assert!(style.add_modifier.contains(Modifier::REVERSED));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn mono_style_carries_no_colour_anywhere() {
        let mono = Theme::with_palette(Palette::Mono);
        for band in [SignalBand::Nominal, SignalBand::Degraded, SignalBand::Critical] {
            assert!(
                mono.sync(band).fg.is_none(),
                "a colour leaked into the no-colour mode"
            );
        }
        assert!(mono.body().fg.is_none());
        assert!(mono.background().bg.is_none());
    }

    #[test]
    fn kanji_is_dropped_where_it_cannot_be_trusted() {
        // specs/07: kanji never carries information, so dropping it loses
        // nothing — while a row of replacement boxes in a status bar is worse
        // than an empty one.
        assert!(!Theme::with_palette(Palette::Mono).kanji());
        assert!(Theme::with_palette(Palette::True).kanji());
    }
}
