# Conferência de impressão digital — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fazer o `fp=` do `seele://` valer nas duas cascas — recusando um primeiro contato que o convite desminta, avisando quando o link discorda de um pin já estabelecido, e anunciando no app o primeiro contato que hoje ele fixa em silêncio.

**Architecture:** A política vira uma função pura em `seele-core::tofu`, que recebe a decisão do TOFU e a impressão esperada e devolve um veredito. As cascas só desenham. `PinDecision::Matches` passa a carregar a impressão — sem isso não há o que comparar. A FFI devolve o veredito na volta do `connect`, onde o valor já está, em vez de emiti-lo num evento que ninguém consegue ouvir.

**Tech Stack:** Rust 2024, rustls/quinn (TOFU em `seele-core::tofu`), Tauri 2 + HTML/CSS/JS sem framework, ratatui.

**Spec:** `docs/superpowers/specs/2026-08-11-conferencia-de-impressao-digital-design.md`

## Global Constraints

- **Lints do workspace são fatais no CI** (`Cargo.toml:29-43`, CI roda clippy com `-D warnings`): `unsafe_code = "forbid"`, `missing_docs = "warn"`, `unreachable_pub = "warn"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `dbg_macro = "deny"`, `indexing_slicing = "warn"`. Use `.get()`, iteradores; `unwrap_or`/`unwrap_or_else` são permitidos, `unwrap()`/`expect()` não. Todo `#[allow(...)]` carrega `reason = "..."`.
- **Idioma (ADR 0023):** `crates/seele-core/src/tofu.rs`, `identity.rs`, `client.rs` e `crates/seele-tui/src/{app,ui}.rs` são **inglês**. `crates/seele-core/src/enlace.rs`, `crates/seele-tui/src/selecao.rs`, `apps/seele-app/src/main.rs` e `ui/seele.js` são **português**. `crates/seele-tui/src/main.rs` é **misto** — acompanhe o que cerca a linha. `docs/` e `specs/` são português.
- **ADR 0002:** `seele-app` só pode depender de `seele-ffi` e `seele-server` (`xtask/src/check_deps.rs:82`). O que vier do core chega reexportado pela `seele-ffi`.
- **`specs/06-clientes-gui.md:19`:** nenhuma lógica de protocolo em JavaScript. A **comparação** fica em Rust; a impressão atravessa a ponte só para ser lida por uma pessoa.
- **`specs/08-seguranca.md`:** o tratamento impossível-de-ignorar é reservado à **troca de chave**. Um convite que discorda não é troca de chave e não ganha esse tratamento.
- **CSS:** só variáveis de `apps/seele-app/ui/tokens.css` (`apps/seele-app/tests/tokens.rs` guarda).
- Verificação: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo xtask check-deps`.

---

### Task 1: A política, pura, no `seele-core`

**Files:**
- Modify: `crates/seele-core/src/tofu.rs` (`PinDecision` em 35-54, `PinStore` em 59-64, `MemoryPinStore`, `TofuVerifier::decide` em 141-153)
- Modify: `crates/seele-core/src/identity.rs` (`impl PinStore for FilePinStore` em 144-164)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `PinDecision::Matches { fingerprint: String }` — a variante deixa de ser vazia
  - `PinStore::unpin(&self, host: &str)` — método novo do trait
  - `seele_core::tofu::Verdict` com cinco variantes (abaixo)
  - `seele_core::tofu::verdict(decision: &PinDecision, expected: Option<&str>) -> Verdict` — pura, sem efeito

- [ ] **Step 1: Escrever os testes que falham**

Acrescente ao `mod tests` de `crates/seele-core/src/tofu.rs`:

```rust
    const A: &str = "aaaa1111";
    const B: &str = "bbbb2222";

    #[test]
    fn a_first_contact_with_no_invite_is_blind_and_says_so() {
        let decision = PinDecision::FirstContact { fingerprint: A.into() };
        assert_eq!(
            verdict(&decision, None),
            Verdict::FirstContact { fingerprint: A.into() }
        );
    }

    #[test]
    fn a_first_contact_the_invite_confirms_is_verified() {
        // ADR 0006 exists to produce exactly this outcome, and until now
        // nothing could tell it apart from the blind one.
        let decision = PinDecision::FirstContact { fingerprint: A.into() };
        assert_eq!(
            verdict(&decision, Some(A)),
            Verdict::FirstContactVerified { fingerprint: A.into() }
        );
    }

    #[test]
    fn a_first_contact_the_invite_contradicts_is_refused() {
        // No prior pin, so the invite was the only evidence, and it failed.
        let decision = PinDecision::FirstContact { fingerprint: A.into() };
        assert_eq!(
            verdict(&decision, Some(B)),
            Verdict::InviteRefused { expected: B.into(), offered: A.into() }
        );
    }

    #[test]
    fn a_matching_pin_with_no_invite_has_nothing_to_say() {
        let decision = PinDecision::Matches { fingerprint: A.into() };
        assert_eq!(verdict(&decision, None), Verdict::Known);
    }

    #[test]
    fn a_matching_pin_the_invite_confirms_has_nothing_to_say_either() {
        let decision = PinDecision::Matches { fingerprint: A.into() };
        assert_eq!(verdict(&decision, Some(A)), Verdict::Known);
    }

    #[test]
    fn a_matching_pin_the_invite_contradicts_warns_and_does_not_refuse() {
        // This is the hole: `plug` compared the expected value with itself,
        // because `Matches` carried no fingerprint to compare against.
        // Trust on first use already proved this is yesterday's server, so
        // the link is what is wrong — refusing would lock somebody out of a
        // Dogma they use because a friend sent a stale link.
        let decision = PinDecision::Matches { fingerprint: A.into() };
        assert_eq!(
            verdict(&decision, Some(B)),
            Verdict::InviteDisagrees { expected: B.into(), offered: A.into() }
        );
    }

    #[test]
    fn the_comparison_ignores_case() {
        let decision = PinDecision::FirstContact { fingerprint: "abcdef".into() };
        assert_eq!(
            verdict(&decision, Some("ABCDEF")),
            Verdict::FirstContactVerified { fingerprint: "abcdef".into() }
        );
    }

    #[test]
    fn unpinning_a_host_makes_the_next_visit_a_first_contact_again() {
        // The refusal has to undo the pin the verifier already wrote, or the
        // next visit without a link walks straight into the server that was
        // just rejected.
        let store = MemoryPinStore::new();
        store.pin("casa", A.into());
        assert_eq!(store.pinned("casa"), Some(A.into()));

        store.unpin("casa");
        assert_eq!(store.pinned("casa"), None);
    }

    #[test]
    fn unpinning_a_host_that_was_never_pinned_is_not_an_error() {
        let store = MemoryPinStore::new();
        store.unpin("nunca visto");
        assert_eq!(store.pinned("nunca visto"), None);
    }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core tofu`
Expected: FAIL na compilação — `cannot find type Verdict`, `no function verdict`, `no method unpin`, e `PinDecision::Matches` não aceita campo.

- [ ] **Step 3: `PinDecision::Matches` carrega a impressão**

Em `crates/seele-core/src/tofu.rs`, substitua a variante (linha 45-46):

```rust
    /// The certificate matches what was pinned.
    ///
    /// Carries the fingerprint because a caller comparing an invite against
    /// what the server offered needs something to compare *with*. Without it
    /// the terminal client ended up comparing the expected value with itself,
    /// which is a test that cannot fail.
    Matches {
        /// The fingerprint that both the pin and the certificate carry.
        fingerprint: String,
    },
