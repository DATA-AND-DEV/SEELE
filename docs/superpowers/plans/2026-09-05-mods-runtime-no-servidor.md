# MODs — o runtime no servidor: liberdade total, medida e isolada

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Um MOD habilitado roda JavaScript dentro do servidor, com acesso ao domínio inteiro — ler o que a sala tem, reagir a tudo o que acontece nela, e chamar todo verbo que o servidor sabe fazer — sem que um MOD ruim derrube a sala.

**Architecture:** QuickJS embutido no `seele-server`, um contexto por MOD, com teto de memória e interrupção por tempo. A superfície que o MOD enxerga é um **arquivo congelado** (`api/v1.json`) mapeado para o interior, e um build vermelho cobra que todo nome do arquivo ainda aponte para alguma coisa — renomear por dentro fica livre, e o conserto é o mapeamento, nunca os MODs.

**Tech Stack:** `rquickjs` 0.12.2 (MIT), medido em `spikes/mod-em-js/`.

**Spec:** `docs/superpowers/specs/2026-09-05-mods-design.md` · ADR [0044](../../adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md)

**Plano anterior:** `2026-09-05-mods-nucleo-local.md` (concluído). **Spike:** `spikes/mod-em-js/README.md` (concluído).

## O recorte, e a decisão que o define

Pedido do dono, nas palavras dele: **«total liberdade desde a v1.»**

Isso é sobre **capacidade** e não sobre nomes. Um MOD alcança tudo o que o
servidor alcança; o que continua congelado é como as coisas **se chamam**, que
foi a resposta à objeção do próprio dono sobre renomear variável quebrar MOD.

Levantado do código, e é o que a v1 tem de cobrir:

| | quantos | de onde |
|---|---|---|
| verbos que o servidor executa | **29** | `ClientMessage` |
| momentos em que algo acontece | **32** | `ServerMessage` |
| permissões | **13** | `Permission` |

Mais o que não está no protocolo e a liberdade total inclui: **o quintal de
dados** do MOD, **rede de saída** (o MOD de túnel do dono depende disso) e
**tempo** (relógio e agendamento).

## Quando congelar passa a doer, e por que isso não bloqueia este plano

O ADR 0044 diz que uma versão de API nunca é editada depois de publicada, e é
verdade. **Mas «publicada» quer dizer «existe MOD de terceiro no mundo», e o
indexador é o plano 6.** Até lá, `api/v1.json` ainda se mexe: o guarda de
append-only nasce aqui e o teste que o prende ao mundo nasce com o indexador.

Isto é o que torna «liberdade total na v1» construível sem adivinhação: a
superfície cresce ao longo deste plano e dos seguintes, e fecha antes de
publicar — não antes de existir.

## Global Constraints

Os mesmos do plano anterior, e três a mais:

- **`unsafe_code = "forbid"`** vale para os nossos crates. `rquickjs` usa
  `unsafe` por dentro; é dependência, não código nosso, e não pede exceção. O
  ADR 0041 é a exceção **nomeada** e continua sendo a única.
- **`seele-server` só pode depender de `seele-proto`** (`cargo xtask check-deps`).
  `rquickjs` é terceiro e não muda a regra.
- **O orçamento é 1 vCPU / 512 MB**, e os números do spike são o piso contra o
  qual qualquer regressão é medida: **+1 098 KiB** de binário, **39 ns** por
  chamada, **5,9 MB** de RSS com 50 contextos.
- **Código de MOD nunca toca o caminho de áudio nem o de vídeo.** É fronteira de
  desenho e vale como critério de revisão: se uma tarefa precisar disso, o
  desenho está errado.

## Estrutura de arquivos

