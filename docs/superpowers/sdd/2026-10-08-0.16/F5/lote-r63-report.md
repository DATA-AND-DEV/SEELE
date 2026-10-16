# Relatório do lote R63 (F5, 0.16)

O resto aberto da pendência 63: o que o tique de 500 ms ainda refazia sem mudança nenhuma na tela de sessão —
o contador, a telemetria, o `#voz-estado`, as regiões de MOD e a conversa.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/lote-r63`
- Ramo: `0.16/lotes-r63`, aberto no `1d0c7ebf` (o `0.16/lotes` com o AX e o C64).
- Brief: `.superpowers/sdd/lotes/lote-r63-brief.md`; a medida que é o vermelho, o `relatorio.md` da medida de
  16/10 (`…/scratchpad/m63/`), com os scripts dela reaproveitados.
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (cinco commits só dele),
  `apps/seele-app/bancada/linhas-pelo-id.cjs` (os casos 12, 13 e 14, numa seção própria), `.github/workflows/ci.yml`
  (duas linhas do comentário do passo da `linhas-pelo-id.cjs`), `docs/pendencias.md` (a 63) e este relatório.
- `base.js` **não** foi tocado. O fio não mudou.
- Status: **DONE_WITH_CONCERNS** — as preocupações estão no fim, e nenhuma bloqueia.

Nada foi empurrado, nenhum subagente foi usado, o app não foi aberto, o microfone não foi aberto, nada foi
baixado, o `xtask web` e a conformidade não rodaram, e a `~/.config/seele` de verdade não foi lida nem tocada.

## Os commits

| commit | passo | o que entra |
|---|---|---|
| `f642679c` | 1 | `escreverOTexto`: o texto só quando muda, e pelo `nodeValue` — o contador, a telemetria, o `#voz-estado` (só `tela-sessao.js`) |
| `59d08b0f` | 1 | os atributos reescritos iguais (a classe e o `title` de `medido`/`naoMedido`, o `data-faixa`, o `hidden` do rodapé) também só quando mudam (só `tela-sessao.js`) |
| `644efc18` | 1 | a bancada, caso 12 |
| `4824d313` | 2 | as regiões de MOD pelo `reconciliarALista`, com a chave da ação e o marcador do conteúdo (só `tela-sessao.js`) |
| `557ebae0` | 2 | a bancada, caso 13 |
| `3dd4d100` | 3 | o `vestirComo` escreve o `style` pelo CSSOM, que a CSP aceita (só `tela-sessao.js`) |
| `98f4e6cd` | 3 | a conversa reconciliada pelo id da mensagem (só `tela-sessao.js`) |
| `1c0760bd` | 3 | a bancada, caso 14, e a linha do comentário no `ci.yml` |
| este | — | a pendência 63 e este relatório |

Todas as mensagens estão em português e terminam em `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
Cada conserto vem antes da bancada dele, para nenhum commit do ramo ficar com a bancada vermelha; o vermelho de cada
caso foi rodado contra o código de antes do conserto, e está na mensagem do commit da bancada e abaixo.

## A medida antes

É a de 16/10 (o `relatorio.md` do `m63/`), no Chromium 151 e no WebKit 26.5 da bancada, com o `1d0c7ebf`. Os dois
motores deram os mesmos números de DOM; a árvore AX só se mede no Chromium, pelo CDP.

| região | o que se perdia, por tique |
|---|---|
| `#pessoas-conta` | a seleção, mesmo com o texto igual: o nó de texto trocado; 2 de 3 ids AX |
| a telemetria (7 números) | a seleção num número que não mudou (JITTER «3ms» → «»); 7 nós de texto trocados; 14 de 51 ids AX |
| `#voz-estado` (`role="status"`, à vista) | uma região viva reescrita duas vezes por segundo com o mesmo texto |
| `#roster-mods`, `#telemetria-mods`, com MOD | o foco na ação do MOD (cai no `BODY`) e a seleção no texto do MOD; 4 de 7 ids AX |
| a conversa | com mensagem nova, e também com alguém chegando ou um retrato mudando: o foco no link e no REMOVER, a seleção, e 433 de 434 ids AX (512 de 513). A rolagem ficava |

Antes de cada conserto, os scripts da medida foram reaproveitados no scratchpad desta sessão (`r63/`): o
`comum.cjs` e o `cenario-a.cjs` sem mudança, e o `medida-a.cjs` como `medida-r63.cjs`, apontado para este worktree.