```

Em `TofuVerifier::decide` (linha 150):

```rust
            Some(pinned) if pinned == offered => PinDecision::Matches { fingerprint: offered },
```

- [ ] **Step 4: O trait ganha `unpin`**

Em `tofu.rs`, no `pub trait PinStore` (linha 59-64), acrescente:

```rust
    /// Forgets the fingerprint pinned for a host.
    ///
    /// Exists because refusing a connection is not enough on its own: the
    /// verifier pins before anyone can judge, so a refusal that left the pin
    /// behind would let the very next visit — without a link to check against
    /// — walk into the server that was just rejected.
    fn unpin(&self, host: &str);
```

No `impl PinStore for MemoryPinStore`, acrescente:

```rust
    fn unpin(&self, host: &str) {
        if let Ok(mut pins) = self.pins.lock() {
            pins.remove(host);
        }
    }
```

Em `crates/seele-core/src/identity.rs`, no `impl PinStore for FilePinStore` (linha 144-164):

```rust
    fn unpin(&self, host: &str) {
        let Ok(mut pins) = self.pins.lock() else {
            return;
        };
        pins.retain(|(known, _)| known != host);
        self.flush(&pins);
    }
```

- [ ] **Step 5: O veredito e a política**

Ainda em `tofu.rs`, depois de `PinDecision`:

```rust
/// O que a conferência concluiu — já decidido, para a casca só desenhar.
///
/// Cinco variantes porque são cinco coisas distintas a dizer. `PinDecision`
/// descreve o que o TOFU viu; isto descreve o que fazer a respeito, e é a
/// diferença entre as duas que faz a regra existir num lugar só.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was pinned and no invite vouched for anything. Pinned blind.
    ///
    /// `specs/08-seguranca.md` wants this stated rather than accepted in
    /// silence — the shell must say what it just trusted.
    FirstContact {
        /// What was pinned.
        fingerprint: String,
    },
    /// Nothing was pinned, and the invite confirmed what the server offered.
    ///
    /// This is what ADR 0006 invented the link to produce.
    FirstContactVerified {
        /// What was pinned, now vouched for.
        fingerprint: String,
    },
    /// The pin matches and nothing contradicts it. Nothing to say.
    Known,
    /// First contact, and the invite named a different key. Refused.
    InviteRefused {
        /// What the link promised.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// The pin is the usual one, but the invite names a different key.
    ///
    /// The connection stands: trust on first use already established that this
    /// is the same server as before, so the link is what is wrong.
    InviteDisagrees {
        /// What the link promised.
        expected: String,
        /// What the server offered, and what stays pinned.
        offered: String,
    },
}