| arquivo | responsabilidade |
|---|---|
| `crates/seele-server/Cargo.toml` | modificar — `rquickjs` |
| `crates/seele-server/src/mods/mod.rs` | **criar** — o hospedeiro: um contexto por MOD, os tetos, o despacho |
| `crates/seele-server/src/mods/api.rs` | **criar** — a fachada: ler o `api/v1.json` e ligar cada nome ao interior |
| `crates/seele-server/src/mods/quintal.rs` | **criar** — chave→valor sobre `mod_data`, com teto |
| `crates/seele-server/src/mods/isolamento.rs` | **criar** — o que acontece quando um MOD lança |
| `api/v1.json` | **criar** — a superfície congelada |
| `xtask/src/check_api.rs` | **criar** — o build vermelho contra deriva |
| `xtask/src/main.rs` | modificar — o comando novo |

O hospedeiro é um diretório e não um arquivo porque são quatro
responsabilidades que mudam por motivos diferentes, e porque um arquivo que
decide o que código de terceiro alcança é a última coisa que deve crescer sem
que ninguém repare.

---

### Task 1: QuickJS entra, com os dois tetos, e o contexto sobrevive

**Files:**
- Modify: `crates/seele-server/Cargo.toml`
- Create: `crates/seele-server/src/mods/mod.rs`
- Modify: `crates/seele-server/src/lib.rs`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub struct Anfitriao { … }` com `pub fn novo() -> Result<Self>`
  - `pub fn carregar(&mut self, id: &str, fonte: &str) -> Result<(), Falha>`
  - `pub fn chamar(&mut self, id: &str, momento: &str, carga: &str) -> Result<(), Falha>`
  - `pub enum Falha { NaoCarregou, Lancou, EstourouMemoria, PassouDoTempo }`

- [ ] **Step 1: Declarar a dependência**

Em `crates/seele-server/Cargo.toml`, em `[dependencies]`, em ordem alfabética:

```toml
# O interpretador dos MODs (ADR 0044). QuickJS e não Boa, e não por gosto:
# `spikes/mod-em-js/` mediu 11× menos binário, ~2× menos custo por chamada e
# ~4× menos memória com 50 contextos. MIT, então nada muda no `deny.toml`.
#
# Custo que ele traz: `rquickjs-sys` compila fonte em C, então construir este
# crate passa a exigir um compilador de C. Existe nos três alvos.
rquickjs = "0.12.2"
```

- [ ] **Step 2: Escrever os testes que falham**

Criar `crates/seele-server/src/mods/mod.rs` com só este bloco:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_mod_que_carrega_responde_a_um_momento() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/exemplo",
                "globalThis.aoAcontecer = (momento) => { globalThis.ultimo = momento; };",
            )
            .expect("carregar");
        anfitriao
            .chamar("seele/exemplo", "PersonJoined", "{}")
            .expect("chamar");
    }

    /// O teto de memória do ADR 0044, e a metade que importa: **a sala
    /// continua**. Um MOD que aloca sem parar é cortado, e o contexto dele
    /// segue respondendo — medido em `spikes/mod-em-js/src/bin/tetos.rs`.
    #[test]
    fn um_mod_que_aloca_sem_parar_e_cortado_e_o_contexto_sobrevive() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/glutao",
                "globalThis.aoAcontecer = () => { const a = []; for (;;) a.push(new Array(1024)); };",
            )
            .expect("carregar");

        let falha = anfitriao.chamar("seele/glutao", "PersonJoined", "{}");
        assert!(matches!(falha, Err(Falha::EstourouMemoria) | Err(Falha::Lancou)));

        // E o contexto ainda responde.
        anfitriao
            .carregar("seele/glutao", "globalThis.aoAcontecer = () => {};")
            .expect("recarregar depois do estouro");
        anfitriao
            .chamar("seele/glutao", "PersonJoined", "{}")
            .expect("o contexto morreu junto com o estouro");
    }

    /// Um laço infinito num MOD não pode ser a diferença entre a sala funcionar
    /// e não funcionar. É o orçamento de 1 vCPU que cobra isto.
    #[test]
    fn um_laco_infinito_e_interrompido() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar("seele/eterno", "globalThis.aoAcontecer = () => { while (true) {} };")
            .expect("carregar");

        let inicio = std::time::Instant::now();
        let falha = anfitriao.chamar("seele/eterno", "PersonJoined", "{}");
        assert!(matches!(falha, Err(Falha::PassouDoTempo) | Err(Falha::Lancou)));
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(5),
            "o laço infinito segurou a sala por {:?}",
            inicio.elapsed()
        );
    }

    /// Dois MODs não se enxergam. Um que escreve em `globalThis` não aparece no
    /// outro — senão dois MODs de autores diferentes brigariam por um nome.
    #[test]
    fn dois_mods_nao_dividem_o_mesmo_globalThis() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar("seele/um", "globalThis.marca = 1; globalThis.aoAcontecer = () => {};")
            .expect("um");
        anfitriao
            .carregar(
                "seele/dois",
                "globalThis.aoAcontecer = () => { if (globalThis.marca) throw new Error('vazou'); };",
            )
            .expect("dois");
        anfitriao
            .chamar("seele/dois", "PersonJoined", "{}")
            .expect("o global de um MOD vazou para o outro");
    }

    /// Um MOD com erro de sintaxe é recusado ao carregar, e não na primeira vez
    /// que alguém entra na sala.
    #[test]
    fn um_mod_que_nao_compila_e_recusado_ao_carregar() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        assert!(matches!(
            anfitriao.carregar("seele/quebrado", "isto ( não é javascript"),
            Err(Falha::NaoCarregou)
        ));
    }

    /// Um MOD que não declara `aoAcontecer` não é um erro: é um MOD que só tem
    /// metade de cliente e cujo `servidor/` existe para outra coisa.
    #[test]
    fn um_mod_sem_ao_acontecer_nao_e_falha() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar("seele/mudo", "globalThis.nada = 1;")
            .expect("carregar");
        anfitriao
            .chamar("seele/mudo", "PersonJoined", "{}")
            .expect("um MOD calado virou falha");
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p seele-server mods::tests`
Expected: FAIL na compilação — `cannot find type 'Anfitriao'`.

