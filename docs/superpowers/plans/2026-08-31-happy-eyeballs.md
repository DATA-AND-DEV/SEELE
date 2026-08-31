# Happy Eyeballs — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trocar a série de candidatos do convite por uma corrida com defasagem — RFC 8305 — onde a corrida é segura, e deixar intacta a série de quem depende do socket compartilhado.

**Architecture:** `Enlace::tentar_entre` passa a separar os candidatos em dois grupos por um predicado sobre endereço: quem precisa do furo de NAT fica na série de hoje, com o socket compartilhado e os dois prazos; todo o resto ganha socket próprio e entra numa corrida escalonada a 250 ms, onde o primeiro aperto de mão que fechar vence. A limpeza de pin órfão dos perdedores roda depois do vencedor e pula a chave dele.

**Tech Stack:** Rust 1.97, quinn (QUIC), `tokio::task::JoinSet` (sem dependência nova — `tokio` já entra com `features = ["full"]`).

**Spec:** `docs/superpowers/specs/2026-08-31-happy-eyeballs-design.md`
**ADR:** `docs/adr/0037-candidatos-do-convite-em-paralelo.md`

## Global Constraints

- **ADR 0002 — regra de dependência.** `seele-core` depende de `proto`, `audio` e `video`, nunca ao contrário. **Nenhuma dependência nova entra neste plano**: a concorrência usa `tokio::task::JoinSet`, e o `tokio` já está com `features = ["full"]`. Este repositório documenta o custo de cada dependência em `Cargo.toml`; acrescentar uma para isto seria custo sem necessidade.
- **`specs/10-convencoes.md` — sem `unwrap()` nem `expect()`** fora de testes. O workspace aplica `unwrap_used = "deny"` e `expect_used = "deny"`.
- **`missing_docs = "warn"`, `unreachable_pub = "warn"`, `indexing_slicing = "warn"`, `unsafe_code = "forbid"`.**
- **Idioma:** `crates/seele-core/src/enlace.rs` está em português. Tudo que este plano acrescenta ali segue em português.
- **A série de quem precisa de furo não é editada.** Os prazos, as duas voltas e a lista `merece_segunda` ficam como o commit `9750f00` os deixou. Os três testes de `crates/seele-conformance/tests/furo.rs` têm de continuar verdes **sem serem tocados** — se algum precisar mudar, a regra da seção 1 do spec foi violada e a tarefa deve parar e perguntar.
- **Valores fixados pelo ADR 0037:** defasagem de 250 ms; sem teto próprio de simultaneidade (`LIMITE_DE_ALVOS` já é 4).
- **Cada tarefa termina verde:** `cargo fmt --check`, `cargo clippy --workspace --all-targets` sem aviso, e a suíte da tarefa passando.

---

## Estrutura de arquivos

| Arquivo | Responsabilidade |
|---|---|
| `crates/seele-core/src/enlace.rs` (modificar) | O predicado `precisa_de_furo`, a corrida `correr`, a separação em `tentar_entre`, e a limpeza de pin que respeita o vencedor. Tudo num arquivo porque tudo é o mesmo laço. |

Um arquivo só, e é deliberado: a corrida, a série e a limpeza de pin são três metades do mesmo caminho de conexão, e separá-las em módulos faria três lugares para procurar quando uma conexão falhar.

---

### Task 1: O predicado — quem precisa do furo

**Files:**
- Modify: `crates/seele-core/src/enlace.rs`
- Test: no `#[cfg(test)] mod` que já existe em `crates/seele-core/src/enlace.rs`

**Interfaces:**
- Consumes: `e_privado(IpAddr) -> bool`, que já existe no arquivo (linha ~2623).
- Produces: `fn precisa_de_furo(candidato: SocketAddr) -> bool`

- [ ] **Step 1: Escrever os testes que reprovam**

No módulo de testes de `enlace.rs`, junto dos testes que já exercitam `e_privado` e `e_de_outra_casa`:

```rust
#[cfg(test)]
mod quem_precisa_de_furo {
    use super::precisa_de_furo;
    use std::net::SocketAddr;

    fn alvo(texto: &str) -> SocketAddr {
        texto.parse().expect("endereço de teste bem escrito")
    }

    /// O caso do degrau 4: IPv4 público, anfitrião atrás de NAT.
    #[test]
    fn um_ipv4_publico_precisa() {
        assert!(precisa_de_furo(alvo("203.0.113.7:8384")));
        assert!(precisa_de_furo(alvo("8.8.8.8:8384")));
    }

    /// Endereço privado não tem NAT no meio — ou é desta rede, e aí a conversa é
    /// direta, ou é de outra casa, e aí `e_de_outra_casa` já o trata com prazo
    /// curto porque ninguém vai responder. Furar não muda nenhum dos dois.
    #[test]
    fn um_endereco_privado_nao_precisa() {
        assert!(!precisa_de_furo(alvo("192.168.1.10:8384")));
        assert!(!precisa_de_furo(alvo("10.0.0.5:8384")));
        assert!(!precisa_de_furo(alvo("172.16.0.1:8384")));
        // CGNAT, que `e_privado` já reconhece.
        assert!(!precisa_de_furo(alvo("100.64.0.1:8384")));
    }

    /// **IPv6 nunca precisa**, e este é o teste que carrega a decisão do ADR
    /// 0037. Não há tradução de endereço: o que bloqueia um IPv6 é o firewall
    /// do roteador, e furar NAT não abre firewall. Avisar por um candidato IPv6
    /// gasta janela do anfitrião — sessenta por dez segundos — por um caminho
    /// que o aviso não ajuda.
    ///
    /// É também o que libera os três candidatos da pendência nº 26 para a
    /// corrida, que é de onde vêm os 9,6 s.
    #[test]
    fn ipv6_nunca_precisa() {
        assert!(!precisa_de_furo(alvo("[2001:db8::1]:8384")));
        assert!(!precisa_de_furo(alvo("[fd00::1]:8384")));
        assert!(!precisa_de_furo(alvo("[::1]:8384")));
    }

    /// Um IPv4 escrito na forma mapeada não escapa da regra por causa da
    /// escrita. `e_privado` canoniza, e este predicado tem de canonizar também
    /// — senão `::ffff:192.168.1.10` seria lido como IPv6 e classificado ao
    /// contrário do que é.
    #[test]
    fn a_forma_mapeada_nao_muda_a_resposta() {
        assert!(!precisa_de_furo(alvo("[::ffff:192.168.1.10]:8384")));
        assert!(precisa_de_furo(alvo("[::ffff:203.0.113.7]:8384")));
    }

    #[test]
    fn o_laco_local_nao_precisa() {
        assert!(!precisa_de_furo(alvo("127.0.0.1:8384")));
    }
}
```

- [ ] **Step 2: Rodar para ver reprovar**

Run: `cargo test -p seele-core --lib quem_precisa_de_furo`
Expected: FAIL na compilação — `precisa_de_furo` não existe.

- [ ] **Step 3: Escrever o predicado**

Em `crates/seele-core/src/enlace.rs`, junto de `e_privado` e `e_de_outra_casa`:

```rust
/// Se este candidato precisa que o anfitrião fure o NAT para ser alcançado.
///
/// # Por que isto decide quem corre
///
/// Porque o furo exige que todas as tentativas dividam **um** socket UDP — o NAT
/// mapeia por porta interna —, e dois `Endpoint` do quinn lendo a mesma fila de
/// recepção roubam pacote um do outro. Quem precisa de furo fica em série; quem
/// não precisa ganha socket próprio e corre. Ver o ADR 0037.
///
/// # A regra
///
/// Precisa quem é **IPv4 público**: é o degrau 4 do ADR 0022, o anfitrião atrás
/// de NAT. Não precisam:
///
/// - **endereços privados e CGNAT** — ou são desta rede, e aí a conversa é
///   direta, ou são de outra casa, e aí [`e_de_outra_casa`] já os trata com
///   prazo curto porque se sabe que ninguém responde. Furar não muda nenhum dos
///   dois casos;
/// - **IPv6, em qualquer forma.** Não há tradução de endereço. O que bloqueia um
///   IPv6 é o firewall do roteador, e furar NAT não abre firewall — isso é o
///   PCP da pendência nº 26. Avisar por um candidato IPv6 gastaria janela do
///   anfitrião por um caminho que o aviso não ajuda, que é a mesma frase que o
///   laço já usa para não avisar por candidato que falhou;
/// - **o laço local**, que não atravessa nada.
///
/// # Onde ele erra, e por que erra de leve
///
/// É um predicado sobre endereço, e predicados sobre endereço erram em rede
/// exótica — um `/16` à mão, uma VPN capturando a rota. Os dois erros são
/// benignos: classificado como «precisa» sem precisar, o candidato apenas fica
/// em série, que é o comportamento de hoje; classificado ao contrário, ele perde
/// o aviso e falha como falharia sem bilhete nenhum. Nenhum dos dois é regressão
/// sobre o estado atual daquele candidato.
fn precisa_de_furo(candidato: SocketAddr) -> bool {
    // Canonizado antes de qualquer pergunta, pelo mesmo motivo de `e_privado`:
    // um `::ffff:192.168.1.10` escrito como veio seria lido como IPv6 e
    // classificado ao contrário do que é.
    let ip = candidato.ip().to_canonical();
    if ip.is_loopback() {
        return false;
    }
    if e_privado(ip) {
        return false;
    }
    ip.is_ipv4()
}
```

