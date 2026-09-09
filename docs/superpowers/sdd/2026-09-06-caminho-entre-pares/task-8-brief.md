### Task 8: O cliente pede ao par, e cai para o servidor quando falha

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (braço novo perto de `:2441`, onde `HostUplink` já é tratado)
- Modify: `crates/seele-core/src/par.rs`
- Modify: `crates/seele-core/src/state.rs` (o braço provisório que a Task 6 deixou, perto de `:1213`)

**O braço de `state.rs` é seu, e ele tem dono só desde 06/09.** A Task 6 precisou
acrescentá-lo porque `Room::apply` faz `match` exaustivo sobre `ServerMessage` e
parou de compilar com as variantes novas. Ele é andaime com `warn!`, e **não é
ponto morto**: `crates/seele-ffi/src/lib.rs:3358` chama `room.apply(message)` para
toda mensagem recebida em produção. Decida o que a `Room` faz com
`AssistaTelaPor` e `SirvaTelaPara` — mesmo que a resposta seja «nada, porque quem
age é o `enlace`», essa resposta tem de estar escrita ali em vez de um `warn!` de
andaime.

**Interfaces:**
- Consumes: `ligar`, `ParLigado`, `ErroDePar` (Tasks 1–4); `ServerMessage::AssistaTelaPor` (Task 6)
- Produces: `pub enum PorOndeAssistir { Par(Box<ParLigado>), Servidor }` e `pub async fn por_onde(ponta: &quinn::Endpoint, enderecos: &[SocketAddr], impressao: String, prazo: Duration) -> PorOndeAssistir`
  - `Box` porque `ParLigado` carrega uma `quinn::Connection` e a variante `Servidor` não carrega nada: sem ele o `enum` inteiro tem o tamanho da maior variante, e o `clippy::large_enum_variant` reclama com razão.

- [ ] **Step 1: Write the failing test**

Em `par.rs`, em `mod testes`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn quando_o_par_nao_liga_a_resposta_e_o_servidor() {
    // **A malha é alívio, nunca dependência.** Decisão de quem desenha o
    // produto, 05/09/2026: ninguém perde imagem por causa da máquina de outra
    // pessoa. É a propriedade de segurança da malha inteira, e por isso ela é
    // provada aqui e não adiada para o subprojeto B — uma propriedade de
    // segurança provada depois é uma propriedade que passou um tempo sem
    // existir.
    let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    // Uma porta em que ninguém atende: o endereço é válido e o aperto de mão
    // nunca fecha.
    let ninguem = SocketAddr::from(([127, 0, 0, 1], 1));

    let onde = por_onde(
        &ponta,
        &[ninguem],
        "a".repeat(64),
        std::time::Duration::from_millis(300),
    )
    .await;

    assert!(
        matches!(onde, PorOndeAssistir::Servidor),
        "o par não ligou e o cliente não caiu para o servidor: alguém ficou sem imagem"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::quando_o_par_nao_liga`
Expected: FAIL de compilação — `por_onde` não existe.

- [ ] **Step 3: Write minimal implementation**

Em `par.rs`:

```rust
/// De onde a imagem desta transmissão vai vir.
pub enum PorOndeAssistir {
    /// Por este par.
    Par(Box<ParLigado>),
    /// Pelo servidor, como sempre.
    Servidor,
}

/// Tenta o par, e cai para o servidor sem drama quando ele não vem.
///
/// **Nunca devolve erro**, e é de propósito: quem chama não tem decisão a tomar
/// sobre a falha. A malha é alívio; falhar nela é voltar ao caminho de antes
/// dela existir, e isso não é um erro, é o normal.
///
/// O motivo enumerado da falha **não some**: ele vai para o `tracing` aqui e
/// para o servidor no `ParFalhou` que quem chama manda.
pub async fn por_onde(
    ponta: &quinn::Endpoint,
    enderecos: &[std::net::SocketAddr],
    impressao: String,
    prazo: std::time::Duration,
) -> PorOndeAssistir {
    match ligar(ponta, enderecos, impressao, prazo).await {
        Ok(ligado) => PorOndeAssistir::Par(Box::new(ligado)),
        Err(erro) => {
            tracing::info!(%erro, "o par não veio; a tela vem do servidor");
            PorOndeAssistir::Servidor
        }
    }
}
```

Em `enlace.rs`, ao lado do braço de `ServerMessage::HostUplink` (`:2441`), trate `ServerMessage::AssistaTelaPor` chamando `por_onde`; no caso `Servidor`, mande `ClientMessage::ParFalhou` com o motivo que o `tracing` registrou. Trate `ServerMessage::SirvaTelaPara` chamando `passar_a_atender` (se ainda não estiver atendendo) e `ligar` para o outro lado — as duas tentativas simultâneas são o furo.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 7 testes.

- [ ] **Step 5: Prove the guard**

Faça `por_onde` devolver `PorOndeAssistir::Par` mesmo no `Err` (com um `unreachable` temporário para compilar). Esperado: `quando_o_par_nao_liga_a_resposta_e_o_servidor` FALHA. Desfaça.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs crates/seele-core/src/enlace.rs
git commit -m "feat(par): pede a tela ao par, e cai para o servidor quando ele não vem"
```

---

