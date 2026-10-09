# Lote K3 da F5: relatório

Base `411778ef` (`0.16/lotes`). Branch `0.16/lotes-k3`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af8a3ef763b612f0f`. São sete
commits de código e texto, e este relatório por último. Tudo é local: sem push,
tag, release ou Actions.

| commit | item | o que muda |
|---|---|---|
| `66133eca` | **P49-9** (fecha) e Docs-m5 do 1D | No automático misto, `resumo` soma `quemPinta(ponto, "")`. Ganha o campo `tambemDesenham`, e `preteridos` passa a juntar `perderam`. `oQueAGestaoDiz` diz «automática: mod/a para todos, mod/b para quem declarou». Os docs de `resumo` e de `quemPinta` dizem os três casos. |
| `4e4bc93d` | «Depois da onda 1» (o bloco do K2) | O caso de `diagnostico-de-mods.cjs` que prende na página o bloco «a região e as páginas de cada MOD» |
| `f62ed74a` | P51-23: os contornos (T7 M2 e T7 M3 do 1D) | O teto conta o que corta, e o aviso diz «N pontos não contornados (teto de 300)». Um nó com área que o recorte zera não ganha caixa. |
| `0db8cbc7` | P51-23: as recusas do pedido (1C) | Cria o ajudante `dizerRecusaDoPedido` e reescreve o guarda `toda_recusa_de_mod_que_a_janela_registra_leva_o_id_em_campo_proprio` no mesmo commit |
| `f7cb4fcf` | P51-23: a anotação do teto sem os números (T8 m1) e os docs de `LIMITES_DO_CARTAO` | A anotação passa a ser o porquê com os números, nos quatro lugares. Os docs dizem que numa contribuição o bolso é de cada destino. |
| `19391f03` | P51-23: a frase do P12 com o ponto sem «» (1C) | Muda `o conteúdo de «ponto» não saiu`, em `mods-contribuicoes.js` |
| `69f194f7` | P51-23: o resto do texto | Trata quatro coisas: as duas definições e o «nenhum script cria» (T7 m1 e m2), o `ci.yml`, o plano 1C, os comentários da bancada e da aba (T6), e o «a tempo» (Docs-m7) |

Arquivos mudados (`411778ef..69f194f7`). Todos estão na lista do lote:
- `apps/seele-app/ui/mods-contribuicoes.js`, `camada-mods.js`, `base.js` e `mods-regiao.js`;
- `apps/seele-app/ui/camada-mods.css`, porque a nota dos contornos ganhou classe nova;
- `apps/seele-app/bancada/contribuicoes-e-camadas.cjs` e `diagnostico-de-mods.cjs`;
- `apps/seele-app/tests/frontend.rs`;
- `.github/workflows/ci.yml`, só o comentário;
- `docs/superpowers/plans/2026-09-29-1.0-plano-1c-mods-no-log.md`.

Ficaram intocados:
- o fio, a FFI, a API congelada (`api/`) e a CSP;
- o `index.html`;
- o que é da F6, da F3 e do J.

Não há dependência nova, e nada foi baixado.

Nenhum `.cjs` novo: os casos novos entraram nas duas bancadas que já existem.
Nenhuma linha acrescentada diz «API 3» (`git diff 411778ef..HEAD | grep "^+" |
grep "API 3"` não acha nada).

Não toquei nas frases da API 3 da gestão (`modsDoCaminhoAntigo`, o ramo
`escolhidoNaoSubstitui` com `cartoesDosMods`, as frases «(API 3)»), nem em
`atenderOMod`, nem no caso da bancada que as fixa.

## 1. P49-9: a gestão diz quem desenha (`66133eca`)

**Produto.**
- `resumo` (`mods-contribuicoes.js`) calcula `pelosAlvos =
  this.quemPinta(ponto, "", estaDePe)`. Só faz isso quando o ponto é
  substituível, a escolha é automática (`""`) e alguém vence o alvo vazio
  (`disputa.escolhida`).
- Com isso:
  - `tambemDesenham` são os de `pelosAlvos.substitui`, fora o escolhido. Fora
    desse caso, a lista é vazia.
  - `preteridos` é o `Set` da disputa do alvo vazio somado a
    `pelosAlvos.perderam`.
  - O caso parcial, as escolhas gravadas e o SEELE escolhido não passam por
    esse ramo.
- `oQueAGestaoDiz` (`camada-mods.js`) mostra
  `automática: ${escolhido} para todos, ${…} para quem declarou` quando há
  `tambemDesenham`. Com mais de um, a frase é `mod/b e mod/c, cada um para quem
  declarou`, como na aba. A nota vem de `preteridos` sem mudança: «Também
  pediram este lugar e não o receberam: mod/e. Escolha abaixo qual deve
  desenhar.». Os botões não mudam.
- Os docs mudaram (Docs-m5):
  - o de `resumo` passa a listar três casos: o automático misto, ninguém no
    alvo vazio com escolha, e o caso parcial;
  - o de `quemPinta` dizia «`resumo` responde pelo alvo vazio», e passa a dizer
    que respondia, e onde pergunta a ele hoje.

**Prova: o caso «M1 · a gestão · o automático misto»**, em
`contribuicoes-e-camadas.cjs`. Ele roda sobre o registro de verdade, com
`oQueAGestaoDiz` recortada de `camada-mods.js` por `abaDoDiagnostico()`.
- O cenário é o comando da pendência 49, item 9:
  - mod/a no cartão de todos;
  - mod/b com prioridade 10 na pessoa 2;
  - mod/e com prioridade -5 na pessoa 3.
- O caso confere o comando (a pessoa 2 é de mod/b e a 3 de mod/a), o estado, a
  nota, os botões e a concordância com `quemPinta`.
- Os controles são três:
  - mod/a sozinho, com um MOD que só acrescenta, continua «apresentado por mod/a
    (escolha automática)»;
  - com mod/a escolhido, `tambemDesenham` é vazio;
  - com o SEELE escolhido, também.
- **Vermelho antes do conserto.** A primeira rodada morreu com
  `TypeError: linha.tambemDesenham is not iterable`, que não é o motivo certo.
  O caso passou a ler `?? []`, e o vermelho saiu pelas frases:
  - «no automático, mod/a desenha o cartão de todos e mod/b o da pessoa 2, e a gestão nomeou um MOD só — quem escreve mod/b lê que ele não desenha nada: «apresentado por mod/a (escolha automática)»»
  - «mod/e pediu o cartão da pessoa 3 e perdeu para mod/a, e a gestão não o disse entre quem pediu e não recebeu: «»»
  - «a gestão e a aba DIAGNÓSTICO disseram coisas diferentes do mesmo cartão: gestão {…"preteridos":[]…}, aba {…"substitui":["mod/a","mod/b"],"perderam":["mod/e"]…}»
- **Verde** depois do conserto. O caso parcial («M1 · a gestão · o caso
  parcial») ficou verde em todas as rodadas, e a bancada de navegador também.
- **Reversões.** Em cada uma, `mods-contribuicoes.js` foi restaurado da cópia
  verde e conferido com `cmp`, que deu idêntico.
  1. `tambemDesenham: []` deixa o caso vermelho com «… a gestão nomeou um MOD só
     …: «apresentado por mod/a (escolha automática)»», e com a concordância.
  2. `preteridos` sem `pelosAlvos?.perderam` deixa o caso vermelho com «mod/e
     pediu o cartão da pessoa 3 e perdeu para mod/a, e a gestão não o disse …:
     «»», e com a concordância.

## 2. O bloco da região e das páginas na página (`4e4bc93d`)

O caso novo é `aRegiaoEAsPaginasDeCadaModAparecemNaAba`, em
`diagnostico-de-mods.cjs`.
- `midia_do_mod` recusa todo caminho com «<caminho> não está no pacote».
- mod/x desenha a região com uma mídia (`desenharARegiaoDoMod`) e monta uma
  página com outra (`superficiesDoMod(...).criar(...)` e `.montar(...)`).
- A aba DIAGNÓSTICO é aberta pelo clique.
- O caso exige `#lista-quem-pinta + ul[data-midia-dos-mods="sim"]` com «a região
  e as páginas de mod/x: 2 recusadas — img/regiao.png não está no pacote;
  img/pagina.png não está no pacote».

