# O caminho entre pares — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Um cliente passa a poder receber uma transmissão de tela de **outro cliente** em vez do servidor, com identidade conferida e queda de volta para o servidor quando o par falha.

**Architecture:** A ponta QUIC que o cliente já tem aprende a atender (`Endpoint::set_server_config`), sem socket novo nem porta nova. O servidor — conectado aos dois e vendo o endereço público de cada um — apresenta um ao outro com a impressão digital do certificado. Os dois discam simultaneamente; as tentativas são o furo de NAT, e a primeira que fecha o aperto de mão vence.

**Tech Stack:** Rust 1.97, `quinn` 0.11.11, `rustls`, `rcgen`, `postcard`, `tokio`.

**Spec:** `docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`

## Global Constraints

- **Vocabulário (ADR 0033 / 0035).** **servidor**, **sala de voz**, **pessoa**, **PERSISTENCE**. Nunca `Dogma`, `Cage`, `Pilot`, `Linha`, `CASPER` em texto que descreve o sistema de hoje.
- **Idioma.** Código e comentários novos em português, como o resto de `seele-core` e `seele-server`. Doc de item público obrigatório (`missing_docs = "warn"`).
- **`unsafe_code = "forbid"`** no workspace inteiro.
- **`unwrap_used` e `expect_used` são `deny`** fora de teste. Em teste, o módulo abre com o `#[allow(...)]` com `reason` que os outros já usam.
- **`indexing_slicing = "warn"`**: entrada de rede nunca é indexada direto.
- **Motivo de erro é enumerado** (`specs/02-protocolo.md`), com os campos que a casca precisa para escrever a frase (ADR 0012). Nenhuma string livre chega à interface.
- **ADR 0002:** `seele-core` **não** depende de `seele-server`.
- **`PROTOCOL_VERSION` vai a 4**, `COMPATIBILITY_WINDOW` fica em 1.
- **Impressão digital** é sempre `String`: SHA-256 do DER em hex minúsculo, via `seele_proto::transport::certificate_fingerprint`.
- **Cada tarefa termina verde**: `cargo test -p <crate>`, `cargo fmt`, `cargo clippy --all-targets` sem aviso.
- **Todo guarda é provado revertendo o conserto** e vendo o teste falhar, antes do commit. Um teste que nunca foi visto vermelho não é prova.

---

### Task 1: O certificado efêmero de quem empresta

**Files:**
- Create: `crates/seele-core/src/par.rs`
- Modify: `crates/seele-core/src/lib.rs` (registrar `pub mod par;`)
- Modify: `crates/seele-core/Cargo.toml` (dependência `rcgen`)

**Interfaces:**
- Consumes: `seele_proto::transport::certificate_fingerprint(der: &[u8]) -> String`
- Produces:
  - `pub struct Identidade { pub cadeia: Vec<rustls::pki_types::CertificateDer<'static>>, pub chave: rustls::pki_types::PrivateKeyDer<'static> }`
  - `pub fn identidade_efemera() -> Result<Identidade, ErroDePar>`
  - `pub fn impressao(identidade: &Identidade) -> String`
  - `pub enum ErroDePar { Certificado(String), Escuta(String), NaoAlcancou, ImpressaoNaoBate { esperada: String, veio: String } }`

- [ ] **Step 1: Write the failing test**

Em `crates/seele-core/src/par.rs`, no fim do arquivo:

```rust
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    #[test]
    fn cada_identidade_e_nova_e_a_impressao_a_distingue() {
        // **Efêmero é o ponto, e não um detalhe.** O certificado do servidor é
        // persistido porque o pino do ADR 0003 depende dele. Este não é pinado
        // por ninguém: quem confere recebe a impressão digital pelo servidor a
        // cada apresentação. Guardá-lo em disco criaria um identificador
        // estável da máquina de quem empresta, que é metadado que ninguém pediu.
        let uma = identidade_efemera().unwrap();
        let outra = identidade_efemera().unwrap();
        assert_ne!(
            impressao(&uma),
            impressao(&outra),
            "duas identidades saíram com a mesma impressão: ela não distingue nada"
        );
    }

    #[test]
    fn a_impressao_e_a_do_certificado_e_no_formato_de_sempre() {
        // Um formato só para a mesma coisa. Se isto divergir de
        // `certificate_fingerprint`, o pino do `seele://` e a apresentação
        // entre pares passam a dizer o mesmo hash de dois jeitos, e um dia
        // discordam.
        let identidade = identidade_efemera().unwrap();
        let der = identidade.cadeia.first().unwrap();
        assert_eq!(
            impressao(&identidade),
            seele_proto::transport::certificate_fingerprint(der.as_ref())
        );
        assert_eq!(impressao(&identidade).len(), 64, "não é SHA-256 em hex");
        assert!(impressao(&identidade).chars().all(|c| c.is_ascii_hexdigit()));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::`
Expected: FAIL de compilação — `identidade_efemera` não existe. Adicione o cabeçalho do módulo e as assinaturas com `todo!()` só se precisar compilar; o vermelho que vale é o da asserção.

- [ ] **Step 3: Write minimal implementation**

No topo de `crates/seele-core/src/par.rs`:

```rust
//! O caminho entre pares: um cliente serve tela a outro.
//!
//! O `§5.1` do desenho de compartilhamento de tela recusou isto em 22/08 com
//! uma frase — *«custa um caminho que este produto nunca teve»*. Este módulo é
//! esse caminho, e a spec de 05/09 conta por que ele passou a valer a pena.
//!
//! # Por que o certificado não vem do `seele-server`
//!
//! O `tls.rs` de lá faz a mesma coisa, e o ADR 0002 proíbe o `seele-core` de
//! depender do daemon. Repetir as poucas linhas de `rcgen` é o que `crate::tela`
//! já decidiu para as constantes de enquadramento, e pela mesma razão:
//! *«quarenta linhas repetidas custam menos que um crate de transporte que os
//! dois dependeriam e nenhum seria dono»*. O que não pode divergir é o
//! **formato da impressão digital**, e ele não diverge porque os dois lados
//! chamam `seele_proto::transport::certificate_fingerprint`.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Um certificado e a chave que assina por ele.
pub struct Identidade {
    /// O certificado, em DER.
    pub cadeia: Vec<CertificateDer<'static>>,
    /// A chave privada, em DER.
    pub chave: PrivateKeyDer<'static>,
}

