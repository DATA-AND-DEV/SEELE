# Conectividade P2P — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fazer o degrau 4 do ADR 0022 conectar de verdade, e deixar cada falha medível em vez de misteriosa.

**Architecture:** O furo de NAT já existe e abre fora de hora — o anfitrião fura por 600 ms e quem entra só chega ao endereço furado 4 a 12 segundos depois. O conserto é colar o aviso ao ponto de encontro em cada candidato que precisa dele, em vez de mandá-lo uma vez antes do laço. Em volta disso: a escada para de mentir sobre o degrau que declara, um gerente de conexão dá nome a cada etapa, e um subcomando de diagnóstico transforma relato de campo em dado.

**Tech Stack:** Rust (workspace de 10 crates), quinn 0.11.11 (QUIC), tokio, socket2, if-addrs, Opus. Frontend sem framework e sem npm (ADR 0019). Testes com `cargo test` e `cargo nextest` quando disponível.

**Spec:** `docs/superpowers/specs/2026-08-20-conectividade-p2p-design.md`

## Global Constraints

- Lints do workspace: `forbid` em `unsafe_code`, `unwrap_used`, `expect_used`, `dbg_macro`; `warn` em `indexing_slicing`. Nenhum código novo pode usá-los. Em teste, prefira `let ... else`, `matches!` e `assert!` a `unwrap`.
- ADR 0002 (regra de dependência, cobrada por `cargo xtask check-deps`): `seele-proto` → nada do workspace; `seele-audio` → `seele-proto`; `seele-core` → `seele-proto` + `seele-audio`; `seele-server` → **`seele-proto` apenas**; `seele-tui`/`seele-ffi` → `seele-core` apenas (com a exceção já registrada de `seele-tui` → `seele-server`).
- Amplificação do ponto de encontro **≤ 1:1**. Todo datagrama `SEELE-ENC/1` tem exatamente `seele_proto::encontro::TAMANHO = 96` bytes.
- **Sem relay.** O degrau 5 do ADR 0022 está fora de escopo por decisão. Não introduzir retransmissão de áudio em hipótese nenhuma.
- **Sem ICE bidirecional.** Quem entra nunca lê resposta do **ponto de encontro**. Ler o `FURO` (que vem do anfitrião) é permitido — decisão 4 do spec.
- Documentação e comentários de código em **português do Brasil**, na prosa densa e explicativa do projeto. Nomes de teste em português, no estilo `o_que_ele_afirma_quando_passa`.
- Frases que a pessoa lê moram na casca (`apps/seele-app/ui/frases.js` e `crates/seele-tui`), **nunca** no Rust do núcleo — ADR 0012 e 0023. O Rust exporta nomes estáveis, não texto.
- Toda propriedade arquitetural vira teste que reprova sozinho quando ela é quebrada.
- Trabalho direto na `main`, com commit por tarefa.

## Estrutura de arquivos

| Arquivo | Responsabilidade | Tarefas |
|---|---|---|
| `crates/seele-server/src/alcance.rs` | a escada, `Degrau`, `Tipo`, `Alcance::decidir` | 1, 2, 3 |
| `crates/seele-server/src/alcance/encontro.rs` | lado do anfitrião no degrau 4 | 4, 7 |
| `crates/seele-core/src/encontro.rs` | `Batida` — o aviso, do lado de quem entra | 6 |
| `crates/seele-core/src/enlace.rs` | laço de candidatos, prazos | 6, 7 |
| `crates/seele-core/src/chegada.rs` | **novo** — `Chegada`, `Etapa`, `Passo` | 8 |
| `crates/seele-ffi/src/lib.rs` | travessia para o app: trilha, jitter, caminho | 5, 8, 10 |
| `crates/seele-tui/src/rede.rs` | **novo** — `plug --rede` | 9 |
| `apps/seele-app/ui/frases.js` | as frases das variantes novas | 1, 8, 10 |
| `crates/seele-conformance/tests/furo.rs` | **novo** — coordenação do furo | 7 |
| `crates/seele-conformance/tests/estados.rs` | **novo** — máquina de estados | 8 |

## Ordem e paralelismo

Quatro grupos tocam arquivos disjuntos e podem correr ao mesmo tempo. Dentro de um grupo, as tarefas são **sequenciais**.

```
GRUPO A  seele-server/src/alcance.rs        Tarefa 1 → 2 → 3
GRUPO B  seele-server/src/alcance/encontro.rs   Tarefa 4
GRUPO C  seele-core (encontro, enlace, chegada)  Tarefa 6 → 7 → 8
GRUPO D  seele-ffi + frases.js               Tarefa 5 → 10
GRUPO E  seele-tui                           Tarefa 9        (depois da 3)
GRUPO F  crates/seele-udp (novo)             Tarefa 11 → 12  (depois do campo)
```

**Restrição de ordem que não é negociável:** `PACOTES_DO_FURO = 1` mora **dentro da Tarefa 7**, junto do aviso por candidato. Sozinho, ele encolhe a janela do furo de 600 ms para 120 ms sem a coordenação que a substitui, e piora o defeito que este plano existe para consertar.

**Portão de campo:** depois da Tarefa 9, pare e refaça o teste das duas casas com o roteiro da seção 8.4 do spec. As Tarefas 11 e 12 são desenhadas com o que ele medir.

---

### Tarefa 1: `Degrau::EnderecoDireto` — a VPS deixa de ler "só funciona na sua rede"

Defeito 3.2 do spec. `Alcance::decidir` não tem ramo para "a máquina tem IPv4 global", então uma VPS cai no `else` final e lê a pior frase da escada embaixo de um link que alcança o mundo.

**Files:**
- Modify: `crates/seele-server/src/alcance.rs` (enum `Degrau` ~369, `nome()` ~418, `alcanca_de_fora()` ~438, `decidir()` ~563-583)
- Modify: `apps/seele-app/ui/frases.js` (bloco dos degraus, ~226-252)
- Test: `crates/seele-server/src/alcance.rs`, `mod testes`

**Interfaces:**
- Produces: `Degrau::EnderecoDireto`, com `nome() == "EnderecoDireto"` e `alcanca_de_fora() == true`. A Tarefa 9 lê `Degrau::nome()`; a Tarefa 10 depende de a casca ter frase para toda variante.

- [ ] **Step 1: Escrever o teste que reprova**

Em `crates/seele-server/src/alcance.rs`, dentro de `mod testes`:

```rust
#[test]
fn um_endereco_publico_nao_e_um_link_que_so_funciona_na_sua_rede() {
    // Uma VPS: IPv4 global na placa, sem UPnP (não há roteador a pedir), sem
    // IPv6, sem túnel. O degrau 4 não é tentado de propósito — `subir` só
    // pergunta ao ponto de encontro quando não há IPv4 público, e numa VPS
    // perguntar seria pagar metadado por um caminho que já existe (ADR 0022).
    //
    // Antes deste conserto a escada caía no `else` final e declarava
    // `SoRedeLocal`, cuja frase manda encaminhar a porta 8383 num roteador que
    // não existe. É o defeito do relato do Cloudflare WARP com o sinal
    // invertido: lá a frase prometia demais, aqui promete de menos.
    let alcance = Alcance::decidir(
        Escuta::nova(8383, Pilha::Dupla),
        None,
        None,
        &[na_placa("45.33.32.156")],
        None,
    );

    assert_eq!(alcance.degrau(), Degrau::EnderecoDireto);
    assert!(alcance.degrau().alcanca_de_fora());
    assert!(
        alcance
            .alvos()
            .iter()
            .any(|alvo| alvo.ip().to_string() == "45.33.32.156"),
        "o endereço que dá nome ao degrau tem de estar no convite"
    );
}
```

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-server um_endereco_publico_nao_e_um_link
```

Esperado: FAIL — `no variant named EnderecoDireto found for enum Degrau`.

- [ ] **Step 3: Acrescentar a variante**

Em `crates/seele-server/src/alcance.rs`, no `enum Degrau`, **depois** de `Ipv6Direto` e **antes** de `RedeLocalOuVpn`:

```rust
    /// **Degrau 1, com endereço próprio.** A máquina tem IPv4 global na placa:
    /// uma VPS, uma máquina com IP fixo, ou uma porta já encaminhada à mão.
    ///
    /// O ADR 0022 chama isto de "o caminho de quem hospeda a sério", e é o
    /// único degrau em que nada foi pedido a ninguém — nem ao roteador, nem a um
    /// ponto de encontro. Por isso ele fica acima do 2 na frase: alcança quem
    /// tem IPv4, que é quase todo mundo.
    ///
    /// Variante própria, e não um `SoRedeLocal` de sorte, pelo critério que
    /// [`Degrau::RedeLocalOuVpn`] já usa: **o que a pessoa faz a respeito é
    /// diferente**. Aqui não há nada a fazer, e a frase que mandava encaminhar a
    /// porta num roteador inexistente era pior que silêncio.
    EnderecoDireto,
```

- [ ] **Step 4: Ensinar `nome()` e `alcanca_de_fora()`**

Em `nome()`, dentro do `match`:

```rust
            Self::EnderecoDireto => "EnderecoDireto",
```

Em `alcanca_de_fora()`, o `matches!` passa a ser:

```rust
        matches!(
            self,
            Self::PortaNoRoteador | Self::FuroDeNat | Self::Ipv6Direto | Self::EnderecoDireto
        )
