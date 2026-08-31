# Bitrate adaptativo — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construir o bitrate adaptativo que `specs/03-audio.md` especifica desde a primeira redação e que nunca existiu — três faixas comandadas por perda de subida medida no servidor a partir de lacunas de `seq`.

**Architecture:** O servidor conta lacunas de `seq` por `ssrc` dentro da `VoiceRoom` — que já decodifica o cabeçalho para conferir o `ssrc` — e devolve a fração perdida **só** para quem a produziu, num quadro de protocolo novo. No cliente, um controlador puro com histerese e permanência escolhe uma de três faixas e escreve em `Controls::bitrate`, que o laço de voz já lê a cada volta. Nada decodifica payload em lugar nenhum.

**Tech Stack:** Rust 1.97, postcard (serialização), quinn (QUIC), `shiguredo_opus` (codec).

**Spec:** `docs/superpowers/specs/2026-08-30-bitrate-adaptativo-design.md`
**ADR:** `docs/adr/0036-bitrate-adaptativo-em-faixas.md`

## Global Constraints

- **ADR 0002 — regra de dependência.** `seele-ffi` → `seele-core` → `seele-audio` / `seele-proto`. Nunca ao contrário. `seele-core` **não** pode ser dependência de `seele-server`.
- **`specs/08-seguranca.md` — o servidor nunca decodifica payload.** Toda medida deste plano vive do cabeçalho de mídia, que é claro. Nenhuma tarefa aqui lê um byte de payload.
- **`specs/10-convencoes.md` — sem `unwrap()` nem `expect()`** fora de testes e invariantes provadas. O workspace aplica `unwrap_used = "deny"` e `expect_used = "deny"`; testes relaxam com `#![cfg_attr(test, allow(...))]` ou `#![allow(...)]` no topo do arquivo de teste.
- **`missing_docs = "warn"`, `unreachable_pub = "warn"`, `indexing_slicing = "warn"`, `unsafe_code = "forbid"`.** Todo item público precisa de doc.
- **Idioma.** Código e documentação novos em português quando o módulo ao redor está em português (`voice_room.rs`, `tela.rs`, `taxa.rs`), em inglês quando o módulo ao redor está em inglês (`codec.rs`, `device.rs`). Ver ADR 0023 e `specs/10-convencoes.md`.
- **Valores fixados pela spec:** faixas 48 000 / 32 000 / 16 000 bps; desce acima de 5% de perda; sobe abaixo de 2%; permanência de 10 s; janela de medida de 5 s.
- **Migrações de banco:** nenhuma. Este plano não toca em persistência.
- **Cada tarefa termina verde:** `cargo fmt --check`, `cargo clippy --workspace --all-targets` sem aviso, e a suíte da tarefa passando.

---

## Estrutura de arquivos

| Arquivo | Responsabilidade |
|---|---|
| `crates/seele-audio/src/bitrate.rs` (**novo**) | O controlador puro: faixas, histerese, permanência. Sem I/O, relógio por parâmetro. |
| `crates/seele-audio/src/codec.rs` (modificar) | Reconciliar as constantes com a spec e corrigir a atribuição falsa ao ADR 0010. |
| `crates/seele-audio/src/lib.rs` (modificar) | Declarar o módulo `bitrate`. |
| `crates/seele-server/src/perda_de_subida.rs` (**novo**) | O estimador puro: lacunas de `seq` em janela deslizante. Sem I/O. |
| `crates/seele-server/src/lib.rs` (modificar) | Declarar o módulo `perda_de_subida`. |
| `crates/seele-server/src/server.rs` (modificar) | Acrescentar `Event::UplinkLoss` ao barramento. |
| `crates/seele-proto/src/version.rs` (modificar) | `PROTOCOL_VERSION` de 1 para 2. |
| `crates/seele-proto/src/control.rs` (modificar) | `ServerMessage::UplinkLoss`, acrescentado **no fim** do enum. |
| `crates/seele-server/src/voice_room.rs` (modificar) | Um estimador por `Member`; alimentar em `forward`; emitir o evento. |
| `crates/seele-server/src/session.rs` (modificar) | `Session` guarda a versão negociada; `translate` filtra por destinatário; só manda o quadro a cliente v2. |
| `crates/seele-core/src/state.rs` (modificar) | `Room::apply` dobra o quadro novo. |
| `crates/seele-core/src/voice.rs` (modificar) | `Voice` hospeda o controlador e expõe `observar_perda`. |
| `crates/seele-ffi/src/lib.rs` (modificar) | `fold` liga o quadro ao `Voice`. |

---

### Task 1: O controlador de faixas

**Files:**
- Create: `crates/seele-audio/src/bitrate.rs`
- Modify: `crates/seele-audio/src/lib.rs`
- Test: dentro de `crates/seele-audio/src/bitrate.rs`, em `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub const FAIXAS_BPS: [u32; 3]` = `[48_000, 32_000, 16_000]`
  - `pub struct Limiares { pub descer_acima_de: f32, pub subir_abaixo_de: f32, pub permanencia: Duration }`, com `impl Default`
  - `pub struct Controlador`
  - `pub fn Controlador::novo() -> Self`
  - `pub fn Controlador::com_limiares(limiares: Limiares) -> Self`
  - `pub fn Controlador::bitrate_bps(&self) -> u32`
  - `pub fn Controlador::observar(&mut self, perda: f32, agora: Instant) -> Option<u32>` — devolve `Some(bps)` **só quando a faixa muda**

- [ ] **Step 1: Escrever o arquivo com os testes que reprovam**

Criar `crates/seele-audio/src/bitrate.rs` com **apenas** o módulo de testes abaixo mais as declarações mínimas que não compilam ainda. O objetivo deste passo é ter testes escritos antes da implementação.