/// Turns what the TOFU verifier saw into what to do about it.
///
/// Pure on purpose: the refusal's side effect — removing the pin the verifier
/// already wrote — belongs to the caller, so this can be tested as the table
/// it is.
#[must_use]
pub fn verdict(decision: &PinDecision, expected: Option<&str>) -> Verdict {
    let agrees = |offered: &str| {
        expected.is_none_or(|expected| expected.eq_ignore_ascii_case(offered))
    };

    match decision {
        PinDecision::FirstContact { fingerprint } if agrees(fingerprint) => {
            if expected.is_some() {
                Verdict::FirstContactVerified {
                    fingerprint: fingerprint.clone(),
                }
            } else {
                Verdict::FirstContact {
                    fingerprint: fingerprint.clone(),
                }
            }
        }
        PinDecision::FirstContact { fingerprint } => Verdict::InviteRefused {
            expected: expected.unwrap_or_default().to_owned(),
            offered: fingerprint.clone(),
        },
        PinDecision::Matches { fingerprint } if agrees(fingerprint) => Verdict::Known,
        PinDecision::Matches { fingerprint } => Verdict::InviteDisagrees {
            expected: expected.unwrap_or_default().to_owned(),
            offered: fingerprint.clone(),
        },
        // `Changed` never reaches here: the verifier refuses it at the TLS
        // layer, with or without an invite, and it surfaces as a connection
        // error rather than a verdict.
        PinDecision::Changed { pinned, offered } => Verdict::InviteRefused {
            expected: pinned.clone(),
            offered: offered.clone(),
        },
    }
}
```

**Atenção ao `unwrap_or_default`:** os dois ramos que o usam só são alcançáveis quando `agrees` foi falso, o que exige `expected.is_some()`. É `unwrap_or_default` e não `unwrap()` porque o segundo é proibido pelo lint, e um `String::new()` inalcançável é melhor que um `expect` com razão inventada.

- [ ] **Step 6: Consertar os casadores que quebraram**

`PinDecision::Matches` mudou de forma. Ache todos:

Run: `grep -rn "PinDecision::Matches" crates/ apps/`

Espere quatro lugares: `crates/seele-core/src/tofu.rs` (o próprio `decide` e os testes), `crates/seele-tui/src/main.rs:507`, `crates/seele-ffi/src/lib.rs:678`. Nas Tasks 3 e 4 esses caminhos são reescritos; **por ora só faça compilar**, casando `PinDecision::Matches { .. }` e preservando o comportamento atual.

- [ ] **Step 7: Rodar os testes**

Run: `cargo test -p seele-core tofu && cargo build --workspace`
Expected: PASS, 9 testes novos, workspace compila.

- [ ] **Step 8: Clippy, formato, commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add crates/seele-core/src/tofu.rs crates/seele-core/src/identity.rs crates/seele-tui/src/main.rs crates/seele-ffi/src/lib.rs
git commit -m "feat(core): o veredito de identidade, decidido num lugar só

\`PinDecision::Matches\` era um marcador vazio, e por isso o \`plug\`
comparava a impressão esperada consigo mesma — um teste que não podia
falhar. Agora ela carrega o que o pin e o certificado têm em comum.

A política é uma função pura porque é uma tabela: seis situações, cinco
vereditos. O efeito da recusa — desfazer o pin que o verificador já
escreveu — fica com quem chama, senão não daria para testar a tabela
como tabela."
```

