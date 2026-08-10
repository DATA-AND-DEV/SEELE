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

### Task 1: `seele_core::busca`

Módulo puro, sem I/O. É a peça que as duas cascas consomem, e por isso vem primeiro.

**Files:**
- Create: `crates/seele-core/src/busca.rs`
- Modify: `crates/seele-core/src/lib.rs` (lista de módulos, linhas 27-34)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `seele_core::busca::normalizar(texto: &str) -> String`
  - `seele_core::busca::ocorrencias(texto: &str, termo: &str) -> Vec<(usize, usize)>`
  - `seele_core::busca::Casamento { mensagem: usize, inicio: usize, fim: usize }` (Copy, PartialEq, Eq, Debug, Clone, **`serde::Serialize`** — a Task 7 o manda pela ponte do Tauri)
  - `seele_core::busca::Busca` com `nova<S: AsRef<str>>(corpos: impl IntoIterator<Item = S>, termo: &str) -> Busca`, `proxima(&mut self) -> Option<Casamento>`, `anterior(&mut self) -> Option<Casamento>`, `atual(&self) -> Option<Casamento>`, `posicao(&self) -> (usize, usize)`, `ordinal_na_mensagem(&self) -> Option<usize>`, `vazia(&self) -> bool`, `casamentos(&self) -> &[Casamento]`

**Por que a assinatura mudou em relação à spec.** A spec registrou `nova<'a>(corpos: impl Iterator<Item = &'a str>, …)`. Não serve: as cascas precisam passar corpos **normalizados**, que são `String` recém-criadas e não sobrevivem como `&'a str`. `S: AsRef<str>` aceita as duas formas. A spec é corrigida no Passo 8 desta tarefa.

- [ ] **Step 1: Escrever os testes que falham**

Crie `crates/seele-core/src/busca.rs` com **apenas** o bloco de testes abaixo (o código vem no Passo 3):

```rust
#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "test vectors are fixed and local")]
mod tests {
    use super::*;

    #[test]
    fn a_caixa_nao_importa() {
        let busca = Busca::nova(["o SYNC caiu"], "sync");
        assert_eq!(busca.posicao(), (1, 1));
        assert_eq!(busca.atual(), Some(Casamento { mensagem: 0, inicio: 2, fim: 6 }));
    }

    #[test]
    fn o_acento_nao_importa_e_o_intervalo_continua_certo() {
        // A tabela é 1:1 por caractere, e é exatamente aqui que ela se
        // denunciaria se deixasse de ser: `não` tem três caracteres, e o
        // intervalo devolvido tem que ter três.
        let busca = Busca::nova(["não foi"], "nao");
        let casamento = busca.atual().unwrap_or(Casamento { mensagem: 9, inicio: 9, fim: 9 });
        assert_eq!(casamento.inicio, 0);
        assert_eq!(casamento.fim, 3);
    }

    #[test]
    fn o_acento_dobra_nos_dois_sentidos() {
        assert_eq!(Busca::nova(["nao foi"], "não").posicao(), (1, 1));
        assert_eq!(Busca::nova(["MAIÚSCULA"], "maiuscula").posicao(), (1, 1));
    }

    #[test]
    fn o_cursor_da_a_volta_nas_duas_pontas() {
        let mut busca = Busca::nova(["sync", "sync", "sync"], "sync");
        assert_eq!(busca.posicao(), (1, 3));
        busca.proxima();
        busca.proxima();
        assert_eq!(busca.posicao(), (3, 3));
        // Do fim, `n` volta ao começo: numa conversa a última ocorrência e a
        // primeira são vizinhas para quem está procurando.
        busca.proxima();
        assert_eq!(busca.posicao(), (1, 3));
        busca.anterior();
        assert_eq!(busca.posicao(), (3, 3));
    }

    #[test]
    fn varias_ocorrencias_na_mesma_mensagem_contam_separado() {
        let busca = Busca::nova(["sync e sync"], "sync");
        assert_eq!(busca.posicao(), (1, 2));
        assert_eq!(busca.ordinal_na_mensagem(), Some(0));
    }

    #[test]
    fn o_ordinal_conta_dentro_da_mensagem_e_nao_no_total() {
        let mut busca = Busca::nova(["sync", "sync e sync"], "sync");
        busca.proxima();
        assert_eq!(busca.atual().map(|c| c.mensagem), Some(1));
        assert_eq!(busca.ordinal_na_mensagem(), Some(0));
        busca.proxima();
        assert_eq!(busca.ordinal_na_mensagem(), Some(1));
    }

    #[test]
    fn termo_vazio_ou_so_espaco_nao_casa_nada() {
        assert!(Busca::nova(["sync"], "").vazia());
        assert!(Busca::nova(["sync"], "   ").vazia());
        assert_eq!(Busca::nova(["sync"], "").posicao(), (0, 0));
    }

    #[test]
    fn sem_casamento_nao_e_erro_nem_panico() {
        let mut busca = Busca::nova(["sync caiu"], "harmônicos");
        assert!(busca.vazia());
        assert_eq!(busca.atual(), None);
        assert_eq!(busca.proxima(), None);
        assert_eq!(busca.anterior(), None);
        assert_eq!(busca.posicao(), (0, 0));
    }

    #[test]
    fn um_termo_maior_que_o_corpo_nao_estoura() {
        assert!(Busca::nova(["oi"], "oi mesmo").vazia());
        assert!(Busca::nova([""], "oi").vazia());
    }

    #[test]
    fn normalizar_colapsa_o_espaco_como_o_wrap_da_tui_faz() {
        // `seele-tui::ui::wrap` usa `split_whitespace`, que colapsa. Sem esta
        // normalização os deslocamentos apontariam para o lugar errado em
        // qualquer corpo com espaço duplo.
        assert_eq!(normalizar("a  b\tc\nd "), "a b c d");
        assert_eq!(normalizar("   "), "");
    }

    #[test]
    fn ocorrencias_acha_todas_e_em_ordem() {
        assert_eq!(ocorrencias("sync e sync", "sync"), vec![(0, 4), (7, 11)]);
        assert_eq!(ocorrencias("nada", "sync"), Vec::new());
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core busca`
Expected: FAIL na compilação — `cannot find type Busca in this scope`, `cannot find function normalizar`.

