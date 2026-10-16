# Auditoria do `api/v6.json` — frente 4: `formas` e `figuras`

Ramo `0.16/api-6-corte`, ponta `48b6a1ba`. Só leitura no repositório.

## O que as duas seções prometem

As duas são **listas de nomes**, sem atributo nenhum: `formas` tem 30 nomes e
`figuras` tem 4 (`api/v6.json:180-217`). A promessa verificável de cada entrada
é, então, «o renderer monta esta forma (ou pinta esta figura), e não a recusa
como desconhecida». Os atributos que outras seções prendem a formas (`erro`,
`sujo`, `validacao`, `confirmacao` em `superficies.dialogo.formularios`; os 71
bytes de figura com `chave` e `cor` em `limites`; o toque em figura com `chave`
em `celular`) são de outras frentes — ver «Fora das minhas seções», no fim.

O renderer é um só (desktop, web e celular): `apps/seele-app/ui/mods-regiao.js`,
tabela `FORMAS_DA_REGIAO` (l. 358-393). Conferido por script: o conjunto de
chaves dela é **igual** ao `formas` do `v6.json` (30 = 30, nenhuma sobra dos dois
lados). Não há validação de árvore do lado Rust (`git grep` em `crates/` e
`apps/seele-app/src/`: nada).

## Como medi

As reversões foram feitas **numa cópia** de `apps/seele-app/ui/` e das bancadas no
scratchpad (`scratchpad/copia/`), nunca no repositório:

- `regiao-do-mod.cjs` (node puro): linha de base verde («as 49 provas»); depois,
  uma reversão por vez em `mods-regiao.js` (tirar a linha da forma de
  `FORMAS_DA_REGIAO`, ou renomear o `case` da figura em `pintarFiguras`).
- `camadas.cjs` (navegador): linha de base verde com
  `PLAYWRIGHT=<o node_modules/playwright de um repositório de MOD>`;
  reversões de `escolha`, `numero`, `cor` e `formulario`.

«medido: reprova» quer dizer que a reversão deixou a bancada vermelha; «medido:
passa» quer dizer que a bancada continuou verde **com a entrega tirada**.

Onde cada prova roda:

- `regiao-do-mod.cjs` → `BANCADAS` de `xtask/src/check_runtime.rs:101-115` →
  job «bancadas de MOD (node puro)», `cargo xtask check-runtime`
  (`.github/workflows/ci.yml:477-496`, ubuntu).
- `camadas.cjs` → job «bancadas de navegador», `ci.yml:350`
  (ubuntu, chromium, `if: !cancelled()`).
- `mod_de_referencia.rs` → `cargo test --workspace` (CI, três sistemas); não tem
  `#[ignore]` nem `cfg`.

## A tabela