```rust
//! O bitrate segue a perda de subida, em três faixas.
//!
//! `specs/03-audio.md` fecha o parâmetro em «16–48 kbps, adaptativo», e detalha:
//! «cai para 16 kbps sob perda > 5%, sobe de volta gradualmente». Este módulo é
//! essa frase, e nada além dela.
//!
//! # Por que faixas, e não uma curva
//!
//! Porque trocar de bitrate **reconstrói o encoder**: o `shiguredo_opus` não
//! expõe setter em tempo de execução (ADR 0008, conferido no fonte do binding), e
//! a reconstrução custa um quadro sem histórico de predição. O ADR 0010 chamou
//! uma malha contínua de inaceitável por isso, e tinha razão.
//!
//! Três faixas, com histerese e permanência, tornam a troca rara — um punhado por
//! chamada, e nenhuma numa chamada cujo regime de rede não muda. A objeção não é
//! contornada: ela é a restrição que desenha este módulo. Ver o ADR 0036.
//!
//! # Por que o relógio é parâmetro
//!
//! Porque a permanência é a metade da malha que mais erra, e um teste que
//! provasse «não sobe antes de dez segundos» com um `sleep` de dez segundos não
//! seria rodado por ninguém.

use std::time::{Duration, Instant};

/// As faixas, da melhor para a pior.
///
/// Os extremos são os de `specs/03-audio.md`. O ponto do meio existe para que a
/// queda sob perda moderada não vá direto ao piso — cair ao fundo por 6% de
/// perda gastaria qualidade que o enlace ainda comportava.
pub const FAIXAS_BPS: [u32; 3] = [48_000, 32_000, 16_000];

/// Quando descer, quando subir, e quanto esperar antes de subir.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limiares {
    /// Acima disto, desce uma faixa na medida seguinte.
    pub descer_acima_de: f32,
    /// Abaixo disto, começa a contar a permanência para subir.
    pub subir_abaixo_de: f32,
    /// Quanto tempo a perda tem de ficar boa antes de uma subida.
    pub permanencia: Duration,
}

impl Default for Limiares {
    fn default() -> Self {
        Self {
            // `specs/03-audio.md`, textual.
            descer_acima_de: 0.05,
            // Três pontos de histerese. Larga o bastante para que ruído de
            // medida não atravesse os dois limiares na mesma chamada, que é o
            // que produziria troca de faixa sem mudança de regime.
            subir_abaixo_de: 0.02,
            permanencia: Duration::from_secs(10),
        }
    }
}

/// Escolhe a faixa a partir de uma sequência de medidas de perda.
#[derive(Debug)]
pub struct Controlador {
    limiares: Limiares,
    /// Índice em [`FAIXAS_BPS`]. Zero é a melhor.
    indice: usize,
    /// Desde quando a perda está abaixo do limiar de subida, sem interrupção.
    bom_desde: Option<Instant>,
}

impl Controlador {
    /// Um controlador na faixa de cima, com os limiares da spec.
    #[must_use]
    pub fn novo() -> Self {
        Self::com_limiares(Limiares::default())
    }

    /// Um controlador na faixa de cima, com limiares escolhidos.
    #[must_use]
    pub fn com_limiares(limiares: Limiares) -> Self {
        Self {
            limiares,
            // Começa no teto e desce sob evidência. É o que «adaptativo» quer
            // dizer, e é o que responde ao pedido de qualidade máxima sem
            // inventar um número que a spec não tenha.
            indice: 0,
            bom_desde: None,
        }
    }

    /// A faixa em vigor.
    #[must_use]
    pub fn bitrate_bps(&self) -> u32 {
        FAIXAS_BPS.get(self.indice).copied().unwrap_or(FAIXAS_BPS[0])
    }

    /// Dobra uma medida de perda, e diz se a faixa mudou.
    ///
    /// `Some(bps)` **só** quando houve troca: quem chama reconstrói o encoder com
    /// isso, e devolver o valor atual a cada medida faria a reconstrução
    /// acontecer cinquenta vezes por segundo — exatamente o que este desenho
    /// existe para não fazer.
    pub fn observar(&mut self, perda: f32, agora: Instant) -> Option<u32> {
        if perda > self.limiares.descer_acima_de {
            // Descer é imediato: quem está perdendo pacote já está sendo ouvido
            // mal, e esperar para confirmar é esperar em cima do problema.
            self.bom_desde = None;
            if self.indice + 1 < FAIXAS_BPS.len() {
                self.indice += 1;
                return Some(self.bitrate_bps());
            }
            return None;
        }

        if perda >= self.limiares.subir_abaixo_de {
            // A zona morta entre os dois limiares. Nem desce nem conta tempo para
            // subir — e zerar a contagem aqui é o que impede uma medida
            // beirando o limiar de acumular permanência aos pedaços.
            self.bom_desde = None;
            return None;
        }

        match self.bom_desde {
            None => {
                self.bom_desde = Some(agora);
                None
            }
            Some(desde) => {
                if agora.duration_since(desde) < self.limiares.permanencia {
                    return None;
                }
                // A contagem recomeça a cada subida: subir duas faixas exige
                // duas permanências inteiras, que é o «gradualmente» da spec.
                self.bom_desde = Some(agora);
                if self.indice == 0 {
                    return None;
                }
                self.indice -= 1;
                Some(self.bitrate_bps())
            }
        }
    }
}
```

- [ ] **Step 2: Acrescentar o módulo de testes ao mesmo arquivo**

```rust
#[cfg(test)]
mod tests {
    use super::{Controlador, Limiares, FAIXAS_BPS};
    use std::time::{Duration, Instant};

    /// Limiares apertados, para os testes não medirem o relógio de verdade.
    fn limiares() -> Limiares {
        Limiares {
            descer_acima_de: 0.05,
            subir_abaixo_de: 0.02,
            permanencia: Duration::from_secs(10),
        }
    }

    #[test]
    fn comeca_na_faixa_de_cima() {
        // O pedido de qualidade máxima é este: começa-se onde a qualidade é
        // melhor e desce-se sob evidência, em vez de começar no meio por
        // precaução e nunca subir.
        assert_eq!(Controlador::novo().bitrate_bps(), FAIXAS_BPS[0]);
        assert_eq!(FAIXAS_BPS[0], 48_000);
    }

    #[test]
    fn perda_alta_derruba_uma_faixa_por_medida() {
        let agora = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());

        assert_eq!(controlador.observar(0.10, agora), Some(32_000));
        assert_eq!(controlador.observar(0.10, agora), Some(16_000));
    }

    #[test]
    fn o_piso_da_spec_e_o_piso_de_verdade() {
        // Dezesseis kbps é o fundo declarado. Abaixo dele não há faixa, e
        // continuar «descendo» seria devolver troca sem troca — cada uma
        // reconstruindo o encoder por nada.
        let agora = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, agora);
        let _ = controlador.observar(0.10, agora);

        assert_eq!(controlador.bitrate_bps(), 16_000);
        assert_eq!(
            controlador.observar(0.99, agora),
            None,
            "desceu abaixo do piso da spec"
        );
    }

    #[test]
    fn nao_sobe_antes_da_permanencia_inteira() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);
        assert_eq!(controlador.bitrate_bps(), 32_000);

        // Nove segundos de rede boa não bastam. É o «gradualmente» da spec, e é
        // o que impede uma trégua curta de custar uma reconstrução.
        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(9)),
            None
        );
    }

    #[test]
    fn sobe_depois_da_permanencia() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);

        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(10)),
            Some(48_000)
        );
    }

    #[test]
    fn subir_duas_faixas_custa_duas_permanencias() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);
        let _ = controlador.observar(0.10, inicio);
        assert_eq!(controlador.bitrate_bps(), 16_000);

        assert_eq!(controlador.observar(0.0, inicio), None);
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(10)),
            Some(32_000)
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(15)),
            None,
            "subiu a segunda faixa com meia permanência"
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(20)),
            Some(48_000)
        );
    }

    /// O teste que justifica a histerese existir.
    ///
    /// Uma medida oscilando dentro da zona morta não pode produzir troca
    /// nenhuma. Com um limiar único ela produziria uma por medida, e cada uma
    /// reconstrói o encoder — o pior de todos os mundos, e o defeito que este
    /// desenho existe para não ter.
    #[test]
    fn oscilar_na_zona_morta_nao_troca_de_faixa() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());

        for passo in 0..100_u64 {
            let perda = if passo % 2 == 0 { 0.03 } else { 0.045 };
            assert_eq!(
                controlador.observar(perda, inicio + Duration::from_secs(passo)),
                None,
                "trocou de faixa no passo {passo} sem a rede mudar de regime"
            );
        }
        assert_eq!(controlador.bitrate_bps(), 48_000);
    }

    /// Uma medida ruim no meio zera a contagem da permanência.
    #[test]
    fn uma_medida_ruim_recomeca_a_contagem() {
        let inicio = Instant::now();
        let mut controlador = Controlador::com_limiares(limiares());
        let _ = controlador.observar(0.10, inicio);

        assert_eq!(controlador.observar(0.0, inicio), None);
        // Oito segundos de bom, depois uma medida na zona morta.
        assert_eq!(
            controlador.observar(0.03, inicio + Duration::from_secs(8)),
            None
        );
        // Os dez segundos contam do zero, e não do começo.
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(9)),
            None
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(18)),
            None,
            "aproveitou a contagem anterior à interrupção"
        );
        assert_eq!(
            controlador.observar(0.0, inicio + Duration::from_secs(19)),
            Some(48_000)
        );
    }
}
```

