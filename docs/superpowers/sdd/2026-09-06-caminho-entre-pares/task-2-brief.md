### Task 2: A ponta que o cliente já tem aprende a atender

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `Identidade`, `ErroDePar` (Task 1)
- Produces: `pub fn passar_a_atender(ponta: &quinn::Endpoint, identidade: Identidade) -> Result<(), ErroDePar>`

- [ ] **Step 1: Write the failing test**

Dentro de `mod testes`:

```rust
#[tokio::test]
async fn uma_ponta_de_cliente_passa_a_atender_sem_socket_novo() {
    // **Nem escuta nova, nem porta nova.** `set_server_config` recebe `&self`,
    // então a ponta que o cliente já usa para falar com o servidor aprende a
    // atender na mesma porta — e naquela porta o mapeamento de NAT já está
    // vivo, mantido pelo keep-alive da conexão que já existe. Abrir uma porta
    // à parte perderia essa propriedade, que é a melhor do desenho.
    let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let porta_antes = ponta.local_addr().unwrap();

    passar_a_atender(&ponta, identidade_efemera().unwrap()).unwrap();

    assert_eq!(
        ponta.local_addr().unwrap(),
        porta_antes,
        "atender trocou a porta: o mapeamento de NAT que já estava vivo se perdeu"
    );
    // E ela de fato aceita: sem `set_server_config`, `accept()` devolve `None`
    // na hora em que a ponta é fechada.
    let aceitando = tokio::spawn(async move { ponta.accept().await.is_some() });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    aceitando.abort();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::uma_ponta_de_cliente`
Expected: FAIL de compilação — `passar_a_atender` não existe.

- [ ] **Step 3: Write minimal implementation**

Em `par.rs`:

```rust
/// Diz a uma ponta que já existe que ela também atende.
///
/// **Só é chamada quando a pessoa optou por emprestar a subida.** Quem não
/// optou nunca passa por aqui, e a ponta dela continua só discando, como antes
/// desta onda existir.
///
/// # Errors
///
/// Falha se o `rustls` recusar o certificado ou a chave.
pub fn passar_a_atender(
    ponta: &quinn::Endpoint,
    identidade: Identidade,
) -> Result<(), ErroDePar> {
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(identidade.cadeia, identidade.chave)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    ponta.set_server_config(Some(quinn::ServerConfig::with_crypto(std::sync::Arc::new(
        quic,
    ))));
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 3 testes.

- [ ] **Step 5: Prove the guard**

Troque `set_server_config(Some(...))` por `set_server_config(None)`. Esperado: o teste ainda passa na asserção de porta (ela não cobre isto) — **e é por isso que a Task 4 existe**, que é onde atender de verdade é provado. Anote isso no doc do teste e desfaça.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): a ponta do cliente aprende a atender, na mesma porta"
```

---

