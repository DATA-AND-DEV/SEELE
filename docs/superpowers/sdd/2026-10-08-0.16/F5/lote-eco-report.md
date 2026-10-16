# Relatório do lote ECO (F5, 0.16)

O item da pendência 63 que o R63 deixou como limite conhecido (o M2 da revisão dele): quando a própria mensagem
pendente é gravada, ela troca de nó. A pessoa perde o link, o REMOVER, a seleção e o nó do leitor de tela, e isso
acontece em toda mensagem que ela manda. A F1 já faz a confirmada própria chegar pela FFI com a mesma
`client_message_id` da pendente. Este lote faz a casca usar essa chave.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-ad85a9e63e56d9d97`
- Ramo: `0.16/lotes-eco`, aberto no `e37509f4` (o `0.16/lotes` com o AX, o C64 e o R63).
- Brief: `.superpowers/sdd/lotes/lote-eco-brief.md`; a rodada 1, `.superpowers/sdd/lotes/lote-eco-rodada1.md`, com o
  veredito em `.superpowers/sdd/lotes/review-eco-report.md`.
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (o conserto da chave; na rodada 1, o do reconciliador e um só
  de comentário, cada um num commit só dele), `apps/seele-app/bancada/linhas-pelo-id.cjs` (o passo 14 (j) e o
  comentário do 14 (h); na rodada 1, o 14 (k), o caso 8 com dois vizinhos e os controles B1 e B2),
  `docs/pendencias.md` (a 63) e este relatório.
- Status: **DONE_WITH_CONCERNS** na primeira passada; a revisão aprovou com ressalvas (0 Altas, 1 Média, 3 Baixas), e
  a rodada 1 trata as quatro (a B3 como limite aceito). A preocupação que fica está em «Ressalvas»: o conserto só vale
  no app junto com o ramo da F1, e nenhum app foi aberto com os dois juntos.

Nada foi empurrado e nenhum subagente foi usado. O app, o microfone, o `xtask web` e a conformidade não rodaram, nada
foi baixado, e o fio não mudou. O `target` foi semeado por `cp -c` e nunca movido, com o `CARGO_TARGET_DIR` desligado.

## Os commits

| commit | o que entra |
|---|---|
| `e91da621` | **o vermelho**: o passo 14 (j) da `linhas-pelo-id.cjs`, com os dois controles. Reprova no `e37509f4` |
| `3944d2c8` | **o conserto**, só no `tela-sessao.js`: a chave da conversa é a do envio quando há uma, e o id quando não há |
| `2466426d` | a 63, este relatório e o comentário do 14 (h), que citava a chave `pendente:` |

Os da rodada 1 estão na seção «Rodada 1», no fim. Todas as mensagens estão em português e terminam em `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## O vermelho

O passo novo fica no fim do caso 14, depois do (i), na mesma página. A ponte simulada dá ao `messages` a forma da F1.
Os valores são os do teste dela, `a_confirmada_propria_chega_com_a_chave_da_pendente` (`git show
0.16/protocolo-eco:crates/seele-ffi/src/lib.rs`, linha 7782).

1. **O primeiro retrato** tem três mensagens de outras pessoas (1, 3 e 4, com `client_message_id: null`) e a
   pendente própria: `id` 0, `client_message_id` "77", `own` true, `estado` "Enviando". O corpo dela tem um endereço,
   que vira o `span.link-externo`, e ela tem o REMOVER. A bancada põe o foco no link e seleciona «minha mensagem,
   recém-escrita» no corpo.
2. **O segundo retrato** chega numa revisão só. A pendente sai, e entram:
   - a confirmada própria, com `id` 5, `client_message_id` "77" e `estado` "Confirmada";
   - a própria relida do histórico, a 2, com "31", uma chave que não é de pendente nenhuma;
   - uma de outra pessoa, a 7, com `null`.

   A de outra pessoa é a 7, e não a 6 do teste da F1, porque o MOD do passo (g) desenha o corpo da 6.
