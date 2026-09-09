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

