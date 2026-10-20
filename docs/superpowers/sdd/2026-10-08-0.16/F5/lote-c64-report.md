# Relatório do lote C64 (F5, 0.16)

A pendência 64: a coluna de canais reconciliada pelo id, para que o foco do
teclado num canal não caia a cada tique.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-ab4ddb0a8ae76ba1a`
- Ramo: `0.16/lotes-c64`, aberto no `8cc38f6a` (o `0.16/lotes` com o lote AX).
- Brief: `.superpowers/sdd/lotes/lote-c64-brief.md`; a rodada 1,
  `.superpowers/sdd/lotes/lote-c64-rodada1.md`, com o veredito em
  `.superpowers/sdd/lotes/review-c64-report.md`.
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (dois commits só dele),
  `apps/seele-app/bancada/linhas-pelo-id.cjs` (os casos 10 e 11), `.github/workflows/ci.yml`
  (uma linha do comentário do passo da `linhas-pelo-id.cjs`), `apps/seele-app/ui/camada-moderar.js`
  e `apps/seele-app/tests/frontend.rs` (só comentário), `docs/pendencias.md` (a 64 fechada, uma
  linha na 63) e este relatório.
- Status: **DONE_WITH_CONCERNS** na primeira passada; a revisão aprovou com
  ressalvas (0 Críticos, 1 Importante, 3 Menores), e a rodada 1 trata os quatro.
  A preocupação que fica é a primeira de «Preocupações»: o lote precisou de um
  segundo conserto, fora da coluna de canais (o `emGrupos`).

Nada foi empurrado, nenhum subagente foi usado, o app não foi aberto, o
microfone não foi aberto, nada foi baixado, e o fio não mudou.

## Os commits

| commit | o que entra |
|---|---|
| `627666f8` | `emGrupos`: um id repetido num grupo do MOD não põe a mesma fileira duas vezes (só `tela-sessao.js`) |
| `ea11880e` | **o conserto**: a coluna de canais pelo `reconciliarALista`, com a chave `canal:<id>` e o `canal.item` pelo marcador (só `tela-sessao.js`) |
| `17953e77` | a bancada: o caso 10 da `linhas-pelo-id.cjs`, mais uma linha no comentário do passo dela no `ci.yml` |
| `365e8695` | só comentário: o RENOMEAR do canal em `camada-moderar.js` diz por que lê o retrato de agora |
| `fd09da1d` | a pendência 64 fechada, e uma linha na 63 |

Os da rodada 1 estão na seção «Rodada 1», no fim. Todas as mensagens estão em
português e terminam em `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**A ordem foi refeita uma vez, e só no meu ramo local.** O conserto e a bancada
nasceram como `09e3bbd4` e `0b35d75a`, direto sobre o `8cc38f6a`. A bateria
inteira achou a `regioes-do-produto.cjs` vermelha (ver «O segundo conserto»).
Então pus o conserto do `emGrupos` primeiro, num commit sobre o `8cc38f6a`, e
reapliquei os dois por cima com um `git rebase --onto` não interativo. Assim
nenhum commit do ramo fica com bancada vermelha. Os dois commits originais não
existem em ramo nenhum.

## O vermelho

O caso 10 da `linhas-pelo-id.cjs`, rodado com o `tela-sessao.js` do `8cc38f6a`
(sha256 `805977b9…`), antes do conserto:

```
AssertionError [ERR_ASSERTION]: o mesmo retrato, redesenhado pelo tique, tirou o foco do segundo canal:
o botão não é o mesmo nó, saiu do documento, e o foco está em BODY. Quem navega a coluna de canais pelo
teclado perde o lugar a cada meio segundo
saida=1
```

Uma bancada para no primeiro `assert`. Por isso medi os outros passos uma vez,
com as asserções desligadas por uma variável de ambiente. A instrumentação
ficou só nessa rodada: a bancada voltou à cópia limpa, e nada dela entrou em
commit. Resultados:

- **o mesmo retrato**: o tique redesenhou; o botão não é o mesmo nó e saiu do
  documento; o foco está em `BODY`; os botões e as fileiras dos 4 canais foram
  trocados;
- **outro canal muda de nome** (o foco foi posto de novo no botão do segundo
  canal que estava na tela): o mesmo, com o foco em `BODY`. As 2 mutações caem
  fora da fileira do renomeado, porque são a coluna inteira trocada;