3. **O terceiro retrato** tira a 3, de outra pessoa. As que ficam guardam o nó: as de outras pessoas pelo id, e a
   relida e a gravada pela chave.
4. **Por fim, a bancada aperta o REMOVER**, que é o mesmo nó desde a pendente, e confirma.

As asserções do pedido:
- a `li` da gravada é `===` à da pendente, e está no documento;
- o foco segue no link;
- a seleção é a mesma;
- o REMOVER é o mesmo nó;
- nenhuma outra `li` mudou de nó: as de outras pessoas e o divisor estão no documento, na mesma ordem, e dizem o que
  diziam.

E o dado de agora no nó de antes:
- o `data-moderar-mensagem` do REMOVER é "5";
- o `data-mensagem` aponta, em `mensagens`, a mensagem de id 5;
- o selo do envio saiu;
- a caixa do REMOVER cita o corpo da gravada;
- o confirmar pede `remover_mensagem` com `message: 5`.

**No `e37509f4`** (`tela-sessao.js` com o sha256 `a838c5ca…e754`), a bancada reprova:

```
AssertionError [ERR_ASSERTION]: a própria mensagem foi gravada e a linha dela trocou de nó: o foco está em BODY,
o REMOVER não é o mesmo, e a seleção foi de "minha mensagem, recém-escrita" a ""
saida 1
```

**Os outros passos, medidos de uma vez.** Uma bancada para no primeiro `assert`. Por isso rodei uma cópia da bancada
no scratchpad que imprime o resultado do passo antes das asserções. Nada dela entrou em commit. No `e37509f4`, ela
mostrou:
- a `li`, o REMOVER e o foco perdidos: `mesmaLi` false, `mesmoRemover` false, o foco no `BODY` e a seleção «»;
- o REMOVER velho, fora do documento, ainda diz "0", e o selo ainda diz «enviando»;
- o clique nele não abre a caixa, e nada é pedido (`removeu: []`);
- na saída da 3, o foco segue no `BODY`.

**Os dois controles passam no `e37509f4`**, como devem:
- as linhas de outras pessoas e o divisor ficam, na mesma ordem;
- a 7 e a relida ganham um nó cada, que diz a própria mensagem;
- na saída da 3, nenhuma das que ficam troca de nó.

## O conserto

Em `desenharMensagens` (`tela-sessao.js`), duas linhas de código e os comentários delas:

```js
const chave = mensagem.client_message_id
  ? `envio:${mensagem.client_message_id}`
  : mensagem.id ? `mensagem:${mensagem.id}` : null;
```

**A escolha.** A pendente e a gravada só dividem uma coisa: a `client_message_id`. O id não serve, porque é 0 numa e
5 na outra. Por isso a chave do envio vem **antes** do id, e no mesmo espaço de nomes nas duas (`envio:`, e não mais
`pendente:`, que diria «pendente» numa mensagem gravada). A ponte só manda a chave nas próprias:
- na pendente;
- na confirmada que a substitui, na mesma revisão em que a pendente sai;
- na relida do histórico, com a chave que o servidor gravou.

A de outra pessoa chega com `null` e segue por `mensagem:<id>`. Os dois prefixos não se cruzam, então um id e uma
chave com o mesmo número não disputam um nó. A chave relida de outra execução não colide com uma pendente desta,
porque a metade alta é aleatória por processo (a F1). Se um dia vierem duas mensagens com a mesma chave, o
`reconciliarALista` dá o nó velho à primeira e um nó novo à segunda. É o que ele já fazia com chaves repetidas.

**O resto é do `vestirComo` e da reconciliação dos filhos**, sem mudança nenhuma:
- na `li` reaproveitada, o `span.envio` sai, porque a gravada não tem selo;
- o `data-mensagem` passa ao índice novo;
- no REMOVER, o `data-moderar-mensagem` passa de "0" a "5", e o `aria-label` é reescrito com a hora da gravada
  quando ela muda o minuto;
- os nós de texto do corpo e o link são os mesmos, e por isso a seleção e o foco ficam.

