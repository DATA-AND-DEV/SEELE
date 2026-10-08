# Lote K2 da F5: relatório

Base `fbeff8e` (`0.16/lotes`). Branch `0.16/lotes-k2`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a94d4f36c07c8f8ea`. Três
commits de código e este relatório; depois, quatro commits da rodada 1 de
conserto (seção 7) e este relatório de novo. Todos locais: sem push, tag,
release ou Actions.

| commit | item | o que muda |
|---|---|---|
| `75fb60f` | **P51-07** (fecha) e a metade de P49-5 que mora no fundo de tela | `provarImagem` e o `decode()` nos quatro caminhos, com os bytes reservados antes e devolvidos na recusa; o `<img>` do retrato ganha ouvinte de `error`; o fundo de tela anota «carregando», «pronta» e «recusada» e esquece a anotação quando o fundo ou a tela saem |
| `82fd154` | **P49-5** (fecha) | `contarAMidia` e `midiasDaRegiaoEDasPaginas` em `base.js`; o bloco «A região e as páginas de cada MOD» na aba; o doc de `anotarMidia` |
| `677f973` | **K1-m2** (fecha) | `pedidoDeArquivo` corta a finalidade por ponto de código |
| `73866cc` | rodada 1, item 1 | as provas do guarda de `podeFalar()` depois do `decode()` |
| `86d0a2d` | rodada 1, item 2 | `.then(aplicar, recusar)` nos quatro caminhos, e `dizerTropecoDaMidia` |
| `af7d6ef` | rodada 1, item 3 | o comentário da bancada sem «API 3» |
| `6ca4fdf` | rodada 1, ajuste do controlador | o tropeço de aplicar chega ao `seele.log`, com o id do MOD |

Arquivos mudados (`fbeff8e..6ca4fdf`), só os cinco da lista do lote:
- `apps/seele-app/ui/mods-regiao.js`
- `apps/seele-app/ui/base.js`
- `apps/seele-app/ui/camada-mods.js`
- `apps/seele-app/bancada/regiao-do-mod.cjs`
- `apps/seele-app/bancada/contribuicoes-e-camadas.cjs`

Nada no Rust, no fio, na API congelada (`api/v1..v5.json` e `api/README.md`
intocados), na CSP, no `index.html` nem nas folhas. Nenhuma dependência nova, e
nada baixado.

## 1. P51-07: a imagem só vale depois do `decode()` (`75fb60f`)

**Produto.**
- `provarImagem(uri)`, em `mods-regiao.js`, logo depois de `motivoDaFalha`: um
  `new Image()` com o `src`, e o `decode()`. A recusa vira
  `Error("a imagem não decodificou")`. A promessa resolve com a própria imagem
  decodificada. É um acréscimo à forma do brief: o fundo de tela pinta com essa
  imagem, e assim o `onload` vira o `decode()` do próprio `img`, como o brief
  pede, sem uma segunda imagem.
- Os quatro caminhos encadeiam `return provarImagem(midia.uri).then(…)` dentro
  do `then` que já existia. A recusa cai no `catch` de cada um:

  | caminho | o que o `catch` faz agora com a imagem que não decodifica |
  |---|---|
  | retrato (`montarRetrato`) | anota «recusada», diz ao registro, volta à inicial, devolve os bytes. **Sem evento ao MOD.** |
  | fundo de mídia (`montarFundoDeMidia`) | anota, diz ao registro, devolve os bytes, manda `midia`/`falhou` com o motivo |
  | fundo de tela (`buscarFundoDaTela`) | anota, diz ao registro, manda `tela`/`fundo-falhou` com o motivo (não tem bolso) |
  | avatar contribuído (`avatarContribuido`, `base.js`) | `situacao: "recusada"`, `estado.bytes = 0`, registro e `midia`/`falhou` com a chave `"avatar"` |

- **A reserva vem antes do `decode()`.** `this[bolso.conta] += bytes` no fundo,
  `this[conta] += midia.bytes` no retrato e `estado.bytes = midia.bytes` no
  avatar continuam onde estavam, antes da espera, e voltam na recusa: `devolver()`
  no fundo, `tirarAImagem()` no retrato (a mesma função que o descarte usa) e
  `estado.bytes = 0` no avatar.
- Depois da espera do `decode()`, cada caminho pergunta de novo `cancelado`,
  `this.solta` e `podeFalar()`. Quando só `podeFalar()` é falso, a reserva
  também volta. Isso não estava no brief: sem essa devolução, a sessão que acabou
  no meio do `decode()` deixaria bytes contados para uma imagem que nunca vai à
  tela.
- **O retrato ganha o ouvinte de `error`**, no molde de `naoAbriu` de
  `montarMidia`. O ouvinte chama a mesma `recusar(...)` do `catch`: tira o
  `<img>`, volta à inicial, devolve os bytes, anota e diz ao registro. O ouvinte
  sai por nome no descarte.

**Bancadas.**
- `regiao-do-mod.cjs`:
  - `imagemDeMentira()` entra no `bancada()` como o `Image` padrão. O `decode()`
    dele resolve por padrão, recusa `URI_QUE_NAO_DECODIFICA` (um PNG de cabeçalho
    válido e corpo corrompido), e segura em `Imagem.presas` todo `uri` que começa
    com `URI_QUE_DEMORA`. `Imagem.decodificadas` anota o que passou pelo
    `decode()`.
  - O pincel de mentira ganha `drawImage`.
  - Provas novas:
    - `umaImagemQueNaoDecodificaERecusadaEDita`, com retrato, fundo de mídia e
      fundo de tela. Cada caso confere que o `decode()` foi chamado, a anotação
      «recusada» com o motivo, a linha do registro, o evento ao MOD (ou a falta
      dele, no retrato), os bytes em zero e que nada foi pintado. Mais o `<img>`
      do retrato que dispara `error`;
    - `osBytesSaoReservadosAntesDoDecode`, no retrato e no fundo. Duas imagens de
      `teto/2 + 1` bytes: a segunda tem de ser recusada pelo teto enquanto a
      primeira decodifica. Também cobre a saída no meio do `decode()` e o
      `decode()` que recusa e devolve.
- `contribuicoes-e-camadas.cjs`, no R4e:
  - `provarImagem` e `LIMITES_DO_CARTAO` entram por recorte de `mods-regiao.js`.
    Sem o teto do cartão no contexto, uma imagem de verdade lançaria
    `ReferenceError` no lugar da recusa;
  - entra um `Image` de mentira;
  - o caso 14 traz o corpo corrompido;
  - os três casos conferem também o `midia`/`falhou` ao MOD e os bytes devolvidos;
  - um caso novo confere o avatar que abre: «pronta», os bytes contados, um
    redesenho e o `uri` devolvido.

**RED** (as bancadas novas contra o produto de `fbeff8e`):
- `regiao-do-mod.cjs`, 26 falhas, todas pelo motivo certo: a imagem quebrada
  ficava «pronta» e calada. Trechos:
  - `o retrato que não decodifica: a imagem não passou pelo \`decode()\`: o caminho a deu por pronta sem saber se ela abre`
  - `o fundo de mídia que não decodifica: … a aba DIAGNÓSTICO a conta de pé: [{"situacao":"pronta",…}]`
  - `o fundo de mídia que não decodifica: o MOD não recebeu «midia»/«falhou» com o motivo, e acha que a imagem está na tela: []`
  - `o fundo de tela que não decodifica: o MOD não recebeu «tela»/«fundo-falhou» com o motivo, …: []`
  - `o retrato cujo <img> não abriu: o <img> quebrado continuou cobrindo a inicial, …: img=true, estado=imagem, 40 bytes`
  - `a reserva · fundo · a recusa devolve: a imagem que o \`decode()\` recusou continua ocupando 40 bytes do teto: [{"situacao":"pronta",…}]`