O caso cobre um comportamento que já existia, e ficou verde ao entrar.

**Reversão.** Tirei `desenharAMidiaDosMods(lista);` de `desenharQuemPinta`. Li o
«tirar a chamada de `desenharQuemPinta`» do brief como a chamada que
`desenharQuemPinta` faz ao bloco. O caso ficou vermelho com «a aba DIAGNÓSTICO
não desenhou, abaixo dos onze lugares, o bloco da região e das páginas de mod/x
com as duas recusas e os motivos: a mídia recusada da faixa e da página de um
MOD chega ao seele.log e ao MOD, e não à aba — ficou "(sem bloco)"».
`camada-mods.js` foi restaurado, e o `cmp` deu idêntico.

## 3. P51-23: os contornos (`f62ed74a`)

**Produto** (`desenharContornos`, em `camada-mods.js`).
- **O teto.** Saiu o `if (caixas.length >= TETO_DE_CONTORNOS) break;`.
  - O laço olha todos os nós com o mesmo critério de «à vista».
  - Passado o teto, cada nó à vista conta em `deFora` em vez de ganhar caixa.
  - Com `deFora > 0`, o aviso `.contornos-dos-pontos-aviso` ganha dentro dele um
    `<span class="contornos-dos-pontos-nota">`, numa linha própria. O texto é
    `${emQuantidade(deFora, "ponto não contornado", "pontos não contornados")}
    (teto de 300)`.
  - A regra `.contornos-dos-pontos-nota { display: block; pointer-events: none; }`
    entrou em `camada-mods.css`.
  - O seletor entrou na lista de `pointer-events` de
    `o_modo_de_desenvolvedor_nao_toma_o_teclado_nem_o_clique`.
  - A nota não usa `focus`, `blur`, `tabIndex` nem ouvintes, e o mesmo guarda,
    que lê o corpo de `desenharContornos`, continua verde.