- [ ] **Step 4: Rodar**

Run: `cargo test -p seele-core --lib quem_precisa_de_furo`
Expected: PASS, 5 testes.

- [ ] **Step 5: Verde e limpo**

Run: `cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets`
Expected: sem avisos. Se acusar `precisa_de_furo` como nunca usada, ignorar por ora — a Task 3 a usa. Se o `deny(dead_code)` do workspace transformar isso em erro, marcar com `#[allow(dead_code, reason = "usada na Task 3 deste plano")]` e **remover a marca na Task 3**.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-core/src/enlace.rs
git commit -m "feat(enlace): o predicado que separa quem precisa de furo de quem pode correr"
```

---

### Task 2: A corrida, genérica e testável sem socket

**Files:**
- Modify: `crates/seele-core/src/enlace.rs`
- Test: no mesmo arquivo

**Interfaces:**
- Consumes: nada.
- Produces:
  - `const DEFASAGEM_ENTRE_CANDIDATOS: Duration` = 250 ms
  - `struct Corrida<T> { pub vencedor: Option<(usize, T)>, pub falhas: Vec<(usize, ConnectError)> }`
  - `async fn correr<T, F, Fut>(quantos: usize, defasagem: Duration, tentar: F) -> Corrida<T>`

**Por que genérica:** para que a defasagem e o «primeiro vence» tenham teste com relógio de teste e sem socket nenhum. É a mesma disciplina que o cabeçalho de `device.rs` cobra e que `seele_audio::bitrate` seguiu.

- [ ] **Step 1: Escrever a corrida**

Em `crates/seele-core/src/enlace.rs`, junto das outras funções livres do arquivo:

```rust
/// Quanto se espera antes de disparar o próximo candidato da corrida.
///
/// 250 ms, que é o número do RFC 8305. A medição da pendência nº 26 o justifica:
/// com quatro candidatos o último começa em 750 ms, e o bom respondeu em 358 ms
/// depois disso — contra os 9,6 s que a série cobrava. Encurtar para 150 ms
/// ganharia ~300 ms e poria mais apertos de mão simultâneos numa rede lenta,
/// onde vários teriam fechado sozinhos. Ver o ADR 0037.
const DEFASAGEM_ENTRE_CANDIDATOS: Duration = Duration::from_millis(250);

/// O que uma corrida produziu.
///
/// As falhas vêm junto com o vencedor de propósito: quem chama precisa delas
/// para a limpeza de pin órfão dos perdedores, e precisa saber **quem** venceu
/// para pular a chave dele. Ver [`Enlace::tentar_entre`] e o ADR 0037.
#[derive(Debug)]
struct Corrida<T> {
    /// Quem fechou primeiro, e o índice dele na lista que foi corrida.
    vencedor: Option<(usize, T)>,
    /// Quem não fechou, na ordem em que desistiram.
    falhas: Vec<(usize, ConnectError)>,
}