- **um canal sai e outro entra**: o que sai perde o nó, mas os botões dos
  canais 1, 2 e 4 também foram trocados, e o foco caiu;
- **o `canal.item`**: 4 de 4 nós de MOD saíram do documento.

Isso confere com a medida da pendência (15/10), que viu o foco no `BODY` com o
mesmo retrato e com outro canal mudando de nome.

## O desenho

**O caminho do AX foi reaproveitado, e não há um segundo reconciliador.** As
fileiras continuam montadas como sempre, no `desenharCanais`. O que mudou:

- a fileira (`li`) ganha a chave `canal:<id>` em `chaveNaLista`, como a sala
  ganha `sala:<id>` na coluna de salas. É o id do canal, e não a posição;
- o `#lista-linhas` passa pelo `reconciliarALista`, e não pelo `repovoar`. Por
  dentro da fileira, o botão `.linha`, o `#`, o rótulo, o `div.linha-controles`,
  o `×` e o lápis casam pela posição e pela primeira classe (`mesmoFeitio`).
  Recebem os atributos e o texto novos pelo `vestirComo`. Quando o canal é
  aberto, `linha` vira `linha aberto` e o botão continua o mesmo nó, porque a
  primeira classe não muda; o `aria-current` entra e sai pelo `vestirComo`.
  Desde a rodada 1, o caso 10 (e) prende isso;
- **o ponto de MOD da fileira, o `canal.item`** (dentro do botão), passa pelo
  marcador: `nosDeMod(conteudoDasContribuicoes("canal.item", …))`, como os oito
  pontos do AX. Agora são nove chamadas de `noDeMod`/`nosDeMod`;
- **os ouvintes**: a fileira não tem ouvinte próprio. O abrir (`alternarCanal`),
  o renomear e o apagar já eram delegados no `#lista-linhas`. Eles leem o
  `data-linha`, o `data-renomear-linha` e o `data-apagar-linha` do botão, que a
  chave por id nunca troca, e o retrato de agora (`desenhado`, `lerSnapshot()`).
  Nenhuma closure prende o canal velho. Não há menu de contexto no canal: o
  único ouvinte de `contextmenu` é o global de `base.js`;
- **os cabeçalhos dos grupos** de `servidor.navegacao` (`li.canais-grupo`) não
  têm chave e casam pela posição, como na coluna de salas;
- **as chaves são esquecidas com a sessão** pelo mesmo `esquecerOEstadoDaSessao`
  do AX. A estrutura é a mesma, então nada foi acrescentado.

## O conserto

`ea11880e`, só no `tela-sessao.js`: são 3 linhas de código no `desenharCanais`
(a chave, o `nosDeMod` e a troca do `repovoar` pelo `reconciliarALista`), mais
os comentários e duas frases nos comentários de `chaveNaLista` e
`reconciliarALista`.

### O segundo conserto: `emGrupos` (`627666f8`)

**Como foi achado.** Com o conserto, a `regioes-do-produto.cjs` do job
`bancadas` reprovou:

```
servidor.navegacao: os canais saíram ["MESA 1","combinados","combinados","OUTROS CANAIS","geral"],
  e o esperado é ["MESA 1","combinados","OUTROS CANAIS","geral"]
servidor.navegacao: 3 de 2 canais na tela — alguém sumiu
```

**A causa.** A bancada dá aos canais `ids: [2, 2, 99]`. O comentário do
`emGrupos` diz que um id repetido é ignorado, mas o `filter` consultava
`usadas` antes de o laço a encher. Assim o repetido passava, e a **mesma**
fileira entrava duas vezes no vetor. O `repovoar` (`replaceChildren`) põe o
mesmo nó duas vezes como um nó só, e escondia isso. O `reconciliarALista` não:
a primeira ocorrência casa com o nó velho, e a segunda, com a chave já usada,
entra como nó novo. O resultado é uma fileira a mais.

**A coluna de salas já tinha isso desde o AX.** Medi por uma sonda
(`sonda-id-repetido.sh`): a `regioes-do-produto.cjs` com o arranjo de salas
trocado para `ids: [1, 1]` só durante a rodada. Os dois arquivos voltaram
idênticos pelo `cmp`. Resultados:

- `1bb6450a` (antes do AX): as salas saem `["MESA 1","Sala principal"]`, certo;
- `8cc38f6a`: saem `["MESA 1","Sala principal","Sala principal"]`, e «a sala
  perdeu a linha do produto» (duas `li.voice_room`);
- `8cc38f6a` com o conserto: `["MESA 1","Sala principal"]` de novo.

A única outra falha nas três rodadas é a contagem do DIAGNÓSTICO (4 e não 3).
Ela é da própria sonda, que acrescenta um id repetido, e a contagem está certa.

**O conserto** é uma linha: `[...new Set(ids.map(String))]` antes do `filter`.
Ele vale para as três listas que usam o `emGrupos`: salas, canais e pessoas.
Veio antes do conserto dos canais, para nenhum commit do ramo ter bancada
vermelha. Desde a rodada 1, o caso 11 da `linhas-pelo-id.cjs` o guarda nas duas
colunas.

## As reversões

Todas foram feitas pelo `reverter.sh`, na ponta `17953e77`. O roteiro de cada
uma: cópia do arquivo, mutante, `touch`, prova, devolução da cópia, `touch`,
`cmp` contra a cópia e sha256 antes e depois. **Todas reprovaram, e todas
voltaram idênticas**: o sha256 do `tela-sessao.js` foi
`7595a9e08e85fb6b3b1e6fb504e2c3d5c51598c738c4dcf47e7b77b749d421f6` antes e
depois de cada uma, e o `git status` ficou limpo.

| | o mutante | a prova | o que reprovou |
|---|---|---|---|
| R1 | o `tela-sessao.js` inteiro do `8cc38f6a` | `linhas-pelo-id.cjs` | caso 10: «o botão não é o mesmo nó, saiu do documento, e o foco está em BODY» |
| R2 | **a coluna de canais de volta ao `repovoar`** | `linhas-pelo-id.cjs` | caso 10: o mesmo |
| R3 | sem a chave `canal:<id>` | `linhas-pelo-id.cjs` | caso 10: «o terceiro canal saiu do retrato e a fileira dele ficou no documento» |
| R4 | o `canal.item` sem o marcador | `linhas-pelo-id.cjs` | caso 10, `canal.item`: «4 de 4 nós de MOD saíram do documento» |
| R5 | o `emGrupos` sem o `Set` | `regioes-do-produto.cjs` | «os canais saíram ["MESA 1","combinados","combinados",…]» e «3 de 2 canais na tela» |

**O R3 mostra onde mora o guarda da chave.** Sem a chave, as fileiras casam
pela posição. Com o mesmo retrato, ou com um nome mudando, os nós continuam os
mesmos, e as partes (a) e (b) do caso passam. Só a parte (c), em que um canal
sai do meio, pega a falta. Por isso ela tira o terceiro canal, e não o último.

Antes da troca de ordem, R1 a R4 também rodaram sobre o `0b35d75a`, com os
mesmos resultados e o mesmo retorno idêntico (sha256 `42adda5a…`).

Os R6 e R7 são da rodada 1.

## Os guardas, com os números

Na ponta do código, `17953e77`. Depois vieram só um comentário em
`camada-moderar.js` e os documentos. Depois do comentário, rodei de novo o
`--test frontend` e a `linhas-pelo-id.cjs`, na ponta `fd09da1d`.

**Rust.** Cada comando rodou pelo `cargo-casa-falsa.sh`, um script executável
que dá a cada comando uma HOME falsa nova no scratchpad e mantém o `rustup` e o
`cargo` de verdade. Ele confere que o log não ficou vazio e que a HOME falsa não
ganhou `.config/seele`: todas terminaram com 0 entradas. O `CARGO_TARGET_DIR`
estava desligado, e o target foi semeado como o brief manda.

- **`cargo test -p seele-app --test frontend`**: 296 de 296.
- **`cargo test -p xtask`**: 230 (88 + 4 + 121 + 1 + 3 + 10 + 1 + 2), com
  `toda_bancada_da_pasta_roda_em_algum_lugar`,
  `todo_passo_de_bancada_do_job_de_navegador_roda_depois_de_uma_vermelha_e_tem_prazo`
  e `todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`.
- **`cargo xtask check-runtime`**: passa, com as 12 bancadas de Node puro.

**Bancadas e roteiros.** O Playwright é o do `SEELE-MOD-PERFIS`, pelo
`PLAYWRIGHT=`.