## O desenho de cada passo

### Passo 1 — o texto que não mudou não é reescrito

`escreverOTexto(nodo, texto)`, em `tela-sessao.js`, ao lado de `medido`/`naoMedido`:

- com o texto igual, nada é escrito;
- com o texto novo, e o campo com um só nó de texto, muda o `nodeValue` desse nó, e não o nó. **Por que o
  `nodeValue`, e não o `textContent` com uma guarda:** a guarda sozinha resolveria o texto igual, mas a cada número
  que muda o leitor de tela ganharia outro nó, e uma seleção que tocasse o nó trocado iria a «». Com o `nodeValue`,
  o nó fica: o `StaticText` do Chromium mantém o id (só a `InlineTextBox` do texto que mudou é refeita), e uma
  seleção num número vizinho, que não mudou, não é tocada. Uma seleção **no** número que mudou colapsa — é o dado
  mudando, como a medida já dizia;
- fora desse caso — o campo vazio, ou com mais de um filho — vale o `textContent` de sempre, e a tela sai igual.

Usada em `medido`, `naoMedido` (19 chamadas em três arquivos, todas com o mesmo resultado na tela), no
`#voz-estado`, na marca da média da sala (`#tel-sinal-marca`) e no `#tel-aparelho`.

**Os atributos** (`59d08b0f`, à parte). Depois do texto, o tique ainda fazia, com o retrato igual, mutações de
atributo: o `class` do `#pessoas-conta` (`classList.remove` de uma classe que não está reescreve o `class` quando o
elemento tem um), o `class` e o `title` do `#tel-sinal-valor` (`naoMedido`), e o `hidden` do `#tel-local` e do
`#tel-aparelho`; com a média, o `data-faixa`. **A medida, com só o commit do texto:** no Chromium, eles não trocam id
AX nenhum (o contador 3 de 3, a telemetria 50 de 51 — o que troca é o relógio) e não tocam a seleção. O que eles
fazem é aparecer a todo observador de mutações, duas vezes por segundo; e o WebKit do app não se mede por aqui. Pus
a guarda — uma comparação cada um —, e assim o caso 12 exige «nenhuma mutação com o retrato igual», sem exceção para
atributo. Está num commit à parte justamente por ser uma decisão: tirá-lo é um `git revert` limpo, e o caso 12 diz
o que volta.

### Passo 2 — as regiões de MOD

`aplicarARegiao` passa pelo `reconciliarALista` do AX. **Não há um terceiro reconciliador**: a única mudança no
caminho do AX é o parâmetro `comChave` de `reconciliarALista` (padrão `"li"`, como sempre), que diz onde procurar os
nós velhos com chave — numa região de MOD, os botões de ação, que são filhos diretos do contêiner.

- **O conteúdo** do MOD é o mesmo nó entre um retrato e o seguinte (`montarContribuicao`): entra pelo marcador
  (`nosDeMod`), e não sai do documento.
- **O botão da ação** tem chave em `chaveNaLista`: `acao:<mod>\0<ação>#<n>`, o MOD e a ação, contados (um MOD pode
  repetir a mesma ação no mesmo ponto; sem a contagem, o segundo seria refeito a cada tique — a armadilha do id
  repetido do C64). Sem chave, casado pela posição pelo `mesmoFeitio`, a saída da ação de um MOD daria o botão dela à
  ação do vizinho, com o foco de quem estava nele — o defeito do FORA DO AR, que a medida mostrou.
- **Vazia e sem nada na tela** — o caso de quase todo quadro, sem MOD —, não há o que reconciliar e nada é alocado
  (a armadilha da alocação por quadro). O `hidden` só é escrito quando muda.
- Vale para as cinco regiões que passam por ali: `#roster-mods`, `#telemetria-mods`, e também `#operador-mods`,
  `#compositor-mods` e `#trilha-mods`.

