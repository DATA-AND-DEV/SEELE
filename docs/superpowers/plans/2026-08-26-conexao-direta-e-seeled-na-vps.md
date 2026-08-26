# Conexão direta e `seeled` na VPS — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remover a escada de alcançabilidade do ADR 0022 e o IPv6 do caminho de conexão, reduzindo o convite a um endereço mais uma impressão digital, e transformar o `seeled` num artefato de VPS de primeira classe.

**Architecture:** Poda, não reescrita. O enum `Tipo` em `seele-server/src/alcance.rs` já classifica candidatos em `Local`/`Global`/`Tunel` — que são LAN, VPS e Tailscale literalmente — e em três variantes que existem só para pedir algo a um roteador ou a um ponto de encontro. As três saem, junto com o IPv6 e com a lista de candidatos do convite. O que sobra é uma pergunta síncrona: *quais IPv4 desta máquina servem para receber gente?*

**Tech Stack:** Rust 1.97 (MSRV do workspace), quinn/QUIC, rustls com provider `ring`, rusqlite `bundled`, Tauri 2 no app, GitHub Actions para release.

**Spec:** `docs/superpowers/specs/2026-08-26-conexao-direta-e-seeled-na-vps-design.md` — leia antes da Task 1. Este plano argumenta a partir dele e não o repete.

## Global Constraints

Valem para **toda** task. Não são repetidas em cada uma.

- **O portão é o workspace inteiro, nunca a crate afetada.** Ao fim de cada task: `cargo fmt --all`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`, `cargo xtask check-deps`. Esta regra foi aprendida em 25/08, quando três portões estreitos deram falsa confiança no mesmo dia.
- **`unsafe_code = "forbid"`** no workspace. Nenhuma task o relaxa.
- **`unwrap_used` e `expect_used` são `deny`** fora de teste. Dentro de teste são liberados por `cfg_attr` que já existe em cada crate.
- **Idioma:** código, comentários, doc-comments e mensagens de commit em português (ADR 0023, ADR 0013). Os módulos em inglês que já existem (`session.rs`, `permissions.rs`) ficam como estão — nenhuma task deste plano os traduz.
- **Comentários explicam *por quê*, nunca *o quê*.** É a convenção mais visível desta base; um comentário que narra a linha abaixo dele será rejeitado na revisão.
- **Compatibilidade de link:** parâmetro desconhecido é **ignorado**, nunca recusado (`uri.rs`, regra já escrita). A única exceção deste plano é um `alvo` IPv6, que é recusado — Task 11.
- **Um commit por task.** Mensagens no formato `tipo(escopo): frase`, com `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>` no fim.
- **Ninguém empurra nada.** `git push` não aparece em nenhuma task.

## Mapa de arquivos

| Arquivo | Destino |
|---|---|
| `crates/seele-encontro/` (crate inteira) | apagada — Task 4 |
| `crates/seele-proto/src/encontro.rs` | apagado — Task 4 |
| `crates/seele-proto/src/uri.rs` | `Bilhete` sai (T3), `alt=` sai (T13), IPv6 recusado (T11) |
| `crates/seele-core/src/encontro.rs` | apagado — Task 1 |
| `crates/seele-core/src/enlace.rs` | furo sai (T1), corrida sai (T12) |
| `crates/seele-core/src/chegada.rs` | etapas de furo saem (T1), etapas novas (T14) |
| `crates/seele-server/src/alcance/encontro.rs` | apagado — Task 2 |
| `crates/seele-server/src/alcance/pcp.rs` | apagado — Task 5 |
| `crates/seele-server/src/alcance/porta.rs` | apagado — Task 5 |
| `crates/seele-server/src/alcance/interfaces.rs` | **intocado** |
| `crates/seele-server/src/alcance/firewall.rs` | **intocado** |
| `crates/seele-server/src/alcance.rs` | `Pilha` sai (T9), vira `Enderecos` (T15) |
| `crates/seele-server/src/hospedagem.rs` | acompanha T2, T15 |
| `crates/seele-server/src/main.rs` | escuta IPv4 (T10), guarda do aberto (T21) |
| `crates/seele-ffi/src/lib.rs` | `ConnectStage` (T1, T14), `resolve()` assíncrono (T16) |
| `crates/seele-tui/src/rede.rs` | apagado — Task 4 |
| `apps/seele-app/ui/frases.js` | etapas (T1, T14) |
| `apps/seele-app/ui/tela-boot.js` | acompanha T14 |
| `apps/seele-app/ui/tela-hospedar.js` (ou equivalente) | lista de interfaces — T17 |
| `.github/workflows/release.yml` | alvos musl e nome — T18, T19 |
| `install.sh` / `install.ps1` | nome e aarch64 — T19 |
| `empacotar/seele.service` | criado — T20 |
| `docs/vps.md` | criado — T20 |
| `docs/adr/0036/0037/0038` | criados — T22 |

---

# Fase 1 · O degrau 4 sai

### Task 1: O cliente para de furar NAT

**Files:**
- Modify: `crates/seele-core/src/enlace.rs`
- Modify: `crates/seele-core/src/chegada.rs`
- Modify: `crates/seele-core/src/lib.rs`
- Modify: `crates/seele-ffi/src/lib.rs:223-270`
- Modify: `apps/seele-app/ui/frases.js:166-180`
- Delete: `crates/seele-core/src/encontro.rs`
- Test: `crates/seele-core/src/chegada.rs` (módulo `tests` no fim do arquivo)

**Interfaces:**
- Consumes: nada de tasks anteriores — esta é a primeira.
- Produces: `Etapa` sem as variantes `Avisando` e `CaminhoAberto`; `Etapa::Parada` sem o campo `com_bilhete_e_impressao`. `Enlace::conectar_entre_com_bilhete` continua existindo com a mesma assinatura, mas o parâmetro `bilhete` deixa de ser lido — ele só desaparece na Task 3, e essa ordem é o que mantém o workspace compilando.

- [ ] **Step 1: Escrever o teste que falha**

Em `crates/seele-core/src/chegada.rs`, no módulo `tests`:

```rust
#[test]
fn uma_chegada_nunca_avisa_ponto_de_encontro() {
    // O degrau 4 saiu. Nenhuma trilha pode conter a etapa que existia só para
    // ele, e a máquina de estados não pode nem sequer nomeá-la: uma transição
    // para um estado que não existe é um erro de compilação, e é assim que esta
    // remoção fica permanente em vez de virar convenção.
    let etapas = ["Parada", "Tentando", "Dentro", "Desistiu"];
    for nome in etapas {
        assert!(
            Etapa::nome_conhecido(nome),
            "a máquina perdeu a etapa {nome}, que não é do degrau 4"
        );
    }
    assert!(!Etapa::nome_conhecido("Avisando"));
    assert!(!Etapa::nome_conhecido("CaminhoAberto"));
}
```

E acrescente o auxiliar que ele exige, junto de `transicao_legal`:

```rust
/// Se este nome é de uma etapa que esta máquina tem.
///
/// Existe para a guarda do degrau 4: uma etapa removida tem de ficar removida,
/// e sem isto a única prova seria a ausência de uma linha num `match`.
#[must_use]
pub fn nome_conhecido(nome: &str) -> bool {
    matches!(nome, "Parada" | "Tentando" | "Dentro" | "Desistiu")
}
```

- [ ] **Step 2: Rodar o teste e ver falhar**

Run: `cargo test -p seele-core uma_chegada_nunca_avisa_ponto_de_encontro`
Expected: FAIL — o método `nome_conhecido` ainda não existe, ou o `assert!(!...)` de `Avisando` falha porque a variante existe.

- [ ] **Step 3: Apagar o lado do cliente**

```bash
git rm crates/seele-core/src/encontro.rs
```

Em `crates/seele-core/src/lib.rs`, remova `pub(crate) mod encontro;` (ou `mod encontro;`).

Em `crates/seele-core/src/enlace.rs`, remova:

- as constantes `ESPERA_DO_FURO`, `AVISOS_POR_CANDIDATO`, `INTERVALO_DO_AVISO`, com os doc-comments inteiros;
- a função `avisar_pelo_candidato` (`enlace.rs:2545`);
- a função `e_publico` **se** ela não tiver outro chamador — confira com `grep -n "e_publico" crates/seele-core/src/enlace.rs` antes de apagar;
- o parâmetro `batida` de `Enlace::tentar_entre` e o `let batida = ...` em `Enlace::entre` (`enlace.rs:657-667`);
- o `emprestar` e todas as chamadas `emprestar_socket` — `Self::conectar_por` passa a receber `None` no primeiro argumento;
- as chamadas `repeticao.abort()` e as variáveis `repeticao`.

Em `crates/seele-core/src/chegada.rs`, remova as variantes `Etapa::Avisando` e `Etapa::CaminhoAberto`, o campo `com_bilhete_e_impressao` de `Etapa::Parada`, o método `ponto_a_avisar`, e as linhas correspondentes de `transicao_legal`. O `match` de `transicao_legal` fica:

```rust
pub fn transicao_legal(atual: &Self, para: &str) -> bool {
    match (atual, para) {
        // Sem candidato nenhum não há o que tentar. É a única desistência que
        // não passa por uma tentativa, e ela é honesta: um convite sem endereço
        // não tem aonde chegar.
        (Self::Parada { candidatos }, "Tentando") => *candidatos > 0,
        (Self::Parada { candidatos: 0 }, "Desistiu") => true,
        // Só do último candidato. Uma desistência com endereços ainda por
        // tentar é a forma que um passo perdido toma quando chega aqui.
        (Self::Tentando { candidato, de, .. }, "Desistiu") => {
            u16::from(*candidato) + 1 == u16::from(*de)
        }
        (Self::Tentando { .. }, "Tentando" | "Dentro") => true,
        _ => false,
    }
}
```

- [ ] **Step 4: Acompanhar na fronteira e na tela**

Em `crates/seele-ffi/src/lib.rs`, remova de `ConnectStage` as variantes `Avisando` e `CaminhoAberto` e o campo `com_bilhete_e_impressao` de `Parada`, mais os braços de conversão que as produziam.

Em `apps/seele-app/ui/frases.js`, o `ETAPAS` fica:

```js
const ETAPAS = {
  Parada: "LENDO O CONVITE",
  Tentando: "TENTANDO UM ENDEREÇO DO CONVITE",
  Dentro: "DENTRO",
  // Neutra de propósito. `Desistiu` carrega o `ConnectError` inteiro — o núcleo
  // o guardou assim para não achatar `PinChanged` e `InviteMismatch`, os dois
  // erros que **não são de rede** (ADR 0003) — e afirmar aqui que nenhum
  // endereço atendeu apagaria esse alarme na tela justamente quando ele é a
  // coisa mais importante escrita nela.
  Desistiu: "A CHEGADA PAROU AQUI",
};
```

- [ ] **Step 5: Rodar o portão**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

Expected: PASS. `crates/seele-conformance/tests/furo.rs` **vai falhar a compilação** — ele testa o degrau 4. Apague-o nesta task:

```bash
git rm crates/seele-conformance/tests/furo.rs
```

E rode o portão de novo.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(enlace): o cliente para de furar NAT

Degrau 4 do ADR 0022, lado de quem entra. Saem `Batida`, o aviso por
candidato, os três prazos que existiam para o furo, e as duas etapas de
chegada que só o degrau 4 produzia.

`nome_conhecido` é a guarda: uma etapa removida fica removida.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: O servidor para de furar NAT

**Files:**
- Delete: `crates/seele-server/src/alcance/encontro.rs`
- Modify: `crates/seele-server/src/alcance.rs` (`Escada::subir`, `Alcance::decidir`, `Degrau`, `Tipo`)
- Modify: `crates/seele-server/src/hospedagem.rs:200-215, 260-270, 430-465`
- Modify: `crates/seele-server/src/lib.rs` (o `pub use` do vocabulário de encontro)
- Test: `crates/seele-server/src/alcance.rs` (módulo `tests`)

**Interfaces:**
- Consumes: nada da Task 1 — os dois lados são independentes.
- Produces: `Degrau` sem a variante `FuroDeNat`; `Tipo` sem `Refletido`; `Escada::subir(escuta: Escuta)` com **um** parâmetro (a `Convocacao` sai); `Escada::bilhete()` deixa de existir.

- [ ] **Step 1: Escrever o teste que falha**

No módulo `tests` de `crates/seele-server/src/alcance.rs`:

```rust
#[test]
fn nenhum_degrau_depende_de_ponto_de_encontro() {
    // A escada perdeu o único degrau que punha um terceiro no caminho. Isto é
    // uma guarda de forma e não de comportamento: enquanto `Degrau` puder
    // nomear `FuroDeNat`, alguém volta a produzi-lo.
    for degrau in Degrau::todos() {
        assert!(
            !degrau.nome().contains("Furo"),
            "{} sobreviveu à remoção do degrau 4",
            degrau.nome()
        );
    }
}
```

E o auxiliar que ele exige, no `impl Degrau`:

```rust
/// Todos os degraus que esta escada pode declarar.
///
/// Existe para a guarda do degrau 4 e para o teste de exaustividade das
/// frases: uma variante nova sem frase é uma tela muda, e uma variante velha
/// que devia ter saído é um caminho que volta.
#[must_use]
pub fn todos() -> &'static [Degrau] {
    &[
        Degrau::PortaNoRoteador,
        Degrau::Ipv6Direto,
        Degrau::EnderecoDireto,
        Degrau::RedeLocalOuVpn,
        Degrau::SoRedeLocal,
    ]
}
```

- [ ] **Step 2: Rodar o teste e ver falhar**

Run: `cargo test -p seele-server nenhum_degrau_depende_de_ponto_de_encontro`
Expected: FAIL — ou o método não existe, ou você incluiu `FuroDeNat` em `todos()` e o assert dispara.

- [ ] **Step 3: Apagar o degrau 4 do servidor**

```bash
git rm crates/seele-server/src/alcance/encontro.rs
```

Em `crates/seele-server/src/alcance.rs`:

- remova `pub mod encontro;`;
- remova a variante `Degrau::FuroDeNat` e a variante `Tipo::Refletido`, com os doc-comments;
- em `Escada::subir`, remova o parâmetro `convocacao`, o bloco `let precisa = !tem_ipv4_global(...)` inteiro e o `match (precisa, convocacao)`, e o argumento `encontro.as_ref().map(...)` da chamada a `Alcance::decidir`;
- remova o campo `encontro` da struct `Escada` e `.com_recusa_do_encontro(...)`;
- remova `Alcance::encontro_recusado` e o campo que ele lê;
- em `Alcance::decidir`, remova o parâmetro `encontrado` e o bloco que empurra `Tipo::Refletido` em `candidatos`;
- remova `tem_ipv4_global` **se** não sobrar chamador.

Em `crates/seele-server/src/lib.rs`, remova o `pub use seele_proto::encontro::{...}` inteiro, com o doc-comment que o explica.

Em `crates/seele-server/src/hospedagem.rs`:

- remova `Escada::bilhete` e o `match` que punha `com_bilhete(bilhete)` em `convite()` — o corpo passa a ser `convite.to_string()` direto;
- remova o argumento de convocação da chamada a `Escada::subir`;
- apague o teste que afirma `folga <= crate::alcance::encontro::PRAZO + ...` (`hospedagem.rs:462`) e o que fala em "o ponto de encontro fora do ar mudou o degrau da escada" (`hospedagem.rs:434`).

- [ ] **Step 4: Rodar o portão**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo xtask check-deps
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(alcance): o servidor para de furar NAT

Degrau 4 do ADR 0022, lado de quem hospeda. Saem `alcance::encontro`
inteiro, `Degrau::FuroDeNat`, `Tipo::Refletido` e o bilhete que o convite
carregava.

`Degrau::todos()` é a guarda, e serve também ao teste de frases.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `enc=` vira parâmetro ignorado

**Files:**
- Modify: `crates/seele-proto/src/uri.rs` (struct `Convite`, `Display`, `analisar`, `Bilhete`, `ErroDeUri`)
- Modify: `crates/seele-core/src/enlace.rs` (assinaturas que carregavam `Option<Bilhete>`)
- Modify: `crates/seele-core/src/chegada.rs` (campo `bilhete`)
- Modify: `crates/seele-ffi/src/lib.rs` (onde o `Bilhete` atravessa)
- Test: `crates/seele-proto/src/uri.rs` (módulo `tests`)

**Interfaces:**
- Consumes: da Task 1, `Enlace::tentar_entre` já não lê `bilhete`; da Task 2, `hospedagem::convite()` já não o escreve.
- Produces: `Convite` sem o campo `bilhete`; `Enlace::conectar_entre(destinos, chave, pins)` como único ponto de entrada com lista (o de bilhete some); `ErroDeUri` sem `BilheteInvalido`.

- [ ] **Step 1: Escrever o teste que falha**

No módulo `tests` de `crates/seele-proto/src/uri.rs`:

```rust
#[test]
fn um_link_antigo_com_enc_entra_e_o_enc_nao_volta() {
    // Compatibilidade nos dois sentidos, e é a regra desta casa: recusar link
    // novo é o que faz cliente velho virar parede, e recusar link velho faz o
    // mesmo com quem ainda tem um colado numa conversa. O `enc=` é lido, é
    // jogado fora, e não é reescrito.
    let texto = format!("seele://server.exemplo:8383?enc=ponto.exemplo:8384/203.0.113.7:9000&fp={FP}");
    let convite = analisar(&texto).expect("um link antigo continua sendo um link");

    assert_eq!(convite.alvo, "server.exemplo:8383");
    assert_eq!(convite.impressao_digital.as_deref(), Some(FP));

    let de_volta = convite.to_string();
    assert!(!de_volta.contains("enc="), "o bilhete voltou: {de_volta}");
    assert!(de_volta.contains(&format!("fp={FP}")));
}
```

- [ ] **Step 2: Rodar o teste e ver falhar**

Run: `cargo test -p seele-proto um_link_antigo_com_enc_entra_e_o_enc_nao_volta`
Expected: FAIL com `o bilhete voltou: seele://server.exemplo:8383?enc=...` — o `Display` ainda escreve o campo.

