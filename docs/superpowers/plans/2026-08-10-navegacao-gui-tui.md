# Navegação completa na TUI e na GUI — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fechar a navegação das duas cascas — busca que busca de verdade, foco entre painéis, ejetar sem matar o processo no `plug`, e a tela de entrada com Dogmas visitados e convite colado no app.

**Architecture:** O que seria idêntico nas duas cascas desce para `seele-core::busca`, um módulo puro. A TUI ganha teclas e um laço externo em volta da sessão; o app ganha seis comandos Tauri finos e a metade que faltava (gravar em `conhecidos` depois de conectar). Nenhuma lógica de protocolo entra em JavaScript.

**Tech Stack:** Rust 2024, ratatui + crossterm (TUI), Tauri 2 + HTML/CSS/JS sem framework e sem npm (app), tokio.

**Spec:** `docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md`

## Global Constraints

- **Lints do workspace são fatais no CI** (`Cargo.toml:29-43`, CI roda clippy com `-D warnings`): `unsafe_code = "forbid"`, `missing_docs = "warn"`, `unreachable_pub = "warn"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `dbg_macro = "deny"`, `indexing_slicing = "warn"`. **Não indexe nem fatie slices** — use `.get()`, `.windows()`, iteradores. `unwrap_or` e `unwrap_or_else` são permitidos; `unwrap()` e `expect()` não.
- **Relaxamento em testes** é por crate com `cfg_attr`, seguindo o que o crate já faz. Veja `apps/seele-app/tests/frontend.rs:14` para o padrão.
- **Idioma** (`specs/10-convencoes.md` + ADR 0013): código, identificadores e comentários em inglês; documentação e specs em português. **Exceção real e deliberada deste plano:** `seele-tui/src/selecao.rs` e as partes novas do `main.rs` do app estão em português. Cada arquivo tocado mantém o idioma que já usa. Código novo em `seele-core` segue o inglês do crate.
- **ADR 0002 — regra de dependência:** uma casca vê `seele-core` e mais nada. Verificado por `cargo xtask check-deps`.
- **`specs/06-clientes-gui.md:19` — inegociável:** nenhuma lógica de protocolo em JavaScript. O teste `the_frontend_never_names_a_protocol_concept` em `apps/seele-app/tests/frontend.rs:171` guarda isso.
- **`specs/05-cliente-tui.md:105`:** nenhuma informação transmitida **só** por cor. Todo realce precisa de acompanhante textual.
- Comandos de verificação: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.

---

### Task 1: `seele_core::search`

Módulo puro, sem I/O. É a peça que as duas cascas consomem, e por isso vem primeiro.

**Files:**
- Create: `crates/seele-core/src/search.rs`
- Modify: `crates/seele-core/src/lib.rs` (lista de módulos, linhas 27-34)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `seele_core::search::normalize(text: &str) -> String`
  - `seele_core::search::occurrences(text: &str, term: &str) -> Vec<(usize, usize)>`
  - `seele_core::search::Match { message: usize, start: usize, end: usize }` (Copy, PartialEq, Eq, Debug, Clone, **`serde::Serialize`** — a Task 7 o manda pela ponte do Tauri)
  - `seele_core::search::Search` com `new<S: AsRef<str>>(bodies: impl IntoIterator<Item = S>, term: &str) -> Search`, `next_match(&mut self) -> Option<Match>`, `previous_match(&mut self) -> Option<Match>`, `current(&self) -> Option<Match>`, `position(&self) -> (usize, usize)`, `ordinal_in_message(&self) -> Option<usize>`, `is_empty(&self) -> bool`, `matches(&self) -> &[Match]`

**Por que a assinatura mudou em relação à spec.** A spec registrou `new<'a>(bodies: impl Iterator<Item = &'a str>, …)`. Não serve: as cascas precisam passar corpos **normalizados**, que são `String` recém-criadas e não sobrevivem como `&'a str`. `S: AsRef<str>` aceita as duas formas. A spec é corrigida no Passo 7 desta tarefa.

- [ ] **Step 1: Escrever os testes que falham**

Crie `crates/seele-core/src/search.rs` com **apenas** o bloco de testes abaixo (o código vem no Passo 3):

```rust
#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "test vectors are fixed and local")]
mod tests {
    use super::*;

    #[test]
    fn case_does_not_matter() {
        let search = Search::new(["o SYNC caiu"], "sync");
        assert_eq!(search.position(), (1, 1));
        assert_eq!(search.current(), Some(Match { message: 0, start: 2, end: 6 }));
    }

    #[test]
    fn accent_folding_keeps_the_match_range_correct() {
        // The table is 1:1 per character, and this is exactly where it would
        // give itself away if it stopped being so: `não` has three
        // characters, and the range returned has to have three.
        let search = Search::new(["não foi"], "nao");
        let found = search.current().unwrap_or(Match { message: 9, start: 9, end: 9 });
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
        // `seele-tui::ui::wrap` uses `split_whitespace`, which collapses. Without
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core search`
Expected: FAIL na compilação — `cannot find type Search in this scope`, `cannot find function normalize`.

- [ ] **Step 3: Escrever a implementação**

Coloque **acima** do bloco de testes em `crates/seele-core/src/search.rs`:

```rust
//! Finding a term in what a shell is showing.
//!
//! This lives here rather than in a shell because it would be identical in
//! both — the rule `seele-tui::view` already wrote down. How case and accent
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
//! # Callers pass normalised bodies
//!
//! [`normalize`] collapses whitespace the same way `seele-tui::ui::wrap` does
//! and the way HTML does. Both shells already show text with runs of
//! whitespace collapsed, so searching the normalised body is searching exactly
//! what is on screen.

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

/// Collapses runs of whitespace, the way both shells already display text.
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
                    .map(move |(start, end)| Match { message, start, end })
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
```

- [ ] **Step 4: Registrar o módulo e conferir o `serde`**

Em `crates/seele-core/src/lib.rs`, na lista de módulos (linhas 27-34), acrescente em ordem alfabética, depois de `pub mod battery;`:

```rust
pub mod search;
```

`Match` deriva `serde::Serialize`. Confirme que o crate já tem a dependência:

Run: `grep -n "^serde" crates/seele-core/Cargo.toml`
Expected: uma linha com `serde`. Se não houver, acrescente `serde = { workspace = true }` seguindo o padrão dos outros crates — **nunca** com versão literal, porque `Cargo.toml:45` diz que versões vivem só no workspace.

- [ ] **Step 5: Rodar os testes**

Run: `cargo test -p seele-core search`
Expected: PASS, 11 testes.

- [ ] **Step 6: Clippy e formato**

Run: `cargo clippy -p seele-core --all-targets -- -D warnings && cargo fmt --all --check`
Expected: sem saída.

- [ ] **Step 7: Corrigir a assinatura na spec**

Em `docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md`, troque a linha da assinatura por:

```rust
    /// Os corpos já normalizados, na ordem em que a casca os desenha.
    pub fn new<S: AsRef<str>>(bodies: impl IntoIterator<Item = S>, term: &str) -> Self;
```

E acrescente, logo abaixo do parágrafo que começa com "**Entra por corpos**":

```markdown
**E os corpos entram normalizados.** `seele-tui::ui::wrap` quebra com
`split_whitespace`, que colapsa espaço repetido; HTML colapsa sozinho. As duas
cascas já mostram o texto colapsado, então `search::normalize` no meio é o que
faz o deslocamento devolvido apontar para o que está na tela. Sem isso, um
casamento depois de um espaço duplo apontaria para o lugar errado só na TUI.
```

- [ ] **Step 8: ADR sobre o idioma dentro do `seele-core`**

Crie `docs/adr/0023-idioma-dentro-do-seele-core.md`, seguindo a forma dos ADRs vizinhos (leia `docs/adr/0013-idioma-de-manifestos-e-ci.md` primeiro — é curto e é o mais próximo).

Conteúdo, em português como manda `specs/10-convencoes.md` para ADRs:

