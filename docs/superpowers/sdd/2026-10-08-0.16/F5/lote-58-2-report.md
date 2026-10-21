# Relatório do lote 58-2 (F5, 0.16)

O item 12 da pendência 58: a sala pedida, apagada no meio da volta de rede.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a546eb6e0b1203785`
- Branch: `0.16/lotes-58-2`, aberto em `bfad7594`.
- Brief: `.superpowers/sdd/lotes/lote-58-2-brief.md`, com `regras-58.md` e a
  seção do item 12 em `triagem-58.md`.
- Arquivo mudado: só `crates/seele-sessao/src/assento.rs`, mais este relatório.
- Status: **DONE_WITH_CONCERNS**. As ressalvas estão no fim, e nenhuma bloqueia.

## Os commits

| commit | o que entra |
|---|---|
| `1a6d3086` | o conserto e os dois testes, em `assento.rs` |
| este | o relatório |

## O vermelho, antes do conserto

Os dois casos do item viraram testes de unidade no `#[cfg(test)]` do arquivo.
Rodados contra o código de `bfad7594`, reprovaram **como a triagem disse**:

| teste | o caso | exige | deu |
|---|---|---|---|
| `a_recusa_da_sala_pedida_que_foi_apagada_devolve_a_de_antes` | em A, pedir B, `VoiceRoomDeleted{B}`, a recusa, o `Pong` de B | `Some(A)` | `None` |
| `a_recusa_seguinte_nao_devolve_a_sala_pedida_que_foi_aceita_e_apagada` | em A, pedir B e C, `VoiceRoomDeleted{B}`, o `Pong` de B, a recusa, o `Pong` de C | `None` | `Some(B)` |

As frases do vermelho: «o servidor recusou B, que tinha apagado, e manteve a
pessoa em A, e a tela a deixou fora de toda sala» (`left: None, right:
Some(VoiceRoomId(1))`), e «a recusa de C devolveu a pessoa a B, que o servidor
aceitou e apagou: a tela a mostra numa sala que não existe» (`left:
Some(VoiceRoomId(2)), right: None`).

**Os números dos `Pong`.** O «`Pong(1)`» e o «`Pong(2)`» do brief são os do
pedido de B e do de C. O `em_a()` dos testes gasta o número 1 com a entrada em
A, então nos testes eles são `pong_do_pedido(b)` (2) e `pong_do_pedido(c)` (3).
O sentido é o mesmo.

Os dois testes conferem também o resto do que a triagem pediu:

- no primeiro, a tela sai de B quando o aviso chega;
- no primeiro, sem recusa, o `Pong` deixa a pessoa fora de sala: o servidor
  leu o pedido, sentou a pessoa em B e a tirou de lá ao apagar. Isso já era
  verde antes, e continua;
- no segundo, `confirmada` depois do `Pong` de B é `None`. Antes dava `Some(B)`:
  o áudio abriria em B, que não existe.

**O servidor recusa mesmo a sala que não existe.** Conferido em
`seele-server/src/session.rs`, no braço de `ClientMessage::EnterVoiceRoom`:
`voice_room_liberado` recusa a sala inexistente com `VoiceRoomEntryRefused` e
segue (`continue`), sem mexer no assento. É a recusa que o primeiro caso espera.

## O conserto

Segue o esboço da triagem, em `EntradasPendentes`:

- **A marca.** `Entrada` ganha `apagada`. No braço de `VoiceRoomDeleted { X }`,
  o pedido no ar para X, ainda sem recusa, fica marcado.
- **O pedido que define o assento segue valendo.** Se quem põe a pessoa em X é
  o pedido no ar de X (o último da fila, com `vale`), o assento vai a `None` e o
  pedido não é superado. A recusa dele devolve o `antes`, pelo caminho de
  sempre de `recusar` (o caso 1 volta a A); o `Pong` dele deixa `None`. Nos
  outros casos, o braço supera tudo como antes.