- **O recorte vazio.**
  - Depois das comparações estritas, que ficaram como estavam, entram
    `larguraVista` e `alturaVista`.
  - `if (r.width > 0 && r.height > 0 && (larguraVista <= 0 || alturaVista <= 0))
    continue;`: um nó **com área** cuja interseção com o recorte tem largura ou
    altura zero não ganha caixa. É o `r.bottom === cima` do brief, e também o
    recorte sem altura.
  - O nó sem área continua pelas comparações estritas e continua contornado.
  - A largura e a altura da caixa passam a ser as da interseção.
  - O doc da função diz as duas coisas.

**Prova**, em `oModoDeDesenvolvedorContornaSemTomarNada`
(`diagnostico-de-mods.cjs`), depois do caso da página aberta.
- **O recorte.** Uma lista `position: fixed` em (1200, 300), com 120×100 e
  `overflow: hidden`, tem três filhos absolutos e é rolada 20 px:
  - um item de 50×20, que acaba rente ao topo da lista;
  - um vazio de 0×0 em (10, 50), que fica em (1210, 330);
  - um `div` comum, só para dar o que rolar.

  O caso exige que a rolagem seja 20, que nenhum contorno novo comece em
  «1200px 300px», e que os contornos novos sejam exatamente `["1210px 330px
  0px×0px"]`.
- **O teto.** O caso conta os contornos de antes (`antes`, que precisa estar
  entre 0 e 300) e confere que a nota está vazia. Põe 320 retratos de 4×4 no
  alto da janela e exige:
  - 300 caixas;
  - a nota `${antes + 20} pontos não contornados (teto de 300)`;
  - a nota vazia de novo depois que os retratos saem.
- **Vermelhos antes do conserto**, na ordem em que apareceram:
  - Os dois primeiros foram defeitos da bancada, não do produto, e foram
    consertados na bancada:
    - eu lia `scrollTop` depois de tirar a lista;
    - um «resto» de 0×0 não dava o que rolar.
  - O recorte: «um item com área que a rolagem deixou rente ao topo da lista,
    sem nada à vista, ganhou contorno — o traço de 2 px do recorte vazio:
    1200px 300px 50px×0px, 1210px 330px 0px×0px».
  - Com o recorte consertado, o teto: «com 349 lugares à vista, o teto de 300
    deixou 49 sem contorno, e a camada não disse quantos — quem escreve o MOD
    conclui que o lugar dele não existe: «»».