---

### Task 2: Ligar a impressão esperada ao `Enlace`

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (`Destino` em 55-66, `Enlace::conectar` em ~200-215, `pin_decision` em 284)

**Interfaces:**
- Consumes: `Verdict`, `verdict()`, `PinStore::unpin` da Task 1.
- Produces: `Destino::impressao_esperada: Option<String>`; `Enlace::veredito(&self) -> &Verdict`.

- [ ] **Step 1: Escrever o teste que falha**

Acrescente ao `mod tests` de `crates/seele-core/src/enlace.rs` (arquivo português, teste em português como os vizinhos — confira com `grep -n "fn " crates/seele-core/src/enlace.rs | tail -20`):

```rust
    #[test]
    fn uma_recusa_desfaz_o_pin_que_o_verificador_acabou_de_escrever() {
        // Sem isto a recusa é decorativa: a visita seguinte, sem link para
        // conferir, veria `Matches` e entraria no servidor recém-rejeitado.
        let loja = std::sync::Arc::new(seele_core::MemoryPinStore::new());
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, loja.as_ref(), "casa");

        assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
    }

    #[test]
    fn um_veredito_que_nao_recusa_deixa_o_pin_onde_esta() {
        let loja = std::sync::Arc::new(seele_core::MemoryPinStore::new());
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, loja.as_ref(), "casa");

        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core enlace`
Expected: FAIL — `cannot find function aplicar_veredito`.

- [ ] **Step 3: `Destino` ganha o campo**

Em `crates/seele-core/src/enlace.rs`, no `pub struct Destino` (linha 55-66), depois de `segredo`:

```rust
    /// A impressão digital que o convite prometeu, quando veio de um link.
    ///
    /// `None` para quem digitou o endereço à mão — aí não há o que conferir, e
    /// o primeiro contato segue sendo cego, como sempre foi.
    pub impressao_esperada: Option<String>,
```

- [ ] **Step 4: O efeito da recusa**

Ainda em `enlace.rs`, no nível do módulo:

```rust
/// Aplica o que o veredito manda fazer com o pin.
///
/// Separado da decisão porque a decisão é uma tabela pura e isto é um efeito.
/// Só a recusa tem efeito: ela desfaz o pin que o verificador escreveu antes
/// de alguém poder julgar.
fn aplicar_veredito(veredito: &Verdict, pins: &dyn PinStore, chave_do_pin: &str) {
    if matches!(veredito, Verdict::InviteRefused { .. }) {
        pins.unpin(chave_do_pin);
    }
}
```

- [ ] **Step 5: Produzir o veredito na conexão**

Em `Enlace::conectar`, onde hoje está `let pin = cliente.pin_decision().clone();` (linha ~207), passe a calcular e guardar o veredito, aplicando o efeito e devolvendo erro quando for recusa. Guarde o `Verdict` no `Enlace` ao lado do `PinDecision` que já existe, e exponha:

```rust
    /// O que a conferência de identidade concluiu nesta conexão.
    #[must_use]
    pub fn veredito(&self) -> &Verdict {
        &self.veredito
    }
```

Uma recusa tem de **derrubar a conexão**, não só relatar. **Não reúse `ConnectError::PinChanged`**: um convite que não confere não é uma chave trocada, e a frase que a casca desenha para `PinChanged` diria a coisa errada. Acrescente uma variante ao `ConnectError` (arquivo português, então nome português):

```rust
    /// O convite prometia outra identidade, e não havia pin para desempatar.
    ///
    /// Distinta de [`ConnectError::PinChanged`] de propósito: lá a chave de um
    /// servidor conhecido mudou, que é o alerta do ADR 0003. Aqui nunca houve
    /// chave conhecida, e quem discorda é o link.
    InviteMismatch {
        /// O que o link prometeu.
        expected: String,
        /// O que o servidor ofereceu.
        offered: String,
    },
```

E inclua-a em `vale_insistir` (linha 642-647), ao lado de `PinChanged` e `Refused` — martelar contra um convite errado só repetiria o mesmo erro a cada backoff:

```rust
        ConnectError::PinChanged { .. }
            | ConnectError::Refused { .. }
            | ConnectError::InviteMismatch { .. }
```