```

- [ ] **Step 5: Acrescentar o ramo em `decidir()`**

Em `crates/seele-server/src/alcance.rs`, na cadeia que decide `degrau` (~linha 566), **entre** o ramo do `Ipv6Direto` e o do `RedeLocalOuVpn`:

```rust
        } else if achados.iter().any(|achado| {
            matches!(achado.ip, IpAddr::V4(quatro) if porta::global_v4(quatro))
                && achado.classe() == interfaces::Origem::Fisica
                && escuta.serve(achado.ip)
        }) {
            // A mesma conjunção do degrau 2, e pelo mesmo motivo: um degrau só
            // pode ser declarado se a escuta o servir. `tem_ipv4_global` faz
            // metade desta pergunta em `subir`, para decidir se vale bater no
            // ponto de encontro; aqui a outra metade é a escuta.
            Degrau::EnderecoDireto
```

- [ ] **Step 6: Rodar e ver passar**

```bash
cargo test -p seele-server um_endereco_publico_nao_e_um_link
```

Esperado: PASS.

- [ ] **Step 7: A frase na casca**

Em `apps/seele-app/ui/frases.js`, no bloco dos degraus, **antes** de `PortaNoRoteador`:

```js
    // Degrau 1 com endereço próprio: VPS, IP fixo, porta já encaminhada à mão.
    // O único degrau em que nada foi pedido a ninguém — nem ao roteador, nem a
    // um ponto de encontro —, e por isso o único sem ressalva nenhuma na
    // segunda linha. Nasceu de um defeito: antes dele uma VPS lia «ESTE LINK SÓ
    // FUNCIONA NA SUA REDE», que manda encaminhar a porta num roteador que não
    // existe, embaixo de um link que alcança o mundo inteiro.
    EnderecoDireto:
      "ESTA MÁQUINA TEM ENDEREÇO PRÓPRIO.\nEste link deve funcionar pela internet, sem depender de ninguém.",
```

- [ ] **Step 8: Conferir que a casca cobre toda variante**

```bash
cd apps/seele-app && npm test 2>/dev/null || node --test ui/*.test.js
```

Se houver um guarda que exige uma frase por variante de `Degrau`, ele agora passa. Se ele **não** existir, acrescente-o na Tarefa 8 (`toda_transicao_de_estado_tem_uma_frase_e_nenhuma_e_um_beco` cobre a mesma classe de defeito).

- [ ] **Step 9: Suíte inteira do crate**

```bash
cargo test -p seele-server
```

Esperado: verde. Se algum teste antigo afirmava `SoRedeLocal` para uma máquina com IPv4 global, ele estava gravando o defeito — conserte a expectativa e explique no commit.

- [ ] **Step 10: Commit**

```bash
git add crates/seele-server/src/alcance.rs apps/seele-app/ui/frases.js
git commit -m "fix(escada): uma VPS deixa de ler que o link só funciona na rede dela"
```

---

### Tarefa 2: reservar antes de truncar, e ler o degrau dos alvos que sobraram

Defeito 3.1 do spec. `alcance.rs:563` promete em comentário que *"o degrau é lido dos candidatos que sobraram"*, e o código lê `externo.is_some()`/`furado.is_some()` — variáveis de **antes** do `truncate(4)`.

**Files:**
- Modify: `crates/seele-server/src/alcance.rs` (`decidir()`, ~499-590)
- Test: `crates/seele-server/src/alcance.rs`, `mod testes`

**Interfaces:**
- Consumes: `Degrau::EnderecoDireto` da Tarefa 1.
- Produces: garantia de que `alcance.degrau()` só nomeia um degrau cujo endereço está em `alcance.alvos()`. A Tarefa 3 reescreve a montagem da lista e tem de manter isso.

- [ ] **Step 1: Escrever os dois testes que reprovam**

```rust
#[test]
fn o_endereco_furado_nunca_e_truncado_para_fora_do_convite() {
    // Ethernet e wifi ligadas, numa rede com IPv6 nativo: dois endereços de
    // ordem 0 e dois de ordem 1. São quatro, que é o `LIMITE_DE_CANDIDATOS`
    // inteiro — e o endereço furado, que é de ordem 3, cai fora da lista.
    //
    // É exatamente a máquina de casa com CGNAT, que é o único caso que o
    // degrau 4 existe para servir.
    let furado = SocketAddr::from(([200, 100, 30, 40], 61234));
    let alcance = Alcance::decidir(
        Escuta::nova(8383, Pilha::Dupla),
        None,
        Some(furado),
        &[
            na_placa("192.168.0.10"),
            na_placa("192.168.0.11"),
            na_placa("2804:388::1"),
            na_placa("2804:388::2"),
        ],
        None,
    );

    assert!(
        alcance.alvos().contains(&furado),
        "o endereço furado é insubstituível: sem ele o degrau 4 não alcança ninguém"
    );
}

#[test]
fn a_escada_so_diz_furo_de_nat_se_o_endereco_furado_estiver_no_convite() {
    // O comentário de `decidir` diz que o degrau é lido dos candidatos que
    // sobraram, "assim não há como dizer que se alcança por um endereço que não
    // está no convite". Este teste é o que cobra a frase.
    //
    // Encenado com a escuta recusando o endereço furado, que é o caminho que
    // sobra depois de a reserva impedir o truncamento: uma escuta só-IPv4 não
    // serve um alvo IPv6, e o furado sai da lista sem passar por `truncate`.
    let furado_ipv6 = SocketAddr::from((
        "2001:db8::1".parse::<Ipv6Addr>().unwrap_or(Ipv6Addr::UNSPECIFIED),
        61234,
    ));
    let alcance = Alcance::decidir(
        Escuta::nova(8383, Pilha::SoIpv4),
        None,
        Some(furado_ipv6),
        &[na_placa("192.168.0.10")],
        None,
    );

    assert_ne!(
        alcance.degrau(),
        Degrau::FuroDeNat,
        "declarar um degrau cujo endereço não está no convite é a forma \
         confiante do silêncio que o ADR 0022 existe para não produzir"
    );
}
```

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-server o_endereco_furado_nunca_e_truncado
cargo test -p seele-server a_escada_so_diz_furo_de_nat_se
```

Esperado: o primeiro FAIL (o furado foi truncado); o segundo FAIL (`FuroDeNat` declarado sem o endereço).

- [ ] **Step 3: Reservar antes de cortar**

Em `crates/seele-server/src/alcance.rs`, substituir o bloco que hoje faz `sort_by_key` → dedup → `truncate`:

```rust
        // Reservar antes de cortar. Dois endereços são insubstituíveis e não
        // podem perder a vaga para um terceiro da mesma classe:
        //
        // - o **primeiro `Local`**, porque sem ele os dois na mesma casa param
        //   de se achar — foi o que a 0.5.0 quebrou, e o ADR 0006 registra;
        // - o **furado**, porque ele é o único endereço que o degrau 4 produz, e
        //   sem ele o degrau não alcança ninguém.
        //
        // O que dá a vaga é o excedente da mesma classe: uma segunda placa, um
        // segundo IPv6. Na prática isto vira "no máximo dois `Local`".
        candidatos.sort_by_key(|(ordem, _)| *ordem);

        let mut reservados: Vec<SocketAddr> = Vec::new();
        if let Some((_, primeiro_local)) =
            candidatos.iter().find(|(ordem, _)| *ordem == 0)
        {
            reservados.push(*primeiro_local);
        }
        if let Some(furado) = furado {
            reservados.push(furado);
        }

        let mut alvos: Vec<SocketAddr> = Vec::new();
        for alvo in &reservados {
            if !alvos.contains(alvo) {
                alvos.push(*alvo);
            }
        }
        for (_, alvo) in &candidatos {
            if alvos.len() >= LIMITE_DE_CANDIDATOS {
                break;
            }
            if !alvos.contains(alvo) {
                alvos.push(*alvo);
            }
        }
        // A ordem de tentativa é a das classes, e não a da reserva: reservar é
        // sobre quem sobrevive ao corte, nunca sobre quem vem primeiro.
        let posicao = |alvo: &SocketAddr| {
            candidatos
                .iter()
                .find(|(_, candidato)| candidato == alvo)
                .map_or(u8::MAX, |(ordem, _)| *ordem)
        };
        alvos.sort_by_key(posicao);
```

- [ ] **Step 4: Ler o degrau dos alvos que sobraram**

Substituir as duas primeiras condições da cadeia de `degrau`:

```rust
        // Dos alvos que sobraram, e não das variáveis que os produziram. Sem
        // isto o comentário abaixo é uma intenção, e não uma propriedade: um
        // endereço truncado para fora do convite continuaria dando nome ao
        // degrau, e a escada ficaria reavivando um caminho que candidato nenhum
        // usa.
        let degrau = if externo.is_some_and(|alvo| alvos.contains(&alvo)) {
            Degrau::PortaNoRoteador
        } else if furado.is_some_and(|alvo| alvos.contains(&alvo)) {
            Degrau::FuroDeNat
        } else if /* … o ramo de Ipv6Direto, inalterado … */
```

- [ ] **Step 5: Rodar os dois testes**

```bash
cargo test -p seele-server o_endereco_furado_nunca_e_truncado
cargo test -p seele-server a_escada_so_diz_furo_de_nat_se
```

Esperado: PASS nos dois.

- [ ] **Step 6: A segunda truncagem, em `uri.rs`**

`crates/seele-proto/src/uri.rs` corta de novo em `LIMITE_DE_ALVOS = 4` (linhas 146 e 418). Como `decidir` agora entrega no máximo 4, o corte de lá vira inofensivo — mas isso é coincidência de números, não invariante. Acrescentar em `crates/seele-proto/src/uri.rs`, `mod testes`:

```rust
#[test]
fn o_convite_cabe_todos_os_alvos_que_a_escada_entrega() {
    // Duas truncagens na mesma direção — `LIMITE_DE_CANDIDATOS` no
    // `seele-server` e `LIMITE_DE_ALVOS` aqui — e nenhuma das duas sabe da
    // outra. Enquanto os números forem iguais nada se perde; este teste é o que
    // faz alguém que mexa num deles descobrir o outro.
    assert!(
        LIMITE_DE_ALVOS >= 4,
        "a escada entrega até 4 alvos; cortar abaixo disso descarta em silêncio"
    );
}
```

- [ ] **Step 7: Suíte inteira**

```bash
cargo test -p seele-server -p seele-proto
```

- [ ] **Step 8: Commit**

```bash
git add crates/seele-server/src/alcance.rs crates/seele-proto/src/uri.rs
git commit -m "fix(escada): o degrau declarado passa a sair dos alvos que sobraram"
```

---

### Tarefa 3: `Tipo` de candidato — a ordem sai do tipo, e não o contrário

Seção 2 do spec. Hoje o tipo **é** a ordem: `Vec<(u8, SocketAddr)>` com um `u8` anônimo que carrega a preferência inteira do projeto e some no `sort_by_key`.

**Files:**
- Modify: `crates/seele-server/src/alcance.rs` (`decidir()`, e o `Vec<(u8, SocketAddr)>`)
- Test: `crates/seele-server/src/alcance.rs`, `mod testes`

**Interfaces:**
- Consumes: a reserva da Tarefa 2.
- Produces: `pub enum Tipo` com `ordem()`, `precisa_de_furo()`, `insubstituivel()`. A Tarefa 7 usa `precisa_de_furo()` do lado de quem entra, **deduzido do convite** — não pelo enum, que vive no `seele-server` e não atravessa o ADR 0002.

- [ ] **Step 1: O teste que reprova**

```rust
#[test]
fn a_ordem_do_convite_sai_do_tipo_do_candidato() {
    // A ordem deixa de ser um número solto que alguém pode editar sem saber o
    // que ele significa. Cada posição tem nome, e o nome responde a pergunta
    // que o número não respondia: **o que este candidato precisa para
    // funcionar**.
    assert!(Tipo::Local.ordem() < Tipo::Global.ordem());
    assert!(Tipo::Global.ordem() < Tipo::PortaNoRoteador.ordem());
    assert!(Tipo::PortaNoRoteador.ordem() < Tipo::Refletido.ordem());
    assert!(Tipo::Refletido.ordem() < Tipo::Tunel.ordem());
    assert!(Tipo::Tunel.ordem() < Tipo::Ponte.ordem());

    // Só o refletido depende de alguém ter furado o caminho. É esta linha que
    // impede o aviso por candidato da Tarefa 7 de queimar a janela de furos do
    // anfitrião com quem não precisa dela.
    assert!(Tipo::Refletido.precisa_de_furo());
    for tipo in [
        Tipo::Local,
        Tipo::Global,
        Tipo::PortaNoRoteador,
        Tipo::Tunel,
        Tipo::Ponte,
    ] {
        assert!(!tipo.precisa_de_furo());
    }

    // A porta do roteador é refletida por configuração, e não por observação:
    // ela existe porque alguém pediu, não porque alguém contou de onde o pacote
    // veio. Por isso variante própria, e por isso não precisa de furo.
    assert!(Tipo::Local.insubstituivel());
    assert!(Tipo::Refletido.insubstituivel());
    assert!(!Tipo::PortaNoRoteador.insubstituivel());
}
```

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-server a_ordem_do_convite_sai_do_tipo
```

Esperado: FAIL — `cannot find type Tipo in this scope`.

- [ ] **Step 3: O enum**

Em `crates/seele-server/src/alcance.rs`, acima de `LIMITE_DE_CANDIDATOS`:

```rust
/// O que um candidato é, e o que ele precisa para funcionar.
///
/// Antes disto o tipo **era** a ordem: um `u8` solto de 0 a 5, que respondia
/// "onde ele vai na lista" e não respondia "o que ele precisa". A inversão é
/// literal — a ordem passa a ser derivada do tipo —, e o que ela destrava está
/// em [`Tipo::precisa_de_furo`]: sem essa pergunta, avisar o ponto de encontro
/// por candidato queimaria a janela de furos do anfitrião com endereços que
/// nunca dependeram de furo nenhum.
///
/// `Local` e `Global` são os *host candidates* da literatura de NAT traversal;
/// `Refletido` é o *server-reflexive*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Um endereço desta máquina, na rede de casa. O caso comum, e o único com
    /// resposta imediata.
    Local,
    /// Um endereço desta máquina que sai para a internet: IPv6 nativo, ou o
    /// IPv4 de uma VPS. Alcança de fora e também de dentro.
    Global,
    /// A porta que o roteador abriu a pedido (degrau 3). Refletido por
    /// **configuração**, e não por observação: existe porque alguém pediu.
    /// Alcança de fora, e de dentro só com *hairpin*, que muitos roteadores não
    /// fazem.
    PortaNoRoteador,
    /// O endereço que o ponto de encontro observou (degrau 4). É o único que
    /// depende de o anfitrião ter furado o caminho.
    Refletido,
    /// Um túnel: Tailscale, WireGuard, WARP. Dois pares na mesma VPN se acham
    /// por aqui, e ninguém mais.
    Tunel,
    /// Uma ponte de contêiner. Não sai desta máquina, e está aqui só porque a
    /// lista de nomes que a reconhece é heurística.
    Ponte,
}

impl Tipo {
    /// Onde ele entra na lista de tentativa.
    #[must_use]
    pub fn ordem(self) -> u8 {
        match self {
            Self::Local => 0,
            Self::Global => 1,
            Self::PortaNoRoteador => 2,
            Self::Refletido => 3,
            Self::Tunel => 4,
            Self::Ponte => 5,
        }
    }

    /// Se alguém precisa furar um NAT para este endereço atender.
    ///
    /// Só o refletido. É o que separa "avisar o ponto de encontro antes deste
    /// candidato" de "não gastar metadado nem orçamento de furo com quem não
    /// precisa" — e o caso dos dois na mesma casa não perde um milissegundo.
    #[must_use]
    pub fn precisa_de_furo(self) -> bool {
        matches!(self, Self::Refletido)
    }

    /// Se ele não pode perder a vaga no convite para outro da mesma classe.
    ///
    /// O primeiro `Local` porque sem ele os dois na mesma casa param de se
    /// achar; o `Refletido` porque ele é o único endereço que o degrau 4
    /// produz. Ver a reserva em [`Alcance::decidir`].
    #[must_use]
    pub fn insubstituivel(self) -> bool {
        matches!(self, Self::Local | Self::Refletido)
    }
}
```

- [ ] **Step 4: Trocar o `u8` pelo `Tipo` em `decidir()`**

`Vec<(u8, SocketAddr)>` vira `Vec<(Tipo, SocketAddr)>`. O `match` que hoje produz números produz variantes:

```rust
            let tipo = match (achado.classe(), achado.e_da_rede_local()) {
                (interfaces::Origem::Fisica, true) => Tipo::Local,
                (interfaces::Origem::Fisica, false) => Tipo::Global,
                (interfaces::Origem::Tunel, _) => Tipo::Tunel,
                (interfaces::Origem::Virtual, _) => Tipo::Ponte,
            };
            candidatos.push((tipo, alvo));
```

E os dois `push` de fora do laço:

```rust
        if let Some(externo) = externo {
            candidatos.push((Tipo::PortaNoRoteador, externo));
        }
        // … 
        if let Some(furado) = furado {
            candidatos.push((Tipo::Refletido, furado));
        }
```

O `sort_by_key` e o `posicao` da Tarefa 2 passam a usar `tipo.ordem()`, e a busca do primeiro `Local` passa a ser `.find(|(tipo, _)| tipo.insubstituivel() && *tipo == Tipo::Local)`.

- [ ] **Step 5: Rodar tudo**

```bash
cargo test -p seele-server
```

Esperado: PASS, incluindo os testes das Tarefas 1 e 2 sem alteração — a refatoração não muda comportamento.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server/src/alcance.rs
git commit -m "refactor(escada): a ordem do candidato passa a sair do tipo dele"
```

---

### Tarefa 4: o `AQUI` de origem estranha não vira furo — GRUPO B, paralelo a 1-3

Decisão 8 do spec. `atender` destrói o remetente em `let (lidos, _) = ...`, e forjar um `AQUI` direto na escuta de avisos é mais barato que forjar um `LEVE`, porque não passa pelo ponto de encontro.

**Files:**
- Modify: `crates/seele-server/src/alcance/encontro.rs` (`atender()` ~536-576)
- Test: `crates/seele-server/src/alcance/encontro.rs`, `mod testes`

**Interfaces:**
- Produces: `atender` passa a receber o endereço do ponto de encontro e a recusar `AQUI` de qualquer outra origem. Não muda assinatura pública — `abrir` já conhece `ponto`.

- [ ] **Step 1: O teste que reprova**

```rust
#[tokio::test]
async fn um_aqui_de_origem_estranha_nao_vira_furo() {
    // O `AQUI` é o único datagrama que faz o Dogma mandar pacote para um
    // endereço que outra pessoa escolheu. Forjá-lo direto na escuta de avisos é
    // mais barato que forjar um `LEVE`: não passa pelo ponto de encontro, então
    // nem a marca nem a janela de furos são pagas duas vezes.
    //
    // A marca continua sendo a cinta principal — quem tem o link, tem. Esta é a
    // segunda: o pacote também tem de ter vindo de onde o ponto de encontro
    // atende.
    let ponto = SocketAddr::from(([203, 0, 113, 7], PORTA_PADRAO));
    let intruso = SocketAddr::from(([198, 51, 100, 9], 9000));
    let marca = Marca::nova("abcdef0123456789").unwrap_or_else(|| {
        panic!("marca de teste tem de ser válida")
    });

    assert!(
        !aviso_e_do_ponto(intruso, ponto),
        "um AQUI que não veio do ponto de encontro não abre caminho nenhum"
    );
    assert!(aviso_e_do_ponto(ponto, ponto));
    // A porta de origem não conta: um ponto de encontro atrás de um balanceador
    // responde de porta efêmera, e recusar isso quebraria topologias legítimas
    // sem baixar a superfície de abuso — quem forja endereço forja porta.
    let mesma_maquina_outra_porta = SocketAddr::from(([203, 0, 113, 7], 40000));
    assert!(aviso_e_do_ponto(mesma_maquina_outra_porta, ponto));
    let _ = marca;
}
```

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-server um_aqui_de_origem_estranha
```

Esperado: FAIL — `cannot find function aviso_e_do_ponto`.

- [ ] **Step 3: A função de conferência**

Em `crates/seele-server/src/alcance/encontro.rs`, ao lado de `cabe_mais_um_furo`:

```rust
/// Se este aviso veio de onde o ponto de encontro atende.
///
/// A marca já separa "alguém com o convite" de "a internet batendo na porta", e
/// continua sendo a cinta principal. Esta é a segunda, e ela fecha um caminho
/// mais barato que o outro: um `AQUI` forjado direto nesta escuta não passa pelo
/// ponto de encontro, então quem o manda não paga a ida até lá.
///
/// **Compara o endereço, não a porta.** Um ponto de encontro atrás de um
/// balanceador responde de porta efêmera, e recusar isso quebraria topologias
/// legítimas sem ganhar nada: quem consegue forjar um endereço de origem forja a
/// porta junto.
fn aviso_e_do_ponto(origem: SocketAddr, ponto: SocketAddr) -> bool {
    origem.ip() == ponto.ip()
}
```

- [ ] **Step 4: Usar em `atender`**

Em `atender`, o ramo que hoje descarta a origem:

```rust
            recebido = avisos.recv_from(&mut balde) => {
                let Ok((lidos, origem)) = recebido else { continue };
                if !aviso_e_do_ponto(origem, ponto) {
                    tracing::debug!(%origem, "aviso de fora do ponto de encontro; ignorado");
                    continue;
                }
                let Some((marca, endereco)) = balde.get(..lidos).and_then(encontro::ler_aqui)
                else {
                    continue;
                };
```

O resto do ramo fica como está.

- [ ] **Step 5: Rodar**

```bash
cargo test -p seele-server um_aqui_de_origem_estranha
cargo test -p seele-server -- encontro
```

Esperado: PASS, e nenhum teste existente do módulo reprovando.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server/src/alcance/encontro.rs
git commit -m "fix(furo): um AQUI que não veio do ponto de encontro não abre caminho"
```

---

### Tarefa 5: o JITTER da tela deixa de ser sempre zero — GRUPO D, paralelo

Defeito 3.3 do spec. `session.rs:1584` manda `jitter_ms: 0.0` de propósito (o servidor não pode medir jitter, que se mede no receptor), e `seele-ffi/src/lib.rs:1171` lê o jitter **do relatório do Dogma** — ou seja, lê o zero.

**Files:**
- Modify: `crates/seele-ffi/src/lib.rs` (~1171 e o `Shared`/`snapshot`)
- Test: `crates/seele-ffi/src/lib.rs`, `mod testes`

**Interfaces:**
- Produces: `Snapshot.jitter_ms` passa a carregar o jitter de chegada da RFC 3550 medido neste receptor. A Tarefa 10 mostra esse campo.

- [ ] **Step 1: O teste que reprova**

```rust
#[test]
fn o_jitter_da_tela_nao_e_a_profundidade_do_anel() {
    // Duas grandezas com o mesmo nome, e mostrar a errada é pior que não
    // mostrar nada:
    //
    // - `worst_jitter_depth_ms` é **profundidade do anel de reprodução**, e o
    //   ADR 0028 acabou de dar um alvo a ele. Mostrá-lo como "jitter" exibiria a
    //   nossa própria reserva como ruído da rede — e uma reserva saudável
    //   apareceria na tela como problema;
    // - `SourceTelemetry::jitter_ms` é o de **chegada** (RFC 3550), medido aqui,
    //   e é o que a pessoa quer saber.
    //
    // E o que a tela lia antes deste conserto era um terceiro valor: o do
    // relatório do Dogma, que é sempre `0.0` porque o servidor não tem como
    // saber — `session.rs` diz isso em comentário desde sempre.
    // Uma reserva de anel saudável (42 ms) ao lado de um jitter de rede baixo
    // (7,5 ms): mostrar a primeira faria uma conexão boa parecer ruim.
    assert!(
        (jitter_para_a_tela(7.5, 42.0) - 7.5).abs() < 0.01,
        "a tela mostra o jitter de chegada, não a profundidade do anel"
    );
    // E nunca o zero que o Dogma manda de propósito, que era o que a tela lia.
    assert!(jitter_para_a_tela(7.5, 42.0) > 0.0);
}
```

> **Nota ao implementador:** `jitter_para_a_tela` não existe. Escreva-a como função livre — `fn jitter_para_a_tela(chegada_ms: f32, profundidade_do_anel_ms: f32) -> f32` — que devolve o primeiro argumento e ignora o segundo. Uma função que ignora um parâmetro parece boba e não é: o parâmetro existe para o segundo argumento ter de ser **escrito** por quem chamar, e para quem trocar os dois reprovar aqui em vez de na tela de outra pessoa. Documente as duas grandezas no doc-comment dela.

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-ffi o_jitter_da_tela_nao_e_a_profundidade
```

- [ ] **Step 3: Guardar o número certo**

Em `crates/seele-ffi/src/lib.rs`, no `Shared`, acrescentar um campo ao lado dos outros contadores de voz:

```rust
    /// O jitter de chegada deste receptor, em milissegundos (RFC 3550).
    ///
    /// Guardado aqui porque quem o calcula é `measure`, no laço de voz, e quem o
    /// mostra é o `snapshot`, na casca — e antes disto ele era calculado, usado
    /// no Sync Ratio e jogado fora, enquanto a tela lia o zero que o Dogma manda
    /// de propósito.
    jitter_de_chegada_ms: f32,
```

Em `measure` (~1808-1834), onde hoje `jitter_ms` recebe `worst_jitter_depth_ms`, **manter** esse cálculo para o Sync Ratio (ele é o insumo certo lá) e **acrescentar** a gravação do de chegada:

```rust
        // Duas grandezas, dois destinos. `worst_jitter_depth_ms` continua indo
        // para o Sync Ratio, que é o que ele mede: quanta reserva o anel teve.
        // O de chegada vai para a tela, que é o que a pessoa quer saber.
        if let Some(chegada) = telemetry.jitter_de_chegada_ms() {
            compartilhado.jitter_de_chegada_ms = chegada;
        }
```

> Se `SourceTelemetry` ainda não expõe o jitter de chegada, acrescente o acessor em `crates/seele-proto/src/media.rs` calculando-o pela RFC 3550 — `J += (|D(i-1,i)| - J) / 16` — a partir dos carimbos que a estrutura já guarda. Escreva o teste do acessor no mesmo commit.

- [ ] **Step 4: Ler o número certo no `snapshot`**

Em `crates/seele-ffi/src/lib.rs:1171`, trocar:

```rust
                jitter_ms: compartilhado.jitter_de_chegada_ms,
```

- [ ] **Step 5: Rodar**

```bash
cargo test -p seele-ffi
cargo test -p seele-proto
```

- [ ] **Step 6: Commit**

```bash
git add crates/seele-ffi/src/lib.rs crates/seele-proto/src/media.rs
git commit -m "fix(telemetria): o jitter da tela passa a ser o de chegada, e não zero"
```

---

### Tarefa 6: `Batida` — separar abrir o socket de mandar o aviso

Seção 2 do spec. `bater()` hoje faz três coisas: abre socket, resolve o nome, manda dois avisos. Para avisar por candidato, as duas primeiras precisam acontecer uma vez e a terceira várias.

**Files:**
- Modify: `crates/seele-core/src/encontro.rs` (todo o módulo)
- Test: `crates/seele-core/src/encontro.rs`, `mod testes`

**Interfaces:**
- Produces:
  - `pub(crate) struct Batida` — dona do socket, do destino resolvido e da marca.
  - `Batida::preparar(bilhete: &Bilhete, impressao: Option<&str>) -> Option<Batida>` — abre o socket e resolve o nome, **sem mandar pacote nenhum**.
  - `Batida::avisar(&self)` — manda **um** datagrama de 96 bytes.
  - `Batida::socket(&self) -> &std::net::UdpSocket` — para o `try_clone` por tentativa.
  - A Tarefa 7 consome as três.

- [ ] **Step 1: O teste que reprova**

```rust
#[tokio::test]
async fn preparar_abre_o_socket_e_nao_manda_pacote_nenhum() {
    // A separação existe para o aviso poder sair colado em cada candidato. Se
    // `preparar` mandasse um aviso, o primeiro candidato — que é o da rede de
    // casa e nunca precisou de furo — pagaria metadado e um furo da janela do
    // anfitrião por nada.
    //
    // O ponto de encontro deste teste é um socket nosso que nunca lê: o que se
    // afirma é que nada chegou nele.
    let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok();
    let Some(ponto) = ponto else { return };
    let Ok(onde) = ponto.local_addr() else { return };

    let bilhete = bilhete_de_teste(onde);
    let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
    let Some(batida) = batida else {
        panic!("preparar tem de dar certo com um ponto de encontro que existe");
    };

    let mut balde = [0_u8; seele_proto::encontro::TAMANHO];
    let nada = tokio::time::timeout(
        std::time::Duration::from_millis(120),
        ponto.recv_from(&mut balde),
    )
    .await;
    assert!(nada.is_err(), "preparar não manda pacote nenhum");

    // E `avisar` manda exatamente um, do tamanho fixo do protocolo.
    batida.avisar();
    let chegou = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        ponto.recv_from(&mut balde),
    )
    .await;
    let Ok(Ok((lidos, _))) = chegou else {
        panic!("avisar tem de mandar um datagrama");
    };
    assert_eq!(lidos, seele_proto::encontro::TAMANHO);
}
```

> **Nota ao implementador:** `bilhete_de_teste` e `IMPRESSAO_DE_TESTE` não existem. Monte o `Bilhete` com `ponto` apontando para `onde` e `aviso` para qualquer endereço global — `aviso` não é usado por `preparar`. `IMPRESSAO_DE_TESTE` é uma cadeia hexadecimal de ao menos 16 caracteres.

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-core preparar_abre_o_socket
```

Esperado: FAIL — `cannot find struct Batida`.

- [ ] **Step 3: O tipo**

Em `crates/seele-core/src/encontro.rs`, substituindo `bater`:

```rust
/// O aviso ao ponto de encontro, e o socket por onde ele saiu.
///
/// Antes disto havia uma função só, `bater`, que abria o socket, resolvia o nome
/// e mandava dois avisos, tudo antes do laço de candidatos. O furo abria por
/// 600 ms e o aperto de mão chegava até doze segundos depois — o defeito que
/// este ciclo existe para consertar.
///
/// A separação é o conserto: **preparar uma vez, avisar por candidato**. O
/// socket tem de ser um só porque o NAT mapeia por porta interna, e um aviso que
/// saísse de outra porta faria o anfitrião furar o caminho errado.
pub(crate) struct Batida {
    socket: tokio::net::UdpSocket,
    ponto: SocketAddr,
    datagrama: [u8; encontro::TAMANHO],
}

impl Batida {
    /// Abre o socket e resolve o ponto de encontro. **Não manda nada.**
    ///
    /// `None` quando não dá para bater — nome que não resolve, convite sem
    /// impressão digital, máquina sem rota. Nesse caso quem chama conecta como
    /// sempre conectou: nenhum endereço do convite depende disto, e o degrau 4 é
    /// o único que se perde.
    pub(crate) async fn preparar(
        bilhete: &Bilhete,
        impressao_digital: Option<&str>,
    ) -> Option<Self> {
        let marca = impressao_digital
            .and_then(|impressao| impressao.get(..16))
            .and_then(Marca::nova)?;
        let aviso = bilhete.aviso().ok()?;
        let ponto = tokio::time::timeout(PRAZO, resolver(bilhete)).await.ok()??;

        let socket = abrir_socket_local()?;
        let socket = tokio::net::UdpSocket::from_std(socket).ok()?;
        let ponto = mapear(ponto, &socket);

        let mut datagrama = [0_u8; encontro::TAMANHO];
        datagrama.copy_from_slice(&encontro::leve(aviso, &marca));

        Some(Self { socket, ponto, datagrama })
    }

    /// Manda **um** datagrama de 96 bytes ao ponto de encontro.
    ///
    /// Síncrono e sem esperar: o aviso é de mão única, não há resposta a
    /// aguardar, e quem chama tem um aperto de mão para começar. `WouldBlock` é
    /// ignorado pelo mesmo motivo que em `mandar_pelo_dogma`, do lado do
    /// servidor: um aviso perdido é coberto pela repetição do candidato.
    pub(crate) fn avisar(&self) {
        if let Err(erro) = self.socket.try_send_to(&self.datagrama, self.ponto) {
            if erro.kind() != std::io::ErrorKind::WouldBlock {
                tracing::debug!(%erro, ponto = %self.ponto, "aviso não saiu");
            }
        }
    }

    /// O socket por onde o aviso saiu.
    ///
    /// Quem conecta em seguida tem de conectar por ele: o anfitrião abriu
    /// caminho para **esta** porta, e um aperto de mão saindo de outra
    /// continuaria batendo numa porta fechada.
    pub(crate) fn socket(&self) -> &tokio::net::UdpSocket {
        &self.socket
    }
}
```

`resolver`, `abrir_socket_local` e `mapear` ficam como estão. `AVISOS` e `INTERVALO` saem.

- [ ] **Step 4: Rodar**

```bash
cargo test -p seele-core preparar_abre_o_socket
```

Esperado: PASS.

- [ ] **Step 5: Compilar o workspace (os chamadores de `bater` quebraram)**

```bash
cargo check --workspace
```

Esperado: erros em `enlace.rs:451` e `enlace.rs:1193`. **Não conserte agora** — é a Tarefa 7. Se quiser um commit verde, deixe `bater` como um invólucro temporário sobre `Batida` e remova-o na Tarefa 7.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-core/src/encontro.rs
git commit -m "refactor(furo): separar abrir o socket de mandar o aviso"
```

---

### Tarefa 7: o aviso sai colado em cada candidato — **o conserto**

Seção 2 do spec, e o defeito que originou este plano.

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (`conectar_entre_com_bilhete` ~438, `tentar_entre` ~458-520, constantes ~369)
- Modify: `crates/seele-server/src/alcance/encontro.rs` (`PACOTES_DO_FURO`)
- Test: `crates/seele-conformance/tests/furo.rs` (**novo**)

**Interfaces:**
- Consumes: `Batida::preparar`, `Batida::avisar`, `Batida::socket` da Tarefa 6.
- Produces: `tentar_entre` avisa antes de cada candidato que precisa de furo. A Tarefa 8 embrulha esse laço em `Chegada::tentar` sem mudar o comportamento.

- [ ] **Step 1: Os três testes que reprovam**

Criar `crates/seele-conformance/tests/furo.rs`:

```rust
//! A coordenação entre o aviso e o aperto de mão.
//!
//! Nenhum teste do projeto olhava para o **relógio entre as duas coisas**:
//! `candidatos.rs` prova que o próximo candidato é tentado, `apresentacao.rs`
//! prova que o aviso chega, e o intervalo entre eles não era de ninguém. Era
//! exatamente ali que o defeito morava — o furo abria por 600 ms e o aperto de
//! mão chegava até doze segundos depois.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Um ponto de encontro que só anota quando cada aviso chegou.
async fn ponto_que_anota() -> Option<(SocketAddr, Arc<Mutex<Vec<Instant>>>)> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
    let onde = socket.local_addr().ok()?;
    let quando: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let anotador = Arc::clone(&quando);
    tokio::spawn(async move {
        let mut balde = [0_u8; 96];
        while socket.recv_from(&mut balde).await.is_ok() {
            if let Ok(mut lista) = anotador.lock() {
                lista.push(Instant::now());
            }
        }
    });
    Some((onde, quando))
}

#[tokio::test]
async fn o_aviso_sai_imediatamente_antes_do_candidato_que_precisa_dele() {
    // Um convite com um endereço morto de rede local na frente e um endereço
    // "público" atrás. Antes do conserto o aviso saía no instante zero e o
    // segundo candidato só era tentado quatro segundos depois; agora o aviso
    // sai colado nele.
    let Some((ponto, quando)) = ponto_que_anota().await else { return };

    let comeco = Instant::now();
    let _ = tentar_convite_de_teste(ponto, &["10.255.255.1:8383", "203.0.113.7:8383"]).await;

    let Ok(avisos) = quando.lock() else { return };
    let Some(primeiro) = avisos.first() else {
        panic!("nenhum aviso saiu; o degrau 4 não aconteceu");
    };
    let atraso = primeiro.duration_since(comeco);
    assert!(
        atraso > Duration::from_millis(500),
        "o aviso saiu no instante zero — é o defeito de origem: ele tem de sair \
         colado ao candidato refletido, e não antes do laço (saiu em {atraso:?})"
    );
}

#[tokio::test]
async fn um_candidato_da_rede_de_casa_nao_gasta_aviso_nenhum() {
    // Um convite só com endereços privados não precisa de furo nenhum. Avisar
    // ali gastaria metadado de quem não pediu e um furo da janela do anfitrião,
    // que tem teto de vinte por dez segundos: quatro candidatos por pessoa
    // fariam cinco pessoas fecharem a janela contra gente legítima.
    let Some((ponto, quando)) = ponto_que_anota().await else { return };

    let _ = tentar_convite_de_teste(ponto, &["10.255.255.1:8383", "192.168.255.1:8383"]).await;

    let Ok(avisos) = quando.lock() else { return };
    assert!(
        avisos.is_empty(),
        "nenhum candidato privado precisa de furo, e nenhum aviso devia ter saído"
    );
}

#[tokio::test]
async fn um_convite_de_enderecos_mortos_termina_em_segundos_e_nao_em_dezenas() {
    // Quatro endereços privados de outra casa: cada um custava
    // PRAZO_POR_CANDIDATO = 4 s, e a má notícia chegava em dezesseis segundos.
    // Com o prazo curto do candidato distante o pior caso cai para poucos.
    let Some((ponto, _)) = ponto_que_anota().await else { return };

    let comeco = Instant::now();
    let _ = tentar_convite_de_teste(
        ponto,
        &[
            "10.255.255.1:8383",
            "10.255.255.2:8383",
            "10.255.255.3:8383",
            "10.255.255.4:8383",
        ],
    )
    .await;

    let gasto = comeco.elapsed();
    assert!(
        gasto < Duration::from_secs(8),
        "quatro endereços mortos levaram {gasto:?}; o prazo curto do candidato \
         distante não está sendo aplicado"
    );
}
```

> **Nota ao implementador:** `tentar_convite_de_teste(ponto, alvos)` não existe. Escreva-a no mesmo arquivo: monta um `seele://` com os `alvos` como candidatos e `enc=` apontando para `ponto`, e chama o caminho público de conexão do `seele-core`. Ela **sempre falha** em conectar — é isso que se quer: o que se mede é o relógio, não o sucesso. Os endereços `10.255.255.x` são privados e roteáveis-para-lugar-nenhum de propósito.

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-conformance --test furo
```

Esperado: o primeiro FAIL (aviso no instante zero), o segundo FAIL (aviso saiu), o terceiro FAIL (dezesseis segundos).

- [ ] **Step 3: As constantes novas**

Em `crates/seele-core/src/enlace.rs`, ao lado de `PRAZO_POR_CANDIDATO`:

```rust
/// Quanto se espera entre avisar o ponto de encontro e começar o aperto de mão.
///
/// É o que tem de caber entre o `LEVE` sair daqui e o `Initial` chegar ao NAT do
/// outro lado: uma perna até o ponto, mais uma perna do ponto até o anfitrião —
/// que somadas dão mais ou menos a ida e volta que o ADR 0022 mediu entre 20 e
/// 200 ms.
///
/// **Erra-se para baixo de propósito.** Errar para baixo custa um PTO do quinn,
/// que cabe folgado nos 4 s do candidato; errar para cima é pago sempre, por
/// todo mundo, inclusive por quem ia conectar de qualquer jeito.
const ESPERA_DO_FURO: Duration = Duration::from_millis(200);

/// Quantos avisos saem por candidato que precisa de furo, e de quanto em quanto.
///
/// Três, espaçados, **enquanto o aperto de mão corre**. É a retentativa que não
/// existia: antes eram dois avisos antes do laço, e um `AQUI` perdido custava a
/// conexão inteira em silêncio — o anfitrião nunca furava, o candidato queimava
/// os quatro segundos, e o erro que saía era o de outro endereço.
const AVISOS_POR_CANDIDATO: u8 = 3;
/// O intervalo entre eles.
const INTERVALO_DO_AVISO: Duration = Duration::from_millis(700);

/// O prazo de um candidato privado que não é desta rede.
///
/// Um `192.168.x.x` visto de outra casa não devolve ICMP nenhum: ele queima o
/// prazo inteiro. Um segundo cabe dez idas e voltas de rede local e um PTO.
///
/// **Nunca descartar, só encurtar.** Um /16 configurado à mão ou uma VPN
/// capturando a rota dão falso negativo, e falso negativo só custa velocidade.
const PRAZO_DE_CANDIDATO_DISTANTE: Duration = Duration::from_secs(1);
```

- [ ] **Step 4: Descobrir se um candidato é de outra casa**

Ainda em `enlace.rs`:

```rust
/// Se este candidato é um endereço privado que não é desta rede.
///
/// A pergunta é feita com destino: `connect` num socket UDP não manda pacote
/// nenhum, mas faz o núcleo escolher a rota, e `local_addr` conta **qual
/// endereço meu o sistema usaria para alcançar aquele destino**.
///
/// Isto não é o truque que o ADR 0022 reprovou. Lá a pergunta era "qual é o meu
/// endereço", respondida pela rota padrão, e uma VPN capturava a resposta. Aqui
/// há destino, e é exatamente o que o `connect` responde.
fn e_de_outra_casa(candidato: SocketAddr) -> bool {
    let privado = match candidato.ip() {
        IpAddr::V4(quatro) => {
            quatro.is_private() || quatro.is_link_local() || e_cgnat(quatro)
        }
        IpAddr::V6(seis) => (seis.segments().first().copied().unwrap_or(0) & 0xfe00) == 0xfc00,
    };
    if !privado {
        return false;
    }
    let Ok(sonda) = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))) else {
        return false;
    };
    if sonda.connect(candidato).is_err() {
        // Sem rota para lá: é de outra casa, e o sistema já sabe disso.
        return true;
    }
    let Ok(daqui) = sonda.local_addr() else { return false };
    !mesma_rede(daqui.ip(), candidato.ip())
}