- **As 19 bancadas do job `bancadas`**, cada uma por nome, como o `ci.yml` as
  escreve: 19 de 19 verdes.
- **A `linhas-pelo-id.cjs`**: os casos de 1 a 9 como antes, e o caso 10. Com
  outro canal mudando de nome houve 5 mutações, todas na fileira dele: o rótulo,
  mais o `aria-label` e o `title` do `×` e do lápis.
- **A `um-redesenho-por-quadro.cjs`**, com o atraso máximo antes e depois:

  | quando | antes | depois |
  |---|---|---|
  | rodadas soltas | 35, 29, 30 ms (`8cc38f6a`) | 32, 27, 28 ms (`0b35d75a`) |
  | intercalada | 32, 28, 32 ms (`8cc38f6a`) | 31, 30, 34 ms (`0b35d75a`) |
  | intercalada | 32, 29, 32 ms (`8cc38f6a`) | 30, 31, 32 ms (`17953e77`) |

  A carga de 1 min ficou entre 5,7 e 8,2. Não houve piora. **O retrato dessa
  bancada tem 2 canais**, então ela mede o caminho geral, e não uma coluna de
  canais grande. A coluna grande foi medida pela revisão (ver «Rodada 1»).
- **Os 40 roteiros**, numa chamada da `tools/carga-da-casca.py` com os
  `--roteiro` lidos do `ci.yml`: «os 40 passaram», «carga limpa: nenhum script
  estourou».
- **A bancada do celular inteira** (`tools/bancada-do-celular.py`): 110 casos
  passam, e o **G3 é IGUAL pixel a pixel no Chromium e no WebKit**. Levou 9 min
  25 s com a máquina carregada.
- **Os cartões de MOD**: o caso 5 da `linhas-pelo-id.cjs` (os oito pontos da
  lista de pessoas), o caso 10 (o `canal.item`), a `regioes-do-produto.cjs`, a
  `diagnostico-de-mods.cjs` (que contorna o `canal.item` no
  `#lista-linhas button[data-linha]`), a `avatares-do-mod.cjs`, a `capa.cjs`, a
  `camadas.cjs`, a `palco-da-chamada.cjs` e a `regiao-do-mod.cjs` (no
  `check-runtime`): todas verdes.

**O que não rodou, como o brief manda:** as 3 bancadas de wasm, o `cargo xtask
web`, o `cargo test --workspace`, a conformidade e a sonda `arvore-ax.py`.

Os brutos ficaram no scratchpad desta sessão, em `c64/`, com os scripts que os
geraram (`bancada.sh`, `reverter.sh`, `mutar.py`, `intercalar.sh`,
`sonda-id-repetido.sh`, `bancadas-do-ci.sh`, `roteiros.sh` e
`cargo-casa-falsa.sh`). O scratchpad não fica: os números estão aqui e nas
mensagens dos commits.

## O que ficou de fora

- **A árvore de acessibilidade de um WebView de verdade.** A bancada prende o
  foco e a identidade do nó no Chromium, e é isso que a 64 mede. A sonda
  `arvore-ax.py` não foi pedida, e o app não foi aberto.
- **O celular na coluna de canais**: ela só ouve cliques, delegados, e não
  adapta as fileiras. A bancada do celular passou inteira, mas não há caso de
  foco nela.
- **O `reconciliarOsFilhos` não tolera o mesmo nó duas vezes** (da rodada 1:
  ver lá).
- **O `roster-mods`, o `pessoas-conta`, a conversa e a telemetria** seguem
  repovoados a cada retrato (pendência 63, sem dona). O celular da coluna de
  salas segue com a F8.
- ~~Um guarda permanente para o id repetido na coluna de salas~~: o caso 11,
  na rodada 1.
- ~~Um comentário velho em `apps/seele-app/tests/frontend.rs`, perto da linha
  557~~: corrigido na rodada 1 (`d5246bc7`).

## Preocupações

1. **O lote passou do pedido em um commit.** O `627666f8` mexe no `emGrupos`,
   que não é a coluna de canais. Sem ele, a `regioes-do-produto.cjs` do CI
   reprova com o conserto da 64. Ele também conserta uma regressão que o AX
   pôs na coluna de salas (`8cc38f6a`): um id de sala repetido no arranjo de
   `servidor.navegacao` duplicava a fileira da sala, e com ela o ENTRAR NA
   SALA. Só se vê com um MOD que repita um id. É uma linha, só no
   `tela-sessao.js`, em commit isolado, e as reversões R5 e R7 a provam.