**Estado:** aceito. **Contexto:** `specs/10` manda identificadores e comentários em inglês, e o `seele-core` obedece em `state.rs`, `battery.rs`, `tofu.rs` e `voice.rs` — mas `conhecidos.rs` e `enlace.rs`, os dois módulos mais recentes, são inteiramente portugueses, identificadores e doc. A deriva nunca foi decidida; aconteceu. **Decisão:** módulos novos do `seele-core` seguem `specs/10` — inglês. `conhecidos` e `enlace` ficam como estão: renomeá-los agora tocaria as duas cascas e o `seele-ffi` por uma questão de coerência, e o custo não paga. **Consequências:** o crate fica com dois sotaques por um tempo, e isto fica escrito para o próximo módulo não re-litigar. Quem renomear `conhecidos` ou `enlace` algum dia faz isso como trabalho próprio, não de passagem.

- [ ] **Step 9: Commit**

```bash
git add crates/seele-core/src/search.rs crates/seele-core/src/lib.rs docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md docs/adr/0023-idioma-dentro-do-seele-core.md
git commit -m "feat(core): achar um termo no que está escrito na tela

Dobra caixa e acento numa tabela 1:1, que é o que mantém o intervalo
devolvido alinhado com o texto que entrou. A tabela é do português e só
dele — puxar \`unicode-normalization\` para o core por doze caracteres não
é troca que este projeto faz, e o limite fica dito no doc em vez de
fingido.

Os corpos entram normalizados porque o \`wrap\` da TUI colapsa espaço com
\`split_whitespace\` e o HTML colapsa sozinho: as duas cascas já mostram o
texto colapsado, e buscar nele é buscar no que está na tela."
```

---

### Task 2: TUI — foco entre painéis

**Files:**
- Modify: `crates/seele-tui/src/app.rs` (`Panel` em 44-65, `Key` em 538, `on_normal` em 386-415)
- Modify: `crates/seele-tui/src/main.rs:724-738` (tradução de tecla)

**Interfaces:**
- Consumes: nada da Task 1.
- Produces: `Panel::prev(self) -> Panel`; variante `Key::BackTab`.

- [ ] **Step 1: Escrever os testes que falham**

Acrescente ao `mod tests` de `crates/seele-tui/src/app.rs`:

```rust
#[test]
fn h_and_l_move_between_panels_and_wrap() {
    // `specs/05-cliente-tui.md:42` promises "h j k l / setas navegar", and
    // until now only j and k did anything.
    let mut app = App::new();
    app.focus = Panel::Dogma;

    app.on_key(Key::Char('l'));
    assert_eq!(app.focus, Panel::Channels);
    app.on_key(Key::Right);
    assert_eq!(app.focus, Panel::Messages);
    // Wraps around, the way Tab already does.
    app.on_key(Key::Char('l'));
    assert_eq!(app.focus, Panel::Dogma);

    app.on_key(Key::Char('h'));
    assert_eq!(app.focus, Panel::Messages);
    app.on_key(Key::Left);
    assert_eq!(app.focus, Panel::Channels);
}

#[test]
fn shift_tab_closes_the_cycle_tab_opens() {
    let mut app = App::new();
    let inicio = app.focus;
    app.on_key(Key::Tab);
    assert_ne!(app.focus, inicio);
    app.on_key(Key::BackTab);
    assert_eq!(app.focus, inicio);
}

#[test]
fn h_and_l_do_not_escape_insert_mode() {
    // The letter `l` inside a message is a letter, not a focus command.
    let mut app = App::new();
    app.on_key(Key::Char('i'));
    let foco = app.focus;
    app.on_key(Key::Char('l'));
    assert_eq!(app.focus, foco);
    assert_eq!(app.input, "l");
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-tui --lib app::tests`
Expected: FAIL — `no variant named BackTab found for enum Key`.

- [ ] **Step 3: Implementar**

Em `crates/seele-tui/src/app.rs`, dentro de `impl Panel` (depois de `next`, linha 63):

```rust
    /// The previous panel, for `Shift+Tab` and `h`.
    ///
    /// A three-panel ring where going back costs two presses is a ring nobody
    /// goes back in.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Dogma => Self::Messages,
            Self::Channels => Self::Dogma,
            Self::Messages => Self::Channels,
        }
    }
```

No `enum Key`, depois de `Tab` (linha ~546):

```rust
    /// Shift+Tab.
    BackTab,
```

Em `on_normal`, substitua a linha `Key::Tab => self.focus = self.focus.next(),` por:

```rust
            Key::Tab => self.focus = self.focus.next(),
            Key::BackTab => self.focus = self.focus.prev(),
            // `h`/`l` move the focus and not the selection: with three panels
            // side by side that is the natural reading of left and right, and
            // `j`/`k` already cover movement inside a panel.
            Key::Char('h') | Key::Left => self.focus = self.focus.prev(),
            Key::Char('l') | Key::Right => self.focus = self.focus.next(),
```

Em `crates/seele-tui/src/main.rs`, no `match event.code` (linha 725), depois de `KeyCode::Tab => Key::Tab,`:

```rust
        KeyCode::BackTab => Key::BackTab,
```

- [ ] **Step 4: Rodar os testes**

Run: `cargo test -p seele-tui`
Expected: PASS. Se algum teste antigo de `Left`/`Right` quebrar, ele afirmava que a tecla não fazia nada — atualize-o para a nova verdade.

- [ ] **Step 5: Atualizar a ajuda**

`?` lista os atalhos. Em `crates/seele-tui/src/ui.rs`, ache o texto da sobreposição de ajuda (`grep -n "ajuda\|Tab " crates/seele-tui/src/ui.rs`) e garanta que `h l ← →` e `Shift+Tab` aparecem. Uma ajuda que omite uma tecla que existe é a mesma classe de defeito que este plano está consertando.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-tui/src/app.rs crates/seele-tui/src/main.rs crates/seele-tui/src/ui.rs
git commit -m "feat(tui): h, l e Shift+Tab movem o foco entre os painéis

\`specs/05\` promete \"h j k l / setas navegar\" desde sempre; só j e k
faziam alguma coisa, e Left/Right chegavam ao \`on_normal\` para cair no
\`_ => {}\`. Com três painéis lado a lado, esquerda e direita movem o
foco, e o ciclo do Tab ganha o par que faltava."
```

---

### Task 3: TUI — a busca ligada ao histórico

**Files:**
- Modify: `crates/seele-tui/src/app.rs` (campo novo em `App`, `on_key`, `on_normal`)

**Interfaces:**
- Consumes: `seele_core::search::{Search, normalize}` da Task 1.
- Produces: `App::busca: Option<Search>`, público; `App::refazer_busca(&mut self)`, público.

- [ ] **Step 1: Escrever os testes que falham**

Acrescente ao `mod tests` de `crates/seele-tui/src/app.rs`:

```rust
fn app_with_history() -> App {
    let mut app = App::new();
    app.messages = ["sync caiu", "verificando harmônicos", "sync voltou"]
        .into_iter()
        .map(|corpo| ChatLine {
            at: "12:00".into(),
            author: "piloto".into(),
            body: corpo.into(),
            own: false,
        })
        .collect();
    app
}

#[test]
fn the_search_finds_matches_while_typing() {
    // This is the defect the plan exists to fix: the mode was entered, the
    // bar wrote BUSCA, and the text was discarded without anyone looking at
    // it.
    let mut app = app_with_history();
    app.on_key(Key::Char('/'));
    for character in "sync".chars() {
        app.on_key(Key::Char(character));
    }
    let busca = app.busca.as_ref().map(seele_core::search::Search::position);
    assert_eq!(busca, Some((1, 2)));
}

#[test]
fn n_and_shift_n_step_through_occurrences_in_normal_mode() {
    let mut app = app_with_history();
    app.on_key(Key::Char('/'));
    for character in "sync".chars() {
        app.on_key(Key::Char(character));
    }
    app.on_key(Key::Enter);
    assert_eq!(app.mode, Mode::Normal);

    app.on_key(Key::Char('n'));
    assert_eq!(app.busca.as_ref().map(seele_core::search::Search::position), Some((2, 2)));
    app.on_key(Key::Char('N'));
    assert_eq!(app.busca.as_ref().map(seele_core::search::Search::position), Some((1, 2)));
}

#[test]
fn enter_keeps_the_highlight_and_escape_clears_it() {
    let mut app = app_with_history();
    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Enter);
    assert!(app.busca.is_some(), "confirming a search must not erase it");

    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Esc);
    assert!(app.busca.is_none(), "giving up erases it");
}

#[test]
fn n_without_an_active_search_does_nothing_and_does_not_panic() {
    let mut app = app_with_history();
    assert_eq!(app.on_key(Key::Char('n')), None);
    assert!(app.busca.is_none());
}

#[test]
fn clearing_the_term_down_to_nothing_clears_the_search() {
    // `refazer_busca` with an empty term zeroes out the whole search, and does
    // not leave an empty search standing: a dangling [0/0] counter after
    // clearing everything would say a search was still in progress.
    let mut app = app_with_history();
    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Backspace);
    assert!(app.busca.is_none());
}

#[test]
fn a_new_message_during_a_search_is_rematched() {
    // Indices shift when a new message arrives; redoing the search is what
    // stops the cursor from pointing at a line that moved.
    let mut app = app_with_history();
    app.on_key(Key::Char('/'));
    for character in "sync".chars() {
        app.on_key(Key::Char(character));
    }
    app.on_key(Key::Enter);
    app.messages.push(ChatLine {
        at: "12:05".into(),
        author: "rei".into(),
        body: "sync estável".into(),
        own: false,
    });
    app.refazer_busca();
    assert_eq!(app.busca.as_ref().map(seele_core::search::Search::position), Some((1, 3)));
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-tui --lib app::tests`
Expected: FAIL — `no field busca on type App`.

