### Task 5: Quem atende também confere, e alguém precisa atender

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `Identidade`, `ErroDePar`, `ConfereImpressao`, `passar_a_atender` (Tasks 1–4)
- Produces:
  - `pub fn passar_a_atender(ponta: &quinn::Endpoint, identidade: Identidade, impressao_de_quem_vem: String) -> Result<(), ErroDePar>` — **assinatura mudada**, ver abaixo
  - `pub async fn atender(ponta: quinn::Endpoint) -> Option<ParLigado>`

**Por que esta tarefa existe, e ela é conserto de um defeito meu.** As quatro
anteriores foram escritas supondo que instalar o `ServerConfig` bastava. Não
basta, e a Task 4 mediu isso em vez de supor: o `quinn` enfileira o `Incoming` e
**não responde nada** até alguém chamar `Endpoint::accept()`. Dois pares que só
chamem `ligar` nunca se ligam — o que a Task 4 só contornou com um auxiliar
dentro dos próprios testes.

E ao olhar o buraco apareceu o segundo, pior: **quem aceita não confere
ninguém.** `with_no_client_auth()` quer dizer que a conexão *aceita* nunca passa
pelo `ConfereImpressao`. Como os dois lados discam e qualquer um dos dois pode
vencer, metade das ligações fica sem conferência nenhuma — e quem empresta
serviria quadro a qualquer um que alcançasse a porta. A spec promete o
contrário, com todas as letras: *«a conexão que chega é conferida contra a
impressão digital que o servidor apresentou»*.

**O material para fechar já existe e estava sem uso.** A mensagem
`SirvaTelaPara { screen, enderecos, impressao }` leva a quem empresta a
impressão digital de quem vai receber. Era para isso — e o desenho não dizia o
que fazer com ela. É ela que o verificador de cliente confere.

- [ ] **Step 1: Write the failing test**