**As closures velhas.** O brief supunha que o botão de `acoesDasContribuicoes` liga um ouvinte com os dados daquele
retrato. Não liga: o botão sai de `base.js` só com `data-*` (`data-acao-de-mod`, `data-mod-da-acao`, e o
`data-pessoa-da-acao` ou o `data-canal-da-acao` do contexto), e o clique é delegado no contêiner
(`ligarAcoesDeApresentacao`, um ouvinte por contêiner), que lê esses `data-*` **na hora do clique**. O risco
continua existindo noutra forma: o botão reaproveitado precisa ter os `data-*` de agora. Quem os escreve é o
`vestirComo`, que veste o botão velho com o botão montado no retrato de agora. Nada no botão guarda o retrato em que
ele nasceu. **A prova** é o caso 13 (e), na região `compositor`, que é a do mesmo caminho onde o dado da ação muda
de verdade entre retratos: o canal aberto passa do 1 ao 2, o mesmo botão, com o foco, manda `canal 2`. A reversão
R5 (o botão reaproveitado não vestido) reprova ali: «… entregou ["bancada/regioes-a:a-escrever:canal 1"]». Em
`#roster-mods` e `#telemetria-mods` o contexto é vazio e a ação não muda sem que a contribuição mude (o registro só
`atualizar` o conteúdo); por isso o caso usa o compositor. `base.js` não foi tocado.

### Passo 3 — a conversa, pelo id da mensagem

- **A chave** da mensagem é o id que o servidor deu (`mensagem:<id>`), ou, numa pendente — id zero —, a chave
  desta máquina (`pendente:<client_message_id>`). A pendente que é gravada troca de chave — a FFI manda o
  `client_message_id` só nas não gravadas —, e só ela ganha nó novo. O que isso custa a quem escreveu está em «O que
  ficou de fora» (desde a rodada 1).
- **O divisor de dia** tem a chave do dia, contada: a hora é do servidor e a ordem é a do id, e o mesmo dia pode
  abrir dois divisores — sem a contagem, os dois disputariam um nó.
- `comChave` é `":scope > li"`: só os filhos da lista, e não um `li` que um MOD desenhe dentro de uma mensagem.
- **Os nós de `conversa.mensagem`** — o corpo que um MOD desenha (`substituir`) e o que ele acrescenta
  (`adicionar`) — entram pelo marcador em `itemDeMensagem`. Sem ele, montar a mensagem os moveria para o nó novo,
  fora do documento, para a reconciliação os devolver (R8 reprova assim).
- **A rolagem** segue como antes: o `noFim` é medido antes de desenhar, e o fim é seguido depois. Com a conversa
  rolada para cima, a mensagem nova não a mexe; no fim, ela segue o fim.
- **O REMOVER e o link** agem com o dado de agora: os ouvintes são delegados na lista (`camada-moderar.js` e
  `abrirLinkDaMensagem`) e leem o `data-*` que o `vestirComo` reescreve, e `mensagens`. O comentário de
  `abrirLinkDaMensagem` passou a dizer isso.
- **O `style` pelo CSSOM** (`3dd4d100`, à parte, antes da conversa). O avatar de quem escreve ganha o retrato como
  `background-image` no `style` (`aplicarAvatar`). Medido nos dois motores, com a CSP que a bancada serve (a do
  produto, `style-src 'self'`): o `setAttribute("style", …)` do `vestirComo` deixa o texto no atributo e não aplica
  nada (`background-image` computado `none`, e o console diz que a CSP bloqueou); `style.cssText` aplica, e tirar o
  atributo continua valendo. Nenhum nó das listas que o AX e o C64 reconciliam tinha `style`, e o commit não muda
  nada nelas.

## Os casos

Na `linhas-pelo-id.cjs`, numa seção própria no fim («o resto da pendência 63»), sem bancada nova. Todos pelo
tique, como o caso 10, e cada passo confere que o tique — ou a conversa — redesenhou.

- **12. O texto que não mudou.** (a) Retrato igual: a seleção no contador fica; os nós de texto do contador, do
  `#voz-estado`, do JITTER e da média são os mesmos; 0 mutações no contador, no `#voz-estado` e no rodapé (fora o
  UPTIME — adiantado 5 s para o caso não depender da fase do tique —, que anda no mesmo nó, e o
  `#telemetria-mods`, que é do 13). (b) O ATRASO muda: a seleção no JITTER fica, o ATRASO muda no mesmo nó.
  (c) O `#voz-estado` selecionado atravessa o tique com 0 mutações; quem usa começa a falar e ele diz «… · no ar»
  no mesmo nó, sem mutação de nó. (d) A média passa do travessão ao número no mesmo nó, e o retrato seguinte não
  muda nada na célula. (e) Alguém chega: o contador muda no mesmo nó.
