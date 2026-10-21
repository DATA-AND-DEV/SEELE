//! Finding a term in what a shell is showing.
//!
//! This lives here rather than in a shell because it would be identical in
//! both — the rule the terminal shell wrote down first. How case and accent
//! fold, and the order `n` and `N` walk in, is one decision with one set of
//! tests.
//!
//! # The accent table is Portuguese, and only Portuguese
//!
//! Folding is one character to one character, which is what keeps the offsets
//! this module hands out aligned with the text the caller passed in. Pulling
//! `unicode-normalization` into the core for twelve characters is not a trade
//! this project makes. The cost is real and stated: an accent outside
//! Portuguese does not fold.
//!
//! # Each shell passes the bodies it draws
//!
//! The offsets handed out index the exact strings the caller passed in, so the
//! only correct thing to pass is what is on screen. That is a different string
//! in each shell, and the difference is not a detail:
//!
//! - A shell that wraps text **collapses** runs of whitespace, building lines with
//!   `split_whitespace`, so a run of whitespace is already gone by the time it
//!   is drawn. It calls [`normalize`] first, and the offsets channel up.
//! - The desktop app **preserves**. `.mensagens .corpo` is `white-space:
//!   pre-wrap`, so newlines and double spaces are on screen. It passes the raw
//!   body, and the offsets channel up.
//!
//! A shell that normalises what it draws raw — or draws raw what it normalised
//! — gets ranges that are silently off by one per collapsed run, which is a
//! defect nobody sees until somebody pastes two blank channels. Answer "what
//! string is on screen?" before choosing, and never make the drawing follow the
//! search: flattening a conversation to keep an index aligned is trading the
//! product for the implementation.
//!
//! The honest cost of preserving is small and worth stating: with `' '` and
//! `'\n'` being different characters, a term containing a space does not match
//! across a channel break.

/// Where a term matched.
///
/// `Serialize` because the desktop shell sends these across the Tauri bridge:
/// with accent folding in the middle, a frontend cannot work out where a match
/// began on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Match {
    /// Index into the list of bodies, in the order the shell drew them.
    pub message: usize,
    /// Character offset where the match starts.
    pub start: usize,
    /// Character offset one past the end.
    pub end: usize,
}

/// Collapses runs of whitespace, the way a wrapping shell displays text.
///
/// For a shell that draws what it was given, this is the wrong function: see
/// the module docs. It exists for the terminal, which collapses before drawing
/// and therefore has to search the collapsed string.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Folds one character: lowercase first, then strip the Portuguese accent.
///
/// `to_lowercase` can yield more than one character for a few code points;
/// taking the first keeps this one-to-one, which is what the offsets depend on.
fn fold_char(character: char) -> char {
    let lower = character.to_lowercase().next().unwrap_or(character);
    match lower {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        other => other,
    }
}

fn fold(text: &str) -> Vec<char> {
    text.chars().map(fold_char).collect()
}

/// Every place `term` occurs in `text`, as character ranges, left to right.
///
/// Public because a shell that wraps text into segments needs to light the
/// matches inside a segment without redoing the folding rule.
#[must_use]
pub fn occurrences(text: &str, term: &str) -> Vec<(usize, usize)> {
    let target = fold(term.trim());
    if target.is_empty() {
        return Vec::new();
    }
    let body = fold(text);
    // `windows` yields nothing when the body is shorter than the term, and
    // panics only on a zero width — ruled out above.
    body.windows(target.len())
        .enumerate()
        .filter(|(_, window)| *window == target.as_slice())
        .map(|(start, _)| (start, start + target.len()))
        .collect()
}

/// A search over what a shell is showing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    matches: Vec<Match>,
    cursor: usize,
}

impl Search {
    /// Runs the term over the bodies, in drawing order.
    ///
    /// Rebuilt wholesale rather than patched, like `view::project`: an index
    /// that drifts out of step with the list is a bug that only shows up after
    /// an hour of use.
    #[must_use]
    pub fn new<S: AsRef<str>>(bodies: impl IntoIterator<Item = S>, term: &str) -> Self {
        let matches = bodies
            .into_iter()
            .enumerate()
            .flat_map(|(message, body)| {
                occurrences(body.as_ref(), term)
                    .into_iter()
                    .map(move |(start, end)| Match {
                        message,
                        start,
                        end,
                    })
            })
            .collect();
        Self { matches, cursor: 0 }
    }