- [ ] **Step 3: Apagar o `Bilhete`**

Em `crates/seele-proto/src/uri.rs`:

- remova o campo `pub bilhete: Option<Bilhete>` de `Convite`, o `com_bilhete`, e o bloco do `Display` que escreve `enc=`;
- em `analisar`, troque o braço `"enc" => convite.bilhete = Some(Bilhete::ler(valor)?)` por **nada** — ele cai no `_ => {}` que já ignora desconhecidos. Deixe um comentário no `_ => {}` dizendo que `enc=` chega por ali e é isso que o teste acima prova;
- remova a struct `Bilhete` inteira, o `impl Display for Bilhete` e a variante `ErroDeUri::BilheteInvalido`.

Em `crates/seele-core/src/enlace.rs`, remova `Enlace::conectar_entre_com_bilhete` e o parâmetro `bilhete` de `Enlace::entre`, `tentar_entre` e `conectar_por`. `conectar_entre` passa a chamar `Self::entre(destinos, chave, pins, None)`.

Em `crates/seele-core/src/chegada.rs`, remova o campo `bilhete` da struct e do construtor.

Em `crates/seele-ffi/src/lib.rs`, remova onde o bilhete era lido do convite e passado adiante.

- [ ] **Step 4: Rodar o teste e ver passar**

Run: `cargo test -p seele-proto um_link_antigo_com_enc_entra_e_o_enc_nao_volta`
Expected: PASS.

- [ ] **Step 5: Rodar o portão**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

Expected: PASS. `crates/seele-conformance/tests/convite.rs` pode ter casos que constroem um bilhete — apague **só** esses casos, não o arquivo.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(uri): o convite perde o bilhete de encontro

`enc=` sai da struct, do Display e do parser. Um link antigo que o traga
continua entrando: o campo cai no ramo de parâmetro desconhecido, que já
existia para exatamente isto, e o teste prova que ele não é reescrito.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Os restos do ponto de encontro saem