- [ ] **Step 4: Escrever o hospedeiro**

No topo de `crates/seele-server/src/mods/mod.rs`:

```rust
//! Where a MOD's server half runs.
//!
//! ADR 0044. A MOD is third-party code with real access, by decision — the
//! product's rules protect the product and do not reach a MOD. What this module
//! does is not restrict it: it is to keep one bad MOD from taking the room down
//! with it.
//!
//! # The two ceilings, and why they are not politeness
//!
//! The server is an SFU budgeted at 1 vCPU / 512 MB — `xtask/src/check_deps.rs`
//! forbids two dependency edges over that number. A MOD in an infinite loop
//! cannot be the difference between the room working and not, so:
//!
//! - **memory**, per runtime, refused rather than swapped;
//! - **time**, by interrupt handler, counted in interpreter steps.
//!
//! Both measured in `spikes/mod-em-js/`, together with the property that makes
//! them useful: **the context survives either one**. A MOD that blows up is
//! disabled and the room continues.
//!
//! # One runtime per MOD, and not one shared
//!
//! Two MODs sharing a `globalThis` is two authors fighting over a name, and one
//! MOD reading what another wrote. Separate runtimes also mean the memory
//! ceiling is per MOD rather than per server, which is the number a host can
//! reason about.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use rquickjs::{Context, Function, Runtime};

/// Memory one MOD may hold, in bytes.
///
/// 8 MiB against a 512 MB server: sixty MODs at the ceiling would still leave
/// the SFU its half. The spike measured 5,9 MB of RSS for **fifty** contexts
/// doing nothing, so this is room to work in and not a straitjacket.
const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;

/// Interpreter steps one call may spend before it is interrupted.
///
/// Steps and not milliseconds because that is what the interpreter counts, and
/// because a number of steps behaves the same on a fast machine and a slow one
/// — a wall-clock ceiling would cut a slow host's MOD and spare a fast one's.
/// **Corrigido na execução, e o erro era de fator 5 000.** O contador não conta
/// operações: conta quantas vezes o motor resolve perguntar «continua?».
/// Medido: 1 consulta ≈ 5 000 operações ≈ 0,45 ms. O 200 000 que estava aqui
/// valeria **noventa segundos** de um MOD segurando um SFU de 1 vCPU. Ver a doc
/// do módulo para a tabela.
const TETO_DE_CONSULTAS: usize = 500;

/// Why a MOD did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Falha {
    /// The source did not compile.
    #[error("mod source did not compile")]
    NaoCarregou,
    /// It threw.
    #[error("mod threw")]
    Lancou,
    /// It went past the memory ceiling.
    #[error("mod went past its memory ceiling")]
    EstourouMemoria,
    /// It went past the step ceiling.
    #[error("mod went past its step ceiling")]
    PassouDoTempo,
}

/// One MOD's runtime, and the step counter its interrupt handler reads.
struct Hospede {
    contexto: Context,
    passos: Arc<AtomicUsize>,
    // Held so the runtime outlives its context.
    _runtime: Runtime,
}

/// Every MOD's server half, on this server.
#[derive(Default)]
pub struct Anfitriao {
    hospedes: BTreeMap<String, Hospede>,
}

impl Anfitriao {
    /// An empty host.
    ///
    /// # Errors
    ///
    /// Never today. It returns `Result` because loading does, and a caller that
    /// has to branch in one place and not the other invites the branch being
    /// forgotten.
    pub fn novo() -> Result<Self> {
        Ok(Self::default())
    }

    /// Compiles a MOD's server half into a runtime of its own.
    ///
    /// Loading the same identifier twice replaces it, which is what makes a MOD
    /// updatable without restarting the server.
    ///
    /// # Errors
    ///
    /// [`Falha::NaoCarregou`] when the source does not compile.
    pub fn carregar(&mut self, id: &str, fonte: &str) -> Result<(), Falha> {
        let runtime = Runtime::new().map_err(|_| Falha::NaoCarregou)?;
        runtime.set_memory_limit(TETO_DE_MEMORIA);

        let passos = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&passos);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            contador.fetch_add(1, Ordering::Relaxed) > TETO_DE_PASSOS
        })));

        let contexto = Context::full(&runtime).map_err(|_| Falha::NaoCarregou)?;
        contexto
            .with(|ctx| ctx.eval::<(), _>(fonte))
            .map_err(|_| Falha::NaoCarregou)?;

        self.hospedes.insert(
            id.to_owned(),
            Hospede {
                contexto,
                passos,
                _runtime: runtime,
            },
        );
        Ok(())
    }

    /// Hands a MOD one moment.
    ///
    /// A MOD with no `aoAcontecer` is not a failure: it is a MOD whose server
    /// half exists for something else, and calling it is a no-op.
    ///
    /// # Errors
    ///
    /// [`Falha`] for a MOD that threw or went past a ceiling. A MOD that is not
    /// loaded is also `Lancou` — deliberately the same answer, because a caller
    /// that has to tell them apart is a caller inventing a recovery for a state
    /// it cannot fix.
    pub fn chamar(&mut self, id: &str, momento: &str, carga: &str) -> Result<(), Falha> {
        let hospede = self.hospedes.get(id).ok_or(Falha::Lancou)?;
        // Each call gets the whole budget: a MOD that was slow once is not a
        // MOD that is broken forever.
        hospede.passos.store(0, Ordering::Relaxed);

        hospede.contexto.with(|ctx| {
            let Ok(f) = ctx.globals().get::<_, Function>("aoAcontecer") else {
                return Ok(());
            };
            f.call::<_, ()>((momento, carga)).map_err(|erro| {
                if hospede.passos.load(Ordering::Relaxed) > TETO_DE_PASSOS {
                    Falha::PassouDoTempo
                } else if matches!(erro, rquickjs::Error::Allocation) {
                    Falha::EstourouMemoria
                } else {
                    Falha::Lancou
                }
            })
        })
    }
}
```

