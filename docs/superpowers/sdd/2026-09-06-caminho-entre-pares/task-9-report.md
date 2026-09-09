# Task 9 — relatório

## Status

Completa. TDD seguido: os dois testes do brief foram escritos, vistos falhar
(o primeiro por erro de compilação — `repassar` não existia; o segundo por
assertion depois de `repassar` implementado sem o descarte até a chave), só
então a implementação final. `cargo test --workspace`, `cargo fmt --all --
check` e `cargo clippy --workspace --all-targets` verdes.

`duas_pontas_ligadas()` foi extraída de `dois_pares_se_ligam_e_o_teste_sabe_como`
antes de escrever o primeiro teste novo, como o brief pediu, e esse teste
antigo foi refeito para chamar a extração em vez de duplicar o bloco.

## Commit

`499c9ec` — "feat(par): quem empresta repassa por pedaço, a partir de um quadro-chave"

## Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` de `cargo test --workspace`
(68 suítes, 0 falhas): **1656 testes**. `seele-core` sozinho: 276 (21 em
`par::testes`, os dois novos inclusos).

## Prova dos guardas por reversão

**Guarda 1 — a abertura vai antes de qualquer pedaço.** Reversão: adiada a
escrita da abertura para depois do primeiro pedaço (a linha
`fluxo.write_all(abertura)` movida para dentro do laço, antes do primeiro
`write_all(&pedaco)`, com a escrita original antes do laço removida).

```
running 1 test
test par::testes::o_par_recebe_a_abertura_e_os_pedacos_na_ordem ... FAILED

failures:

---- par::testes::o_par_recebe_a_abertura_e_os_pedacos_na_ordem stdout ----

thread 'par::testes::o_par_recebe_a_abertura_e_os_pedacos_na_ordem' panicked at crates/seele-core/src/par.rs:1952:9:
assertion `left == right` failed: o que chegou ao par não é a abertura seguida dos pedaços na ordem
  left: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 2, 3, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 4, 5]
 right: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 2, 3, 4, 5]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 273 filtered out; finished in 0.01s
```

Desfeita antes do commit.

**Guarda 2 — descarte de tudo antes do primeiro quadro-chave.** Reversão: o
laço de escrita perdeu a checagem `viu_a_chave`/`TipoDeQuadro::e_chave` e
passou a escrever todo pedaço recebido, sem descarte nenhum.

```
running 1 test
test par::testes::quem_entra_por_um_par_entra_num_quadro_chave ... FAILED

failures:

---- par::testes::quem_entra_por_um_par_entra_num_quadro_chave stdout ----

thread 'par::testes::quem_entra_por_um_par_entra_num_quadro_chave' panicked at crates/seele-core/src/par.rs:1986:9:
assertion `left == right` failed: o pedaço comum que veio antes da chave atravessou, ou a chave não atravessou
  left: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 0, 1, 2, 1, 3, 4]
 right: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 3, 4]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 273 filtered out; finished in 0.01s
```

Desfeita antes do commit. Depois de cada reversão desfeita, `cargo test -p
seele-core --lib par::` voltou a 21/21 verdes.

## Preocupações

**A marca de quadro-chave é lida do primeiro byte do pedaço, não recebida
como parâmetro à parte.** O brief fala em "acrescentar a `repassar` o
parâmetro que diz se um pedaço é chave", mas a assinatura pública fixada no
brief (`pedacos: Receiver<Vec<u8>>`, sem campo extra) e o teste do Step 1 —
que manda `vec![1, 2, 3]` cru e o lê de volta byte a byte — não deixam essa
informação viajar por fora do próprio pedaço sem quebrar aquele teste. A
solução foi ler `crate::tela::TipoDeQuadro::de_byte` no primeiro byte de cada
pedaço, que é exatamente o byte que o cabeçalho de quadro já grava no fio
(`escrever_cabecalho_de_quadro`) — e é também, coincidência que não é
coincidência, por que `vec![1, 2, 3]` do Step 1 (primeiro byte `1` =
`TipoDeQuadro::Chave`) atravessa o guarda sem precisar de ajuste. Isto
funciona **se cada `Vec<u8>` do canal começa exatamente onde um cabeçalho de
quadro começa** — verdade nos dois testes daqui, mas uma responsabilidade que
caiu para quem monta o canal (Task 10/11): um pedaço que corte um cabeçalho
ao meio faria `TipoDeQuadro::de_byte` ler lixo. Diferente do
`Enquadramento` do servidor (`crates/seele-server/src/tela.rs`), que conta
bytes através de pedaços para nunca depender desse alinhamento, `repassar`
não faz essa contagem — não foi pedida pelo brief, e replicá-la exigiria
inventar um parser de quadro que já existe do lado do servidor e que
`seele-core` não pode importar de lá (ADR 0002). Registro para quem ligar o
produtor de pedaços de verdade: ele tem de entregar um `Vec<u8>` por quadro
(ou ao menos garantir que o primeiro byte de cada `Vec<u8>` é sempre um byte
de tipo), não pedaços de tamanho arbitrário cortados por conveniência de
I/O.