**Files:**
- Delete: `crates/seele-encontro/` (crate inteira)
- Delete: `crates/seele-proto/src/encontro.rs`
- Delete: `crates/seele-tui/src/rede.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/seele-proto/src/lib.rs`
- Modify: `crates/seele-tui/src/lib.rs`, `crates/seele-tui/src/main.rs`
- Modify: `xtask/src/check_deps.rs:57, 290-292`

**Interfaces:**
- Consumes: das Tasks 1–3, ninguém mais fala o vocabulário de encontro.
- Produces: nada. Esta task só apaga.

- [ ] **Step 1: Confirmar que ninguém mais usa**

```bash
grep -rn "encontro" --include="*.rs" crates apps xtask | grep -v "^crates/seele-encontro" | grep -v "^crates/seele-proto/src/encontro.rs"
```

Expected: nenhuma linha, ou só menções em comentário histórico. **Se aparecer uma chamada real, pare** — significa que uma das tasks 1–3 ficou incompleta, e continuar aqui esconderia isso.

- [ ] **Step 2: Apagar**

```bash
git rm -r crates/seele-encontro
git rm crates/seele-proto/src/encontro.rs
git rm crates/seele-tui/src/rede.rs
```

Em `Cargo.toml`, remova `"crates/seele-encontro",` da lista `members`.
Em `crates/seele-proto/src/lib.rs`, remova `pub mod encontro;`.
Em `crates/seele-tui/src/lib.rs`, remova `pub mod rede;`.
Em `crates/seele-tui/src/main.rs`, remova `use seele_tui::rede;` e o subcomando `--rede` que o chamava, incluindo a linha de ajuda dele.
Em `xtask/src/check_deps.rs`, remova a linha 57 (`("seele-encontro", &["seele-proto"]),`) e os três `assert` das linhas 290–292.

- [ ] **Step 3: Rodar o portão**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace && cargo xtask check-deps
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor: o ponto de encontro sai do repositório

A crate `seele-encontro`, o vocabulário de fio em `seele-proto`, e o
diagnóstico `connection --rede` que era construído sobre eles. Nada mais
os chamava depois das três tasks anteriores.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

# Fase 2 · O degrau 3 sai

### Task 5: UPnP e PCP saem

**Files:**
- Delete: `crates/seele-server/src/alcance/porta.rs`
- Delete: `crates/seele-server/src/alcance/pcp.rs`
- Modify: `crates/seele-server/src/alcance.rs`
- Modify: `crates/seele-server/Cargo.toml:16-42`
- Test: `crates/seele-server/src/alcance.rs` (módulo `tests`)

**Interfaces:**
- Consumes: da Task 2, `Degrau::todos()` existe.
- Produces: `Degrau` sem `PortaNoRoteador`; `Tipo` sem `PortaNoRoteador` e sem `GlobalLiberado`; `Escada::subir(escuta)` síncrona — deixa de ser `async`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn a_escada_nao_pede_nada_a_roteador_nenhum() {
    // Degrau 3 fora. O que sobra são endereços que esta máquina já tem, e por
    // isso `Escada::subir` deixou de ser assíncrona: não há mais ninguém para
    // esperar. Esta guarda é sobre a lista de degraus; a assinatura síncrona é
    // provada pelo compilador, no Step 3.
    for degrau in Degrau::todos() {
        assert!(
            !degrau.nome().contains("Porta"),
            "{} sobreviveu à remoção do degrau 3",
            degrau.nome()
        );
    }
    assert_eq!(Degrau::todos().len(), 4);
}
```

- [ ] **Step 2: Rodar o teste e ver falhar**

Run: `cargo test -p seele-server a_escada_nao_pede_nada_a_roteador_nenhum`
Expected: FAIL — `PortaNoRoteador` ainda está em `todos()`, e o comprimento é 5.

- [ ] **Step 3: Apagar o degrau 3**

```bash
git rm crates/seele-server/src/alcance/porta.rs crates/seele-server/src/alcance/pcp.rs
```

Em `crates/seele-server/src/alcance.rs`:

- remova `pub mod porta;` e `pub mod pcp;`;
- remova `Degrau::PortaNoRoteador`, `Tipo::PortaNoRoteador` e `Tipo::GlobalLiberado`, com os doc-comments;
- remova de `Degrau::todos()` a entrada `PortaNoRoteador`;
- remova de `Escada` os campos `porta` e `firewall`;
- em `Escada::subir`, remova o `tokio::join!` inteiro e as funções `abrir_porta` e `abrir_firewall`. **Tire o `async` da assinatura** — `pub fn subir(escuta: Escuta) -> Self`;
- em `Escada::descer`, remova o que devolvia a porta ao roteador. Se o corpo ficar vazio, remova o método e o chamador em `hospedagem.rs:266-268`;
- em `Alcance::decidir`, remova os parâmetros `mapeada`, `liberada`, `recusa` e `recusa_do_pcp`, e os blocos que os usavam;
- remova `Alcance::porta_recusada` e `Alcance::pcp_recusada`, e o que os expunha na FFI e na tela.

Em `crates/seele-server/Cargo.toml`, remova as três dependências com os comentários inteiros: `crab_nat`, `igd-next`, `netdev`.

Em `crates/seele-server/src/hospedagem.rs`, tire o `.await` da chamada a `Escada::subir`.

- [ ] **Step 4: Rodar o portão**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

Expected: PASS.

- [ ] **Step 5: Conferir que a árvore encolheu**

```bash
cargo tree -p seele-server | grep -E "crab_nat|igd-next|netdev"
```

Expected: nenhuma linha. Se aparecer, alguma delas entrou por transitividade e o comentário do `Cargo.toml` que a justificava mentia — registre isso no commit.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(alcance): sai o degrau 3, e a escada deixa de ser assíncrona

UPnP e PCP fora. Sem roteador para consultar e sem ponto de encontro para
esperar, `Escada::subir` não tem mais o que aguardar: vira síncrona.

Saem três dependências: crab_nat, igd-next e netdev.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

# Fase 3 · IPv4 puro

### Task 6: A escuta liga em `0.0.0.0`

**Files:**
- Modify: `crates/seele-server/src/alcance.rs:36-200` (`Pilha`, `Escuta`, `abrir_escuta`)
- Modify: `crates/seele-server/src/lib.rs:318` e `Daemon::pilha`
- Modify: `crates/seele-server/src/main.rs:46-52`
- Modify: `crates/seele-server/src/hospedagem.rs:78-100`
- Test: `crates/seele-server/src/alcance.rs` (módulo `tests`)

**Interfaces:**
- Consumes: da Task 5, `Escada::subir(escuta)` síncrona.
- Produces: `Escuta::nova(porta: u16)` com **um** parâmetro; `Pilha` deixa de existir; `abrir_escuta(escuta: SocketAddr) -> Result<UdpSocket>` devolve só o socket.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn a_escuta_padrao_e_ipv4() {
    // Decisão 2 do spec de 26/08: IPv4 puro, em todo lugar. Um servidor que
    // ligasse em `[::]` continuaria aceitando IPv6 e continuaria anunciando
    // alcance que o cliente não usa mais.
    let socket = abrir_escuta(SocketAddr::from(([0, 0, 0, 0], 0))).expect("abrir");
    let local = socket.local_addr().expect("local");
    assert!(local.is_ipv4(), "a escuta subiu em {local}");
}

#[test]
fn uma_escuta_ipv6_nomeada_e_recusada() {
    // Não é ignorada em silêncio: um operador que escreveu `[::]:8383` na linha
    // de comando precisa saber que este SEELE não atende ali, e não descobrir
    // pelo silêncio de ninguém conectando.
    let erro = abrir_escuta(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0)));
    assert!(erro.is_err(), "uma escuta IPv6 passou");
}
```

- [ ] **Step 2: Rodar os testes e ver falhar**

Run: `cargo test -p seele-server a_escuta_padrao_e_ipv4 uma_escuta_ipv6_nomeada_e_recusada`
Expected: o segundo FALHA — hoje `abrir_escuta` aceita IPv6 e devolve `Pilha::SoIpv6`.

- [ ] **Step 3: Apagar a pilha dupla**

Em `crates/seele-server/src/alcance.rs`:

- remova o enum `Pilha` inteiro, com `alcanca_ipv4` e `alcanca_ipv6`;
- em `Escuta`, remova o campo `pilha` e o método `pilha()`. `Escuta::nova(porta: u16)`;
- `Escuta::serve` passa a ser `endereco.is_ipv4()`. `anunciar` e `anunciar_com_porta` continuam existindo e continuam privados — eles são a guarda do módulo;
- `abrir_escuta` fica:

```rust
/// Abre o socket em que o servidor atende. **IPv4, e só.**
///
/// O ADR 0036 tirou o IPv6 do caminho de conexão inteiro, e esta é a linha em
/// que isso vira verdade em vez de intenção: não há como um endereço IPv6
/// chegar a um convite se não há socket IPv6 para servi-lo.
///
/// Um endereço IPv6 nomeado é **recusado**, e não recuado em silêncio para
/// IPv4. Recuar aqui daria a um operador um servidor no ar num endereço que ele
/// não pediu, e a descoberta viria pelo silêncio de ninguém conectar.
///
/// # Errors
///
/// Se o endereço for IPv6, ou se o `bind` falhar.
pub fn abrir_escuta(escuta: SocketAddr) -> Result<UdpSocket> {
    if escuta.is_ipv6() {
        anyhow::bail!(
            "este SEELE atende só em IPv4, e `{escuta}` é IPv6. \
             Use `0.0.0.0:{}` ou um endereço IPv4 desta máquina.",
            escuta.port()
        );
    }
    ligar(escuta)
}
```

- remova `pilha_dupla` e o `use socket2::...` se ele não tiver outro uso nesta crate — confira antes.