- [ ] **Step 3: Escrever a implementação**

Coloque **acima** do bloco de testes em `crates/seele-core/src/busca.rs`:

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
//! [`normalizar`] collapses whitespace the same way `seele-tui::ui::wrap` does
//! and the way HTML does. Both shells already show text with runs of
//! whitespace collapsed, so searching the normalised body is searching exactly
//! what is on screen.

/// Where a term matched.
///
/// `Serialize` because the desktop shell sends these across the Tauri bridge:
/// with accent folding in the middle, a frontend cannot work out where a match
/// began on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Casamento {
    /// Index into the list of bodies, in the order the shell drew them.
    pub mensagem: usize,
    /// Character offset where the match starts.
    pub inicio: usize,
    /// Character offset one past the end.
    pub fim: usize,
}

/// Collapses runs of whitespace, the way both shells already display text.
#[must_use]
pub fn normalizar(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Folds one character: lowercase first, then strip the Portuguese accent.
///
/// `to_lowercase` can yield more than one character for a few code points;
/// taking the first keeps this one-to-one, which is what the offsets depend on.
fn dobrar_char(character: char) -> char {
    let baixo = character.to_lowercase().next().unwrap_or(character);
    match baixo {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        outro => outro,
    }
}

fn dobrar(texto: &str) -> Vec<char> {
    texto.chars().map(dobrar_char).collect()
}

/// Every place `termo` occurs in `texto`, as character ranges, left to right.
///
/// Public because a shell that wraps text into segments needs to light the
/// matches inside a segment without redoing the folding rule.
#[must_use]
pub fn ocorrencias(texto: &str, termo: &str) -> Vec<(usize, usize)> {
    let alvo = dobrar(termo.trim());
    if alvo.is_empty() {
        return Vec::new();
    }
    let corpo = dobrar(texto);
    // `windows` yields nothing when the body is shorter than the term, and
    // panics only on a zero width — ruled out above.
    corpo
        .windows(alvo.len())
        .enumerate()
        .filter(|(_, janela)| *janela == alvo.as_slice())
        .map(|(inicio, _)| (inicio, inicio + alvo.len()))
        .collect()
}

/// A search over what a shell is showing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Busca {
    casamentos: Vec<Casamento>,
    cursor: usize,
}