- `contribuicoes-e-camadas.cjs`, 5 falhas:
  - `R4e · o recorte da imagem: \`provarImagem\` não está em \`mods-regiao.js\`, …`
  - `R4e · a gestão: o retrato recusado (não decodificou) não disse em que pé ficou, …: {"uri":"data:image/png;base64,…QUEBRADO","bytes":2,…,"situacao":"pronta","motivo":""}`
  - `R4e · avatar: o MOD não recebeu «midia»/«falhou» do avatar que não montou (não decodificou): []`
  - `R4e · avatar: o avatar recusado (não decodificou) continua ocupando o teto do cartão, …`

  Na primeira rodada o caso 14 ficou vermelho por outro motivo:
  `redesenharAvatares is not defined`. A bancada ganhou um `redesenharAvatares`
  vazio no laço, e a rodada seguinte, a registrada acima, ficou vermelha pelo
  motivo certo: a imagem quebrada dada por «pronta».

**GREEN:** `regiao-do-mod` (38 provas), `contribuicoes-e-camadas`, as sete do
`cargo xtask check-runtime` e `frontend.rs` (259 passed).

**Reversões.** O script, que fica no scratchpad, troca o trecho, roda a bancada,
restaura o arquivo e compara o sha256. Todas voltaram byte a byte.

| reversão | vermelho |
|---|---|
| R1 fundo de mídia sem `provarImagem` (`Promise.resolve()`) | `o fundo de mídia que não decodifica: a imagem não passou pelo \`decode()\`…` (+ a reserva do fundo) |
| R2 retrato sem `provarImagem` | `o retrato que não decodifica: a imagem não passou pelo \`decode()\`…` (+ a reserva do retrato) |
| R3 fundo de tela sem `decode()` (um `Image` sem prova) | `o fundo de tela que não decodifica: … o MOD não recebeu «tela»/«fundo-falhou»…` e `o fundo pedido não disse que está carregando: «pronta»` |
| R4 avatar sem `provarImagem` | `R4e · a gestão: … "situacao":"pronta"` e `R4e · avatar: o MOD não recebeu «midia»/«falhou»…` |
| R5a fundo sem `devolver()` no `catch` | `a reserva · fundo · a recusa devolve: … continua ocupando 40 bytes do teto` |
| R5b retrato: `recusar` sem `tirarAImagem()` | `o retrato que não decodifica: a imagem recusada continua ocupando 40 bytes…` e `o retrato cujo <img> não abriu: … 40 bytes` |
| R5c avatar sem `estado.bytes = 0` no `catch` | `R4e · avatar: o avatar recusado (não decodificou) continua ocupando o teto do cartão…: {"uri":null,"bytes":2,…}` |
| R6a fundo: a soma só depois do `decode()` | `a reserva · fundo: duas imagens que juntas passam do teto foram as duas decodificar … (2 decodificando, 0 bytes contados)` |
| R6b retrato: a soma só depois do `decode()` | `a reserva · retrato: … (2 decodificando, 0 bytes contados)` |
| R7 retrato sem o ouvinte de `error` | `o retrato cujo <img> não abriu: o <img> do retrato que não abriu continuou anotado «pronta»` |