/// `100.64.0.0/10`, que a RFC 6598 reservou para CGNAT.
fn e_cgnat(quatro: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = quatro.octets();
    a == 100 && (64..128).contains(&b)
}

/// Um /24 para IPv4 e um /64 para IPv6.
///
/// É chute quando a rede é /16, e o chute é para o lado seguro: um vizinho
/// legítimo de outra faixa cai no prazo curto e ainda tem um segundo inteiro.
fn mesma_rede(daqui: IpAddr, la: IpAddr) -> bool {
    match (daqui, la) {
        (IpAddr::V4(a), IpAddr::V4(b)) => a.octets()[..3] == b.octets()[..3],
        (IpAddr::V6(a), IpAddr::V6(b)) => a.segments()[..4] == b.segments()[..4],
        _ => false,
    }
}
```

- [ ] **Step 5: O laço**

Em `conectar_entre_com_bilhete`, trocar a chamada de `bater` por `Batida::preparar`:

```rust
        // Preparado antes do laço porque o socket tem de ser um só — o NAT mapeia
        // por porta interna. Mas **nenhum pacote sai daqui**: o aviso é por
        // candidato, e é essa mudança que conserta a corrida.
        let batida = match &bilhete {
            Some(bilhete) => {
                let impressao = destinos
                    .first()
                    .and_then(|destino| destino.impressao_esperada.as_deref());
                crate::encontro::Batida::preparar(bilhete, impressao).await
            }
            None => None,
        };
        Self::tentar_entre(destinos, batida.as_ref(), bilhete, chave, pins).await