- [ ] **Step 3: Declarar o módulo**

Em `crates/seele-audio/src/lib.rs`, antes de `pub mod codec;` (a lista é alfabética):

```rust
pub mod bitrate;
```

- [ ] **Step 4: Rodar os testes**

Run: `cargo test -p seele-audio --lib bitrate`
Expected: PASS, 7 testes.

- [ ] **Step 5: Verde e limpo**

Run: `cargo fmt -p seele-audio && cargo clippy -p seele-audio --all-targets`
Expected: sem avisos.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-audio/src/bitrate.rs crates/seele-audio/src/lib.rs
git commit -m "feat(audio): o controlador de faixas de bitrate, com histerese e permanência"
```

---

### Task 2: Reconciliar as constantes com a spec

**Files:**
- Modify: `crates/seele-audio/src/codec.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `DEFAULT_BITRATE_BPS = 48_000`, `MAX_BITRATE_BPS = 48_000`, `MIN_BITRATE_BPS = 16_000` (inalterado).

**Contexto:** hoje `MAX` vale 64 000 contra os 48 kbps da spec, e o doc credita ao ADR 0010 um estreitamento que ele nunca fez. Ver a abertura do spec de desenho.

- [ ] **Step 1: Procurar quem depende dos valores velhos**

Run: `grep -rn "DEFAULT_BITRATE_BPS\|MAX_BITRATE_BPS\|32_000\|64_000" crates apps --include=*.rs`

Anotar cada teste que afirme 32 000 ou 64 000: eles descrevem o comportamento que está mudando de propósito e serão atualizados no passo 3.

- [ ] **Step 2: Trocar as constantes e a prosa falsa**

Em `crates/seele-audio/src/codec.rs`, substituir o bloco das três constantes por:

```rust
/// Default encoder bitrate — the top band.
///
/// The top and not the middle: the controller in [`crate::bitrate`] starts here
/// and comes down on evidence, which is what "adaptativo" means in
/// `specs/03-audio.md`. Starting in the middle out of caution and never rising
/// is the behaviour this replaced.
pub const DEFAULT_BITRATE_BPS: u32 = 48_000;

/// Bottom of the range `specs/03-audio.md` declares.
pub const MIN_BITRATE_BPS: u32 = 16_000;

/// Top of the range `specs/03-audio.md` declares.
///
/// **This was 64 000 and the spec always said 48.** The old value carried a doc
/// line crediting ADR 0010 with having "narrowed" the range; ADR 0010 is about
/// in-band FEC and says nothing about bitrate. The number and the sentence were
/// both wrong, and both are gone. See ADR 0036.
pub const MAX_BITRATE_BPS: u32 = 48_000;
```

- [ ] **Step 3: Atualizar os testes que afirmavam os valores velhos**

Para cada sítio anotado no passo 1 que afirme `32_000` como padrão ou `64_000` como teto, trocar pelo valor novo. Um teste que afirme «pedir acima do teto satura no teto» continua válido: só o número muda.

- [ ] **Step 4: Rodar a suíte do crate**

Run: `cargo test -p seele-audio`
Expected: PASS. Qualquer falha aqui é um sítio do passo 1 que ficou para trás.

- [ ] **Step 5: Rodar o workspace, porque o padrão atravessa crates**

Run: `cargo test --workspace --no-fail-fast 2>&1 | grep -E "^test result|FAILED"`
Expected: 0 falhas.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-audio/src/codec.rs
git commit -m "fix(audio): o teto do bitrate passa a ser o da spec, e a atribuição falsa ao ADR 0010 sai"
```

---

### Task 3: O estimador de perda de subida

**Files:**
- Create: `crates/seele-server/src/perda_de_subida.rs`
- Modify: `crates/seele-server/src/lib.rs`
- Test: dentro de `crates/seele-server/src/perda_de_subida.rs`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub const JANELA: Duration` = 5 s
  - `pub const MINIMO_DE_PACOTES: u32` = 50
  - `pub struct PerdaDeSubida`
  - `pub fn PerdaDeSubida::nova() -> Self`
  - `pub fn PerdaDeSubida::com_janela(janela: Duration) -> Self`
  - `pub fn PerdaDeSubida::chegou(&mut self, seq: u16, agora: Instant)`
  - `pub fn PerdaDeSubida::fracao(&mut self, agora: Instant) -> Option<f32>`

- [ ] **Step 1: Escrever o módulo**

Criar `crates/seele-server/src/perda_de_subida.rs`:

```rust
//! Quanto da voz de alguém não chega, medido por quem recebe.
//!
//! # Por que aqui, e por que uma medida nova
//!
//! O `Telemetry::loss_fraction` que esta sessão já manda vem de `stats.path` do
//! quinn, e não serve para comandar um encoder por duas razões que não se
//! corrigem uma à outra:
//!
//! - **é a direção errada** — mede o que o servidor mandou e se perdeu, ou seja
//!   o *download* de quem escuta. Encolher o microfone de alguém porque o
//!   download dele está ruim é o oposto do que `specs/03-audio.md` pede;
//! - **é cumulativo desde o início da conexão** — uma razão monótona que, uma
//!   vez subida, só decai assintoticamente. «Sobe de volta gradualmente» é
//!   aritmeticamente impossível a partir dela.
//!
//! O número certo já passa debaixo do nariz do servidor: a `VoiceRoom` decodifica
//! o cabeçalho de mídia para conferir que o `ssrc` não foi forjado, e com ele
//! `seq` vem de graça.
//!
//! # Por que lacuna de `seq` é perda, e nunca silêncio
//!
//! Porque o DTX **não** incrementa `seq`. O carimbo de tempo conta amostras
//! decorridas e a sequência conta pacotes emitidos — é a separação que M1.9
//! introduziu, e está escrita no `seele-core::voice`:
//!
//! > The timestamp counts elapsed samples whether or not anything goes out; the
//! > sequence counts only what does.
//!
//! Quem cala não produz lacuna: produz ausência de pacote com `seq` parado, e o
//! pacote seguinte continua de onde parou. Toda lacuna é, então, um pacote que
//! saiu e não chegou. Não há heurística a calibrar.
//!
//! # O que este módulo não faz
//!
//! Não toca no payload. `specs/08-seguranca.md` proíbe, e a promessa de que E2EE
//! é incremento e não reescrita depende disso.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Sobre quanto tempo a fração é calculada.
///
/// Cinco segundos, e o número é uma troca de verdade: curto demais e a malha do
/// cliente persegue ruído; longo demais e ela reage tarde. A 50 quadros por
/// segundo são ~250 pacotes, e 5% de 250 são 12 — amostra grande o bastante para
/// o limiar da spec não ser decidido por dois pacotes. Abaixo disso, «5%» vira
/// uma frase sobre meia dúzia deles.
pub const JANELA: Duration = Duration::from_secs(5);

/// Quantos pacotes a janela precisa ter antes de a fração significar algo.
///
/// Um segundo de fala. Abaixo disso a divisão tem denominador pequeno demais
/// para o limiar de 5% distinguir rede de acaso, e [`PerdaDeSubida::fracao`]
/// responde «não sei» em vez de um número que seria ruído com aparência de
/// medida.
pub const MINIMO_DE_PACOTES: u32 = 50;

/// Uma lacuna maior que isto é tratada como recomeço, e não como perda.
///
/// O `seq` é um `u16` e dá a volta. Uma diferença enorme é, na prática, uma
/// conexão que recomeçou ou um `seq` que voltou — nunca mil pacotes perdidos de
/// uma vez, porque a essa altura não haveria conversa para medir. Contá-la como
/// perda enfiaria um degrau falso na janela inteira.
const MAIOR_LACUNA_CRIVEL: u32 = 1_000;

/// Um pedaço da janela: quando, quantos se esperavam, quantos chegaram.
#[derive(Debug, Clone, Copy)]
struct Amostra {
    quando: Instant,
    esperados: u32,
    chegados: u32,
}

/// A perda de subida de um `ssrc`, sobre uma janela deslizante.
#[derive(Debug)]
pub struct PerdaDeSubida {
    janela: Duration,
    amostras: VecDeque<Amostra>,
    ultimo_seq: Option<u16>,
}

impl Default for PerdaDeSubida {
    fn default() -> Self {
        Self::nova()
    }
}

impl PerdaDeSubida {
    /// Um estimador com a janela do projeto.
    #[must_use]
    pub fn nova() -> Self {
        Self::com_janela(JANELA)
    }

    /// Um estimador com a janela escolhida.
    #[must_use]
    pub fn com_janela(janela: Duration) -> Self {
        Self {
            janela,
            amostras: VecDeque::new(),
            ultimo_seq: None,
        }
    }

    /// Um pacote chegou com este `seq`.
    pub fn chegou(&mut self, seq: u16, agora: Instant) {
        let Some(anterior) = self.ultimo_seq else {
            // O primeiro pacote não tem antecessor com que comparar: ele
            // estabelece o ponto de partida e não afirma nada sobre perda.
            self.ultimo_seq = Some(seq);
            return;
        };

        let avanco = u32::from(seq.wrapping_sub(anterior));
        if avanco == 0 {
            // Duplicata, ou um pacote reordenado que voltou ao mesmo número.
            // Não é perda e não é chegada nova.
            return;
        }
        if avanco > MAIOR_LACUNA_CRIVEL {
            // Recomeço. Ver `MAIOR_LACUNA_CRIVEL`.
            self.ultimo_seq = Some(seq);
            return;
        }

        self.ultimo_seq = Some(seq);
        self.amostras.push_back(Amostra {
            quando: agora,
            esperados: avanco,
            chegados: 1,
        });
    }

    /// A fração perdida na janela, ou `None` enquanto não há amostra bastante.
    ///
    /// `&mut self` porque a leitura é o momento natural de descartar o que saiu
    /// da janela: um estimador de alguém que parou de falar não tem por que ser
    /// varrido por um relógio próprio.
    pub fn fracao(&mut self, agora: Instant) -> Option<f32> {
        while let Some(primeira) = self.amostras.front() {
            if agora.duration_since(primeira.quando) > self.janela {
                self.amostras.pop_front();
            } else {
                break;
            }
        }

        let mut esperados = 0_u32;
        let mut chegados = 0_u32;
        for amostra in &self.amostras {
            esperados = esperados.saturating_add(amostra.esperados);
            chegados = chegados.saturating_add(amostra.chegados);
        }

        if esperados < MINIMO_DE_PACOTES {
            return None;
        }
        let perdidos = esperados.saturating_sub(chegados);
        #[allow(
            clippy::cast_precision_loss,
            reason = "contagens de uma janela de cinco segundos; muito abaixo do que f32 representa exatamente"
        )]
        Some(perdidos as f32 / esperados as f32)
    }
}
```

- [ ] **Step 2: Acrescentar o módulo de testes ao mesmo arquivo**

```rust
#[cfg(test)]
mod tests {
    use super::{PerdaDeSubida, MINIMO_DE_PACOTES};
    use std::time::{Duration, Instant};

    /// Manda `quantos` pacotes seguidos, um a cada 20 ms, a partir de `seq`.
    fn seguidos(perda: &mut PerdaDeSubida, inicio: Instant, primeiro: u16, quantos: u16) {
        for passo in 0..quantos {
            perda.chegou(
                primeiro.wrapping_add(passo),
                inicio + Duration::from_millis(u64::from(passo) * 20),
            );
        }
    }

    #[test]
    fn uma_sequencia_sem_lacuna_nao_perde_nada() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 200);
        assert_eq!(perda.fracao(inicio + Duration::from_secs(4)), Some(0.0));
    }

    /// O teste que mais importa deste módulo.
    ///
    /// Silêncio de DTX não é perda. Quem cala não manda pacote e **não**
    /// incrementa `seq`, então a sequência continua de onde parou depois de uma
    /// pausa longa no relógio. Se isto virasse perda, a malha do cliente
    /// derrubaria o bitrate de quem simplesmente parou de falar — e o faria
    /// justamente nas conversas mais calmas.
    #[test]
    fn silencio_de_dtx_nao_conta_como_perda() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));

        // Um segundo de fala.
        seguidos(&mut perda, inicio, 0, 50);
        // Trinta segundos calado: nenhum pacote, e `seq` parado em 49.
        // Depois, a fala recomeça exatamente em 50.
        let volta = inicio + Duration::from_secs(30);
        seguidos(&mut perda, volta, 50, 50);

        assert_eq!(
            perda.fracao(volta + Duration::from_secs(1)),
            Some(0.0),
            "o silêncio do DTX foi contado como perda"
        );
    }

    #[test]
    fn uma_lacuna_de_seq_e_perda() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));

        // Cem pacotes, com o de número 50 faltando.
        for passo in 0..100_u16 {
            if passo == 50 {
                continue;
            }
            perda.chegou(passo, inicio + Duration::from_millis(u64::from(passo) * 20));
        }

        let fracao = perda
            .fracao(inicio + Duration::from_secs(2))
            .expect("cem pacotes passam do mínimo");
        assert!(
            (fracao - 0.01).abs() < 0.005,
            "um perdido em cem deu {fracao}"
        );
    }

    #[test]
    fn abaixo_do_minimo_a_resposta_e_nao_sei() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 10);
        assert_eq!(
            perda.fracao(inicio + Duration::from_millis(300)),
            None,
            "afirmou uma fração com dez pacotes de amostra"
        );
        assert!(MINIMO_DE_PACOTES > 10);
    }

    /// A propriedade que o número cumulativo de hoje não tem, e a razão de
    /// existir medida nova: quando o enlace melhora, a fração **desce**.
    #[test]
    fn a_janela_desliza_e_a_fracao_volta_a_cair() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(5));

        // Dois segundos ruins: metade dos pacotes some.
        for passo in 0..100_u16 {
            if passo % 2 == 0 {
                continue;
            }
            perda.chegou(passo, inicio + Duration::from_millis(u64::from(passo) * 20));
        }
        let ruim = perda
            .fracao(inicio + Duration::from_secs(2))
            .expect("cem pacotes");
        assert!(ruim > 0.4, "a fração ruim deu {ruim}");

        // Cinco segundos depois, tudo limpo. O trecho ruim saiu da janela.
        let limpo = inicio + Duration::from_secs(8);
        seguidos(&mut perda, limpo, 200, 200);
        let bom = perda
            .fracao(limpo + Duration::from_secs(4))
            .expect("duzentos pacotes");
        assert_eq!(bom, 0.0, "o trecho ruim não saiu da janela");
    }

    #[test]
    fn duplicata_nao_conta() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 100);
        let antes = perda.fracao(inicio + Duration::from_secs(2));

        perda.chegou(99, inicio + Duration::from_secs(2));
        let depois = perda.fracao(inicio + Duration::from_secs(2));
        assert_eq!(antes, depois, "uma duplicata mexeu na medida");
    }

    /// `seq` é `u16` e dá a volta no meio de uma chamada longa. A volta é o caso
    /// normal, e não um recomeço: dezesseis bits a cinquenta pacotes por segundo
    /// dão a volta a cada 22 minutos.
    #[test]
    fn a_volta_do_u16_e_continuidade_e_nao_lacuna() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, u16::MAX - 49, 50);
        seguidos(&mut perda, inicio + Duration::from_secs(1), 0, 50);
        assert_eq!(
            perda.fracao(inicio + Duration::from_secs(2)),
            Some(0.0),
            "a volta do contador foi lida como perda"
        );
    }
}
```