Numa sonda medi também o MOD do passo (g), que segue registrado e tem um acréscimo para o alvo "5". A nota dele
entra na `li` reaproveitada assim que a mensagem passa a ter id, com o foco, a seleção e o REMOVER no lugar. O caso
não confere isso.

**Quem lê o id da mensagem no nó**, conferido:
- **o REMOVER** (`camada-moderar.js:705-718`) tem o ouvinte delegado no `#lista-mensagens`. Ele lê o
  `data-moderar-mensagem` na hora do clique e procura o id em `mensagens`. A bancada aperta o mesmo nó e vê o pedido
  `remover_mensagem` com `message: 5`. No `e37509f4`, o nó velho saía do documento e o clique não fazia nada;
- **o `data-mensagem`**: a bancada confere que ele aponta, em `mensagens`, a mensagem 5. A busca
  (`tela-sessao.js:4855`) usa o mesmo atributo;
- **o celular** (`celular-conversa.js:46-66`) tem o ouvinte delegado. Ele lê o `data-mensagem`, pega o `id` em
  `mensagens` e aperta o `button[data-moderar-mensagem="<id>"]` da lista de agora. Os dois atributos mudam no nó,
  como acima. A bancada do celular passa inteira, com o G3 IGUAL (abaixo);
- **o responder**: não existe na casca. Não há botão de responder nem leitor de `replies_to` em `apps/seele-app/ui/`
  (`grep`); o único «reply» é o `ModReply` de `base.js`, que é o canal dos MODs. Não há o que conferir.

O `data-pendente` do TENTAR DE NOVO e do DESISTIR (`agirNaPendente`) só existe no selo das pendentes, e sai com ele.

## A reversão

O `tela-sessao.js` consertado foi copiado para o scratchpad. Um script trocou **só a expressão da chave** pela antiga,
e os comentários ficaram. A bancada reprova:

```
AssertionError [ERR_ASSERTION]: a própria mensagem foi gravada e a linha dela trocou de nó: o foco está em BODY,
o REMOVER não é o mesmo, e a seleção foi de "minha mensagem, recém-escrita" a ""
saida 1
```

O arquivo voltou da cópia. O `cmp` com a cópia e com o `git show HEAD:apps/seele-app/ui/tela-sessao.js` deu igual
nos dois, e o sha256 é `6e83f77c…0bc7`, antes e depois.

**Os controles provados contra um mutante cada**, pelo mesmo script. A cada um, o arquivo voltou e o `cmp` deu igual.
- **Sem conferir que a chave existe** (`envio:${mensagem.client_message_id}` em toda mensagem). As mensagens sem chave
  caem todas em `envio:null`. A bancada inteira reprova antes, no 14 (a): «alguém chegou ao servidor: o foco no link da
  mensagem 11 caiu (está em BODY)». No passo (j), medido pela cópia, a gravada passa, mas o **controle 1** reprova:
  - as `li` da 3 e da 4 trocaram de nó, e a ordem mudou;
  - na saída da 3, a 4 e a 7 trocaram de nó.
- **A própria pela autoria** (`envio:${mensagem.author}` nas próprias). A bancada inteira reprova antes, no 14 (h). No
  passo (j), o **controle 2** reprova: a relida tomou o nó da pendente, e a gravada ganhou um nó novo. O REMOVER que
  era da pendente passou a dizer "2", e o confirmar pediu `remover_mensagem` com `message: 2`: **outra mensagem**. É o
  «roubar o nó de outra» que o controle guarda.

## Os guardas, com os números

Na ponta do código, `3944d2c8`. Os documentos e o comentário do 14 (h) vieram depois. A bateria do navegador rodou já
com o comentário. O Playwright é o do `SEELE-MOD-PERFIS`, pelo `PLAYWRIGHT=`.

- **A `linhas-pelo-id.cjs`**, todos os casos (de 1 a 14, com o (j)): saída 0, em uns 50 s (eram uns 49 s), dentro dos
  5 min do passo.