Em `crates/seele-server/src/lib.rs`, junto das outras declarações, em ordem
alfabética:

```rust
pub mod mods;
```

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test -p seele-server mods::tests`
Expected: PASS, seis testes.

**A variante do estouro é `Allocation`** — conferida em `rquickjs-core-0.12.2/src/result.rs`, e não `OutOfMemory`, que foi o nome que eu supus ao escrever isto. Se ela mudar de nome numa versão futura, não invente:
rode `cargo doc -p rquickjs --open` e leia as variantes de `Error` da versão que
o `Cargo.lock` travou. O que não muda é o desenho — o estouro tem variante
própria, e cair para `Lancou` seria perder a única informação que distingue um
MOD glutão de um MOD com defeito.

- [ ] **Step 6: Provar os dois tetos contra a regressão**

O `CLAUDE.md` é explícito, e este é o lugar em que ele mais importa: um teto que
ninguém provou é um comentário.

1. Comente `runtime.set_memory_limit(TETO_DE_MEMORIA);`.
2. Run: `cargo test -p seele-server mods::tests::um_mod_que_aloca_sem_parar`
3. Expected: **o teste não termina**, ou termina com o processo morto pelo
   sistema. Isso é a prova; interrompa e restaure.
4. Comente o `set_interrupt_handler`.
5. Run: `cargo test -p seele-server mods::tests::um_laco_infinito`
6. Expected: **não termina**. Interrompa e restaure.
7. Restaure os dois e rode a suíte inteira: PASS.

- [ ] **Step 7: Medir contra o piso do spike**

Run:
```sh
cargo build --release -p seele-server
ls -l target/release/seeled
```
Compare com o binário antes desta tarefa. O spike mediu **+1 098 KiB**; um
crescimento muito acima disso quer dizer que entrou mais coisa que o
interpretador, e é para ser investigado e não aceito.

- [ ] **Step 8: Commit**

```bash
git add crates/seele-server/Cargo.toml crates/seele-server/src/mods/ crates/seele-server/src/lib.rs Cargo.lock
git commit -m "feat(mods): o servidor roda JavaScript de terceiro, com dois tetos e um contexto por MOD

