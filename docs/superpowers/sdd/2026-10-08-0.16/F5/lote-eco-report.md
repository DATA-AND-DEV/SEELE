# Relatório do lote ECO (F5, 0.16)

O item da pendência 63 que o R63 deixou como limite conhecido (o M2 da revisão dele): quando a própria mensagem
pendente é gravada, ela troca de nó. A pessoa perde o link, o REMOVER, a seleção e o nó do leitor de tela, e isso
acontece em toda mensagem que ela manda. A F1 já faz a confirmada própria chegar pela FFI com a mesma
`client_message_id` da pendente. Este lote faz a casca usar essa chave.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-ad85a9e63e56d9d97`
- Ramo: `0.16/lotes-eco`, aberto no `e37509f4` (o `0.16/lotes` com o AX, o C64 e o R63).
- Brief: `.superpowers/sdd/lotes/lote-eco-brief.md`.
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (um commit, só dele),
  `apps/seele-app/bancada/linhas-pelo-id.cjs` (o passo 14 (j), e o comentário do 14 (h)), `docs/pendencias.md` (a 63)
  e este relatório.
- Status: **DONE_WITH_CONCERNS**. A preocupação está em «Ressalvas»: o conserto só vale no app junto com o ramo da F1,
  e nenhum app foi aberto com os dois juntos.

Nada foi empurrado e nenhum subagente foi usado. O app, o microfone, o `xtask web` e a conformidade não rodaram, nada
foi baixado, e o fio não mudou. O `target` foi semeado por `cp -c` e nunca movido, com o `CARGO_TARGET_DIR` desligado.

## Os commits

| commit | o que entra |
|---|---|
| `e91da621` | **o vermelho**: o passo 14 (j) da `linhas-pelo-id.cjs`, com os dois controles. Reprova no `e37509f4` |
| `3944d2c8` | **o conserto**, só no `tela-sessao.js`: a chave da conversa é a do envio quando há uma, e o id quando não há |
| este | a 63, este relatório e o comentário do 14 (h), que citava a chave `pendente:` |

Todas as mensagens estão em português e terminam em `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

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
  a gravação no mesmo lugar.

## Ressalvas

- **O conserto só vale no app junto com a F1** (`0.16/protocolo-eco`, `fe42c231`), que não está neste ramo. Tudo o
  que está acima foi medido com a ponte simulada na forma do teste dela.
- A bancada usa `estado: "Enviando"` na pendente, que é o caso de toda mensagem enviada. A troca de «sem resposta»
  para gravada, com o eco, passa pelo mesmo caminho (a chave e o `vestirComo`), mas nenhum caso a cobre com o eco.