- **A `um-redesenho-por-quadro.cjs`**: o maior atraso foi de 30 ms na bateria, e de 34, 30 e 26 ms soltas, com a
  carga de 1 min entre 6 e 7 (outras sessões rodando). No R63 foram 28 a 31 ms. A bancada não tem mensagem própria, e
  o conserto troca uma cadeia de chave.
- **As 19 bancadas do job `bancadas`**, cada uma por nome, como o `ci.yml` as escreve: 19 de 19.
- **Os 40 roteiros**, numa chamada só, como no CI: «os 40 passaram», «carga limpa: nenhum script estourou».
- **`cargo test --offline -p seele-app --test frontend`**: 296 de 296.
- **`cargo xtask check-runtime`**: passa, com as 12 bancadas de Node puro.
- **A bancada do celular inteira** (`python3 tools/bancada-do-celular.py <pasta no scratchpad>`): «a bancada passa»,
  com 110 casos e «G3 (chromium)» e «G3 (webkit)» IGUAL pixel a pixel. Levou 9 min com a carga alta.

Os dois comandos de Rust rodaram pelo `cargo-casa-falsa.sh` do scratchpad. É um script executável que:
- dá a cada comando uma HOME falsa nova;
- mantém o `rustup`, o `cargo` e o `node` de verdade;
- desliga o `CARGO_TARGET_DIR` e as `XDG_*`;
- confere que o log não ficou vazio (321 e 33 linhas) e que a HOME falsa não ganhou `.config/seele` (0 entradas nas
  duas).

A primeira tentativa do `check-runtime` reprovou porque o `node` não estava no `PATH` do script («não consegui rodar o
`node`»). Pus a pasta do `nvm` no `PATH` dele e rodei de novo.

## O que ficou de fora

- **O app de verdade com o ramo da F1.** O brief deixa isso para a integração e para a conferência da orquestradora.
  Sem o eco, a confirmada chega com `client_message_id: None` e cai em `mensagem:<id>`, como antes do lote: nada
  quebra, e nada melhora.
- **O 14 (h) ficou como estava.** Ele grava a primeira pendente **sem** o eco (`client_message_id: null` na
  confirmada), como a ponte fazia antes da F1, e por isso a `li` dela sai. O que ele guarda é que duas pendentes não
  casem pela posição, e isso segue valendo. Só o comentário dele mudou, para citar a chave nova. O caso com o eco é o
  (j).
- **A mensagem própria que muda de lugar ao ser gravada.** As pendentes vêm depois das confirmadas, e a gravada entra
  na ordem do id. Se outra pessoa gravar uma mensagem entre o envio e a gravação, a gravada pode precisar ir para antes
  dela. O `reconciliarALista` move o nó. Não medi se o foco atravessa esse movimento, e o caso não o cobre: o pedido é
  a gravação no mesmo lugar. **A revisão mediu: não atravessava** (o nó era o mesmo, mas tirado e posto, e o foco e a
  seleção caíam). A rodada 1 o consertou no reconciliador; ver «Rodada 1».

## Ressalvas

- **O conserto só vale no app junto com a F1** (`0.16/protocolo-eco`, `fe42c231`), que não está neste ramo. Tudo o
  que está acima foi medido com a ponte simulada na forma do teste dela.
- A bancada usa `estado: "Enviando"` na pendente, que é o caso de toda mensagem enviada. A troca de «sem resposta»
  para gravada, com o eco, passa pelo mesmo caminho (a chave e o `vestirComo`), mas nenhum caso a cobre com o eco.
  (Na rodada 1, o 14 (k) passou a cobri-la, e com a gravada mudando de lugar.)

## Rodada 1

A revisão (`.superpowers/sdd/lotes/review-eco-report.md`) deu o lote conforme e o aprovou com ressalvas: 0 Altas,
1 Média, 3 Baixas e 3 observações. O pedido da rodada é o `.superpowers/sdd/lotes/lote-eco-rodada1.md`. A ponta de
partida foi o `2466426d`.

