# Relatório do lote FORA (F5, 0.16)

Um item da pendência 63: as linhas do grupo FORA DO AR (os membros desconectados) não tinham chave, casavam pela
posição, e quando alguém se conectava os nomes escorregavam pelos nós. O leitor de tela parado num nome perdia o nó, e
a seleção no nome se desfazia. Este lote dá a cada linha a chave do membro.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af8dd831e616548bb`
- Ramo: `0.16/lotes-fora`, aberto no `bec085df` (o `0.16/lotes` com a `versao/0.16` e o sala-viva, a P6, e os lotes AX,
  C64, R63 e ECO).
- Brief: `.superpowers/sdd/lotes/lote-fora-brief.md`. A medida de 16/10 está no `relatorio.md`, seção «O grupo FORA
  DO AR», da pasta `m63/` do scratchpad da orquestradora.
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (o conserto, num commit só dele),
  `apps/seele-app/bancada/linhas-pelo-id.cjs` (o caso 15), `apps/seele-app/tests/frontend.rs` (um guarda estático
  que teria reprovado o conserto), `docs/pendencias.md` (a 63) e este relatório.
- Status: **DONE**. Nada foi empurrado e nenhum subagente foi usado. O app, o microfone, o `xtask web` e a
  conformidade não rodaram, nada foi baixado, e o fio não mudou. O `target` foi semeado por `cp -c` e nunca movido,
  com o `CARGO_TARGET_DIR` desligado e `cargo --offline`.

## Os commits

| commit | o que entra |
|---|---|
| `b0df8fda` | **o vermelho**: o caso 15 da `linhas-pelo-id.cjs`, com o controle da chave. Reprova no `bec085df` |
| `8483ed28` | o guarda estático `o_fora_do_ar_mostra_so_o_nome_e_nunca_um_horario` aceita o `id` do membro, só dentro da chave. Passa antes e depois do conserto |
| `5591aab3` | **o conserto**, só no `tela-sessao.js`: `chaveNaLista.set(li, \`membro:${membro.id}\`)` no `grupoForaDoAr`, e os comentários que diziam o contrário |
| este | a 63 e este relatório |

O `8483ed28` fica entre o vermelho e o conserto por uma razão só: o `frontend.rs` exigia que o `grupoForaDoAr` lesse do
membro **só** `nickname`, e o conserto lê também `id`. Se o ajuste viesse depois, o commit do conserto deixaria a suíte
do `frontend` vermelha; se viesse junto, o conserto deixava de ser «só no `tela-sessao.js`». Um guarda que passa nos dois
estados resolve as duas coisas. Todas as mensagens estão em português e terminam em `Co-Authored-By: Claude Opus 5.5
<noreply@anthropic.com>`.

## O vermelho

O caso novo é o 15, o próximo número livre, no fim da bancada, em páginas próprias. A lista do FORA DO AR vem do acessor
`membros_fora_do_ar`, e não do retrato; a ponte simulada o devolve (`testTable.membros_fora_do_ar`), e cada passo anda a
`fora_do_ar_revision` pelo caminho do evento (`RosterChanged`) e espera a lista que o tique busca depois do retrato
(`revisaoDoForaDoAr`). Os membros são Ana (101), Bruno (102) e Carla (103), nessa ordem, que é a do Rust
(`Room::fora_do_ar` ordena pelo apelido).

1. **`[A, B, C] → [B, C]`** (o A se conectou), com a seleção no nome do C. As asserções do pedido: os nós de B e de C
   são `===` e estão no documento; nenhuma mutação dentro das linhas que ficaram; o nó que saiu é o do A (as linhas
   tiradas do `ul` e as que deixaram o documento); a seleção no C é a mesma (o texto e o nó de texto).
2. **`[A, C] → [A, B, C]`**, com a seleção no C: os nós de A e de C ficam, o do C segue dizendo «Carla», só o B ganha
   um nó, e a seleção fica.
3. **O retrato igual:** depois de cada transição, 1,2 s de tique (o retrato desenhado tem de ser outro objeto no fim, senão
   o tique parado não mediria nada): 0 mutações na lista inteira, os mesmos nós.
4. **O controle da chave (c):** quem se conecta aparece em `presentes` no retrato, e o acessor só responde depois.
   Por um instante a Ana é uma `li.pessoa` e uma `li.membro-fora-do-ar` ao mesmo tempo, com o mesmo id. A bancada
   simula a janela: o retrato com a Ana em `presentes` e a revisão adiante, com o acessor ainda na lista velha, e depois
   a resposta nova. Nenhuma das duas linhas leva o nó da outra, e o B e o C não trocam de nó.

**No `bec085df`** (`tela-sessao.js` com o sha256 `8a8f87e6…3889`), a bancada inteira reprova no caso 15, depois de os 14
de antes passarem:

```
AssertionError [ERR_ASSERTION]: caso 15, o A se conectou: o nó de Bruno não é o mesmo (no documento: true, e o nó de
antes diz agora "Carla"). Os nomes escorregam pelos nós, e o leitor de tela parado em Bruno perde o dele
saida 1
```

**Os outros passos, medidos de uma vez.** Uma bancada para na primeira asserção. Por isso rodei um rascunho no
scratchpad, gerado do caso 15 do arquivo, que imprime o medido e junta as falhas em vez de parar (nada dele entra em
commit). No `bec085df`, no Chromium 151 e no WebKit 26.5, com os mesmos números nos dois:

| transição | nós de antes | texto reescrito nas que ficaram | quem saiu | seleção |
|---|---|---|---|---|
| `[A, B, C] → [B, C]` | o nó do A diz «Bruno», o do B diz «Carla», e **o do C saiu do documento** | **2** | **o nó do C** | «Carla» → «» (fora do nó) |
| `[A, C] → [A, B, C]` | o A fica; o nó do C diz «Bruno», e o C ganha um nó novo no fim | **1** | ninguém | «Carla» → «» |
| retrato igual | os mesmos | 0 mutações (**já passava**) | — | — |

É o que a medida de 16/10 viu no `531b9cba`. O controle (c) tem 2 falhas no `bec085df`: quando o acessor responde, o
nó do membro Ana segue no documento (reescrito para «Bruno»), e o B e o C trocam de nó. A janela em si (a Ana conectada e
a velha ao mesmo tempo) ele não reprova no `bec085df`: sem chave nenhuma, as duas linhas não disputam nada. Ele existe
para o **depois**, para a chave nova não tomar o nó da outra, e a prova está em «A reversão».

O rascunho tem 15 falhas no `bec085df` (8 na primeira transição, 5 na segunda, 2 no controle) e 0 depois do conserto,
nos dois motores.

## O conserto

Em `grupoForaDoAr` (`tela-sessao.js`), uma linha de código, o corpo do `map` passando a ter chaves, e os comentários:

```js
const li = elemento("li", "membro-fora-do-ar", membro.nickname);
chaveNaLista.set(li, `membro:${membro.id}`);
```

**Conferido:**
- **O `MembroForaDoAr` traz o `id`.** `crates/seele-ffi/src/types.rs:293-298`: `pub id: u64` e `pub nickname: String`, e
  o `Connection::membros_fora_do_ar` (`lib.rs:3392`) monta os dois. Atravessa a ponte (`#[derive(serde::Serialize)]`), e a
  casca recebe `{ id, nickname }`. O guarda de D-fio-4 (`MembroForaDoAr` é só id e apelido) segue valendo.
- **A chave não colide.** Os prefixos que o `tela-sessao.js` põe em `chaveNaLista` são `pessoa:`, `sala:`, `grupo:`,
  `canal:`, `acao:` (com `#vez`), `dia:` (com `#vez`), `envio:` e `mensagem:`; `membro:` não existia. Na lista de pessoas
  (`#lista-roster`) convivem `pessoa:`, `sala:`, `grupo:` e agora `membro:`, e cada `reconciliarALista` tem o seu mapa
  de nós velhos. O que importa de verdade é `pessoa:<id>`: a mesma conta, depois de conectada, é uma `li.pessoa`, e por
  um instante é as duas coisas (o retrato chega antes da lista de membros). É o controle (c).
- **O `id` não vai para o texto da linha.** A linha segue sendo só o apelido, e o guarda estático confere que toda
  leitura de `membro.id` está dentro de `` `membro:${membro.id}` ``.
- **Uma conta repetida na lista** (que o Rust não manda: `fora_do_ar` sai de um `BTreeMap` por id) caberia ao que o
  `reconciliarALista` já faz com chave repetida: o nó velho vai para a primeira, e a segunda ganha um nó novo.

**A bancada depois do conserto**, no Chromium e no WebKit, o caso 15 inteiro:

| transição | nós | mutações nas que ficaram | quem saiu | seleção |
|---|---|---|---|---|
| `[A, B, C] → [B, C]` | **B e C são os mesmos** e no documento | **0** | **o nó do A** (`tiradas` e `sairam`: `["Ana"]`) | «Carla» → «Carla», no mesmo nó de texto |
| `[A, C] → [A, B, C]` | **A e C são os mesmos**, o C diz «Carla» | **0** | ninguém; entra um nó, o do B | «Carla» → «Carla» |
| retrato igual | os mesmos | 0 mutações, redesenhou | — | — |

O grupo é o mesmo nó nas duas transições, e o cabeçalho diz «2» e «3». No controle (c), a Ana conectada é uma `li.pessoa`
em outro nó, o nó do membro segue no FORA DO AR dizendo «Ana», o B e o C não trocam de nó; quando o acessor responde, o
nó do membro sai, a `li.pessoa` fica no mesmo nó e o B e o C também.

## A reversão

O `tela-sessao.js` consertado foi copiado para o scratchpad (`mutante.sh`). O script troca **só o texto** pedido, roda a
`linhas-pelo-id.cjs` inteira no Chromium, devolve o arquivo da cópia e confere com `cmp` (com a cópia e com o
`git show HEAD:…`). Em todas, o arquivo voltou idêntico, com o sha256 `8e371a0b…fc5a` antes e depois, e o
`git status` limpo.

| mutante | a bancada |
|---|---|
| **sem a linha do conserto** (a reversão do pedido) | reprova no caso 15, com a frase do vermelho: «o A se conectou: o nó de Bruno não é o mesmo (no documento: true, e o nó de antes diz agora "Carla")» |
| **o prefixo `pessoa:`** no lugar de `membro:` | os dois primeiros passos passam; reprova o **controle (c)**: «a Ana conectada não ganhou a linha de pessoa dela, ou a ganhou no nó do membro dela (… "diz":"AnaSINAL E ESTADO——EM ESCUTA" …): a chave do membro colide com a da pessoa». O nó do membro virou a linha de pessoa |
| **a posição disfarçada de chave** (`membro:${membros.indexOf(membro)}`) | reprova no caso 15, com a frase do vermelho |

O segundo é o que o controle (c) guarda, e é por isso que ele existe.

## Os guardas, com os números

Na ponta do código, `5591aab3`. Os documentos vieram depois. O Playwright é o do `SEELE-MOD-PERFIS`, pelo
`PLAYWRIGHT=`: Chromium 151.0.7922.34 e WebKit 26.5.

- **A `linhas-pelo-id.cjs` inteira, no Chromium** (como no CI): saída 0, em uns 58 s (eram uns 55 s no `bec085df`, com
  os 14 casos), dentro dos 5 min do passo.
- **A mesma, no WebKit**, numa cópia de rascunho ao lado da bancada que troca o `chromium` pelo `webkit` (a cópia some
  no fim, e a troca não foi commitada): saída 0, com os 15 casos. No `bec085df` o caso 15 também reprova no WebKit (o
  rascunho, com os mesmos números do Chromium).
- **A `um-redesenho-por-quadro.cjs`**: o maior atraso foi de 29 ms na bateria, e de 27, 28 e 30 ms soltas, com a carga
  de 1 min em torno de 3 (outras sessões rodando). No R63 foram 28 a 31 ms. A bancada não tem FORA DO AR, e o conserto põe uma
  chave num `WeakMap`.
- **As 19 bancadas do job `bancadas`**, cada uma por nome, na ordem em que o `ci.yml` as escreve: 19 de 19.
- **Os 46 roteiros de `tools/roteiros/`**, numa chamada só, a lista lida do `ci.yml` (46 no `ci.yml`, 46 na pasta), com o
  `fora-do-ar.js` entre eles: «os 46 passaram», «carga limpa: nenhum script estourou». O `fora-do-ar.js` desenha
  `kaji, misato`, passa à revisão 2 com só a `misato`, depois sem grupo, e o servidor seguinte sem grupo.
- **`cargo --offline test -p seele-app --test frontend`**: 303 de 303, inclusive
  `o_fora_do_ar_mostra_so_o_nome_e_nunca_um_horario`.
- **`cargo --offline xtask check-runtime`**: passa, com as 12 bancadas de Node puro.
- **Em acréscimo, `cargo --offline test -p xtask`**: 89 de 89 no `xtask` (e os outros alvos do pacote, sem falha), incluindo
  `toda_bancada_da_pasta_roda_em_algum_lugar` e `todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`.

Os três comandos de Rust rodaram pelo `cargo-casa-falsa.sh` do scratchpad (`…/scratchpad/fora/`). É um script
executável que:
- dá a cada comando uma HOME falsa nova;
- mantém o `rustup`, o `cargo` e o `node` de verdade;
- desliga o `CARGO_TARGET_DIR` e as `XDG_*`;
- confere que o log não ficou vazio (318, 33 e 297 linhas) e que a HOME falsa não ganhou `.config/seele` (0 entradas
  nas três).

A primeira tentativa do `cargo test -p xtask` reprovou **um** teste, `a_carga_da_casca_passa_no_proprio_autoteste`, com
`TypeError: unsupported operand type(s) for |: 'type' and 'NoneType'` em `tools/carga-da-casca.py:690`. Não é do lote:
o `PATH` do script não tinha o Python do runner, e o `python3` que sobrava era o do sistema, o 3.9, que não aceita
`pathlib.Path | None`. Pus o Python 3.14 no `PATH` do script e rodei de novo: 89 de 89.

## O celular

**O FORA DO AR não tem adaptação no celular, e por isso a bancada do celular não rodou.** Conferido por `grep`:
- em `celular-*.js` não há `fora-do-ar`, `foraDoAr`, `membro-fora-do-ar` nem `data-fora` (0 ocorrências);
- o `celular-pessoas.js` é o único que mexe na lista (`#lista-roster`), e só por `li.pessoa`: o ouvinte delegado que
  abre a folha da pessoa (`closest("li.pessoa")`, linha 22) e a lista dos nomes (`querySelectorAll("li.pessoa")`, linha
  55). As linhas do FORA DO AR são `li.membro-fora-do-ar`, e nenhuma dessas buscas as alcança;
- a única menção é uma regra de CSS, `celular-texto.css:107`, que dá o tamanho da fonte à classe. Ela não depende do nó.

A chave mora num `WeakMap` do `tela-sessao.js`, sem atributo no DOM, e não muda a classe nem o texto da linha.

## O que ficou de fora

- **O app de verdade.** O app não rodou (restrição do lote). A medida é a da bancada, que usa o `ui/` real, o HTML, o CSS e
  o registro reais, e só a ponte simulada. A forma do que a ponte devolve é a do tipo (`{ id, nickname }`, em ordem de
  apelido).
- **Um apelido que muda.** O Rust reordena o FORA DO AR pelo apelido, e um membro renomeado pode ir para outro lugar da
  lista. Com a chave, o nó é o do membro e o `reconciliarOsFilhos` o move (e reescreve o texto, que é o dado mudando). O
  caso 15 não cobre isso: o pedido é a conexão e a entrada. Pela leitura do código, o nó segue o membro.
- **A Ana nos dois lugares por um instante.** Entre o retrato com ela conectada e a resposta do acessor, a tela a desenha
  em `presentes` e ainda no FORA DO AR. É o comportamento de antes do lote (a lista é buscada depois do retrato), e este
  lote não o muda: o controle (c) só prova que, nessa janela, as duas linhas não se confundem.

## Ressalvas

- O `8483ed28` (o guarda estático) é **permissivo de propósito**: aceita o `id` só dentro de `membro:${membro.id}`, mas
  não exige a chave. Quem exige é o caso 15, que é o guarda de verdade e roda no CI. Exigir a chave no `frontend.rs`
  também custaria um texto a mais para manter, e «existir» não é «funcionar».
- O vermelho (`b0df8fda`) é um commit com a bancada reprovando, de propósito, como o brief pede («o vermelho vem antes do
  conserto»). Quem fizer `git bisect` nesse trecho encontra o `b0df8fda` vermelho e o `5591aab3` verde.
