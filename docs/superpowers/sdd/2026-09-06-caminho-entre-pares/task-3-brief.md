### Task 3: A conferência da impressão digital do par

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `ErroDePar` (Task 1)
- Produces: `pub(crate) struct ConfereImpressao { esperada: String }` implementando `rustls::client::danger::ServerCertVerifier`, e `pub(crate) fn config_de_cliente(esperada: String) -> Result<quinn::ClientConfig, ErroDePar>`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn o_par_certo_passa_e_o_errado_e_recusado_com_o_motivo_certo() {
    // **A identidade aqui não é TOFU — é apresentação.** O ADR 0003 vale para
    // o `seele://`, onde não há intermediário e a primeira vez tem de ser
    // confiada. Aqui há: os dois clientes já fixaram o mesmo servidor e já se
    // autenticaram nele por chave pública (ADR 0004). O servidor ocupa o lugar
    // que o link ocupa no `seele://`.
    //
    // Sem pino novo em disco, e sem par anônimo alimentando quadro.
    let identidade = identidade_efemera().unwrap();
    let der = identidade.cadeia.first().unwrap().clone();
    let certa = impressao(&identidade);

    assert!(ConfereImpressao::nova(certa.clone()).confere(&der).is_ok());

    let erro = ConfereImpressao::nova("f".repeat(64)).confere(&der).unwrap_err();
    match erro {
        ErroDePar::ImpressaoNaoBate { esperada, veio } => {
            assert_eq!(esperada, "f".repeat(64));
            assert_eq!(veio, certa);
        }
        outro => panic!("o motivo errado saiu de uma impressão que não bate: {outro:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::o_par_certo_passa`
Expected: FAIL de compilação — `ConfereImpressao` não existe.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Aceita **um** certificado, o que o servidor apresentou, e nenhum outro.
///
/// Espelho do `TofuVerifier` de `crate::client`, com a diferença que é o
/// assunto todo: aquele **aprende** na primeira vez e guarda; este não aprende
/// nada e não guarda nada. A impressão digital chega pelo servidor a cada
/// apresentação, então não há primeira vez a confiar.
#[derive(Debug)]
pub(crate) struct ConfereImpressao {
    esperada: String,
    provedor: std::sync::Arc<rustls::crypto::CryptoProvider>,
}

impl ConfereImpressao {
    pub(crate) fn nova(esperada: String) -> Self {
        Self {
            esperada,
            provedor: std::sync::Arc::new(rustls::crypto::ring::default_provider()),
        }
    }

    /// A conferência em si, fora do `trait`, para o teste poder afirmá-la
    /// sem montar uma sessão TLS inteira.
    pub(crate) fn confere(
        &self,
        certificado: &rustls::pki_types::CertificateDer<'_>,
    ) -> Result<(), ErroDePar> {
        let veio = seele_proto::transport::certificate_fingerprint(certificado.as_ref());
        if veio == self.esperada {
            Ok(())
        } else {
            Err(ErroDePar::ImpressaoNaoBate {
                esperada: self.esperada.clone(),
                veio,
            })
        }
    }
}
```

E o `impl rustls::client::danger::ServerCertVerifier for ConfereImpressao`, seguindo **exatamente** a forma do `TofuVerifier` em `crates/seele-core/src/client.rs` — copie os métodos `verify_tls12_signature`, `verify_tls13_signature` e `supported_verify_schemes` de lá, e faça `verify_server_cert` chamar `self.confere(end_entity)`, traduzindo `Err` para `rustls::Error::General(erro.to_string())`.

Mais o construtor do config:

```rust
/// O `ClientConfig` com que se disca para um par.
///
/// # Errors
///
/// Falha se o `rustls` recusar a configuração.
pub(crate) fn config_de_cliente(esperada: String) -> Result<quinn::ClientConfig, ErroDePar> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(ConfereImpressao::nova(esperada)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    Ok(quinn::ClientConfig::new(std::sync::Arc::new(quic)))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 4 testes.

- [ ] **Step 5: Prove the guard**

Faça `confere` devolver sempre `Ok(())`. Esperado: FALHA em `o_par_certo_passa_e_o_errado_e_recusado_com_o_motivo_certo`. Desfaça.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): a impressão digital do par é conferida contra a que o servidor apresentou"
```

---