### Os commits

| commit | achado | o que entra |
|---|---|---|
| `ad19f06d` | M1, o vermelho | o 14 (k), com o foco e a seleção e depois só com a seleção, e o caso 8 com dois vizinhos trocados (só a bancada) |
| `82be2db2` | M1, o conserto | `reconciliarOsFilhos` deixa no lugar o nó que tem o foco, ou, sem foco, o começo da seleção (só o `tela-sessao.js`) |
| `53a447a6` | B1 e B2 | a relida com a chave "7" e duas próprias gravadas sem chave (só a bancada) |
| `ce071c64` | O1 | só comentário no `tela-sessao.js` |
| este | M1 e B3 | a 63 e esta seção |

**O vermelho foi refeito uma vez, e só no meu ramo local.** Ele nasceu como `ac646d74`, sem o passo «só a seleção» do
14 (k). Eu o acrescentei com `--amend`, antes do conserto, para a preferência pela âncora da seleção também começar
vermelha. O `ac646d74` não existe em ramo nenhum.

### M1: quem tem o foco fica, e quem se move é o outro

**O cenário.** A própria mensagem pendente está no fim, sem resposta, com o foco no link e uma seleção no corpo. Antes
dela já está a mensagem de outra pessoa (a 14), que falou enquanto o enlace estava caído. A repetição da chave devolve
a linha que o servidor já tinha gravado, com o id de antes (a 13), e a gravada vai para antes da 14. Com só a chave do
lote, o nó era o mesmo, mas a maior subsequência crescente o movia: as posições velhas dos dois são `[4, 3]`, e o
desempate deixa a última no lugar. Mover é tirar e pôr, e o foco e a seleção caíam. Ao vivo isso não acontece, porque
o servidor anuncia na ordem do id.

**O vermelho** (`ad19f06d`), no `2466426d` (`tela-sessao.js` com o sha256 `6e83f77c…0bc7`):
- **o caso 8**, com dois vizinhos da sala trocando de lugar e o foco no nome do que vem para a frente, que é o que a
  subsequência movia. É a bancada inteira que reprova, e já aqui:

  ```
  AssertionError [ERR_ASSERTION]: p-007 e p-006 trocaram de lugar e o foco no nome de p-007 caiu (está em BODY):
  moveu-se ["p-007"], e quem tem o foco fica
  ```
- **o 14 (k)**, numa cópia da bancada sem as asserções do caso 8:

  ```
  AssertionError [ERR_ASSERTION]: a própria mensagem foi gravada antes de uma que já estava na tela e o foco no link
  dela caiu (está em BODY): o nó é o mesmo, mas foi tirado e posto de novo (movidas: ["13"])
  ```

  Medido pela mesma cópia, sem asserção nenhuma do caso 14: a `li` é a mesma e a ordem é a do retrato nos dois passos
  do (k). No primeiro, o foco vai ao `BODY` e a seleção a «». No segundo, sem foco nenhum (a bancada tira o foco com
  `blur`) e só com a seleção no corpo da 23, a seleção vai a «» (movidas: `["23"]`).

**O conserto** (`82be2db2`, só o `tela-sessao.js`). Ele fica no caminho que só roda quando há o que mover; o caminho
rápido, quando nada muda de lugar, não mudou.
- `indiceDoQueGuardaOFoco` acha, entre os nós desejados que já estão ali, o que tem dentro o `document.activeElement`.
  Sem o foco em nenhum deles, acha o que tem o `anchorNode` da seleção.
- Antes da subsequência, os nós fora de ordem em relação a ele saem da conta: os que vêm antes dele e estavam depois, e
  os que vêm depois e estavam antes.
- Tudo o que sobra é compatível com ele, e por isso a maior sequência do que sobra passa por ele: ele fica, e os outros
  se movem em volta.