impl Busca {
    /// Runs the term over the bodies, in drawing order.
    ///
    /// Rebuilt wholesale rather than patched, like `view::project`: an index
    /// that drifts out of step with the list is a bug that only shows up after
    /// an hour of use.
    #[must_use]
    pub fn nova<S: AsRef<str>>(corpos: impl IntoIterator<Item = S>, termo: &str) -> Self {
        let casamentos = corpos
            .into_iter()
            .enumerate()
            .flat_map(|(mensagem, corpo)| {
                ocorrencias(corpo.as_ref(), termo)
                    .into_iter()
                    .map(move |(inicio, fim)| Casamento { mensagem, inicio, fim })
            })
            .collect();
        Self { casamentos, cursor: 0 }
    }

    /// Whether nothing matched.
    #[must_use]
    pub fn vazia(&self) -> bool {
        self.casamentos.is_empty()
    }

    /// Every match, in drawing order.
    ///
    /// A shell that paints the whole history at once — the desktop one does —
    /// needs all of them, not just the one the cursor is on.
    #[must_use]
    pub fn casamentos(&self) -> &[Casamento] {
        &self.casamentos
    }

    /// The match the cursor is on.
    #[must_use]
    pub fn atual(&self) -> Option<Casamento> {
        self.casamentos.get(self.cursor).copied()
    }

    /// `n` — the next match, wrapping past the end.
    pub fn proxima(&mut self) -> Option<Casamento> {
        self.andar(1)
    }

    /// `N` — the previous match, wrapping past the start.
    pub fn anterior(&mut self) -> Option<Casamento> {
        self.andar(-1)
    }

    fn andar(&mut self, passo: isize) -> Option<Casamento> {
        let total = self.casamentos.len();
        if total == 0 {
            return None;
        }
        // Wrapping on purpose: for somebody searching, the last occurrence and
        // the first are neighbours.
        let total_i = isize::try_from(total).unwrap_or(isize::MAX);
        let atual = isize::try_from(self.cursor).unwrap_or(0);
        let proximo = (atual + passo).rem_euclid(total_i);
        self.cursor = usize::try_from(proximo).unwrap_or(0);
        self.atual()
    }

    /// `(1, 3)` for drawing "[1/3]". `(0, 0)` when nothing matched.
    #[must_use]
    pub fn posicao(&self) -> (usize, usize) {
        if self.casamentos.is_empty() {
            return (0, 0);
        }
        (self.cursor + 1, self.casamentos.len())
    }

    /// Which occurrence *within its own message* the cursor is on, from zero.
    ///
    /// A shell that lights matches segment by segment counts them the same way
    /// and needs this to tell the current one from its neighbours.
    #[must_use]
    pub fn ordinal_na_mensagem(&self) -> Option<usize> {
        let atual = self.atual()?;
        Some(
            self.casamentos
                .iter()
                .take(self.cursor)
                .filter(|casamento| casamento.mensagem == atual.mensagem)
                .count(),
        )
    }
}
```

- [ ] **Step 4: Registrar o módulo e conferir o `serde`**

Em `crates/seele-core/src/lib.rs`, na lista de módulos (linhas 27-34), acrescente em ordem alfabética, depois de `pub mod battery;`:

```rust
pub mod busca;
```

`Casamento` deriva `serde::Serialize`. Confirme que o crate já tem a dependência:

Run: `grep -n "^serde" crates/seele-core/Cargo.toml`
Expected: uma linha com `serde`. Se não houver, acrescente `serde = { workspace = true }` seguindo o padrão dos outros crates — **nunca** com versão literal, porque `Cargo.toml:45` diz que versões vivem só no workspace.

- [ ] **Step 5: Rodar os testes**

Run: `cargo test -p seele-core busca`
Expected: PASS, 11 testes.

- [ ] **Step 6: Clippy e formato**

Run: `cargo clippy -p seele-core --all-targets -- -D warnings && cargo fmt --all --check`
Expected: sem saída.

- [ ] **Step 7: Corrigir a assinatura na spec**

Em `docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md`, troque a linha da assinatura por:

```rust
    /// Os corpos já normalizados, na ordem em que a casca os desenha.
    pub fn nova<S: AsRef<str>>(corpos: impl IntoIterator<Item = S>, termo: &str) -> Self;