Acrescente também um caso ao teste `insistir_contra_recusa_nao_muda_a_resposta` (linha 653), afirmando `!vale_insistir(&ConnectError::InviteMismatch { … })`.

- [ ] **Step 6: Rodar tudo**

Run: `cargo test -p seele-core && cargo clippy -p seele-core --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS. Os chamadores de `Destino` em `seele-tui` e `seele-ffi` vão quebrar por campo faltando — acrescente `impressao_esperada: None` nos dois por ora; as Tasks 3 e 4 os preenchem de verdade.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-core/src/enlace.rs crates/seele-tui/src/main.rs crates/seele-ffi/src/lib.rs
git commit -m "feat(core): a impressão do convite chega ao enlace, e a recusa desfaz o pin

O verificador fixa a chave dentro do retorno de chamada do TLS, antes de
existir alguém para julgar. Então recusar sem desfixar seria teatro: a
visita seguinte, sem link, veria \`Matches\` e entraria sem hesitar no
servidor que acabou de ser rejeitado."
```

---

### Task 3: A FFI devolve o veredito em vez de emiti-lo para o vazio

**Files:**
- Modify: `crates/seele-ffi/src/lib.rs` (`ConnectConfig` em 77-96, `Plug::connect` em 201-272, `drive` em ~665-690)
- Modify: `crates/seele-ffi/src/types.rs` (`Trust` em 196-208)

**Interfaces:**
- Consumes: `Verdict` da Task 1, `Destino::impressao_esperada` da Task 2.
- Produces: `ConnectConfig::expected_fingerprint: Option<String>`; `Plug::connect(config) -> Result<(Arc<Plug>, Trust), PlugError>`; `Trust` espelhando as cinco variantes de `Verdict`.

- [ ] **Step 1: Ler o que já está lá**

`crates/seele-ffi/src/lib.rs:264` é `let trust = ready_rx.recv().map_err(|_| PlugError::Unreachable)??;` — **o veredito já está na mão**. A linha 271 o joga num `Event::Connected` que nenhuma casca consegue ouvir, porque `connect` só devolve o `Arc<Plug>` depois de notificar, e é do `Arc` que se assina.

Confirme que o evento é mesmo inobservável antes de removê-lo:

Run: `grep -rn "Event::Connected" crates/ apps/`
Se algum teste o observa, ele o observa por dentro, não por uma casca; diga no relatório o que achou.

- [ ] **Step 2: Escrever o teste que falha**

Acrescente ao `mod tests` de `crates/seele-ffi/src/types.rs` (arquivo inglês):

```rust
    #[test]
    fn every_verdict_the_core_produces_has_a_shell_facing_twin() {
        // Trust used to have two variants where the core now has five, and
        // folding five into two would throw away exactly the information this
        // work exists to create.
        use seele_core::tofu::Verdict;

        let cases = [
            Verdict::FirstContact { fingerprint: "a".into() },
            Verdict::FirstContactVerified { fingerprint: "a".into() },
            Verdict::Known,
            Verdict::InviteRefused { expected: "b".into(), offered: "a".into() },
            Verdict::InviteDisagrees { expected: "b".into(), offered: "a".into() },
        ];

        let seen: std::collections::BTreeSet<String> = cases
            .iter()
            .map(|verdict| format!("{:?}", Trust::from(verdict.clone())))
            .collect();

        assert_eq!(seen.len(), cases.len(), "two verdicts collapsed into one Trust");
    }
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p seele-ffi types`
Expected: FAIL — `Trust: From<Verdict>` não existe.

- [ ] **Step 4: `Trust` espelha o veredito**

Em `crates/seele-ffi/src/types.rs`, substitua o `enum Trust` (196-208) por cinco variantes com os mesmos nomes e campos do `Verdict`, mantendo `serde::Serialize` e os doc comments que o `Verdict` já carrega — **em inglês**, como o arquivo. Acrescente:

```rust
impl From<seele_core::tofu::Verdict> for Trust {
    fn from(verdict: seele_core::tofu::Verdict) -> Self {
        use seele_core::tofu::Verdict;

        // An exhaustive match rather than a blanket conversion: when the core
        // grows a sixth verdict this stops compiling, instead of silently
        // mapping it onto one that already exists.
        match verdict {
            Verdict::FirstContact { fingerprint } => Self::FirstContact { fingerprint },
            Verdict::FirstContactVerified { fingerprint } => {
                Self::FirstContactVerified { fingerprint }
            }
            Verdict::Known => Self::Known,
            Verdict::InviteRefused { expected, offered } => {
                Self::InviteRefused { expected, offered }
            }
            Verdict::InviteDisagrees { expected, offered } => {
                Self::InviteDisagrees { expected, offered }
            }
        }
    }
}
```