Das duas formas sugeridas, escolhi esta, e não «a melhor que termina nele mais a melhor que começa nele». Ela reusa a
`maiorSequenciaCrescente` sem mudança: é um laço sobre os valores, mais uma função de nove linhas. As duas dão o mesmo
comprimento, e a bancada prova o resultado.

Depois do conserto:
- no caso 8 com dois vizinhos, quem se move é o p-006, e o foco fica no p-007;
- no 14 (k), quem se move é a 14 (movidas: `["14"]`), e o foco no link e a seleção ficam;
- só com a seleção, ela fica;
- o passo de antes do caso 8 (a primeira da sala vai para o fim, com o foco no meio) continua movendo um nó só.

**O preço, medido numa sonda** (uma cópia da bancada com o caso 8 invertido). Quando quem tem o foco é justo quem mudou
de lugar (a última da sala vai para a frente, com o foco no nome dela), ficam no lugar ele e mais ninguém, e as outras
18 linhas do grupo se movem. Antes, movia-se uma, e o foco caía. A bancada não prende isso, que é o preço, e não a
promessa.

**As reversões**, pelo mesmo script: aplica o mutante, roda a bancada, devolve o arquivo da cópia e confere com o
`cmp`. Todas voltaram iguais, com o sha256 `03631fd1…` (o do `82be2db2`) antes e depois.
- **sem a preferência** (`const preferido = -1`):
  - a bancada inteira reprova no caso 8, com a frase do vermelho;
  - a cópia sem as asserções do caso 8 reprova no 14 (k), também com a frase do vermelho (movidas: `["13"]`).
- **sem a âncora da seleção** (só o foco): a bancada inteira reprova no 14 (k), só com a seleção: «a própria mensagem
  foi gravada antes de uma que já estava na tela, sem foco nenhum, e a seleção no corpo dela foi a "" (movidas:
  ["23"])».

### B1 e B2: os dois mutantes que passavam (`53a447a6`, só a bancada)

- **B1:** a chave da relida do 14 (j) passa de "31" a "7", que é o id da mensagem de outra pessoa do mesmo passo. É o
  que os prefixos `envio:` e `mensagem:` garantem, e nenhum controle prendia. O comentário do passo diz por que a chave
  não é mais a do teste da F1.
- **B2:** duas próprias gravadas com `client_message_id: null` (a 8 e a 9), como chegam de um servidor antigo ou no
  app sem a F1. A saída da 3 confere que elas guardam o nó.

Cada mutante rodou contra uma cópia da bancada do `2466426d` e contra a nova. A cada um, o arquivo voltou e o `cmp`
deu igual.

| mutante | a bancada de antes | a bancada nova |
|---|---|---|
| sem os prefixos (`${cmid}` e `${id}`) | saída 0 | «a 3 saiu e as mensagens [7] trocaram de nó» |
| `own ? envio:${cmid} : mensagem:${id}` | saída 0 | «a 3 saiu e as mensagens [9] trocaram de nó» |

### B3: a chave não confere o `own` (limite aceito, sem código)

A chave do envio vale para toda mensagem que tem `client_message_id`, sem conferir `mensagem.own`. Fica assim, e a
razão é esta.

**Hoje, só as próprias trazem a chave.** A revisão conferiu os três produtores da `Message`:
- o `Room::apply` da F1 grava a chave só na própria;
- as pendentes da FFI são próprias por construção;
- a ponte do navegador dobra os quadros no mesmo `Room`.

O fio leva a chave de todos, e o filtro mora na F1.

**E, se o filtro regredisse, uma chave de outra máquina não colidiria com uma pendente daqui.** A metade alta da chave
é sorteada por processo. Para tomar o nó de uma pendente, a mensagem de outra pessoa teria de trazer a mesma chave de
64 bits, vinda de outro processo. Duas mensagens com a mesma chave também não chegam juntas: o servidor tem
`UNIQUE (author_id, client_message_id)`, e a F1 tira a pendente na mesma revisão em que a confirmada chega.

Conferir o `own` custaria uma condição. Mas ele exigiria um controle com uma mensagem de outra pessoa **com** chave,
que é um dado que a F1 não entrega. Fica dito aqui, e o código não muda.