- [ ] **Step 3: Declarar o módulo**

Em `crates/seele-server/src/lib.rs`, entre `pub mod hospedagem;` e `pub mod permissions;` (a lista é alfabética):

```rust
pub mod perda_de_subida;
```

- [ ] **Step 4: Rodar os testes**

Run: `cargo test -p seele-server --lib perda_de_subida`
Expected: PASS, 7 testes.

- [ ] **Step 5: Verde e limpo**

Run: `cargo fmt -p seele-server && cargo clippy -p seele-server --all-targets`
Expected: sem avisos.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server/src/perda_de_subida.rs crates/seele-server/src/lib.rs
git commit -m "feat(servidor): a perda de subida medida por lacuna de seq, em janela deslizante"
```

---

### Task 4: O quadro novo e a subida de versão

**Files:**
- Modify: `crates/seele-proto/src/version.rs`
- Modify: `crates/seele-proto/src/control.rs`
- Modify: `crates/seele-server/src/session.rs` (struct `Session`, e onde ela é construída)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `PROTOCOL_VERSION: u8 = 2`
  - `ServerMessage::UplinkLoss { fraction: f32 }` — **a última variante do enum**
  - `Session::protocol_version: u8`

- [ ] **Step 1: Escrever o teste que reprova, sobre a compatibilidade**

Em `crates/seele-proto/src/version.rs`, dentro do `#[cfg(test)] mod tests` que já existe:

```rust
    /// Um cliente da versão anterior continua entrando.
    ///
    /// O ADR 0036 sobe a versão para carregar `UplinkLoss`, e a promessa que
    /// acompanha a subida é esta: ninguém que já instalou perde o servidor. Um
    /// cliente v1 conecta, não recebe o quadro novo, e roda no bitrate fixo —
    /// que é exatamente o comportamento que ele tinha antes.
    #[test]
    fn a_versao_anterior_continua_dentro_da_janela() {
        assert_eq!(PROTOCOL_VERSION, 2);
        assert_eq!(oldest_supported_version(), 1);
        assert!(negotiate(1).is_ok(), "um cliente v1 foi recusado");
        assert!(negotiate(2).is_ok());
        assert!(negotiate(3).is_err(), "um cliente do futuro foi aceito");
    }
```

O `mod tests` deste arquivo já faz `use super::*` (linha 98), então `negotiate` e `oldest_supported_version` estão disponíveis.

- [ ] **Step 2: Rodar para ver reprovar**

Run: `cargo test -p seele-proto --lib version`
Expected: FAIL — `assert_eq!(PROTOCOL_VERSION, 2)` acusa 1.

- [ ] **Step 3: Subir a versão**

Em `crates/seele-proto/src/version.rs`:

```rust
/// Version of the wire protocol implemented by this build.
///
/// Versioned independently of the product version (`specs/10-convencoes.md`).
///
/// **2 desde o ADR 0036**, que acrescentou `ServerMessage::UplinkLoss`. O
/// postcard indexa variante por posição, então um cliente v1 não sabe decodificar
/// a variante nova; a janela de compatibilidade é o que garante que ele continue
/// conectando e simplesmente não a receba.
pub const PROTOCOL_VERSION: u8 = 2;
```

- [ ] **Step 4: Acrescentar a variante no fim do enum**

Em `crates/seele-proto/src/control.rs`, como **última** variante de `ServerMessage`:

```rust
    /// Quanto da voz desta conexão não está chegando ao servidor.
    ///
    /// Escrito **para uma sessão só**, e nunca difundido: a perda de subida de
    /// alguém não é assunto de mais ninguém, e espalhá-la contaria a toda a sala
    /// a qualidade da rede de cada um.
    ///
    /// Medido por quem recebe, a partir de lacunas de `seq` — ver
    /// `seele_server::perda_de_subida` e o ADR 0036. O `loss_fraction` de
    /// [`Telemetry`] **não** serve para isto: mede a outra direção e é cumulativo.
    ///
    /// Zero a um.
    UplinkLoss {
        /// A fração perdida na janela mais recente.
        fraction: f32,
    },
```

**No fim, e não em ordem alfabética ou temática.** O postcard escreve o índice da variante; inserir no meio renumeraria tudo o que vem depois e faria dois builds da mesma versão discordarem sobre o que cada quadro significa.

- [ ] **Step 5: Guardar a versão negociada na sessão**

Em `crates/seele-server/src/session.rs`, acrescentar ao `pub struct Session`:

```rust
    /// A versão de protocolo que este par declarou no `Hello`.
    ///
    /// Guardada porque quadros acrescentados depois da v1 só podem ser escritos
    /// para quem sabe decodificá-los: o postcard não é autodescritivo, e uma
    /// variante desconhecida não é ignorada — ela quebra a leitura do fluxo.
    pub protocol_version: u8,
```

Depois, na função que faz o aperto de mão, onde `Session` é construída, preencher o campo com o `version` que já chega no `ClientMessage::Hello` e já passa por `seele_proto::version::negotiate(version)` (por volta da linha 458). O valor a guardar é o `version` do cliente, não `PROTOCOL_VERSION`.

- [ ] **Step 6: Compilar e consertar o que faltar**

Run: `cargo build --workspace --all-targets`
Expected: erros apontando cada construção de `Session` sem o campo novo — inclusive em testes. Preencher cada uma com `seele_proto::PROTOCOL_VERSION` nos testes que não estão exercitando compatibilidade.

- [ ] **Step 7: Rodar**

Run: `cargo test -p seele-proto -p seele-server --no-fail-fast`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/seele-proto/src/version.rs crates/seele-proto/src/control.rs crates/seele-server/src/session.rs
git commit -m "feat(protocolo): a versão sobe para 2 e traz UplinkLoss, escrito para uma sessão só"
```

---

### Task 5: A sala mede, e a sessão entrega a quem produziu

**Files:**
- Modify: `crates/seele-server/src/voice_room.rs`
- Modify: `crates/seele-server/src/server.rs` (enum `Event`, linha 41)
- Modify: `crates/seele-server/src/session.rs` (função `translate`)
- Test: `crates/seele-server/src/voice_room.rs`, no `mod tests` existente

**Interfaces:**
- Consumes: `seele_server::perda_de_subida::PerdaDeSubida` (Task 3); `ServerMessage::UplinkLoss` e `Session::protocol_version` (Task 4).
- Produces: `Event::UplinkLoss { person: PersonId, fraction: f32 }`.

- [ ] **Step 1: Acrescentar a variante de evento**

Em `crates/seele-server/src/server.rs`, no `pub enum Event` da linha 41:

```rust
    /// Quanto da voz de alguém não está chegando.
    ///
    /// Vai ao barramento como todo evento, porque a sala não tem outro caminho
    /// até a sessão — mas `session::translate` o entrega **só** a `person`. Ver
    /// o ADR 0036 sobre por que difundi-lo seria contar a toda a sala a
    /// qualidade da rede de cada um.
    UplinkLoss {
        /// De quem é a subida medida.
        person: PersonId,
        /// A fração perdida, zero a um.
        fraction: f32,
    },