`InviteRefused` chega aqui apenas pela completude do tipo: a Task 2 fez a recusa derrubar a conexão, então na prática ela sai como `ConnectError` e nunca como `Trust`. Manter o braço é o que faz o `match` ser exaustivo, e é o que quebra a compilação se algum dia a recusa deixar de derrubar.

- [ ] **Step 5: `ConnectConfig` e o retorno do `connect`**

Em `crates/seele-ffi/src/lib.rs`, no `ConnectConfig` (77-96), depois de `join_secret`:

```rust
    /// A impressão digital que o convite prometeu, quando veio de um link.
    pub expected_fingerprint: Option<String>,
```

Passe-a ao `Destino` dentro de `drive`. Troque a assinatura:

```rust
    pub fn connect(config: ConnectConfig) -> Result<(Arc<Self>, Trust), PlugError> {
```

e o fim do corpo (264-272): devolva `Ok((plug, trust))` em vez de notificar. **Remova o `Event::Connected`** e sua variante, se o Passo 1 confirmou que ninguém a observa — um evento que não pode ser ouvido é código morto que parece vivo.

- [ ] **Step 6: Rodar e commitar**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add crates/seele-ffi/src/lib.rs crates/seele-ffi/src/types.rs apps/seele-app/src/main.rs
git commit -m "feat(ffi): o veredito volta do connect, onde o valor já estava

\`ready_rx.recv()\` sempre devolveu o veredito do handshake, e a linha
seguinte o jogava num \`Event::Connected\` que nenhuma casca consegue
ouvir: assina-se a partir do \`Arc<Plug>\`, e \`connect\` só o devolve
depois de notificar. O valor existia, estava certo, e era descartado.

\`Trust\` passa a ter as cinco variantes do core. Dobrar cinco em duas
jogaria fora justamente a informação nova."
```

---

### Task 4: O `plug` para de conferir sozinho

**Files:**
- Modify: `crates/seele-tui/src/main.rs` (bloco de conferência em 502-521, alerta de primeiro contato em ~525-531)

**Interfaces:**
- Consumes: `Verdict`, `Enlace::veredito()`, `Destino::impressao_esperada`.
- Produces: nada.

- [ ] **Step 1: Passar a impressão e apagar a comparação**

`Args::expected_fingerprint` já existe e já vem do link (`main.rs:61-62`). Ligue-o ao `Destino` — o campo da Task 2 — e **apague o bloco inteiro de 502-521**, aquele que compara `oferecida` com `esperada`. Ele deixa de existir: a decisão vem pronta.

- [ ] **Step 2: Desenhar os cinco vereditos**

Substitua o bloco apagado e o alerta de primeiro contato por um `match` sobre `client.veredito()`:

| Veredito | O que o `plug` faz |
|---|---|
| `FirstContact { fingerprint }` | `Alert` não bloqueante: `PRIMEIRO CONTATO — CHAVE FIXADA {fingerprint}` (texto de hoje, preservado) |
| `FirstContactVerified { fingerprint }` | `Alert` não bloqueante dizendo que o convite confirmou |
| `Known` | nada |
| `InviteDisagrees { expected, offered }` | `Alert` não bloqueante: o link não corresponde a este Dogma, com as duas |
| `InviteRefused { .. }` | não chega aqui — a Task 2 fez a recusa derrubar a conexão, e o caminho de erro já existe |

O texto da recusa continua sendo o de hoje, com `\n\n` e `\n` separando as duas impressões — e agora sai **alinhado**, porque `render_lost` passou a preservar quebras no commit `c70bf08`.

- [ ] **Step 3: Rodar**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Se algum teste afirmava o comportamento antigo do bloco apagado, ele afirmava o buraco — atualize e diga no relatório.

- [ ] **Step 4: Commit**

```bash
git add crates/seele-tui/src/main.rs
git commit -m "fix(tui): o convite passa a ser conferido também no Dogma já conhecido

O ramo \`Matches\` comparava a impressão esperada consigo mesma, porque
não havia impressão do outro lado para comparar. Um Dogma fixado com A e
um link prometendo B conectavam calados, sob um comentário que dizia
transformar primeiro contato cego em verificado.

As doze linhas saíram. A decisão agora vem pronta do core, e esta casca
só desenha."
```