- [ ] **Step 3: Implementar**

Em `crates/seele-tui/src/app.rs`, acrescente ao `struct App` depois de `pub input: String,` (linha 245):

```rust
    /// The current search, when there is one.
    ///
    /// Stored here rather than recomputed every frame because the cursor for
    /// `n` and `N` is state: recomputing would lose which occurrence the
    /// person was on.
    pub busca: Option<seele_core::search::Search>,
    /// The term that produced `busca`, kept to redraw the highlight.
    pub termo: String,
```

Em `App::new`, depois de `input: String::new(),`:

```rust
            busca: None,
            termo: String::new(),
```

Acrescente ao `impl App`:

```rust
    /// Redoes the search over the current history, keeping the term.
    ///
    /// Called when a message arrives: indices shift, and a cursor that does
    /// not keep up points at the wrong line. If the current occurrence
    /// disappeared, the cursor goes back to the first instead of falling out
    /// of range.
    pub fn refazer_busca(&mut self) {
        if self.termo.trim().is_empty() {
            self.busca = None;
            return;
        }
        self.busca = Some(seele_core::search::Search::new(
            self.messages
                .iter()
                .chain(&self.local)
                .map(|linha| seele_core::search::normalize(&linha.body)),
            &self.termo,
        ));
    }
```

Em `on_normal`, acrescente antes de `Key::Enter =>`:

```rust
            // `n` and `N` were free, and it is where Vim puts them.
            Key::Char('n') => {
                if let Some(busca) = self.busca.as_mut() {
                    busca.next_match();
                }
            }
            Key::Char('N') => {
                if let Some(busca) = self.busca.as_mut() {
                    busca.previous_match();
                }
            }
```

Em `on_key`, substitua o braço `Mode::Search` inteiro (linhas 379-383) por:

```rust
            // The search finds matches while typing, and the counter moves
            // along with it: it is the feedback that says whether it is worth
            // continuing to type. `Enter` confirms and returns to Normal with
            // the highlight in place; `Esc` gives up and clears it.
            Mode::Search => {
                let desistiu = key == Key::Esc;
                let resultado = self.on_typing(key, Action::Command);
                if desistiu {
                    self.busca = None;
                    self.termo.clear();
                } else if self.mode == Mode::Search {
                    self.termo.clone_from(&self.input);
                    self.refazer_busca();
                } else if let Some(Action::Command(termo)) = resultado {
                    self.termo = termo;
                    self.refazer_busca();
                }
                None
            }
```

**Cuidado.** `on_typing` devolve a ação **só** quando o texto não está vazio, e limpa `self.input` no `Enter`. Por isso o ramo do `Enter` lê o termo do resultado e não de `self.input`.

Em `on_normal`, no braço `Key::Char('/')`, acrescente a limpeza do termo anterior:

```rust
            Key::Char('/') => {
                self.mode = Mode::Search;
                self.input.clear();
                self.termo.clear();
                self.busca = None;
            }
```

- [ ] **Step 4: Rodar os testes**

Run: `cargo test -p seele-tui`
Expected: PASS.

- [ ] **Step 5: Ligar o refazer ao fluxo**

Em `crates/seele-tui/src/main.rs`, no laço de eventos, toda vez que `view::project(&runtime.room, &mut runtime.app)` é chamado o histórico pode ter mudado. Acrescente `runtime.app.refazer_busca();` imediatamente depois de **cada** chamada de `view::project` no arquivo.

Run: `grep -n "view::project(&runtime.room" crates/seele-tui/src/main.rs`
Confirme que cada ocorrência ganhou a linha seguinte.

- [ ] **Step 6: Clippy e commit**

```bash
cargo clippy -p seele-tui --all-targets -- -D warnings && cargo fmt --all --check
git add crates/seele-tui/src/app.rs crates/seele-tui/src/main.rs
git commit -m "feat(tui): a busca passa a buscar

O modo entrava, a barra escrevia BUSCA, o texto ia para \`App::input\` e
era descartado sem ninguém olhar. O comentário no \`on_key\` afirmava que
ela filtrava a lista de mensagens; não havia filtragem nenhuma no
\`ui.rs\`. Uma interface que promete em voz alta o que não faz é pior do
que uma que não promete nada.

Agora acha enquanto se digita, \`n\` e \`N\` andam entre as ocorrências, e
mensagem que chega refaz a conta — senão o cursor aponta para uma linha
que mudou de lugar."
```

---

### Task 4: TUI — desenhar o realce e o contador

**Files:**
- Modify: `crates/seele-tui/src/ui.rs` (`render_messages` em 402, `message_lines` em 428, `compose_line` em 448)

**Interfaces:**
- Consumes: `App::busca`, `App::termo` da Task 3; `seele_core::search::occurrences` da Task 1.
- Produces: nada que outra tarefa consuma.

- [ ] **Step 1: Escrever o teste que falha**

Acrescente ao `mod tests` de `crates/seele-tui/src/ui.rs`, seguindo o padrão de teste de tela que já existe no arquivo (veja o teste em torno da linha 879, que faz `assert!(screen.contains("MENSAGENS"))`):

```rust
#[test]
fn the_search_shows_the_counter_and_marks_the_current_line() {
    // `specs/05-cliente-tui.md:105`: nothing may be conveyed by colour alone.
    // The counter is the highlight's textual companion, and it is what
    // survives NO_COLOR and a 16-colour SSH terminal.
    let mut app = App::new();
    app.messages = vec![ChatLine {
        at: "12:03".into(),
        author: "shinji".into(),
        body: "o sync caiu aqui".into(),
        own: false,
    }];
    app.mode = Mode::Search;
    app.input = "sync".into();
    app.termo = "sync".into();
    app.refazer_busca();

    let screen = draw(&app);
    assert!(screen.contains("[1/1]"), "{screen}");
}

#[test]
fn a_search_with_no_results_says_zero_instead_of_disappearing() {
    let mut app = App::new();
    app.messages = vec![ChatLine {
        at: "12:03".into(),
        author: "shinji".into(),
        body: "o sync caiu aqui".into(),
        own: false,
    }];
    app.mode = Mode::Search;
    app.input = "harmônicos".into();
    app.termo = "harmônicos".into();
    app.refazer_busca();

    let screen = draw(&app);
    assert!(screen.contains("[0/0]"), "{screen}");
}
```

**Nota para quem implementa:** o helper `draw(&app)` já existe no `mod tests` do `ui.rs` (é o que os testes de tela usam). Se o nome local for outro, use o que o arquivo já usa — não crie um segundo.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-tui --lib ui::tests`
Expected: FAIL — a tela não contém `[1/1]`.

- [ ] **Step 3: O contador na linha de composição**

Em `crates/seele-tui/src/ui.rs`, no fim de `compose_line`, antes de montar o `Paragraph`, acrescente o sufixo quando houver busca:

```rust
    // The counter is the highlight's textual half. Without it, "where am I
    // among the three occurrences" would be information conveyed by colour
    // alone, which `specs/05:105` forbids.
    let contador = app.busca.as_ref().map(|busca| {
        let (posicao, total) = busca.position();
        format!("  [{posicao}/{total}]")
    });