- **Verde** depois dos dois consertos.
- **Reversões.** Em cada uma, `camada-mods.js` ou `.css` foi restaurado da cópia
  verde e conferido com `cmp`, que deu idêntico.
  - A: tirar o `deFora += 1`. O teto fica vermelho com a mesma frase de cima
    («… deixou 49 sem contorno …»).
  - B: tirar a linha do critério da interseção. O recorte fica vermelho com a
    frase de cima («… 1200px 300px 50px×0px …»).
  - C: aplicar o critério a todo nó, sem o `r.width > 0 && r.height > 0`. Fica
    vermelho, mas pela asserção que já existia: «o contorno de «canal.cabecalho»
    não apareceu; apareceram: …». Os contêineres vazios do produto somem antes de
    o caso novo do vazio rodar.
  - C′: tirar só os nós 0×0. Fica vermelho pela mesma asserção antiga, e pelo
    mesmo motivo (ver as preocupações).
  - O guarda de CSS: com o `pointer-events: none` tirado da regra da nota,
    `o_modo_de_desenvolvedor_nao_toma_o_teclado_nem_o_clique` reprova com
    «`.contornos-dos-pontos-nota {` não declara `pointer-events: none`, e o
    contorno pega o clique que era do canal ou do botão embaixo dele».

## 4. P51-23: as recusas do pedido passam por um ajudante (`0db8cbc7`)

**Produto.** `function dizerRecusaDoPedido(id, oQue)` fica logo antes de
`pedirAoServidor`, em `base.js`, e chama `registrarNoAnfitriao("pedido-de-mod",
\`${id}: ${oQue}\`, "aviso", id)`. As quatro recusas passam por ele:
- a da geração;
- a dos oito pedidos;
- a da carga;
- a do prazo.

**O guarda, no mesmo commit.**
`toda_recusa_de_mod_que_a_janela_registra_leva_o_id_em_campo_proprio` passa a:
- exigir que `base.js` tenha o ajudante;
- conferir o ajudante como conferia cada chamada: o id na quarta posição e
  `"aviso"` na terceira;
- exigir que `pedirAoServidor` o chame pelo menos quatro vezes, todas com `id`
  como primeiro argumento;
- exigir que não sobre chamada direta a `registrarNoAnfitriao` em
  `pedirAoServidor`.

A parte de `atenderOMod` não mudou.

**Vermelho antes do conserto:** «`base.js` não tem o ajudante
`dizerRecusaDoPedido`, e as recusas de `pedirAoServidor` escrevem à mão, cada
uma, o formato de toda recusa de MOD».

**O guarda de antes contra o código novo.** Pus o `frontend.rs` de `HEAD` sobre o
`base.js` com o ajudante. Ele reprova com «`async function pedirAoServidor(`
deixou de chamar `registrarNoAnfitriao`, ou o recorte não o achou — e este
guarda passaria por não olhar nada». É o que obriga o mesmo commit. O
`frontend.rs` foi restaurado, e o `cmp` deu idêntico.

**Reversões.** Em cada uma, `base.js` foi restaurado da cópia verde e conferido
com `cmp`, que deu idêntico.

| reversão | frase do vermelho |
|---|---|
| o id na terceira posição do ajudante | «`function dizerRecusaDoPedido(` registra recusa de MOD sem `"aviso"` como terceiro argumento — o nível que o plano fixa para ela — ou sem `id` como quarto: …» |
| `"erro"` no ajudante | a mesma |
| sem a recusa do prazo | «`pedirAoServidor` diz 3 recusa(s) por `dizerRecusaDoPedido`, e não as quatro — a da geração, a dos oito pedidos, a da carga e a do prazo: …» |
| a dos oito pedidos à mão, no lugar do ajudante | a mesma (a contagem reprova primeiro) |
| a dos oito pedidos com `request` em vez de `id` | «`pedirAoServidor` diz recusa por `dizerRecusaDoPedido` sem o `id` do pedido como primeiro argumento, e a linha sai com o `mod_id=` de outro ou de ninguém: …» |
| uma chamada à mão a mais, ao lado das quatro | «`pedirAoServidor` voltou a chamar `registrarNoAnfitriao` direto, com o formato de toda recusa de MOD escrito à mão ao lado do ajudante que o escreve num lugar só: …» |

A bancada `envio-de-imagens.cjs` recorta de `let filaDePedidosDeMod =` até
`/**\n * Põe um MOD de pé`. O ajudante está dentro desse trecho, e o
`check-runtime` ficou verde.

## 5. P51-23: a anotação do teto diz os números, e os docs de `LIMITES_DO_CARTAO` (`f7cb4fcf`)

**Produto** (`mods-regiao.js`).
- A anotação da recusa pelo teto de bytes deixa de ser `passou do teto de
  ${teto} bytes de mídia deste lugar`. Passa a ser o porquê que o MOD e o
  registro já recebiam, com o lugar: `${bytes} bytes somados aos ${já} já
  montados passam do teto de ${teto} bytes de mídia deste lugar`.