```

E acrescente, logo abaixo do parágrafo que começa com "**Entra por corpos**":

```markdown
**E os corpos entram normalizados.** `seele-tui::ui::wrap` quebra com
`split_whitespace`, que colapsa espaço repetido; HTML colapsa sozinho. As duas
cascas já mostram o texto colapsado, então `busca::normalizar` no meio é o que
faz o deslocamento devolvido apontar para o que está na tela. Sem isso, um
casamento depois de um espaço duplo apontaria para o lugar errado só na TUI.
```

- [ ] **Step 8: Commit**

```bash
git add crates/seele-core/src/busca.rs crates/seele-core/src/lib.rs docs/superpowers/specs/2026-08-10-navegacao-gui-tui-design.md
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
fn h_e_l_andam_entre_os_paineis_e_dao_a_volta() {
    // `specs/05-cliente-tui.md:42` promete "h j k l / setas navegar", e até
    // agora só j e k faziam alguma coisa.
    let mut app = App::new();
    app.focus = Panel::Dogma;

    app.on_key(Key::Char('l'));
    assert_eq!(app.focus, Panel::Channels);
    app.on_key(Key::Right);
    assert_eq!(app.focus, Panel::Messages);
    // A volta, como o Tab já dá.
    app.on_key(Key::Char('l'));
    assert_eq!(app.focus, Panel::Dogma);

    app.on_key(Key::Char('h'));
    assert_eq!(app.focus, Panel::Messages);
    app.on_key(Key::Left);
    assert_eq!(app.focus, Panel::Channels);
}

#[test]
fn shift_tab_fecha_o_ciclo_que_o_tab_abre() {
    let mut app = App::new();
    let inicio = app.focus;
    app.on_key(Key::Tab);
    assert_ne!(app.focus, inicio);
    app.on_key(Key::BackTab);
    assert_eq!(app.focus, inicio);
}