Em `par.rs`, em `mod testes`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn quem_atende_recusa_quem_o_servidor_nao_apresentou() {
    // **A parede simétrica.** Os dois lados discam, então qualquer um dos dois
    // pode acabar sendo quem aceita — e quem aceita não passa pelo
    // `ConfereImpressao`, que só roda em quem disca. Sem esta parede, metade
    // das ligações não confere nada, e quem empresta a subida serve quadro a
    // qualquer um que alcance a porta.
    //
    // Não é hipótese: `with_no_client_auth()` é literalmente «aceite qualquer
    // um».
    let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let ia = identidade_efemera().unwrap();
    let impressao_do_anfitriao = impressao(&ia);

    let intruso = identidade_efemera().unwrap();
    let esperada_de_outro = impressao(&identidade_efemera().unwrap());

    // O anfitrião só aceita quem apresentar `esperada_de_outro` — e o intruso
    // apresenta a dele, que é outra.
    passar_a_atender(&anfitriao, ia, esperada_de_outro).unwrap();
    let onde = anfitriao.local_addr().unwrap();
    let atendendo = tokio::spawn(atender(anfitriao.clone()));

    let ponta_do_intruso = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    passar_a_atender(&ponta_do_intruso, intruso, impressao_do_anfitriao.clone()).unwrap();
    let tentou = ligar(
        &ponta_do_intruso,
        &[onde],
        impressao_do_anfitriao,
        std::time::Duration::from_secs(3),
    )
    .await;

    assert!(
        tentou.is_err(),
        "o intruso apresentou um certificado que o anfitrião nunca esperou, e entrou"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), atendendo)
            .await
            .map(|ligado| ligado.ok().flatten().is_none())
            .unwrap_or(true),
        "o anfitrião deu por boa uma ligação de quem o servidor não apresentou"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dois_pares_apresentados_se_ligam_pelos_dois_lados() {
    // O caso feliz, e a razão de `atender` existir no produto e não só no
    // teste: com os dois lados discando **e** os dois lados atendendo, a
    // primeira ligação que fechar vence, venha de que direção vier. É isso que
    // faz o furo assimétrico se resolver sozinho.
    let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let ia = identidade_efemera().unwrap();
    let impressao_a = impressao(&ia);
    let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let ib = identidade_efemera().unwrap();
    let impressao_b = impressao(&ib);

    passar_a_atender(&a, ia, impressao_b.clone()).unwrap();
    passar_a_atender(&b, ib, impressao_a.clone()).unwrap();
    let onde_a = a.local_addr().unwrap();
    let atendendo = tokio::spawn(atender(a.clone()));

    let ligado = ligar(&b, &[onde_a], impressao_a, std::time::Duration::from_secs(5))
        .await
        .expect("dois pares apresentados um ao outro não se ligaram");
    assert_eq!(ligado.como, ComoChegou::Local);

    let do_outro_lado = tokio::time::timeout(std::time::Duration::from_secs(2), atendendo)
        .await
        .expect("o lado que atende travou")
        .expect("a tarefa de atender morreu");
    assert!(
        do_outro_lado.is_some(),
        "quem atendeu não devolveu a ligação: quem empresta não tem por onde mandar quadro"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p seele-core --lib par::testes::quem_atende par::testes::dois_pares_apresentados`
Expected: FAIL de compilação — `atender` não existe e `passar_a_atender` tem outra aridade.

- [ ] **Step 3: Write minimal implementation**

`passar_a_atender` ganha o terceiro parâmetro e instala um verificador de
**certificado de cliente**, em vez de `with_no_client_auth()`:

```rust
/// Confere quem **chega**, contra a impressão que o servidor apresentou.
///
/// Espelho de [`ConfereImpressao`] na outra direção. As duas existem porque os
/// dois lados discam: quem disca confere com aquele, quem atende confere com
/// este, e sem os dois metade das ligações não passaria por conferência nenhuma.
#[derive(Debug)]
pub(crate) struct ConfereQuemChega {
    esperada: String,
}
```

Implemente `rustls::server::danger::ClientCertVerifier` para ele — os métodos
mecânicos (`verify_tls12_signature`, `verify_tls13_signature`,
`supported_verify_schemes`) saem do mesmo lugar que os de `ConfereImpressao`, e
`root_hint_subjects` devolve `&[]`, porque não há autoridade a sugerir: a
conferência é por impressão digital, não por cadeia. `client_auth_mandatory`
devolve `true` — um par que não apresente certificado nenhum tem de ser
recusado, não aceito.

Em `passar_a_atender`, troque `with_no_client_auth()` por
`with_client_cert_verifier(Arc::new(ConfereQuemChega { esperada: impressao_de_quem_vem }))`.

E a função que faltava:

```rust
/// Atende **uma** ligação de par, e devolve o que chegou.
///
/// Uma só, e não um laço: no A1 quem empresta serve um par por vez, e um laço
/// aqui prometeria a topologia que o subprojeto B ainda vai desenhar.
///
/// `None` quando a ponta fechou sem ninguém chegar.
pub async fn atender(ponta: quinn::Endpoint) -> Option<ParLigado> {
    let chegando = ponta.accept().await?;
    let conexao = chegando.await.ok()?;
    let ida_e_volta = conexao.rtt();
    let como = como_chegou(conexao.remote_address());
    tracing::info!(?como, ?ida_e_volta, "um par foi atendido");
    Some(ParLigado {
        conexao,
        como,
        ida_e_volta,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p seele-core`
Expected: PASS. Os testes da Task 4 que usavam `atender_em_segundo_plano` podem
agora usar `atender`; se o auxiliar ficar sem uso, **remova-o** — andaime que
sobrevive ao substituto é o defeito que a Task 3 já pagou uma vez.

- [ ] **Step 5: Prove the guards**

1. Volte `with_client_cert_verifier(...)` para `with_no_client_auth()`. Esperado:
   `quem_atende_recusa_quem_o_servidor_nao_apresentou` FALHA.
2. Faça `client_auth_mandatory` devolver `false`. Esperado: o mesmo teste FALHA
   (um par sem certificado nenhum passaria).
3. Faça `atender` devolver `None` sem chamar `accept()`. Esperado:
   `dois_pares_apresentados_se_ligam_pelos_dois_lados` FALHA.

Desfaça as três.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): quem atende confere quem chega, e alguém atende"
```

---