- A mudança vale em quatro lugares: `montarFundoDeMidia`, `montarRetrato`,
  `montarMidia` e, como quarto, a recarga do som em `carregarSom` (do K5). O
  código do K5 diz que ela é «a recusa de `montarMidia`». O `porque` que vai ao
  MOD e o texto do registro não mudaram.
- **Os docs de `LIMITES_DO_CARTAO`.** Em `midias` e `bytesDeMidia`, «somados em
  todos os cartões deste MOD» passa a ser «o bolso de cartão de um renderer».
  O bolso é:
  - de todos os cartões nos de `SeeleUI.cartoes`, que moram no renderer da
    região;
  - **de cada destino** numa contribuição, que tem um renderer por destino
    (`montarContribuicao`). Um `pessoa.cartao` sem alvo numa sala de 64 tem 64
    bolsos.
- O som decodificado é dito como a exceção, por contribuição, como o K5 deixou
  em `bytesDeSomDecodificado`. Li o bloco de agora antes de escrever. O texto
  novo não diz «API 3».

**Prova: o caso novo `aRecusaPeloTetoDeBytesDizOsNumeros`**, em
`diagnostico-de-mods.cjs`. Numa região de 10 MiB, ele monta três formas, e cada
uma passa do teto:
- uma `midia` de 11 MiB;
- uma `caixa` com `fundoDeMidia` de 12 MiB;
- um `retrato` de 13 MiB.

O caso exige três recusadas e os três motivos com os números.
- **Vermelho antes do conserto.** Na primeira forma do caso, a primeira asserção
  dizia «não foram as três recusados», e esse não era o motivo: as três frases
  iguais viravam um motivo só. Reordenei o caso: primeiro a contagem, depois os
  motivos. O vermelho ficou «a recusa pelo teto de bytes chegou à aba sem os
  números — quanto a mídia tinha e quanto já estava montado —, e quem abre a aba
  não sabe se a mídia era grande demais ou se o lugar já estava cheio», com
  `['passou do teto de 10485760 bytes de mídia deste lugar']` contra os três
  esperados.
- **Verde** depois do conserto.
- **Reversões.** Em cada uma, `mods-regiao.js` foi restaurado da cópia verde e
  conferido com `cmp`, que deu idêntico. Cada reversão volta só uma das três
  anotações à forma sem números:
  - `montarMidia` deixa a bancada vermelha sem o motivo de 11534336 bytes;
  - `montarFundoDeMidia`, sem o de 12582912;
  - `montarRetrato`, sem o de 13631488.

  As três reprovam com a frase acima.
- O quarto lugar, em `carregarSom`, não tem vermelho próprio (ver as
  preocupações).

## 6. P51-23: a frase do P12 com o ponto sem «» (`19391f03`)

Era a única recusa de contribuição no `seele.log` com o ponto solto:
`${mod}: o conteúdo de ${ponto} não saiu — …`, em `revogar`, no
`mods-contribuicoes.js`. As de `base.js` dizem `«ponto» não montou` e `N nó(s)
de «ponto»`. A frase passa a ser `o conteúdo de «${ponto}» não saiu`.

- **Prova.** O caso «R4d · não saiu» de `contribuicoes-e-camadas.cjs` passa a
  exigir «o conteúdo de «canal.item» não saiu».