#[test]
fn h_e_l_nao_escapam_do_modo_de_insercao() {
    // A letra `l` numa mensagem é uma letra, não um comando de foco.
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
            // `h`/`l` movem o foco e não a seleção: com três painéis lado a
            // lado é a leitura natural de esquerda e direita, e `j`/`k` já
            // cobrem o movimento dentro de um painel.
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
- Consumes: `seele_core::busca::{Busca, normalizar}` da Task 1.
- Produces: `App::busca: Option<Busca>`, público; `App::refazer_busca(&mut self)`, público.

- [ ] **Step 1: Escrever os testes que falham**

Acrescente ao `mod tests` de `crates/seele-tui/src/app.rs`:

```rust
fn com_historico() -> App {
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
fn a_busca_encontra_enquanto_se_digita() {
    // Este é o defeito que o plano existe para consertar: o modo entrava, a
    // barra escrevia BUSCA, e o texto era descartado sem ninguém olhar.
    let mut app = com_historico();
    app.on_key(Key::Char('/'));
    for character in "sync".chars() {
        app.on_key(Key::Char(character));
    }
    let busca = app.busca.as_ref().map(seele_core::busca::Busca::posicao);
    assert_eq!(busca, Some((1, 2)));
}

#[test]
fn n_e_N_andam_entre_as_ocorrencias_no_modo_normal() {
    let mut app = com_historico();
    app.on_key(Key::Char('/'));
    for character in "sync".chars() {
        app.on_key(Key::Char(character));
    }
    app.on_key(Key::Enter);
    assert_eq!(app.mode, Mode::Normal);

    app.on_key(Key::Char('n'));
    assert_eq!(app.busca.as_ref().map(seele_core::busca::Busca::posicao), Some((2, 2)));
    app.on_key(Key::Char('N'));
    assert_eq!(app.busca.as_ref().map(seele_core::busca::Busca::posicao), Some((1, 2)));
}

#[test]
fn enter_guarda_o_destaque_e_esc_o_apaga() {
    let mut app = com_historico();
    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Enter);
    assert!(app.busca.is_some(), "confirmar uma busca não pode apagá-la");

    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Esc);
    assert!(app.busca.is_none(), "desistir apaga");
}

#[test]
fn n_sem_busca_ativa_nao_faz_nada_e_nao_estoura() {
    let mut app = com_historico();
    assert_eq!(app.on_key(Key::Char('n')), None);
    assert!(app.busca.is_none());
}

#[test]
fn apagar_o_termo_ate_o_fim_limpa_a_busca() {
    // `refazer_busca` com termo vazio zera a busca inteira, e não deixa uma
    // busca vazia de pé: um contador [0/0] pendurado depois de apagar tudo
    // diria que ainda há uma busca em curso.
    let mut app = com_historico();
    app.on_key(Key::Char('/'));
    app.on_key(Key::Char('s'));
    app.on_key(Key::Backspace);
    assert!(app.busca.is_none());
}

#[test]
fn mensagem_nova_durante_a_busca_e_reencontrada() {
    // Os índices andam quando chega mensagem nova; refazer é o que impede o
    // cursor de apontar para uma linha que mudou de lugar.
    let mut app = com_historico();
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
    assert_eq!(app.busca.as_ref().map(seele_core::busca::Busca::posicao), Some((1, 3)));
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-tui --lib app::tests`
Expected: FAIL — `no field busca on type App`.

- [ ] **Step 3: Implementar**

Em `crates/seele-tui/src/app.rs`, acrescente ao `struct App` depois de `pub input: String,` (linha 245):

```rust
    /// A busca corrente, quando há uma.
    ///
    /// Guardada aqui e não recalculada a cada quadro porque o cursor de `n` e
    /// `N` é estado: recomputar perderia em que ocorrência a pessoa estava.
    pub busca: Option<seele_core::busca::Busca>,
    /// O termo que produziu `busca`, guardado para redesenhar o realce.
    pub termo: String,
```

Em `App::new`, depois de `input: String::new(),`:

```rust
            busca: None,
            termo: String::new(),
```

Acrescente ao `impl App`:

```rust
    /// Refaz a busca sobre o histórico atual, mantendo o termo.
    ///
    /// Chamado quando chega mensagem: os índices andam, e um cursor que não
    /// acompanha aponta para a linha errada. Se a ocorrência corrente sumiu, o
    /// cursor volta à primeira em vez de sair do intervalo.
    pub fn refazer_busca(&mut self) {
        if self.termo.trim().is_empty() {
            self.busca = None;
            return;
        }
        self.busca = Some(seele_core::busca::Busca::nova(
            self.messages
                .iter()
                .chain(&self.local)
                .map(|linha| seele_core::busca::normalizar(&linha.body)),
            &self.termo,
        ));
    }
```

Em `on_normal`, acrescente antes de `Key::Enter =>`:

```rust
            // `n` e `N` estavam livres, e é onde o Vim as põe.
            Key::Char('n') => {
                if let Some(busca) = self.busca.as_mut() {
                    busca.proxima();
                }
            }
            Key::Char('N') => {
                if let Some(busca) = self.busca.as_mut() {
                    busca.anterior();
                }
            }
```

Em `on_key`, substitua o braço `Mode::Search` inteiro (linhas 379-383) por:

```rust
            // A busca acha enquanto se digita, e o contador anda junto: é o
            // retorno que diz se vale continuar escrevendo. `Enter` confirma e
            // volta ao Normal com o destaque; `Esc` desiste e apaga.
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
- Consumes: `App::busca`, `App::termo` da Task 3; `seele_core::busca::ocorrencias` da Task 1.
- Produces: nada que outra tarefa consuma.

- [ ] **Step 1: Escrever o teste que falha**

Acrescente ao `mod tests` de `crates/seele-tui/src/ui.rs`, seguindo o padrão de teste de tela que já existe no arquivo (veja o teste em torno da linha 879, que faz `assert!(screen.contains("MENSAGENS"))`):

```rust
#[test]
fn a_busca_mostra_o_contador_e_marca_a_linha_corrente() {
    // `specs/05-cliente-tui.md:105`: nada pode ser transmitido só por cor. O
    // contador é o acompanhante textual do realce, e é o que sobrevive ao
    // NO_COLOR e a um terminal de 16 cores por SSH.
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
fn uma_busca_sem_resultado_diz_zero_em_vez_de_sumir() {
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
    // O contador é a metade textual do realce. Sem ele, "onde estou nas três
    // ocorrências" seria informação só de cor, que `specs/05:105` proíbe.
    let contador = app.busca.as_ref().map(|busca| {
        let (posicao, total) = busca.posicao();
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
    termo: &str,
    corrente: Option<usize>,
) -> Vec<Line<'static>> {
    let header = format!("{} {}", message.at, message.author);
    let mut lines = vec![Line::from(Span::styled(
        truncate(&header, budget),
        if message.own { theme.accent() } else { theme.label() },
    ))];

    // Conta as ocorrências em ordem, exatamente como o core as conta, para
    // saber qual delas é a corrente. As duas varreduras são da esquerda para a
    // direita sobre o mesmo texto normalizado, então os ordinais batem.
    let mut vistas = 0usize;
    for wrapped in wrap(&message.body, budget.saturating_sub(2)) {
        lines.push(Line::from(realcar(
            &wrapped,
            termo,
            theme,
            corrente,
            &mut vistas,
        )));
    }
    lines
}

/// Um segmento já quebrado, partido em pedaços aceso e apagado.
///
/// O realce é aplicado por segmento, e não por deslocamento no corpo inteiro,
/// porque `wrap` colapsa espaço com `split_whitespace` e um deslocamento
/// calculado no corpo cru apontaria para o lugar errado depois de um espaço
/// duplo.
fn realcar(
    segmento: &str,
    termo: &str,
    theme: Theme,
    corrente: Option<usize>,
    vistas: &mut usize,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("  ".to_owned(), theme.body())];
    if termo.trim().is_empty() {
        spans.push(Span::styled(segmento.to_owned(), theme.body()));
        return spans;
    }

    let caracteres: Vec<char> = segmento.chars().collect();
    let mut cursor = 0usize;
    for (inicio, fim) in seele_core::busca::ocorrencias(segmento, termo) {
        let antes: String = caracteres.iter().skip(cursor).take(inicio - cursor).collect();
        if !antes.is_empty() {
            spans.push(Span::styled(antes, theme.body()));
        }
        let aceso: String = caracteres.iter().skip(inicio).take(fim - inicio).collect();
        let esta = corrente == Some(*vistas);
        spans.push(Span::styled(
            aceso,
            if esta { theme.accent() } else { theme.label() },
        ));
        *vistas += 1;
        cursor = fim;
    }
    let resto: String = caracteres.iter().skip(cursor).collect();
    if !resto.is_empty() {
        spans.push(Span::styled(resto, theme.body()));
    }
    spans
}
```

Em `render_messages`, troque o laço que monta as linhas por um que sabe qual mensagem é a corrente:

```rust
    let corrente = app.busca.as_ref().and_then(seele_core::busca::Busca::atual);
    let ordinal = app
        .busca
        .as_ref()
        .and_then(seele_core::busca::Busca::ordinal_na_mensagem);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (indice, message) in app.messages.iter().chain(&app.local).enumerate() {
        let nesta = corrente.filter(|casamento| casamento.mensagem == indice);
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
    // Com busca ativa, a cauda deixa de ser o que interessa: o que interessa é
    // onde o termo está. Uma ocorrência fora da tela que ninguém rola até é
    // uma ocorrência que não foi achada.
    let skip = match linha_da_corrente {
        Some(linha) if app.busca.is_some() => linha.saturating_sub(visible / 2),
        _ => skip,
    };
```

Para isso, `message_lines` precisa dizer em que linha desenhada a corrente caiu. Acumule enquanto monta: guarde `linha_da_corrente = Some(lines.len())` **antes** de estender com a mensagem que contém `corrente`.

- [ ] **Step 6: Rodar os testes**

Run: `cargo test -p seele-tui && cargo clippy -p seele-tui --all-targets -- -D warnings`
Expected: PASS, sem avisos.

- [ ] **Step 7: Conferir a olho**

Run: `cargo run -p seele-tui --example telas`
O exemplo desenha as telas sem servidor. Confirme que o realce aparece e que o contador está legível.

- [ ] **Step 8: Commit**

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
fn ejetar_deixa_de_ser_sair() {
    // Mudança de comportamento assumida: o botão do app se chama EJETAR e
    // volta à tela de entrada, e o terminal passa a fazer o mesmo. Sair do
    // programa continua sendo `:q`.
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
fn ejetar_e_sair_sao_estados_diferentes() {
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
    /// `:ejetar` — sair deste Dogma e voltar à tela de seleção.
    ///
    /// Separado de [`Command::Quit`] de propósito: sair do programa e sair de
    /// uma conversa são coisas diferentes, e o app já tratava as duas como
    /// diferentes com o botão EJETAR.
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
    /// Marcado quando a sessão acabou mas o programa continua.
    pub ejetou: bool,
```

Em `App::new`, depois de `quit: false,`:

```rust
            ejetou: false,
```

No `impl App`, depois de `quit`:

```rust
    /// Sai deste Dogma sem sair do programa. `:ejetar`.
    pub fn ejetar(&mut self) {
        self.ejetou = true;
    }
```

**Não acrescente uma variante a `Action`.** `:ejetar` chega por `Command::Eject` e é tratado em `run_command`; nenhuma tecla do modo Normal produz a ação, e uma variante que ninguém constrói é código morto que o `unreachable_pub` não pega mas o leitor seguinte tropeça.

- [ ] **Step 5: Partir o `run` em laço e sessão**

Em `crates/seele-tui/src/main.rs`, renomeie a função `run` atual (linha 360) para `sessao`, mude a assinatura para receber `args` já resolvidos e devolver se ejetou:

```rust
/// Uma sessão, do primeiro pacote ao último.
///
/// Devolve `true` quando saiu por `:ejetar`, que é o sinal para o laço de
/// `run` voltar à tela de seleção em vez de encerrar.
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
/// O laço externo: escolher, conversar, ejetar, escolher de novo.
///
/// A sessão inteira vive numa volta deste laço, e `Enlace` e `Voice` são
/// soltos ao fim dela. Isto **não** é o que a pendência #9 recusou — lá era
/// trocar a conexão por baixo de uma sessão viva; aqui é derrubar tudo e
/// voltar a uma tela que não tem roster, telemetria nem áudio.
async fn run(terminal: &mut Screen1, args: Option<Args>, holds: bool) -> Result<()> {
    let home = config_dir();
    let tema = Theme::detect();

    // Com flag, a tela de seleção não aparece no arranque — quem digitou
    // `--server` já disse aonde vai. Ao ejetar ela aparece, e está certo:
    // ejetar é o pedido explícito de ir para outro lugar.
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

### Task 7: App — os comandos que faltavam, e gravar em `conhecidos`

**Files:**
- Modify: `apps/seele-app/src/main.rs` (`Session` em 37, `connect` em 96, `generate_handler!` em 328)

**Interfaces:**
- Consumes: `seele_core::busca::{Busca, normalizar}` da Task 1; `seele_core::conhecidos::{Conhecidos, Conhecido}`; `seele_core::uri::analisar`.
- Produces: os comandos Tauri `conhecidos`, `esquecer`, `analisar_convite`, `buscar`, `busca_andar`, `busca_limpar`; o tipo `BuscaEstado { casamentos: Vec<Casamento>, atual: Option<Casamento>, posicao: u32, total: u32 }`, serializável.

- [ ] **Step 1: Escrever os comandos**

Em `apps/seele-app/src/main.rs`, acrescente ao `struct Session` (linha 37):

```rust
    /// A busca corrente. O cursor é estado de sessão, e é o que impede a regra
    /// de dar-a-volta de ser reescrita em JavaScript.
    busca: Mutex<Option<seele_core::busca::Busca>>,
```

Acrescente o tipo devolvido e os comandos:

```rust
/// O que o frontend precisa saber sobre a busca corrente.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct BuscaEstado {
    /// Onde o termo casou, na ordem em que a tela desenha.
    casamentos: Vec<seele_core::busca::Casamento>,
    /// A ocorrência em que o cursor está.
    atual: Option<seele_core::busca::Casamento>,
    /// Posição de 1, para desenhar "[1/3]". Zero quando não casou nada.
    posicao: u32,
    /// Quantas ao todo.
    total: u32,
}

impl BuscaEstado {
    fn de(busca: &seele_core::busca::Busca) -> Self {
        let (posicao, total) = busca.posicao();
        Self {
            // Todos, e não só o corrente: o app pinta o histórico inteiro de
            // uma vez, e acender só a ocorrência do cursor esconderia as
            // outras que estão na mesma tela.
            casamentos: busca.casamentos().to_vec(),
            atual: busca.atual(),
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
    let busca = seele_core::busca::Busca::nova(
        snapshot
            .messages
            .iter()
            .map(|mensagem| seele_core::busca::normalizar(&mensagem.body)),
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
        busca.proxima();
    } else {
        busca.anterior();
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

`Casamento` já deriva `serde::Serialize` desde a Task 1, e `busca.casamentos()` já existe — nada a acrescentar no core aqui.

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
Expected: compila. Se `Casamento` não serializar, volte ao Passo 1.

- [ ] **Step 5: Rodar os guardas do frontend**

Run: `cargo test -p seele-app`
Expected: **FALHA esperada** em `no_command_is_registered_and_never_called` — os seis comandos novos ainda não são chamados por nenhum `invoke`. A Task 8 os chama. Anote e siga.

- [ ] **Step 6: Commit**

```bash
git add apps/seele-app/src/main.rs crates/seele-core/src/busca.rs
git commit -m "feat(app): os comandos da tela de entrada e da busca

O app nunca tocou em \`conhecidos\` — nem para ler nem para gravar. A
lista na tela é a metade visível; \`registrar\` depois de conectar é a
metade que a faz valer alguma coisa, e sem ela a seção nasceria
permanentemente vazia.

O cursor da busca fica na \`Session\` e não no frontend, porque é ele que
impede a regra de dar-a-volta de ser reescrita em JavaScript. Ler um
\`seele://\` também é Rust: \`specs/06:19\` é inegociável."
```

---

### Task 8: App — a tela de entrada e a busca no painel

**Files:**
- Modify: `apps/seele-app/ui/index.html` (tela de boot em 25-78, painel de mensagens em 148-156)
- Modify: `apps/seele-app/ui/seele.js` (`conectar` em 377, ouvintes em 481)
- Modify: `apps/seele-app/ui/seele.css`

**Interfaces:**
- Consumes: os seis comandos da Task 7.
- Produces: nada.

- [ ] **Step 1: Marcação da tela de entrada**

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

- [ ] **Step 2: Preencher a lista de visitados**

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

- [ ] **Step 3: Ler o convite colado**

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

- [ ] **Step 4: Ligar a busca**

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

- [ ] **Step 5: Acender o termo nas mensagens**

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

**O app tem que desenhar o corpo normalizado, e isto não é detalhe.** O comando `buscar` roda sobre `normalizar(&mensagem.body)` e devolve deslocamentos naquele texto. Fatiar o corpo **cru** com esses deslocamentos erra o alvo em qualquer mensagem com espaço duplo ou quebra de linha — e HTML colapsar espaço na hora de exibir **não** conserta isso, porque o erro é de índice de string, não de pintura.

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

- [ ] **Step 6: Estilo**

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

- [ ] **Step 7: Rodar os guardas**

Run: `cargo test -p seele-app`
Expected: PASS, incluindo `every_command_the_frontend_calls_is_registered`, `no_command_is_registered_and_never_called`, `every_element_the_script_reaches_for_exists_in_the_page` e `the_frontend_never_names_a_protocol_concept`.

Se `the_frontend_never_names_a_protocol_concept` falhar, algum conceito de protocolo vazou para o JS. O conserto é mover a lógica para um comando, nunca relaxar o teste.

- [ ] **Step 8: Conferir a olho**

Run: `cargo tauri dev` (ou o que `docs/como-testar.md` mandar)
Confirme: sem visitados a tela é a de antes; depois de conectar uma vez e ejetar, o Dogma aparece na lista; colar um `seele://` preenche o endereço; um link inválido mostra a frase sem limpar o formulário; a busca acende e o contador anda.

- [ ] **Step 9: Rodar tudo e commitar**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add apps/seele-app/ui/index.html apps/seele-app/ui/seele.js apps/seele-app/ui/seele.css apps/seele-app/src/main.rs crates/seele-core/src/busca.rs
git commit -m "feat(app): a tela de entrada lembra onde você esteve, e a busca busca

A lista de visitados fica acima do formulário e some inteira quando está
vazia — o estado vazio é exatamente a tela de antes, e nada fica
escondido atrás de um clique. Colar um seele:// preenche o endereço, e
quem colou errado não perde o que já tinha digitado.

Os intervalos do realce vêm prontos do Rust: com dobramento de acento o
frontend não teria como saber onde o casamento começou."
```

---

### Task 9: Fechar a documentação

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

- [ ] **Step 2: Atualizar as pendências**

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

- [ ] **Step 3: Commit**

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
                      └─> Task 7 (comandos do app) ──> Task 8 (frontend)
Task 2 (foco) ────────────> (independente)
Task 5 (ejetar) ──────────> Task 6 (conformidade)
Task 9 (docs) ────────────> depois de todas
```

Tasks 2 e 5 não dependem de nada e podem sair primeiro se convier. **A Task 6 é a que carrega o risco** — se ela falhar, a Task 5 precisa voltar à mesa antes de qualquer coisa a jusante.