---

### Task 5: O app diz o que fixou, e o que não bateu

**Files:**
- Modify: `apps/seele-app/src/main.rs` (`connect` em ~112-200, `disconnect`, `ConviteLido` em 418-435)
- Modify: `apps/seele-app/ui/seele.js` (`conectar`, `lerConvite`, `#boot-aviso`)
- Modify: `apps/seele-app/tests/frontend.rs`

**Interfaces:**
- Consumes: `Plug::connect -> (Arc<Plug>, Trust)` e `ConnectConfig::expected_fingerprint` da Task 3.
- Produces: nada.

- [ ] **Step 1: Passar a impressão guardada**

`Session.convite` já guarda o `Convite` inteiro, com `impressao_digital`. Em `connect`, preencha `ConnectConfig::expected_fingerprint` a partir dele — o descarte por `alvo` diferente já existe e continua valendo.

- [ ] **Step 2: Limpar o convite ao sair**

No comando `disconnect`, limpe `session.convite` junto com `plug` e `hospedagem`. Enquanto o app não conferia nada isso era inerte; deixou de ser: reconectar sem recolar reutilizaria a impressão de um link anterior.

- [ ] **Step 3: O veredito atravessa a ponte**

`ConviteLido::conferencia_pendente` some — não há mais conferência pendente. Em vez dele, `connect` devolve o veredito junto do `Snapshot`. A impressão digital **atravessa** agora, e o doc comment que explicava por que ela não atravessava precisa ser reescrito, não apagado: ele registra uma decisão que se inverteu, e o motivo da inversão é que a comparação agora acontece em Rust e a string vai para ser lida por uma pessoa.

- [ ] **Step 4: Desenhar**

| Veredito | O que aparece |
|---|---|
| `FirstContact` | `#boot-aviso` laranja com a impressão, dizendo para conferir por outro canal |
| `FirstContactVerified` | confirmação discreta — o link bateu |
| `Known` | nada |
| `InviteDisagrees` | `#boot-aviso` laranja: o link não corresponde a este Dogma |
| `InviteRefused` | a conexão falha; `#boot-erro` com as duas impressões |

O vermelho continua reservado ao que impede de entrar. `tokens.css:19` marca `--seele-vermelho-alerta` como "EXCLUSIVO alerta e queda", e `specs/08-seguranca.md` reserva o tratamento impossível-de-ignorar à troca de chave.

- [ ] **Step 5: Guardas**

Dois guardas em `apps/seele-app/tests/frontend.rs` (arquivo inglês). O primeiro é o que importa e tem de ser **executável**, não textual — três testes desta natureza já nasceram incapazes de falhar neste projeto:

```rust
#[test]
fn every_verdict_the_bridge_can_send_has_its_own_sentence_in_the_page() {
    // Uma variante sem frase é uma tela em branco no momento em que ela mais
    // precisa dizer algo. Serializa o Trust de verdade, para renomear uma
    // variante quebrar isto em vez de deixar um ramo morto.
    let script = read("ui/seele.js");
    for verdict in [
        seele_ffi::Trust::FirstContact { fingerprint: "a".into() },
        seele_ffi::Trust::FirstContactVerified { fingerprint: "a".into() },
        seele_ffi::Trust::Known,
        seele_ffi::Trust::InviteDisagrees { expected: "b".into(), offered: "a".into() },
    ] {
        let Ok(json) = serde_json::to_string(&verdict) else {
            panic!("Trust não serializa");
        };
        // O nome da variante, como serde o escreve, tem de aparecer no script.
        let Some(nome) = json.trim_matches('"').split("\":").next() else {
            panic!("forma inesperada: {json}");
        };
        let nome = nome.trim_start_matches('{').trim_matches('"');
        assert!(
            script.contains(nome),
            "o veredito {nome} não tem tratamento no script"
        );
    }
}
```

`InviteRefused` fica de fora da lista de propósito: ele sai como erro de conexão, não como veredito, e sua frase vive no caminho de `#boot-erro`.