**A premissa do brief sobre o `Image` padrão, medida.** Tirei o `Image` do
contexto do `bancada()` com o produto já consertado. As provas que já existiam
e ficaram vermelhas foram três:
- `fundoTrocaSoltaECancela`, que lançou um `TypeError`;
- «retrato ignora atrasada», de `aImagemAcompanhaAMudancaDaFonte`;
- «fundo e retrato anotados», de `oFundoEORetratoTambemDizemOEstado`, com
  `Image is not defined`.

Três das seis que o brief cita **não** ficam vermelhas:
- `oRetratoDoCartaoEOMesmoNoEntreDoisRetratos`;
- `osTetosDoCartaoSaoDoCartaoENaoDaRegiao`;
- `aMidiaDoServidorTemOMesmoDono`.

Elas usam a forma `midia` (ou só texto), e esse caminho continua com o `<img>`
e o `error` de `b167b9f`, sem `new Image()`. Nada a decidir: o `Image` padrão
entrou como o brief pede.

## 2. P49-5: a aba conta a mídia da região e das páginas (`75fb60f` + `82fd154`)

**O fundo de tela** (em `75fb60f`, porque é o mesmo caminho de recusa):
- «carregando» ao pedir;
- «pronta» depois do `decode()`, antes de repintar;
- «recusada» nas três saídas: o papel errado, o `catch` da carga e a recusa do
  `decode()`;
- `esquecerMidia` quando a declaração tira o fundo e no `guardar` da tela. Este
  segundo lugar o plano antigo não previa.

Dois acréscimos meus, com prova:
- **Um fundo declarado sem origem** (`fundo: {}`) também esquece a anotação.
  Antes ele saía calado e deixaria valendo o «pronta» do fundo anterior.
- **A recusa de um pedido que o MOD já trocou** não pisa a anotação do pedido de
  agora (`if (fundo.pedido === chave)`). O evento `tela`/`fundo-falhou` e a
  linha do registro continuam saindo como antes.

**A aba** (`82fd154`):
- `contarAMidia(soma, anotada, mod)`, em `base.js`, é a regra que
  `midiasDoPonto` tinha dentro dela: as três situações, até três motivos sem
  repetir, cada motivo cortado em 200 pontos de código. `midiasDoPonto` passa a
  chamá-la, e é o mesmo comportamento: o MOD continua na frente do motivo.
- `midiasDaRegiaoEDasPaginas(regioes, superficies)` fica entre `midiasDoPonto` e
  o doc «Cala o som de MOD…», dentro do recorte que a bancada já fazia. A função
  é pura e devolve um `Map` por id de MOD com `{carregando, pronta, recusada,
  motivos}`. Ela soma `regiao.midiasAnotadas()` de cada região e
  `s.renderer.midiasAnotadas()` de cada `conjunto.porChave.values()`, **sem
  filtro de cartão**.
- `camada-mods.js`:
  - `partesDaMidia` e `porqueDaMidia` saem de `fraseDaMidia`, que passa a usá-las;
  - `frasesDaMidiaDosMods()` diz «a região e as páginas de mod/x: 3 recusadas —
    …», «… de mod/y: nenhuma mídia montada», ou «nenhum MOD tem região nem
    página de pé agora». As três ficam dentro do recorte `APRESENTACAO_NATIVA`,
    que a bancada já roda;
  - `desenharQuemPinta` chama `desenharAMidiaDosMods(lista)`.
- **O bloco é uma `ul.server-dispositivos` irmã de `#lista-quem-pinta`**, e não
  mais uma linha dela. Ela é criada na primeira vez, marcada com
  `data-midia-dos-mods="sim"` e reaproveitada depois. Dentro vai uma
  `li.server-dispositivo.mods-linha-gestao`, com `mods-linha-texto`, `mods-id` e
  `mods-versao`. A razão é medida: `diagnostico-de-mods.cjs` confere
  `#lista-quem-pinta > li` igual a 11, e esse arquivo não é do lote. O
  `index.html` e as folhas não mudam, e o guarda `$("…")` do `frontend.rs`
  continua verde, porque nenhum id novo é procurado.
- O doc de `anotarMidia` passa a dizer quem lê as anotações: as duas somas. Não
  diz mais «ninguém as lê» e não cita os cartões «da API 3».
- `contribuicoes-e-camadas.cjs`: `abaDoDiagnostico()` ganha `regioesDosMods` e
  `superficiesDosMods`. O bloco novo «M1 · a aba · a região e as páginas» monta:
  - uma região de mod/x com um som recusado e um retrato de cartão recusado
    (`cartao: true`);
  - uma página de mod/x com um fundo recusado;
  - uma região de mod/y sem mídia.

  A função devolve `recusada: 3` com os três motivos, em ordem, e as frases
  saem como acima.