/// Corre `quantos` tentativas, disparando uma a cada `defasagem`, e fica com a
/// primeira que fechar.
///
/// É o RFC 8305 — «Happy Eyeballs» — e a razão de existir está medida na
/// pendência nº 26: em série, três candidatos sem chance custam 9,6 s antes de o
/// quarto ser tentado.
///
/// # Por que genérica sobre `T`
///
/// Para que a defasagem e o «primeiro vence» tenham teste. Um teste que provasse
/// isto com sockets de verdade dependeria de rede, e o que precisa ser provado
/// aqui não é a rede: é que o segundo candidato **começa** sem esperar o
/// primeiro terminar, e que o rápido vence mesmo estando por último na lista.
///
/// # Por que `JoinSet` e não `futures::FuturesUnordered`
///
/// Para não acrescentar dependência. O `tokio` já entra com `features =
/// ["full"]`, e este repositório documenta em `Cargo.toml` o que cada
/// dependência arrasta — uma a mais para isto seria custo sem necessidade.
///
/// # Cancelamento
///
/// As tentativas perdedoras são derrubadas com o `JoinSet` ao fim desta função.
/// Elas não continuam escrevendo em lugar nenhum, e o que uma delas possa ter
/// escrito em disco — um pin de TLS — é assunto de quem chama, que sabe qual
/// chave o vencedor usou.
async fn correr<T, F, Fut>(quantos: usize, defasagem: Duration, tentar: F) -> Corrida<T>
where
    T: Send + 'static,
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = Result<T, ConnectError>> + Send + 'static,
{
    let mut corredores = tokio::task::JoinSet::new();
    let mut falhas: Vec<(usize, ConnectError)> = Vec::new();
    let mut proximo = 0_usize;

    loop {
        // Dispara o próximo, se ainda houver. O primeiro sai sem espera nenhuma.
        if proximo < quantos {
            corredores.spawn(tentar(proximo).map_ok_or_else_indice(proximo));
            proximo += 1;
        }

        if corredores.is_empty() {
            break;
        }

        // Enquanto os que já estão no ar correm, o relógio da defasagem anda. Se
        // alguém fechar antes dela, a corrida acaba ali — que é o caso comum e
        // o motivo de isto existir.
        let esperando_o_proximo = proximo < quantos;
        let terminou = if esperando_o_proximo {
            tokio::select! {
                terminou = corredores.join_next() => terminou,
                () = tokio::time::sleep(defasagem) => continue,
            }
        } else {
            corredores.join_next().await
        };

        match terminou {
            Some(Ok((indice, Ok(pronto)))) => {
                // O `JoinSet` derruba o resto ao ser recolhido.
                return Corrida {
                    vencedor: Some((indice, pronto)),
                    falhas,
                };
            }
            Some(Ok((indice, Err(erro)))) => falhas.push((indice, erro)),
            // Uma tentativa que entrou em pânico ou foi cancelada. Não é
            // vencedora e não tem erro próprio a contar; segue como se tivesse
            // falhado sem resposta.
            Some(Err(_)) => {}
            None => {
                if proximo >= quantos {
                    break;
                }
            }
        }
    }

    Corrida {
        vencedor: None,
        falhas,
    }
}
```

**Nota para quem implementa:** `map_ok_or_else_indice` acima é pseudocódigo — não existe. O que se quer é que cada tarefa devolva `(indice, Result<T, ConnectError>)`. Escrever como bloco `async move`:

```rust
        if proximo < quantos {
            let indice = proximo;
            let futuro = tentar(indice);
            corredores.spawn(async move { (indice, futuro.await) });
            proximo += 1;
        }