**A conexão só sobrevive ao `spawn` do teste porque cada teste guarda um
clone extra.** `repassar` recebe `ligado: &ParLigado` por referência — não é
dona da conexão —, mas o `'static` do `tokio::spawn` obriga o teste a mover
o `ParLigado` inteiro para dentro do bloco; sem um clone de
`ligado.conexao` mantido vivo fora dele, a última alça da `Connection`
soltava assim que `repassar` retornava, e `ConnectionRef::drop` fecha a
ligação na hora (`implicit_close`) — antes de o laço de I/O da conexão ter
tido a chance de escrever ao par os bytes que já tinham sido aceitos no
buffer local. Sem esse clone, os dois testes falhavam **de forma
determinística** (5/5 execuções, com `ApplicationClosed` do lado de quem
lê), não de forma rara — não é uma tolerância a timing que ficou frouxa, é a
diferença entre a conexão ainda ter uma alça viva ou não. Isto é uma
particularidade da forma como o teste isola `repassar` numa tarefa própria;
em produção, quem chama `repassar` mantém o `ParLigado` vivo por uma sessão
inteira, bem além de uma chamada só, então esta situação não aparece fora de
teste.

## Arquivos tocados

- `crates/seele-core/src/par.rs` — `repassar`, `duas_pontas_ligadas` (teste),
  dois testes novos, e o teste antigo `dois_pares_se_ligam_e_o_teste_sabe_como`
  reduzido para chamar a extração.

---

# Fix round 1 — relatório

## Status

Completa. Os cinco achados do revisor (1 Critical, 4 Important) já estavam na
árvore quando este round assumiu — o implementador anterior morreu num erro
de API depois de os aplicar, sem commitar. Conferi cada um contra o código,
completei os dois que não tinham guarda (achados 4 e 5), e provei os cinco
por reversão, um de cada vez, restaurando depois de cada prova.

## O que já estava feito ao assumir

1. **Critical — `Enquadramento` em `seele-core::tela`.** Gêmeo do
   `seele-server::tela::Enquadramento`, contando bytes através dos pedaços em
   vez de ler o primeiro byte de cada um. `repassar` pergunta a
   `Enquadramento::entrada` o deslocamento de entrada e repassa de
   `pedaco.get(deslocamento..)` em diante. Os dois testes do brief
   (`o_par_recebe_a_abertura...`, `quem_entra_por_um_par...`) já tinham sido
   reescritos para montar quadros completos (cabeçalho + corpo) em vez de
   pedaços crus, e dois testes novos — `um_pedaco_com_dois_quadros_nao_perde_o_
   quadro_chave_que_vem_depois_do_comum` (Modo A) e
   `um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso` (Modo B) —
   já alimentavam exatamente os dois cortes que o revisor mediu.
2. **Important 2 — `stopped()`.** `repassar` já esperava
   `fluxo.stopped().await` depois do `finish()`, com o `match` de três braços
   (`Ok(None)`/`Ok(Some(codigo))`/`Err`) que `client.rs:1746-1763` já ensina.
   Os clones `mantida_viva` que os dois testes do Step 1/5 do brief usavam
   como muleta já tinham sumido dos dois testes.