```

`compose_line` hoje devolve `Paragraph::new(...)` a partir de um único texto. Troque por uma `Line` de spans, para o contador ter estilo próprio:

```rust
    let mut spans = vec![
        Span::styled(prefix.to_owned(), style),
        Span::styled(shown, theme.body()),
    ];
    if let Some(contador) = contador {
        spans.push(Span::styled(contador, theme.label()));
    }
    Paragraph::new(Line::from(spans))
```

Mantenha o cálculo de `room` como está: o orçamento da cauda visível continua sendo o do texto, e o contador é curto o bastante para caber depois dele em 80 colunas.

- [ ] **Step 4: O realce dentro das linhas**

Substitua `message_lines` por uma versão que recebe o termo e se a mensagem é a corrente:

```rust
fn message_lines(
    message: &ChatLine,
    budget: usize,
    theme: Theme,
    term: &str,
    corrente: Option<usize>,
) -> Vec<Line<'static>> {
    let header = format!("{} {}", message.at, message.author);
    let mut lines = vec![Line::from(Span::styled(
        truncate(&header, budget),
        if message.own { theme.accent() } else { theme.label() },
    ))];

    // Counts occurrences in order, exactly the way the core counts them, to
    // know which one is the current one. Both passes go left to right over
    // the same normalised text, so the ordinals line up.
    let mut vistas = 0usize;
    for wrapped in wrap(&message.body, budget.saturating_sub(2)) {
        lines.push(Line::from(realcar(
            &wrapped,
            term,
            theme,
            corrente,
            &mut vistas,
        )));
    }
    lines
}

/// An already-wrapped segment, split into lit and unlit pieces.
///
/// The highlight is applied per segment, and not by offset into the whole
/// body, because `wrap` collapses whitespace with `split_whitespace` and an
/// offset computed on the raw body would point at the wrong place after a
/// double space.
fn realcar(
    segmento: &str,
    term: &str,
    theme: Theme,
    corrente: Option<usize>,
    vistas: &mut usize,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("  ".to_owned(), theme.body())];
    if term.trim().is_empty() {
        spans.push(Span::styled(segmento.to_owned(), theme.body()));
        return spans;
    }

    let characters: Vec<char> = segmento.chars().collect();
    let mut cursor = 0usize;
    for (start, end) in seele_core::search::occurrences(segmento, term) {
        let antes: String = characters.iter().skip(cursor).take(start - cursor).collect();
        if !antes.is_empty() {
            spans.push(Span::styled(antes, theme.body()));
        }
        let aceso: String = characters.iter().skip(start).take(end - start).collect();
        let esta = corrente == Some(*vistas);
        spans.push(Span::styled(
            aceso,
            if esta { theme.accent() } else { theme.label() },
        ));
        *vistas += 1;
        cursor = end;
    }
    let resto: String = characters.iter().skip(cursor).collect();
    if !resto.is_empty() {
        spans.push(Span::styled(resto, theme.body()));
    }
    spans
}
```

Em `render_messages`, troque o laço que monta as linhas por um que sabe qual mensagem é a corrente:

```rust
    let corrente = app.busca.as_ref().and_then(seele_core::search::Search::current);
    let ordinal = app
        .busca
        .as_ref()
        .and_then(seele_core::search::Search::ordinal_in_message);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (indice, message) in app.messages.iter().chain(&app.local).enumerate() {
        let nesta = corrente.filter(|candidate| candidate.message == indice);
        lines.extend(message_lines(
            message,
            budget,
            theme,
            &app.termo,
            nesta.and(ordinal),
        ));
    }
```

- [ ] **Step 5: Rolar até a ocorrência**

Em `render_messages`, o `skip` hoje mostra sempre a cauda. Com busca ativa e uma ocorrência fora da cauda, a rolagem tem que ir até ela. Depois do cálculo de `skip`, acrescente:

```rust
    // With a search active, the tail stops being what matters: what matters
    // is where the term is. An occurrence off screen that nobody scrolls to
    // is an occurrence that was not found.
    let skip = match linha_da_corrente {
        Some(linha) if app.busca.is_some() => linha.saturating_sub(visible / 2),
        _ => skip,
    };
```

Para isso, `message_lines` precisa dizer em que linha desenhada a corrente caiu. Acumule enquanto monta: guarde `linha_da_corrente = Some(lines.len())` **antes** de estender com a mensagem que contém `corrente`.

- [ ] **Step 6: Rodar os testes**

Run: `cargo test -p seele-tui && cargo clippy -p seele-tui --all-targets -- -D warnings`
Expected: PASS, sem avisos.

**Conferência a olho: não é sua.** O controlador roda `cargo run -p seele-tui --example telas` depois desta tarefa. Não tente abrir um terminal interativo.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-tui/src/ui.rs
git commit -m "feat(tui): o termo acende, e o contador diz onde você está

O realce é por segmento já quebrado e não por deslocamento no corpo: o
\`wrap\` colapsa espaço com \`split_whitespace\`, e uma conta feita no corpo
cru erraria o alvo depois de um espaço duplo.

O contador [1/3] não é enfeite. \`specs/05:105\` proíbe informação
transmitida só por cor, e ele é o que sobrevive ao NO_COLOR e aos 16
cores de um terminal por SSH."
```

---

### Task 5: TUI — `:ejetar` volta à tela de seleção

**Files:**
- Modify: `crates/seele-tui/src/command.rs` (`Command` em 9-65, `parse` em 84, testes em 148)
- Modify: `crates/seele-tui/src/app.rs` (`Action`, linha 195)
- Modify: `crates/seele-tui/src/main.rs` (`run` em 360, `run_command` em 850)

**Interfaces:**
- Consumes: nada das tarefas anteriores.
- Produces: `Command::Eject`; `App::ejetar(&mut self)` e `App::ejetou: bool`.

- [ ] **Step 1: Escrever os testes que falham**

Em `crates/seele-tui/src/command.rs`, no `mod tests`:

```rust
#[test]
fn ejecting_is_no_longer_quitting() {
    // Assumed behaviour change: the app's button is called EJETAR and
    // returns to the entry screen, and the terminal now does the same.
    // Quitting the program is still `:q`.
    assert_eq!(parse(":ejetar"), Command::Eject);
    assert_eq!(parse(":q"), Command::Quit);
    assert_eq!(parse(":sair"), Command::Quit);
    assert_eq!(parse(":quit"), Command::Quit);
}
```

E **corrija** o teste existente `the_commands_the_spec_names_all_parse`, que hoje afirma `parse(":q") == Command::Quit` — essa parte continua verdadeira e fica como está. Procure por qualquer asserção que trate `ejetar` como `Quit` e atualize-a.

Em `crates/seele-tui/src/app.rs`, no `mod tests`:

```rust
#[test]
fn ejecting_and_quitting_are_different_states() {
    let mut app = App::new();
    app.ejetar();
    assert!(app.ejetou, "ejetar marca a volta à seleção");
    assert!(!app.quit, "ejetar não é sair do programa");
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-tui`
Expected: FAIL — `no variant named Eject`, `no method named ejetar`.

- [ ] **Step 3: Implementar o comando**

Em `crates/seele-tui/src/command.rs`, no `enum Command`, depois de `Quit`:

```rust
    /// `:ejetar` — leave this Dogma and go back to the selection screen.
    ///
    /// Separate from [`Command::Quit`] on purpose: quitting the program and
    /// leaving a conversation are different things, and the app already
    /// treated the two as different with the EJETAR button.
    Eject,
```

Na função `parse`, substitua a linha 84 por:

```rust
        "q" | "quit" | "sair" => Command::Quit,
        "ejetar" | "eject" => Command::Eject,
```

- [ ] **Step 4: Implementar o estado**

Em `crates/seele-tui/src/app.rs`, no `struct App`, depois de `pub quit: bool,`:

```rust
    /// Set when the session has ended but the program keeps running.
    pub ejetou: bool,
```

Em `App::new`, depois de `quit: false,`:

```rust
            ejetou: false,
```