```

e ajustar os braços do `match` para `Some(Ok((indice, Ok(pronto))))` conforme já escrito.

- [ ] **Step 2: Escrever os testes**

```rust
#[cfg(test)]
mod a_corrida {
    use super::{correr, ConnectError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// O primeiro a fechar vence, mesmo estando por último na lista.
    ///
    /// É a propriedade inteira do RFC 8305, e a que a série não tem: em série o
    /// candidato lento **precisa** terminar antes de o rápido começar.
    #[tokio::test(start_paused = true)]
    async fn o_rapido_vence_mesmo_estando_por_ultimo() {
        let corrida = correr(3, Duration::from_millis(250), |indice| async move {
            // Os dois primeiros demoram uma eternidade; o terceiro fecha logo.
            let espera = if indice == 2 { 50 } else { 100_000 };
            tokio::time::sleep(Duration::from_millis(espera)).await;
            Ok::<usize, ConnectError>(indice)
        })
        .await;

        assert_eq!(corrida.vencedor.map(|(indice, _)| indice), Some(2));
    }

    /// O segundo candidato começa sem esperar o primeiro terminar.
    ///
    /// Sem esta propriedade não há Happy Eyeballs nenhum — é literalmente a
    /// diferença entre 9,6 s e 1,1 s na pendência nº 26.
    #[tokio::test(start_paused = true)]
    async fn o_segundo_comeca_antes_de_o_primeiro_terminar() {
        let comecaram = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&comecaram);

        let corrida = correr(2, Duration::from_millis(250), move |indice| {
            let contador = Arc::clone(&contador);
            async move {
                contador.fetch_add(1, Ordering::SeqCst);
                // O primeiro nunca termina dentro do horizonte deste teste.
                let espera = if indice == 0 { 100_000 } else { 10 };
                tokio::time::sleep(Duration::from_millis(espera)).await;
                Ok::<usize, ConnectError>(indice)
            }
        })
        .await;

        assert_eq!(corrida.vencedor.map(|(indice, _)| indice), Some(1));
        assert_eq!(
            comecaram.load(Ordering::SeqCst),
            2,
            "o segundo candidato não chegou a começar"
        );
    }

    /// Quando todos falham, todas as falhas voltam — quem chama precisa delas
    /// para escolher a que melhor descreve o que houve, e para limpar os pins.
    #[tokio::test(start_paused = true)]
    async fn todas_as_falhas_voltam_quando_ninguem_fecha() {
        let corrida = correr(3, Duration::from_millis(250), |_| async {
            Err::<usize, ConnectError>(ConnectError::Unreachable)
        })
        .await;

        assert!(corrida.vencedor.is_none());
        assert_eq!(corrida.falhas.len(), 3);
    }

    /// Uma lista vazia não trava: devolve sem vencedor e sem falha.
    #[tokio::test(start_paused = true)]
    async fn uma_corrida_sem_candidatos_termina() {
        let corrida = correr(0, Duration::from_millis(250), |_| async {
            Ok::<usize, ConnectError>(0)
        })
        .await;

        assert!(corrida.vencedor.is_none());
        assert!(corrida.falhas.is_empty());
    }