3. **Important 3 — `set_priority`.** Já rebaixava para
   `crate::tela::PRIORIDADE_DA_TELA` (−2) logo depois de abrir o fluxo, antes
   de escrever o byte de tipo.
4. **Important 5 — `ErroDePar::Repasse`.** Variante nova já existia, com doc
   explicando por que não é `Escuta`, e todos os `map_err` dentro de
   `repassar` já apontavam para ela.

## O que faltava, e o que fiz

- **Important 4 — rastro.** `repassar` já tinha os três pontos pedidos:
  `info!` ao abrir o fluxo, `debug!` com `descartados` quando a chave é
  achada, e `warn!` quando o canal fecha sem chave nenhuma ter atravessado.
  O que faltava era um guarda: nenhum teste inspecionava rastro em
  `seele-core` (zero precedente no crate, e `tracing-subscriber` não está nas
  `dev-dependencies`). Escrevi um `Subscriber` mínimo
  (`CapturaDeRastro`, ~30 linhas, sem dependência nova — `tracing-core` já
  expõe o necessário) e o teste
  `repassar_avisa_quando_o_canal_fecha_sem_a_chave_chegar`, que manda só um
  quadro comum, nunca uma chave, e afirma que um `WARN` com "nenhum
  quadro-chave" foi ao rastro.
- **Important 5 — guarda que faltava.** A variante existia, mas nenhum teste
  distinguia `Repasse` de `Escuta` no ponto de chamada. Escrevi
  `quem_caiu_antes_do_repasse_e_relatado_como_repasse_e_nao_escuta`: fecha a
  conexão do lado de quem atende antes de `repassar` abrir o fluxo, espera
  `ligado.conexao.closed()` para não torcer com a rede, e afirma que o erro
  que volta é `ErroDePar::Repasse`, não `Escuta`.

## Prova dos cinco guardas por reversão

Cada reversão foi feita, testada e desfeita antes de seguir para a próxima.
`git diff` ficou limpo (sem marcadores de reversão) ao final de cada uma —
conferido com `grep -n "REVERSÃO" crates/seele-core/src/par.rs
crates/seele-core/src/tela.rs`, que não achou nada.

### 1 — Critical: `Enquadramento` que conta bytes

Reversão: `repassar` voltou a ler o tipo do primeiro byte de cada pedaço, sem
consultar `Enquadramento::entrada` — a suposição exata que o revisor mediu
como quebrada.

```
running 25 tests
...
test par::testes::um_pedaco_com_dois_quadros_nao_perde_o_quadro_chave_que_vem_depois_do_comum ... FAILED
test par::testes::um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso ... FAILED

failures:

---- par::testes::um_pedaco_com_dois_quadros_nao_perde_o_quadro_chave_que_vem_depois_do_comum stdout ----
thread '...' panicked at crates/seele-core/src/par.rs:2105:9:
assertion `left == right` failed: o quadro-chave que vinha depois do comum, no mesmo pedaço, não atravessou
  left: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9]
 right: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 0, 0, 0, 2, 3, 4]

---- par::testes::um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso stdout ----
thread '...' panicked at crates/seele-core/src/par.rs:2157:9:
assertion `left == right` failed: o corpo do quadro comum, partido ao meio, atravessou como se fosse um cabeçalho
  left: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 9, 9, 1, 0, 0, 0, 2, 7, 8]
 right: [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 0, 0, 0, 2, 7, 8]

test result: FAILED. 23 passed; 2 failed; 0 ignored; 0 measured; 253 filtered out; finished in 0.48s
```

Os dois Modos do Critical mordem — Modo A (pedaço com dois quadros engole a
chave) e Modo B (corpo partido lido como cabeçalho) falham exatamente como o
revisor descreveu. Desfeita; suíte de volta a 25/25.

### 2 — Important: `stopped()`

Reversão: `repassar` devolve `Ok(())` logo depois do `finish()`, sem esperar
`fluxo.stopped()`. Rodado 5 vezes seguidas para conferir se é determinístico,
não uma corrida de tempo:

```
=== run 1 a 5 (idêntico nas 5) ===
thread 'par::testes::o_par_recebe_a_abertura_e_os_pedacos_na_ordem' panicked at crates/seele-core/src/par.rs:2008:46:
called `Result::unwrap()` on an `Err` value: ApplicationClosed(ApplicationClose { error_code: 0, reason: b"" })

thread 'par::testes::quem_entra_por_um_par_entra_num_quadro_chave' panicked at crates/seele-core/src/par.rs:2051:46:
called `Result::unwrap()` on an `Err` value: ApplicationClosed(ApplicationClose { error_code: 0, reason: b"" })

thread 'par::testes::um_pedaco_com_dois_quadros_nao_perde_o_quadro_chave_que_vem_depois_do_comum' panicked ...
called `Result::unwrap()` on an `Err` value: ApplicationClosed(ApplicationClose { error_code: 0, reason: b"" })

thread 'par::testes::um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso' panicked ...
called `Result::unwrap()` on an `Err` value: ApplicationClosed(ApplicationClose { error_code: 0, reason: b"" })  (ou, numa das 5, Read(ConnectionLost(ApplicationClosed(..))) — mesma causa, ponto de leitura diferente)

test result: FAILED. 21 passed; 4 failed; 0 ignored; 0 measured; 253 filtered out; finished in ~0.30s (5/5)
```

Determinístico nas 5 execuções (mesma causa — `ApplicationClosed` — em todas,
como o relatório do round anterior já tinha registrado para o clone que fazia
o mesmo papel). Sem o clone extra do round anterior e sem `stopped()`, os
quatro testes que chamam `repassar` diretamente falham, não só os dois
originais. Desfeita; suíte de volta a 25/25.

### 3 — Important: `set_priority`

Reversão: tirada a linha `fluxo.set_priority(crate::tela::PRIORIDADE_DA_TELA)`.

```
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 253 filtered out; finished in 0.31s
```

**Nenhum teste morde este achado**, e medi por que antes de concluir: a API
pública do `quinn` não expõe a prioridade de um `SendStream` para ninguém
além do próprio handle que a definiu — `Connection` não tem `send_stream(id)`
público, e não há uma segunda alça para o mesmo stream. Quem recebe
(`RecvStream`) não vê prioridade nenhuma: é um hint local de agendamento de
saída, nunca viaja no fio. `repassar` não devolve o `fluxo` para um teste
inspecionar, e criar esse caminho só para este `assert` seria maior do que a
mudança que se está guardando. É o mesmo estado dos outros dois lugares do
produto que fazem a mesma chamada — `crate::tela.rs:1245` e
`seele-server/src/tela.rs:939` — nenhum dos dois tem teste de prioridade
também. Registrado como preocupação aberta abaixo, não escondido atrás de um
teste que não prenderia nada.

### 4 — Important: rastro (`warn!` sem chave)

Reversão: tirado o bloco `if !viu_a_chave { tracing::warn!(...) }`.

```
running 1 test
test par::testes::repassar_avisa_quando_o_canal_fecha_sem_a_chave_chegar ... FAILED

thread '...' panicked at crates/seele-core/src/par.rs:2276:9:
o canal fechou sem chave nenhuma atravessar, e nenhum WARN foi ao rastro: ["TRACE: ", ... "INFO: um par foi atendido", ... "INFO: repasse de tela para o par começou", ... "INFO: repasse de tela para o par terminou e foi confirmado", "TRACE: connection closed"]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 277 filtered out; finished in 0.01s
```

O rastro capturado mostra os dois `info!` (abertura e fim confirmado) e nada
de `WARN` — exatamente o que se espera ao tirar o aviso. Desfeita; suíte de
volta a 25/25 (277 filtrados porque o teste roda sozinho, sem `multi_thread`).

### 5 — Important: `ErroDePar::Repasse` distinto de `Escuta`

Reversão: `open_uni()` voltou a mapear para `ErroDePar::Escuta` em vez de
`Repasse`.

```
running 1 test
test par::testes::quem_caiu_antes_do_repasse_e_relatado_como_repasse_e_nao_escuta ... FAILED

thread '...' panicked at crates/seele-core/src/par.rs:2193:9:
queda no meio do repasse voltou como Escuta("closed by peer: caiu antes do repasse (code 0)"), não como ErroDePar::Repasse

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 277 filtered out; finished in 0.01s
```

Desfeita; suíte de volta a 25/25.