**RED:**
- Fundo de tela, com o `decode()` já no lugar e sem as anotações, 6 falhas.
  Entre elas:
  - `recusa anotada · fundo de tela que não é imagem: … a aba não o conta: []`;
  - `o fundo de tela anotado: o fundo pedido não disse que está carregando: «»`;
  - `… a recusa de um fundo que o MOD já trocou pisou a anotação do fundo de agora: «»`.
- Aba: `M1 · a aba · a região e as páginas: \`midiasDaRegiaoEDasPaginas\` (\`base.js\`) ou \`frasesDaMidiaDosMods\` (\`camada-mods.js\`) não estão nos recortes da aba: ela continua sem ler a mídia da região e das páginas`.

**GREEN:** as duas bancadas, as sete do `check-runtime`, `frontend.rs` (259
passed) e a bancada de navegador `diagnostico-de-mods.cjs`, que ainda conta 11
lugares e não registra erro de página.

**Reversões** (todas restauradas byte a byte):

| reversão | vermelho |
|---|---|
| R8a sem anotar a recusa do `catch` do fundo de tela | `recusa anotada · fundo de tela que o Rust recusou: … [{"situacao":"carregando",…}]` e `o fundo de tela que não decodifica: …` |
| R8b sem anotar o papel errado | `recusa anotada · fundo de tela que não é imagem: …` |
| R8c sem anotar «pronta» | `o fundo de tela anotado: o fundo decodificado não virou «pronta»…: «carregando»` |
| R9a sem `esquecerMidia` quando a declaração tira o fundo | `… a declaração tirou o fundo e a aba continua contando o mapa` |
| R9b sem `esquecerMidia` no `guardar` da tela | `… a tela saiu e o fundo dela continua anotado` |
| R9c sem `esquecerMidia` no fundo sem origem | `… um fundo sem origem herdou a anotação do fundo anterior` |
| R10 sem o `if (fundo.pedido === chave)` | `… a recusa de um fundo que o MOD já trocou pisou a anotação do fundo de agora: «recusada»` |
| R11 sem a soma da superfície | `… {"recusada":2,"motivos":["o som não decodificou","o retrato passou do teto do cartão"]}`: dá 2, como o brief previa |
| R12 com o filtro de cartão de volta | `… {"recusada":2,"motivos":["o som não decodificou","o fundo não é imagem"]}` |
| R13 `midiasDoPonto` sem o MOD na frente do motivo | `M1 · a aba · o retrato: o motivo do retrato não saiu cortado em 200 pontos de código inteiros: "mm😀"` |
| R14 `contarAMidia` cortando por índice | o mesmo caso: `"mmm\ud83d"` |

R13 e R14 provam que a extração de `contarAMidia` está presa pelos casos que já
existiam.

**Olhado no Chromium** (script descartável no scratchpad, que não foi
commitado). Usei a página real com a ponte simulada, como faz
`diagnostico-de-mods.cjs`. O MOD declara:
- um retrato e um fundo de tela com um PNG de cabeçalho válido e corpo trocado
  por lixo;
- um fundo de mídia com um PNG bom.

Resultado:
- o `decode()` **de verdade** do Chromium recusou as duas imagens quebradas;
- o retrato ficou na inicial, sem `<img>`;
- a aba mostrou «A região e as páginas de cada MOD / a região e as páginas de
  mod/x: 1 pronta · 2 recusadas — a imagem não decodificou»;
- os 11 lugares continuaram 11, e a página não registrou erro.

## 3. K1-m2: a finalidade cortada por ponto de código (`677f973`)

- `pedidoDeArquivo`: `[...no.finalidade].slice(0, 200).join("")`, no modelo de
  `ef707c7`, com o porquê no doc.
- Os outros cortes por índice da família (`atualizarArquivo`, `textoDoRotulo`,
  a descrição em 120) só vão a `aria-label`/`title` e ficam como estão.
- Na bancada, o `dono()` de mentira ganha `escolherArquivo`, que anota o pedido e
  responde `null`. A prova nova é `aFinalidadeDoArquivoNaoPartePar`, no molde de
  `aChaveDoCampoNaoPartePar`: a finalidade `"a".repeat(199) + "😀"` chega
  inteira e bem formada ao clique.

**RED:** `a finalidade do arquivo: a finalidade de 200 pontos de código, terminada num par substituto, foi cortada pelo meio do par: o \`serde_json\` recusa o pedido inteiro, e o seletor não abre: "aa\ud83d"`.

**GREEN:** `regiao-do-mod` (39 provas), `check-runtime`, `frontend.rs` (259),
`cargo fmt --check`.

**Reversão R15** (de volta ao `slice` por índice): o mesmo vermelho, `"aa\ud83d"`.
Restaurado byte a byte.

## 4. O que rodou, e o que não rodou

Rodou, antes de cada commit:
- `cargo xtask check-runtime`, as sete bancadas;
- `cargo test -p seele-app --test frontend`, 259 passed;
- `cargo fmt --all -- --check`.