- **Vermelho antes:** «R4d · não saiu: a recusa do descarte escreveu o ponto
  sem «», ao contrário das outras recusas de contribuição:
  [["recusa-de-mod","mod/a: o conteúdo de canal.item não saiu — descarte
  quebrado","aviso","mod/a"]]».
- **Verde** depois.
- **Reversão.** Com o `sed` de volta à frase antiga, o caso fica vermelho com a
  mesma frase. O arquivo foi restaurado, e o `cmp` deu idêntico.

## 7. P51-23: o resto do texto (`69f194f7`)

- **As duas definições de «classe que o script aplica»** (T7 m1). Em
  `frontend.rs`:
  - `classes_que_os_scripts_criam` vira `classes_que_os_scripts_aplicam`, e o
    doc dele é a definição: aplicar é pôr a classe num elemento, nas três
    formas;
  - o doc do guarda do mapa de contêineres dizia «"Criam" quer dizer criar» e
    definia por «aplica». Passa a apontar para essa definição, e só para ela;
  - o doc de `CONTEINERES_DOS_PONTOS`, em `camada-mods.js`, diz «o que os
    scripts aplicam a um elemento».
- **«nenhum script cria»** (T7 m2). As duas frases do vermelho passam a ser:
  - «… e nenhum script aplica essa classe a um elemento: …»;
  - «… e nenhuma chamada a `elemento(` cria um `<{etiqueta}>` com essa classe:
    …».

  As duas foram provadas:
  - `.pessoa-rodape` trocado por `.pessoa-rodape-velho` dá «o contêiner de
    «pessoa.acoes» é `.pessoa-rodape-velho`, e nenhum script aplica essa classe
    a um elemento: o contorno do ponto some sem ninguém saber»;
  - `li.pessoa` trocado por `ul.pessoa` dá «o contêiner de «pessoa.cartao» é
    `ul.pessoa`, e nenhuma chamada a `elemento(` cria um `<ul>` com essa classe:
    o seletor não casa com nada, e o contorno do ponto some sem ninguém saber».

  `camada-mods.js` foi restaurado, e o `cmp` deu idêntico.
- **`ci.yml`.** «… da instalação do Chromium e das quatro bancadas de navegador»
  passa a «… e das bancadas de navegador». O `git diff` mostra uma linha de
  comentário, e nada mais.
- **O plano 1C.** `fn os_tres_comandos_de_midia_passam_pela_recusa_dita()` (:1458)
  e o comando `…::os_tres_comandos` (:1684) passam ao nome de hoje,
  `os_quatro_comandos_de_midia_passam_pela_recusa_dita`. O caminho
  `a_midia_recusada_e_dita_no_registro::` confere com o `mod` de `main.rs`.
- **Os comentários da aba (T6).**
  - O doc de `ligarAsAbasDeMods` abria com «Quatro atividades, quatro abas», e as
    abas são cinco. Passa a «Cinco abas: as quatro atividades de U12 e U32, e o
    DIAGNÓSTICO da fase M1».
  - O doc do mapa de contêineres mudou, como dito acima.
- **Os comentários da bancada (T6).**
  - O cabeçalho de `diagnostico-de-mods.cjs` passa a dizer o bloco da região e
    das páginas, «quem desenha o que», o recorte e o teto dos contornos (no
    commit dos contornos).
  - Ele também diz onde está a medida do som no app de verdade
    (`docs/evidencias/som-de-mod-wkwebview/registro.md`), e não mais «a Task 9».
  - Dois comentários de `contribuicoes-e-camadas.cjs` diziam que `resumo`
    «responde» ou «decide» pelo alvo vazio. Passam ao passado, com os dois casos
    em que ele soma os alvos.
- **O «a tempo»** (Docs-m7). O `@param avisar` de `TocadorDeSomDeMod` não dizia o
  que é `naoLigou`, e é ele que decide a frase. O doc diz agora:
  - `naoLigou` é a resposta do navegador ao `resume()` recusado antes do prazo,
    e `undefined` quando o prazo venceu;
  - com `undefined`, a saída «não acordou a tempo»;
  - com resposta, a frase não diz «a tempo» e leva a resposta.

## 8. Os testes

Rodados por último, em `69f194f7`:
- `cargo xtask check-runtime`: as sete bancadas de Node puro passaram, entre
  elas `contribuicoes-e-camadas.cjs` e a `regiao-do-mod.cjs` com «as 44 provas».
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --test frontend`: 268
  passaram, nenhum falhou.
- `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl
  -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/diagnostico-de-mods.cjs`
  rodou as nove provas, as sete de antes e as duas novas, no Chromium, com a CSP
  do produto.
- `cargo fmt --all -- --check` passou.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings` passou.
  O único aviso é o do linker, `ld: duplicate -rpath`, que é do ambiente.

Também rodaram:
- a bancada de navegador e o `check-runtime`, antes de cada commit que mexeu em
  `ui/` ou `bancada/`;
- o `--test frontend`, antes de cada commit.

## 9. O que não rodou

- `cargo test -p xtask`. Os testes de `check_runtime.rs` leem o `ci.yml`, e só
  um comentário dele mudou: os passos dos jobs são os mesmos. Não está na lista
  de testes do brief.
- As outras bancadas de navegador: `avatares-do-mod.cjs`, `conversa-com-estado.cjs`,
  `compartilhar-por-identidade.cjs`, `qualidade-da-voz.cjs`, `ajustes-v013.cjs`,
  `escolha-de-versao-de-mod.cjs` e `gif-na-conversa.cjs`. O brief pede só a de
  diagnóstico. A `avatares-do-mod.cjs` passa por `montarRetrato`, que ganhou a
  anotação com números, mas só a recusa pelo teto mudou, e ela é provada aqui.
- `cargo test -p seele-app --bin seele-app`: o `main.rs` não mudou.
- `seele-conformance` e `--workspace`, que são proibidos nesta onda.

## 10. Para a v6

Nada. `tambemDesenham` é campo interno da linha de `resumo`, não da API. Nenhum
evento, campo de retrato ou frase do `api/README.md` mudou.

## 11. Preocupações

1. **Os textos das revisões T6 m1 a m4, Docs-m5 e Docs-m7 não estão no
   repositório.** Procurei em `docs/`, no `git log` e nas pastas `.superpowers`.
   Identifiquei os alvos pelo código, e o que mudou para cada um está nas seções
   1 e 7. Vale conferir se é o que a revisão apontou:
   - Docs-m5 são os docs de `resumo` e de `quemPinta`;
   - Docs-m7 é o doc de `avisar`;
   - T6 são as cinco abas, o mapa de contêineres, o cabeçalho da bancada e os
     dois comentários de `resumo`.

   Os docs do «a tempo» em `SILENCIO_ANTES_DE_SUSPENDER_MS` e no `recusou` já
   diziam o que o código faz desde `a9b392ce`. Não achei outra divergência.
2. **O guarda de CSS tem leitor próprio.** É
   `toda_classe_que_o_script_aplica_tem_regra_de_css`, e ele lê o segundo
   argumento de `elemento(` e `.className = "…"`, sem `classList.add`. Unifiquei
   as duas definições que o brief nomeia, e não esta.
   - Pelo `grep`, todas as classes literais de `classList.add` têm regra hoje,
     então pôr esse guarda sobre `classes_que_os_scripts_aplicam` provavelmente
     ficaria verde.
   - Não fiz, porque mexe num guarda de defeito de campo fora do que o brief
     pede.
3. **O quarto lugar da anotação do teto não tem vermelho próprio.** É a recarga
   do som em `carregarSom`, do K5. Só a `regiao-do-mod.cjs` passa por ele, e ela
   não é deste lote. Ela confere `figura === "cheia"` e `/teto/` no `porque`, e
   continuou verde.
4. **O vazio que continua contornado.** As reversões C e C′ ficam vermelhas pela
   asserção antiga («o contorno de «canal.cabecalho» não apareceu»), e não pela
   nova: os contêineres vazios do produto somem antes. A asserção nova está lá
   (`["1210px 330px 0px×0px"]`), mas o primeiro a reprovar é o guarda que já
   existia.
5. **A nota no singular.** Para um nó só, a nota diz «1 ponto não contornado
   (teto de 300)», por `emQuantidade`. A frase fixa do brief é a do plural, e é
   ela que a bancada prende.
6. **O custo do modo passado o teto.** Os nós além do teto passam pelo mesmo
   critério de «à vista», com a subida pelos ancestrais.
   - Antes, o laço parava no teto.
   - Os nós que o recorte esconde já pagavam esse custo antes.
   - Agora, os que passam do teto também pagam, só com o modo ligado.
7. **O automático misto muda a nota de outros cenários.** Um MOD que pediu só
   outro alvo e perdeu em todos passa a aparecer em «Também pediram este lugar e
   não o receberam». É o caso de mod/e na Lia, em
   `quemDesenhaOQueDizAEscolhaDestaMaquina`, antes de ele ser escolhido. É o que
   o brief pede, e nenhuma bancada prende essa nota no navegador.
8. **O plano 1C ainda diz a frase de antes.** O comando de :1684 passou ao nome
   de hoje, mas ainda espera «`async fn ler_imagem_mod(` não passa por
   `recusa_de_midia_dita(`». Hoje `ler_imagem_mod` passa por `imagem_lida_dita`.
   O brief pedia só o nome, e o resto é história do plano.
9. **Uma mensagem foi emendada.** A de `69f194f7` foi emendada (`--amend`, só a
   mensagem) antes do commit deste relatório, para não dizer «uma definição só»
   de mais do que as duas que mudaram. Tudo é local.