```

- [ ] **Step 2: Dar um estimador e um relógio a cada `Member`**

Em `crates/seele-server/src/voice_room.rs`, acrescentar ao `struct Member`:

```rust
    /// Quanto da voz desta pessoa não está chegando. Ver ADR 0036.
    perda: crate::perda_de_subida::PerdaDeSubida,
    /// Quando medir de novo.
    ///
    /// A fração é calculada uma vez por segundo e não a cada pacote: ela varre a
    /// janela, e fazê-lo cinquenta vezes por segundo por participante seria
    /// pagar a varredura cinquenta vezes para um número que o cliente só usa
    /// uma.
    proxima_medida: Instant,
```

E, na construção do `Member` dentro de `VoiceRoomCommand::Join`:

```rust
                        perda: crate::perda_de_subida::PerdaDeSubida::nova(),
                        proxima_medida: now + INTERVALO_DE_MEDIDA,
```

Junto das outras constantes do topo do arquivo:

```rust
/// De quanto em quanto tempo a perda de subida de cada pessoa é recalculada.
const INTERVALO_DE_MEDIDA: Duration = Duration::from_secs(1);
```

Acrescentar `use std::time::Duration;` ao topo se ainda não estiver lá.

- [ ] **Step 3: Alimentar o estimador em `forward`**

Em `VoiceRoom::forward`, **depois** da checagem de `may_speak` e **antes** do limitador de taxa, acrescentar:

```rust
        // Alimentado antes do limitador de taxa, de propósito: um quadro que o
        // limitador descarta **chegou**, e contá-lo como perda misturaria uma
        // decisão nossa com o que a rede fez. O que se mede aqui é a rede.
        member.perda.chegou(header.seq, now);
        if now >= member.proxima_medida {
            member.proxima_medida = now + INTERVALO_DE_MEDIDA;
            if let Some(fracao) = member.perda.fracao(now) {
                if let Some(eventos) = self.eventos.as_ref() {
                    let _ = eventos.send(Event::UplinkLoss {
                        person,
                        fraction: fracao,
                    });
                }
            }
        }
```

**Nota de empréstimo:** `member` vem de `self.members.get_mut(&person)` e `self.eventos` é outro campo de `self`, então o empréstimo mutável de `members` conflita com a leitura de `eventos`. Se o compilador reclamar, calcular a fração primeiro, soltar `member`, e só então emitir:

```rust
        member.perda.chegou(header.seq, now);
        let medida = if now >= member.proxima_medida {
            member.proxima_medida = now + INTERVALO_DE_MEDIDA;
            member.perda.fracao(now)
        } else {
            None
        };
        if let Some(fracao) = medida {
            if let Some(eventos) = self.eventos.as_ref() {
                let _ = eventos.send(Event::UplinkLoss {
                    person,
                    fraction: fracao,
                });
            }
        }
```

O campo do barramento é `self.eventos: Option<broadcast::Sender<Event>>` (linha 208), e `Event` já está importado no topo do arquivo como `use crate::server::Event;`.

- [ ] **Step 4: Entregar só a quem produziu**

Em `crates/seele-server/src/session.rs`, na função `translate`, acrescentar um braço junto dos outros:

```rust
        // Só para quem produziu. É a única mensagem deste enum cuja audiência é
        // uma pessoa, e o filtro está aqui — e não em quem emite — porque a sala
        // não conhece sessão nenhuma.
        Event::UplinkLoss { person, fraction } if *person == self_person => {
            Some(ServerMessage::UplinkLoss {
                fraction: *fraction,
            })
        }
        Event::UplinkLoss { .. } => None,
```

- [ ] **Step 5: Não mandar a cliente que não sabe ler**

No braço `event = events.recv()` do `select!` de `run_session`, onde o resultado de `translate` é escrito, guardar a escrita atrás da versão. Localizar:

```rust
                if let Some(message) = translate(&event, &channels, session.person) {
                    frame::write(&mut send, &message).await?;
                }
```

e trocar por:

```rust
                if let Some(message) = translate(&event, &channels, session.person) {
                    // Um cliente v1 não conhece as variantes que a v2 acrescentou,
                    // e o postcard não é autodescritivo: mandá-la não seria
                    // ignorada do outro lado, seria o fluxo de controle dele
                    // ficando deslocado para sempre. A janela de compatibilidade
                    // do ADR 0036 promete que ele continua funcionando, e é aqui
                    // que a promessa é cumprida.
                    let entende = match message {
                        ServerMessage::UplinkLoss { .. } => session.protocol_version >= 2,
                        _ => true,
                    };
                    if entende {
                        frame::write(&mut send, &message).await?;
                    }
                }
```

- [ ] **Step 6: Escrever os testes**

No `mod tests` de `crates/seele-server/src/voice_room.rs`:

```rust
    /// A sala mede a subida de quem fala e conta a quem falou.
    #[test]
    fn a_sala_relata_a_perda_de_subida_de_quem_fala() {
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);

        let inicio = Instant::now();
        // Cem quadros com o de número 50 faltando, ao longo de dois segundos:
        // passa do mínimo de amostra e do intervalo de medida.
        for passo in 0..100_u16 {
            if passo == 50 {
                continue;
            }
            let quadro = quadro_de_voz(100, passo);
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: quadro,
                },
                inicio + Duration::from_millis(u64::from(passo) * 20),
            );
        }

        let mut relatada = None;
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::UplinkLoss { person, fraction } = evento {
                assert_eq!(person, PersonId(1), "a perda foi atribuída a outra pessoa");
                relatada = Some(fraction);
            }
        }
        let fracao = relatada.expect("a sala não relatou perda nenhuma");
        assert!(
            (fracao - 0.01).abs() < 0.01,
            "um perdido em cem foi relatado como {fracao}"
        );
    }

    /// Quem fala sem perder nada não vira notícia ruim.
    #[test]
    fn uma_subida_limpa_e_relatada_como_zero() {
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);

        let inicio = Instant::now();
        for passo in 0..100_u16 {
            let quadro = quadro_de_voz(100, passo);
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: quadro,
                },
                inicio + Duration::from_millis(u64::from(passo) * 20),
            );
        }

        let mut relatada = None;
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::UplinkLoss { fraction, .. } = evento {
                relatada = Some(fraction);
            }
        }
        assert_eq!(relatada, Some(0.0));
    }
```

Este `mod tests` já tem helpers `member`, `sala_com_barramento` e `espectador`. Falta um que produza um quadro de voz com `seq` escolhido — os testes existentes usam um payload qualquer. Acrescentar junto dos outros helpers:

```rust
    /// Um quadro de voz com `ssrc` e `seq` escolhidos.
    fn quadro_de_voz(ssrc: u32, seq: u16) -> Vec<u8> {
        let cabecalho = seele_proto::MediaHeader {
            version: seele_proto::PROTOCOL_VERSION,
            ssrc,
            seq,
            timestamp: u32::from(seq) * 960,
        };
        let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
        let tamanho = cabecalho
            .encode_datagram(&[7_u8; 80], &mut fora)
            .expect("o quadro cabe");
        fora.truncate(tamanho);
        fora
    }