No P49-5 rodou também a bancada de navegador `diagnostico-de-mods.cjs`, com o
Playwright de `SEELE-MOD-PERFIS`, sem baixar nada. O clippy não se aplica:
nenhum `.rs` mudou.

**Não rodou: as WebViews de verdade.** O `decode()` de bytes corrompidos no
WKWebView, no WebView2 e no WebKitGTK não se mede aqui. O que se mediu:
- o `Image` de mentira no Node, nas bancadas;
- o Chromium do Playwright, no script descartável. É o motor do WebView2, mas
  não é o WebView2 empacotado.

O que falta medir no app de verdade:
- que o `decode()` do WKWebView recusa o PNG de corpo corrompido, em vez de
  resolver e pintar vazio;
- que um retrato, um fundo e um mapa bons continuam aparecendo, agora um pouco
  mais tarde.

## 5. Para a v6

Nenhuma promessa pública nova entrou:
- nenhum evento novo;
- nenhum campo novo no retrato;
- nenhuma frase nova no `api/README.md`.

Os eventos que passam a sair numa circunstância nova (`midia`/`falhou` no fundo
e no avatar, `tela`/`fundo-falhou` no fundo de tela) já existiam com esse
`estado`. O que muda é um `porque` novo, «a imagem não decodificou».

Para o rascunho da v6:
- **O retrato não fala com o MOD.** Uma imagem de retrato recusada (papel
  errado, teto, carga que falhou, `decode()` ou `error` do `<img>`) é anotada e
  vai ao registro, mas o MOD não fica sabendo. Dizer `midia`/`falhou` ou
  `recusada` no retrato seria um evento novo.
- **O papel errado e o teto no fundo de mídia, e o papel errado no fundo de
  tela, também não falam com o MOD.** Esse silêncio é anterior a este lote. Hoje
  só o `catch` desses dois caminhos fala com o MOD.
- **O evento `fundo-falhou` não diz qual fonte falhou** (rodada 1 de conserto).
  Ele leva `chave`, `estado` e `porque`, e nada da declaração do fundo. Um MOD
  que troca de fundo rápido recebe o `fundo-falhou` do pedido anterior enquanto
  o fundo atual já está «pronta» — a anotação da aba ignora a recusa velha
  (`if (fundo.pedido === chave)`), mas o MOD não tem como fazer o mesmo. Dizer
  a fonte no evento, ou calar a recusa de um pedido trocado, é mudança no
  contrato do evento.

## 6. Preocupações e escolhas minhas

- **O bloco é uma lista irmã, e não uma linha de `#lista-quem-pinta`.** Medi a
  restrição: a bancada de navegador conta 11 linhas, e ela não é do lote. Se a
  revisão preferir o bloco dentro da lista, a conta dessa bancada precisa mudar
  junto.
- **A devolução da reserva quando só `podeFalar()` falha depois do `decode()`**
  (fundo, retrato e avatar) não estava no brief. Ela evita bytes contados para
  uma imagem que não vai à tela numa sessão que acabou. Na primeira entrega
  nenhuma prova a media isoladamente; a rodada 1 de conserto acrescentou a
  prova (seção 7, item 1).
- **O fundo de tela sem origem esquece a anotação, em vez de anotar «recusada»**,
  como `montarMidia` faz com a mídia sem `fonte` nem `doServidor`. Escolhi não
  criar um motivo novo que o brief não pedia. Se a revisão quiser os dois
  iguais, a mudança é uma linha, e R9c passa a ser o caso da recusa.
- **A recusa de um fundo de tela que o MOD já trocou ainda manda
  `tela`/`fundo-falhou`.** Esse comportamento é anterior ao lote, e hoje vale
  também para o `decode()`. Só a anotação passou a ignorá-la.
- **Uma exceção dentro de `pintarTela`** cairia no `catch` do fundo e sairia
  como `fundo-falhou`, com o motivo errado. A revisão achou o mesmo defeito no
  avatar, e a rodada 1 de conserto o fechou nos quatro caminhos (seção 7, item
  2).
- **O retrato, o fundo de mídia e o avatar decodificam numa imagem à parte** e
  depois entregam o mesmo `uri` ao `<img>` ou ao CSS. O navegador costuma
  reaproveitar o que já decodificou, mas o custo não foi medido. A imagem passa
  a ficar «pronta» um pouco depois, como o brief previa.
- **WebP num WKWebView antigo** (antes do macOS 11) não decodifica. Ele passava a
  ser um quadrado vazio calado, e agora é «recusada» dita. Isso é o comportamento
  certo, mas pode aparecer como recusa nova em máquina velha.
- **A mídia de um cartão aparece em dois blocos** da aba enquanto a capacidade
  `cartoes` existir, como decidido.
- **Os motivos do bloco não dizem a chave do nó.** Duas imagens quebradas do
  mesmo MOD dão «2 recusadas — a imagem não decodificou», uma vez só. É a mesma
  forma de `midiasDoPonto`, e o registro continua dizendo a chave de cada uma.

## 7. Rodada 1 de conserto