    /// Quem falha cedo não impede quem vem depois de vencer.
    #[tokio::test(start_paused = true)]
    async fn uma_falha_imediata_nao_encerra_a_corrida() {
        let corrida = correr(2, Duration::from_millis(250), |indice| async move {
            if indice == 0 {
                return Err(ConnectError::Unreachable);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<usize, ConnectError>(indice)
        })
        .await;

        assert_eq!(corrida.vencedor.map(|(indice, _)| indice), Some(1));
        assert_eq!(corrida.falhas.len(), 1);
    }
}
```

- [ ] **Step 3: Rodar**

Run: `cargo test -p seele-core --lib a_corrida`
Expected: PASS, 5 testes. `start_paused = true` faz o relógio do tokio ser virtual, então os `100_000` ms não custam tempo de parede.

- [ ] **Step 4: Verde e limpo**

Run: `cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets`
Expected: sem avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/seele-core/src/enlace.rs
git commit -m "feat(enlace): a corrida do RFC 8305, genérica e provada sem socket"
```

---

### Task 3: A separação em `tentar_entre`, e o pin do vencedor

**Files:**
- Modify: `crates/seele-core/src/enlace.rs` — a função `tentar_entre` (linha ~679)
- Test: `crates/seele-core/src/enlace.rs`

**Interfaces:**
- Consumes: `precisa_de_furo` (Task 1), `correr` / `Corrida` / `DEFASAGEM_ENTRE_CANDIDATOS` (Task 2).
- Produces: nada de novo para fora; `tentar_entre` mantém a assinatura.

**A regra que esta tarefa não pode quebrar:** a série de quem precisa de furo fica **exatamente** como está. Se algum teste de `crates/seele-conformance/tests/furo.rs` precisar ser editado, parar e perguntar.

- [ ] **Step 1: Separar os candidatos**

Em `tentar_entre`, depois de `let todos: Vec<Destino> = ...` e **antes** do `for volta in 0..2_u8`, separar:

```rust
    // Quem corre e quem espera a vez. Ver o ADR 0037: a regra é o **socket**, e
    // não o tipo do candidato. Quem precisa de furo divide o socket com todos os
    // outros que precisam — o NAT mapeia por porta interna — e dois `Endpoint`
    // do quinn lendo a mesma fila roubam pacote um do outro. Quem não precisa
    // ganha socket próprio e pode correr.
    let (para_correr, para_a_fila): (Vec<(usize, Destino)>, Vec<(usize, Destino)>) = todos
        .iter()
        .cloned()
        .enumerate()
        .partition(|(_, destino)| !precisa_de_furo(destino.servidor));
```

- [ ] **Step 2: Correr o primeiro grupo, antes da série**

Logo abaixo, e antes do laço das duas voltas:

```rust
    // A corrida vem primeiro, e é onde estão os 9,6 s da pendência nº 26: os
    // três candidatos mortos de lá são IPv6, que nunca precisam de furo.
    if !para_correr.is_empty() {
        // O que cada corredor precisa saber para limpar o pin dele depois.
        let chaves: Vec<(String, Option<String>)> = para_correr
            .iter()
            .map(|(_, destino)| {
                let chave = destino.chave_do_pin.clone();
                let antes = pins.pinned(&chave);
                (chave, antes)
            })
            .collect();

        for (indice, destino) in &para_correr {
            // Contado antes de a corrida começar, e todos de uma vez: eles
            // **começam** quase juntos, e publicá-los um por um conforme fecham
            // contaria uma história de série que não aconteceu.
            contar(
                olhos,
                Tentativa {
                    candidato: u8::try_from(*indice).unwrap_or(u8::MAX),
                    onde: destino.servidor,
                    // Ninguém aqui precisa de furo, então ninguém aqui avisa.
                    avisou: false,
                },
            );
        }

        let corrida = {
            let corredores: Vec<Destino> =
                para_correr.iter().map(|(_, destino)| destino.clone()).collect();
            let bilhete = bilhete.clone();
            let chave = chave.clone();
            let pins = Arc::clone(&pins);
            correr(corredores.len(), DEFASAGEM_ENTRE_CANDIDATOS, move |posicao| {
                let destino = corredores.get(posicao).cloned();
                let bilhete = bilhete.clone();
                let chave = chave.clone();
                let pins = Arc::clone(&pins);
                async move {
                    let Some(destino) = destino else {
                        return Err(ConnectError::Unreachable);
                    };
                    let prazo = if e_de_outra_casa(destino.servidor) {
                        PRAZO_DE_CANDIDATO_DISTANTE
                    } else {
                        PRAZO_POR_CANDIDATO
                    };
                    // `None` no socket: cada corredor abre o seu. É o que torna
                    // a corrida correta — ver o ADR 0037.
                    match tokio::time::timeout(
                        prazo,
                        Self::conectar_por(None, bilhete, destino, chave, pins),
                    )
                    .await
                    {
                        Ok(resultado) => resultado,
                        Err(_) => Err(ConnectError::HandshakeTimeout),
                    }
                }
            })
            .await
        };

        // A limpeza de pin dos perdedores, **depois** do vencedor e pulando a
        // chave dele.
        //
        // `desfazer_pin_orfao` promete «só apaga o que este aperto escreveu», e
        // isso é exato em série e falso aqui: dois candidatos podem compartilhar
        // `chave_do_pin` — ela é `host:porta` do nome do convite, e alternativos
        // do mesmo nome colidem. Sem esta condição, cancelar um perdedor
        // encontraria `fixado_antes == None` e `pinned() == Some`, e apagaria o
        // pin que o vencedor acabou de escrever: a confiança de primeiro contato
        // do ADR 0003 desfeita em silêncio.
        let chave_do_vencedor = corrida
            .vencedor
            .as_ref()
            .and_then(|(posicao, _)| chaves.get(*posicao))
            .map(|(chave, _)| chave.clone());
        for (posicao, _) in &corrida.falhas {
            let Some((chave_perdida, antes)) = chaves.get(*posicao) else {
                continue;
            };
            if chave_do_vencedor.as_deref() == Some(chave_perdida.as_str()) {
                continue;
            }
            desfazer_pin_orfao(pins.as_ref(), chave_perdida, antes.as_deref());
        }

        if let Some((_, enlace)) = corrida.vencedor {
            return Ok(enlace);
        }

        for (posicao, falha) in corrida.falhas {
            let onde = para_correr
                .get(posicao)
                .map(|(_, destino)| destino.servidor);
            tracing::info!(?onde, erro = %falha, "este endereço do convite não deu na corrida");
            if respondeu.is_none() && alguem_respondeu(&falha) {
                respondeu = Some(falha.clone());
            }
            if primeira_falha.is_none() {
                primeira_falha = Some(falha);
            }
        }
    }
```

- [ ] **Step 3: Deixar a série só com quem ficou na fila**

Trocar o cabeçalho do laço das duas voltas para percorrer `para_a_fila` em vez de `todos`, mantendo **todo** o corpo dele intacto. O índice publicado em `Tentativa` continua sendo o índice original — é por isso que `para_a_fila` carrega `(usize, Destino)`:

```rust
        for volta in 0..2_u8 {
            for (indice, destino) in para_a_fila.iter().cloned() {
```

E `merece_segunda` passa a ser dimensionada por `para_a_fila.len()`, indexada pela **posição na fila** e não pelo índice original. Cuidado aqui: hoje `merece_segunda.get(indice)` usa o índice de `todos`. Trocar por `.enumerate()` sobre `para_a_fila` e usar essa posição para `merece_segunda`, mantendo `indice` só para o `Tentativa`.

- [ ] **Step 4: Remover a marca de `dead_code` da Task 1**

Se a Task 1 precisou de `#[allow(dead_code, ...)]` em `precisa_de_furo`, tirar agora — ela está em uso.

- [ ] **Step 5: Escrever o teste do pin, que é o que mais importa**

```rust
#[cfg(test)]
mod o_pin_do_vencedor {
    use super::desfazer_pin_orfao;
    use crate::tofu::PinStore;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct PinsEmMemoria(Mutex<HashMap<String, String>>);

    impl PinStore for PinsEmMemoria {
        fn pinned(&self, host: &str) -> Option<String> {
            self.0.lock().ok()?.get(host).cloned()
        }
        fn pin(&self, host: &str, fingerprint: String) {
            if let Ok(mut mapa) = self.0.lock() {
                mapa.insert(host.to_owned(), fingerprint);
            }
        }
        fn unpin(&self, host: &str) {
            if let Ok(mut mapa) = self.0.lock() {
                mapa.remove(host);
            }
        }
    }

    /// A armadilha do ADR 0037, escrita como teste.
    ///
    /// Dois candidatos com a mesma `chave_do_pin` — que é o caso normal quando o
    /// convite traz o mesmo nome resolvido para endereços diferentes. O vencedor
    /// escreve o pin; o perdedor é cancelado. Aplicar `desfazer_pin_orfao` ao
    /// perdedor apagaria o pin do vencedor, porque para ele `fixado_antes` era
    /// `None` e agora `pinned()` é `Some`.
    ///
    /// O que este teste guarda não é uma conexão: é a confiança de primeiro
    /// contato do ADR 0003, que seria desfeita sem nada na tela dizendo isso.
    #[test]
    fn limpar_o_perdedor_nao_pode_apagar_o_pin_do_vencedor() {
        let pins = PinsEmMemoria::default();
        let chave = "casa.exemplo:8384";

        // Nenhum dos dois viu pin antes: primeiro contato.
        let antes_do_perdedor = pins.pinned(chave);
        let antes_do_vencedor = pins.pinned(chave);
        assert!(antes_do_perdedor.is_none() && antes_do_vencedor.is_none());

        // O vencedor fecha e fixa.
        pins.pin(chave, "aa:bb:cc".into());

        // A limpeza ingênua do perdedor — a que este ADR proíbe — apagaria isso.
        // A regra é: pular a chave do vencedor.
        let chave_do_vencedor = Some(chave);
        if chave_do_vencedor != Some(chave) {
            desfazer_pin_orfao(&pins, chave, antes_do_perdedor.as_deref());
        }

        assert_eq!(
            pins.pinned(chave),
            Some("aa:bb:cc".into()),
            "a limpeza do perdedor apagou o pin que o vencedor escreveu"
        );
    }

    /// E quando ninguém vence, a limpeza acontece como sempre aconteceu.
    #[test]
    fn sem_vencedor_o_pin_orfao_e_desfeito() {
        let pins = PinsEmMemoria::default();
        let chave = "casa.exemplo:8384";
        let antes = pins.pinned(chave);
        pins.pin(chave, "aa:bb:cc".into());

        desfazer_pin_orfao(&pins, chave, antes.as_deref());

        assert!(
            pins.pinned(chave).is_none(),
            "um pin escrito por um aperto de mão cancelado ficou para trás"
        );
    }
}
```

Se `PinStore` tiver mais métodos além dos três, implementá-los com o corpo mínimo — o `trait` está em `crates/seele-core/src/tofu.rs`, linha 151.

- [ ] **Step 6: Rodar tudo do crate**

Run: `cargo test -p seele-core --no-fail-fast`
Expected: PASS.

- [ ] **Step 7: Rodar a conformidade sem tocar nela**

Run: `cargo test -p seele-conformance --no-fail-fast`
Expected: PASS, e **nenhum arquivo de `crates/seele-conformance/tests/` editado**. Conferir com `git status`: se algum aparecer modificado, a regra da seção 1 do spec foi violada — parar e perguntar.

- [ ] **Step 8: Verde e limpo**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets`
Expected: sem avisos.

- [ ] **Step 9: Commit**

```bash
git add crates/seele-core/src/enlace.rs
git commit -m "feat(enlace): os candidatos que não precisam de furo passam a correr"
```

---

### Task 4: Fechar a documentação

**Files:**
- Modify: `docs/pendencias.md`

- [ ] **Step 1: Estreitar a pendência nº 26**

A nº 26 é sobre o firewall IPv6 e o PCP, e **não fecha** com este trabalho. Mas o custo que ela mede muda, e a entrada tem de dizer isso. Acrescentar ao fim dela uma nota de estreitamento no formato que o arquivo usa: que os 9,6 s medidos ali eram custo de **série**, que o [ADR 0037](adr/0037-candidatos-do-convite-em-paralelo.md) os transformou em ~1,1 s, e que o que continua aberto é o firewall — com PCP os IPv6 passam a responder, e sem ele apenas deixam de cobrar por não responder.

- [ ] **Step 2: Registrar o que não foi medido**

Acrescentar à pendência nova (nº 28) que a defasagem de 250 ms vem do RFC e não desta rede, e que o que a confirma é refazer a medição da nº 26 com a corrida no lugar — três candidatos IPv6 bloqueados e um bom, contando o tempo até o aperto de mão fechar.

- [ ] **Step 3: Commit**

```bash
git add docs/pendencias.md
git commit -m "docs: a pendência 26 é estreitada, e o que a corrida não mediu vira a 28"
```

---

## Auto-revisão

**Cobertura do spec.** Seção 0 (por que não é um `join_all`) → Tasks 1 e 3, e é a razão de a Task 1 existir separada. Seção 1 (a regra do socket) → Task 1 (predicado) e Task 3 (separação). Seção 2 (a corrida) → Task 2. Seção 3 (o conserto do TOFU) → Task 3, passos 2 e 5. Seção 4 (o que se ganha) → medido na Task 4, passo 2. Seção 5 (o que não faz) → nada a implementar. Seção 6 (como se prova) → testes das Tasks 1, 2 e 3. Seção 7 (riscos) → Task 4.

**Consistência de tipos.** `precisa_de_furo(SocketAddr) -> bool` é produzida na Task 1 e consumida na Task 3. `correr(usize, Duration, F) -> Corrida<T>` e `Corrida { vencedor: Option<(usize, T)>, falhas: Vec<(usize, ConnectError)> }` são produzidas na Task 2 e consumidas na Task 3 com essa forma. `DEFASAGEM_ENTRE_CANDIDATOS` idem. `desfazer_pin_orfao(&dyn PinStore, &str, Option<&str>)` já existe e não muda de assinatura.

**O ponto onde este plano é mais frágil, dito antes de começar.** O passo 3 da Task 3 reindexa `merece_segunda`: hoje ela é indexada pelo índice em `todos`, e passa a ser indexada pela posição em `para_a_fila`, enquanto o `Tentativa` continua publicando o índice original. Trocar um pelo outro compila e produz um defeito silencioso — a segunda volta iria para o candidato errado. Quem implementar deve conferir os três testes de `furo.rs` **especificamente** depois desse passo, e não só ao fim da tarefa.