    /// Whether nothing matched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Every match, in drawing order.
    ///
    /// A shell that paints the whole history at once — the desktop one does —
    /// needs all of them, not just the one the cursor is on.
    #[must_use]
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// The match the cursor is on.
    #[must_use]
    pub fn current(&self) -> Option<Match> {
        self.matches.get(self.cursor).copied()
    }

    // Named `next_match`/`previous_match` rather than `next`/`previous`: an
    // inherent `fn next(&mut self) -> Option<T>` on a type that is not an
    // `Iterator` trips `clippy::should_implement_trait`, and this workspace
    // builds with `-D warnings`.

    /// `n` — the next match, wrapping past the end.
    pub fn next_match(&mut self) -> Option<Match> {
        self.step(1)
    }

    /// `N` — the previous match, wrapping past the start.
    pub fn previous_match(&mut self) -> Option<Match> {
        self.step(-1)
    }

    fn step(&mut self, step: isize) -> Option<Match> {
        let total = self.matches.len();
        if total == 0 {
            return None;
        }
        // Wrapping on purpose: for somebody searching, the last occurrence and
        // the first are neighbours.
        let total_i = isize::try_from(total).unwrap_or(isize::MAX);
        let current = isize::try_from(self.cursor).unwrap_or(0);
        let next = (current + step).rem_euclid(total_i);
        self.cursor = usize::try_from(next).unwrap_or(0);
        self.current()
    }

    /// Puts the cursor back where it was, after the search was rebuilt.
    ///
    /// A search is rebuilt wholesale whenever the drawn history changes, and a
    /// rebuild starts at occurrence one. In a live conversation that is not a
    /// detail: anybody speaking would throw a reader sitting on match 7 of 12
    /// back to the first, which makes `n`/`N` unusable exactly where searching
    /// matters.
    ///
    /// The cursor lands on the first match at or after `previous`, in drawing
    /// order. If the occurrence it was on disappeared — an edited or deleted
    /// message — it lands on the last match rather than falling out of range.
    ///
    /// Lives here rather than in a shell because both shells rebuild, both have
    /// to hold the cursor, and a rule written twice is a rule that agrees twice
    /// and then stops agreeing.
    pub fn resume_at(&mut self, previous: Match) {
        if self.matches.is_empty() {
            self.cursor = 0;
            return;
        }
        self.cursor = self
            .matches
            .iter()
            .position(|candidate| {
                (candidate.message, candidate.start) >= (previous.message, previous.start)
            })
            .unwrap_or_else(|| self.matches.len().saturating_sub(1));
    }