A revisão aprovou o lote, sem crítico nem importante, e pediu três menores
antes do merge. O quarto menor, a prova versionada do desenho do bloco
(`desenharAMidiaDosMods`), fica para o lote K3: o lugar dela é o
`diagnostico-de-mods.cjs`, que não é deste lote.

O script de reversão é o mesmo das seções 1 a 3: troca o trecho, roda a
bancada, restaura e compara o sha256. Todas as reversões desta seção voltaram
byte a byte.

### Item 1: o guarda de `podeFalar()` depois do `decode()` tem prova (`73866cc`)

**O que mudou.** Só as bancadas. O guarda já estava certo no código, mas nenhuma
reversão o derrubava. Cada caso segura a decodificação (`URI_QUE_DEMORA`), troca
o `podeFalar` do dono para `() => false` e só então solta o `decode()`:
- `osBytesSaoReservadosAntesDoDecode`, para o retrato e o fundo de mídia: nada
  montado (nem `<img>`, nem `background-image`) e bytes em zero. Depois do
  `soltar()`, os bytes continuam em zero: o descarte não devolve de novo a
  reserva que já tinha voltado;
- `oFundoDeTelaDizOEstado`, para o fundo de tela, que não tem bolso: o mapa
  não vira `fundo.imagem` e o pincel não o pinta. O pedido falava em três
  caminhos, «nos que couber». Este quarto coube, e o guarda dele era igualmente
  sem prova;
- `contribuicoes-e-camadas.cjs`, no R4e, para o avatar: o `Image` de mentira
  ganha a decodificação presa. O avatar fica reservado enquanto decodifica
  (`bytes: 2`) e, com `podeFalar()` falso, termina em `bytes: 0`, `uri: null`,
  fora de «pronta» e sem redesenho.

**Prova.** Os casos são verdes de saída, porque o guarda já existia. O dente
deles está nas reversões. Comando:
`python3 -I reverter.py <arquivo> <bancada> <nome> <trecho> <troca>`.

| reversão | vermelho |
|---|---|
| C1a fundo sem `if (!this.dono.podeFalar()) { devolver(); return; }` | `a reserva · fundo · a sessão acaba no meio: a imagem decodificada depois de a sessão acabar montou, ou ficou contada: 40 bytes, img=false, fundo="linear-gradient(…), url(\"…DEMORA-sessao\")"` |
| C1b fundo com o guarda e sem `devolver()` | o mesmo caso: `40 bytes, img=false, fundo=""` |
| C1c retrato sem o guarda | `a reserva · retrato · a sessão acaba no meio: … 40 bytes, img=true` |
| C1f retrato com o guarda e sem `tirarAImagem()` | o mesmo caso: `40 bytes, img=false` |
| C1d fundo de tela sem o `!this.dono.podeFalar()` depois do `decode()` | `o fundo de tela anotado · a sessão acaba no meio: o mapa decodificado depois de a sessão acabar virou o fundo da tela: [["imagem","…DEMORA-sessao"]]` |
| C1e avatar sem o guarda | `R4e · a sessão acaba no meio: … {"uri":"…DEMORA-sessao","bytes":2,…,"situacao":"pronta"}, 1 redesenho(s)` |
| C1g avatar com o guarda e sem `estado.bytes = 0` | o mesmo caso: `{"uri":null,"bytes":2,…,"situacao":"carregando"}, 0 redesenho(s)` |

Depois do item 2, o guarda mudou de lugar: foi para dentro de `aplicar`. Rodei
de novo C1a, C1c, C1d e C1e contra o código final. Todas ficaram vermelhas com
as mesmas frases e voltaram byte a byte.

### Item 2: aplicar fica fora do caminho de recusa (`86d0a2d`)

**O que mudou.** Os quatro caminhos agora têm a forma
`busca → provarImagem → .then(aplicar, recusar) → .catch(tropeço)`.
- Só a busca e o `decode()` caem em `recusar`.
- O que lança dentro de `aplicar` — ou dentro do próprio `recusar` — cai no
  último `.catch`. Esse `.catch` diz o tropeço no `console` da janela e não
  mexe no estado:
  - na região, por `RegiaoDeMod#dizerTropecoDaMidia(onde, chave, falha)`, que
    corta a chave como `dizerRecusaDeMidia`;
  - no avatar, por um `console.warn` com o id do MOD e a pessoa.

  Neste commit o tropeço ia só ao `console`. O ajuste do controlador, mais
  abaixo (`6ca4fdf`), o leva também ao `seele.log`.
- Nenhuma promessa fica sem quem a pegue.
- Fiz nos quatro caminhos, e não só nos dois que a revisão citou (avatar e
  fundo de tela). O retrato e o fundo de mídia tinham o mesmo defeito, por
  outra porta: um `midiaMudou` de dono que lança dentro do `anotarMidia`.
  O vermelho abaixo mostra o fundo de mídia na tela, «recusada» e com 0 bytes.
- No retrato e no fundo de mídia, o aviso ao dono (`anotarMidia`, que chama
  `midiaMudou`) vem por último dentro de `aplicar`. No retrato, um dono que
  lança encontra o `<img>` já montado, com o ouvinte de `error`, e os bytes
  contados. **No fundo de tela, não:** `anotarMidia("pronta")` vem antes de
  `pintarTela`, na ordem que já existia antes do lote. Um dono que lança ali
  encontra o mapa em `fundo.imagem` e anotado, e a repintura fica para o
  próximo desenho.