- **O `Pong` decide a marca.** Quando o `Pong` fecha como aceito um pedido
  marcado (sem recusa), o `antes` dos pedidos seguintes que partiam da sala
  dele passa a `None`. A recusa de C já não devolve a B (o caso 2).

São 35 linhas novas de produto e 5 que saem, comentários incluídos. Os dois
testes são as outras 75. O fio não muda.

## As reversões

Cada uma aplicada por script sobre uma cópia do arquivo consertado, rodada, e
o arquivo devolvido da cópia, com o `cmp` idêntico.

| reversão | o que se desfez | reprovou |
|---|---|---|
| o braço atropela | o `superar()` de volta, sem a exceção do pedido no ar | só o primeiro, com a frase do vermelho (`left: None`) |
| o `Pong` | a correção do `antes` desligada (`&& false`) | só o segundo, com a frase do vermelho (`left: Some(VoiceRoomId(2))`) |
| a marca | sem `entrada.apagada = true` | só o segundo, com a mesma frase |

A reversão do conserto inteiro é o vermelho de cima. Os três testes de antes
(`a_sala_apagada_tira_quem_estava_nela_e_so_ela`,
`a_recusa_nao_devolve_a_sala_de_antes_que_foi_apagada_com_o_pedido_no_ar` e
`a_sala_apagada_que_ainda_e_um_pedido_no_ar_fica_para_a_resposta_dele`) ficaram
verdes em todas as rodadas.

## Os guardas

- `cargo --offline test -p seele-sessao`: 83 passaram na `lib`, e mais 1 em
  `tests/sem_hash_nos_ids_do_servidor.rs`.
- `cargo --offline test -p seele-ffi --lib`, que dobra o `Room`: 218 passaram,
  1 ignorado, que já era ignorado antes. O «panicked» no log é de
  `lib.rs:13680`, o pânico de propósito com o cadeado da voz.
- `cargo --offline clippy -p seele-sessao --all-targets -- -D warnings`: limpo.
- `cargo fmt -p seele-sessao --check`: limpo.
- `cargo --offline xtask web`: as 11 etapas «ok», o `seele-sessao` para
  `wasm32-unknown-unknown` entre elas. Rodou com o SDK clonado e o `curl` de
  mentira no começo do `PATH`, e o log não tem «curl bloqueado».
- Os roteiros de `tools/roteiros/`, numa chamada à `carga-da-casca.py`: os 46
  passaram, e nenhum script estourou.

## As ressalvas

1. **O áudio fica em A durante a espera.** No caso 1, entre o aviso e a
   resposta, a tela mostra a pessoa fora de sala, e `confirmada` dá A, pelo
   `antes` do pedido que segue valendo. Antes do conserto, ele dava `None`, e o
   áudio fechava. É o que já acontecia antes do aviso: a tela em B e o áudio em
   A. Se o servidor apagou B antes de ler o pedido, A está certo. Se ele aceitou
   B e depois apagou, o áudio fica em A por uma volta de rede, até o `Pong`.
2. **Dois vizinhos medidos, e deixados.** Os dois são um terceiro evento dentro
   da mesma volta. Medi com testes de passagem, sem commit. Os dois dão `None`
   antes e depois do conserto, e por isso não são regressão:
   - **um pedido novo na espera.** Em A, pedir B, B apagada, pedir C, e o
     servidor recusa B e C: fica `None`, e o servidor tem a pessoa em A. O
     `antes` de C é o `None` da tela, e a corrente de `recusar` não o liga a
     B;
   - **o `desfazer` do motor na espera** (a senha grande demais). O `desfazer`
     só devolve o assento ao pedido de antes «se o assento é o dele», e o
     assento de um pedido marcado é `None`. No app esse caminho não chega:
     `seele_ffi::Connection::enter_voice_room` recusa a senha antes de mandar o
     comando ao motor.

   Consertar os dois pede mais que o item: ligar o `antes` de um pedido novo à
   marca, e o `desfazer` a ela. Fica para quem fechar a 58.