/// Por que o caminho entre pares não deu certo.
///
/// Enumerado, e cada variante distingue um conserto diferente — ver a tabela do
/// §4 da spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDePar {
    /// Não deu para gerar o certificado desta sessão.
    #[error("não deu para gerar o certificado deste par: {0}")]
    Certificado(String),
    /// Não deu para pôr a ponta a atender.
    #[error("não deu para pôr esta ponta a atender: {0}")]
    Escuta(String),
    /// Nenhum dos endereços fechou aperto de mão.
    #[error("nenhum endereço deste par respondeu")]
    NaoAlcancou,
    /// Alguém respondeu, e não era quem o servidor apresentou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`]**, e juntá-las apagaria a
    /// informação inteira: a diferença entre «não consegui falar com ele» e
    /// «alguém respondeu no lugar dele» é a que o ADR 0003 existe para nomear.
    #[error("o par apresentou {veio}, e o servidor tinha dito {esperada}")]
    ImpressaoNaoBate {
        /// O que o servidor apresentou.
        esperada: String,
        /// O que veio no aperto de mão.
        veio: String,
    },
}

/// Gera a identidade desta sessão. **Nunca vai para o disco.**
///
/// # Errors
///
/// Falha se o `rcgen` não gerar chave ou certificado.
pub fn identidade_efemera() -> Result<Identidade, ErroDePar> {
    // A forma exata que `tela.rs:2049` já usa neste crate, e por isso está
    // provada contra esta versão do `rcgen`. Não invente outra.
    let gerado = rcgen::generate_simple_self_signed(vec!["seele-par".to_owned()])
        .map_err(|erro| ErroDePar::Certificado(erro.to_string()))?;
    let cadeia = vec![CertificateDer::from(gerado.cert.der().to_vec())];
    let chave = rustls::pki_types::PrivatePkcs8KeyDer::from(gerado.signing_key.serialize_der());
    Ok(Identidade {
        cadeia,
        chave: chave.into(),
    })
}

/// A impressão digital que o servidor vai apresentar por esta identidade.
#[must_use]
pub fn impressao(identidade: &Identidade) -> String {
    identidade.cadeia.first().map_or_else(String::new, |cert| {
        seele_proto::transport::certificate_fingerprint(cert.as_ref())
    })
}
```

Em `crates/seele-core/src/lib.rs`, junto dos outros `pub mod`, em ordem alfabética:

```rust
pub mod par;
```

Em `crates/seele-core/Cargo.toml`: **`rcgen = "0.14.8"` já existe, em
`[dev-dependencies]`** — `tela.rs:2049` e `frame.rs:79` o usam em teste. **Mova a
linha** para `[dependencies]`, sem mudar a versão. Acrescentar uma segunda linha
deixaria a mesma dependência declarada duas vezes com liberdade de divergirem.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 2 testes.

- [ ] **Step 5: Prove the guard**

Troque `impressao` para devolver uma constante (`"x".repeat(64)`) e rode de novo. Esperado: `cada_identidade_e_nova_e_a_impressao_a_distingue` FALHA. Desfaça.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs crates/seele-core/src/lib.rs crates/seele-core/Cargo.toml
git commit -m "feat(par): a identidade efêmera de quem empresta a subida"
```

---

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

### Task 6: O protocolo — a v4 e as quatro mensagens