O segundo guarda é textual e vale pelo que proíbe: `assert!(!script.contains("toLowerCase()"))` na vizinhança da impressão digital não serve — `toLowerCase` tem outros usos legítimos. Em vez disso, afirme que o script **não contém a palavra** `impressao_digital` nem `fingerprint` fora de um identificador de elemento: a comparação só pode acontecer em Rust, e o frontend só recebe o veredito já decidido mais a string a exibir. Se este guarda for difícil de escrever sem falso positivo, **diga isso no relatório em vez de escrever um que passa sempre** — o primeiro teste é o que carrega a garantia.

Os guardas existentes já cobrem comandos e ids.

- [ ] **Step 6: Rodar e commitar**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check && cargo xtask check-deps
git add apps/seele-app crates/seele-ffi
git commit -m "feat(app): o app diz o que fixou, e recusa o link que não bate

Ele fixava chaves em silêncio — \`tofu.rs\` manda a casca dizer o que
acabou de confiar, e esta não dizia. E lia o \`fp=\` do convite só para
descartá-lo, o que fazia de colar um link exatamente tão cego quanto
digitar o endereço na mão.

A impressão atravessa a ponte agora, e é inversão de uma decisão
anterior: antes ela ficava em Rust porque o app não sabia conferir e
mandar o valor convidaria a comparação a ser reescrita em JavaScript.
Agora o veredito chega pronto, e a string vai para ser lida por uma
pessoa."
```

---

### Task 6: Conformidade e documentação

**Files:**
- Create: `crates/seele-conformance/tests/convite.rs`
- Modify: `docs/pendencias.md` (entrada 12)
- Modify: `specs/06-clientes-gui.md`

- [ ] **Step 1: Ler o padrão**

Run: `sed -n '1,60p' crates/seele-conformance/tests/ejetar.rs`
Copie dali a forma de subir um Dogma e conectar — helpers `dogma`, `destino`, `esperar`, `ocupantes`. **Não invente uma segunda forma.** Repare que `destino` já recebe apelido como parâmetro, e que os testes usam `#[tokio::test]` de uma thread quando a ordem de desmonte importa.

- [ ] **Step 2: Os três testes**

Crie `crates/seele-conformance/tests/convite.rs` (português, como os vizinhos):

1. **A impressão certa verifica.** Sobe um Dogma, lê a impressão real dele, conecta com ela no `Destino::impressao_esperada`, afirma `Verdict::FirstContactVerified`.
2. **A impressão errada recusa, e desfixa.** Conecta com uma impressão que não é a do Dogma; afirma que a conexão falhou **e** que reconectar sem link dá `FirstContact` de novo, não `Known`. Sem esta segunda metade o teste passaria com o pin intacto e a recusa seria decorativa.
3. **O Dogma já conhecido avisa e não derruba.** Conecta uma vez sem link (fixa), depois conecta de novo com um link que discorda; afirma `InviteDisagrees` **e que a sessão está viva** — mande uma mensagem e espere ouvi-la de volta, como o `ejetar.rs` faz.

Depois de cada teste passar, **quebre o que ele guarda e confirme que fica vermelho**: no 2, pule o `unpin`; no 3, volte `verdict` a devolver `Known`. Restaure e relate os dois resultados.

- [ ] **Step 3: `docs/pendencias.md`**

A entrada 12 descreve exatamente o que este trabalho consertou. Reescreva-a para o que **sobrou**, sem renumerar nada — "pendência #9" é citada em vários lugares de `docs/` e `specs/`, e o arquivo ordena por quanto atrapalha. Se nada sobrou, diga isso e por quê, com a data.

- [ ] **Step 4: `specs/06-clientes-gui.md`**

A seção da tela de entrada diz que o app não confere a impressão. Passou a conferir. Reescreva em prosa declarativa — o que o software **é**, não o que mudou.

- [ ] **Step 5: Commit**

```bash
git add crates/seele-conformance/tests/convite.rs docs/pendencias.md specs/06-clientes-gui.md
git commit -m "test,docs: o convite conferido de ponta a ponta, e as specs alcançam

O teste da recusa afirma as duas metades: que a conexão caiu e que não
sobrou pin. Só a primeira passaria com a recusa decorativa que este
trabalho existiu para tirar."
```

---

## Ordem e dependências

```
Task 1 (política pura) ──> Task 2 (efeito no enlace) ─┬─> Task 3 (FFI) ──> Task 5 (app)
                                                      └─> Task 4 (plug)
                                                          Task 6 (conformidade + docs) ← depois de 4 e 5
```

Task 1 é a única sem dependências e a única inteiramente pura. **A Task 2 carrega o risco**: é ela que faz a recusa desfazer o pin, e a Task 6 é quem prova que desfez.
