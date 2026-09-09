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