- **13. As regiões de MOD.** (a) Sem MOD, 0 mutações nelas (nem o `hidden`). (b) Com dois MODs pelo registro de
  verdade, duas passadas de retrato igual (o foco na ação de uma região e a seleção no texto do MOD da outra, e o
  contrário): o foco fica no mesmo botão, a seleção fica, conteúdos e botões são os mesmos, 0 mutações. (c) O
  clique na ação, depois dos tiques, chega ao MOD certo. (d) A ação de A sai, com o foco na de B: o botão de A sai, o
  de B fica com o foco, e o clique chega a B. (e) O canal aberto muda: o mesmo botão do compositor, com o foco, manda
  o canal novo.
- **14. A conversa**, com 60 mensagens, o foco no link da 11 e a seleção no corpo da 21, que valem através de todos
  os passos. (a) Alguém chega: 0 mutações, a rolagem em 800. (b) Um retrato novo: nenhum nó tirado nem posto na
  lista, toda mutação dentro de um avatar, o avatar é o mesmo nó e mostra a imagem. (c) Uma mensagem nova, rolada
  para cima: só ela entra (e o divisor, se ela virou a meia-noite), nada sai nem muda dentro das outras, a rolagem
  fica. (d) Outra, no fim: a rolagem segue o fim. (e) A 31 sai: só o nó dela sai. (f) A 12 e a 11 são editadas: o
  REMOVER e o link são os mesmos nós, o link abre o endereço novo e o REMOVER confirma o texto novo. (g) Os nós de
  `conversa.mensagem` (um acréscimo na 5, o corpo da 6) não saem do documento com uma mensagem nova.

A ponte simulada da bancada passou a dar um clone da conversa a cada pedido, como já dava o retrato, e a anotar o
que a página pede (`window.chamadas`), para o caso 14 (f) conferir o link.

**Os vermelhos** (contra o código de antes de cada conserto, com uma cópia da bancada no scratchpad em que as
asserções não param na primeira):
- 12: «o mesmo retrato, redesenhado pelo tique, desfez a seleção no contador: "20 EM SALAS · 50 NO SERVIDOR" →
  ""»; os quatro nós de texto trocados; 20 mutações no rodapé em dois tiques; o `#voz-estado` com `childList`.
- 13: o `hidden` das três regiões reescrito a cada tique; «tirou o foco da ação do MOD em #roster-mods: o foco
  está em BODY»; a seleção «MOD 42 ms» → «»; os quatro botões trocados; o clique depois da troca de canal sem
  entregar nada.
- 14: em todos os passos o foco no `BODY`, a seleção a «», as 60 mensagens trocadas; alguém chegando trocou a lista
  inteira; a mensagem nova tirou 62 nós e pôs 63; o link e o REMOVER editados trocados; 2 de 2 nós de MOD fora do
  documento.

## As reversões

Pelo `reverter.sh` do scratchpad: cópia do arquivo, mutante (`mutar.py`), toque, prova (a `linhas-pelo-id.cjs`),
devolução da cópia, toque, `cmp` contra a cópia e sha256 antes e depois. **Todas reprovaram, cada uma no seu
caso, e todas voltaram idênticas**; o `git status` ficou limpo.

| | o mutante | o que reprovou |
|---|---|---|
| R1 | `escreverOTexto` volta ao `textContent =` sempre | 12 (a): «desfez a seleção no contador: "20 EM SALAS · 50 NO SERVIDOR" → ""» |
| R1v | só o `#voz-estado` volta ao `textContent =` | 12 (a): «o mesmo retrato trocou o nó de texto de: voz» |
| R2 | a classe, o `title`, o `data-faixa` e o `hidden` escritos sempre | 12 (a): «o mesmo retrato fez mutações no contador: attributes:class@pessoas-conta, …» |
| R3 | `aplicarARegiao` de volta ao `repovoar` | 13 (a): o `hidden` das três regiões; com as asserções suaves, também o foco no `BODY`, a seleção, os botões trocados, o foco fora de B e o clique sem entrega |
| R4 | o botão da ação sem chave | 13 (d): «a ação do MOD A saiu do registro e o botão dela ficou na lista» (o nó de A virou o de B) |
| R5 | o botão reaproveitado não é vestido | 13 (e): «… entregou ["bancada/regioes-a:a-escrever:canal 1"]. Com o canal 1, o botão reaproveitado age com o dado do retrato em que nasceu» |
| R6 | o `tela-sessao.js` inteiro de antes da conversa (`3dd4d100`) | 14 (a): «alguém chegou ao servidor: o foco no link da mensagem 11 caiu (está em BODY)»; com as asserções suaves, de (a) a (g) |
| R7 | o `style` de volta ao `setAttribute` | 14 (b): «o avatar da mensagem 1 não mostra a imagem (background-image "none"): a CSP do produto recusa o `style` escrito como atributo» |
| R8 | os nós de MOD da conversa sem o marcador | 14 (g): «conversa.mensagem: 2 de 2 nós de MOD saíram do documento com uma mensagem nova» |
| R9 | a mensagem sem chave | 14 (e): «a mensagem 31 saiu: 31 mensagens trocaram de nó (32, 33, 34, 35, 36…)» |