```

Em `tentar_entre`, dentro do `for destino in ...`, antes de montar a tentativa:

```rust
            let onde = destino.servidor;
            // Só o candidato refletido depende de alguém ter furado o caminho.
            // O da rede de casa não paga metadado, não gasta furo da janela do
            // anfitrião, e não espera um milissegundo.
            let precisa_de_furo = batida.is_some() && !onde.ip().is_loopback() && e_publico(onde.ip());
            let repeticao = if precisa_de_furo {
                if let Some(batida) = batida {
                    batida.avisar();
                }
                tokio::time::sleep(ESPERA_DO_FURO).await;
                batida.map(|batida| {
                    let batida = batida.clonar_para_repetir();
                    tokio::spawn(async move {
                        for _ in 1..AVISOS_POR_CANDIDATO {
                            tokio::time::sleep(INTERVALO_DO_AVISO).await;
                            batida.avisar();
                        }
                    })
                })
            } else {
                None
            };

            let prazo = if e_de_outra_casa(onde) {
                PRAZO_DE_CANDIDATO_DISTANTE
            } else {
                PRAZO_POR_CANDIDATO
            };
```

e trocar `PRAZO_POR_CANDIDATO` por `prazo` no `tokio::time::timeout`. Depois do `match` da tentativa, encerrar a repetição:

```rust
            // A repetição para quando o candidato termina, dando certo ou não:
            // avisar sobre um candidato que já falhou gastaria furo do
            // anfitrião por um caminho que ninguém vai tentar de novo.
            if let Some(repeticao) = repeticao {
                repeticao.abort();
            }