QuickJS, medido em spikes/mod-em-js contra Boa: 11x menos binario, ~2x menos
custo por chamada, ~4x menos memoria com 50 contextos.

Um runtime por MOD, e nao um compartilhado: dois MODs dividindo globalThis sao
dois autores brigando por um nome e um lendo o que o outro escreveu. Tambem faz
o teto de memoria ser por MOD, que e o numero que quem hospeda consegue pensar.

Os dois tetos foram provados por reversao: sem eles, o teste do glutao e o do
laco infinito nao terminam."
```

---

### Task 2: `api/v1.json` — a superfície declarada

**Files:**
- Create: `api/v1.json`
- Create: `api/README.md`

**Interfaces:**
- Produces: o arquivo que a Task 3 lê e o `xtask` confere.

- [ ] **Step 1: Escrever o arquivo**

`api/v1.json`. Os nomes são **portugueses** porque são o vocabulário do
glossário — `servidor`, `sala de voz`, `canal de texto`, `pessoa` — e o MOD
manipula as coisas do SEELE com os nomes do SEELE. As **chaves de estrutura**
são inglesas (ADR 0013): o arquivo é lido por ferramenta.

```json
{
  "version": 1,
  "reads": {
    "servidor.nome": "ServerConfig::name",
    "servidor.pessoas": "persistence::people",
    "servidor.salas": "persistence::voice_rooms",
    "servidor.canais": "persistence::channels",
    "servidor.papeis": "persistence::roles",
    "pessoa.id": "PersonId",
    "pessoa.apelido": "Person::nick",
    "pessoa.papel": "Person::role",
    "pessoa.permissoes": "Permissions::of",
    "sala.id": "VoiceRoomId",
    "sala.nome": "VoiceRoom::name",
    "sala.pessoas": "VoiceRoom::occupants",
    "canal.id": "ChannelId",
    "canal.nome": "Channel::name",
    "canal.historico": "persistence::messages::history",
    "mensagem.id": "MessageId",
    "mensagem.autor": "Message::author",
    "mensagem.texto": "Message::body",
    "mensagem.quando": "Message::created_at"
  },
  "moments": [
    "PersonJoined", "PersonLeft", "PersonPresent", "PersonGone",
    "PersonRenamed", "PersonIconChanged",
    "MessageReceived", "MessageEdited", "MessageRemoved",
    "VoiceRoomCreated", "VoiceRoomRenamed", "VoiceRoomDeleted",
    "MovedToVoiceRoom",
    "ChannelCreated", "ChannelRenamed", "ChannelDeleted", "ChannelWeighed",
    "ScreenShareStarted", "ScreenShareStopped",
    "ServerRenamed", "ServerIconChanged"
  ],
  "actions": {
    "mandarMensagem": "ClientMessage::SendMessage",
    "removerMensagem": "ClientMessage::RemoveMessage",
    "criarSala": "ClientMessage::CreateVoiceRoom",
    "renomearSala": "ClientMessage::RenameVoiceRoom",
    "apagarSala": "ClientMessage::DeleteVoiceRoom",
    "criarCanal": "ClientMessage::CreateChannel",
    "renomearCanal": "ClientMessage::RenameChannel",
    "apagarCanal": "ClientMessage::DeleteChannel",
    "moverPessoa": "ClientMessage::MovePerson",
    "expulsarPessoa": "ClientMessage::KickPerson",
    "banirPessoa": "ClientMessage::BanPerson",
    "renomearServidor": "ClientMessage::RenameServer",
    "definirIconeDoServidor": "ClientMessage::SetServerIcon"
  },
  "own": {
    "guardar": "mods::quintal::set",
    "ler": "mods::quintal::get",
    "esquecer": "mods::quintal::remove",
    "listar": "mods::quintal::keys"
  },
  "world": {
    "buscar": "mods::mundo::fetch",
    "agora": "mods::mundo::now",
    "registrar": "mods::mundo::log"
  }
}
```

- [ ] **Step 2: Escrever o porquê ao lado**

`api/README.md`:

```markdown
# A superfície que um MOD enxerga