```

Conferir a forma exata com que os testes vizinhos montam um datagrama e casar com ela; se já houver helper equivalente, estender o existente com um parâmetro `seq` em vez de criar um segundo.

- [ ] **Step 7: Rodar**

Run: `cargo test -p seele-server --no-fail-fast`
Expected: PASS. Os testes de limite de taxa vizinhos continuam verdes — a medida entra antes do limitador e não muda o que ele conta.

- [ ] **Step 8: Verde e limpo**

Run: `cargo fmt -p seele-server && cargo clippy -p seele-server --all-targets`
Expected: sem avisos.

- [ ] **Step 9: Commit**

```bash
git add crates/seele-server/src/voice_room.rs crates/seele-server/src/server.rs crates/seele-server/src/session.rs
git commit -m "feat(servidor): a sala mede a subida de quem fala, e só quem falou recebe"
```

---

### Task 6: O cliente reage

**Files:**
- Modify: `crates/seele-core/src/state.rs`
- Modify: `crates/seele-core/src/voice.rs`
- Modify: `crates/seele-ffi/src/lib.rs` (função `fold`, por volta da linha 2972)
- Test: `crates/seele-core/src/voice.rs`

**Interfaces:**
- Consumes: `seele_audio::bitrate::Controlador` (Task 1); `ServerMessage::UplinkLoss` (Task 4).
- Produces: `pub fn Voice::observar_perda(&self, perda: f32, agora: Instant) -> Option<u32>`.

- [ ] **Step 1: Hospedar o controlador no `Voice`**

Em `crates/seele-core/src/voice.rs`, acrescentar ao `struct Voice`:

```rust
    /// Escolhe a faixa de bitrate a partir da perda de subida que o servidor
    /// relata. Ver o ADR 0036.
    ///
    /// `Mutex` e não atômico porque o controlador tem estado — o índice da faixa
    /// e desde quando a rede está boa —, e porque ele é tocado uma vez por
    /// segundo e não no laço de áudio. O que o laço lê continua sendo
    /// `Controls::bitrate`, que é atômico e não espera por ninguém.
    faixa: std::sync::Mutex<seele_audio::bitrate::Controlador>,
```

Inicializar com `seele_audio::bitrate::Controlador::novo()` em cada construção de `Voice`.

- [ ] **Step 2: Escrever o teste que reprova**

No `#[cfg(test)]` de `crates/seele-core/src/voice.rs`:

```rust
#[cfg(test)]
mod faixa_de_bitrate {
    use seele_audio::bitrate::FAIXAS_BPS;
    use seele_audio::codec::DEFAULT_BITRATE_BPS;

    /// O padrão do codec é a faixa de cima do controlador.
    ///
    /// Os dois números moram em módulos diferentes e nada os obriga a concordar.
    /// Se divergirem, a primeira medida de rede boa «subiria» para um valor que
    /// já estava em vigor, e a primeira de rede ruim desceria a partir de outro
    /// lugar — o encoder e a malha discordando sobre onde a conversa começou.
    #[test]
    fn o_padrao_do_codec_e_a_faixa_de_cima() {
        assert_eq!(DEFAULT_BITRATE_BPS, FAIXAS_BPS[0]);
    }
}
```

Um teste sobre `observar_perda` exigiria um `Voice` aberto, que precisa de placa de som; a malha em si já está provada na Task 1, e o que falta provar aqui é a costura entre os dois números.

- [ ] **Step 3: Expor `observar_perda`**

Junto dos outros `pub fn set_*` de `impl Voice`:

```rust
    /// Dobra a perda de subida relatada pelo servidor, e move a faixa se ela mudou.
    ///
    /// Devolve o bitrate novo quando houve troca, para quem quiser registrá-la.
    /// `None` é o caso comum e não é falha: a maior parte das medidas não muda
    /// nada, e é justamente isso que mantém a reconstrução do encoder rara.
    pub fn observar_perda(&self, perda: f32, agora: Instant) -> Option<u32> {
        let Ok(mut faixa) = self.faixa.lock() else {
            return None;
        };
        let novo = faixa.observar(perda, agora)?;
        // O laço de voz lê isto a cada volta e chama `VoiceEncoder::set_bitrate`,
        // que só reconstrói quando o valor mudou de verdade.
        self.controls.bitrate.store(novo, Ordering::Relaxed);
        Some(novo)
    }
```

- [ ] **Step 4: Guardar o número no `Room`, para a telemetria mostrá-lo**

Em `crates/seele-core/src/state.rs`, acrescentar o campo ao `struct Room` (junto dos outros de telemetria) e o braço em `Room::apply`:

```rust
            ServerMessage::UplinkLoss { fraction } => {
                self.perda_de_subida = Some(*fraction);
                changed.telemetry = true;
            }
```

O campo:

```rust
    /// Quanto da nossa voz não está chegando ao servidor, se ele já disse.
    ///
    /// `None` até a primeira medida, e num servidor v1 para sempre. Distinto de
    /// `Some(0.0)`, que é o servidor afirmando que nada se perdeu.
    pub perda_de_subida: Option<f32>,
```

Inicializar como `None` no construtor de `Room` e em `Room::adopt`, pelo mesmo motivo que o ícone é zerado ali: um aperto de mão descreve o servidor do zero.

- [ ] **Step 5: Ligar em `fold`**

Em `crates/seele-ffi/src/lib.rs`, na função `fold`, **depois** do `room.apply(message)` e das notificações que já existem:

```rust
    // A malha do ADR 0036. Aqui e não dentro do `Room` porque o `Room` não
    // conhece o `Voice` — ele é estado do servidor, e a decisão de encolher o
    // microfone é do lado do áudio.
    if let seele_core::ServerMessage::UplinkLoss { fraction } = message {
        if let Ok(voice) = shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                if let Some(bps) = voice.observar_perda(*fraction, std::time::Instant::now()) {
                    tracing::info!(bps, perda = fraction, "a faixa de bitrate mudou");
                }
            }
        }
    }
```

Conferir o nome exato do campo de voz no `Shared` (é `voice: Mutex<Option<Voice>>`, por volta da linha 615) e o caminho de importação de `ServerMessage` já usado no arquivo.

- [ ] **Step 6: Rodar**

Run: `cargo test -p seele-core -p seele-ffi --no-fail-fast`
Expected: PASS.