2. **A ordem dos commits foi refeita no meu ramo local**, para nenhum commit
   ficar vermelho. Os SHAs que eu tinha antes (`09e3bbd4`, `0b35d75a`) não
   valem mais.
3. **A carga da máquina variou de 5,4 a 10,0** durante as medidas, com outras
   sessões rodando. Os atrasos da `um-redesenho-por-quadro.cjs` estão dentro do
   ruído das rodadas intercaladas.

## Rodada 1

A revisão (`.superpowers/sdd/lotes/review-c64-report.md`) achou o lote conforme
e o aprovou com ressalvas: 0 Críticos, 1 Importante, 3 Menores. O pedido da
rodada (`.superpowers/sdd/lotes/lote-c64-rodada1.md`) mandou I1, M1, M3 e M2,
nessa ordem. Ponta de partida `fd09da1d`. **A rodada não muda código de
produção**: entre o `fd09da1d` e a ponta da rodada, o `git diff` de
`apps/seele-app/ui/` é vazio, e o `tela-sessao.js` segue com o sha256
`7595a9e0…21f6`.

### Os commits

| commit | achado | o que entra |
|---|---|---|
| `ffba93b5` | I1 | a bancada, caso 10 (e): o canal com o foco vira o aberto |
| `b11d1464` | M1 | a bancada, caso 11: um id repetido no arranjo do MOD, nas salas e nos canais |
| `d5246bc7` | M3 | só comentário, na nota do `renomear_linha` em `tests/frontend.rs` |
| este | M2 | a 64 com os 2 canais, a medida dos 60 e o «Fica de fora», e este relatório no repositório |

### I1 — o canal que vira o aberto, com o foco nele

- **O achado:** é o Enter de quem navega pela coluna pelo teclado. O
  `alternarCanal` pede o retrato, ele vem com o canal aberto, e o botão passa
  de `linha` a `linha aberto`. O nó só sobrevive porque o `mesmoFeitio` compara
  a primeira classe. A revisão mostrou que o mutante
  `linha.open ? "aberto linha" : "linha"` tirava o foco para o `BODY`, e a
  bancada passava verde.
- **O passo (e)** do caso 10: o foco vai ao botão do canal 2; o retrato passa a
  ter o 2 `open` e o 1 não; espera-se o tique. O botão do 2 é `===` ao de antes,
  está `isConnected`, é o `document.activeElement` e tem a classe `linha aberto`
  e `aria-current="true"`. O do 1 é o mesmo nó, com a classe `linha` e sem
  `aria-current`.
- **A reversão (R6):** o mutante da revisão, sobre o `tela-sessao.js` da ponta,
  reprova: «o segundo canal, com o foco, virou o aberto e o foco saiu dele: o
  botão não é o mesmo nó, saiu do documento, e o foco está em BODY». O arquivo
  voltou idêntico pelo `cmp` (sha256 `7595a9e0…` antes e depois).

### M1 — o id repetido nas salas, guardado

- **O caso 11**, na `linhas-pelo-id.cjs`, sem tocar na `regioes-do-produto.cjs`
  da F3. Um MOD, pelo `registrarMod` da bancada, com `servidor.navegacao` em
  `substituir`:
  - `salas: { grupos: [{ titulo: "MESA", ids: [1, 1] }] }`;
  - `canais: { grupos: [{ titulo: "MESA", ids: [2, 2] }] }`.

  São dois retratos e um tique, porque a duplicata só nasce quando já há nó
  velho. O caso exige:
  - que o arranjo tenha chegado, com o título MESA nas duas colunas, senão o
    caso não mediria nada;
  - um `li.voice_room` e um `button[data-voice_room]` por sala e um
    `button[data-linha]` por canal, depois dos retratos e depois do tique;
  - o ENTRAR da sala 1 `===` através do tique.