**Prova.**
- `regiao-do-mod.cjs` ganha `aplicarQueLancaNaoViraRecusa`:
  - no retrato e no fundo de mídia, o `midiaMudou` passa a lançar depois do
    «carregando»;
  - no fundo de tela, o `drawImage` do pincel lança na repintura;
  - cada caso confere a imagem montada, os bytes contados, a anotação
    «pronta», nada dito como recusa ao MOD ou ao registro, o tropeço no
    `console` e nenhuma `unhandledRejection`.
- `contribuicoes-e-camadas.cjs` ganha «R4e · o redesenho que lança»: com
  `redesenharAvatares` lançando, o avatar fica «pronta», com `bytes: 2` e o
  `uri`, sem `falhou` ao MOD, sem linha no registro, com o tropeço no `console`
  e sem promessa solta.

**RED** (contra `73866cc`), com `node apps/seele-app/bancada/regiao-do-mod.cjs`:
- `aplicar que lança · o retrato: … — montou=false, 0 bytes, [{"situacao":"recusada","motivo":"o aviso da gestão caiu",…}]`
- `aplicar que lança · o fundo de mídia: … — montou=true, 0 bytes, [{"situacao":"recusada","motivo":"o aviso da gestão caiu",…}]`
- `aplicar que lança · o fundo de tela: o mapa decodificou e a repintura lançou, e o fundo foi anotado como recusado com a imagem posta: imagem=true, [{"situacao":"recusada","motivo":"o pincel caiu",…}]`
- `aplicar que lança · o fundo de tela: o tropeço da repintura saiu como «fundo-falhou» ao MOD ou ao registro: [{"nome":"tela","chave":"mapa","estado":"fundo-falhou","porque":"o pincel caiu"}] …`
- `aplicar que lança: o tropeço de aplicar terminou numa promessa que ninguém pegou: Error: o aviso da gestão caiu …`

Com `node apps/seele-app/bancada/contribuicoes-e-camadas.cjs`:
- `R4e · o redesenho que lança: … {"uri":"data:image/png;base64,AB","bytes":0,…,"situacao":"recusada","motivo":"o redesenho caiu"}`
- `R4e · o redesenho que lança: o tropeço do redesenho foi dito como recusa ao MOD ou ao registro: [{"nome":"midia","chave":"avatar","estado":"falhou",…}] ["avatar da pessoa 17 não montou — o redesenho caiu"]`

É o defeito que a revisão descreveu, medido: o avatar visível, «recusado» e
fora da conta; o mapa posto e anotado «recusada», com `fundo-falhou` ao MOD.

**GREEN:**
- `regiao-do-mod` (40 provas) e `contribuicoes-e-camadas`;
- `cargo xtask check-runtime`, as sete;
- `cargo test -p seele-app --test frontend` (259 passed);
- `diagnostico-de-mods.cjs` no Chromium;
- o script descartável da seção 2, que deu o mesmo resultado no Chromium:
  retrato e mapa quebrados recusados pelo `decode()` de verdade, e o bloco
  dizendo «1 pronta · 2 recusadas».

**Reversões:**

| reversão | vermelho |
|---|---|
| C2a fundo de mídia: `.then(aplicar).catch(recusar)` | `aplicar que lança · o fundo de mídia: … montou=true, 0 bytes, [{"situacao":"recusada",…}]` |
| C2b retrato: `.then(aplicar).catch(…)` | `aplicar que lança · o retrato: … montou=false, 0 bytes, …` |
| C2c fundo de tela: `.then(aplicar).catch(recusar)` | `aplicar que lança · o fundo de tela: … anotado como recusado com a imagem posta …`, o `fundo-falhou` ao MOD, e `… engolido calado: []` |
| C2d avatar: `.then(aplicar).catch(recusar)` | `R4e · o redesenho que lança: … "bytes":0,…,"situacao":"recusada"`, o `falhou` ao MOD, e `… engolido calado …` |
| C2e fundo de mídia sem o `.catch` do tropeço | `aplicar que lança · o fundo de mídia: o tropeço de aplicar foi engolido calado: []` e `… terminou numa promessa que ninguém pegou: Error: o aviso da gestão caiu … at aplicar` |
| C2f avatar sem o `.catch` do tropeço | `R4e · o redesenho que lança: o tropeço do redesenho foi engolido calado, ou terminou numa promessa que ninguém pegou: [] Error: o redesenho caiu … at aplicar` |

As reversões principais do P51-07 mudaram de forma junto com o código, e rodei
de novo contra ele:
- R1' a R3', o `decode()` tirado do fundo, do retrato e do fundo de tela:
  `… a imagem não passou pelo \`decode()\`…`;
- R4', o avatar sem `decode()`: `R4e · a gestão: … "situacao":"pronta"`;
- R5a', o fundo sem `devolver()` em `recusar`:
  `a reserva · fundo · a recusa devolve: … 40 bytes`.

Todas ficaram vermelhas e voltaram byte a byte.

### Item 3: «API 3» fora do texto novo (`af7d6ef`)