```

> **Nota ao implementador:** `e_publico(ip)` é a negação de "privado, loopback, link-local, ULA ou CGNAT" — reaproveite a classificação de `e_de_outra_casa` em vez de duplicá-la. `Batida::clonar_para_repetir` precisa de um `Arc` interno; a saída mais simples é `Batida` guardar `Arc<tokio::net::UdpSocket>` e derivar `Clone`. Ajuste a Tarefa 6 se necessário — é o mesmo commit lógico.

- [ ] **Step 6: `PACOTES_DO_FURO` cai para 1**

**Só agora, e nunca antes.** Em `crates/seele-server/src/alcance/encontro.rs`:

```rust
/// Quantos pacotes o Dogma manda para abrir o caminho.
///
/// **Um.** Eram cinco, e os cinco nunca compraram resistência a perda: o
/// mapeamento de NAT nasce quando o pacote **sai** do roteador do anfitrião, não
/// quando chega ao outro lado. O que os cinco compravam era cobertura temporal —
/// e ela agora vem do aviso sair colado à tentativa, do outro lado.
///
/// A conta de segurança melhora junto. A origem de um UDP é forjável, então um
/// `LEVE` forjado com o endereço de uma vítima faz o Dogma mandar pacotes para
/// ela. Com cinco, o ganho era 5:1; com um, é **1:1**, que é o teto que o ADR
/// 0022 fixou. Quem repete paga 96 bytes por repetição.
const PACOTES_DO_FURO: u8 = 1;
```

`INTERVALO_DO_FURO` fica: com um pacote ele não é usado no laço, mas removê-lo apaga o registro de por que ele existia. Se o lint reclamar de constante não usada, mova a explicação para o comentário de `PACOTES_DO_FURO` e remova.

- [ ] **Step 7: O teste da amplificação**

Em `crates/seele-server/src/alcance/encontro.rs`, `mod testes`:

```rust
#[test]
fn o_furo_manda_um_pacote_por_aviso_e_nunca_mais_que_isso() {
    // A propriedade estrutural do ADR 0022: quem abusa não ganha banda. Um
    // datagrama forjado de 96 bytes faz chegar à vítima escolhida um datagrama
    // de 96 bytes, e nada além.
    //
    // Subir este número de volta para cinco reprova aqui, e é para reprovar:
    // seria trocar um conserto de conectividade por um amplificador 5:1.
    assert_eq!(PACOTES_DO_FURO, 1);
    let saida = usize::from(PACOTES_DO_FURO) * encontro::TAMANHO;
    assert!(
        saida <= encontro::TAMANHO,
        "a saída por aviso ({saida} bytes) passou do datagrama que a causou"
    );
}
```

- [ ] **Step 8: Rodar tudo**

```bash
cargo test -p seele-conformance --test furo
cargo test -p seele-server -- encontro
cargo test -p seele-core
```

Esperado: os três testes de `furo.rs` passam, e nada mais reprova.

- [ ] **Step 9: Commit**

```bash
git add crates/seele-core/src/enlace.rs crates/seele-core/src/encontro.rs \
        crates/seele-server/src/alcance/encontro.rs \
        crates/seele-conformance/tests/furo.rs