Em `crates/seele-server/src/lib.rs`, `main.rs` e `hospedagem.rs`, troque toda escuta padrão de `[::]` para `0.0.0.0` e remova as chamadas a `Daemon::pilha`. Em `main.rs:46-52` o `unwrap_or_else` fica `format!("0.0.0.0:{}", seele_proto::transport::DEFAULT_PORT)`, e o comentário sobre o degrau 2 sai com ele.

- [ ] **Step 4: Rodar os testes e ver passar**

Run: `cargo test -p seele-server a_escuta_padrao_e_ipv4 uma_escuta_ipv6_nomeada_e_recusada`
Expected: PASS.

- [ ] **Step 5: Rodar o portão**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(alcance): a escuta é IPv4, e um IPv6 nomeado é recusado

Sai `Pilha` e sai a pilha dupla. O padrão passa de `[::]` para `0.0.0.0`.

Recusar em vez de recuar é a escolha: um recuo silencioso põe o servidor
num endereço que o operador não pediu, e ele descobre pelo silêncio.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: O degrau 2 sai da escada

**Files:**
- Modify: `crates/seele-server/src/alcance.rs` (`Degrau::Ipv6Direto`, filtro de candidatos)
- Modify: `apps/seele-app/ui/frases.js` (a frase do degrau 2)
- Test: `crates/seele-server/src/alcance.rs`

**Interfaces:**
- Consumes: da Task 6, `Escuta::serve` já recusa IPv6.
- Produces: `Degrau` com exatamente três variantes: `EnderecoDireto`, `RedeLocalOuVpn`, `SoRedeLocal`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn a_escada_tem_tres_degraus_e_nenhum_e_ipv6() {
    // §3 do spec: o que a pessoa faz a respeito é diferente em cada um, e é
    // esse o critério que define quantos são. Três, e nenhum menciona IPv6.
    assert_eq!(Degrau::todos().len(), 3);
    for degrau in Degrau::todos() {
        assert!(!degrau.nome().contains("Ipv6"), "{}", degrau.nome());
    }
}