R1 a R3 rodaram na ponta do passo 1 ou 2 (sha256 `e39d21bf…9a86` e `f7c9c420…02ee`), R4 a R9 nas pontas dos passos
2 e 3 (`f7c9c420…02ee`, `a838c5ca…e754`). **E todas de novo na ponta do código, `1c0760bd`**, uma depois da
outra: as dez reprovaram com a mesma frase, e o `tela-sessao.js` voltou idêntico em cada uma (sha256
`a838c5ca3b416e2173e58ff6a3900a86875127e0d84616d8772e2b21fe76e754` antes e depois).

## Os guardas, com os números

Na ponta do código, `1c0760bd` (depois vieram só os documentos). Os testes de Rust rodaram pelo
`cargo-casa-falsa.sh` do scratchpad: um script executável que dá a cada comando uma HOME falsa nova, mantém o
`rustup` e o `cargo` de verdade, confere que o log não ficou vazio e que a HOME falsa não ganhou `.config/seele` —
todas terminaram com 0 entradas. `CARGO_TARGET_DIR` desligado, e `--offline`.

- **A `linhas-pelo-id.cjs`**: os casos de 1 a 14, saída 0. Leva uns 40 s (eram 15), dentro dos 5 min do passo.
- **A `um-redesenho-por-quadro.cjs`**, com o atraso máximo:

  | quando | `1d0c7ebf` (antes) | ponta |
  |---|---|---|
  | soltas, antes de começar | 28, 36, 30 ms | — |
  | intercalada (carga 3,6–4,1) | 30, 28, 29 ms | 29, 31, 30 ms |
  | na bateria das 19 | — | 30 ms |

  Não piorou. O retrato dela não tem MOD nas regiões: lá, o `aplicarARegiao` cai no caminho de quem está vazio, que
  não aloca nada.
- **A conversa com 300 mensagens** (`medir-conversa.cjs`, do scratchpad, fora do CI): o `desenharMensagens` com uma
  mensagem entrando e saindo, 80 chamadas, intercalado com o `1d0c7ebf` três vezes — mediana 17,2–17,4 ms antes e
  16,2–16,4 ms na ponta, p90 18,7–19,0 contra 16,9–18,2. Com a conversa no fim, o desenho lê o `scrollHeight`, e o
  layout já está dentro da conta. Custa um pouco menos: os nós ficam, e o layout refaz menos.
- **As 19 bancadas do job `bancadas`**, cada uma por nome, lidas do `ci.yml`: 19 de 19.
- **Os 40 roteiros**, numa chamada da `tools/carga-da-casca.py` com os `--roteiro` lidos do `ci.yml`: «os 40
  passaram», «carga limpa: nenhum script estourou».
- **`cargo test -p seele-app --test frontend`**: 296 de 296 (depois de cada passo, e de novo depois dos documentos).
- **`cargo test -p xtask`**: 230 (88 + 4 + 121 + 1 + 3 + 10 + 1 + 2).
- **`cargo xtask check-runtime`**: passa, com as 12 bancadas de Node puro.
- **A bancada do celular inteira** (`python3 tools/bancada-do-celular.py <pasta>`): 110 casos passam, e o **G3 é
  IGUAL pixel a pixel no Chromium e no WebKit**. Levou 9 min 17 s.

**O que não rodou, como o brief manda:** o `cargo test --workspace`, as 3 bancadas de wasm (o `xtask web`), a
conformidade, a sonda `arvore-ax.py` e o app.

## A medida depois