**Files:**
- Modify: `crates/seele-proto/src/control.rs:1210` (fim de `ClientMessage`) e `:1776` (fim de `ServerMessage`)
- Modify: `crates/seele-proto/src/version.rs:52`

**Interfaces:**
- Produces:
  - `ClientMessage::EmprestarSubida { emprestando: bool, impressao: String, locais: Vec<SocketAddr> }`
  - `ClientMessage::ParFalhou { screen: ScreenId, motivo: MotivoDeFalhaDePar }`
  - `ServerMessage::SirvaTelaPara { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
  - `ServerMessage::AssistaTelaPor { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
  - `pub enum MotivoDeFalhaDePar { NaoAlcancou, ImpressaoNaoBate, CaiuNoMeio, ParouDeMandar }`

- [ ] **Step 1: Write the failing test**

Em `crates/seele-proto/src/control.rs`, no `mod tests` que já existe:

```rust
#[test]
fn as_mensagens_do_caminho_entre_pares_atravessam_o_fio() {
    // Ida e volta pelo `postcard`, como as outras. O que este teste prende de
    // verdade é a **posição** das variantes: o `postcard` indexa variante por
    // posição, então acrescentar no meio troca o significado de todas as
    // seguintes para quem já está no ar.
    let emprestar = ClientMessage::EmprestarSubida {
        emprestando: true,
        impressao: "a".repeat(64),
        locais: vec!["192.168.1.7:41234".parse().unwrap()],
    };
    assert_eq!(
        postcard::from_bytes::<ClientMessage>(&postcard::to_allocvec(&emprestar).unwrap()).unwrap(),
        emprestar
    );

    let falhou = ClientMessage::ParFalhou {
        screen: ScreenId(7),
        motivo: MotivoDeFalhaDePar::ImpressaoNaoBate,
    };
    assert_eq!(
        postcard::from_bytes::<ClientMessage>(&postcard::to_allocvec(&falhou).unwrap()).unwrap(),
        falhou
    );

    let sirva = ServerMessage::SirvaTelaPara {
        screen: ScreenId(7),
        enderecos: vec!["203.0.113.9:8383".parse().unwrap()],
        impressao: "b".repeat(64),
    };
    assert_eq!(
        postcard::from_bytes::<ServerMessage>(&postcard::to_allocvec(&sirva).unwrap()).unwrap(),
        sirva
    );

    let assista = ServerMessage::AssistaTelaPor {
        screen: ScreenId(7),
        enderecos: vec!["203.0.113.9:8383".parse().unwrap()],
        impressao: "b".repeat(64),
    };
    assert_eq!(
        postcard::from_bytes::<ServerMessage>(&postcard::to_allocvec(&assista).unwrap()).unwrap(),
        assista
    );
}
```

Em `crates/seele-proto/src/version.rs`, no `mod tests`:

```rust
#[test]
fn a_versao_subiu_para_a_do_caminho_entre_pares() {
    // A v3 **já saiu** no release `v0.10.5-1` (commit `12a6401a6`), então as
    // quatro mensagens novas não pegam carona como as do ADR 0036 pegaram na
    // v2. Um cliente v3 conecta pela janela, nunca recebe as mensagens novas, e
    // é servido pelo servidor — que é o comportamento de antes desta onda.
    assert_eq!(PROTOCOL_VERSION, 4);
    assert_eq!(oldest_supported_version(), 3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-proto`
Expected: FAIL — variantes não existem, e `PROTOCOL_VERSION` é 3.

- [ ] **Step 3: Write minimal implementation**

Em `control.rs`, **no fim** de `ClientMessage` (depois de `UnwatchScreen`, antes do `}` da linha 1210):

```rust
    /// «Eu empresto a minha subida», ou «deixei de emprestar».
    ///
    /// **Opt-in, e por duas razões independentes.** A primeira é privacidade:
    /// numa malha, quem assiste passa a conhecer o endereço de quem lhe
    /// repassa, e hoje ninguém conhece endereço de ninguém — um servidor não é
    /// necessariamente entre amigos, e o ADR 0021 deixa a admissão poder ser
    /// aberta. A segunda é custo: a máquina de quem empresta passa a subir
    /// cópias para outras pessoas, e ninguém deve gastar a internet de alguém
    /// sem perguntar.
    EmprestarSubida {
        /// Se empresta a partir de agora.
        emprestando: bool,
        /// SHA-256 do certificado desta sessão, em hex minúsculo. O mesmo
        /// formato do `fp=` do `seele://` e do pino do ADR 0003 — um formato só
        /// para a mesma coisa.
        impressao: String,
        /// Endereços de **rede local** por onde este par atende.
        ///
        /// O público não vem daqui: ele é a origem da conexão que já está
        /// aberta, e o servidor o tem sem perguntar. Um endereço público que o
        /// cliente afirma seria um endereço que ele pode mentir.
        locais: Vec<std::net::SocketAddr>,
    },
    /// O par que estava servindo esta transmissão não serve mais.
    ///
    /// **Mandada por quem recebe, e nunca por quem empresta:** quem sabe que a
    /// imagem parou é quem estava esperando por ela, e quem empresta pode ter
    /// caído sem chegar a saber de nada.
    ParFalhou {
        /// Qual transmissão.
        screen: ScreenId,
        /// O que aconteceu.
        motivo: MotivoDeFalhaDePar,
    },
