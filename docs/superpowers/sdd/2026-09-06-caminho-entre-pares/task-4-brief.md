### Task 4: Os dois discam, e o resultado é enumerado

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `config_de_cliente`, `ErroDePar`, `passar_a_atender`
- Produces:
  - `pub enum ComoChegou { Local, Furo }`
  - `pub struct ParLigado { pub conexao: quinn::Connection, pub como: ComoChegou, pub ida_e_volta: std::time::Duration }`
  - `pub async fn ligar(ponta: &quinn::Endpoint, enderecos: &[std::net::SocketAddr], impressao: String, prazo: std::time::Duration) -> Result<ParLigado, ErroDePar>`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn dois_pares_se_ligam_e_o_teste_sabe_como() {
    // **Os dois discam, e o furo sai de graça.** Como as duas pontas atendem,
    // as próprias tentativas de conexão são os pacotes que abrem o NAT dos dois
    // lados; a primeira que fecha o aperto de mão vence e a outra é descartada.
    // Resolve o caso assimétrico sozinho — se só um lado consegue sair, é a
    // conexão dele que vinga — e usa só a API pública do `quinn`.
    //
    // Em `127.0.0.1` não há NAT a furar, então o que este teste prende é o
    // resto: que a ligação fecha, que a impressão digital é conferida no
    // caminho, e que o resultado diz **como** chegou. A taxa de furo de verdade
    // é o roteiro de duas máquinas que mede, e nenhum teste daqui pode medi-la.
    let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let ia = identidade_efemera().unwrap();
    let impressao_a = impressao(&ia);
    passar_a_atender(&a, ia).unwrap();
    let endereco_a = a.local_addr().unwrap();

    let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let ib = identidade_efemera().unwrap();
    passar_a_atender(&b, ib).unwrap();

    let ligado = ligar(
        &b,
        &[endereco_a],
        impressao_a,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("os dois pares não se ligaram");

    assert_eq!(ligado.como, ComoChegou::Local, "127.0.0.1 não é rede local?");
    assert!(ligado.conexao.close_reason().is_none(), "a conexão já morreu");
}

#[tokio::test(flavor = "multi_thread")]
async fn um_par_com_a_impressao_errada_nao_liga() {
    // O caso que separa «não consegui falar com ele» de «alguém respondeu no
    // lugar dele». Sem esta parede, qualquer um que alcance a porta alimenta
    // quadro de tela a quem estava esperando o par certo.
    let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    passar_a_atender(&a, identidade_efemera().unwrap()).unwrap();
    let endereco_a = a.local_addr().unwrap();

    let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    let erro = ligar(
        &b,
        &[endereco_a],
        "f".repeat(64),
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(erro, ErroDePar::ImpressaoNaoBate { .. }),
        "a recusa saiu com o motivo errado: {erro:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::dois_pares`
Expected: FAIL de compilação — `ligar` e `ComoChegou` não existem.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Como a ligação com um par foi conseguida.
///
/// **É metade da razão de o subprojeto A existir.** Toda a aritmética da malha
/// supõe que dois clientes domésticos se alcançam, e ninguém mediu isso. Se o
/// furo falhar em boa parte dos pares, a árvore do subprojeto B não pode supor
/// que qualquer par se alcança — e vira «árvore entre quem se alcança, estrela
/// para o resto», que é um desenho bem diferente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComoChegou {
    /// Mesma rede: nenhum furo foi necessário.
    Local,
    /// Endereço público: o furo deu certo.
    Furo,
}

/// Um par ligado, e o que a ligação ensinou.
pub struct ParLigado {
    /// A conexão viva.
    pub conexao: quinn::Connection,
    /// Como ela foi conseguida.
    pub como: ComoChegou,
    /// O ida e volta que o `quinn` está medindo nela. É o custo de um salto.
    pub ida_e_volta: std::time::Duration,
}

/// Disca para um par e devolve a primeira conexão que fechar.
///
/// Todos os endereços em paralelo, como o ADR 0037 faz para o servidor: uma
/// lista tentada em série multiplica o pior caso pelo número de candidatos, e o
/// pior caso é justamente o endereço que não responde.
///
/// # Errors
///
/// [`ErroDePar::ImpressaoNaoBate`] quando alguém respondeu e não era quem o
/// servidor apresentou; [`ErroDePar::NaoAlcancou`] quando ninguém respondeu no
/// prazo.
pub async fn ligar(
    ponta: &quinn::Endpoint,
    enderecos: &[std::net::SocketAddr],
    impressao_esperada: String,
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar> {
    let mut tentativas = tokio::task::JoinSet::new();
    for endereco in enderecos {
        let config = config_de_cliente(impressao_esperada.clone())?;
        let ponta = ponta.clone();
        let endereco = *endereco;
        tentativas.spawn(async move {
            let ligando = ponta
                .connect_with(config, endereco, "seele-par")
                .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
            let conexao = ligando.await.map_err(|erro| classificar(&erro))?;
            Ok::<_, ErroDePar>((conexao, como_chegou(endereco)))
        });
    }

    let mut ultimo = ErroDePar::NaoAlcancou;
    let ate = tokio::time::Instant::now() + prazo;
    while let Ok(Some(acabou)) = tokio::time::timeout_at(ate, tentativas.join_next()).await {
        match acabou {
            Ok(Ok((conexao, como))) => {
                let ida_e_volta = conexao.rtt();
                // A primeira que fecha vence; as outras são abandonadas, e
                // abandoná-las é o que fecha as conexões que sobraram.
                tentativas.abort_all();
                tracing::info!(?como, ?ida_e_volta, "um par ligou");
                return Ok(ParLigado {
                    conexao,
                    como,
                    ida_e_volta,
                });
            }
            // **A impressão que não bate ganha do silêncio.** Se um endereço
            // respondeu com o certificado errado e outro não respondeu, o que
            // se quer contar é o primeiro: ele é evento de segurança, e o
            // segundo é rotina.
            Ok(Err(erro @ ErroDePar::ImpressaoNaoBate { .. })) => return Err(erro),
            Ok(Err(erro)) => ultimo = erro,
            Err(erro) => ultimo = ErroDePar::Escuta(erro.to_string()),
        }
    }
    Err(ultimo)
}

/// Se este endereço é da mesma rede, e portanto não precisou de furo.
fn como_chegou(endereco: std::net::SocketAddr) -> ComoChegou {
    let local = match endereco.ip() {
        std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || (v6.segments().first().is_some_and(|s| s & 0xfe00 == 0xfc00))
        }
    };
    if local {
        ComoChegou::Local
    } else {
        ComoChegou::Furo
    }
}

/// Traduz uma falha de conexão para o motivo enumerado.
fn classificar(erro: &quinn::ConnectionError) -> ErroDePar {
    let texto = erro.to_string();
    if texto.contains("o par apresentou") {
        // A frase vem do `ErroDePar::ImpressaoNaoBate` que o verificador
        // devolveu ao `rustls`, e é o único jeito que o `quinn` tem de a
        // devolver: `rustls::Error::General` chega aqui como texto.
        return ErroDePar::ImpressaoNaoBate {
            esperada: String::new(),
            veio: String::new(),
        };
    }
    ErroDePar::NaoAlcancou
}
```

> **Nota para quem implementa:** a `classificar` acima é o caminho honesto e feio.
> Antes de aceitá-la, tente carregar o `ErroDePar` real do verificador para fora
> — por exemplo com um `Arc<Mutex<Option<ErroDePar>>>` guardado no
> `ConfereImpressao` e lido depois da falha. Se der certo, use isso e apague o
> `contains`; se não der, deixe a `classificar` **com este comentário explicando
> por que ela compara texto**, porque comparar texto de erro sem explicação é o
> tipo de coisa que ninguém entende em seis meses.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 6 testes.

- [ ] **Step 5: Prove the guards**

1. Faça `ligar` ignorar `impressao_esperada` (passe `String::new()` ao config). Esperado: `um_par_com_a_impressao_errada_nao_liga` FALHA.
2. Faça `como_chegou` devolver sempre `ComoChegou::Furo`. Esperado: `dois_pares_se_ligam_e_o_teste_sabe_como` FALHA.

Desfaça as duas.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): os dois discam, e o resultado diz como a ligação chegou"
```

---