    /// `(1, 3)` for drawing "[1/3]". `(0, 0)` when nothing matched.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        if self.matches.is_empty() {
            return (0, 0);
        }
        (self.cursor + 1, self.matches.len())
    }

    /// Which occurrence *within its own message* the cursor is on, from zero.
    ///
    /// A shell that lights matches segment by segment counts them the same way
    /// and needs this to tell the current one from its neighbours.
    #[must_use]
    pub fn ordinal_in_message(&self) -> Option<usize> {
        let current = self.current()?;
        Some(
            self.matches
                .iter()
                .take(self.cursor)
                .filter(|candidate| candidate.message == current.message)
                .count(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "test vectors are fixed and local")]
mod tests {
    use super::*;

    #[test]
    fn case_does_not_matter() {
        let search = Search::new(["o SYNC caiu"], "sync");
        assert_eq!(search.position(), (1, 1));
        assert_eq!(
            search.current(),
            Some(Match {
                message: 0,
                start: 2,
                end: 6
            })
        );
    }

    #[test]
    fn accent_folding_keeps_the_match_range_correct() {
        // The table is 1:1 per character, and this is exactly where it would
        // give itself away if it stopped being so: `não` has three
        // characters, and the range returned has to have three.
        let search = Search::new(["não foi"], "nao");
        let found = search.current().unwrap_or(Match {
            message: 9,
            start: 9,
            end: 9,
        });
        assert_eq!(found.start, 0);
        assert_eq!(found.end, 3);
    }

    #[test]
    fn the_accent_folds_in_both_directions() {
        assert_eq!(Search::new(["nao foi"], "não").position(), (1, 1));
        assert_eq!(Search::new(["MAIÚSCULA"], "maiuscula").position(), (1, 1));
    }

    #[test]
    fn the_cursor_wraps_at_both_ends() {
        let mut search = Search::new(["sync", "sync", "sync"], "sync");
        assert_eq!(search.position(), (1, 3));
        search.next_match();
        search.next_match();
        assert_eq!(search.position(), (3, 3));
        // From the end, `n` wraps back to the start: in a conversation the
        // last occurrence and the first are neighbours for someone searching.
        search.next_match();
        assert_eq!(search.position(), (1, 3));
        search.previous_match();
        assert_eq!(search.position(), (3, 3));
    }

    #[test]
    fn multiple_occurrences_in_the_same_message_count_separately() {
        let search = Search::new(["sync e sync"], "sync");
        assert_eq!(search.position(), (1, 2));
        assert_eq!(search.ordinal_in_message(), Some(0));
    }

    #[test]
    fn the_ordinal_counts_within_the_message_and_not_the_total() {
        let mut search = Search::new(["sync", "sync e sync"], "sync");
        search.next_match();
        assert_eq!(search.current().map(|c| c.message), Some(1));
        assert_eq!(search.ordinal_in_message(), Some(0));
        search.next_match();
        assert_eq!(search.ordinal_in_message(), Some(1));
    }

    #[test]
    fn resuming_lands_on_the_first_match_at_or_after_the_previous_one() {
        // Somebody sitting on the second of three occurrences, and a message
        // arriving above them: every index moved by one, and the cursor has to
        // move with it instead of snapping back to the first.
        let mut before = Search::new(["sync", "sync", "sync"], "sync");
        before.next_match();
        assert_eq!(before.position(), (2, 3));
        let Some(was_on) = before.current() else {
            panic!("a search with three matches has a current one");
        };

        let mut after = Search::new(["nova", "sync", "sync", "sync"], "sync");
        // The occurrence that was message 1 is message 2 now, so the raw
        // position is what moves; what must not move is the occurrence.
        after.resume_at(Match {
            message: was_on.message + 1,
            ..was_on
        });
        assert_eq!(after.position(), (2, 3));
    }

    #[test]
    fn resuming_past_the_end_lands_on_the_last_match() {
        // The message the cursor was in was deleted, or edited until the term
        // left it. Falling out of range would be an index nobody can draw. Three
        // matches, not one: with a single match, cursor 0 is simultaneously
        // "first" and "last", and a `resume_at` that is a complete no-op would
        // pass just as well as the fallback this test exists to prove.
        let mut after = Search::new(["sync", "sync", "sync"], "sync");
        after.resume_at(Match {
            message: 40,
            start: 0,
            end: 4,
        });
        assert_eq!(after.position(), (3, 3));
    }

    #[test]
    fn resuming_into_nothing_does_not_panic() {
        let mut after = Search::new(["nada aqui"], "sync");
        after.resume_at(Match {
            message: 3,
            start: 1,
            end: 5,
        });
        assert!(after.is_empty());
        assert_eq!(after.position(), (0, 0));
    }

    #[test]
    fn an_empty_or_whitespace_only_term_matches_nothing() {
        assert!(Search::new(["sync"], "").is_empty());
        assert!(Search::new(["sync"], "   ").is_empty());
        assert_eq!(Search::new(["sync"], "").position(), (0, 0));
    }

    #[test]
    fn no_match_is_not_an_error_or_a_panic() {
        let mut search = Search::new(["sync caiu"], "harmônicos");
        assert!(search.is_empty());
        assert_eq!(search.current(), None);
        assert_eq!(search.next_match(), None);
        assert_eq!(search.previous_match(), None);
        assert_eq!(search.position(), (0, 0));
    }

    #[test]
    fn a_term_longer_than_the_body_does_not_overflow() {
        assert!(Search::new(["oi"], "oi mesmo").is_empty());
        assert!(Search::new([""], "oi").is_empty());
    }

    #[test]
    fn normalize_collapses_whitespace_the_way_the_tui_wrap_does() {
        // A wrapping shell uses `split_whitespace`, which collapses. Without
        // this normalisation the offsets would point to the wrong place in any
        // body with a double space.
        assert_eq!(normalize("a  b\tc\nd "), "a b c d");
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn occurrences_finds_all_matches_in_order() {
        assert_eq!(occurrences("sync e sync", "sync"), vec![(0, 4), (7, 11)]);
        assert_eq!(occurrences("nada", "sync"), Vec::new());
    }
}