O comentário do contexto da aba, em `contribuicoes-e-camadas.cjs`, passa a
dizer «os cartões e o tema do caminho antigo (`modsDoCaminhoAntigo`)».

A prova é de texto. `git diff fbeff8e -- apps/ | grep -c '^+.*API 3'` dá 0.
O `check-runtime` continua verde. Não há reversão a fazer: o item não tem
comportamento.

### Ajuste do controlador: o tropeço chega ao `seele.log` (`6ca4fdf`)

A primeira preocupação desta rodada era que o tropeço de aplicar ia só ao
`console` da janela. O controlador decidiu: o `console` ninguém lê, e o
tropeço tem de chegar ao `seele.log` de quem opera, com o mesmo `onde` e a
mesma forma de id das outras linhas de mídia.

**O que mudou.**
- `RegiaoDeMod#dizerTropecoDaMidia` continua escrevendo no `console`, com a
  pilha, e passa a dizer também pelo `dizerRecusaDeMidia`. É a porta das outras
  linhas de mídia: ela leva ao `dono.anotarRecusa`, que em `base.js` é
  `registrarNoAnfitriao("recusa-de-mod", "<id>: …", "aviso", <id>)`, e corta a
  chave e engole o dono que lança como sempre.
- No avatar, o último `.catch` chama `dono.anotarRecusa` dentro de um `try`. O
  avatar não tem `dizerRecusaDeMidia`, e as linhas dele já saíam por essa
  porta.
- O texto leva a palavra «tropeçou» e diz «não foi uma recusa», para a linha
  não ser lida como a recusa que não é.

As linhas, como chegaram ao `registrar_da_janela` nas bancadas:
- região: `{"nivel":"aviso","onde":"recusa-de-mod","oQue":"a/b: mídia «rosto» — o retrato tropeçou depois da busca, e não foi uma recusa: o aviso da gestão caiu","modId":"a/b"}`.
  O fundo e o fundo de tela saem na mesma forma, com «cx» e «mapa»;
- avatar: `{"nivel":"aviso","onde":"recusa-de-mod","oQue":"seele/perfis: avatar da pessoa 17 tropeçou depois da busca, e não foi uma recusa — o redesenho caiu","modId":"seele/perfis"}`.

**Prova.** Um anotador de mentira não mede nem o `onde` nem o id, porque quem
os põe é o dono de `base.js`. Por isso os casos de «aplicar que lança» rodam
com o `anotarRecusa` e o `registrarNoAnfitriao` **de verdade**, recortados de
`base.js` num contexto próprio com um `invoke` que anota, como faz o bloco «o
elo»:
- em `regiao-do-mod.cjs`, por `registroDeVerdade()`, nos três casos;
- em `contribuicoes-e-camadas.cjs`, no R4e, no avatar.

Cada caso exige uma linha só, com:
- `nivel: "aviso"`, `onde: "recusa-de-mod"` e `modId` igual ao id do MOD;
- `oQue` começando com o id e a chave;
- «tropeçou» e o motivo no texto;
- nada de «falhou no» ou «não montou», que são as frases da recusa.

**RED** (contra `af7d6ef`), quatro casos, todos com a lista de linhas vazia:
- `aplicar que lança · o retrato: o tropeço de aplicar não chegou ao seele.log como uma linha só, no \`onde\` das linhas de mídia e com o id do MOD em campo próprio — ou chegou dito como recusa: []`;
- o mesmo para o fundo de mídia e para o fundo de tela;
- `R4e · o redesenho que lança: o tropeço do redesenho não chegou ao seele.log como uma linha só, … : []`.

**GREEN:**
- `regiao-do-mod` (40 provas) e `contribuicoes-e-camadas`;
- `cargo xtask check-runtime`, as sete;
- `cargo test -p seele-app --test frontend` (259 passed);
- `diagnostico-de-mods.cjs` no Chromium;
- `cargo fmt --check`.

**Reversões** (restauradas byte a byte):

| reversão | vermelho |
|---|---|
| C4a `dizerTropecoDaMidia` sem a chamada a `dizerRecusaDeMidia` (só o `console`) | os três casos da região: `… não chegou ao seele.log como uma linha só … : []` |
| C4b o avatar sem o `try { dono.anotarRecusa(…) }` | `R4e · o redesenho que lança: o tropeço do redesenho não chegou ao seele.log … : []` |

### Preocupações da rodada

- **O tropeço vai ao registro no `onde` das recusas (`recusa-de-mod`)**, como
  foi pedido, para ficar junto das outras linhas de mídia do MOD. O que separa
  uma linha da outra é o texto: «tropeçou … e não foi uma recusa». Um filtro
  que conte as linhas por esse `onde` passa a contar também os tropeços.
- **Um `aplicar` que lança no meio deixa o que já tinha aplicado.** No retrato
  e no fundo de mídia, a ordem dentro dele põe o estado primeiro e o aviso ao
  dono por último, e por isso o estado que fica é o da tela. No fundo de tela, a
  anotação «pronta» vem antes da repintura, como já vinha: a repintura que lança
  deixa o mapa em `fundo.imagem` e anotado «pronta», e o próximo desenho o
  pinta.