### O1, O2 e O3

- **O1** (`ce071c64`, só comentário): a docstring de `desenharMensagens` diz a chave na ordem do código: a do envio
  quando a mensagem a traz, e senão o id. E o comentário da chave diz que quem tira o selo do envio é a reconciliação
  dos filhos, e que o `vestirComo` reescreve o id e os `data-*`. A 63 diz o mesmo. E o comentário do conserto do M1
  diz o que nenhuma ordem evita: o nó com o foco que muda de pai (a pessoa que entra numa sala) vem de fora, e é
  posto.
- **O2 e O3:** sem mudança. O foco num TENTAR DE NOVO que deixa de existir, e o link dentro de um corpo que um MOD
  substitui, saem com o nó que os tinha, como a revisão diz.

### Os guardas da rodada

Na ponta do código, `ce071c64` (o `tela-sessao.js` com o sha256 `37f1620e…7928`). O O1 é um commit só de comentário, e
eu o refiz uma vez com `--amend`, depois da bancada do celular: ele ganhou uma frase sobre o nó com o foco que muda de pai.
A bancada do celular rodou com o arquivo de antes dessa frase (sha256 `10d90f6b…`), que só difere dela em comentário.
Todo o resto rodou na ponta. O Playwright é o do `SEELE-MOD-PERFIS`, pelo `PLAYWRIGHT=`.

- **A `linhas-pelo-id.cjs`**, todos os casos (de 1 a 14, com o 8 de dois vizinhos, o (j) com o B1 e o B2, e o (k)):
  saída 0, em uns 54 s, dentro dos 5 min do passo.
- **A `um-redesenho-por-quadro.cjs`**, intercalada, com o `tela-sessao.js` do `2466426d` (antes) e o da ponta do
  conserto (depois), três voltas; a carga de 1 min estava em 2,6 no fim. O maior atraso:

  | volta | antes | depois |
  |---|---|---|
  | 1 | 27 ms | 30 ms |
  | 2 | 32 ms | 27 ms |
  | 3 | 25 ms | 29 ms |

  Não piorou. Na bateria das 19, foram 28 ms. A bancada é de `PersonState`, e a ordem não muda: o reconciliador fica
  no caminho rápido, que o conserto não toca.
- **As 19 bancadas do job `bancadas`**, cada uma por nome, como o `ci.yml` as escreve: 19 de 19.
- **Os 40 roteiros**, numa chamada só, como no CI: «os 40 passaram», «carga limpa: nenhum script estourou».
- **`cargo test --offline -p seele-app --test frontend`**: 296 de 296.
- **`cargo xtask check-runtime`**: passa, com as 12 bancadas de Node puro.
- **A bancada do celular inteira**: «a bancada passa», com 110 casos e «G3 (chromium)» e «G3 (webkit)» IGUAL pixel a
  pixel. Levou uns 14 min.

Os dois comandos de Rust rodaram pelo mesmo `cargo-casa-falsa.sh`: os logs não ficaram vazios (311 e 33 linhas), e a
HOME falsa terminou com 0 entradas em `.config/seele` nas duas.

**E no WebKit, numa cópia da bancada no scratchpad** (o `chromium()` trocado pelo `webkit` do mesmo Playwright). Nada
dela entrou em commit.
- Na ponta, o caso 8 de dois vizinhos e os dois passos do 14 (k) dão o mesmo que no Chromium:
  - o foco fica no p-007, e se move o p-006;
  - o foco fica no link, a seleção fica, e se move a 14;
  - só com a seleção, ela fica, e se move a 24.
- Sem a preferência, o 14 (k) dá no WebKit o defeito da revisão: o foco no `BODY` e a seleção a «».
- A bancada inteira não passa no WebKit, e isso não é deste lote. Ela reprova no 14 (a), «alguém chegou ao servidor e
  a rolagem da conversa foi a 297», com e sem a preferência. A bancada é calibrada para o Chromium.