```

Antes de `pub enum ClientMessage`, o enumerado:

```rust
/// Por que um par deixou de servir uma transmissão.
///
/// Cada variante distingue um conserto diferente, e é por isso que são quatro e
/// não uma. `ImpressaoNaoBate` **não** é `NaoAlcancou`: a diferença entre «não
/// consegui falar com ele» e «alguém respondeu no lugar dele» é a informação
/// inteira, e é a mesma distinção que o ADR 0003 existe para nomear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoDeFalhaDePar {
    /// Nenhum dos endereços fechou aperto de mão.
    NaoAlcancou,
    /// Alguém respondeu, e não era quem o servidor apresentou.
    ImpressaoNaoBate,
    /// Estava servindo, e a conexão morreu.
    CaiuNoMeio,
    /// Conexão viva, e quadro nenhum dentro do prazo.
    ParouDeMandar,
}
```

Em `ServerMessage`, **no fim** (depois de `PersonRenamed`):

```rust
    /// Sirva esta transmissão a este par.
    ///
    /// Quem recebe isto disca para os endereços **e** passa a atender: as duas
    /// tentativas simultâneas são o que abre o NAT dos dois lados, e a primeira
    /// que fecha o aperto de mão vence.
    SirvaTelaPara {
        /// Qual transmissão.
        screen: ScreenId,
        /// Onde o outro par pode ser alcançado.
        enderecos: Vec<std::net::SocketAddr>,
        /// A impressão digital que o outro par vai apresentar.
        impressao: String,
    },
    /// Assista a esta transmissão por este par, em vez de esperar por mim.
    ///
    /// Simétrica de [`Self::SirvaTelaPara`] de propósito: os dois lados fazem a
    /// mesma coisa com ela — discar e conferir a impressão digital —, e a
    /// assimetria fica só em quem já tem os bytes.
    AssistaTelaPor {
        /// Qual transmissão.
        screen: ScreenId,
        /// Onde o par pode ser alcançado.
        enderecos: Vec<std::net::SocketAddr>,
        /// A impressão digital que ele vai apresentar.
        impressao: String,
    },
```

Em `version.rs:52`:

```rust
pub const PROTOCOL_VERSION: u8 = 4;
```

E **atualize o doc dessa constante** dizendo o que a v4 trouxe, como as versões anteriores fazem.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-proto && cargo test --workspace`
Expected: PASS. Se algum teste de outro crate afirmar `PROTOCOL_VERSION == 3`, corrija-o — mas **leia** cada um antes: se ele afirma compatibilidade, a correção pode ser outra.

- [ ] **Step 5: Prove the guard**