## Testes novos deste round

- `um_pedaco_com_dois_quadros_nao_perde_o_quadro_chave_que_vem_depois_do_comum`
  (herdado do round interrompido — Modo A do Critical)
- `um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso` (herdado —
  Modo B do Critical)
- `quem_caiu_antes_do_repasse_e_relatado_como_repasse_e_nao_escuta` (novo,
  achado 5)
- `repassar_avisa_quando_o_canal_fecha_sem_a_chave_chegar` (novo, achado 4,
  com `CapturaDeRastro` — um `tracing::Subscriber` mínimo escrito para este
  teste, sem dependência nova)

`seele-core` sozinho: 278 testes (25 em `par::testes`). Workspace inteiro:
68 suítes, **1660 testes**, 0 falhas — `cargo test --workspace`,
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets`
verdes.

(Uma primeira contagem deu 72 suítes/1661 testes porque o `tee` daquela
rodada escreveu no mesmo arquivo de log que uma chamada anterior, ainda
rodando em segundo plano por engano — o log saiu com blocos de doctest
duplicados. A contagem final acima veio de uma única execução limpa, sem
processo concorrente, conferida com `ps aux | grep "cargo test"` vazio antes
de fechar.)

## Preocupações

- **`set_priority` não tem guarda automatizado**, nem este nem os outros dois
  lugares do produto que fazem a mesma chamada. É uma lacuna real, mas
  estrutural: a API pública do `quinn` não dá como observar a prioridade de
  um stream de fora do handle que a definiu. Consertar exigiria ou expor o
  `SendStream` para fora de `repassar` (mudança de forma, não de conteúdo) ou
  um teste de efeito indireto (contenção de banda sob competição), que seria
  lento e propenso a instabilidade num `cargo test` comum. Deixo registrado
  para quem unificar rastro/observabilidade de stream no produto, em vez de
  inventar um teste que não prenderia nada de verdade.
- **A preocupação já registrada no relatório original sobre alinhamento de
  pedaço com cabeçalho de quadro está resolvida** pelo `Enquadramento`: ele
  não depende mais de o primeiro byte de cada `Vec<u8>` ser um byte de tipo,
  então o produtor de pedaços de verdade (Task 10/11) pode entregar pedaços
  de tamanho arbitrário sem quebrar o repasse.

---

# Fix round 2 — relatório

## Status

Completa. O achado do round: `Enquadramento` (o tipo que fecha o Critical do
round 1) tinha chegado ao core sem suíte própria — só o exercício indireto
que os testes de `crate::par::repassar` faziam dele. O revisor mediu
quebrando cada invariante que a doc de `entrada` promete, um de cada vez, e a
suíte inteira de `seele-core` (278 testes) ficou verde nas cinco vezes —
inclusive na mais grave, `comeca_aqui` removido, que é metade do próprio
Critical (um cabeçalho partido entre dois pedaços virando porta de entrada, e
a segunda metade dele lida como tamanho de quadro).

Trouxe os oito testes do gêmeo em `crates/seele-server/src/tela.rs`
(`a_porta_de_entrada_e_o_comeco_de_um_quadro_chave`,
`sem_quadro_chave_nao_ha_porta`, `um_cabecalho_partido_ao_meio_nao_vira_porta`,
`o_enquadramento_atravessa_pedacos_de_qualquer_tamanho`,
`um_tamanho_impossivel_encerra_o_fluxo`, `um_quadro_vazio_encerra_o_fluxo`,
`um_byte_de_tipo_que_este_fluxo_nao_conhece_o_encerra`,
`um_quadro_de_som_atravessa_e_nao_e_porta_de_entrada`) para
`crates/seele-core/src/tela.rs`, num módulo novo `mod o_enquadramento`,
adaptados ao `TipoDeQuadro` e aos erros nomeados deste crate — o servidor não
distingue quadro vazio, tamanho grande demais e tipo desconhecido (os três
caem em `FimDaTela::FluxoMalformado`), e `seele-core` tem uma variante
(`ErroDeTela::{QuadroVazio,QuadroGrandeDemais,TipoDesconhecido}`) para cada
um. Todos os oito couberam sem repetição — o mínimo pedido (cinco) e o
recomendado (oito) coincidiram.

Também apliquei o Minor: `CapturaDeRastro::enabled` (o `Subscriber` de teste
do round 1) filtra agora por `*metadata.level() <= tracing::Level::WARN`, em
vez de aceitar tudo — a falha de um teste de rastro não despeja mais as ~180
linhas de handshake do `quinn` para achar a ausência de um `WARN`.

`set_priority` fica sem guarda, como combinado — o caminho de extrair
`abrir_fluxo_de_repasse` para um guarda de verdade fica registrado, não
percorrido.

## Prova dos cinco (na verdade, seis) guardas por reversão

Cada reversão foi feita contra `crates/seele-core/src/tela.rs::Enquadramento::entrada`,
rodando `cargo test -p seele-core --lib` (o comando exato que o revisor usou
para medir a ausência de guarda), e desfeita antes da próxima. `grep -n
"REVERSÃO" crates/seele-core/src/tela.rs crates/seele-core/src/par.rs` não
achou nada ao final.

### `tamanho > MAX_QUADRO_LEN` apagado

```
---- tela::o_enquadramento::um_tamanho_impossivel_encerra_o_fluxo stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3215:9:
assertion `left == right` failed
  left: Ok(None)
 right: Err(QuadroGrandeDemais { len: 524289 })