| Entrada (caminho no JSON) | Promessa (curta) | Código (arquivo:linha) | Prova (arquivo: caso/teste; onde roda) | Estado | Nota |
|---|---|---|---|---|---|
| `formas[0]` `titulo` | monta um título | `mods-regiao.js:361` (`h3`), com filhos `:425-433` | `regiao-do-mod.cjs:713` `oCartaoUsaOMesmoRendererComGramaticaMenor` («o título virou h3», texto «Lia»); também `:910`; check-runtime | PROVADA | medido: reprova (4 falhas). Provado pelo perfil do cartão, que usa o mesmo `planejar`/`criar`. |
| `formas[1]` `texto` | monta um parágrafo | `mods-regiao.js:360` (`p`) | `regiao-do-mod.cjs:713` («o texto virou p»), `:1363` (forma desconhecida não vira elemento, o `texto` sim); check-runtime | PROVADA | medido: reprova (6 falhas). |
| `formas[2]` `linha` | monta uma linha (contêiner horizontal) | `mods-regiao.js:362` (`div`), filhos `:425-433`; layout `mods-controles.css:258` | `regiao-do-mod.cjs:3166` `oSomNumCartaoNaoTemBotao`, caso «dois sons… um em cada linha» (`:3204-3232`); check-runtime | PROVADA | medido: reprova (14 falhas). **Indireta**: o caso é do som, e reprova porque as mídias dentro da `linha` somem. O layout horizontal (o CSS) não tem prova. |
| `formas[3]` `lista` | monta uma lista | `mods-regiao.js:363` (`ul`), filhos `:425-433` | nenhuma: nenhum teste que roda declara `forma: "lista"` (só a sonda manual `testes/sonda-de-fronteira/cliente/main.js:258`, fora do CI); o guarda de `frontend.rs:14450` só confere que `lista` está no perfil do **cartão**; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | Tirar `lista` de `FORMAS_DA_REGIAO` não reprova nada (o guarda do cartão lê `PERFIS_DE_RENDER.cartao`, não a tabela). Prova: um caso em `regiao-do-mod.cjs` com `lista` › `item` conferindo `ul > li` e o texto. |
| `formas[4]` `item` | monta um item de lista | `mods-regiao.js:364` (`li`), filhos `:425-433` | nenhuma (idem `lista`; `frontend.rs:14451` é do cartão); **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | O mesmo caso de `lista` cobre os dois. |
| `formas[5]` `campo` | monta um campo de texto editável | `mods-regiao.js:365` (`label`), `montarCampo:1227`, `atualizarCampo:1256` | `regiao-do-mod.cjs:910`, `:1635`, `:1656`; `camadas.cjs` §7 (`:369`), §9 (`:457`), §10 (`:485`), §13 (`:577`); check-runtime e navegador | PROVADA | medido: reprova (8 falhas no node). |
| `formas[6]` `escolha` | monta uma escolha (select) | `mods-regiao.js:366` (`label`), `montarEscolha:1284`, `atualizarEscolha:1309` | `camadas.cjs` §12 (`:541`, «a escolha não mostrou o erro do MOD», `:557`); navegador | PROVADA | medido: reprova (`camadas.cjs`). Nenhuma bancada node a declara. |
| `formas[7]` `botao` | monta um botão que fala `botao` | `mods-regiao.js:367` (`button`), `montarBotao:1343`, `atualizarBotao:1191` | `regiao-do-mod.cjs:1576` (quatro recursos registrados), `:1733` (clique no rodapé chega uma vez); `camadas.cjs` §11 (`:516`); check-runtime e navegador | PROVADA | medido: reprova (5 falhas). |
| `formas[8]` `arquivo` | monta o botão que abre o seletor do produto | `mods-regiao.js:368` (`button`), `montarArquivo:1394`, `atualizarArquivo:1464` | `regiao-do-mod.cjs:1678` (finalidade), `:1705` (recusa do seletor pelo nome); check-runtime | PROVADA | medido: reprova (3 falhas). |
| `formas[9]` `tela` | monta a tela de desenho (canvas) | `mods-regiao.js:369` (`canvas`), `montarTela:2436`, `pintarTela:2665` | `regiao-do-mod.cjs:981`, `:1046`, `:1097`; check-runtime | PROVADA | medido: reprova (10 falhas). |
| `formas[10]` `midia` | monta a mídia gerenciada | `mods-regiao.js:370` (`figure`), `montarMidia:2829`, `atualizarMidia:3526` | `regiao-do-mod.cjs:840`, `:1191` e as seções 3-20; `camadas.cjs` §5 (`:269`); check-runtime e navegador | PROVADA | medido: reprova (134 falhas). |
| `formas[11]` `caixa` | monta uma caixa de composição (com `fundoDeMidia`) | `mods-regiao.js:372` (`div`), `:1091` (`montarFundoDeMidia:1827`); CSS `mods-controles.css:229` | `regiao-do-mod.cjs:1765` `fundoTrocaSoltaECancela`, seções 9-10; `camadas.cjs` §5 e §6 (`:305`); check-runtime e navegador | PROVADA | medido: reprova (24 falhas). Provada pelo caminho do `fundoDeMidia`; a caixa como contêiner simples é provada pela montagem por partes de `camadas.cjs` §6. |
| `formas[12]` `pilha` | monta uma pilha vertical | `mods-regiao.js:373` (`div`); CSS `mods-controles.css:213` | `regiao-do-mod.cjs:1613` `aPreviaNaoTrocaOsAncestraisDoCampo` **declara** uma pilha, mas não confere nada dela; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | medido: **passa** com `pilha` fora da tabela. O caso só confere o campo vizinho. Prova: no mesmo caso, conferir que a pilha montou (um `div[data-forma="pilha"]` com «Perfil» dentro). |
| `formas[13]` `grade` | monta uma grade | `mods-regiao.js:374` (`div`), `:1159`; CSS `mods-controles.css:231` (2 colunas) | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | `frontend.rs:14455` só confere o perfil do cartão. |
| `formas[14]` `rolagem` | monta um contêiner que rola | `mods-regiao.js:375` (`div`); CSS `mods-controles.css:238` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[15]` `separador` | monta um separador | `mods-regiao.js:376` (`hr`); CSS `mods-controles.css:247` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[16]` `espaco` | monta um espaço flexível | `mods-regiao.js:377` (`div`); CSS `mods-controles.css:253` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[17]` `formulario` | monta um `form` que envia `submeter` com os valores | `mods-regiao.js:379` (`form`), `montarFormulario:2080`, `atualizarFormulario:2106`, `valoresDoFormulario:2170` | `camadas.cjs` §7 (`:369`, «o submeter não levou os valores», `:427`), §9, §10, §13 (`:613`); navegador | PROVADA | medido: reprova em `camadas.cjs` (por *timeout* do `page.fill`, e não por uma asserção nomeada). Em `regiao-do-mod.cjs`, medido: **passa** sem ela: o caso `:1613` não a confere. |
| `formas[18]` `acoes` | monta a faixa de ações (com `fixas` no rodapé) | `mods-regiao.js:380` (`div`), `aplicar:839-842`, `atualizar:1163-1167`; CSS `mods-controles.css:284-291` | `regiao-do-mod.cjs:1733` `oRodapeEReconciliadoSemMoverOBotao`; check-runtime | PROVADA | medido: reprova (4 falhas). Só o caminho com `fixas: true` é exercitado; `acoes` sem `fixas` (contêiner comum) não tem caso. |
| `formas[19]` `abas` | monta abas com teclado, só o painel ativo, evento `aba` | `mods-regiao.js:381` (`div`), `montarAbas:2343`, `atualizarAbas:2368`, `aplicar:844-847` | nenhuma de comportamento; `frontend.rs:14696` é guarda de **texto** (`tiras.dataset.abas !== assinatura`); **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | O guarda de texto não reprova se `abas` sair da tabela: existir não é funcionar. É a única origem do evento `aba` da lista de `eventos`. |
| `formas[20]` `aba` | monta o painel de uma aba (`tabpanel`) | `mods-regiao.js:382` (`section`), `atualizarAba:2419`; CSS `mods-controles.css:555` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[21]` `textoLongo` | monta uma caixa de várias linhas que fala `campo` | `mods-regiao.js:383` (`label`), `montarTextoLongo:1508`, `atualizarTextoLongo:1529` | nenhuma; `frontend.rs:14432` só a proíbe no cartão; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[22]` `numero` | monta um número com limites, que fala `numero` | `mods-regiao.js:384` (`label`), `montarNumero:1554`, `atualizarNumero:1578` | `camadas.cjs` §7 (`:369`: `minimo`/`maximo`/`inteiro`; `submeter` leva `nivel: 5, peso: 70` como número, `:427`); navegador | PROVADA | medido: reprova (*timeout* do `page.fill` no campo `nivel`). O evento `numero` em si não é conferido. |
| `formas[23]` `deslizante` | monta um deslizante com o valor legível | `mods-regiao.js:385` (`label`), `montarDeslizante:1598`, `atualizarDeslizante:1619` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | Ver «Fora das minhas seções»: não recebe `erro` nem `formulario.erros`. |
| `formas[24]` `marca` | monta uma caixa de seleção que fala `marca` | `mods-regiao.js:386` (`label`), `montarMarca:1637`, `atualizarMarca:1656` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | Com `interruptor`, é a única origem do evento `marca`. |
| `formas[25]` `interruptor` | monta um interruptor (`role=switch`) | `mods-regiao.js:387` (`label`), `montarMarca:1637`, `role` em `:1665` | nenhuma; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | — |
| `formas[26]` `cor` | monta seletor de cor e campo hexadecimal | `mods-regiao.js:388` (`label`), `montarCor:1680`, `atualizarCor:1731` | `camadas.cjs` §12 (`:541`, «a cor não mostrou o erro do MOD», `:558`; hexa incompleto); navegador | PROVADA | medido: reprova (`camadas.cjs`). |
| `formas[27]` `retrato` | monta o retrato (imagem com inicial) | `mods-regiao.js:390` (`span`), `montarRetrato:1903`, `atualizarRetrato:2025` | `regiao-do-mod.cjs:840` `aImagemAcompanhaAMudancaDaFonte` (laço `retrato`/`midia`), `:2430`; `camadas.cjs` §5; check-runtime e navegador | PROVADA | medido: reprova (22 falhas). |
| `formas[28]` `distintivo` | monta um distintivo | `mods-regiao.js:391` (`span`); CSS `mods-controles.css:597` | `regiao-do-mod.cjs:1613` **declara** um distintivo e não o confere; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | medido: **passa** com `distintivo` fora da tabela. A mesma prova sugerida para `pilha` cobre os dois. |
| `formas[29]` `link` | monta um botão que abre o endereço fora, mostrando-o | `mods-regiao.js:392` (`button`), `montarLink:2049`, `atualizarLink:2061`; `base.js:1145` (`abrirEndereco`) | nenhuma; `frontend.rs:14438` só o proíbe no cartão; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | É a única origem do evento `link`. |
| `figuras[0]` `retangulo` | pinta um retângulo pegável | `mods-regiao.js:2757-2761` (`pintarFiguras:2733`) | `regiao-do-mod.cjs:1097` `oArrastePegaAFiguraDeCima` (pintado `retangulo`; acerto em `peca-de-baixo`); check-runtime | PROVADA | medido: reprova (2 falhas). `preenchida: false` (`strokeRect`) e `cor` não são conferidos. |
| `figuras[1]` `circulo` | pinta um círculo pegável | `mods-regiao.js:2762-2770` | `regiao-do-mod.cjs:1097` (pintado `circulo`; acerto em `peca-de-cima`), `:1046`; check-runtime | PROVADA | medido: reprova (5 falhas). |
| `figuras[2]` `texto` | pinta um texto pegável | `mods-regiao.js:2771-2786` | `regiao-do-mod.cjs:1097` (pintado `["texto","GOBLIN"]`); check-runtime | PROVADA | medido: reprova (1 falha). O acerto do texto e o `corpo` não são conferidos. |
| `figuras[3]` `linha` | pinta uma linha, que não é pegável | `mods-regiao.js:2787-2795` | `regiao-do-mod.cjs:1097` cita a «parede», mas só confere que ela **não é pega** (`:1179-1184`); **passo 5**: `regiao-do-mod.cjs` `cadaFiguraDoV6Pinta`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | medido: **passa** com o `case "linha"` renomeado (a figura vira desconhecida, não é pintada e conta uma recusa que ninguém lê). O pincel de mentira já registra: o primeiro `"traco"` do `pintado` é a linha. Prova: conferir `pintado[0] === "traco"` (o tabuleiro não tem `tracos`), ou `regiao.aplicar(...) === 0`. |

### As duas listas como um todo (fora da contagem acima)

| Entrada | Promessa | Código | Prova | Estado | Nota |
|---|---|---|---|---|---|
| `formas` (a lista, nos dois sentidos) | o produto monta estas 30, e só estas | `FORMAS_DA_REGIAO:358-393` (hoje igual ao JSON, conferido por script) | **nenhum guarda espelho.** O `check-api` (`xtask/src/check_api.rs`, `regras_da_v6:809-816`) confere pontos, tipos, nomes de servidor, tetos e apontadores, e não lê `formas`. `mod_de_referencia.rs:311-329` confere só que as 8 formas do vetor (`acoes`, `arquivo`, `botao`, `campo`, `midia`, `tela`, `texto`, `titulo`) estão na tabela.; **passo 5**: `regiao-do-mod.cjs` `cadaFormaDoV6MontaOQuePromete`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | O rascunho §13 (`docs/superpowers/specs/2026-10-08-api-6-rascunho.md:1733`) diz que o `check-api` «passa a conferir, nos dois sentidos», as formas (tarefa 2), e a tarefa 2 do plano não as inclui. Uma forma tirada do renderer, ou acrescentada sem o JSON, só reprova onde houver bancada, e 15 das 30 não têm. O sentido «só estas» é provado: uma forma desconhecida não vira elemento (`regiao-do-mod.cjs:1363`). **Mas ela some calada** nos perfis `regiao` e `superficie`: `mods-regiao.js:966` só conta a recusa quando há `permitidas` (o cartão). Medido numa sonda: `{forma:"textolongo"}` dá `recusados = 0` em `regiao` e em `superficie`, e `1` no cartão. O `superficie-montar` responde `ok`, e o comentário de `:354-356` («recusada e contada») não vale ali. «O produto sabe e não conta.» |
| `figuras` (a lista, nos dois sentidos) | o produto pinta estes 4 tipos, e só estes | `pintarFiguras:2756-2800` (`switch`; o `default` conta a recusa) | nenhum guarda espelho; nenhum caso declara um tipo desconhecido; **passo 5**: `regiao-do-mod.cjs` `cadaFiguraDoV6Pinta`, com o espelho nos dois sentidos (portão), `09c9a375` | PROVADA | Medido numa sonda: `{tipo:"estrela"}` dá `recusados = 1`, então este lado conta. Falta só a prova. |

## Contagem (as 34 entradas)

- PROVADA: 18 (formas: `titulo`, `texto`, `linha`, `campo`, `escolha`, `botao`,
  `arquivo`, `tela`, `midia`, `caixa`, `formulario`, `acoes`, `numero`, `cor`,
  `retrato`; figuras: `retangulo`, `circulo`, `texto`)
- INCERTA: 0 (as três que seriam incertas — `pilha`, `distintivo`, figura
  `linha` — foram medidas: a reversão passa, e elas vão para SEM PROVA)
- SEM PROVA: 16 (formas: `lista`, `item`, `pilha`, `grade`, `rolagem`,
  `separador`, `espaco`, `abas`, `aba`, `textoLongo`, `deslizante`, `marca`,
  `interruptor`, `distintivo`, `link`; figura `linha`)
- SEM CÓDIGO: 0
- NÃO SE APLICA: 0

Além delas, as duas linhas de «a lista como um todo», as duas SEM PROVA.

## Fora das minhas seções (achado no caminho, para a frente de `superficies` e a de `eventos`)

- `superficies.dialogo.formularios.erro` promete o erro «em toda forma de entrada,
  inclusive cor e escolha». **`deslizante`, `marca` e `interruptor` não chamam
  `marcarValidade`** (`mods-regiao.js:1598-1678`; as que chamam estão em `:1262`,
  `:1316`, `:1541`, `:1589` e `:1744`). Então o `erro` declarado nelas é
  ignorado, `formulario.erros({ <chave delas>: … })` é recusado como
  `chave-desconhecida` (`camposPorChave:2234` só conhece quem passou por
  `caixasDoCampo.set`, `:2303`; a recusa sai em `mods-superficies.js:740`), e
  mexer nelas não suja a superfície (`temAlteracoes` lê `iniciais`, que só
  `marcarValidade` grava). O `valoresDoFormulario:2170` as inclui no envio. Se
  «forma de entrada» inclui essas três, é SEM CÓDIGO na outra frente.
- Os eventos `aba`, `marca` e `link` da lista `eventos` só nascem de formas SEM
  PROVA aqui (`atualizarAbas:2398`, `montarMarca:1644`, `base.js:1154`). O
  `check-api` acha o `falar({ nome })` pelo texto, mas nenhum teste os dispara.
- O MOD de referência, que o cabeçalho de `mod_de_referencia.rs` descreve como
  «um MOD mínimo… que usa cada coisa que a API oferece», declara 8 das 30 formas
  e nenhuma figura.