No `impl App`, depois de `quit`:

```rust
    /// Leaves this Dogma without quitting the program. `:ejetar`.
    pub fn ejetar(&mut self) {
        self.ejetou = true;
    }
```

**Não acrescente uma variante a `Action`.** `:ejetar` chega por `Command::Eject` e é tratado em `run_command`; nenhuma tecla do modo Normal produz a ação, e uma variante que ninguém constrói é código morto que o `unreachable_pub` não pega mas o leitor seguinte tropeça.

- [ ] **Step 5: Partir o `run` em laço e sessão**

Em `crates/seele-tui/src/main.rs`, renomeie a função `run` atual (linha 360) para `sessao`, mude a assinatura para receber `args` já resolvidos e devolver se ejetou:

```rust
/// One session, from the first packet to the last.
///
/// Returns `true` when it exited via `:ejetar`, which is the signal for
/// `run`'s loop to go back to the selection screen instead of quitting.
async fn sessao(
    terminal: &mut Screen1,
    args: &Args,
    holds: bool,
    home: &std::path::Path,
    tema: Theme,
) -> Result<bool> {
```

Remova de `sessao` o bloco que resolve `args` (linhas 361-371) e as declarações de `home` e `tema` — passam a vir por parâmetro. Em cada `return Ok(())` da função, devolva `Ok(false)`. No fim do laço de eventos, onde hoje está `if runtime.app.quit { return Ok(()); }`, use:

```rust
        if runtime.app.quit {
            return Ok(false);
        }
        if runtime.app.ejetou {
            return Ok(true);
        }
```

Escreva o `run` novo:

```rust
/// The outer loop: choose, talk, eject, choose again.
///
/// The whole session lives inside one turn of this loop, and `Enlace` and
/// `Voice` are dropped at the end of it. This is **not** what issue #9
/// turned down — that was swapping the connection out from under a live
/// session; this is tearing everything down and going back to a screen that
/// has no roster, no telemetry, and no audio.
async fn run(terminal: &mut Screen1, args: Option<Args>, holds: bool) -> Result<()> {
    let home = config_dir();
    let tema = Theme::detect();

    // With a flag, the selection screen does not appear at startup — whoever
    // typed `--server` already said where to go. When ejecting it does
    // appear, and rightly so: ejecting is the explicit request to go
    // somewhere else.
    let mut escolhidos = args;

    loop {
        let args = match escolhidos.take() {
            Some(args) => args,
            None => match escolher(terminal, tema, &home)? {
                Some(args) => args,
                None => return Ok(()),
            },
        };

        if !sessao(terminal, &args, holds, &home, tema).await? {
            return Ok(());
        }
    }
}
```

- [ ] **Step 6: Encerrar a hospedagem ao ejetar**

Em `sessao`, a `hospedagem` (linha 400) vive numa variável local e cai no fim da função. Isso não basta: o `disconnect` do app espera a porta voltar antes de permitir hospedar de novo (`apps/seele-app/src/main.rs:238`), e sem isso a próxima volta do laço falha com porta ocupada.

Antes de cada `return` que devolve `Ok(true)`, acrescente:

```rust
        if let Some(dogma) = hospedagem.take() {
            dogma.encerrar().await;
        }
```

Para isso, declare a hospedagem como `let mut hospedagem = ...`.

- [ ] **Step 7: Ligar o comando à ação**

Em `run_command` (linha 850), acrescente ao `match command`:

```rust
        Command::Eject => runtime.app.ejetar(),
```

- [ ] **Step 8: Atualizar a ajuda e o `:sobre`**

`?` e `:sobre` listam o vocabulário. Acrescente `:ejetar` com a frase "sair deste Dogma e escolher outro", e confirme que `:q` continua descrito como sair do programa.

- [ ] **Step 9: Rodar tudo**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, sem avisos.

- [ ] **Step 10: Commit**

```bash
git add crates/seele-tui/src/command.rs crates/seele-tui/src/app.rs crates/seele-tui/src/main.rs crates/seele-tui/src/ui.rs
git commit -m "feat(tui): ejetar volta à tela de seleção em vez de matar o processo

O app ganhou EJETAR e o \`plug\` não tinha nada equivalente: quem entrava
num Dogma só saía fechando o programa. Agora \`:ejetar\` volta à seleção e
\`:q\` continua saindo — são coisas diferentes e passam a parecer
diferentes.

O \`run\` virou laço em volta de \`sessao\`, com \`Enlace\` e \`Voice\` soltos
ao fim de cada volta e a hospedagem encerrada esperando a porta voltar,
como o \`disconnect\` do app já fazia. Isto não é a troca de conexão sob
sessão viva que a pendência #9 recusou."
```

---

### Task 6: Conformidade — conectar, ejetar, conectar

Esta é a tarefa que prova que a Task 5 estava certa. Se ela não passar, a decisão do laço externo está errada, e é melhor descobrir aqui.

**Files:**
- Create: `crates/seele-conformance/tests/ejetar.rs`

**Interfaces:**
- Consumes: `seele_core::Enlace`, `seele_server::hospedagem::Hospedagem`.
- Produces: nada.

- [ ] **Step 1: Ler o padrão existente**

Run: `sed -n '1,60p' crates/seele-conformance/tests/bateria_interna.rs`

Copie desse arquivo a forma de subir um Dogma de teste e de conectar. **Não invente uma segunda forma** — o arquivo existente é o contrato.

- [ ] **Step 2: Escrever o teste**

Crie `crates/seele-conformance/tests/ejetar.rs`:

```rust
//! Ejetar e voltar, no mesmo processo.
//!
//! A pendência #9 recusou trocar a conexão por baixo de uma sessão viva. O
//! laço externo do `plug` faz outra coisa: derruba tudo e reconstrói. Este
//! teste existe para provar que o teardown do `Enlace` fecha de verdade — se
//! ele falhar, a decisão do laço estava errada.

// (mesmos `use` e helpers de bateria_interna.rs)

#[tokio::test]
async fn conectar_ejetar_e_conectar_de_novo_no_mesmo_processo() {
    let dogma = subir_dogma().await;

    let primeiro = conectar(&dogma, "piloto").await;
    assert!(primeiro.sessao().pilot.0 > 0, "a primeira sessão não subiu");

    // Ejetar é soltar o enlace. Se algo ficar segurando a conexão ou a thread
    // de áudio, é aqui que se vê.
    drop(primeiro);

    let segundo = conectar(&dogma, "piloto").await;
    assert!(
        segundo.sessao().pilot.0 > 0,
        "reconectar depois de ejetar falhou — o teardown não fechou"
    );
}

#[tokio::test]
async fn hospedar_ejetar_e_hospedar_de_novo_libera_a_porta() {
    // `Hospedagem::encerrar` espera a porta voltar. Sem essa espera, a segunda
    // volta do laço falharia com porta ocupada — que é exatamente o caso que o
    // `disconnect` do app já tratava.
    let porta = porta_livre();
    let primeiro = Hospedagem::iniciar(porta, Location::Memory, "Casa")
        .await
        .expect("primeiro Dogma");
    primeiro.encerrar().await;

    let segundo = Hospedagem::iniciar(porta, Location::Memory, "Casa").await;
    assert!(segundo.is_ok(), "a porta não voltou depois de encerrar");
}
```

**Nota:** `porta_livre()`, `subir_dogma()` e `conectar()` devem sair dos helpers que os testes existentes já usam. Se não existirem com esses nomes, use os nomes reais do arquivo lido no Passo 1.

- [ ] **Step 3: Rodar**

Run: `cargo test -p seele-conformance --test ejetar -- --nocapture`
Expected: PASS.

**Se falhar:** não remende o teste. O teste está descrevendo o que a Task 5 prometeu. Volte à Task 5 e ache o que continua segurando a conexão ou a porta, e registre o achado em `docs/pendencias.md` se não for consertável agora.

- [ ] **Step 4: Commit**

```bash
git add crates/seele-conformance/tests/ejetar.rs
git commit -m "test: ejetar e voltar, no mesmo processo

A parte de risco do laço externo é o teardown, e teardown que não fecha
falha em silêncio meses depois. Este teste conecta, solta e conecta de
novo — e faz o mesmo com a porta da hospedagem, que é onde a falha
apareceria primeiro."
```