git commit -m "fix(furo): o aviso passa a sair colado em cada candidato que precisa dele"
```

---

### Tarefa 8: `Chegada` — dar nome a cada etapa, e levar a trilha até a tela

Seção 1 do spec, decisões 1 e 2.

**Files:**
- Create: `crates/seele-core/src/chegada.rs`
- Modify: `crates/seele-core/src/lib.rs` (declarar o módulo)
- Modify: `crates/seele-tui/src/main.rs:600`, `crates/seele-ffi/src/lib.rs:1542`
- Modify: `apps/seele-app/ui/frases.js`
- Test: `crates/seele-conformance/tests/estados.rs` (**novo**)

**Interfaces:**
- Consumes: `Enlace::conectar_entre_com_bilhete` (Tarefa 7).
- Produces: `Chegada::nova`, `::acompanhar`, `::chegar`, `::trilha`; `Etapa`; `Passo`. A Tarefa 10 lê `Etapa::CaminhoAberto` para a linha do caminho; a Tarefa 12 dispara uma `Chegada` nova por mudança de rede.

- [ ] **Step 1: O teste que reprova**

Criar `crates/seele-conformance/tests/estados.rs`:

```rust
//! A máquina de estados da chegada, e as arestas que não existem.

use seele_core::chegada::{Chegada, Etapa};