Os scripts da medida (`medida-r63.cjs`, a cópia do `medida-a.cjs` apontada para este worktree), na ponta, nos dois
motores, com as mesmas 132 passadas por motor:

- **o foco**: 0 de 48 perdidos em cada motor — a ação do MOD nas duas regiões, o link e o REMOVER da conversa e o
  controle, nos seis casos (`igual`, `outra-fala`, `mensagem-nova`, `telemetria-muda`, `alguem-chega`,
  `retrato-de-alguem`);
- **a seleção**: 4 de 72 perdidas em cada motor, e as quatro são o dado mudando — o ATRASO com `telemetria-muda` e
  o contador com `alguem-chega`, nas duas páginas. O JITTER, o texto do MOD, o contador com o texto igual e o corpo
  da mensagem 20 ficam em todos os casos;
- **as mutações**: 0 no `#roster-mods`, no contador e na conversa com o retrato igual; na conversa, com
  `mensagem-nova`, 1 nó posto e 0 tirados; no rodapé, só o texto do UPTIME;
- **a árvore AX** (Chromium): com o retrato igual, o `#roster-mods` mantém 7 de 7 ids, o contador 3 de 3, a conversa
  434 de 434 e a telemetria 57 de 58 com MOD (50 de 51 sem) — o que troca é a `InlineTextBox` do UPTIME. Com a
  mensagem nova, a conversa mantém os 434 e ganha 7; com alguém chegando ou um retrato mudando, 513 de 513;
- **as regiões vivas**: o `#voz-estado` saiu da lista das que o tique muta (antes, 1 mutação por tique em todos os
  casos). Ficam o `#banner`, o `#aviso-audio` e o `#bateria`, escondidas, com atributos (ver «O que ficou de fora»);
- **a rolagem**: 800 → 800 em todos os casos.

Os brutos estão no scratchpad desta sessão (`r63/saidas/medida-ponta-{chromium,webkit}.txt`, e o
`medida-so-texto-chromium.txt`, com só o commit do texto).

## O que ficou de fora

- **O grupo FORA DO AR** (`grupoForaDoAr`), que só existe no `0.16/sala-viva`. A medida de 16/10 confirmou que as
  linhas casam pela posição e que a chave `membro:<id>` resolve. Não foi tocado aqui; a 63 segue aberta por ele.
- **O celular** (dona: a F8): a coluna de salas segue adaptada a cada quadro pelo `celular-servidor.js`. E o
  comentário do ouvinte da folha da mensagem em `celular-conversa.js:62-63` diz «a lista é refeita quando chega
  mensagem»: deixou de ser verdade. O código continua certo com a reconciliação — o ouvinte é delegado e, na hora do
  toque, lê o `data-mensagem` (que o `vestirComo` reescreve), lê `mensagens[indice]` e procura o REMOVER pelo id.
  Trocar o comentário é da F8; desde a rodada 1 a 63 o registra para ela. Não mexi no arquivo.
- **O que um VoiceOver de verdade anuncia** — de um `nodeValue` que muda no `#voz-estado`, por exemplo. Não se mede
  aqui, e o app não foi aberto.
- **Outras escritas iguais por tique, fora das cinco regiões do brief**: o `#banner`, o `#aviso-audio` e o
  `#bateria` (regiões vivas, escondidas: só atributos, `hidden`), o `#operador-nome`, a `#operador-inicial`, os
  botões MUDO e SURDO (`replaceChildren` do glifo a cada quadro) e o `#canal-cabecalho-mods` e as
  `#compositor-ferramentas`, que ainda passam por `repovoar` direto. A medida não os pediu; ficam registrados.