- [ ] **Step 7: Verde e limpo**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets`
Expected: sem avisos.

- [ ] **Step 8: Commit**

```bash
git add crates/seele-core/src/voice.rs crates/seele-core/src/state.rs crates/seele-ffi/src/lib.rs
git commit -m "feat(cliente): a faixa de bitrate segue a perda de subida que o servidor relata"
```

---

### Task 7: A costura, ponta a ponta

**Files:**
- Create: `crates/seele-conformance/tests/bitrate_adaptativo.rs`

**Interfaces:**
- Consumes: tudo das tarefas 1 a 6.
- Produces: nada.

**Por que este teste, e o que ele não tenta:** as malhas já estão provadas isoladas. O que nenhuma tarefa anterior prova é que o quadro **atravessa** — que a sala mede, a sessão filtra por destinatário, e o cliente certo o recebe. Induzir perda de verdade num enlace QUIC local não é confiável e não é o que falta provar; o que falta é a costura.

- [ ] **Step 1: Escrever o teste**

Criar `crates/seele-conformance/tests/bitrate_adaptativo.rs` seguindo o idioma de `crates/seele-server/tests/voz_sob_carga.rs`, que já tem o par cru e o aperto de mão à mão. Copiar dali o `AceitaQualquer`, o `server()`, o `abrir()` e o `quadro()`, e escrever:

```rust
/// O relato de perda chega a quem falou, e só a ele.
///
/// A sala mede lacuna de `seq`; a sessão entrega só ao dono da subida. Este
/// teste é a costura: nenhuma tarefa anterior prova que o quadro atravessa o
/// fio, e é no fio que um filtro por destinatário erra.
#[tokio::test(flavor = "multi_thread")]
async fn quem_perde_pacote_e_avisado_e_o_vizinho_nao() -> Result<()> {
    let (endereco, servidor) = server().await?;

    let mut falante = abrir(endereco, 1, 1024 * 1024).await?;
    let sala = falante.sala;
    frame::write(
        &mut falante.envio,
        &ClientMessage::EnterVoiceRoom { voice_room: sala, password: None },
    )
    .await?;

    let mut vizinho = abrir(endereco, 2, 1024 * 1024).await?;
    frame::write(
        &mut vizinho.envio,
        &ClientMessage::EnterVoiceRoom { voice_room: sala, password: None },
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Cem quadros com o de número 50 nunca enviado: uma lacuna de `seq` que o
    // servidor lê como um pacote perdido em cem.
    for seq in 0..100_u16 {
        if seq == 50 {
            continue;
        }
        let _ = falante
            .conexao
            .send_datagram(quadro(falante.ssrc, seq).into());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // O falante recebe o relato.
    let mut relatada = None;
    let prazo = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < prazo && relatada.is_none() {
        match tokio::time::timeout(
            Duration::from_millis(500),
            frame::read::<ServerMessage>(&mut falante.recebe),
        )
        .await
        {
            Ok(Ok(ServerMessage::UplinkLoss { fraction })) => relatada = Some(fraction),
            Ok(Ok(_)) => {}
            Ok(Err(_)) => break,
            Err(_) => {}
        }
    }
    let fracao = relatada.expect("quem perdeu pacote não foi avisado");
    assert!(
        fracao > 0.0,
        "a perda foi relatada como zero, e um pacote em cem sumiu"
    );

    // E o vizinho, que não mandou voz nenhuma, não recebe relato sobre a rede
    // de ninguém. É a promessa de privacidade do ADR 0036, e é o filtro por
    // destinatário que a cumpre.
    while let Ok(Ok(quadro)) = tokio::time::timeout(
        Duration::from_millis(200),
        frame::read::<ServerMessage>(&mut vizinho.recebe),
    )
    .await
    {
        assert!(
            !matches!(quadro, ServerMessage::UplinkLoss { .. }),
            "o vizinho recebeu a medida de rede de outra pessoa"
        );
    }

    servidor.shutdown();
    Ok(())
}
```

Conferir se `seele-conformance` tem as dependências de teste que este arquivo usa (`quinn`, `rustls`, `ed25519_dalek`, `seele_server`); se faltar alguma, acrescentar em `crates/seele-conformance/Cargo.toml` sob `[dev-dependencies]`, e se `seele-server` não puder ser dependência daqui, escrever o teste em `crates/seele-server/tests/` ao lado de `voz_sob_carga.rs`.

- [ ] **Step 2: Rodar**

Run: `cargo test -p seele-conformance --test bitrate_adaptativo -- --nocapture`
Expected: PASS.

- [ ] **Step 3: A suíte inteira**

Run: `cargo test --workspace --no-fail-fast 2>&1 | grep -E "^test result|FAILED"`
Expected: 0 falhas.

- [ ] **Step 4: Verde e limpo**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets`
Expected: sem avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/seele-conformance/tests/bitrate_adaptativo.rs
git commit -m "test(conformidade): o relato de perda atravessa o fio e só chega a quem produziu"
```

---

### Task 8: Fechar a documentação

**Files:**
- Modify: `docs/pendencias.md`
- Modify: `docs/adr/0010-fec-do-opus.md`

- [ ] **Step 1: Registrar a dívida de FEC que agora tem como ser medida**

Ao fim de `docs/adr/0010-fec-do-opus.md`, antes de «Custo de reverter», acrescentar:

```markdown
## Adendo — 2026-08-30

O [ADR 0036](0036-bitrate-adaptativo-em-faixas.md) construiu o bitrate
adaptativo, e ao fazê-lo criou a medida que faltava aqui: perda de subida por
pessoa, em janela deslizante, medida no servidor por lacuna de `seq`.

Este ADR recusou o FEC por, entre outras coisas, não haver medição de perda em
internet real. Agora há. A decisão **não muda com este adendo** — nada foi
medido ainda —, mas o obstáculo que a adiava saiu do caminho, e reavaliar FEC
passa a ser trabalho que alguém pode fazer em vez de esperar.

Um dos dois bloqueios que este ADR nomeou para o bitrate adaptativo **continua
de pé**, e vale repetir para quem ler os dois documentos: o `shiguredo_opus` não
expõe setter em tempo de execução. O ADR 0036 não o contorna; ele desenha em
faixas por causa dele.
```

- [ ] **Step 2: Registrar a pendência em `docs/pendencias.md`**

Acrescentar uma entrada nova ao fim, com o próximo número livre, seguindo o formato do arquivo (a regra do topo diz que entradas nunca são renumeradas). O conteúdo: os números de partida do ADR 0036 — janela de 5 s, permanência de 10 s, limiares de 5% e 2% — são ponto de partida e não foram medidos em rede real. O que os confirma é rodar os perfis de `crates/seele-audio/src/netsim.rs` contra a malha e ver se a faixa acompanha o regime sem perseguir ruído.

- [ ] **Step 3: Commit**

```bash
git add docs/pendencias.md docs/adr/0010-fec-do-opus.md
git commit -m "docs: o ADR 0010 ganha adendo, e os números do adaptativo entram como pendência"
```

---

## Auto-revisão

**Cobertura do spec.** Seção 0 (bloqueios do ADR 0010) → Task 1, que desenha em faixas por causa do bloqueio que ficou, e Task 8, que o registra. Seção 1 (o sinal) → Tasks 3 e 5. Seção 2 (o controlador) → Task 1, com os números de partida no `Limiares::default`. Seção 3 (protocolo) → Task 4. Seção 4 (valores) → Task 2. Seção 5 (o que não faz) → nada a implementar, e a dívida de FEC vai para a Task 8. Seção 6 (como se prova) → os testes das Tasks 1, 3, 5 e 7. Seção 7 (riscos) → Task 8, passo 2.

**Consistência de tipos.** `Controlador::observar(f32, Instant) -> Option<u32>` é produzida na Task 1 e consumida na Task 6 com essa assinatura. `PerdaDeSubida::{chegou, fracao}` é produzida na Task 3 e consumida na Task 5. `Event::UplinkLoss { person, fraction }` é produzida na Task 5 (lib.rs) e consumida na Task 5 (translate). `ServerMessage::UplinkLoss { fraction }` é produzida na Task 4 e consumida nas Tasks 5, 6 e 7. `Session::protocol_version` é produzida na Task 4 e consumida na Task 5.

**Uma dependência de ordem que não pode ser trocada.** A Task 2 muda `DEFAULT_BITRATE_BPS` para 48 000 e a Task 1 fixa `FAIXAS_BPS[0]` em 48 000; o teste da Task 6, passo 2, cobra que os dois concordem. Executar a Task 6 antes da Task 2 faria esse teste reprovar por um motivo que não é defeito.