Um arquivo por versão, e **nenhum deles é editado depois de publicado**.

## Por que congelado

O ADR 0044 decidiu que a API de MOD é uma **fachada** e não uma projeção do
protocolo. A diferença é a que importa quando alguém renomeia um campo:

- projeção: o nome muda por dentro, a API muda junto, **todo MOD quebra**;
- fachada: o nome muda por dentro, o build fica vermelho, e o conserto é o
  **mapeamento** — o nome que o MOD escreve nunca mudou.

`cargo xtask check-api` é quem cobra isso, e a pergunta que ele faz é «todo nome
deste arquivo ainda aponta para alguma coisa?». Nunca o contrário.

É a mesma disciplina das migrações do servidor — *«Append only once shipped»*
mais `no_migration_contains_a_down_step` —, aplicada a um segundo lugar onde
ela vale pelo mesmo motivo.

## Quando congelar passa a doer

**Quando existe MOD de terceiro no mundo**, e não antes. Enquanto o indexador
não publica, este arquivo ainda se mexe. Depois, não: editar uma versão
publicada é quebrar o MOD de todo mundo, que é a razão pela qual esta porta não
fecha.

## Os quatro blocos

- **`reads`** — o que um MOD lê do domínio.
- **`moments`** — quando ele é chamado. São eventos do `ServerMessage`.
- **`actions`** — o que ele manda o servidor fazer. São verbos do `ClientMessage`,
  e passam pelas **mesmas permissões** que a janela atravessa: não há caminho
  paralelo, então não há semântica paralela para divergir.
- **`own`** e **`world`** — o quintal de dados do MOD, e o que não está no
  protocolo: rede, relógio e log. É onde a «liberdade total» do ADR 0044 mora, e
  é a parte que a tela de aceite tem de dizer em voz alta.