---

### Task 7: App — os comandos, a tela de entrada e a busca

**Tarefa grande de propósito.** As duas metades não se separam: o guarda `no_command_is_registered_and_never_called` (`apps/seele-app/tests/frontend.rs:76`) falha enquanto os seis comandos existirem sem nenhum `invoke` chamando, e só o frontend os chama. Uma tarefa que fecha com a suíte vermelha não é uma tarefa que alguém possa aprovar.

**Files:**
- Modify: `apps/seele-app/src/main.rs` (`Session` em 37, `connect` em 96, `generate_handler!` em 328)
- Modify: `apps/seele-app/ui/index.html` (tela de boot em 25-78, painel de mensagens em 148-156)
- Modify: `apps/seele-app/ui/seele.js` (`conectar` em 377, `desenharMensagens` em 246, ouvintes em 481)
- Modify: `apps/seele-app/ui/seele.css`
- Modify: `apps/seele-app/tests/frontend.rs`

**Interfaces:**
- Consumes: `seele_core::search::{Search, normalize}` da Task 1; `seele_core::conhecidos::{Conhecidos, Conhecido}`; `seele_core::uri::analisar`.
- Produces: os comandos Tauri `conhecidos`, `esquecer`, `analisar_convite`, `buscar`, `busca_andar`, `busca_limpar`; o tipo `BuscaEstado { casamentos: Vec<Match>, atual: Option<Match>, posicao: u32, total: u32 }`, serializável.

- [ ] **Step 1: Escrever os comandos**

Em `apps/seele-app/src/main.rs`, acrescente ao `struct Session` (linha 37):

```rust
    /// A busca corrente. O cursor é estado de sessão, e é o que impede a regra
    /// de dar-a-volta de ser reescrita em JavaScript.
    busca: Mutex<Option<seele_core::search::Search>>,
```

Acrescente o tipo devolvido e os comandos:

```rust
/// O que o frontend precisa saber sobre a busca corrente.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct BuscaEstado {
    /// Onde o termo casou, na ordem em que a tela desenha.
    casamentos: Vec<seele_core::search::Match>,
    /// A ocorrência em que o cursor está.
    atual: Option<seele_core::search::Match>,
    /// Posição de 1, para desenhar "[1/3]". Zero quando não casou nada.
    posicao: u32,
    /// Quantas ao todo.
    total: u32,
}

impl BuscaEstado {
    fn de(busca: &seele_core::search::Search) -> Self {
        let (posicao, total) = busca.position();
        Self {
            // Todos, e não só o corrente: o app pinta o histórico inteiro de
            // uma vez, e acender só a ocorrência do cursor esconderia as
            // outras que estão na mesma tela.
            casamentos: busca.matches().to_vec(),
            atual: busca.current(),
            posicao: u32::try_from(posicao).unwrap_or(0),
            total: u32::try_from(total).unwrap_or(0),
        }
    }
}

/// Os Dogmas onde este piloto já esteve.
///
/// Uma lista de atalhos corrompida não pode fechar a porta: `specs/05` diz que
/// este arquivo é conveniência e pode ser apagado sem consequência. Por isso
/// falha vira lista vazia, e nunca erro.
#[tauri::command]
fn conhecidos(app: AppHandle) -> Vec<seele_core::conhecidos::Conhecido> {
    let home = std::path::PathBuf::from(config_dir(&app));
    seele_core::conhecidos::Conhecidos::abrir(home.join("conhecidos"))
        .map(|lista| lista.listar())
        .unwrap_or_default()
}

/// Tira um Dogma da lista.
#[tauri::command]
fn esquecer(app: AppHandle, alvo: String) -> Result<(), ()> {
    let home = std::path::PathBuf::from(config_dir(&app));
    let Ok(mut lista) = seele_core::conhecidos::Conhecidos::abrir(home.join("conhecidos")) else {
        return Ok(());
    };
    lista.esquecer(&alvo).map_err(|_| ())
}

/// Lê um `seele://` colado.
///
/// Mora aqui e não no JavaScript porque `specs/06:19` é inegociável: se o
/// frontend precisasse saber o que é uma impressão digital, algo estaria
/// errado.
#[tauri::command]
fn analisar_convite(link: String) -> Result<seele_core::uri::Convite, String> {
    seele_core::uri::analisar(&link).map_err(|erro| erro.to_string())
}

#[tauri::command]
fn buscar(session: State<'_, Session>, termo: String) -> Result<BuscaEstado, PlugError> {
    let snapshot = session.plug()?.snapshot();
    let busca = seele_core::search::Search::new(
        snapshot
            .messages
            .iter()
            .map(|mensagem| seele_core::search::normalize(&mensagem.body)),
        &termo,
    );
    let estado = BuscaEstado::de(&busca);
    if let Ok(mut slot) = session.busca.lock() {
        *slot = Some(busca);
    }
    Ok(estado)
}

#[tauri::command]
fn busca_andar(session: State<'_, Session>, adiante: bool) -> BuscaEstado {
    let Ok(mut slot) = session.busca.lock() else {
        return BuscaEstado::default();
    };
    let Some(busca) = slot.as_mut() else {
        return BuscaEstado::default();
    };
    if adiante {
        busca.next_match();
    } else {
        busca.previous_match();
    }
    BuscaEstado::de(busca)
}

#[tauri::command]
fn busca_limpar(session: State<'_, Session>) {
    if let Ok(mut slot) = session.busca.lock() {
        *slot = None;
    }
}
```

`Match` já deriva `serde::Serialize` desde a Task 1, e `busca.matches()` já existe — nada a acrescentar no core aqui.

- [ ] **Step 2: Gravar em `conhecidos` depois de conectar**

No comando `connect`, **depois** da conexão dar certo e antes do `Ok(snapshot)`, acrescente:

```rust
    // A metade invisível da lista de visitados: sem isto a seção da tela de
    // entrada ficaria permanentemente vazia. A política é a mesma que o `plug`
    // já escreveu em `seele-tui/src/main.rs:467`.
    //
    // Registrado só depois de dar certo — guardar antes encheria a lista de
    // endereços errados digitados uma vez, que é o oposto de uma lista de
    // atalhos. E um Dogma hospedado aqui não entra: `127.0.0.1` não é lugar
    // aonde se volta, é o botão HOSPEDAR.
    if !server.starts_with("127.0.0.1") && !server.starts_with("localhost") {
        if let Ok(mut lista) = seele_core::conhecidos::Conhecidos::abrir(
            std::path::PathBuf::from(&home).join("conhecidos"),
        ) {
            // Falhar em gravar um atalho não derruba uma conversa de pé.
            let _ = lista.registrar(&server, &nickname, None);
        }
    }
```

- [ ] **Step 3: Registrar os comandos**

Em `generate_handler!` (linha 328), acrescente ao fim da lista:

```rust
            conhecidos,
            esquecer,
            analisar_convite,
            buscar,
            busca_andar,
            busca_limpar,
```

- [ ] **Step 4: Compilar**

Run: `cargo build -p seele-app`
Expected: compila. **Não rode `cargo test -p seele-app` ainda** — o guarda `no_command_is_registered_and_never_called` vai falhar até o frontend chamar os comandos, e é o Passo 12 que fecha isso. Não commite aqui: esta tarefa tem um commit só, no fim.

- [ ] **Step 5: Marcação da tela de entrada**

Em `apps/seele-app/ui/index.html`, dentro de `<div class="boot">`, **antes** do `<form id="form-conectar">`:

```html
  <!--
    Onde você já esteve. Some inteira quando não há visitado nenhum — o estado
    vazio da tela é exatamente o de antes desta seção existir, e um cabeçalho
    sobre uma lista vazia é pior que nenhum cabeçalho.
  -->
  <section id="visitados" class="visitados" hidden>
    <h2 class="visitados-titulo">ONDE VOCÊ JÁ ESTEVE</h2>
    <ul id="lista-visitados" class="lista-visitados"></ul>
  </section>
```

Dentro do `<form id="form-conectar">`, depois do campo PILOTO:

```html
      <label class="campo">
        <span class="rotulo">CONVITE</span>
        <input id="campo-convite" name="convite" placeholder="cole um seele://…"
               spellcheck="false" autocapitalize="off">
      </label>