test result: FAILED. 285 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

### `tamanho == 0` apagado

```
---- tela::o_enquadramento::um_quadro_vazio_encerra_o_fluxo stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3226:9:
assertion `left == right` failed
  left: Ok(None)
 right: Err(QuadroVazio)

test result: FAILED. 285 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

### tipo desconhecido virando `Comum` em silêncio

Reversão: `TipoDeQuadro::de_byte(byte_de_tipo).unwrap_or(TipoDeQuadro::Comum)`
em vez de encerrar com `TipoDesconhecido`.

```
---- tela::o_enquadramento::um_byte_de_tipo_que_este_fluxo_nao_conhece_o_encerra stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3242:9:
assertion `left == right` failed
  left: Ok(None)
 right: Err(TipoDesconhecido { byte: 3 })

test result: FAILED. 285 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

### quadro de som aceito como porta de entrada

Reversão: `if (tipo.e_chave() || matches!(tipo, TipoDeQuadro::Som)) &&
comeca_aqui && entrada.is_none()`.

```
---- tela::o_enquadramento::um_quadro_de_som_atravessa_e_nao_e_porta_de_entrada stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3256:9:
assertion `left == right` failed: um quadro de som foi tratado como fluxo malformado
  left: Ok(Some(0))
 right: Ok(None)

test result: FAILED. 285 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

### `comeca_aqui` removido — a metade do Critical

Reversão: `if tipo.e_chave() && entrada.is_none()`, sem `comeca_aqui`.

```
---- tela::o_enquadramento::o_enquadramento_atravessa_pedacos_de_qualquer_tamanho stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3210:9:
assertion `left == right` failed
  left: Some(309)
 right: None

---- tela::o_enquadramento::um_cabecalho_partido_ao_meio_nao_vira_porta stdout ----
thread '...' panicked at crates/seele-core/src/tela.rs:3185:9:
assertion `left == right` failed
  left: Ok(Some(0))
 right: Ok(None)

test result: FAILED. 284 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

Esta é a única reversão que derruba dois testes — exatamente porque
`comeca_aqui` é o mecanismo que os dois protegem por ângulos diferentes: um
prova que um cabeçalho partido não vira porta, o outro prova que um pedaço de
qualquer tamanho (inclusive byte a byte) nunca acha porta onde não devia.

## Testes novos deste round

`crates/seele-core/src/tela.rs::o_enquadramento` — os oito citados acima.
`seele-core` sozinho: 286 testes (278 + 8). Workspace inteiro: 68 suítes,
**1668 testes**, 0 falhas — `cargo test --workspace`,
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets`
verdes. Nenhum processo de `cargo test` ficou para trás (`ps aux | grep
"cargo test"` vazio antes de fechar).

## Preocupações

- Nenhuma nova. O `set_priority` sem guarda continua registrado do round
  anterior, por decisão do coordenador neste round (não entra).