#[test]
fn avisar_nunca_reprova_uma_chegada() {
    // A aresta que não pode existir. Um ponto de encontro fora do ar, um nome
    // que não resolve, um convite sem impressão digital — nenhum deles pode
    // reprovar uma chegada, porque **nenhum endereço do convite depende dele**.
    //
    // Hoje isso é garantido por um prazo de 600 ms em `encontro.rs`. Aqui vira
    // uma transição que não se pode escrever, que é mais barato de conferir.
    assert!(
        !Etapa::transicao_legal(&Etapa::Avisando { ponto: String::new() }, "Desistiu"),
        "o degrau 4 é o de cima da escada: perdê-lo não perde os de baixo"
    );
    assert!(Etapa::transicao_legal(
        &Etapa::Avisando { ponto: String::new() },
        "Tentando"
    ));
}

#[tokio::test]
async fn a_trilha_sobrevive_a_uma_chegada_que_falhou() {
    // «Tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
    // recusou» é o dado que faltou quando o teste das duas casas falhou e
    // ninguém soube dizer por quê.
    //
    // Custa zero em privacidade: todo endereço da trilha já estava no convite de
    // quem a lê.
    let chegada = Chegada::nova(destinos_mortos_de_teste(), None);
    let resultado = chegada.chegar(chave_de_teste(), pins_de_teste()).await;
    assert!(resultado.is_err(), "endereços mortos não conectam");

    // A trilha é lida do erro, e não do objeto: a `Chegada` é de uso único.
    let Err(erro) = resultado else { return };
    let trilha = erro.trilha();
    assert!(
        trilha.len() >= 2,
        "uma chegada que tentou dois candidatos deixa ao menos dois passos"
    );
    assert!(
        trilha.iter().any(|passo| matches!(passo.etapa, Etapa::Desistiu(_))),
        "a trilha termina no motivo, e o motivo é o do código"
    );
}
```

> **Nota ao implementador:** `Etapa::transicao_legal` é uma função de conferência que você escreve junto do enum — ela existe para a máquina de estados ser testável sem socket nenhum. `destinos_mortos_de_teste`, `chave_de_teste` e `pins_de_teste` seguem o padrão dos auxiliares já existentes em `crates/seele-conformance/tests/`.

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-conformance --test estados
```

- [ ] **Step 3: O módulo**

Criar `crates/seele-core/src/chegada.rs` com o `enum Etapa`, `struct Passo`, `struct Chegada` e `transicao_legal`, exatamente como a seção 1 do spec descreve — incluindo `CaminhoAberto { onde }`, e **sem** a aresta `Avisando → Desistiu`.

Nesta tarefa `Chegada::chegar` **delega** a `Enlace::conectar_entre_com_bilhete` sem mover uma linha do laço, publicando `Etapa` no `watch` a cada transição. Mover o laço para dentro é a Tarefa 8b, e não é pré-requisito de nada.

- [ ] **Step 4: Declarar o módulo**

Em `crates/seele-core/src/lib.rs`:

```rust
pub mod chegada;
```

- [ ] **Step 5: Trocar as duas chamadas**

`crates/seele-tui/src/main.rs:600` e `crates/seele-ffi/src/lib.rs:1542` passam a construir uma `Chegada` e chamar `chegar`. No `seele-ffi`, acrescentar a trilha ao erro que atravessa para o app.

- [ ] **Step 6: As frases dos estados**

Em `apps/seele-app/ui/frases.js`, uma entrada por `Etapa` que a tela mostra, com o guarda que exige cobertura total — o mesmo padrão que `the_nat_punching_rung_promises_nothing_it_cannot_keep` já usa para a escada.

- [ ] **Step 7: Rodar**

```bash
cargo test -p seele-conformance --test estados
cargo test --workspace
cd apps/seele-app && node --test ui/*.test.js
```

- [ ] **Step 8: Commit**

```bash
git add crates/seele-core/src/chegada.rs crates/seele-core/src/lib.rs \
        crates/seele-tui/src/main.rs crates/seele-ffi/src/lib.rs \
        apps/seele-app/ui/frases.js crates/seele-conformance/tests/estados.rs
git commit -m "feat(chegada): cada etapa de uma conexão ganha nome, e a trilha sobrevive à falha"
```

---

### Tarefa 9: `plug --rede` — o instrumento

Seção 4 do spec. **Depois desta tarefa, pare e refaça o teste de campo.**

**Files:**
- Create: `crates/seele-tui/src/rede.rs`
- Modify: `crates/seele-tui/src/main.rs` (a análise de argumentos)
- Test: `crates/seele-tui/src/rede.rs`, `mod testes`

**Interfaces:**
- Consumes: `Degrau::nome()` (Tarefa 1), `seele_proto::encontro` (`ONDE`, `LEVE`), `seele_server::alcance::interfaces::descobrir`.
- Produces: um `ExitCode`. Nada consome esta tarefa.

- [ ] **Step 1: O teste do que a ferramenta pode afirmar**

```rust
#[test]
fn o_tipo_de_nat_e_desconhecido_com_um_ponto_de_encontro_so() {
    // Classificar cone contra simétrico exige comparar o mapeamento do **mesmo
    // socket local** visto de **dois destinos diferentes**. `ONDE` responde pelo
    // socket que recebeu e `LEVE` reflete a partir do mesmo socket: a origem de
    // todo `AQUI` é `IP-do-ponto:8384`, invariavelmente. Não há segundo ponto de
    // vista, e inventar um seria a mentira confiante que o ADR 0022 existe para
    // não produzir.
    let visto = "200.100.30.40:61234".parse().ok();
    let meus = ["192.168.1.20".parse().ok()].into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(classificar_nat(&[visto].into_iter().flatten().collect::<Vec<_>>(), &meus), Nat::Desconhecido);
}

#[test]
fn sem_nat_no_caminho_quando_o_endereco_visto_e_meu() {
    // O que uma máquina só afirma com certeza, e ainda vale: se o endereço que o
    // ponto de encontro viu é um dos endereços desta máquina, não há NAT no
    // caminho. É o degrau 1 do ADR 0022, medido em vez de deduzido.
    let visto = "45.33.32.156:61234".parse().ok();
    let meus = ["45.33.32.156".parse().ok()].into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(classificar_nat(&[visto].into_iter().flatten().collect::<Vec<_>>(), &meus), Nat::Nenhum);
}

#[test]
fn dois_pontos_com_o_mesmo_mapeamento_sao_cone() {
    let a = "200.100.30.40:61234".parse().ok();
    let b = "200.100.30.40:61234".parse().ok();
    let meus = ["192.168.1.20".parse().ok()].into_iter().flatten().collect::<Vec<_>>();
    let vistos = [a, b].into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(classificar_nat(&vistos, &meus), Nat::Cone);
}

#[test]
fn dois_pontos_com_mapeamentos_diferentes_sao_simetrico() {
    // O caso sem saída do ADR 0022, e o único que este comando consegue nomear
    // antes de a pessoa perder uma tarde tentando.
    let a = "200.100.30.40:61234".parse().ok();
    let b = "200.100.30.40:52001".parse().ok();
    let meus = ["192.168.1.20".parse().ok()].into_iter().flatten().collect::<Vec<_>>();
    let vistos = [a, b].into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(classificar_nat(&vistos, &meus), Nat::Simetrico);
}
```

- [ ] **Step 2: Rodar e ver reprovar**

```bash
cargo test -p seele-tui -- rede
```

- [ ] **Step 3: `Nat` e `classificar_nat`**

```rust
/// O que dá para dizer sobre o NAT desta máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nat {
    /// O endereço observado é desta máquina: não há NAT no caminho.
    Nenhum,
    /// Dois pontos de encontro viram o mesmo mapeamento.
    Cone,
    /// Dois pontos viram mapeamentos diferentes. É o caso que o ADR 0022 deixou
    /// sem saída: o endereço que um ponto vê não é por onde o outro lado
    /// chegaria, e a resposta a isso seria retransmissão.
    Simetrico,
    /// Um ponto de encontro só. **Não dá para saber**, e dizer que dá seria
    /// pior que calar.
    Desconhecido,
}

fn classificar_nat(vistos: &[SocketAddr], meus: &[IpAddr]) -> Nat {
    let Some(primeiro) = vistos.first() else { return Nat::Desconhecido };
    if meus.contains(&primeiro.ip()) {
        return Nat::Nenhum;
    }
    match vistos.get(1) {
        None => Nat::Desconhecido,
        Some(segundo) if segundo == primeiro => Nat::Cone,
        Some(_) => Nat::Simetrico,
    }
}
```

- [ ] **Step 4: A sonda de entrada não solicitada**

```rust
/// Se entrada de fora chega mesmo, e não só "tem chance de chegar".
///
/// `LEVE <meu próprio endereço global:esta porta>` faz o ponto de encontro
/// mandar um datagrama **não solicitado** a um socket que nunca falou com ele.
/// Se chega, entrada de fora funciona de verdade.
///
/// É o único teste que transforma o "chance, e não certeza" de
/// `Degrau::alcanca_de_fora` em fato medido — e pega **de fora** o sucesso
/// mentiroso do CGNAT, que hoje só é pego por heurística sobre o endereço WAN.
///
/// Limite honesto, e ele fica na saída: prova que 96 bytes daquela origem
/// chegaram àquela porta, não que o aperto de mão QUIC sobe.
async fn entrada_de_fora_chega(
    socket: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    meu_global: SocketAddr,
    marca: &Marca,
) -> bool {
    let datagrama = encontro::leve(meu_global, marca);
    if socket.send_to(&datagrama, ponto).await.is_err() {
        return false;
    }
    let mut balde = [0_u8; encontro::TAMANHO];
    tokio::time::timeout(Duration::from_secs(1), socket.recv_from(&mut balde))
        .await
        .is_ok_and(|recebido| recebido.is_ok())
}
```

- [ ] **Step 5: O relatório**

Imprimir exatamente o formato da seção 4 do spec, em português, uma linha por fato. `plug --rede --esperar` e `plug --rede <bilhete>` são o modo par. **Fora do modo par, a linha do furo diz `não testado`, nunca `FALHOU`.**

- [ ] **Step 6: Ligar em `main.rs`**

O caminho sai **antes** de o terminal alternativo abrir e termina em `ExitCode`. Nenhuma dependência nova entra em `seele-server` por causa disto.

- [ ] **Step 7: Rodar de verdade**