```

No painel de mensagens (linha 148), depois do `<h2 class="painel-titulo">`:

```html
      <!--
        A busca. `[1/3]` não é enfeite: `specs/05-cliente-tui.md` proíbe
        informação transmitida só por cor, e o contador é o acompanhante
        textual do realce.
      -->
      <form id="form-busca" class="busca" autocomplete="off">
        <span class="prompt">/</span>
        <input id="campo-busca" name="busca" placeholder="buscar…"
               spellcheck="false" autocomplete="off">
        <span id="busca-contador" class="busca-contador"></span>
        <button id="busca-anterior" class="botao-fantasma" type="button">◂</button>
        <button id="busca-proxima" class="botao-fantasma" type="button">▸</button>
      </form>
```

- [ ] **Step 6: Preencher a lista de visitados**

Em `apps/seele-app/ui/seele.js`, acrescente:

```js
/// Quanto tempo faz, em palavras curtas. A data exata não ajuda a escolher.
function quando(segundos) {
  const dias = Math.floor((Date.now() / 1000 - segundos) / 86400);
  if (dias <= 0) return "hoje";
  if (dias === 1) return "ontem";
  return `${dias} dias`;
}

async function desenharVisitados() {
  const lista = await invoke("conhecidos");
  const secao = $("visitados");
  // Sem visitados, a seção some inteira: a tela volta a ser exatamente a de
  // antes, e o estado vazio não piora.
  secao.hidden = lista.length === 0;
  if (lista.length === 0) return;

  repovoar(
    $("lista-visitados"),
    lista.map((conhecido) => {
      const linha = elemento("li", "visitado");
      const ir = elemento("button", "visitado-ir", conhecido.alvo);
      ir.type = "button";
      ir.addEventListener("click", () => {
        $("campo-servidor").value = conhecido.alvo;
        $("campo-apelido").value = conhecido.apelido;
        conectar();
      });
      const esquecer = elemento("button", "botao-fantasma", "esquecer");
      esquecer.type = "button";
      esquecer.addEventListener("click", async (evento) => {
        evento.stopPropagation();
        await invoke("esquecer", { alvo: conhecido.alvo });
        await desenharVisitados();
      });
      linha.append(
        ir,
        elemento("span", "visitado-apelido", conhecido.apelido),
        elemento("span", "visitado-quando", quando(conhecido.visto_em)),
        esquecer,
      );
      return linha;
    }),
  );
}
```

Chame `desenharVisitados()` no arranque, junto dos outros ouvintes (perto da linha 481), e de novo depois de `ejetar()` — quem acabou de sair de um Dogma tem que vê-lo na lista.

- [ ] **Step 7: Ler o convite colado**

```js
async function lerConvite() {
  const campo = $("campo-convite");
  const link = campo.value.trim();
  const erro = $("boot-erro");
  if (link === "") return;

  try {
    const convite = await invoke("analisar_convite", { link });
    $("campo-servidor").value = convite.alvo;
    erro.hidden = true;
  } catch (falha) {
    // O formulário fica intacto: quem colou errado não perde o que já tinha
    // digitado nos outros campos.
    erro.textContent = `este convite não serve: ${falha}`;
    erro.hidden = false;
  }
}

$("campo-convite").addEventListener("change", lerConvite);
$("campo-convite").addEventListener("paste", () => setTimeout(lerConvite, 0));
```

O token do convite tem que chegar ao `connect`. Declare, junto das outras variáveis de módulo (perto da linha 21):

```js
/** O convite lido do último `seele://` colado, se houver. */
let convitePendente = null;
```

Em `lerConvite`, depois de preencher o endereço: `convitePendente = convite;`. No `catch`, `convitePendente = null;`.

Em `conectar()`, na chamada de `invoke("connect", …)`, acrescente o campo:

```js
      joinSecret: convitePendente?.token ?? null,
```

**Confira o nome.** O parâmetro em Rust é `join_secret` (`apps/seele-app/src/main.rs:102`), e o Tauri converte snake_case para camelCase na ponte. Veja como os outros comandos já são chamados no arquivo e siga a mesma forma — se `set_at_field` é invocado com `{ on: … }`, a conversão está ativa.

- [ ] **Step 8: Ligar a busca**

O snapshot já desenhado vive na variável `desenhado` (linha 21). Os casamentos ficam num módulo à parte para `desenharMensagens` alcançá-los:

```js
/** Os casamentos da busca corrente, agrupados por índice de mensagem. */
let casamentosPorMensagem = new Map();
let termoAtual = "";

function guardarCasamentos(estado) {
  casamentosPorMensagem = new Map();
  for (const casamento of estado.casamentos) {
    const lista = casamentosPorMensagem.get(casamento.mensagem) ?? [];
    lista.push(casamento);
    casamentosPorMensagem.set(casamento.mensagem, lista);
  }
}

function desenharBusca(estado) {
  $("busca-contador").textContent =
    estado.total === 0 ? "[0/0]" : `[${estado.posicao}/${estado.total}]`;
  guardarCasamentos(estado);
  desenharMensagens(desenhado);
  if (estado.atual) {
    const linha = $("lista-mensagens").children[estado.atual.mensagem];
    linha?.scrollIntoView({ block: "center" });
  }
}

$("form-busca").addEventListener("submit", (evento) => evento.preventDefault());

$("campo-busca").addEventListener("input", async () => {
  termoAtual = $("campo-busca").value;
  if (termoAtual.trim() === "") {
    await invoke("busca_limpar");
    $("busca-contador").textContent = "";
    casamentosPorMensagem = new Map();
    desenharMensagens(desenhado);
    return;
  }
  desenharBusca(await invoke("buscar", { termo: termoAtual }));
});

$("busca-proxima").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: true })),
);
$("busca-anterior").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: false })),
);
```

No `keydown` da janela (linha 596), acrescente antes do tratamento da barra de espaço:

```js
  // `/` foca a busca, como no terminal. Só fora de um campo de texto: uma
  // barra digitada numa mensagem é uma barra.
  if (evento.key === "/" && !(evento.target instanceof HTMLInputElement)) {
    evento.preventDefault();
    $("campo-busca").focus();
    return;
  }
  if (evento.key === "Escape" && evento.target === $("campo-busca")) {
    $("campo-busca").value = "";
    $("campo-busca").dispatchEvent(new Event("input"));
    $("campo-busca").blur();
    return;
  }
  if (evento.key === "Enter" && evento.target === $("campo-busca")) {
    evento.preventDefault();
    invoke("busca_andar", { adiante: !evento.shiftKey }).then(desenharBusca);
    return;
  }
```

- [ ] **Step 9: Acender o termo nas mensagens**

`desenharMensagens` (linha 246) monta o corpo com `elemento("div", "corpo", mensagem.body)` — o texto entra como **argumento**, e `elemento` não aceita nós. A linha precisa virar duas.

Troque, dentro do `map`, a linha `item.append(cabeca, elemento("div", "corpo", mensagem.body));` por:

```js
    const corpo = elemento("div", "corpo");
    corpo.append(...corpoComRealce(mensagem.body, casamentosPorMensagem.get(indice)));
    item.append(cabeca, corpo);