- **A própria mensagem gravada perde o nó: é um limite conhecido** (corrigido na rodada 1, M2; antes este item
  dizia só que caía o foco num TENTAR DE NOVO). A pendente tem a chave da máquina (`pendente:<client_message_id>`) e
  a confirmada a do servidor (`mensagem:<id>`), e as duas não casam: quando a mensagem que a própria pessoa mandou é
  gravada, a `li` é outra. Vão com ela o link, o REMOVER (a própria mensagem sempre o tem), a seleção no corpo e o
  nó do leitor de tela — **em toda mensagem que a própria pessoa manda**, e não só nas que falharam. A revisão mediu
  (uma pendente `Enviando`, própria, com um endereço: o foco no link vai ao `BODY` e a seleção a «»), e o caso 14 (h)
  vê a `li` velha sair do documento.

  **Onde o dado se perde.** O fio já leva o eco: o `MessageReceived` do `seele-proto` tem `client_message_id`
  («Echo of the sender's identifier, so a client can match its own pending send», `control.rs:1934-1936`). Quem o
  perde é o cliente: a `Message` da sala (`crates/seele-sessao/src/state.rs`) não guarda o campo, e a FFI monta
  `client_message_id: None` nas confirmadas (`crates/seele-ffi/src/lib.rs:3646`). Sem ele, a tela não tem como
  ligar a gravada à pendente que ela foi, a não ser por heurística.

  **O caminho:** a F1 guarda o eco e o leva pela FFI nas confirmadas próprias — a orquestradora já pediu isso a
  ela. Depois que a ponta dela chegar, a F5 faz a pendente e a confirmada própria casarem pela mesma chave na
  conversa, com um caso na `linhas-pelo-id.cjs` que começa vermelho: a própria mensagem gravada mantém o nó, a
  seleção e o REMOVER.
- **Mensagens mais velhas entrando no começo da lista**: a rolagem segue o comportamento do navegador com nós que
  ficam (a âncora de rolagem do Chromium); não medi. O caso do brief é a mensagem nova no fim.

## Preocupações

1. **O passo 1 tem uma decisão que o brief condicionou à medida.** Os atributos reescritos iguais não trocam id AX
   nem tocam a seleção no Chromium; eles aparecem a todo observador de mutações. Guardei-os mesmo assim, num commit à
   parte (`59d08b0f`), para o caso 12 exigir 0 mutações sem exceção. Se a orquestradora preferir não ter esse
   commit, um `git revert` dele e um afrouxamento do caso 12 (só `childList`/`characterData`) bastam.
2. **O `vestirComo` mudou** (`3dd4d100`): o `style` passa pelo CSSOM. É o reconciliador do AX e do C64; nenhuma linha
   deles tem `style` hoje, e as bancadas deles seguem verdes, mas é uma mudança no caminho compartilhado, e a causa —
   a CSP recusando o atributo escrito — vale para qualquer lista que passe a ser reconciliada com `style` dentro.
3. **O `aplicarARegiao` vale para cinco regiões**, e não só para as duas do brief: `#operador-mods`,
   `#compositor-mods` e `#trilha-mods` também deixaram de ser repovoadas. A `regioes-do-produto.cjs` e as outras
   bancadas de MOD seguem verdes; a casca do celular move o `#operador-mods` (`celular-ajustes.js`) e não lê os filhos.
4. **O ambiente.** O `target/debug` deste worktree foi semeado do `eloquent-blackburn`, que já não tem
   `target/debug`, e o build do `seele-app` reprovou na primeira vez: as listas de permissões que o build do `tauri`
   grava em `target/debug/build/tauri-*/out/` têm caminhos absolutos para a pasta de origem. Reescrevi esses 30
   arquivos de texto (`eloquent-blackburn-a0bf2c/target/` → `lote-r63/target/`), só neste worktree; os outros
   arquivos do target não foram tocados, e os testes rodaram com `--offline`. Nada de produto mudou por isso.
5. **A sessão mudou de worktree pelo `EnterWorktree`.** O gancho da sessão recusou o `Edit` no `lote-r63` enquanto
   ela estava no `eloquent-blackburn`; o `EnterWorktree` com o caminho do `lote-r63` pôs a sessão nele, e nada foi
   escrito no `eloquent-blackburn`.

## Rodada 1

A revisão (`.superpowers/sdd/lotes/review-r63-report.md`) deu o lote conforme e aprovado com ressalvas: 0
Críticos, 1 Importante e 3 Menores. O código estava certo; faltavam guardas e docs. O pedido da rodada é o
`.superpowers/sdd/lotes/lote-r63-rodada1.md`, com uma correção da orquestradora sobre o M2 (não é o fio: o fio já
leva o eco, e quem o perde é o cliente). Ponta de partida `31102f9f`. **A rodada não muda código de produção**: o
`tela-sessao.js` segue com o sha256 `a838c5ca3b416e2173e58ff6a3900a86875127e0d84616d8772e2b21fe76e754`, e o `git
diff` de `apps/seele-app/ui/` desde o `31102f9f` é vazio.

### Os commits

| commit | achado | o que entra |
|---|---|---|
| `a3382f33` | I1, M1 | a bancada: 13 (f), 14 (h) e 14 (i) |
| este | M2, M3 | o relatório e a 63 |

### I1 e M1 — os três passos, cada um provado pelo seu mutante

A revisão achou três propriedades que o código cumpre, que este relatório afirma e que nenhum caso prendia; os
mutantes de uma linha dela passavam verdes. Os cenários são os da sonda dela (`sonda-r63.cjs`).

- **13 (f), a mesma ação contribuída duas vezes.** Um MOD contribui a ação `x` duas vezes em `pessoas.lista`
  («X1» e «X2»), com o foco no segundo botão. Depois de um tique: o mesmo nó, no documento, o `activeElement`, e 0
  mutações no `#roster-mods`.
- **14 (h), duas pendentes, e a primeira é gravada.** Duas pendentes `SemResposta`, a 77 e a 78, com o foco no
  TENTAR DE NOVO da 78; a 77 é gravada (id 500). O mesmo botão, com o foco; a `li` da 78 `===`; a `li` velha da 77
  fora do documento; nenhuma `li` que dizia uma pendente dizendo a outra.
- **14 (i), a hora que volta atrás.** Quatro mensagens nos dias D1, D2, D1, D2 (ao meio-dia local, para a virada da
  meia-noite não mudar o dia), que abrem quatro divisores; a conversa igual, redesenhada: 0 mutações e os divisores
  `===`.

| | o mutante | o que reprovou |
|---|---|---|
| Ma | a chave do botão sem o `#${vez}` (`tela-sessao.js:761`) | 13 (f): «a mesma ação, contribuída duas vezes, perdeu o foco do segundo botão no tique: o botão não é o mesmo nó, saiu do documento, e o foco está em BODY» |
| Mb | o divisor sem a contagem (`:1944`) | 14 (i): «com dois divisores do mesmo dia, a conversa igual, redesenhada, teve 4 mutações»; com as asserções suaves, também «2 divisores trocaram de nó» |
| Mc | a pendente sem chave (`:1966`) | 14 (h): «o TENTAR DE NOVO da segunda, com o foco, não é o mesmo botão: o foco está em BODY»; com as asserções suaves, também a `li` da 78 trocada, a velha da 77 no documento, e a linha que dizia «primeira pendente» dizendo outra mensagem |

Pelo mesmo `reverter.sh`: as três reprovaram, e o `tela-sessao.js` voltou idêntico pelo `cmp` em cada uma (sha256
`a838c5ca…e754` antes e depois).

### M2 — a própria mensagem gravada

O item de «O que ficou de fora» dizia só que caía o foco num TENTAR DE NOVO. Agora diz o que se perde — o link, o
REMOVER, a seleção e o nó do leitor de tela, em toda mensagem que a própria pessoa manda —, onde o dado se perde (o
fio leva o eco, `control.rs:1934-1936`; o cliente o perde, `state.rs` sem o campo e a FFI com `None`,
`lib.rs:3646`) e o caminho (a F1, e depois a F5, com o caso começando vermelho). A 63 registra o mesmo, como item
dela, sem número novo. O passo 3 do desenho deixou de atribuir a falta «ao Rust» em geral.

### M3 — o comentário do celular

Uma linha no item «o celular» da 63: o comentário de `celular-conversa.js:62-63` ficou falso, o código está certo
com a reconciliação, e a troca é da F8. O arquivo não foi tocado.

### O que rodou na rodada

Na ponta `a3382f33` (o código de produção é o mesmo do `31102f9f`):
- **a `linhas-pelo-id.cjs`**, todos os casos: saída 0, em uns 49 s;
- **a `um-redesenho-por-quadro.cjs`**: o maior atraso 28 ms, na passada da bateria;
- **as 19 bancadas do job `bancadas`**: 19 de 19, com a carga de 1 min entre 8 e 22 (outras sessões rodando);
- **os 40 roteiros**: «os 40 passaram», «carga limpa: nenhum script estourou»;
- **o cargo e a bancada do celular não rodaram**, como o pedido dispensa: a rodada não muda código de produção nem
  o `frontend.rs`. As últimas passadas são as da rodada 0, acima (296 de 296 no `frontend`, 230 no `xtask`, o
  `check-runtime` passando, e o celular com 110 casos e o G3 IGUAL).