```bash
cargo test -p seele-tui -- rede
cargo run -p seele-tui -- --rede
cargo xtask check-deps
```

- [ ] **Step 8: Commit**

```bash
git add crates/seele-tui/src/rede.rs crates/seele-tui/src/main.rs
git commit -m "feat(rede): plug --rede diz o que esta máquina alcança, e o que não dá para saber"
```

- [ ] **Step 9: PORTÃO DE CAMPO**

Refazer o teste das duas casas com o roteiro da seção 8.4 do spec. Registrar: as duas saídas de `plug --rede`, a frase do anfitrião, o link inteiro, a trilha carimbada da `Chegada`, e o `--barulhento` da VPS com carimbo. **As Tarefas 11 e 12 são desenhadas com o que isso medir.**

---

### Tarefa 10: o caminho e os números na tela — GRUPO D

Seção 5 do spec.

**Files:**
- Modify: `crates/seele-ffi/src/lib.rs` (`Snapshot`)
- Modify: `apps/seele-app/ui/frases.js` e o rodapé `.telemetria`
- Test: `crates/seele-ffi/src/lib.rs`, `mod testes`

**Interfaces:**
- Consumes: `Etapa::CaminhoAberto` e a trilha (Tarefa 8); `jitter_de_chegada_ms` (Tarefa 5).
- Produces: `Snapshot.caminho: Option<&'static str>` com um de `"RedeLocal"`, `"Ipv6Direto"`, `"EnderecoPublico"`, `"FuroDeNat"`.

- [ ] **Step 1: O teste**

```rust
#[test]
fn sem_saber_o_caminho_a_casca_nao_escreve_nada() {
    // «DIRECT» não é dizível: a escada tem cinco degraus, e a distinção que essa
    // palavra apagaria é justamente a que importa — em `FuroDeNat` a conversa é
    // direta **e** alguém soube que ela existe.
    //
    // Inventar um nome quando não se sabe é a mentira confiante que o ADR 0022
    // existe para não produzir. `None` é a resposta certa, e a casca cala.
    assert_eq!(caminho_de(&[]), None);
    assert_eq!(
        caminho_de(&[passo_de_teste(Etapa::Dentro)]),
        None,
        "chegar não diz por onde; quem diz é a trilha"
    );
}
```

- [ ] **Step 2 a 5:** derivar `caminho` da trilha, expor no `Snapshot`, escrever as quatro frases em `frases.js`, e pôr a linha ao lado dos números no rodapé `.telemetria` — escrita uma vez e calada depois. Só a degradação vira frase, e a frase diz o que fazer.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-ffi/src/lib.rs apps/seele-app/ui/
git commit -m "feat(telemetria): a tela diz por qual caminho a conversa saiu"
```

---

### Tarefa 11: `seele-udp` — o demultiplexador

Seção 6 do spec. **Só depois do portão de campo da Tarefa 9.** Esta é a peça mais delicada do ciclo: um demux errado engole pacote QUIC e o sintoma é conexão que trava sem erro nenhum.

**Files:**
- Create: `crates/seele-udp/Cargo.toml`, `crates/seele-udp/src/lib.rs`
- Modify: `Cargo.toml` (membros do workspace), `xtask/src/check_deps.rs`
- Test: `crates/seele-udp/src/lib.rs`, `mod testes`

**Interfaces:**
- Produces: `Peneira` (implementa `quinn::AsyncUdpSocket`), `enum Peneirado { Encontro(..., SocketAddr), Quic }`, e um canal de saída dos datagramas peneirados.

- [ ] **Step 1: Os dois testes que cobram o risco**

```rust
#[test]
fn noventa_e_seis_bytes_de_quic_forjado_chegam_ao_quinn() {
    // A assimetria que decide toda dúvida: **falso positivo derruba a conexão
    // sem erro nenhum** — o quinn nunca vê o pacote e a conexão morre por tempo
    // ocioso —, enquanto **falso negativo só desperdiça um datagrama**, que é o
    // que já acontece hoje. Na dúvida, entrega ao quinn.
    //
    // Um QUIC de cabeçalho longo sempre começa com byte >= 0x80. `S` é 0x53,
    // válido só em pacote 1-RTT — e ali os bytes 1..9 são o Connection ID que
    // nós mesmos sorteamos.
    let mut forjado = [0x53_u8; 96];
    forjado[1..12].copy_from_slice(b"EELE-ENC/1 ");
    // Verbo que não existe: a terceira metade da conjunção salva o pacote.
    forjado[12..16].copy_from_slice(b"XPTO");
    assert_eq!(peneirar(&forjado), Peneirado::Quic);

    // E um de 95 bytes com o prefixo certo também: o tamanho é fixo.
    let quase = [0x53_u8; 95];
    assert_eq!(peneirar(&quase), Peneirado::Quic);
}

#[test]
fn um_lote_gro_misturado_e_peneirado_segmento_a_segmento() {
    // Com GRO ligado, um `RecvMeta` descreve até 64 datagramas num buffer só,
    // delimitados por `stride`. Olhar apenas o começo do buffer é o defeito que
    // quase nunca acontece — o kernel só coalesce mesmo 4-tupla e mesmo tamanho,
    // e um FURO de 96 bytes ao lado de um Initial de 1200 já quebra o lote.
    //
    // "Quase nunca" é exatamente o defeito que aparece na máquina de outra
    // pessoa, e por isso ele tem teste com lote sintético.
    let furo = seele_proto::encontro::furo(&marca_de_teste());
    let mut lote = Vec::new();
    lote.extend_from_slice(&furo);
    lote.extend_from_slice(&[0x53_u8; 96]); // não é SEELE-ENC: é do quinn
    lote.extend_from_slice(&furo);

    let (para_o_quinn, peneirados) = peneirar_lote(&lote, 96);
    assert_eq!(peneirados.len(), 2, "os dois FURO saíram do lote");
    assert_eq!(para_o_quinn.len(), 96, "o do quinn foi compactado e sobrou");
}
```

- [ ] **Step 2 a 7:** o crate, o envelope sobre `quinn_udp::UdpSocketState`, os três padrões sobrescritos (`max_transmit_segments`, `max_receive_segments`, `may_fragment`), a peneira por segmento, a contagem por classe exposta como métrica, e a troca de `Endpoint::new` por `new_with_abstract_socket` nos dois lados.

- [ ] **Step 8: Commit**

```bash
git add crates/seele-udp Cargo.toml xtask/src/check_deps.rs
git commit -m "feat(udp): o socket do Dogma deixa de ser cego"
```

---

### Tarefa 12: reconexão por sinal de rede

Seção 7 do spec, decisão 6. **Depois da Tarefa 11.**

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` (o `Motor`), `crates/seele-core/src/battery.rs`
- Modify: `crates/seele-server/src/alcance.rs` e `alcance/porta.rs` (o lado do anfitrião)
- Test: `crates/seele-conformance/tests/voz_na_reconexao.rs` (vizinho)

- [ ] **Step 1: Os testes**

```rust
#[tokio::test]
async fn a_reconexao_recebe_a_lista_inteira_de_candidatos() {
    // `enlace.rs:1186` reconectava com `self.destino.servidor`, um endereço só:
    // a primeira entrada tinha escada e a reconexão não tinha. Numa rede que
    // mudou é justamente o candidato certo que muda — o da rede local deixa de
    // valer, o público passa a valer.
    //
    // Com a `Chegada` de uso único isto sai de graça: o `Motor` constrói uma
    // nova com a lista inteira.
    let motor = motor_de_teste_com_candidatos(3);
    let chegada = motor.chegada_para_reconectar();
    assert_eq!(chegada.quantos_candidatos(), 3);
}

#[tokio::test]
async fn uma_rede_nova_refaz_o_furo_antes_de_tentar_o_aperto_de_mao() {
    // Socket novo, porta nova, e o caminho que o anfitrião abriu era para a
    // porta anterior. `rebind` não serve aqui: o furo é por porta.
    let (ponto, quando) = ponto_que_anota().await;
    let motor = motor_de_teste_conectado(ponto).await;
    motor.rede_mudou();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!quando.lock().map(|l| l.is_empty()).unwrap_or(true));
}
```

- [ ] **Step 2 a 6:** a tarefa de `if-addrs` a cada 2 s com resumo, o evento `RedeMudou` entrando na `Chegada`, o contador de ciclos por sessão (para um cabo mal encostado não virar tempestade de furos), o lado do anfitrião devolvendo o mapeamento UPnP e subindo a escada de novo, e o `seele://` regenerado com aviso na tela.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-core/src/enlace.rs crates/seele-core/src/battery.rs \
        crates/seele-server/src/alcance.rs crates/seele-server/src/alcance/porta.rs \
        crates/seele-conformance/tests/voz_na_reconexao.rs
git commit -m "feat(reconexao): uma rede nova refaz o furo em vez de esperar o ping morrer"
```

---

## Autorrevisão

**Cobertura do spec:** §1 → Tarefa 8. §2 → Tarefas 3, 6, 7. §3.1 → Tarefa 2. §3.2 → Tarefa 1. §3.3 → Tarefa 5. §4 → Tarefa 9. §5 → Tarefa 10. §6 → Tarefa 11. §7 → Tarefa 12. §8.3 → testes distribuídos por tarefa. Decisão 8 (origem do `AQUI`) → Tarefa 4. Sem lacunas.

**Consistência de tipos:** `Tipo` (Tarefa 3) fica no `seele-server` e **não atravessa** para o `seele-core`, que deduz `precisa_de_furo` do endereço — o ADR 0002 proíbe a travessia, e a Tarefa 7 diz isso explicitamente. `Batida` (6) é consumida em 7 com a mesma assinatura. `Etapa`/`Passo` (8) são consumidos em 10 e 12. `Degrau::EnderecoDireto` (1) é lido em 9 via `nome()`.

**O que este plano não cobre, por decisão do spec:** relay (degrau 5), ICE bidirecional, campo `tipos=` no `seele://`, namespaces de rede em CI, segundo endereço no ponto de encontro do projeto, e compartilhamento de tela — que é o ciclo seguinte, com spec própria.