```

E mude a assinatura do `map` para receber o índice: `snapshot.messages.map((mensagem, indice) => {`.

Acrescente o partidor:

```js
/// Parte o corpo em pedaços aceso e apagado.
///
/// Recebe os intervalos prontos do Rust: com dobramento de acento o frontend
/// não teria como saber onde o casamento começou.
function corpoComRealce(corpo, intervalos) {
  if (!intervalos || intervalos.length === 0) return [document.createTextNode(corpo)];
  const caracteres = [...corpo];
  const pedacos = [];
  let cursor = 0;
  for (const { inicio, fim } of intervalos) {
    if (inicio > cursor) {
      pedacos.push(document.createTextNode(caracteres.slice(cursor, inicio).join("")));
    }
    pedacos.push(elemento("mark", "realce", caracteres.slice(inicio, fim).join("")));
    cursor = fim;
  }
  if (cursor < caracteres.length) {
    pedacos.push(document.createTextNode(caracteres.slice(cursor).join("")));
  }
  return pedacos;
}
```

**O app tem que desenhar o corpo normalizado, e isto não é detalhe.** O comando `buscar` roda sobre `normalize(&mensagem.body)` e devolve deslocamentos naquele texto. Fatiar o corpo **cru** com esses deslocamentos erra o alvo em qualquer mensagem com espaço duplo ou quebra de linha — e HTML colapsar espaço na hora de exibir **não** conserta isso, porque o erro é de índice de string, não de pintura.

O `desenharMensagens` fatia o normalizado:

```js
    const texto = mensagem.body.split(/\s+/).filter(Boolean).join(" ");
    const corpo = elemento("div", "corpo");
    corpo.append(...corpoComRealce(texto, casamentosPorMensagem.get(indice)));
```

Não muda nada visível — HTML já colapsava aquele espaço —, e passa a ser a mesma string dos dois lados da ponte. **Escreva um teste disto**, porque é exatamente o tipo de defeito que só aparece quando alguém cola um texto com duas quebras de linha:

```rust
// Em apps/seele-app/tests/frontend.rs
#[test]
fn o_frontend_normaliza_o_corpo_antes_de_fatiar_o_realce() {
    // Os deslocamentos vêm do Rust, calculados sobre o corpo normalizado.
    // Fatiar o corpo cru com eles erra o alvo depois de um espaço duplo.
    let script = read("ui/seele.js");
    assert!(
        script.contains("split(/\\s+/)"),
        "o corpo não é normalizado antes de receber o realce"
    );
}
```

- [ ] **Step 10: Estilo**

Em `apps/seele-app/ui/seele.css`, acrescente. Use **só** variáveis de `tokens.css` — nenhuma cor literal, porque o teste `apps/seele-app/tests/tokens.rs` guarda isso. Antes de escrever, rode `grep -n "^  --" apps/seele-app/ui/tokens.css` e use os nomes que existirem; os abaixo são a forma, não a lista.

```css
/* Onde você já esteve. */
.visitados { margin-block-end: var(--espaco-3); }
.visitados-titulo { font: inherit; letter-spacing: .12em; color: var(--rotulo); }
.lista-visitados { list-style: none; margin: 0; padding: 0; }
.visitado { display: flex; gap: var(--espaco-2); align-items: baseline; }
.visitado-ir { background: none; border: 0; color: var(--texto); cursor: pointer; text-align: start; }
.visitado-ir:hover { color: var(--acento); }
.visitado-apelido, .visitado-quando { color: var(--rotulo); }

/* A busca no painel de mensagens. */
.busca { display: flex; gap: var(--espaco-2); align-items: center; }
.busca-contador { color: var(--rotulo); font-variant-numeric: tabular-nums; }

/*
  O realce. `specs/05-cliente-tui.md:105` não deixa informação viver só na
  cor, e o contador [1/3] é o acompanhante textual — mas o sublinhado aqui
  faz a ocorrência sobreviver a um monitor ruim sem depender dele.
*/
.realce { background: var(--acento-fraco); color: inherit; text-decoration: underline; }
```

Run: `cargo test -p seele-app tokens`
Expected: PASS. Se falhar, uma cor literal escapou — troque pela variável, nunca relaxe o teste.

- [ ] **Step 11: Rodar os guardas**

Run: `cargo test -p seele-app`
Expected: PASS, incluindo `every_command_the_frontend_calls_is_registered`, `no_command_is_registered_and_never_called`, `every_element_the_script_reaches_for_exists_in_the_page` e `the_frontend_never_names_a_protocol_concept`.

Se `the_frontend_never_names_a_protocol_concept` falhar, algum conceito de protocolo vazou para o JS. O conserto é mover a lógica para um comando, nunca relaxar o teste.

- [ ] **Step 12: Rodar tudo e commitar**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add apps/seele-app/ui/index.html apps/seele-app/ui/seele.js apps/seele-app/ui/seele.css apps/seele-app/src/main.rs apps/seele-app/tests/frontend.rs
git commit -m "feat(app): a tela de entrada lembra onde você esteve, e a busca busca

A lista de visitados fica acima do formulário e some inteira quando está
vazia — o estado vazio é exatamente a tela de antes, e nada fica
escondido atrás de um clique. Colar um seele:// preenche o endereço, e
quem colou errado não perde o que já tinha digitado.

Os intervalos do realce vêm prontos do Rust: com dobramento de acento o
frontend não teria como saber onde o casamento começou."
```

**Conferência a olho: não é sua.** O controlador roda `cargo tauri dev` e confere a tela depois desta tarefa. Não tente abrir a janela.

---

### Task 8: Fechar a documentação

**Files:**
- Modify: `docs/pendencias.md`
- Modify: `specs/05-cliente-tui.md`
- Modify: `specs/06-clientes-gui.md`

- [ ] **Step 1: Atualizar as specs**

Em `specs/05-cliente-tui.md`, na lista de atalhos (linhas 41-52), acrescente:

```
n / N             próxima / anterior ocorrência da busca
Shift+Tab         painel anterior
:ejetar           sair deste Dogma e escolher outro
```

E na linha dos comandos (54), acrescente `:ejetar` à lista.

Em `specs/06-clientes-gui.md`, na seção "Hospedar pelo app", acrescente um parágrafo dizendo que a tela de entrada lista os Dogmas visitados e aceita um `seele://` colado, e que os dois vêm de `seele-core` por comando — nada de protocolo em JavaScript.

- [ ] **Step 2: Alinhar o pseudocódigo da spec de design**

`docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md` ainda mostra `Casamento`, `Busca`, `BuscaEstado`, `proxima`, `anterior` e `atual` no bloco ilustrativo da seção 1 e nas seções 2 e 3. A API real ficou em inglês (ADR 0023). Troque os nomes pelos reais — `Match`, `Search`, `next_match`, `previous_match`, `current`, `position`, `normalize`, `occurrences` — mantendo a **prosa em português**, que é onde a spec deve ficar.

Um documento de design que nomeia tipos que não existem é pior que um sem código nenhum: manda o próximo leitor procurar por `Busca` e não achar.

- [ ] **Step 3: Atualizar as pendências**

Em `docs/pendencias.md`, na pendência **9 · `:conectar` não reconecta em execução**, substitua o texto por:

```markdown
## 9 · `:conectar` não reconecta em execução

O comando existe e avisa que não faz. **`:ejetar` agora resolve o caso comum**:
volta à tela de seleção, com a conexão e o áudio derrubados de verdade, e de lá
se escolhe outro Dogma. O que continua faltando é trocar de destino num comando
só, sem passar pela tela.

O que o laço externo mostrou é que o teardown fecha —
`crates/seele-conformance/tests/ejetar.rs` conecta, solta e conecta de novo no
mesmo processo. O que a pendência recusava era outra coisa: trocar a conexão por
baixo de uma sessão viva, com roster e áudio de pé.
```

- [ ] **Step 4: Commit**

```bash
git add docs/pendencias.md specs/05-cliente-tui.md specs/06-clientes-gui.md
git commit -m "docs: as specs alcançam a navegação que as cascas agora têm

A pendência #9 encolheu em vez de sumir: ejetar resolve o caso comum e o
teste de conformidade mostra que o teardown fecha. Trocar de destino num
comando só, sem passar pela tela, continua sem fazer."
```

---

## Ordem e dependências

```
Task 1 (core::busca) ─┬─> Task 3 (busca na TUI) ──> Task 4 (desenho)
                      └─> Task 7 (app: comandos + tela + busca)
Task 2 (foco) ────────────> (independente)
Task 5 (ejetar) ──────────> Task 6 (conformidade)
Task 8 (docs) ────────────> depois de todas
```

Tasks 2 e 5 não dependem de nada e podem sair primeiro se convier. **A Task 6 é a que carrega o risco** — se ela falhar, a Task 5 precisa voltar à mesa antes de qualquer coisa a jusante.

**A Task 7 é grande e não se parte.** O guarda `no_command_is_registered_and_never_called` amarra os comandos Rust ao frontend que os chama: separá-los produziria uma tarefa que fecha com a suíte vermelha, que ninguém pode aprovar.

**Duas conferências a olho são do controlador**, não dos subagentes: depois da Task 4 (`cargo run -p seele-tui --example telas`) e depois da Task 7 (`cargo tauri dev`).