- **A reversão (R7):** o `emGrupos` sem o `Set` reprova **nas salas**: «um id de
  sala repetido no arranjo do MOD duplicou a fileira da sala: depois de dois
  retratos, 3 li.voice_room e 3 ENTRAR para 2 salas». Também confirmei que a
  asserção dos canais funciona: desliguei a das salas numa rodada só (a bancada
  voltou idêntica pelo `cmp`), e a dos canais reprovou, com «3 botões para 2
  canais». O `tela-sessao.js` voltou idêntico nas duas.
- **O opcional da revisão** (pular um `novo` repetido em `reconciliarOsFilhos`)
  não entrou, por decisão da orquestradora. A causa estava no `emGrupos`, que
  está consertado e agora guardado, e um `Set` a mais por quadro também é
  alocação. Ele está escrito no «Fica de fora» da 64: o mesmo nó posto duas
  vezes por um chamador futuro, ou um id duplicado no próprio retrato, que
  exigiria um servidor repetindo id.

### M3 — a nota do `frontend.rs`

Só comentário, em `apps/seele-app/tests/frontend.rs`. O `git diff` não tem
nenhuma linha fora de comentário. A nota do `renomear_linha` dizia que toda
fileira de `#lista-linhas` é refeita duas vezes por segundo e que isso
«continua verdade». Agora ela diz:

- que **era** verdade então, e que por isso a decisão foi pôr o controle na
  barra do canal aberto;
- que depois o lápis também foi para a fileira (`botaoDeRenomearLinha`), com o
  mesmo diálogo (`abrirNomear`, «RENOMEAR O CANAL», nos dois caminhos);
- que a premissa deixou de ser verdade na 0.16 (pendência 64, lote C64), com a
  coluna reconciliada pelo id.

### M2 — os documentos

- **Na 64 fechada**, no «não piorou»: a `um-redesenho-por-quadro.cjs` tem 2
  canais, e entra a medida da revisão com 60 canais, com os números do veredito
  como estão.
- **Um parágrafo «Fica de fora» na 64**, com três itens: a árvore de
  acessibilidade num WebView de verdade, que não foi medida; a bancada do
  celular, que não tem caso de foco na coluna de canais; e o opcional do
  `reconciliarOsFilhos`. Os commits da rodada também entram, e a linha da 63
  aponta para o caso 11.
- **Este relatório**, no repositório, como o do AX. O de antes estava só no
  scratchpad.

**A medida da revisão, com 60 canais**: o `desenharCanais`, mediana de 300
chamadas, 3 rodadas intercaladas, com a carga entre 11 e 17. **Não a refiz.**

| condição | `8cc38f6a` | ponta |
|---|---|---|
| só o JS, com a sessão escondida | 0,5 ms | 1,6–1,7 ms |
| com o estilo e o layout forçados, na sessão visível | 1,5 ms | 1,4–1,5 ms |
| o p90 dessa medida, como o veredito o escreve | 1,9–2,0 | contra 1,7–1,8 |

A conclusão da revisão: custa mais JS e menos layout, e o quadro não piora.

### O que rodou na rodada

Na ponta do código da rodada, `d5246bc7`. Os documentos vieram depois.

- **A `linhas-pelo-id.cjs`**, casos 1 a 11: saída 0.
- **A `um-redesenho-por-quadro.cjs`:**
  - 36 ms de atraso máximo na passada da bateria;
  - 32, 37 e 30 ms em três rodadas soltas, com a carga entre 6,5 e 7,6;
  - intercalada com o `8cc38f6a`: 30, 31 e 32 ms nele e 31, 32 e 31 ms na
    ponta, com a carga entre 7,4 e 9,9.

  O `tela-sessao.js` é o mesmo da passada anterior, e a variação é ruído.
- **As 19 bancadas do job `bancadas`**: 19 de 19.
- **Os 40 roteiros**: «os 40 passaram», carga limpa.
- **`cargo test -p seele-app --test frontend`** com HOME falsa, depois do M3:
  296 de 296.
- **`cargo test -p xtask`** com HOME falsa: 230.
- **`cargo xtask check-runtime`** com HOME falsa: passa, com 12 bancadas.

Os logs não ficaram vazios, e as HOMEs falsas terminaram sem entradas.

- **A bancada do celular não rodou nesta rodada.** O pedido a dispensa porque
  a rodada não muda código de produção, e não muda: o diff de
  `apps/seele-app/ui/` desde o `fd09da1d` é vazio. A última passada dela é a da
  rodada 0: 110 casos, G3 IGUAL no Chromium e no WebKit.