Volte `PROTOCOL_VERSION` para 3. Esperado: `a_versao_subiu_para_a_do_caminho_entre_pares` FALHA. Desfaça.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets
git add crates/seele-proto/src/control.rs crates/seele-proto/src/version.rs
git commit -m "feat(proto): a v4 leva as quatro mensagens do caminho entre pares"
```

---

### Task 7: O servidor guarda quem empresta, e escolhe (burramente) quem serve quem

**Files:**
- Create: `crates/seele-server/src/pares.rs`
- Modify: `crates/seele-server/src/lib.rs` (registrar `mod pares;`)
- Modify: `crates/seele-server/src/server.rs` (campo `pares` no `Server`)
- Modify: `crates/seele-server/src/session.rs:2191` (região dos braços de tela)

**Interfaces:**
- Consumes: `ClientMessage::EmprestarSubida`, `ServerMessage::{SirvaTelaPara, AssistaTelaPor}` (Task 6)
- Produces:
  - `pub struct QuemEmpresta { pub pessoa: PersonId, pub impressao: String, pub enderecos: Vec<SocketAddr> }`
  - `pub struct Pares { ... }` com `pub fn declarou(&mut self, pessoa, impressao, locais, publico)`, `pub fn saiu(&mut self, pessoa)`, `pub fn escolher(&self, dono: PersonId, quem_quer: PersonId, ja_servindo: &HashSet<PersonId>) -> Option<QuemEmpresta>`

- [ ] **Step 1: Write the failing test**

Em `crates/seele-server/src/pares.rs`:

```rust
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    fn endereco(n: u8) -> SocketAddr {
        SocketAddr::from(([192, 168, 1, n], 8383))
    }

    #[test]
    fn quem_compartilha_nunca_e_escolhido_para_servir_a_si_mesmo() {
        // O espelho infinito, na versão da malha: quem compartilha servindo a
        // própria tela a si mesmo. `crate::voice_room` já prende isto para o
        // caminho do servidor; aqui é a mesma regra no caminho novo.
        let mut pares = Pares::nova();
        pares.declarou(PersonId(1), "a".repeat(64), vec![endereco(1)], endereco(1));
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .is_none());
    }

    #[test]
    fn quem_nao_declarou_nunca_e_escolhido() {
        // O opt-in é a decisão de 05/09, e ela tem de ser respeitada aqui e não
        // só na interface: uma escolha que ignora o `emprestando` gastaria a
        // internet de alguém que disse não.
        let mut pares = Pares::nova();
        pares.declarou(PersonId(3), "c".repeat(64), vec![endereco(3)], endereco(3));
        pares.declarou(PersonId(3), String::new(), Vec::new(), endereco(3));
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .is_none());
    }

    #[test]
    fn quem_ja_esta_servindo_nao_e_escolhido_de_novo() {
        // **Um par por vez, no A1.** Quantos um cliente aguenta é a conta do
        // subprojeto B, e supor «dois» aqui seria inventar um número que
        // ninguém mediu.
        let mut pares = Pares::nova();
        pares.declarou(PersonId(3), "c".repeat(64), vec![endereco(3)], endereco(3));
        let ja = HashSet::from([PersonId(3)]);
        assert!(pares.escolher(PersonId(1), PersonId(2), &ja).is_none());
    }

    #[test]
    fn o_endereco_publico_vem_do_servidor_e_nao_do_cliente() {
        // Um endereço público que o cliente afirma é um endereço que ele pode
        // mentir — e mentir aqui manda outra pessoa discar para onde o mentiroso
        // quiser. O servidor vê a origem da conexão; é ela que vale.
        let mut pares = Pares::nova();
        let publico = SocketAddr::from(([203, 0, 113, 9], 8383));
        pares.declarou(PersonId(3), "c".repeat(64), vec![endereco(3)], publico);
        let escolhido = pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .unwrap();
        assert!(escolhido.enderecos.contains(&publico));
        assert!(escolhido.enderecos.contains(&endereco(3)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-server --lib pares::`
Expected: FAIL de compilação — `Pares` não existe.

- [ ] **Step 3: Write minimal implementation**

```rust
//! Quem declarou que empresta a subida, e quem serve quem.
//!
//! # A escolha aqui é deliberadamente burra
//!
//! Ela aponta o primeiro que declarou, não é quem compartilha, e ainda não
//! serve ninguém. É um espaço reservado com a forma certa: o **subprojeto B** é
//! quem olha subida medida e topologia para escolher bem. Chamar isto de
//! «escolha automática» seria vender como pronto o que é um lugar guardado — e
//! a spec de 05/09 diz isso com todas as letras.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use seele_proto::ids::PersonId;

/// Alguém que declarou que empresta, e como alcançá-lo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuemEmpresta {
    /// Quem.
    pub pessoa: PersonId,
    /// A impressão digital que ele vai apresentar.
    pub impressao: String,
    /// Onde ele atende: os locais que declarou, mais o público que o servidor
    /// **viu**. Nesta ordem, porque a rede local dispensa furo e é a que
    /// responde mais rápido — a mesma razão do ADR 0037.
    pub enderecos: Vec<SocketAddr>,
}

/// Quem empresta a subida nesta sala, agora.
#[derive(Debug, Default)]
pub struct Pares {
    quem: HashMap<PersonId, QuemEmpresta>,
}

impl Pares {
    /// Ninguém emprestando ainda.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Alguém declarou que empresta — ou que deixou de emprestar.
    ///
    /// `publico` é a origem da conexão desta pessoa, vista pelo servidor. Uma
    /// declaração com `impressao` vazia é «deixei de emprestar», e é assim que
    /// `emprestando: false` chega aqui.
    pub fn declarou(
        &mut self,
        pessoa: PersonId,
        impressao: String,
        locais: Vec<SocketAddr>,
        publico: SocketAddr,
    ) {
        if impressao.is_empty() {
            self.quem.remove(&pessoa);
            return;
        }
        let mut enderecos = locais;
        if !enderecos.contains(&publico) {
            enderecos.push(publico);
        }
        self.quem.insert(
            pessoa,
            QuemEmpresta {
                pessoa,
                impressao,
                enderecos,
            },
        );
    }

    /// Esta pessoa saiu. Sem isto, a escolha aponta para quem já foi embora.
    pub fn saiu(&mut self, pessoa: PersonId) {
        self.quem.remove(&pessoa);
    }

    /// Quem pode servir esta transmissão a esta pessoa, se alguém.
    #[must_use]
    pub fn escolher(
        &self,
        dono: PersonId,
        quem_quer: PersonId,
        ja_servindo: &HashSet<PersonId>,
    ) -> Option<QuemEmpresta> {
        self.quem
            .values()
            .find(|candidato| {
                candidato.pessoa != dono
                    && candidato.pessoa != quem_quer
                    && !ja_servindo.contains(&candidato.pessoa)
            })
            .cloned()
    }
}
```

Ligue no `Server` (`server.rs`, ao lado de `pub telas`) e no despacho de `session.rs`, na região dos braços de tela (`:2191` em diante), tratando `ClientMessage::EmprestarSubida` com `connection.remote_address()` como `publico`, e `ClientMessage::ParFalhou` chamando `pares.saiu(...)` quando o motivo for `ImpressaoNaoBate` e nada mais nos outros casos.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-server --lib pares::`
Expected: PASS, 4 testes.

- [ ] **Step 5: Prove the guards**

1. Tire a condição `candidato.pessoa != dono`. Esperado: `quem_compartilha_nunca_e_escolhido...` FALHA.
2. Tire o `if impressao.is_empty()`. Esperado: `quem_nao_declarou_nunca_e_escolhido` FALHA.
3. Tire o `push(publico)`. Esperado: `o_endereco_publico_vem_do_servidor...` FALHA.

Desfaça as três.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p seele-server && cargo clippy -p seele-server --all-targets
git add crates/seele-server/src/pares.rs crates/seele-server/src/lib.rs crates/seele-server/src/server.rs crates/seele-server/src/session.rs
git commit -m "feat(pares): o servidor guarda quem empresta e aponta quem serve quem"
```

---

### Task 8: O cliente pede ao par, e cai para o servidor quando falha

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (braço novo perto de `:2441`, onde `HostUplink` já é tratado)
- Modify: `crates/seele-core/src/par.rs`

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

### Task 9: Quem empresta repassa os bytes, por pedaço e a partir de um quadro-chave

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `ParLigado` (Task 4); `seele_proto::stream::StreamType::Screen`; `seele_proto::screen::ScreenHeader`
- Produces: `pub async fn repassar(ligado: &ParLigado, abertura: &[u8], pedacos: tokio::sync::mpsc::Receiver<Vec<u8>>) -> Result<(), ErroDePar>`

**Por que esta tarefa existe, e por que ela quase não existiu.** As sete
anteriores fazem dois clientes se conectarem. **Nenhuma faz um quadro
atravessar.** É o trabalho todo, e é fácil de perder de vista justamente porque
o §3.4 da spec diz que o lado que recebe já é agnóstico ao transporte — o que é
verdade e não é o suficiente: alguém ainda tem de escrever os bytes do outro
lado.

- [ ] **Step 1: Write the failing test**

Em `par.rs`, em `mod testes`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn o_par_recebe_a_abertura_e_os_pedacos_na_ordem() {
    // **Por pedaço, e nunca remontando quadro.** É a mesma regra que o
    // encaminhamento do servidor segue — *«o encaminhamento é por pedaço, sem
    // remontar quadro»* —, e ela não é estilo: um fluxo QUIC é uma sequência
    // ordenada de bytes, e esperar o quadro inteiro para repassar acrescenta
    // um tempo de quadro de atraso a cada salto. Numa árvore de profundidade
    // três isso seria três quadros, que é o orçamento inteiro.
    let (a, b, ligado) = duas_pontas_ligadas().await;

    let abertura = vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN];
    let (manda, recebe) = tokio::sync::mpsc::channel(8);
    let repassando = tokio::spawn(async move { repassar(&ligado, &abertura_clone, recebe).await });

    manda.send(vec![1, 2, 3]).await.unwrap();
    manda.send(vec![4, 5]).await.unwrap();
    drop(manda);

    // Do outro lado, o que chega é: o byte de tipo, a abertura, e os pedaços
    // na ordem em que foram mandados.
    let mut fluxo = a.accept_uni().await.unwrap();
    let mut tipo = [0_u8; 1];
    fluxo.read_exact(&mut tipo).await.unwrap();
    assert_eq!(tipo[0], seele_proto::stream::StreamType::Screen.byte());
    let mut veio = Vec::new();
    fluxo.read_to_end(&mut veio).await.unwrap();
    assert_eq!(
        veio,
        [vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN], vec![1, 2, 3], vec![4, 5]].concat(),
        "o que chegou ao par não é a abertura seguida dos pedaços na ordem"
    );
    repassando.await.unwrap().unwrap();
    drop(b);
}
```

> **Nota para quem implementa:** `duas_pontas_ligadas()` é um auxiliar a extrair
> do teste `dois_pares_se_ligam_e_o_teste_sabe_como` da Task 4 — ele já monta
> exatamente isso. Extraia-o **antes** de escrever este teste, num commit à
> parte se preferir; duplicar aquele bloco seria dois lugares para consertar
> quando a assinatura de `ligar` mudar. E `abertura_clone` é a cópia que o
> `move` do `spawn` obriga.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::o_par_recebe_a_abertura`
Expected: FAIL de compilação — `repassar` não existe.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Escreve uma transmissão de tela para um par, por pedaço.
///
/// # Por pedaço, e nunca remontando quadro
///
/// A mesma regra do encaminhamento do servidor, pela mesma razão: um fluxo QUIC
/// é uma sequência ordenada de bytes, e esperar o quadro inteiro para repassar
/// acrescenta um tempo de quadro de atraso **a cada salto**.
///
/// # A abertura vai primeiro, e o pedaço nenhum antes dela
///
/// Quem recebe usa `TelaRecebida::do_fluxo`, que lê o byte de tipo e o
/// `ScreenHeader` antes de qualquer coisa. Um pedaço que chegasse antes deslocaria
/// o enquadramento **para sempre** — é o mesmo defeito que o §5.2 do desenho de
/// compartilhamento de tela nomeia, e ele não se corrige depois.
///
/// # Errors
///
/// [`ErroDePar::Escuta`] quando o par para de aceitar bytes.
pub async fn repassar(
    ligado: &ParLigado,
    abertura: &[u8],
    mut pedacos: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> Result<(), ErroDePar> {
    use tokio::io::AsyncWriteExt as _;

    let mut fluxo = ligado
        .conexao
        .open_uni()
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    fluxo
        .write_all(abertura)
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    while let Some(pedaco) = pedacos.recv().await {
        fluxo
            .write_all(&pedaco)
            .await
            .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    }
    fluxo
        .finish()
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 8 testes.

- [ ] **Step 5: Write the second failing test — o quadro-chave**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn quem_entra_por_um_par_entra_num_quadro_chave() {
    // **Ligar no meio de um quadro é ligar em lixo.** O servidor já sabe disto:
    // ele segura quem chega numa lista de espera e só abre a cópia no começo do
    // próximo quadro-chave. Quem repassa tem de fazer o mesmo, ou a pessoa que
    // entrou por um par vê blocos coloridos até o quadro-chave seguinte — e não
    // tem como saber por quê.
    //
    // O que se prende aqui é que `repassar` **descarta** o que vem antes do
    // primeiro pedaço marcado como chave, e nada depois dele.
    todo!(
        "montar dois pedaços — um comum e um chave — e afirmar que só o segundo \
         em diante atravessou. A marca de chave é o `TipoDeQuadro::Chave` do \
         `crate::tela`; confira como `EmCurso::esperando` faz do lado do servidor \
         e siga a mesma regra."
    )
}
```

> **Nota:** este `todo!()` é deliberado e é o terceiro e último do plano. A forma
> exata depende de como `crate::tela` expõe a marca de quadro-chave, e inventá-la
> aqui produziria um teste que não compila contra o código real. O **contrato**
> está escrito: descarta antes da primeira chave, nada depois dela.

- [ ] **Step 6: Run, implement, verify**

Escreva o teste de verdade, veja-o falhar, e acrescente a `repassar` o parâmetro
que diz se um pedaço é chave — descartando tudo até a primeira. Rode até verde.

- [ ] **Step 7: Prove the guards**

1. Escreva a abertura **depois** do primeiro pedaço. Esperado: `o_par_recebe_a_abertura_e_os_pedacos_na_ordem` FALHA.
2. Tire o descarte até a primeira chave. Esperado: `quem_entra_por_um_par_entra_num_quadro_chave` FALHA.

Desfaça as duas.

- [ ] **Step 8: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): quem empresta repassa por pedaço, a partir de um quadro-chave"
```

### Task 10: A costura, num servidor de verdade com dois clientes

**Files:**
- Create: `crates/seele-server/tests/tela_por_um_par.rs`

**Interfaces:**
- Consumes: tudo das Tasks 1–9.

- [ ] **Step 1: Write the failing test**

Copie o andaime de `crates/seele-server/tests/subida_no_arranque.rs` — `AceitaQualquer`, `servidor_com`, `abrir`, `Par` — e escreva:

```rust
/// O quadro chega ao segundo cliente **pelo primeiro**, e o servidor não o subiu.
///
/// # A prova que importa é a negativa
///
/// Que o quadro chegou, um teste ingênuo prova sem querer: o servidor sabe
/// servir tela desde agosto, e um caminho novo que não funcione é indistinguível
/// de um que funcione se ninguém olhar de onde os bytes vieram. Então a
/// asserção é dupla: o quadro bate byte a byte **e** o contador do servidor diz
/// que aquela cópia não saiu dele. É a mesma forma de prova que
/// `subida_no_arranque.rs` usa.
#[tokio::test(flavor = "multi_thread")]
async fn o_quadro_chega_pelo_par_e_o_servidor_nao_o_subiu() -> Result<()> {
    // 1. servidor, três clientes: quem compartilha, quem empresta, quem assiste
    // 2. quem empresta manda `EmprestarSubida { emprestando: true, ... }`
    // 3. quem compartilha abre a tela; quem assiste pede para ver
    // 4. o servidor manda `SirvaTelaPara` a um e `AssistaTelaPor` ao outro
    // 5. o quadro que chega a quem assiste bate byte a byte com o que saiu
    // 6. e o contador de cópias do servidor para aquela transmissão é **menor**
    //    do que seria sem o par
    todo!("escrever segundo o roteiro acima, com o andaime de subida_no_arranque.rs")
}

/// O par morre no meio, e quem estava atrás dele continua vendo.
#[tokio::test(flavor = "multi_thread")]
async fn quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem() -> Result<()> {
    // 1. o mesmo cenário acima, até o quadro chegar pelo par
    // 2. a conexão de quem empresta é fechada à força
    // 3. quem assiste manda `ParFalhou { motivo: CaiuNoMeio }`
    // 4. e o quadro **seguinte** chega — pelo servidor
    todo!("escrever segundo o roteiro acima")
}
```

> **Nota para quem implementa:** os dois `todo!()` acima são as **únicas** ocorrências
> permitidas neste plano, e são deliberadas: o corpo depende das assinaturas que
> as Tasks 7, 8 e 9 fixarem, e escrevê-lo aqui de antemão seria inventar nomes que
> ainda não existem. O roteiro numerado é o contrato; escreva o corpo contra as
> assinaturas reais quando chegar aqui, **antes** de qualquer implementação nesta
> tarefa.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-server --test tela_por_um_par`
Expected: FAIL — `todo!()` entra em pânico, e depois falha nas asserções reais.

- [ ] **Step 3: Write the real test bodies, then whatever glue they demand**

Escreva os corpos. O que faltar de ligação nas Tasks 7, 8 e 9 aparece aqui — é para isso que este teste vem depois delas e não antes.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p seele-server --test tela_por_um_par`
Expected: PASS, 2 testes.

- [ ] **Step 5: Prove the guards**

1. Faça `Pares::escolher` devolver sempre `None`. Esperado: `o_quadro_chega_pelo_par...` FALHA (o servidor sobe a cópia).
2. Faça `por_onde` nunca cair para o servidor. Esperado: `quando_o_par_morre...` FALHA.

Desfaça as duas.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add crates/seele-server/tests/tela_por_um_par.rs
git commit -m "test(par): o quadro chega pelo par, e o servidor assume quando ele morre"
```

---

### Task 11: O roteiro de duas máquinas mede o que nenhum teste daqui mede

**Files:**
- Modify: `docs/teste-duas-maquinas.md`

**Interfaces:** nenhuma. É documento.

- [ ] **Step 1: Write the section**

Acrescente ao fim de `docs/teste-duas-maquinas.md`:

```markdown
## O caminho entre pares (subprojeto A da malha)

**Isto é o que nenhum teste automático deste repositório consegue produzir.** Um
furo de NAT entre dois roteadores domésticos não acontece em `127.0.0.1`, e os
dois números abaixo são a razão de o subprojeto A existir antes do B.

Precisa de **três** máquinas, ou de duas mais um celular em 4G — o ponto é que
quem empresta e quem assiste **não** estejam na mesma rede.

1. Numa máquina, suba o servidor e compartilhe a tela.
2. Noutra rede, entre duas pessoas. Numa delas, ligue o empréstimo de subida.
3. Na terceira, peça para assistir.

Anote, do `tracing` de quem assistiu:

| o que | onde ler | anote |
|---|---|---|
| como a ligação chegou | `um par ligou`, campo `como` | `Local` ou `Furo` |
| ida e volta com o par | `um par ligou`, campo `ida_e_volta` | ms |
| quando não ligou | `o par não veio`, campo `erro` | o motivo enumerado |

Repita **umas dez vezes**, em redes diferentes se der. O que se quer é a
**fração** de `Furo` sobre tentativas, e ela é o número que decide o desenho da
árvore: se o furo falhar em boa parte dos pares, o subprojeto B não pode supor
que qualquer par se alcança, e vira «árvore entre quem se alcança, estrela para
o resto».

Escreva os dois números em `docs/m1-medicoes.md`, ao lado dos outros. **Enquanto
eles não existirem, a aritmética da malha na spec continua sendo estimativa**, e
está marcada como tal.
```

- [ ] **Step 2: Verify**

Run: `cargo test --workspace`
Expected: PASS — nenhum guarda cobre documento, e é por isso que este passo existe: confirmar que nada quebrou junto.

- [ ] **Step 3: Commit**

```bash
git add docs/teste-duas-maquinas.md
git commit -m "docs(teste): o roteiro mede o furo entre pares e o custo de um salto"
```

---

## Depois deste plano

O subprojeto A termina com **o caminho existindo e dois números medidos**. Nada
de árvore, nada de escolha boa, nada de interface.

O **subprojeto B** só deve ser desenhado depois de a Task 11 ter sido executada
por uma pessoa, em máquinas de verdade. Se a fração de furo vier baixa, o
desenho da árvore muda — e essa é a razão inteira de a ordem ser esta.