3. **O `target` da coordenação não existe.**
   `/Users/dev-alexandre/SEELE/.claude/worktrees/eloquent-blackburn-a0bf2c/target`
   não estava lá. Semeei com `cp -c` de `/Users/dev-alexandre/SEELE/target`
   (`debug/{deps,build,.fingerprint}` e o `wasi-sdk` 24.0). Nada foi movido.
4. **O que deu errado comigo.** Devolvi o arquivo de uma reversão com `cp -p`.
   A data antiga da cópia ficou mais velha que a do último build, e o cargo
   reaproveitou o binário da reversão: uma rodada do `seele-sessao` reprovou na
   linha revertida, com o arquivo já idêntico pelo `cmp`. Um `touch` e uma
   rodada nova resolveram. As três reversões não foram afetadas: cada uma
   escreveu o arquivo com data nova, e cada uma reprovou o seu teste. **Para
   devolver um fonte, use `cp` sem `-p`, ou um `touch` depois.**

## Rodada 1

A revisão (`.superpowers/sdd/lotes/review-58-2-report.md`) aprovou com
ressalvas: 0 Críticos, 1 Importante (I1) e 1 Menor (M1). O pedido da rodada
está em `.superpowers/sdd/lotes/lote-58-2-rodada1.md`.

| commit | o que entra |
|---|---|
| `40e351bc` | I1: o `Pong` de um pedido aceito e apagado corrige só o pedido seguinte, e o teste do S25 |
| `cd37cd27` | M1: sai o `superar()` guardado, e o doc do `vale` diz o invariante novo |
| este | esta seção |

### I1: só o pedido seguinte perde o `antes`