```

- [ ] **Step 3: Commit**

```bash
git add api/
git commit -m "feat(mods): a superficie que um MOD enxerga, escrita antes de existir quem a leia

Pedido do dono: total liberdade desde a v1. Levantado do codigo em vez de
inventado — 29 verbos do ClientMessage, 32 momentos do ServerMessage, 13
permissoes — e o que nao esta no protocolo entra em own e world.

Congelar so passa a doer quando existe MOD de terceiro no mundo, e o indexador
e o plano 6. Ate la este arquivo ainda se mexe, e e isso que torna liberdade
total na v1 construivel sem adivinhacao."
```

---

### Task 3: O build vermelho contra a deriva

**Files:**
- Create: `xtask/src/check_api.rs`
- Modify: `xtask/src/main.rs`

**Interfaces:**
- Consumes: `api/v1.json`.
- Produces: `cargo xtask check-api`.

- [ ] **Step 1: Escrever o teste que falha**

Criar `xtask/src/check_api.rs` com só o bloco de teste:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_nome_que_aponta_para_algo_que_existe_passa() {
        let fonte = "pub struct Person { pub nick: String }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }

    /// O guarda inteiro, numa frase: renomear por dentro fica vermelho **aqui**,
    /// e não na máquina de quem escreveu um MOD seis meses atrás.
    #[test]
    fn um_nome_que_aponta_para_o_que_sumiu_reprova() {
        let fonte = "pub struct Person { pub apelido: String }";
        let violacoes = evaluate(&[("pessoa.apelido", "Person::nick")], fonte);
        assert_eq!(violacoes.len(), 1);
        assert!(violacoes[0].contains("Person::nick"));
        assert!(
            violacoes[0].contains("pessoa.apelido"),
            "a violação não diz qual nome de MOD ficou órfão"
        );
    }

    /// A direção importa e é a correção que o dono achou: acrescentar coisa ao
    /// protocolo **não** reprova. Se reprovasse, todo tipo novo empurraria a
    /// API para a frente e a fachada viraria projeção outra vez.
    #[test]
    fn um_tipo_novo_no_interior_que_a_api_nao_menciona_nao_reprova() {
        let fonte = "pub struct Person { pub nick: String, pub retrato: Vec<u8> }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p xtask check_api`
Expected: FAIL — `cannot find function 'evaluate'`.

- [ ] **Step 3: Implementar**

No topo de `xtask/src/check_api.rs`:

```rust
//! Enforces the MOD API façade from `api/`.
//!
//! ADR 0044. The question this asks, and the direction is the whole point:
//!
//! > Does every name in `api/vN.json` still point at something?
//!
//! And never the reverse. A check that asked "did every type reach the API?"
//! would drag the API along behind the protocol, and renaming a field inside
//! would repaint the façade and break every published MOD — which is exactly
//! the defect this file exists to prevent.
//!
//! Same argument as `check_deps`: "a contract checked only by review is a
//! contract that erodes".

use std::process::ExitCode;

/// One MOD-facing name that no longer resolves.
type Violation = String;

/// Pure rule evaluation, kept free of the filesystem so it can be tested.
///
/// `fonte` is every Rust source concatenated; the check is textual on purpose.
/// A real resolver would need the compiler, and what this has to catch — a
/// rename — changes the text.
fn evaluate(mapa: &[(&str, &str)], fonte: &str) -> Vec<Violation> {
    let mut violacoes = Vec::new();
    for (nome, interior) in mapa {
        let alvo = interior.rsplit("::").next().unwrap_or(interior);
        if !fonte.contains(alvo) {
            violacoes.push(format!(
                "`{nome}` aponta para `{interior}`, e `{alvo}` não existe mais. \
                 Conserte o mapeamento em `api/`, nunca o nome que o MOD escreve."
            ));
        }
    }
    violacoes
}

/// Reads `api/*.json` and every crate source, and reports orphans.
pub fn run() -> ExitCode {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    let mut fonte = String::new();
    for crate_dir in ["crates", "apps"] {
        colher(&raiz.join(crate_dir), &mut fonte);
    }

    let mut houve = false;
    let Ok(entradas) = std::fs::read_dir(raiz.join("api")) else {
        eprintln!("check-api: não achei `api/`");
        return ExitCode::FAILURE;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&texto) else {
            eprintln!("check-api: {} não é json", caminho.display());
            return ExitCode::FAILURE;
        };

        let mut mapa: Vec<(String, String)> = Vec::new();
        for bloco in ["reads", "actions", "own", "world"] {
            if let Some(obj) = json.get(bloco).and_then(serde_json::Value::as_object) {
                for (nome, interior) in obj {
                    if let Some(interior) = interior.as_str() {
                        mapa.push((nome.clone(), interior.to_owned()));
                    }
                }
            }
        }
        let emprestado: Vec<(&str, &str)> = mapa
            .iter()
            .map(|(n, i)| (n.as_str(), i.as_str()))
            .collect();
        for violacao in evaluate(&emprestado, &fonte) {
            eprintln!("check-api: {} — {violacao}", caminho.display());
            houve = true;
        }
    }

    if houve {
        ExitCode::FAILURE
    } else {
        println!("check-api: toda a superfície de MOD ainda aponta para algo.");
        ExitCode::SUCCESS
    }
}

/// Concatenates every `.rs` under `dir`.
fn colher(dir: &std::path::Path, destino: &mut String) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            if caminho.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            colher(&caminho, destino);
        } else if caminho.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(texto) = std::fs::read_to_string(&caminho) {
                destino.push_str(&texto);
                destino.push('\n');
            }
        }
    }
}
```

Em `xtask/src/main.rs`:

```rust
mod check_api;
```

e no `match`, antes do braço `Some(other)`:

```rust
        Some("check-api") => check_api::run(),
```

e no `usage()`:

```rust
    eprintln!("  check-api    enforce the MOD API façade from `api/` (ADR 0044)");
```

O `xtask` precisa de `serde_json` no `Cargo.toml` dele, na mesma versão dos
outros: `serde_json = "1.0.151"`.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p xtask check_api && cargo xtask check-api`
Expected: PASS nos dois, e a linha «toda a superfície de MOD ainda aponta para algo».

- [ ] **Step 5: Provar o guarda contra a regressão**

1. Em `api/v1.json`, troque `"Person::nick"` por `"Person::apelidoQueNaoExiste"`.
2. Run: `cargo xtask check-api`
3. Expected: **FAILURE**, nomeando `pessoa.apelido` e o alvo órfão.
4. Restaure e rode de novo: SUCCESS.

- [ ] **Step 6: Commit**

```bash
git add xtask/
git commit -m "feat(mods): renomear por dentro fica vermelho aqui, e nunca na maquina de quem escreveu um MOD

A pergunta e «todo nome da API ainda aponta para alguma coisa?», e nunca o
contrario — que foi a correcao que o dono achou no desenho. Um guarda na outra
direcao arrastaria a API atras do protocolo, e renomear um campo repintaria a
fachada e quebraria todo MOD publicado.

Mesmo argumento do check_deps: um contrato conferido so por revisao e um
contrato que se corroi."
```

---

## O que este plano deliberadamente não faz

- **Os `reads`, `actions`, `own` e `world` estão declarados e ainda não ligados.**
  O `api/v1.json` é o contrato; ligar cada nome ao interior é o plano 4, tarefa
  por bloco, e cada uma entrega MOD que faz mais coisa.
- **Nada atravessa o fio.** Habilitar continua local.
- **Não há tela de aceite**, e é ela que tem de dizer em voz alta o que `world`
  significa — rede de saída na máquina de quem hospeda.
- **O MOD ainda não escreve no banco.** O quintal é o plano 4.

## Ao fim deste plano

- [ ] Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && cargo xtask check-deps && cargo xtask check-api`
- [ ] Conferir o tamanho do `seeled` contra o piso do spike: **+1 098 KiB**.