#[test]
fn um_ipv6_da_maquina_nunca_vira_candidato() {
    // A guarda que impede o degrau 2 de voltar por acidente: mesmo que uma
    // placa desta máquina tenha IPv6 global, ele não entra no convite, porque
    // não há socket IPv6 para atendê-lo.
    let escuta = Escuta::nova(8383);
    let seis: IpAddr = "2001:db8::1".parse().expect("endereço");
    assert!(!escuta.serve(seis));
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server a_escada_tem_tres_degraus_e_nenhum_e_ipv6`
Expected: FAIL — `todos()` ainda tem 4 entradas, incluindo `Ipv6Direto`.

- [ ] **Step 3: Remover**

Em `alcance.rs`, remova `Degrau::Ipv6Direto` e sua entrada em `todos()`, mais o braço que o produzia em `Alcance::decidir`.

Em `apps/seele-app/ui/frases.js`, remova a frase de `Ipv6Direto`. **Confira se existe um teste de exaustividade de frases** (`grep -n "Ipv6Direto" apps/seele-app/ui/*.js`) e ajuste-o.

- [ ] **Step 4: Rodar o portão**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(alcance): o degrau 2 sai, e a escada fica com três

EnderecoDireto, RedeLocalOuVpn e SoRedeLocal — o critério é o que a pessoa
faz a respeito, e são três respostas diferentes.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: O convite recusa endereço IPv6

**Files:**
- Modify: `crates/seele-proto/src/uri.rs` (`validar_alvo`, `ErroDeUri`)
- Modify: `apps/seele-app/ui/frases.js` (frase do erro novo)
- Test: `crates/seele-proto/src/uri.rs`

**Interfaces:**
- Consumes: da Task 3, `ErroDeUri` sem `BilheteInvalido`.
- Produces: `ErroDeUri::EnderecoIpv6`, que a casca tem de saber escrever.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn um_alvo_ipv6_e_recusado_em_voz_alta() {
    // A única exceção deste ciclo à regra de ignorar em silêncio, e ela é
    // deliberada: não há como aceitar calado um endereço em que ninguém mais
    // atende. Ver §8 do spec de 26/08.
    let erro = analisar(&format!("seele://[2001:db8::1]:8383?fp={FP}"))
        .expect_err("um alvo IPv6 tem de ser recusado");
    assert_eq!(erro, ErroDeUri::EnderecoIpv6);
}

#[test]
fn um_alvo_ipv4_e_um_nome_continuam_entrando() {
    // A recusa acima não pode ter pegado junto o caso comum.
    assert!(analisar(&format!("seele://192.168.1.5:8383?fp={FP}")).is_ok());
    assert!(analisar(&format!("seele://seele.exemplo.com.br:8383?fp={FP}")).is_ok());
    assert!(analisar(&format!("seele://100.101.102.103:8383?fp={FP}")).is_ok());
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto um_alvo_ipv6_e_recusado_em_voz_alta`
Expected: FAIL — `ErroDeUri::EnderecoIpv6` não existe.

- [ ] **Step 3: Implementar**

Acrescente a `ErroDeUri`:

```rust
/// O endereço é IPv6, e este SEELE atende só em IPv4.
///
/// Recusado, e não ignorado como um parâmetro desconhecido: um `alt=` perdido
/// custa um caminho a tentar, e um `alvo` que ninguém atende custa a conexão
/// inteira, calada. Ver o ADR 0036.
EnderecoIpv6,
```

Em `validar_alvo`, depois do `separar(alvo)`:

```rust
    let alvo_separado = separar(alvo)?;
    // Só quando é literalmente um IPv6. Um **nome** que resolva para IPv6 não é
    // recusado aqui, e não pode ser: quem resolve é `lookup_host`, do outro
    // lado da fronteira, e recusar um nome sem consultá-lo seria adivinhar.
    if alvo_separado
        .maquina
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_ipv6())
    {
        return Err(ErroDeUri::EnderecoIpv6);
    }
    Ok(())
```

Em `frases.js`, acrescente a frase para `EnderecoIpv6` — no estilo curto das outras: `"ESTE LINK É IPv6, E O SEELE ATENDE EM IPv4"`.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-proto um_alvo_ipv6_e_recusado_em_voz_alta um_alvo_ipv4_e_um_nome_continuam_entrando`
Expected: PASS.

- [ ] **Step 5: Rodar o portão e commitar**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
feat(uri): um alvo IPv6 é recusado com frase própria

A única exceção deste ciclo à regra de ignorar em silêncio. Um `alt=`
perdido custa um caminho; um `alvo` que ninguém atende custa a conexão
inteira, calada.

Um nome que resolva para IPv6 não é recusado aqui: quem resolve é o
`lookup_host`, e recusar sem consultar seria adivinhar.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

# Fase 4 · Um endereço, e o que sobra da escada

### Task 9: `alt=` sai do convite

**Files:**
- Modify: `crates/seele-proto/src/uri.rs` (`alternativos`, `LIMITE_DE_ALVOS`, `com_alternativos`, `candidatos`)
- Modify: `crates/seele-server/src/hospedagem.rs` (`convite()`)
- Test: `crates/seele-proto/src/uri.rs`

**Interfaces:**
- Consumes: da Task 8, `validar_alvo` recusa IPv6.
- Produces: `Convite` sem `alternativos`; `Convite::candidatos()` deixa de existir; `Hospedagem::convite()` usa o **primeiro** alvo de `Enderecos`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn um_link_antigo_com_alt_entra_e_o_alt_nao_volta() {
    // Mesma regra do `enc=`, e pelo mesmo motivo. O primeiro endereço é o que
    // vale — é o que um cliente anterior ao `alt=` já lia.
    let texto = format!("seele://192.168.1.5:8383?alt=203.0.113.7:8383,10.0.0.2:8383&fp={FP}");
    let convite = analisar(&texto).expect("um link antigo continua sendo um link");

    assert_eq!(convite.alvo, "192.168.1.5:8383");

    let de_volta = convite.to_string();
    assert!(!de_volta.contains("alt="), "os alternativos voltaram: {de_volta}");
}

#[test]
fn um_convite_novo_tem_um_endereco_e_uma_impressao() {
    // A forma final: alvo + fp=, e nada de endereçamento além disso.
    let convite = Convite::novo("seele.exemplo.com.br:8383").com_impressao_digital(FP);
    assert_eq!(
        convite.to_string(),
        format!("seele://seele.exemplo.com.br:8383?fp={FP}")
    );
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto um_link_antigo_com_alt_entra_e_o_alt_nao_volta`
Expected: FAIL — o `Display` ainda escreve `alt=`.

- [ ] **Step 3: Implementar**

Em `uri.rs`:

- remova o campo `alternativos`, o método `com_alternativos`, o método `candidatos()` e a constante `LIMITE_DE_ALVOS`;
- remova o bloco do `Display` que escreve `alt=`;
- em `analisar`, remova o braço `"alt" => {...}`. Ele cai no `_ => {}`. Junte o comentário do `_ => {}` para dizer que `alt=` e `enc=` chegam por ali, e que os dois testes acima provam que não voltam.

Em `hospedagem.rs::convite()`, o corpo fica:

```rust
    let alvo = self.alcance().map_or_else(
        || self.endereco_na_rede().unwrap_or(self.endereco).to_string(),
        // O primeiro, e só ele. A ordem é a de `Enderecos`, e quem hospeda
        // escolhe outro pela tela — ver §4 do spec de 26/08.
        |alcance| alcance.alvo().to_string(),
    );
    seele_proto::uri::Convite::novo(alvo)
        .com_impressao_digital(self.impressao_digital())
        .to_string()
```

- [ ] **Step 4: Rodar e ver passar; rodar o portão**

```bash
cargo test -p seele-proto um_link_antigo_com_alt_entra_e_o_alt_nao_volta um_convite_novo_tem_um_endereco_e_uma_impressao
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

`crates/seele-conformance/tests/convite.rs` vai ter casos de múltiplos alvos — apague **só** esses casos.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(uri): o convite leva um endereço

`alt=` sai. Ele existia para quem hospeda em casa com várias interfaces, e
esse caso passa a ser resolvido na tela de hospedar em vez de no link.

Link antigo continua entrando pelo primeiro endereço, que é o que um
cliente anterior ao `alt=` já lia.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: A corrida de candidatos sai do `enlace`

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (`tentar_entre`, constantes, `e_de_outra_casa`)
- Modify: `crates/seele-core/src/chegada.rs`
- Delete: `crates/seele-conformance/tests/candidatos.rs`
- Test: `crates/seele-core/src/enlace.rs`

**Interfaces:**
- Consumes: da Task 9, o convite tem um endereço.
- Produces: `Enlace::conectar(destino, chave, pins)` como **único** ponto de entrada. `conectar_entre` e `conectar_entre_observado` deixam de existir.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn conectar_nao_tem_prazo_proprio_alem_do_aperto_de_mao() {
    // Com um endereço não há fila, e sem fila não há prazo para segurá-la. O
    // único prazo visível volta a ser o `HANDSHAKE_TIMEOUT` do protocolo, e a
    // conta desta guarda é essa: nenhuma constante de prazo sobreviveu ao corte
    // da corrida.
    //
    // Escrita como `const` para falhar na compilação, e não em tempo de teste:
    // uma constante que volte a existir tem de acender uma luz vermelha antes
    // de alguém rodar nada.
    const { assert!(seele_proto::transport::HANDSHAKE_TIMEOUT.as_secs() == 10) };
}
```

E, no mesmo módulo, um teste de fumaça que prova que a entrada única existe:

```rust
#[tokio::test]
async fn conectar_a_um_endereco_morto_devolve_o_erro_e_nao_trava() {
    // `203.0.113.0/24` é TEST-NET-3 (RFC 5737): reservado, não roteado, e
    // portanto um endereço que nunca responde sem depender da rede de quem
    // roda o teste.
    let destino = Destino {
        servidor: "203.0.113.7:8383".parse().expect("endereço"),
        nome_tls: "localhost".into(),
        chave_do_pin: "203.0.113.7:8383".into(),
        apelido: "ninguem".into(),
        segredo: None,
        impressao_esperada: None,
    };
    let pins = Arc::new(crate::tofu::PinsEmMemoria::default());
    let erro = tokio::time::timeout(
        Duration::from_secs(30),
        Enlace::conectar(destino, SigningKey::generate(&mut rand::rngs::OsRng), pins),
    )
    .await
    .expect("conectar não pode passar de 30 s com um endereço só")
    .expect_err("um endereço morto não conecta");
    assert!(matches!(
        erro,
        ConnectError::HandshakeTimeout | ConnectError::Unreachable
    ));
}
```

> **Nota para quem implementa:** confira o nome real do `PinStore` em memória com `grep -n "impl PinStore" crates/seele-core/src/tofu.rs` e ajuste. Se não houver um, use o mesmo auxiliar que os testes existentes de `enlace.rs` usam — `grep -n "PinStore" crates/seele-core/src/enlace.rs | tail -20`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core conectar_a_um_endereco_morto_devolve_o_erro_e_nao_trava`
Expected: FAIL na compilação (a função de teste referencia coisas que ainda convivem com a corrida) ou por timeout de 30 s, porque hoje quatro candidatos × duas voltas passam disso.

- [ ] **Step 3: Apagar a corrida**

Em `crates/seele-core/src/enlace.rs`, remova:

- `Enlace::conectar_entre`, `conectar_entre_observado`, `Enlace::entre` e `Enlace::tentar_entre` inteiras;
- as constantes `PRAZO_POR_CANDIDATO`, `PRAZO_DA_PRIMEIRA_VOLTA`, `PRAZO_DE_CANDIDATO_DISTANTE`, com os doc-comments;
- `e_de_outra_casa`, `e_privado` e `mesma_rede` **se** não sobrar chamador — confira cada uma com `grep -n`;
- a struct `Tentativa` e a função `contar`, junto do canal de observação.

`Enlace::conectar_por` continua existindo como o corpo do aperto de mão; `Enlace::conectar` passa a chamá-la direto com `None` no socket emprestado.

Em `crates/seele-core/src/chegada.rs`, `Chegada::chegar` deixa de escutar o canal de tentativas e passa a chamar `Enlace::conectar` com o único destino. `Etapa::Tentando` mantém os campos `candidato` e `de` por enquanto — a Task 11 os troca.

```bash
git rm crates/seele-conformance/tests/candidatos.rs
```

- [ ] **Step 4: Rodar e ver passar; rodar o portão**

```bash
cargo test -p seele-core conectar_a_um_endereco_morto_devolve_o_erro_e_nao_trava
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(enlace): sai a corrida de candidatos

Com um endereço não há fila, e sem fila não há prazo para segurá-la. Saem
as duas voltas, os três prazos por candidato e a sonda de outra-casa.

O único prazo visível volta a ser o HANDSHAKE_TIMEOUT do protocolo: "não
conectou" passa a ter uma causa, e o log diz qual.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: A trilha conta o que agora acontece

**Files:**
- Modify: `crates/seele-core/src/chegada.rs` (`Etapa`, `transicao_legal`, `nome_conhecido`)
- Modify: `crates/seele-ffi/src/lib.rs` (`ConnectStage`)
- Modify: `apps/seele-app/ui/frases.js`, `apps/seele-app/ui/tela-boot.js:204-215`
- Test: `crates/seele-core/src/chegada.rs`

**Interfaces:**
- Consumes: da Task 10, `Chegada::chegar` tem um destino só.
- Produces: `Etapa` com `Parada`, `Resolvendo`, `Conectando`, `Conferindo`, `Dentro`, `Desistiu`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn a_trilha_conta_as_etapas_de_um_endereco() {
    // §5 do spec: as etapas deixam de contar candidatos e passam a contar o que
    // de fato acontece. `Resolvendo` é a que ganha razão de existir — ela é o
    // prazo do `lookup_host` da Task 12 tendo algo a dizer.
    for nome in ["Parada", "Resolvendo", "Conectando", "Conferindo", "Dentro", "Desistiu"] {
        assert!(Etapa::nome_conhecido(nome), "faltou {nome}");
    }
    assert!(!Etapa::nome_conhecido("Tentando"));

    assert!(Etapa::transicao_legal(&Etapa::Parada, "Resolvendo"));
    assert!(Etapa::transicao_legal(&Etapa::Resolvendo, "Conectando"));
    assert!(Etapa::transicao_legal(&Etapa::Conectando, "Conferindo"));
    assert!(Etapa::transicao_legal(&Etapa::Conferindo, "Dentro"));
    // Desistir é legal de qualquer etapa antes de estar dentro: um DNS que não
    // resolve e um aperto de mão que não fecha param em lugares diferentes, e
    // as duas paradas são reais.
    assert!(Etapa::transicao_legal(&Etapa::Resolvendo, "Desistiu"));
    assert!(!Etapa::transicao_legal(&Etapa::Dentro, "Desistiu"));
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core a_trilha_conta_as_etapas_de_um_endereco`
Expected: FAIL — as variantes novas não existem.

- [ ] **Step 3: Implementar**

Troque `Etapa` por:

```rust
/// Onde uma chegada está.
///
/// Deixou de contar candidatos quando o convite passou a levar um endereço só
/// (ADR 0037). O que ela conta agora é o que de fato acontece com aquele
/// endereço — e cada etapa é um lugar diferente onde a chegada pode parar, que
/// é a informação que quem espera quer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Etapa {
    /// O convite foi lido e nada foi tentado.
    Parada,
    /// Procurando o endereço do nome. Só existe quando o alvo é um nome.
    Resolvendo,
    /// O aperto de mão QUIC está correndo.
    Conectando {
        /// O endereço, já resolvido.
        onde: SocketAddr,
    },
    /// Conectado, conferindo a impressão digital contra a do convite (ADR 0003).
    Conferindo,
    /// Dentro: há sessão.
    Dentro,
    /// Parou aqui, e este é o motivo.
    Desistiu(ConnectError),
}
```

`transicao_legal`:

```rust
pub fn transicao_legal(atual: &Self, para: &str) -> bool {
    match (atual, para) {
        (Self::Parada, "Resolvendo" | "Conectando") => true,
        (Self::Resolvendo, "Conectando") => true,
        (Self::Conectando { .. }, "Conferindo") => true,
        (Self::Conferindo, "Dentro") => true,
        // De qualquer lugar antes de estar dentro. Um nome que não resolve e um
        // aperto de mão que não fecha param em lugares diferentes, e as duas
        // paradas são reais.
        (Self::Parada | Self::Resolvendo | Self::Conectando { .. } | Self::Conferindo, "Desistiu") => true,
        _ => false,
    }
}

#[must_use]
pub fn nome_conhecido(nome: &str) -> bool {
    matches!(
        nome,
        "Parada" | "Resolvendo" | "Conectando" | "Conferindo" | "Dentro" | "Desistiu"
    )
}
```

Espelhe em `ConnectStage` na FFI. Em `frases.js`:

```js
const ETAPAS = {
  Parada: "LENDO O CONVITE",
  Resolvendo: "PROCURANDO O SERVIDOR",
  Conectando: "CONECTANDO",
  Conferindo: "CONFERINDO A CHAVE DO SERVIDOR",
  Dentro: "DENTRO",
  Desistiu: "A CHEGADA PAROU AQUI",
};
```

Em `tela-boot.js`, remova o bloco que montava `O ENDEREÇO n DE m` (linhas 204-215) e troque por: quando a etapa for `Conectando` e trouxer `onde`, escreva `${base} · ${onde}`.

- [ ] **Step 4: Rodar e ver passar; rodar o portão; commitar**

```bash
cargo test -p seele-core a_trilha_conta_as_etapas_de_um_endereco
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
refactor(chegada): a trilha conta etapas, não candidatos

Parada, Resolvendo, Conectando, Conferindo, Dentro, Desistiu. Cada uma é um
lugar diferente onde a chegada pode parar, que é a informação que quem
espera quer — e `Resolvendo` é o que dá voz ao prazo de DNS que vem a
seguir.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: O nome é resolvido com prazo

**Files:**
- Modify: `crates/seele-ffi/src/lib.rs:2595-2626` (`resolve`)
- Modify: `crates/seele-ffi/src/types.rs` (`ConnectionError`, se precisar de variante)
- Test: `crates/seele-ffi/src/lib.rs`

**Interfaces:**
- Consumes: da Task 11, `Etapa::Resolvendo` existe.
- Produces: `async fn resolve(target: &str) -> Result<(SocketAddr, String, String), ConnectionError>`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[tokio::test]
async fn resolver_um_nome_que_nao_existe_falha_dentro_do_prazo() {
    // `invalid` é um TLD reservado (RFC 2606): nenhum resolvedor do mundo o
    // resolve, e por isso ele é o caso de teste que não depende da rede de
    // quem roda.
    //
    // O prazo é a razão desta task existir: `to_socket_addrs` é síncrono e sem
    // teto, e travava uma thread do tokio quando o alvo virou nome.
    let comeco = std::time::Instant::now();
    let erro = resolve("naoexiste.invalid:8383")
        .await
        .expect_err("um nome inválido não resolve");
    assert_eq!(erro, ConnectionError::UnresolvableHost);
    assert!(
        comeco.elapsed() < PRAZO_DE_RESOLUCAO + Duration::from_secs(1),
        "levou {:?}",
        comeco.elapsed()
    );
}

#[tokio::test]
async fn resolver_um_ipv4_literal_nao_consulta_dns() {
    let (endereco, nome_tls, pin) = resolve("192.168.1.5:8383").await.expect("resolver");
    assert_eq!(endereco.to_string(), "192.168.1.5:8383");
    assert_eq!(nome_tls, "localhost");
    assert_eq!(pin, "192.168.1.5:8383");
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-ffi resolver_um_nome_que_nao_existe_falha_dentro_do_prazo`
Expected: FAIL na compilação — `resolve` não é `async` e `PRAZO_DE_RESOLUCAO` não existe.

- [ ] **Step 3: Implementar**

```rust
/// Quanto tempo se espera pelo DNS antes de desistir do nome.
///
/// Cinco segundos, e o número tem dois lados. Para baixo: um resolvedor
/// doméstico responde em dezenas de milissegundos, e um que precise de cinco
/// segundos está com problema. Para cima: metade do `HANDSHAKE_TIMEOUT`, para
/// que a soma dos dois ainda caiba no que quem espera aguenta olhar.
///
/// Existir é o ponto. Antes disto a resolução era `to_socket_addrs`, síncrona e
/// **sem teto**, chamada de dentro de contexto async: com um IP nunca doeu, e
/// com nome — que é o que o ADR 0036 passou a recomendar — um DNS lento
/// travava uma thread do runtime sem limite.
const PRAZO_DE_RESOLUCAO: Duration = Duration::from_secs(5);

async fn resolve(target: &str) -> Result<(SocketAddr, String, String), ConnectionError> {
    let alvo = seele_core::uri::separar(target).map_err(|_| ConnectionError::UnresolvableHost)?;

    let address = tokio::time::timeout(
        PRAZO_DE_RESOLUCAO,
        tokio::net::lookup_host((alvo.maquina, alvo.porta)),
    )
    .await
    .map_err(|_| ConnectionError::UnresolvableHost)?
    .map_err(|_| ConnectionError::UnresolvableHost)?
    // O primeiro IPv4. Com o ADR 0036 não há socket IPv6 do outro lado, então
    // um AAAA que o DNS devolva é um endereço em que ninguém atende — e pegá-lo
    // por ser o primeiro era o defeito que o `.next()` de antes escondia.
    .find(SocketAddr::is_ipv4)
    .ok_or(ConnectionError::UnresolvableHost)?;

    let server_name = if alvo.maquina.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        alvo.maquina.to_owned()
    };
    let pin_key = format!("{}:{}", alvo.maquina, alvo.porta);
    Ok((address, server_name, pin_key))
}
```

Propague o `.await` em todos os chamadores de `resolve`.

> **Nota:** o `pin_key` perdeu o ramo de colchetes de IPv6, que não tem mais caso. Confira com `grep -n "pin_key" crates/seele-ffi/src/lib.rs` que não sobrou um segundo lugar montando a chave.

- [ ] **Step 4: Rodar e ver passar; rodar o portão; commitar**

```bash
cargo test -p seele-ffi resolver_um_nome_que_nao_existe_falha_dentro_do_prazo resolver_um_ipv4_literal_nao_consulta_dns
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
fix(ffi): o nome é resolvido com prazo, e só em IPv4

`to_socket_addrs` é síncrono e sem teto, e era chamado de dentro de
contexto async. Com IP nunca doeu; com nome — que o ADR 0036 passou a
recomendar — um DNS lento travava uma thread do runtime.

Vira `lookup_host` com cinco segundos, e escolhe o primeiro IPv4 em vez do
primeiro qualquer: sem socket IPv6 do outro lado, um AAAA é um endereço em
que ninguém atende.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 13: A escada vira `Enderecos`

**Files:**
- Modify: `crates/seele-server/src/alcance.rs` (colapso de `Escada`/`Alcance`/`Escuta`)
- Modify: `crates/seele-server/src/hospedagem.rs`
- Modify: `crates/seele-ffi/src/lib.rs` (o que expõe o degrau)
- Test: `crates/seele-server/src/alcance.rs`

**Interfaces:**
- Consumes: das Tasks 5–9, a escada já não tem degraus 2, 3 e 4.
- Produces: **duas** construtoras, e a separação é o que torna os testes possíveis sem rede:
  - `Enderecos::a_partir_de(achados: &[interfaces::Achado], porta: u16) -> Enderecos` — a decisão sobre valores, que é o que os testes desta task exercitam;
  - `Enderecos::desta_maquina(porta: u16) -> Enderecos` — chama `interfaces::descobrir()` e delega para a de cima.

  Mais `alvo() -> SocketAddr`, `todos() -> &[SocketAddr]` e `degrau() -> Degrau`. É o mesmo par que `Alcance::decidir`/`Escada::subir` já formava, e pelo mesmo motivo escrito lá: uma decisão sobre valores não precisa de socket para ser testada.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn um_endereco_de_ponte_nunca_e_anunciado() {
    // §3 do spec: WSL, Hyper-V, Docker e VirtualBox produzem endereços que
    // nunca respondem de outra máquina. Com o convite levando um endereço só,
    // um `Ponte` escolhido por acidente seria o link inteiro.
    let achados = vec![
        achado_de_teste("vEthernet (WSL)", "172.30.16.1", Origem::Virtual),
        achado_de_teste("en0", "192.168.1.5", Origem::Fisica),
    ];
    let enderecos = Enderecos::a_partir_de(&achados, 8383);
    assert_eq!(enderecos.alvo().to_string(), "192.168.1.5:8383");
    assert!(
        !enderecos.todos().iter().any(|a| a.ip().to_string().starts_with("172.30.")),
        "uma ponte entrou: {:?}",
        enderecos.todos()
    );
}

#[test]
fn um_ipv4_global_ganha_do_local_e_declara_endereco_direto() {
    // O caso da VPS. É o degrau 1 com endereço próprio, e é o único em que não
    // há nada a fazer a respeito.
    let achados = vec![
        achado_de_teste("en0", "192.168.1.5", Origem::Fisica),
        achado_de_teste("eth0", "203.0.113.7", Origem::Fisica),
    ];
    let enderecos = Enderecos::a_partir_de(&achados, 8383);
    assert_eq!(enderecos.degrau(), Degrau::EnderecoDireto);
    assert_eq!(enderecos.alvo().to_string(), "203.0.113.7:8383");
}

#[test]
fn so_um_tunel_declara_rede_local_ou_vpn() {
    // O caso do Tailscale.
    let achados = vec![achado_de_teste("utun3", "100.101.102.103", Origem::Tunel)];
    let enderecos = Enderecos::a_partir_de(&achados, 8383);
    assert_eq!(enderecos.degrau(), Degrau::RedeLocalOuVpn);
}
```

> **Nota:** `achado_de_teste` é um auxiliar que você escreve neste mesmo módulo, montando um `interfaces::Achado`. Leia `crates/seele-server/src/alcance/interfaces.rs:79-105` para os campos exatos — o arquivo não é tocado por este plano e é a fonte da verdade.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server um_endereco_de_ponte_nunca_e_anunciado`
Expected: FAIL — `Enderecos` não existe.

- [ ] **Step 3: Implementar**

Substitua `Escada`, `Alcance` e `Escuta` por um tipo só. `Tipo` fica com três variantes — `Local`, `Global`, `Tunel` — e `Ponte` sai junto com o filtro que o descarta. A ordem de preferência é `Global` → `Local` → `Tunel`, pelo critério que o `Tipo::ordem()` já usava.

```rust
/// Os endereços desta máquina que servem para receber gente.
///
/// O que sobrou da escada do ADR 0022 depois que o ADR 0036 a removeu. Deixou
/// de ser uma escalada — não há mais roteador a consultar nem ponto de encontro
/// a esperar — e virou uma pergunta síncrona sobre as placas desta máquina.
#[derive(Debug, Clone)]
pub struct Enderecos {
    alvos: Vec<SocketAddr>,
    degrau: Degrau,
}
```

`descobrir_enderecos()` continua chamando `interfaces::descobrir()`; o filtro novo descarta `Origem::Virtual` e todo IP que não seja IPv4.

Em `hospedagem.rs`, troque `Escada::subir(...)` por `Enderecos::desta_maquina(porta)` e `Hospedagem::alcance()` por `Hospedagem::enderecos()`.

- [ ] **Step 4: Rodar e ver passar; rodar o portão; commitar**

```bash
cargo test -p seele-server
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
refactor(alcance): a escada vira Enderecos

Escada, Alcance, Escuta e Pilha colapsam num tipo que responde uma pergunta
síncrona: quais IPv4 desta máquina servem para receber gente.

`Ponte` sai junto. WSL, Hyper-V e Docker produzem endereços que nunca
respondem de fora, e com um endereço no convite um deles escolhido por
acidente seria o link inteiro.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: Quem hospeda escolhe o endereço

**Files:**
- Modify: `apps/seele-app/src/main.rs` (comando que devolve os endereços)
- Modify: `apps/seele-app/ui/` — a tela de hospedar
- Modify: `crates/seele-ffi/src/lib.rs` (expor a lista)
- Test: `crates/seele-ffi/src/lib.rs`

**Interfaces:**
- Consumes: da Task 13, `Enderecos::todos()`.
- Produces: comando Tauri `enderecos_para_hospedar() -> Vec<EnderecoOferecido>`, onde `EnderecoOferecido { link: String, rotulo: String }`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn cada_endereco_oferecido_vira_um_link_completo() {
    // A tela oferece links prontos, não endereços para a pessoa montar. Um
    // link sem `fp=` é um link que conecta cego (ADR 0003), e oferecer um
    // desses seria pior que não oferecer nenhum.
    let oferecidos = enderecos_oferecidos(
        &["192.168.1.5:8383".parse().unwrap(), "100.101.102.103:8383".parse().unwrap()],
        FP,
    );
    assert_eq!(oferecidos.len(), 2);
    for oferecido in &oferecidos {
        assert!(oferecido.link.starts_with("seele://"));
        assert!(oferecido.link.contains(&format!("fp={FP}")));
        assert!(!oferecido.rotulo.is_empty());
    }
    assert_eq!(oferecidos[0].rotulo, "NA SUA REDE");
    assert_eq!(oferecidos[1].rotulo, "PELA VPN");
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-ffi cada_endereco_oferecido_vira_um_link_completo`
Expected: FAIL — a função não existe.

- [ ] **Step 3: Implementar**

Escreva `enderecos_oferecidos` na FFI, e o comando Tauri que o chama. O rótulo sai do `Tipo` do endereço: `Global` → `"PELA INTERNET"`, `Local` → `"NA SUA REDE"`, `Tunel` → `"PELA VPN"`.

Na tela de hospedar, troque o link único por uma lista: um botão de copiar por endereço, com o rótulo ao lado. O primeiro da lista continua sendo o destacado — é o que `Enderecos::alvo()` escolheu.

- [ ] **Step 4: Rodar o portão; conferir na tela**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
cargo build --release -p seele-app && ./target/release/seele-app
```

Aperte HOSPEDAR AQUI e confirme que aparece um link por interface, cada um com rótulo, e que copiar qualquer um deles dá um `seele://` com `fp=`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(app): quem hospeda escolhe por qual endereço convidar

A tela oferece um link pronto por interface — na sua rede, pela VPN, pela
internet — em vez de um link só com todos os endereços dentro.

É a metade de produto da decisão de tirar o `alt=`: o link ficou
determinístico, e quem hospeda passou a saber qual escolheu.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

# Fase 5 · O `seeled` vira artefato

> **Esta fase não compartilha arquivo com nenhuma das anteriores** e pode ser feita em paralelo por outra pessoa.

### Task 15: O `seeled` constrói em musl

**Files:**
- Modify: `.github/workflows/release.yml:246-270`
- Create: `.github/workflows/` — nenhum arquivo novo; o job existente ganha passos

**Interfaces:**
- Consumes: nada.
- Produces: `target/x86_64-unknown-linux-musl/release/seeled` e `target/aarch64-unknown-linux-musl/release/seeled` no runner Linux.

- [ ] **Step 1: Provar localmente antes de tocar o CI**

```bash
rustup target add x86_64-unknown-linux-musl
brew install FiloSottile/musl-cross/musl-cross   # macOS; no Linux: apt install musl-tools
CC_x86_64_unknown_linux_musl=x86_64-linux-musl-gcc \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc \
  cargo build --release --bin seeled --target x86_64-unknown-linux-musl
```

Expected: um binário em `target/x86_64-unknown-linux-musl/release/seeled`.

**Se isto falhar** — `rusqlite` com `bundled` compila SQLite em C, e o `ring` tem assembly —, gaste no máximo um dia. Passado isso, use o recuo do §7 do spec: trocar `ubuntu-24.04` por `ubuntu-22.04` na matriz de `release.yml:100-102`, o que baixa o piso de glibc de 2.39 para 2.35. **O recuo tem de aparecer na mensagem do commit** — um binário que se anuncia estático sem ser é pior que o de hoje.

- [ ] **Step 2: Confirmar que é estático**

```bash
file target/x86_64-unknown-linux-musl/release/seeled
```

Expected: contém `statically linked`. Se disser `dynamically linked`, não está pronto.

- [ ] **Step 3: Levar para o CI**

Em `.github/workflows/release.yml`, no job de empacotar, troque o passo "Compilar seeled" para o Linux:

```yaml
      - name: Compilar seeled (musl, Linux)
        if: matrix.nome == 'linux'
        run: |
          sudo apt-get update && sudo apt-get install -y musl-tools gcc-aarch64-linux-gnu
          rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
          cargo build --release --bin seeled --target x86_64-unknown-linux-musl
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc \
            cargo build --release --bin seeled --target aarch64-unknown-linux-musl
          for alvo in x86_64 aarch64; do
            file "target/${alvo}-unknown-linux-musl/release/seeled" | grep -q "statically linked" \
              || { echo "::error::o binário ${alvo} não saiu estático"; exit 1; }
          done
```

O `grep -q "statically linked"` é a guarda, e ela é o ponto: sem ela o dia em que o musl parar de valer passa despercebido.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "$(cat <<'EOF'
build(release): o seeled do Linux sai estático, em musl

x86_64 e aarch64. O binário de antes vinha de ubuntu-24.04 e carregava um
piso de glibc 2.39 — não roda em Debian 12 nem em Ubuntu 22.04, que é
metade das VPS baratas.

A conferência de `statically linked` é guarda e não zelo: sem ela, o dia em
que o musl parar de valer passa despercebido.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 16: O artefato ganha nome honesto

**Files:**
- Modify: `.github/workflows/release.yml:484-493`
- Modify: `install.sh:55-59, 86`
- Modify: `install.ps1`

**Interfaces:**
- Consumes: da Task 15, os dois binários musl.
- Produces: `seeled-{versão}-linux-x86_64.tar.gz`, `seeled-{versão}-linux-aarch64.tar.gz`, `seeled-{versão}-macos.tar.gz`, `seeled-{versão}-windows-x86_64.zip`.

- [ ] **Step 1: Trocar o empacotamento**

Em `release.yml`, o bloco que hoje monta `seele-cli-...` passa a montar um arquivo por arquitetura no Linux:

```bash
          if [ "${{ matrix.nome }}" = "windows" ]; then
            7z a "$destino/seeled-${versao}-windows-x86_64.zip" ./target/release/seeled.exe
          elif [ "${{ matrix.nome }}" = "linux" ]; then
            for alvo in x86_64 aarch64; do
              tar -czf "$destino/seeled-${versao}-linux-${alvo}.tar.gz" \
                -C "target/${alvo}-unknown-linux-musl/release" seeled
            done
          else
            tar -czf "$destino/seeled-${versao}-macos.tar.gz" -C target/release seeled
          fi
```

E o comentário acima dele deixa de dizer "o arquivo da CLI": passa a dizer que é o servidor, que é o que se põe numa VPS, e que o `install.sh` o baixa.

- [ ] **Step 2: Acompanhar no instalador**

Em `install.sh`, remova o bloco `install.sh:55-59` que recusa não-x86_64 e ponha:

```sh
if [ "$SISTEMA" = linux ]; then
    case "$(uname -m)" in
        x86_64|amd64)   ARQUITETURA=x86_64 ;;
        aarch64|arm64)  ARQUITETURA=aarch64 ;;
        *) erro "não há pacote para $(uname -m).
       Compile do código-fonte: cargo build --release --bin seeled" ;;
    esac
    PACOTE="seeled-${NUMERO}-linux-${ARQUITETURA}.tar.gz"
else
    PACOTE="seeled-${NUMERO}-macos.tar.gz"
fi
```

Mova a definição de `PACOTE` para depois da definição de `NUMERO` e remova a linha 86 antiga. Faça o equivalente em `install.ps1`.

- [ ] **Step 3: Conferir o script sem publicar nada**

```bash
sh -n install.sh && echo "sintaxe ok"
SEELE_VERSION=v0.8.3 SEELE_BASE=file:///dev/null sh install.sh 2>&1 | head -20
```

Expected: a sintaxe passa, e a execução falha no download — que é o esperado, porque `SEELE_BASE` aponta para lugar nenhum. O que importa é ele ter montado o nome certo antes de falhar.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
build(release): o artefato do servidor se chama seeled

`seele-cli-{versão}-linux.tar.gz` não dizia a ninguém que ali dentro está o
servidor — e é ele que se põe numa VPS. Passa a ser
`seeled-{versão}-linux-{x86_64,aarch64}.tar.gz`.

O install.sh deixa de recusar aarch64.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 17: Supervisão e documentação de operação

**Files:**
- Create: `empacotar/seele.service`
- Create: `docs/vps.md`
- Modify: `.github/workflows/release.yml` (pôr o `.service` dentro do tarball do Linux)

**Interfaces:**
- Consumes: da Task 16, o nome do tarball.
- Produces: nada em código.

- [ ] **Step 1: Escrever a unidade**

`empacotar/seele.service`:

```ini
[Unit]
Description=SEELE
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=seele
Group=seele
ExecStart=/usr/local/bin/seeled 0.0.0.0:8383
Environment=SEELE_HOME=/var/lib/seele
Restart=on-failure
RestartSec=5

# O servidor não precisa de nada além do próprio diretório de dados. Isto não
# é paranoia: ele fica exposto na internet por definição, e um daemon que
# escreve só onde precisa é um daemon cuja pior falha é menor.
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
StateDirectory=seele
ReadWritePaths=/var/lib/seele

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 2: Escrever `docs/vps.md`**

Cobre, nesta ordem, e cada seção responde uma pergunta que alguém teria:

1. **O que você vai precisar** — uma VPS com IPv4 público, e a porta UDP 8383 aberta no firewall do provedor. Uma frase sobre por que UDP e não TCP.
2. **Instalar** — baixar o tarball da aba Releases, `install -m755 seeled /usr/local/bin/`, criar o usuário `seele`, `install -m644 seele.service /etc/systemd/system/`, `systemctl enable --now seele`.
3. **Fechar a porta antes de abrir a porta** — `seeled senha` ou `seeled convite`, e por que um servidor aberto na internet não é o padrão que se quer. Esta seção existe por causa da guarda da Task 18; escreva as duas juntas.
4. **A impressão digital** — onde ela é impressa, por que anotá-la, e o que fazer quando um cliente disser que ela mudou (ADR 0003).
5. **Convidar** — o formato do link, e que um nome de domínio funciona no lugar do IP.
6. **Backup** — o que copiar de `/var/lib/seele` e com o serviço parado.
7. **Atualizar** — baixar, `systemctl stop`, trocar o binário, `systemctl start`. E a nota honesta: quem estava conectado reconecta sozinho dentro da janela de graça, e o histórico sobrevive porque o banco é em arquivo.

- [ ] **Step 3: Pôr a unidade dentro do tarball**

No `release.yml`, antes do `tar -czf` do Linux, copie `empacotar/seele.service` para junto do binário e inclua os dois no tarball.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
docs(vps): como pôr o seeled numa VPS

Uma unidade systemd com as restrições que um daemon exposto merece, e um
documento que responde as sete perguntas que alguém teria — na ordem em que
as teria.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 18: O `seeled` recusa subir aberto num endereço público

> **Esta task é a única do plano marcada como reversível.** Foi proposta por quem escreveu o spec, não pedida por quem decidiu o escopo (§7 do spec). Se a revisão a rejeitar, pule-a — nada depende dela.

**Files:**
- Modify: `crates/seele-server/src/main.rs`
- Test: `crates/seele-server/src/main.rs` ou `crates/seele-server/src/lib.rs`

**Interfaces:**
- Consumes: da Task 6, a escuta é IPv4.
- Produces: nada que outra task use.

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[test]
fn subir_aberto_num_endereco_publico_e_recusado() {
    // Somos nós que estamos mandando a pessoa pôr isto numa VPS. Terminar esse
    // caminho com um servidor aberto na internet seria publicar o defeito em
    // vez de evitá-lo.
    assert!(recusa_por_estar_aberto(
        "0.0.0.0:8383".parse().unwrap(),
        Admissao::Aberta
    ));
    assert!(recusa_por_estar_aberto(
        "203.0.113.7:8383".parse().unwrap(),
        Admissao::Aberta
    ));
}

#[test]
fn subir_aberto_na_rede_de_casa_continua_valendo() {
    // O caso que sempre funcionou, e que não pode ser quebrado por uma guarda
    // que existe para outra situação: dois amigos na mesma rede, sem senha.
    assert!(!recusa_por_estar_aberto(
        "192.168.1.5:8383".parse().unwrap(),
        Admissao::Aberta
    ));
    assert!(!recusa_por_estar_aberto(
        "127.0.0.1:8383".parse().unwrap(),
        Admissao::Aberta
    ));
    // E um servidor fechado sobe em qualquer lugar.
    assert!(!recusa_por_estar_aberto(
        "203.0.113.7:8383".parse().unwrap(),
        Admissao::Fechada
    ));
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server recusa_por_estar_aberto`
Expected: FAIL — a função não existe.

- [ ] **Step 3: Implementar**

```rust
/// Se este servidor não deve subir do jeito que foi pedido.
///
/// Uma decisão sobre valores, separada da que chama o sistema, pelo mesmo
/// motivo de `Alcance::decidir`: dá para testar sem abrir socket nenhum.
///
/// `0.0.0.0` conta como público. Ele atende em **todas** as placas, e numa VPS
/// uma delas é a que está na internet.
fn recusa_por_estar_aberto(escuta: SocketAddr, admissao: Admissao) -> bool {
    if admissao != Admissao::Aberta {
        return false;
    }
    let ip = escuta.ip();
    !(ip.is_loopback() || e_privado(ip))
}
```

No `main`, quando `recusa_por_estar_aberto` for verdadeiro, escreva a recusa e saia com código diferente de zero:

```
este SEELE subiria aberto num endereço que a internet alcança, e ele não vai.

qualquer pessoa que descobrisse a porta entraria — sem convite, sem senha,
sem aparecer na portaria.

feche a porta antes, com um dos dois:

  seeled senha            uma senha para todo mundo
  seeled convite          um convite de uso único por pessoa

para hospedar aberto de propósito na sua própria rede, escute nela:

  seeled 192.168.x.x:8383
```

- [ ] **Step 4: Rodar e ver passar; conferir à mão**

```bash
cargo test -p seele-server recusa_por_estar_aberto
cargo build --release --bin seeled
./target/release/seeled 0.0.0.0:8383      # recusa, com a mensagem acima
./target/release/seeled 127.0.0.1:8383    # sobe
```

- [ ] **Step 5: Rodar o portão e commitar**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
feat(seeled): recusa subir aberto num endereço que a internet alcança

Somos nós que passamos a mandar a pessoa pôr isto numa VPS. Terminar esse
caminho com um servidor aberto seria publicar o defeito em vez de evitá-lo.

Na rede de casa nada muda: o caso de dois amigos sem senha continua
subindo, porque é a situação em que a porta aberta é a escolha certa.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

# Fase 6 · ADRs e documentação

### Task 19: Os três ADRs

**Files:**
- Create: `docs/adr/0036-a-conexao-e-direta.md`
- Create: `docs/adr/0037-o-convite-leva-um-endereco.md`
- Create: `docs/adr/0038-o-dono-de-um-servidor.md`
- Modify: `docs/adr/0022-alcancar-um-dogma-pela-internet.md` (marcar como superseded)
- Modify: `docs/adr/0006-esquema-de-uri.md` (marcar a emenda)
- Modify: `docs/adr/README.md`

- [ ] **Step 1: Ler o template**

```bash
cat docs/adr/0000-template.md
cat docs/adr/0022-alcancar-um-dogma-pela-internet.md | head -40
```

Siga a forma exata do template. Não invente seções.

- [ ] **Step 2: Escrever o 0036**

Registra as decisões 1 e 2 do §1 do spec. **A seção de consequências tem de nomear o que se perde**, e são três coisas: uma VPS só com IPv6 deixa de servir; quem hospeda de casa atrás de CGNAT sem VPN deixa de ter caminho; e o projeto deixa de operar qualquer infraestrutura. As três são escolhas, e um ADR que só liste ganhos não é um ADR.

- [ ] **Step 3: Escrever o 0037**

Registra a decisão 4. A parte que não pode faltar é **por que `fp=` sobreviveu à poda** — o argumento está no §4 do spec e é o que impede alguém de "simplificar" o link mais um passo daqui a seis meses.

- [ ] **Step 4: Escrever o 0038**

**Estado: aceito, não implementado.** Registra a escolha de administração pelo fio contra painel web, com os três argumentos do §10 do spec: certificado de autoridade exigiria domínio obrigatório e contradiz o ADR 0003; `Permission::AdministerServer` já existe em `permissions.rs` sem exercente; pelo ADR 0002 as duas cascas ganham de graça. Escreva em voz alta: **não criar um `seele-admin`**.

- [ ] **Step 5: Marcar o que foi superseded**

No cabeçalho do 0022, mude o estado para `Superseded por 0036`. No 0006, acrescente uma linha de emenda apontando para o 0037. Atualize `docs/adr/README.md`.

- [ ] **Step 6: Commit**

```bash
git add docs/adr
git commit -m "$(cat <<'EOF'
docs(adr): 0036, 0037 e 0038

0036 supersede o 0022 e nomeia as três coisas que se perdem. 0037 emenda o
0006 e registra por que o `fp=` sobreviveu à poda. 0038 é aceito e não
implementado: administração pelo fio, e não painel web.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 20: A documentação alcança o código

**Files:**
- Delete: `docs/alcance-pela-internet.md`, `docs/ponto-de-encontro.md`
- Modify: `docs/como-testar.md`, `docs/teste-duas-maquinas.md`, `docs/glossario.md`, `docs/windows.md`

- [ ] **Step 1: Apagar o que descreve o que não existe**

```bash
git rm docs/alcance-pela-internet.md docs/ponto-de-encontro.md
grep -rn "ponto de encontro\|degrau [234]\|UPnP\|NAT-PMP\|furo de NAT\|IPv6" docs/ specs/ README.md
```

Cada linha que o `grep` achar é uma decisão: corrigir, ou apagar. **Nenhuma fica.**

- [ ] **Step 2: Atualizar `teste-duas-maquinas.md`**

É o documento mais importante desta task — é o portão de campo. Ele passa a ter dois cenários em vez de um: duas máquinas na mesma LAN, e uma máquina contra um `seeled` numa VPS. O segundo é novo e é o que este ciclo inteiro existe para tornar possível.

- [ ] **Step 3: Rodar o portão e commitar**

```bash
cargo test --workspace
git add -A
git commit -m "$(cat <<'EOF'
docs: a documentação alcança o código

Saem os dois documentos que descrevem a escada e o ponto de encontro. O
teste de duas máquinas ganha o cenário da VPS, que é o que este ciclo
existe para tornar possível.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## O portão de campo

**Nenhuma suíte deste repositório prova que duas máquinas reais se acham.** Depois da Task 20, e antes de chamar isto de pronto, rode `docs/teste-duas-maquinas.md` de verdade, nos dois cenários:

1. **LAN** — o Windows hospedando pelo app, o Mac entrando pelo link "NA SUA REDE". Este é o cenário que falhou de forma intermitente em 25/08; se ele ainda for intermitente, o ciclo não resolveu o que dizia resolver.
2. **VPS** — um `seeled` numa VPS de verdade, instalado pelo `docs/vps.md` sem atalho nenhum, e dois clientes entrando de redes diferentes.

Anote os resultados em `docs/m1-medicoes.md`.