**O caso (S25).** Em A, pedir B (#2), C (#3), B de novo (#4) e D (#5). O
servidor aceita B, apaga B, aceita C, recusa o segundo B e recusa D: a pessoa
fica em C. O `Pong(2)` limpava o `antes` de todo pedido que partia de B, o #3
**e** o #5. O `antes` do #5 era o do segundo B, que a recusa dele consertaria
para C. A tela acabava fora de sala, com o áudio fechado.

**O vermelho.** `o_pong_do_pedido_aceito_e_apagado_so_corrige_o_pedido_seguinte`:

- contra `1a6d3086`, reprovou com `left: None, right: Some(VoiceRoomId(3))`;
- com o produto de `bfad7594` e os testes do ramo, ele passou. Ali, só os dois
  testes do item 12 reprovaram, como antes.

**O conserto.** No braço do `Pong`, `self.fila.front_mut()` no lugar do laço
sobre a fila inteira, como o `get_mut(1)` de `recusar`. É o esboço da triagem ao
pé da letra.

**As reversões**, cada uma voltada com `cp` sem `-p` e com o `cmp` idêntico:

| reversão | reprovou |
|---|---|
| o laço sobre a fila inteira | só o S25 (`left: None, right: Some(VoiceRoomId(3))`) |
| o `Pong` sem correção nenhuma | só o segundo caso do item 12 (`left: Some(VoiceRoomId(2))`) |

**O modelo da revisão.** O `modelo_atomico.rs` ainda estava em
`scratchpad/rev582-aux/`. Rodei-o com `--release`, 400 mil sementes por
variante, sobre o código final (`cd37cd27`) e sobre a base, que rodei de novo
eu mesmo.

| | sem reúso de id | com reúso de id |
|---|---|---|
| a base, contra a lista do revisor | idêntica (28.688) | idêntica (25.109) |
| o final, contra o estreito do revisor | idêntico (3.234) | idêntico (3.238) |
| o final piora contra a base | **0** | 41 |
| o final ganha da base | 25.454 | 21.912 |
| o final piora contra o conserto entregue | 0 | 0 |
| o final ganha do conserto entregue | 31 | 25 |

**Os 41 com reúso** são os mesmos 41 do estreito do revisor. Todos têm a sala
apagada e criada de novo com o mesmo id (`apaga4` … `cria4`). É a O2: o cliente
não distingue a sala velha da nova, e nenhuma regra por id resolve isso.

### M1: o ramo morto e o doc do `vale`

- **O ramo.** Sai o
  `if !self.fila.back().is_some_and(|e| e.vale && e.sala == X) { self.superar(); }`.
  O braço da sala apagada já não toca no `vale` de pedido nenhum.
- **A prova.** Ela não tem vermelho, porque a conduta não muda:
  - os 84 testes da `lib` passam;
  - o modelo dá o mesmo resultado que o estreito do revisor, que ainda tinha o
    ramo;
  - a reversão para o `superar()` sem guarda, que era a da rodada 0, reprova o
    primeiro caso do item 12 (`left: None, right: Some(VoiceRoomId(1))`). O
    que ficou no braço é o que o segura.
- **O doc do `vale`.** Ele deixa de dizer que a sala apagada o derruba.
  - O pedido escrevia «a sala apagada derruba os **outros** pedidos, e não o da
    própria sala». Escrevi que ela **não derruba nenhum**, porque é o que o
    código faz agora. Se é a sala do pedido que vale, ele segue valendo, com o
    assento em `None`. Se é outra, o assento não está nela, e os pedidos de
    antes do último já não valiam.
  - Com o ramo fora, «derruba os outros» descreveria uma linha que não existe.
- **O invariante, para a F4.** Ele está escrito em dois lugares:
  - **no doc do campo `vale`:** só o último pedido da fila pode valer. Quando
    ele vale, o assento é a sala dele, **ou `None`**, se a sala foi apagada com
    ele no ar. Quem lê `vale` não pode supor `assento == Some(sala)`;
  - **na doc pública de `EntradasPendentes`:** o assento pode ser `None` com um
    pedido no ar que ainda o define. A `confirmada` segue dando o assento de
    antes dele até a resposta.

### Para depois: as observações O1 e O2 da revisão

Ficam fora do lote, como o pedido da rodada manda. São pontos para a F5 ou para
quem fechar a 58:

- **O1, o `MovedToVoiceRoom` depois de uma ação local.** O operador move, o
  cliente sai ou pede outra sala, e o `Moved` chega depois, embora o servidor o
  tenha aplicado antes. O cliente fica na sala do operador, e o servidor fora
  ou noutra sala. O lote não cria essa dessincronia, só deixa de apagá-la por
  acaso (234 intercalações no modelo com o move sem atomicidade). A revisão
  sugere uma pendência própria.
- **O2, o servidor reusa o `VoiceRoomId`.** `voice_rooms.id` é
  `INTEGER PRIMARY KEY` sem `AUTOINCREMENT`
  (`crates/seele-server/src/persistence/channels.rs`). Apagar a sala mais nova
  e criar outra devolve o mesmo id. Os 41 casos de cima são esses, e só o
  `AUTOINCREMENT` no servidor os fecharia. É de outra frente.

### Os guardas da rodada

- `cargo --offline test -p seele-sessao`: 84 passaram na `lib`, e mais 1 em
  `tests/sem_hash_nos_ids_do_servidor.rs`.
- `cargo --offline test -p seele-ffi --lib`: 218 passaram, 1 ignorado, que já
  era ignorado antes.
- O clippy com `-D warnings` e o `fmt --check` do `seele-sessao`: limpos.
- `cargo --offline xtask web`: as 11 etapas «ok», o `seele-sessao` para
  `wasm32-unknown-unknown` entre elas. Rodou com o SDK clonado e o `curl` de
  mentira no começo do `PATH`, e o log não tem «curl bloqueado».
