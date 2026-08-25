//! What the words typed after `:` mean.
//!
//! Parsing is separated from doing so that the vocabulary in
//! `specs/05-cliente-tui.md` can be tested without a server, a sound card, or a
//! terminal. Running the commands is the `plug` binary's job.

/// A parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `:q` — sair do programa.
    Quit,
    /// `:ejetar` — leave this server and go back to the selection screen.
    ///
    /// Separate from [`Command::Quit`] on purpose: quitting the program and
    /// leaving a conversation are different things, and the app already
    /// treated the two as different with the EJETAR button.
    Eject,
    /// `:conectar <host>`.
    Connect {
        /// Whatever was typed. Resolving it is the caller's problem.
        target: String,
    },
    /// `:voice_room <nome>`.
    VoiceRoom {
        /// Name or number.
        which: String,
    },
    /// `:linha <nome>` — not in the spec's list, but a client with Lines and no
    /// way to switch between them can only ever use the first one.
    Line {
        /// Name or number.
        which: String,
    },
    /// `:sync` — detailed diagnosis.
    Sync,
    /// `:audio` — devices.
    Audio,
    /// `:tema` — cycle the palette.
    Theme {
        /// Which one, if one was named.
        which: Option<String>,
    },
    /// `:sobre`.
    About,
    /// `:convite` — mostra o link para convidar alguém.
    ///
    /// Só faz sentido em `--hospedar`: quem é convidado não tem o que oferecer.
    Convite,
    /// `:mudo` / `:surdo` — the same toggles as `m` and `d`, for people who
    /// reach for words.
    AtField,
    /// Isolamento total.
    TotalIsolation,
    /// `:voz <ptt|vad|aberto>`.
    VoiceMode {
        /// Which mode was asked for.
        which: String,
    },
    /// `:volume <pessoa> <0-200>`.
    Volume {
        /// Whose.
        who: String,
        /// Percent.
        percent: u16,
    },
    /// Something that is not a command.
    Unknown {
        /// What was typed, for the error message.
        typed: String,
    },
}

/// Parses a command line, with or without its leading colon.
///
/// Accepts the leading `:` because a person who types `:q` into a prompt that
/// already shows `:` is not making a mistake worth an error message.
#[must_use]
pub fn parse(input: &str) -> Command {
    let trimmed = input.trim().trim_start_matches(':').trim();
    let mut parts = trimmed.split_whitespace();
    let Some(head) = parts.next() else {
        return Command::Unknown {
            typed: String::new(),
        };
    };
    let rest = parts.collect::<Vec<_>>();
    let joined = rest.join(" ");

    match head {
        "q" | "quit" | "sair" => Command::Quit,
        "ejetar" | "eject" => Command::Eject,
        "conectar" | "connect" if !joined.is_empty() => Command::Connect { target: joined },
        "voice room" if !joined.is_empty() => Command::VoiceRoom { which: joined },
        "linha" | "line" if !joined.is_empty() => Command::Line { which: joined },
        "sync" => Command::Sync,
        "audio" | "áudio" => Command::Audio,
        "tema" | "theme" => Command::Theme {
            which: rest.first().map(|word| (*word).to_owned()),
        },
        "sobre" | "about" => Command::About,
        "convite" | "convidar" => Command::Convite,
        "mudo" | "at" => Command::AtField,
        "surdo" | "isolamento" => Command::TotalIsolation,
        "voz" | "voice" if !joined.is_empty() => Command::VoiceMode { which: joined },
        "volume" | "vol" => parse_volume(&rest, trimmed),
        _ => Command::Unknown {
            typed: trimmed.to_owned(),
        },
    }
}

fn parse_volume(rest: &[&str], typed: &str) -> Command {
    let ([who, percent] | [who, percent, ..]) = rest else {
        return Command::Unknown {
            typed: typed.to_owned(),
        };
    };
    percent.trim_end_matches('%').parse::<u16>().map_or_else(
        |_| Command::Unknown {
            typed: typed.to_owned(),
        },
        |percent| Command::Volume {
            who: (*who).to_owned(),
            // 200% is loud, but somebody on a bad microphone is a real
            // situation and refusing to help is not a safety feature.
            percent: percent.min(200),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_the_spec_names_all_parse() {
        // specs/05-cliente-tui.md lists exactly these.
        assert_eq!(
            parse(":conectar seele.exemplo:8383"),
            Command::Connect {
                target: "seele.exemplo:8383".into()
            }
        );
        assert_eq!(
            parse(":voice room central"),
            Command::VoiceRoom {
                which: "central".into()
            }
        );
        assert_eq!(parse(":sync"), Command::Sync);
        assert_eq!(parse(":audio"), Command::Audio);
        assert_eq!(parse(":tema"), Command::Theme { which: None });
        assert_eq!(parse(":sobre"), Command::About);
        assert_eq!(parse(":convite"), Command::Convite);
        assert_eq!(parse(":q"), Command::Quit);
    }

    #[test]
    fn ejecting_is_no_longer_quitting() {
        // Assumed behaviour change: the app's button is called EJETAR and
        // returns to the entry screen, and the terminal now does the same.
        // Quitting the program is still `:q`.
        assert_eq!(parse(":ejetar"), Command::Eject);
        assert_eq!(parse(":eject"), Command::Eject);
        assert_eq!(parse(":q"), Command::Quit);
        assert_eq!(parse(":sair"), Command::Quit);
        assert_eq!(parse(":quit"), Command::Quit);
    }

    #[test]
    fn the_colon_is_optional_because_the_prompt_already_shows_one() {
        assert_eq!(parse("q"), Command::Quit);
        assert_eq!(parse(":q"), Command::Quit);
        assert_eq!(parse("  :q  "), Command::Quit);
    }

    #[test]
    fn a_voice_room_name_may_contain_spaces() {
        assert_eq!(
            parse(":voice room VOICE_ROOM-01 CENTRAL"),
            Command::VoiceRoom {
                which: "VOICE_ROOM-01 CENTRAL".into()
            }
        );
    }

    #[test]
    fn a_command_that_needs_an_argument_and_has_none_is_not_silently_accepted() {
        // `:conectar` alone connecting to the last host would be a surprise the
        // first time somebody mistypes it.
        assert!(matches!(parse(":conectar"), Command::Unknown { .. }));
        assert!(matches!(parse(":voice room"), Command::Unknown { .. }));
    }

    #[test]
    fn volume_takes_a_person_and_a_percentage() {
        assert_eq!(
            parse(":volume ayanami 40"),
            Command::Volume {
                who: "ayanami".into(),
                percent: 40
            }
        );
        assert_eq!(
            parse(":vol ayanami 40%"),
            Command::Volume {
                who: "ayanami".into(),
                percent: 40
            }
        );
    }

    #[test]
    fn volume_is_capped_rather_than_refused() {
        assert_eq!(
            parse(":volume ayanami 9000"),
            Command::Volume {
                who: "ayanami".into(),
                percent: 200
            }
        );
    }

    #[test]
    fn nonsense_comes_back_as_nonsense_with_the_text_intact() {
        // The error message has to be able to quote what was typed.
        assert_eq!(
            parse(":voar"),
            Command::Unknown {
                typed: "voar".into()
            }
        );
    }

    #[test]
    fn an_empty_line_is_not_an_unknown_command() {
        assert_eq!(
            parse(":"),
            Command::Unknown {
                typed: String::new()
            }
        );
    }
}
