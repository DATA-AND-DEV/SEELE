# Lote K5 da F5: relatório

Base `0f94be66` (`0.16/lotes`). Branch `0.16/lotes-k5`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af94d92c0383ba2a2`. Quatro
commits de código e este relatório; depois, nove commits da rodada 1 de
conserto (seção 10) e este relatório de novo. Todos locais: sem push, tag,
release ou Actions.

| commit | item | o que muda |
|---|---|---|
| `6be3d244` | **P49-7c** (fecha a parte daqui) | o som de uma contribuição assenta «pronta» com o tamanho do arquivo e só pede os bytes, decodifica e confere o teto no pedido de tocar; `montarSom` se divide em `montarSom`, `esperarOPedidoDeTocar`, `carregarSom`, `pedirParaTocar` e `falharOSom`; os docs de `LIMITES_DA_REGIAO` e `LIMITES_DO_CARTAO` |
| `d9f481ee` | **P49-8** (fecha) | o destino que tomou os sons e saiu do documento solta o tocador, devolve o bolso, esquece a anotação e devolve a marca (`devolverOsSons`); a segunda volta de `calarOsSonsQueSairamDaTela` a oferece aos destinos que desenham (`tomarOsSonsDevolvidos`); a carga a caminho é cancelada e dita |
| `8cadbc27` | **P51-25** (a parte da janela; opcional) | o dono sem `bytesDoSom` é dito pelo nome no `falhou` |
| `bf906db8` | P49-7c e P49-8, comentário | o que o adiamento segura em cada destino: do pacote, o caminho; do servidor, o `data:` |

Arquivos mudados (`0f94be66..bf906db8`), só os três da lista do lote:
- `apps/seele-app/ui/mods-regiao.js`
- `apps/seele-app/ui/base.js` (só no `d9f481ee`)
- `apps/seele-app/bancada/regiao-do-mod.cjs`

Nada no Rust, no fio, na API congelada (`api/v1..v5.json` e `api/README.md`
intocados), na CSP, nos arquivos da F6. Nenhuma dependência nova, e nada
baixado.

## 1. P49-7c: o som de uma contribuição só decodifica quando o MOD manda tocar (`6be3d244`)

**Produto (`mods-regiao.js`).**
- `async montarSom(` toma a marca como antes, antes de qualquer `await`. Num
  renderer de contribuição, o destino que a toma guarda a montagem
  (`{ plano, estado, origem }`) em `somDaMidia.adiado` e chama
  `esperarOPedidoDeTocar`; fora de uma contribuição, `await this.carregarSom(…)`,
  como antes.
- `esperarOPedidoDeTocar(elem)`: a figura assenta «pronta», e a anotação diz
  «pronta» com o tamanho do arquivo no motivo (`12 bytes de arquivo, já
  conferidos; o som decodifica quando o MOD mandar tocar`). Sem WebAudio na
  janela, o `falhou` sai aqui, e não no pedido: uma figura «pronta» esconderia
  um som que nunca vai tocar. Depois, `aplicarTocando(elem)`.
- `carregarSom(elem, montagem)` é o corpo antigo de `montarSom` depois da
  marca: os bytes (`som_do_mod` ou o `data:`), `decodeAudioData`, o teto do
  bolso com a mesma recusa (figura «cheia», `recusada` com os números, linha no
  registro), o tocador, o botão fora do cartão, e `aplicarTocando`.
- `aplicarTocando` sem tocador passa a `pedirParaTocar(elem, som)`. Só no
  destino que segura os sons, e só para um som adiado; a declaração vale na
  mudança, como com tocador (um `false` é guardado como aplicado, e o `true`
  seguinte é o pedido). O pedido chama `carregarSom`, e o que lançar fora dos
  caminhos de recusa é dito como o `.catch` de `montarMidia` diz.
- `falharOSom` junta as palavras do `falhou` (figura, anotação, evento,
  registro), que eram uma closure de `montarSom`.
- **Docs.** `LIMITES_DA_REGIAO.bytesDeSomDecodificado` diz que cada superfície
  tem o seu bolso (doze superfícies, doze bolsos, além do da faixa) e que a
  decodificação é de uma vez, na montagem ou, numa contribuição, no pedido de
  tocar. `LIMITES_DO_CARTAO.bytesDeSomDecodificado` ganha o parágrafo «Numa
  contribuição, o som só é decodificado quando o MOD o manda tocar». O doc de
  `montarSom` ganha «E só decodificam quando o MOD manda tocar», e as menções
  ao teto do som decodificado passam de `montarSom` para `carregarSom`.
  Nenhum texto novo escreve «API 3».

**Prova** (`regiao-do-mod.cjs`, seção 17,
`oSomDaContribuicaoSoDecodificaQuandoOModMandaTocar`), com dois renderers de
perfil de cartão e o mesmo `sonsDaContribuicao`, como as provas que já existiam:
- pelos dois caminhos, o do pacote e o do servidor do MOD, um som declarado
  `tocando: false` não chama `bytesDoSom` nem `decodeAudioData`; as figuras dos
  dois destinos assentam «pronta»; há uma anotação só, «pronta», de cartão, com
  `12 bytes` no motivo;
- depois de um `tocando: true` nos dois destinos: um pedido a `som_do_mod` (no
  do pacote), uma decodificação, uma fonte tocando e um «tocando» ao MOD;
- o teto: um som acima de `LIMITES_DO_CARTAO.bytesDeSomDecodificado` não é
  recusado na montagem; no pedido de tocar, a figura vira «cheia», o MOD ouve
  `recusada` com «teto» e o número do teto, o registro recebe ««grande»
  recusada: … decodificado», e não há fonte nem bytes no bolso.

## 2. P49-8: o som muda de destino (`d9f481ee`)

**Produto.**
- `calarSonsForaDaTela(cartoesPintam)`, depois de calar como antes: se este
  renderer segura os sons da contribuição e o nó dele saiu do documento,
  `devolverOsSons()`.
- `devolverOsSons()`, por som que chegou a `montarSom`:
  - o pedido de tocar cuja carga ainda estava a caminho (`carregando`) e que o
    MOD ainda quer é dito como a recusa `SOM_FORA_DA_TELA`, como o pedido que
    esperava o áudio da janela já era (S-m5); `estado.carga` sobe, e os bytes e
    a decodificação que chegarem depois não viram tocador aqui;
  - solta o tocador (`estado.som.soltar()`), devolve ao bolso o
    `bytesDoSom`, zera a recusa já dita ao registro, esquece a anotação;
  - guarda o adiamento, como um destino que nunca tomou os sons, com a figura
    «pronta»;
  - e devolve a marca: `sonsDaContribuicao.tomados = false`, `tomouOsSons = false`.
- `tomarOsSonsDevolvidos()`: num renderer de contribuição cujo nó está no
  documento, com a marca vaga e algum som adiado, toma a marca e assenta cada
  som adiado por `esperarOPedidoDeTocar`. **Ele não pede os bytes por tomar a
  marca**: dá a declaração de cada som por aplicada (`declarado = quer`), como
  `calarSonsForaDaTela` faz com o som que sai da tela. O `tocando` religado é o
  pedido de tocar, e toca ali, do começo.
- `calarOsSonsQueSairamDaTela()` (`base.js`) faz duas voltas por
  contribuição: a primeira cala cada destino (e o que saiu devolve a marca); a
  segunda, só quando há `contribuicao.sons` e a marca está vaga, oferece-a aos
  destinos em que a contribuição desenha (`aContribuicaoPinta`), na ordem de
  `montadas`, até um tomar. Numa volta só, um destino visitado antes do que
  saiu acharia a marca tomada.
- Para a recusa do pedido a caminho, `recusou` passou a ser amarrado em
  `montarSom` (e guardado em `somDaMidia.recusar` desde ali), e a montagem
  inteira fica em `somDaMidia.montagem`.
- **Docs.** «Ela não é devolvida: os destinos saem juntos» virou «Ela volta
  quando o destino que a tomou sai da tela» no doc de `montarSom`; os docs de
  `aplicarTocando`, `calarSonsForaDaTela`, `sonsDaContribuicao` e `somDaMidia`
  dizem a devolução; em `base.js`, o doc de `montarContribuicao` diz que
  redesenhar não muda o som de lugar (a montagem é reaproveitada) e quem o
  remonta é a varredura, e o de `calarOsSonsQueSairamDaTela` diz as duas voltas.
- `bf906db8` só acrescenta o comentário de `montarSom` sobre o que o adiamento
  segura em cada destino (ver as preocupações, item 4).

**Prova** (`regiao-do-mod.cjs`, seção 18, `oSomDaContribuicaoMudaDeDestino`).
A varredura é **a de `base.js`**: `varreduraDeBase` recorta
`aContribuicaoPinta` e `calarOsSonsQueSairamDaTela` do arquivo e os roda no
contexto da bancada, como `registroDeVerdade` já recortava `donoDaRegiao`. A
bancada ganhou `janela` e `rodar` no `bancada()` para isso.
- dois renderers com o mesmo `sonsDaContribuicao`, a vinheta `tocando: true`;
  o primeiro toma o som e toca;
- o nó do primeiro sai do documento e a varredura roda: o som para («pausada»
  uma vez); a marca está com o segundo; o primeiro tem zero tocadores, zero
  bytes no bolso e nenhuma anotação; o segundo tem uma anotação «pronta», e
  nada foi decodificado de novo;
- o MOD religa o `tocando` (`false`, depois `true`, nos dois): nenhuma
  recusa, uma fonte tocando, um tocador só, no segundo;
- num segundo passo, o nó do primeiro volta, a varredura roda e o MOD religa de
  novo: continua um tocador só, no segundo, sem recusa, e a figura do primeiro
  diz «pronta»;
- **no meio da decodificação**: a decodificação do primeiro fica presa, o nó
  dele sai e a varredura roda; quando ela termina, não vira tocador nem conta
  no bolso, a marca está com o segundo, e o pedido é dito ao MOD e ao registro
  como recusado por sair da tela; o `tocando` religado toca no segundo.

## 3. P51-25, a parte da janela (`8cadbc27`)

Em `carregarSom`, antes do `try` dos bytes: um som do pacote com
`typeof this.dono.bytesDoSom !== "function"` chega ao MOD como `falhou` com
`porque` «o dono desta região não oferece bytesDoSom», e ao registro como
«mídia «m» falhou ao carregar: o dono desta região não oferece bytesDoSom».
O som do servidor não usa `bytesDoSom`, e não passa por aqui. O doc do contrato
do dono, no construtor, diz o motivo. **Prova:** seção 19,
`oDonoSemBytesDoSomDizPorQue`.

## 4. RED e GREEN

**RED**, as três provas novas contra o código de `0f94be66` (antes de qualquer
conserto), `node apps/seele-app/bancada/regiao-do-mod.cjs`, saída 1:

```
FALHOU — o som da contribuição só decodifica quando o MOD manda tocar · do pacote: uma contribuição declarada com `tocando: false` pediu os bytes do som ou o decodificou na montagem — cada contribuição segura o som inteiro decodificado sem que ninguém o mande tocar: {"pedidos":["som/sino.wav"],"decodificados":[12]}
FALHOU — o som da contribuição só decodifica quando o MOD manda tocar · do pacote: o som que espera o pedido de tocar não ficou anotado «pronta», uma vez, com o tamanho do arquivo — o diagnóstico não diria que ele chegou e coube: [{"situacao":"pronta","motivo":"","cartao":true}]
FALHOU — o som da contribuição só decodifica quando o MOD manda tocar · do servidor do MOD: uma contribuição declarada com `tocando: false` pediu os bytes do som ou o decodificou na montagem — […]: {"pedidos":[],"decodificados":[1]}
FALHOU — o som da contribuição só decodifica quando o MOD manda tocar · do servidor do MOD: o som que espera o pedido de tocar não ficou anotado «pronta», uma vez, com o tamanho do arquivo — […]
FALHOU — o som da contribuição só decodifica quando o MOD manda tocar · o teto, no pedido de tocar: o som acima do teto foi recusado na montagem, sem ninguém o mandar tocar — o teto valeu antes da decodificação que ele mede: [{"nome":"midia","chave":"grande","estado":"recusada","porque":"o som decodificado ocupa 67108872 bytes, e com os 0 já de pé passa do teto de 67108864 bytes de som deste lugar"}]
FALHOU — o som da contribuição muda de destino: a marca dos sons não mudou de destino na varredura: o som fica preso no destino que saiu da tela — {"saiu":true,"outro":false,"tomados":true}
FALHOU — o som da contribuição muda de destino: o destino que saiu da tela segurou o tocador, o som decodificado no bolso, ou a anotação: 1 tocador(es), 384000 bytes, [{"situacao":"pronta","motivo":"","cartao":true}]
FALHOU — o som da contribuição muda de destino: o destino que tomou a marca pediu o som sem um pedido de tocar, ou não o anotou «pronta»: 1 decodificação(ões), []
FALHOU — o som da contribuição muda de destino: o `tocando` religado foi recusado como fora da tela, com a contribuição desenhada noutro destino: [{"nome":"midia","chave":"vinheta","estado":"recusada","porque":"o lugar deste som saiu da tela — uma página fechada, ou um cartão que o SEELE não desenha agora —, e o produto não liga um som cujo lugar saiu da tela"}]
FALHOU — o som da contribuição muda de destino: o `tocando` religado não tocou no destino que desenha, ou tocou em dois: 0 fonte(s), 1 tocador(es), 0 no destino que desenha
FALHOU — o som da contribuição muda de destino: o nó de quem saiu voltou, e o som não ficou com um tocador só, no destino que o tem: 1 tocador(es), 1 fonte(s), recusas [ … a recusa de fora da tela … ]
FALHOU — o som da contribuição muda de destino · no meio da decodificação: a decodificação terminou depois de o lugar sair da tela e virou tocador no destino que já não segura o som: 1 tocador(es), 384000 bytes, a marca com o outro: false
FALHOU — o som da contribuição muda de destino · no meio da decodificação: o `tocando` religado não tocou no destino que tomou a marca: 0 fonte(s), 1 tocador(es)
FALHOU — o dono sem bytesDoSom: um dono sem `bytesDoSom` fez o som chegar ao MOD com o tropeço do motor, e não com o que falta: {"nome":"midia","chave":"m","estado":"falhou","porque":"this.dono.bytesDoSom is not a function"}
FALHOU — o dono sem bytesDoSom: o som sem `bytesDoSom` não chegou ao registro com o que falta: ["mídia «m» falhou ao carregar: this.dono.bytesDoSom is not a function"]
```

Cada um vermelho pelo motivo certo: decodificar na montagem, a recusa pelo teto
na montagem, a marca presa no destino que saiu com a recusa de fora da tela, e o
`TypeError` do motor no lugar do nome.

**GREEN**, em `bf906db8`:
- `regiao-do-mod.cjs`: «as 43 provas e a validação de resumo por linhas
  passam» (eram 40; 41 em `6be3d244`, 42 em `d9f481ee`);
- `cargo xtask check-runtime`: as sete bancadas passam, antes de cada commit;
- `cargo test -p seele-app --test frontend`: 262 passaram, antes de cada commit;
- `cargo test -p xtask` (inclui `mods-regiao.js` e `base.js` por
  `include_str!`): 161 passaram; `cargo xtask check-api`: verde (17 eventos);
- `cargo test -p seele-conformance --test mod_de_referencia` (lê os dois
  arquivos): 4 passaram;
- `diagnostico-de-mods.cjs` no Chromium do Playwright do SEELE-MOD-PERFIS, com
  a CSP do produto: as sete provas passam, antes e depois (o som de
  contribuição sem alvo em dois canais, a varredura nos cinco caminhos);
- `cargo fmt --all -- --check`: limpo. Nenhum crate Rust mudou, e o clippy não
  tinha o que rodar.

## 5. As reversões

Cada uma desfaz um pedaço do conserto, roda a `regiao-do-mod.cjs`, e devolve o
arquivo da cópia, conferido byte a byte (`filecmp`, e as somas `sha256` dos
três arquivos conferidas no fim). Todas saíram com 1, e todas restauraram.

| reversão | o que ficou vermelho |
|---|---|
| 7c, em `6be3d244`: `montarSom` chama `carregarSom` na montagem da contribuição | as cinco linhas do 7c: os bytes e a decodificação na montagem pelos dois caminhos, a anotação sem o tamanho, e o teto «recusado na montagem» |
| 7c, na final: o mesmo | as mesmas cinco, e «o pedido de tocar que decodificava quando o lugar saiu da tela foi desfeito em silêncio» (o som decodificado na montagem já não estava a caminho) |
| 7c: a anotação «pronta» sem o tamanho do arquivo | «não ficou anotado «pronta», uma vez, com o tamanho do arquivo», nos dois caminhos |
| **P49-8: não devolver a marca** (sem a chamada a `devolverOsSons` em `calarSonsForaDaTela`) | **«o `tocando` religado foi recusado como fora da tela»**, com a frase de `SOM_FORA_DA_TELA`; e a marca presa, o tocador e os 384000 bytes no destino que saiu |
| **P49-8: devolver a marca sem remontar no segundo** (sem a segunda volta de `calarOsSonsQueSairamDaTela`, em `base.js`) | **«o `tocando` religado não tocou no destino que desenha: 0 fonte(s), 0 tocador(es)»**, e a marca vaga em ninguém |
| P49-8: a carga a caminho não é cancelada (sem `estado.carga += 1` em `devolverOsSons`) | «a decodificação terminou depois de o lugar sair da tela e virou tocador no destino que já não segura o som: 1 tocador(es), 384000 bytes», e dois tocadores no religar |
| P49-8: o pedido a caminho desfeito em silêncio (sem o `recusar` em `devolverOsSons`) | «foi desfeito em silêncio: ao MOD [], ao registro []» |
| P49-8: quem toma a marca não dá a declaração por aplicada (sem `som.declarado = som.quer` em `tomarOsSonsDevolvidos`) | «o destino que tomou a marca pediu o som sem um pedido de tocar: 2 decodificação(ões)», e o som voltando a tocar sozinho na varredura |
| P51-25: sem a conferência de `bytesDoSom` | as duas linhas do dono sem `bytesDoSom`, com «this.dono.bytesDoSom is not a function» |

## 6. O que não rodou

- **As WebViews de verdade**: o WKWebView do macOS, o WebView2 do Windows e o
  WebKitGTK do Linux. O som de contribuição adiado, a mudança de destino e a
  recusa no pedido de tocar foram medidos no DOM de mentira da bancada e, no que
  a `diagnostico-de-mods.cjs` cobre, no Chromium do Playwright — que é o motor
  do WebView2, e não o WebView2.
- A `diagnostico-de-mods.cjs` não tem um caso de mudança de destino: ela prova
  que o som sem alvo assenta nos dois canais, toca uma vez e para nos cinco
  caminhos de saída da tela. A mudança de destino só é medida na
  `regiao-do-mod.cjs`, com a varredura de verdade recortada de `base.js`.
- A bateria inteira, pelas regras do lote.

## 7. Para a v6

Frases novas que este lote não pôs no `api/README.md`:
- **«Numa contribuição, o som só é decodificado quando o MOD o manda tocar.
  A recusa pelo teto do som decodificado — `recusada`, com os números no
  `porque` — chega quando o MOD pede para tocar, e não na montagem.»** O
  primeiro toque de uma contribuição espera a decodificação.
- **O som de uma contribuição sem alvo muda de destino** (com a decisão da
  rodada 1): «Quando o destino em que o som de uma contribuição toca sai da
  tela, o som para ali (`pausada`) e recomeça, do começo, noutro destino que a
  desenhe (`tocando`). Um som que não tocava — terminado, parado pelo MOD, ou
  recusado — não volta pela troca: o próximo `tocando: true` o toca no destino
  novo.»
- **O religar de uma contribuição só existe na v6.** Hoje o conteúdo de uma
  contribuição não muda depois de registrado (ver as preocupações, item 1): o
  «próximo `tocando: true`» de cima, e o 7c para um som declarado parado, só
  valem quando a v6 deixar o MOD atualizar uma contribuição. **Hoje o P49-8
  vale para o som que tocava**: a linha de quem tomou o som sai do documento
  enquanto a contribuição desenha noutro destino, e o som recomeça ali.
- O pedido de tocar que ainda decodificava quando o lugar saiu da tela chega ao
  MOD como `recusada`, com o motivo de `SOM_FORA_DA_TELA`, como o que esperava
  o áudio da janela.

## 8. Para o K4

- `carregarSom` é o único lugar que pede os bytes e decodifica, nos dois
  momentos: na montagem, para o que não é contribuição (`montarSom`), e no
  pedido de tocar, para a contribuição (`pedirParaTocar`). O mapeamento do 7a
  (`som-grande-demais-decodificado` → figura «cheia», `recusada` com os
  números) no `catch` dos bytes de `carregarSom` vale nos dois.
- A conferência de `bytesDoSom` (P51-25) fica antes do `try` dos bytes.
- Toda volta depois de um `await` em `carregarSom` pergunta `deAlguem()`, que
  inclui `estado.carga`: uma recusa nova ali deve perguntar o mesmo, senão o
  destino que devolveu os sons no meio do caminho a diria.
- O molde da prova numa contribuição é a seção 17 (`sonsDaContribuicao` com
  `tocando: false`, depois `true`), e o da recusa pelo teto no pedido de tocar
  é o bloco «o teto, no pedido de tocar».

## 9. Preocupações

1. **No produto de hoje, o conteúdo de uma contribuição não muda depois de
   registrado.** `montarContribuicao` devolve a montagem guardada sem
   reaplicar, e o registro não tem atualização: um MOD muda uma contribuição
   revogando e registrando outra, com outro `sons`. Por isso:
   - um som de contribuição declarado `tocando: false` nunca recebe um pedido
     de tocar no produto — fica «pronta» sem decodificar até ser revogado.
     Antes ele decodificava e segurava o som inteiro sem nunca tocar, então o
     7c só tira memória;
   - depois da mudança de destino, o som fica calado no destino novo (a
     declaração dada por aplicada), e o «`tocando` religado» da prova só existe
     no renderer e na bancada. O ganho visível hoje é o destino que saiu soltar
     o som decodificado, e a recusa enganosa sair; o caminho inteiro depende de
     a v6 (F3) deixar o MOD atualizar uma contribuição.
2. *(Rodada 1: decidido pelo controlador, e feito em `108907f8` — o som que
   tocava recomeça no destino novo, os outros ficam calados. O texto abaixo é
   o da entrega.)* **A mudança de destino não religa o som sozinha.** É uma escolha, e o brief
   a deixa aberta («só pede os bytes quando houver `tocando`»). A outra seria
   tratar a remontagem como montagem: o som declarado `tocando: true`
   recomeçaria no destino novo na própria varredura — inclusive um som que já
   tinha terminado, ou que a varredura acabara de pausar —, contra a regra de
   `calarSonsForaDaTela` de que o som que saiu da tela não volta sozinho. Se o
   dono preferir a continuidade, é tirar `som.declarado = som.quer` de
   `tomarOsSonsDevolvidos` e mudar a prova (a reversão da tabela mostra o que
   muda).
3. **O pedido de tocar a caminho é dito como recusado** com a frase de
   `SOM_FORA_DA_TELA` pelo destino que saiu, mesmo com a contribuição na tela
   noutro destino. É a regra que já valia para o pedido que esperava o áudio da
   janela (S-m5); o MOD pode religar, e aí toca no destino novo.
4. *(Rodada 1: consertado em `8f67e167` — ver a seção 10. O texto abaixo é o
   da entrega, e media menos que a revisão: a montagem ficava em toda entrada,
   e não só nos destinos.)* **O som do servidor fica guardado em cada destino.** O adiamento segura o
   `data:` que `midia_em_bytes` devolveu (`origem.uri`), em cada destino da
   contribuição, até ele sair — é o que um destino que tome a marca decodifica
   sem pedir de novo ao servidor. Antes, só o destino que tomava a marca o lia,
   e decodificava na hora; os outros o largavam. É o mesmo que a imagem do
   servidor já segura no `src` de cada destino, e o teto de bytes de cada
   destino (`LIMITES_DO_CARTAO.bytesDeMidia`, 20 MiB) já o conta. O pior caso é
   o de 64 destinos com um som de 10 MiB do servidor: uns 13 MiB de texto por
   destino. A outra saída é pedir de novo ao servidor quando um destino toma a
   marca. O som do pacote guarda só o caminho.
5. **A devolução é só para o nó desconectado**, como o brief diz. Um destino
   que segura os sons, continua no documento e deixa de desenhar
   (`cartoesPintam` falso) fica com a marca e o som pausado, como antes. Pelo
   desenho de hoje, a substituição que perde a disputa sai do documento, e cai
   no caso coberto; a prova do T8 N1 (no documento, sem desenhar) segue verde
   com o comportamento antigo.
6. **A segunda volta de `base.js` não é exercida pela
   `contribuicoes-e-camadas.cjs`**: os renderers de mentira dela não têm
   `sons`, e a volta é pulada. Quem a exercita é a `regiao-do-mod.cjs`, pelo
   recorte de `base.js` — que passa a depender do texto de
   `calarOsSonsQueSairamDaTela` e de `aContribuicaoPinta` (o recorte diz, numa
   falha própria, quando a forma mudar).
7. **O tamanho do arquivo vai no motivo da anotação «pronta».** Nenhuma tela
   mostra o motivo de uma «pronta» hoje (`contarAMidia` só lê o das
   recusadas); ele fica para quem lê `midiasAnotadas`.
8. Os riscos do brief valem como escritos: o primeiro toque de uma
   contribuição espera a decodificação; a recusa pelo teto chega no pedido de
   tocar; a troca de tocador recomeça o som do começo (o ponto em que o outro
   parou vai embora com ele); e o pior caso, muitas contribuições tocando ao
   mesmo tempo, continua sem teto por MOD — isso fica com o dono.

## 10. Rodada 1 de conserto

A revisão pediu conserto por dois Importantes, confirmou as preocupações 1 e
3, e o controlador decidiu a 2: o som que tocava recomeça no destino novo.
Mais seis menores. Cada conserto tem o seu commit, o `check-runtime` antes
dele, e a prova por reversão (o arquivo devolvido da cópia e conferido byte a
byte e por `sha256`).

| commit | item | o que muda |
|---|---|---|
| `8f67e167` | **Importante 1** | o `data:` do som do servidor deixa de ficar retido: a montagem só é guardada numa contribuição; o destino que tomou o som larga `origem.uri` quando a carga tem os bytes; os outros guardam o plano, e o que tomar a marca pede o som de novo ao servidor (`pedidoAoServidor`, um lugar só para os argumentos) |
| `8ff12f12` | **Importante 2** | a prova da mudança de destino mede a soltura: `tocadoresDeSomDeMod` exposto à bancada (`tocadoresDePe`) e o áudio do primeiro destino fechado |
| `108907f8` | **a continuidade** | `calarSonsForaDaTela` guarda quais tocavam antes de pausar; `devolverOsSons` passa os lugares deles adiante (`herdados`); `tomarAMarca` e `herdarOSom`: o que tocava recomeça, os outros ficam com `declarado = quer`; `lugaresDasMidias` diz qual som é qual entre os destinos |
| `90557f49` | menor 6 | a mídia que chega ao destino novo depois de ele tomar a marca segue a mesma regra: a herança fica com o destino até o último som dela chegar |
| `44be1a72` | menor 4 | o som «falhou» ou «cheia» passa recusado ao destino novo (`recusa` na entrada, `recusados` na herança): a figura e a anotação com o motivo, sem carregar e sem dizer a recusa outra vez |
| `71a9f1aa` | menor 2 | o pedido de tocar põe o som em «carregando» até decodificar |
| `f6023860` | menor 1 | `sons.devolvidos` abre a segunda volta de `calarOsSonsQueSairamDaTela`; as duas voltas percorrem `montadas` sem copiá-lo |
| `dcf69ab9` | menores 3 e 5 | o doc de `LIMITES_DO_CARTAO` diz «que já foram mandadas tocar»; os comentários velhos da bancada apontam `carregarSom` e `esperarOPedidoDeTocar` |
| `418ee56b` | menor 4, o resto | o destino que devolveu a marca com um som recusado a toma de volta quando volta à tela sem ninguém que a tenha tomado: a recusa volta à anotação, e a segunda volta para de abrir |

Os mesmos três arquivos. A `regiao-do-mod.cjs` passa a 44 provas (a seção 20
é nova; as outras provas da rodada entraram nas seções 17 e 18).

### 10.1 Importante 1: o `data:` do som do servidor (`8f67e167`)

**A medida da memória**, com uma sonda que monta a bancada de verdade e conta,
nas entradas de `somDaMidia`, as cadeias `data:` retidas — e, num processo
novo por cenário, o `heapUsed` mais a memória externa do node depois de um
`gc()`. Cada resposta do servidor é uma cadeia nova, como no `ipc`. Arquivo de
1 MiB (o `data:` tem 1,3 MiB):

| cenário | antes (`d5d384fa`) | depois |
|---|---|---|
| região, um som do servidor pronto | 1 `data:` retido, 1,3 MiB; +3,8 MiB | 0; +2,5 MiB |
| contribuição em 64 destinos, o som tocando | 64 retidos, 85,3 MiB; +89,9 MiB | 0; +6,1 MiB |
| contribuição em 64 destinos, esperando o pedido de tocar | 64 retidos, 85,3 MiB; +91,3 MiB | 1 (o destino que tomou o som, até o pedido), 1,3 MiB; +7,4 MiB |

Com o arquivo de 10 MiB da revisão, o «antes» das contribuições é dez vezes
isso — uns 853 MiB de texto —, e o «depois», 0 ou 13 MiB.

**RED** (seção 20, `oDataDoSomDoServidorNaoFicaRetido`, contra `d5d384fa`):

```
FALHOU — o data: do som do servidor não fica retido · na região: o som do servidor ficou pronto na região e a entrada dele segura o `data:` inteiro pela vida do nó: {"referencias":1,"caracteres":38}
FALHOU — o data: do som do servidor não fica retido · numa contribuição, depois de decodificar: […] quem tomou o som, depois de decodificar, e quem não o tomou: {"referencias":2,"caracteres":76}
FALHOU — o data: do som do servidor não fica retido · esperando o pedido de tocar: o som do servidor espera o pedido de tocar, e o `data:` não está só no destino que o tomou: {"tomou":{"referencias":1,"caracteres":38},"outro":{"referencias":1,"caracteres":38}}
FALHOU — o data: do som do servidor não fica retido · esperando o pedido de tocar: o pedido de tocar decodificou o som do servidor, e o `data:` ficou: {"referencias":2,"caracteres":76}
FALHOU — o data: do som do servidor não fica retido · na mudança de destino: o destino que tomou a marca não pediu o som de novo ao servidor e tocou, ou algum destino segura o `data:`: 1 fonte(s), 2 pedido(s) ao servidor (dois na montagem, um no pedido de tocar), {"referencias":2,"caracteres":76}
```

**Reversões:**
- sem `delete origem.uri` em `carregarSom`: vermelho em «depois de
  decodificar», «esperando o pedido de tocar» (depois do pedido) e «na mudança
  de destino», uma referência em cada;
- os outros destinos guardando o `data:` (`semOData = montagem`): vermelho nos
  mesmos três, e «o `data:` não está só no destino que o tomou»;
- o destino que toma a marca sem pedir de novo ao servidor: «0 fonte(s), 2
  pedido(s) ao servidor»;
- guardar a montagem em toda entrada, com o `delete` no lugar, **fica verde**:
  o `delete` sozinho já solta o texto, e guardar a montagem só numa
  contribuição é higiene. O vermelho da região no RED é dos dois juntos.

### 10.2 Importante 2: soltar, e não só esquecer (`8ff12f12`)

Só a prova mudou: depois da varredura, `b.tocadoresDePe()` (o tamanho de
`tocadoresDeSomDeMod`, exposto pela bancada) e o estado do áudio da janela do
primeiro destino. **Reversão** — sem `estado.som?.soltar()` em
`devolverOsSons` — vermelha, como a revisão mediu:

```
FALHOU — o som da contribuição muda de destino: o destino que saiu da tela esqueceu o tocador sem soltá-lo: o som decodificado segue de pé na janela e o áudio dela não fechou — 1 tocador(es) de pé, áudio «running»
```

Com a continuidade (abaixo), o esperado passou a ser um tocador de pé — o
novo, no outro destino — e o áudio do primeiro «closed»; sem o `soltar`, são
dois, e o áudio do primeiro segue «running».

### 10.3 A continuidade (`108907f8`)

**A regra.** `calarSonsForaDaTela` guarda `tocador.tocando` de cada som antes
de pausar, quando vai devolver a marca. `devolverOsSons` passa adiante, em
`sons.herdados`, os lugares dos sons que tocavam (e, desde os menores 4 e 6,
os de todos e os dos recusados). O destino que toma a marca (`tomarAMarca`)
fica com a herança, e `herdarOSom` decide por som: o que tocava fica com a
declaração por aplicar — o `tocando: true` é o pedido de tocar, e o som
recomeça do começo; os outros ficam com `declarado = quer`, e um som que
terminou não volta (`devolverOsSons` zera `terminou`, e por isso o
`declarado = quer` não sai de todos).

**Qual som é qual** entre os destinos é a ordem dele entre as mídias, no
documento (`lugaresDasMidias`): a chave do plano não é única entre contêineres,
e todo destino monta a mesma declaração com o mesmo perfil e os mesmos tetos.

**RED** (seção 18, contra `8ff12f12`):

```
FALHOU — o som da contribuição muda de destino: o destino que saiu da tela esqueceu o tocador sem soltá-lo: […] — 0 tocador(es) de pé, áudio «closed»
FALHOU — o som da contribuição muda de destino: o som que tocava quando o lugar dele saiu da tela não recomeçou, do começo, no destino que tomou a marca: 1 decodificação(ões), a fonte nova de undefined, 0 tocador(es) no outro, «tocando» dito 1 vez(es), recusas [], [{"situacao":"pronta",…}]
```

O primeiro diz «0», e não «1», porque sem a continuidade ninguém tocava no
outro destino. **Reversões:** sem guardar o que tocava, o mesmo vermelho; com
todo som tomado religando (`declarado = undefined` para todos), «a vinheta
tinha terminado […] e a troca de destino a tocou de novo: 2 decodificação(ões),
2 áudio(s) da janela, […] «tocando» dito 2 vez(es)».

### 10.4 Os menores

- **6, o som que chega depois da troca** (`90557f49`). No destino que saiu, a
  vinheta tocava e o sino tinha terminado; no outro, a mídia do sino ainda vinha
  quando a marca mudou. RED, contra `108907f8`: «o sino, que tinha terminado,
  voltou a tocar […] «tocando» da vinheta 2 vez(es), do sino 2». Reversão — o
  som que chega montado sem a herança — o mesmo vermelho. A herança acaba
  quando o último som dela chega: um som num lugar que o destino que saiu não
  segurava é montado como qualquer outro.
- **4, o som recusado** (`44be1a72`, `418ee56b`). Para «falhou» (`bytesDoSom`
  que rejeita) e para «cheia» (o teto do cartão), RED «o som «falhou» no
  destino que saiu voltou a «pronta» no destino que tomou a marca» (e o mesmo
  para «cheia»). A recusa passa com a figura e o motivo, sem nova linha ao MOD
  nem ao registro (a prova confere os dois). Reversão — `devolverOsSons`
  tratando o recusado como os outros — os dois vermelhos. E o destino que
  devolveu a marca só com um som recusado, ao voltar sem ninguém que a tenha
  tomado, a toma de volta (`tomarOsSonsDevolvidos` conta todo som que chegou,
  e não só os adiados); reversão: «não tomou a marca de volta —
  {"tomou":false,"devolvidos":true,"anotadas":[]}».
- **2, «carregando» durante a decodificação** (`71a9f1aa`). Nos dois jeitos de
  o pedido chegar — `tocando: true` desde a montagem, e religado depois —, RED
  `{"figura":"pronta","anotada":"pronta"}` com a decodificação presa. Quem diz
  «carregando» é `pedirParaTocar`, um lugar só; reversão: os dois vermelhos.
- **1, a segunda volta** (`f6023860`). Uma contribuição sem som e outra com:
  RED «a varredura perguntou 2 vez(es) a um destino se ele queria a marca, sem
  marca devolvida nenhuma», e «6 pergunta(s) na devolução, 2 na varredura
  seguinte». Reversão — a volta aberta pela marca vaga — os dois vermelhos.
- **3 e 5, docs** (`dcf69ab9`): sem prova; é texto.

### 10.5 GREEN da rodada

Em `418ee56b`: `regiao-do-mod.cjs` com 44 provas; `cargo xtask check-runtime`
verde antes de cada commit; `cargo test -p seele-app --test frontend` com 262;
`cargo test -p xtask` com 161; `cargo xtask check-api` verde; `cargo test -p
seele-conformance --test mod_de_referencia` com 4; `diagnostico-de-mods.cjs`
no Chromium do Playwright, verde; `cargo fmt --all -- --check` limpo. As
WebViews de verdade continuam sem rodar.

### 10.6 Preocupações da rodada

1. **O som do servidor pedido de novo** pelo destino que toma a marca é uma
   resposta nova do servidor do MOD: o papel é conferido de novo (`som`, senão
   `falhou`), mas o tamanho que fica no teto de bytes do destino é o da
   montagem. O teto de arquivo do Rust (`midia_em_bytes`) vale nos dois
   pedidos.
2. **Qual som é qual** é a ordem das mídias na declaração. Hoje todo destino de
   uma contribuição monta o mesmo conteúdo, que não muda (preocupação 1). Se a
   v6 deixar um destino desenhar outro conteúdo, a ordem deixa de bastar.
3. **O pedido de tocar a caminho continua recusado com `SOM_FORA_DA_TELA`**
   pelo destino que saiu, e não recomeça no novo: ele ainda não tocava
   (preocupação 3, que a revisão confirmou).
4. O destino que sai e volta só tem a marca de volta se ninguém a tomou; um som
   que já tinha terminado nele segue calado na volta, pela mesma regra.
