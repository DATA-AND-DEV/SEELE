# Relatório do lote AX (F5, 0.16)

A parte (b) da pendência 63, de acessibilidade: a lista de pessoas — e a coluna
de salas, onde mora o ENTRAR NA SALA — reconciliadas pelo id, e não repovoadas a
cada retrato.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a56335a0cd596d31e`
- Branch: `0.16/lotes-ax`, aberto em `1bb6450a`.
- Brief: `.superpowers/sdd/lotes/lote-ax-brief.md`, com o arranjo da sonda que a
  orquestradora mandou (o `arvore-ax.py` do `3436980d`, a pasta só minha, a
  8393, a casa descartável, as conferências antes de cada rodada).
- Arquivos mudados: `apps/seele-app/ui/tela-sessao.js` (o conserto, em dois
  commits só dele, e mais quatro na rodada 1), `apps/seele-app/bancada/linhas-pelo-id.cjs`
  (nova), `.github/workflows/ci.yml` (a bancada nova no job de navegador),
  `apps/seele-app/ui/camada-moderar.js` (só comentário, na rodada 1), e os
  documentos: este relatório e as pendências 63 e 64.
- Status (rodada 1): **DONE_WITH_CONCERNS**. O verde passa com 10, 50 e 100 pessoas em
  toda consulta; as preocupações estão no fim, e nenhuma bloqueia.

## Os commits

| commit | o que entra |
|---|---|
| `c5691d78` | o conserto da lista de pessoas: `reconciliarALista`, as chaves, os marcadores de MOD, o esquecimento das chaves com a sessão (só `tela-sessao.js`) |
| `22a66313` | a bancada `linhas-pelo-id.cjs`, casos 1 a 5, e o passo dela no `ci.yml` |
| `d44f7c1f` | o conserto da coluna de salas: a mesma reconciliação, a sala e a pessoa como chaves, o RENOMEAR com o nome de agora (só `tela-sessao.js`) |
| `bede50d9` | a bancada, caso 6: a coluna de salas e o ENTRAR NA SALA |
| `c0d26666` | a bancada, caso 7: a saída da sala na coluna e o nome de agora no RENOMEAR |
| `30c38dbe` | o relatório e a pendência 63, (b) |

Os da rodada 1 estão na seção «Rodada 1», no fim.

Cada mensagem é em português e termina em `Co-Authored-By: Claude Opus 5.5
<noreply@anthropic.com>`. Nada foi empurrado; o fio não mudou.

## A sonda, antes e depois

**O arranjo.** A sonda é o `docs/evidencias/m3-custo-dos-mods/arvore-ax.py` do
`3436980d` (sha256 `5a7811d8…ba8`), com o `m3.py` do mesmo commit. A pasta da
campanha é só minha, no scratchpad desta sessão, montada como o
`preparar-api6.sh` monta, **só com o que a sonda usa**: o fonte do `1bb6450a` por
`git archive`; o `m3-servidor` e o `m3-falante` em release, com os remendos 3 e
4 do `remendar-api6.py` da base (o `api` do `EnabledMod` e o número da voz); o
`ax` e o `sessao` do `ax.swift` e do `sessao.swift`. Nenhum pacote oficial foi
baixado, e o app não levou os remendos 1 e 2 (a API 6 e o jitter no log): o
vermelho e o verde diferem só no conserto.

- **app vermelho**: o `cargo tauri build --bundles app` do `1bb6450a` como está —
  `seele-app` sha256 `0964f3ec…8459`;
- **app verde**: o mesmo fonte com o `tela-sessao.js` do `d44f7c1f` (sha256
  `4a1fc947…f774`), que é o da ponta do ramo — `seele-app` sha256 `6158440a…0c24`.

Segundo as minhas notas da primeira campanha, o vermelho e o verde de 11/10
tinham estes mesmos sha256. Os hashes de 11/10 não ficaram em artefato nenhum
(o scratchpad se perdeu, e a mensagem do `c5691d78` não os traz), então isso
não se confere; e o verde medido em 11/10 era o verde-1, outra build (abaixo).

Cada rodada é um N só, pelo condutor `sondar.sh`, que antes dela confere o Mac
desbloqueado pelo `sessao` e nenhuma SEELE aberta pelo `pgrep -fl SEELE.app`,
anota a carga da máquina, e roda debaixo de `caffeinate -di`. O app abre com a
`SEELE_HOME` descartável da sonda (`<saida>/<N-hora>/casa`), na 8393.

**A campanha de 15/10**, vermelho e verde intercalados (outras sessões
compilavam e rodavam Chromium na máquina; a carga média de 1 min ficou entre
4,9 e 12,7):

| hora | app | pessoas | a lista inteira | nomes achados | ENTRAR NA SALA | entrou | CPU do WebKit | consulta mais lenta (s) | assenta |
|---|---|---|---|---|---|---|---|---|---|
| 11:12 | vermelho | 10 | 38 de 38 | 9 | 38 de 38 | sim | 7,0% | 0,3 | sim |
| 11:15 | verde | 10 | 35 de 35 | 9 | 35 de 35 | sim | 6,9% | 4,3 | sim |
| 11:18 | vermelho | 50 | **35 de 37** | 0, 39, 49 | 37 de 37 | sim | 18,2% | 0,2 | **não** |
| 11:21 | verde | 50 | 37 de 37 | 49 | 37 de 37 | sim | 16,4% | 0,3 | sim |
| 11:25 | vermelho | 100 | **29 de 36** | 7 a 99, oito valores | 36 de 36 | sim | 30,9% | 0,3 | **não** |
| 11:30 | verde | 100 | 36 de 36 | 99 | 36 de 36 | sim | 28,2% | 0,7 | sim |
| 11:35 | vermelho | 10 | 38 de 38 | 9 | 38 de 38 | sim | 7,0% | 0,3 | sim |
| 11:38 | verde | 100 | 36 de 36 | 99 | 36 de 36 | sim | 32,5% | 0,5 | sim |
| 11:44 | vermelho | 100 | **35 de 36** | 57, 99 | 36 de 36 | sim | 30,8% | 1,7 | **não** |
| 11:50 | verde | 50 | 37 de 37 | 49 | 37 de 37 | sim | 18,4% | 0,3 | sim |

**O verde assenta nas cinco rodadas, com 10, 50 e 100 pessoas, em toda
consulta**: a lista inteira, o ENTRAR NA SALA, e o aperto entrando. O vermelho
reprova com 50 (numa consulta, nenhum nome: a lista inteira sumiu) e com 100 nas
duas; com 10 ele assentou nas duas vezes — a perda com 10 é rara, como a F3
escreveu na sonda.

**A primeira campanha** (noite de 11 para 12/10, com o mesmo app vermelho e um
verde-1 com só a lista reconciliada — `seele-app` sha256 `6e2555a8…5b5`, de um
`tela-sessao.js` que difere do `c5691d78` só no nome da função e no esquecimento
das chaves ao sair), cujos arquivos se perderam — o scratchpad foi
apagado quando a sessão parou; os números ficaram na mensagem do `c5691d78` e
nas minhas notas:

| app | 10 | 50 | 100 |
|---|---|---|---|
| vermelho | 36 de 39 (nomes 1, 3, 4, 9) | 19 de 38 (de 3 a 49) | 0 de 38 (de 0 a 88) |
| verde-1 (só a lista) | 38 de 38 | 38 de 38 | 37 de 37, **e o ENTRAR NA SALA em 34 de 37** |

Segundo as notas, a CPU do WebKit, nela, foi de 32,4% (vermelho) para 27,5%
(verde) com 50 e de 52,0% para 44,2% com 100. **Hoje o vermelho reprova bem
menos que em 11/10** (com 100, 29 e 35 de 36 contra 0 de 38), e a CPU do WebKit
no vermelho medida hoje é 31%. **A hipótese** é que algo na máquina mudou entre
as duas noites, já que o fonte do app, o servidor e o falante são os mesmos —
mas não medi, e nem a CPU de 11/10 tem bruto. A conclusão não depende disso:
nas duas campanhas o vermelho reprova com 50 e com 100, e o verde nunca perdeu a
lista.

**O ENTRAR NA SALA do verde-1.** Com só a lista reconciliada, o botão saiu da
árvore em 3 de 37 consultas com 100 pessoas (em t = 13, 49 e 101 s, com a lista
inteira em todas). Ele mora na coluna de salas (`desenharCanais`), que também
lista quem está em cada sala e era repovoada a cada quadro, pelo mesmo motivo.
A parte (b) da pendência nomeia o ENTRAR NA SALA saindo da árvore, e a sonda o
exige: o `d44f7c1f` estende a reconciliação a essa coluna, e com ele o botão
ficou em 36 de 36 nas duas rodadas de 100.

## O desenho da reconciliação

`reconciliarALista(lista, filhos)`, em `tela-sessao.js`. As linhas continuam
montadas como sempre, por `linhaDoRoster` e `grupoDeSala` (e, na coluna, pelo
`desenharCanais`); a função faz a lista na tela ficar igual à montada mexendo só
no que mudou.

- **A chave é a pessoa, e não o lugar.** `chaveNaLista` (um `WeakMap`, e não um
  atributo: a chave é de quem reconcilia) dá `pessoa:<id>` à linha, `sala:<id>`
  ao grupo de uma sala e `grupo:<nome>` a FORA DE SALA e ao saguão. Antes de
  reconciliar, as linhas velhas com chave são achadas em toda a lista: quem
  entra numa sala leva o próprio nó para o grupo dela, e não ganha outro.
- **Dentro de uma linha, pela posição.** Um filho sem chave casa com o próximo
  filho velho da mesma etiqueta e da mesma **primeira** classe (desde a rodada 1,
  M2; antes era a classe inteira), e recebe os atributos, o texto e o valor do
  novo. O valor de quem está sob o dedo (`document.activeElement`) não é
  trocado, como em `pintarVolumesDoRoster`; o `open` de um `<details>` só segue
  a linha montada quando o que ela pede muda (rodada 1, I1).
- **A ordem.** Fica no lugar a maior subsequência crescente das posições velhas,
  e só o resto se move (rodada 1, M1; antes, o cursor movia todos os irmãos do
  meio quando uma linha ia para trás). Um nó velho com chave que não serve ao
  grupo de agora espera o fim, porque um grupo seguinte pode querê-lo; o que
  ninguém quis sai no fim.
- **Os nós de MOD não saem do documento.** Os cartões e as contribuições chegam
  montados e são o mesmo nó entre um retrato e o seguinte. Anexá-los à linha
  montada os tiraria da lista para voltar — para a árvore de acessibilidade,
  outro nó, e o som e a imagem recomeçando. Por isso a linha montada leva um
  **marcador** (`noDeMod`, um comentário vazio) no lugar de cada um, e a
  reconciliação põe o nó onde o marcador está, sem entrar nele. São oito pontos
  de montagem: os três `cartoes` de `desenharPessoas`, o cartão de quem apresenta
  em `linhaDoRoster`, `pessoa.cartao`, `pessoa.detalhes`, `pessoa.identidade` e
  `sala.acoes`. A linha `for (const cartao of pessoa.cartoes ?? []) item.append(cartao);`,
  que um guarda do `frontend.rs` exige verbatim, ficou como estava: o marcador
  entra antes, em `desenharPessoas`.
- **As chaves são esquecidas com a sessão.** O guarda
  `todo_estado_de_topo_da_sessao_e_esquecido_ao_sair` pegou o primeiro rascunho, e
  com razão: a pessoa 3 de um servidor não é a do seguinte. As três estruturas
  são refeitas em `esquecerOEstadoDaSessao`, e a primeira lista da sessão nasce
  com nós novos.
- **A coluna de salas.** Mesma função. O nome da sala é um botão de renomear com
  ouvinte próprio, e agora ele atravessa os retratos: o ouvinte lê o nome de
  agora do último retrato desenhado (`desenhado`), e não do quadro em que nasceu.
  A casca do celular (`celular-servidor.js`) adapta essa coluna depois de cada
  quadro contando com ela refeita — troca o nome por texto, embrulha o ENTRAR e
  acrescenta o ENTRAR MUDO. O que ela acrescenta não está na lista montada: sai
  na reconciliação seguinte e ela o põe de novo, como fazia antes.

O juntar em um `atualizar()` por quadro (`6717dc2c`) não mudou: o PersonState
continua marcando o quadro, e o quadro pede o retrato.

## O teste de identidade

`apps/seele-app/bancada/linhas-pelo-id.cjs`, no job `bancadas` do `ci.yml`
depois da `um-redesenho-por-quadro.cjs`, com `if: ${{ !cancelled() }}` e
`timeout-minutes: 5`. O HTML, o CSS e o registro reais, só a ponte simulada,
50 pessoas (20 na sala principal, 30 fora das salas). Cada caso guarda as
referências dos nós e confere com `===`, e um `MutationObserver` na lista diz
onde cada mutação caiu.

1. Um PersonState (falar, sinal 61) pelo caminho do evento: toda mutação cai
   dentro da linha da pessoa; as 50 linhas são os mesmos nós; a linha diz
   TRANSMITINDO e 61; o foco no nome de outra pessoa fica onde estava.
2. O mesmo retrato, redesenhado pelo tique de 500 ms por 1,2 s: nenhuma mutação.
3. Alguém de fora das salas entra na sala dois: a linha dela é o mesmo nó, agora
   no grupo da sala; as outras e os grupos que existiam ficam.
4. Alguém sai do servidor (a linha sai) e alguém chega (ganha uma); as outras
   ficam.
5. Com um cartão de MOD (`pessoa.cartao`, substituir) em cada linha: um
   PersonState continua só na linha; nenhum cartão troca de nó ou sai do
   documento.
6. Na coluna de salas: um PersonState só muta dentro da linha da pessoa nela; o
   ENTRAR NA SALA da sala dois é o mesmo nó; o mesmo retrato não muta nada.
7. Ainda nela: quem sai da sala tira só a linha dela; o RENOMEAR de uma sala
   renomeada abre com o nome de agora.

Antes do conserto, reprovou no caso 1 («1 de 1 mutações»: a lista inteira num
registro só), e o caso 6, antes do `d44f7c1f`, do mesmo jeito.

## As reversões

Pelo `reverter.sh`: cópia do arquivo, mutante, toque, prova, devolução, toque,
`cmp` contra a cópia. Sobre a ponta (`c0d26666`). Todas reprovaram, cada uma no
seu caso, e **todas voltaram idênticas**: o sha256 do `tela-sessao.js` antes e
depois da série é o mesmo, `4a1fc947…f774`, e o `git status` ficou limpo.

| | o mutante | a prova | o que reprovou |
|---|---|---|---|
| R1 | o `tela-sessao.js` inteiro do `1bb6450a` | bancada | caso 1: «1 de 1 mutações, e 50 linhas trocaram de nó» |
| R2 | `noDeMod` devolve o nó, e não o marcador | bancada | caso 5: «98 de 103 mutações» fora da linha, com cartões |
| R3 | sem a chave da linha | bancada | caso 1: «80 de 80 mutações, e 14 linhas trocaram de nó» |
| R4 | sem a chave do grupo | bancada | caso 3: «um grupo que já existia trocou de nó» |
| R5 | a coluna de salas de volta ao `repovoar` | bancada | caso 6: «1 de 1 mutações» na coluna |
| R6 | sem a chave da pessoa na coluna | bancada | caso 7: «5 outras linhas da coluna de salas trocaram de nó» |
| R7 | o RENOMEAR com o nome do quadro em que o botão nasceu | bancada | caso 7: «o RENOMEAR abriu com "Sala dois"» |
| R8 | sem esquecer as chaves na saída | `--test frontend`, HOME falsa | `todo_estado_de_topo_da_sessao_e_esquecido_ao_sair`: 295 de 296 |

Na primeira passada, R1 a R4 rodaram também sobre o `c5691d78`, com os mesmos
resultados e o mesmo retorno idêntico.

## O que rodou

Na ponta `c0d26666`, em 15/10, com `CARGO_TARGET_DIR` desligado e os testes de
Rust por um script executável que dá a cada comando uma HOME falsa nova, confere
que o log não ficou vazio e que a HOME falsa não ganhou `.config/seele` (todas
terminaram vazias). A `~/.config/seele` de verdade não foi lida nem tocada; o
app da sonda só abriu com a `SEELE_HOME` descartável dela.

- **`cargo test -p seele-app --test frontend`**: 296 de 296 (de novo depois dos
  documentos: 296);
- **`cargo test -p xtask`**: 230, com `toda_bancada_da_pasta_roda_em_algum_lugar`,
  `todo_passo_de_bancada_do_job_de_navegador_roda_depois_de_uma_vermelha_e_tem_prazo`
  e `todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`;
- **`cargo xtask check-runtime`**: passa (as bancadas de MOD de Node puro);
- **as 22 bancadas de navegador do CI**, cada uma por nome, como o `ci.yml` as
  escreve: as 19 do job `bancadas` (as 18 de antes e a `linhas-pelo-id.cjs`) e as
  3 do `bancadas-de-mod`, com o `.wasm` do `cargo xtask web`. Todas verdes. A
  `um-redesenho-por-quadro.cjs`: 251 eventos num quadro pediram um retrato, e
  1004 eventos em 4 s com 251 cartões tiveram o maior atraso em 29 ms (27 a 30 ms
  nas três rodadas de 11/10 com o conserto da lista);
- **os 40 roteiros**, todos numa chamada da `tools/carga-da-casca.py`, como no
  CI: «os 40 passaram», carga limpa;
- **a bancada do celular inteira** (`tools/bancada-do-celular.py`): 110 casos
  passam, e o **G3 é igual pixel a pixel no Chromium e no WebKit**;
- **a sonda**, acima.

O Playwright das bancadas de Node é o do `SEELE-MOD-PERFIS`, pelo `PLAYWRIGHT=`
que a `playwright.cjs` prevê; o do Python e o `chrome-headless-shell` são os do
cache da máquina.

## O que ficou de fora

- **A coluna de canais (`lista-linhas`)** segue repovoada a cada retrato, e o
  foco do teclado num canal cai a cada tique: é a **pendência 64**, da F5, um
  lote à parte depois deste.
- **O `roster-mods`, o `pessoas-conta` do cabeçalho, a conversa e a telemetria**
  seguem repovoados a cada retrato, sem dona por ora. A sonda não os mede, e esta
  parte da pendência é a lista e o ENTRAR NA SALA.
- **No celular, a coluna de salas ainda gira: a (b) não está resolvida lá.**
  `celular-servidor.js` adapta a coluna depois de cada quadro contando com ela
  refeita, e a reconciliação tira o que ele pôs para ele pôr de novo: o ENTRAR e
  o ENTRAR MUDO de lá continuam trocados a cada quadro, como antes. A aba PESSOAS
  do celular usa a `#lista-roster` do desktop e só ouve cliques, e ganha o
  conserto inteiro. A dona é a F8.
- **O que um VoiceOver de verdade faz com a árvore** — a sonda mede se a árvore
  tem a lista quando é consultada, como ela mesma diz.
- **A parte do servidor da pendência 63** (um PersonState de cada pessoa a todas
  as outras, por segundo) é outra medida, e não é deste lote.
- **Os arquivos brutos da primeira campanha** se perderam com o scratchpad; os
  números dela estão acima e na mensagem do `c5691d78`. Os da segunda estão no
  scratchpad desta sessão (`ax/sonda-vermelho`, `ax/sonda-verde`).

## Preocupações

1. **O vermelho de hoje é mais brando que o de 11/10**, do mesmo fonte: com 100,
   29 e 35 de 36 contra 0 de 38, e a CPU do WebKit em 31% contra 52% segundo as
   notas. Não medi a causa. O verde passou nas duas campanhas; a de hoje é a
   que tem o conserto inteiro.
2. **Duas interrupções que não vieram do produto.** (a) Na primeira campanha, a
   segunda rodada verde não começou: a F3 abriu a SEELE dela (`/tmp/m3-6`) na
   8393, e o condutor parou, como a regra pede. (b) Hoje, na rodada vermelha de
   100 que começou às 11:43:28 (`interrompida-100-114328`), algo de fora terminou
   com SIGTERM os meus processos de fundo, o servidor e o falante (o último
   registro do servidor é das 11:44:11); a sonda ficou esperando os calados, o
   app não chegou a abrir, e eu a encerrei. A refeita é a `100-114455`, que
   começou às 11:44:55 (a linha 11:44 da tabela).
3. **Um falso positivo na conferência de outra SEELE.** O `pgrep -fl SEELE.app`
   casou com a linha de comando de um laço meu de espera, que tinha esse nome no
   texto; o condutor parou, como devia, e passou a contar só o executável
   (`/Contents/MacOS/seele-app`).
4. **A consulta mais lenta do verde com 10 levou 4,3 s**, uma vez, com a carga da
   máquina em 7,6. As outras ficaram em 0,7 s ou menos.

## Rodada 1

A revisão (`.superpowers/sdd/lotes/review-ax-report.md`) aprovou com ressalvas:
0 Críticos, 2 Importantes, 4 Menores. O pedido da rodada
(`.superpowers/sdd/lotes/lote-ax-rodada1.md`) mandou, na ordem, I2, I1, M2, M1,
M4, M3, e a sonda de novo; o acréscimo da orquestradora mandou escrever a
pendência 64 e corrigir os comentários do `camada-moderar.js`. Ponta de partida
`30c38dbe`.

### Os commits

| commit | achado | o que entra |
|---|---|---|
| `68c4544d` | I2 | a bancada guarda cada ponto de montagem de MOD, prova que o tique redesenhou e confere a ordem |
| `64858020` | I1 | o `open` de um `<details>` reaproveitado só segue a linha montada quando o pedido muda (só `tela-sessao.js`) |
| `bcff658a` | I1 | a bancada, caso 9 |
| `607b85f5` | M2 | `mesmoFeitio` pela etiqueta e pela primeira classe (só `tela-sessao.js`) |
| `c75b6008` | M2 | a bancada, caso 5: `pessoa.identidade` numa linha apresentada, com a pessoa começando a falar |
| `ee24b4f3` | M1 | a ordem pela maior subsequência crescente (só `tela-sessao.js`) |
| `b0f828ad` | M1 | a bancada, caso 8: um nó inserido e o foco no meio fica |
| `fea04116` | M4 | o acoplamento com o celular escrito no `vestirComo` (só comentário, só `tela-sessao.js`) |
| `ec8ec2ce` | B | os comentários das duas delegações do `camada-moderar.js` (só comentário) |
| este | M3, A | o relatório e as pendências 63 e 64 |

`celular-servidor.js` e `celular-pessoas.js` não foram tocados: são da F8.

### I2 — a bancada mais forte, antes dos consertos

O caso 5 passou a rodar um ponto de montagem por vez, cada um numa página nova,
com quem usa fora das salas (para os três ramos de `cartoes` montarem linhas) e
as duas salas com gente: `pessoa.cartao` substituir com conteúdo; o mesmo sem
conteúdo, com o cartão de uma região da API 3 simulada (`cartoesDosMods`, o
caminho do PERFIS: `cartaoDeContribuicao` → `regiao.cartaoDe(id)`); os cartões
da API 3 na linha nativa; `pessoa.cartao` adicionar; `pessoa.identidade`;
`pessoa.detalhes`; `sala.acoes`. Em cada um: com um PersonState, nenhum nó de
MOD aparece nos `removedNodes` do observador (nem dentro de um nó removido),
cada um é o mesmo nó na mesma linha, e nenhuma mutação cai fora da linha. O caso
2 exige que `desenhado` seja outro objeto depois de 1,2 s; o caso 8, a ordem da
lista e da coluna igual à do retrato, também depois de uma reordenação.

**Os mutantes da revisão, agora vermelhos** (na ponta `c0d26666`, cada um
voltando idêntico pelo `cmp`; o sha256 do `tela-sessao.js` antes e depois da
série é `4a1fc947…`):

| mutante | o marcador tirado | o que reprovou |
|---|---|---|
| M1 | `pessoa.identidade` | «50 de 50 nós de MOD saíram do documento» |
| M2 (sala) | os `cartoes` da API 3, ramo das salas | «24 de 50» |
| M2 (fora) | os `cartoes`, ramo FORA DE SALA | «1 de 50» |
| M2 (saguão) | os `cartoes`, ramo do saguão | «25 de 50» |
| M3 | `sala.acoes` | «2 de 2» |
| M4 | `pessoa.cartao` adicionar | «50 de 50» |
| — | `pessoa.detalhes` | «50 de 50» |
| — | o cartão de quem apresenta (`noDeMod(cartao)`) | «50 de 50» |
| — | o tique parado (`if (false) atualizar()`) | caso 2: «o tique não redesenhou em 1,2 s» |
| — | a ordem que não move nó já presente | caso 8: «a ordem da lista não é a do retrato» |

Os oito marcadores são as oito chamadas de `noDeMod`/`nosDeMod` do
`tela-sessao.js`.

### I1 — o `<details>` que a pessoa abre continua aberto

- **Vermelho** (caso 9, antes do `64858020`): «o «⋯» de p-003, aberto pela
  pessoa, fechou no tique com o mesmo retrato (1 mutações na linha)».
- **Conserto:** `abertoPedido` (um `WeakMap`) guarda o `open` que a linha montada
  pediu a cada `<details>` no quadro anterior; o `vestirComo` só aplica o `open`
  da linha montada quando esse pedido muda, e fora disso fica o que a pessoa
  escolheu. A exceção é só o `open` de `details`. O pedido é esquecido com a
  sessão (o guarda `todo_estado_de_topo_da_sessao_e_esquecido_ao_sair` o exige).
- **Verde:** o «⋯» de uma linha apresentada por MOD, aberto pelo resumo e com o
  foco nele, atravessa o tique sem mutação e com o foco; o volume de dentro fica
  ao alcance do teclado (`checkVisibility()` e o foco) através de outro tique; o
  SINAL E ESTADO aberto à mão numa linha nativa também fica; quem fica muda tem o
  SINAL E ESTADO aberto pelo produto, e fechado quando deixa de ficar.
- **O celular**, que depende de os outros atributos saírem (M4): a
  `tools/bancada-do-celular.py` inteira passou depois do conserto — 110 casos,
  o G3 IGUAL pixel a pixel no Chromium e no WebKit.

### M2 — o detalhe nativo de quem começa a falar não é refeito

- **Vermelho** (o ponto «pessoa.identidade numa linha apresentada, com a pessoa
  começando a falar» do caso 5, antes do `607b85f5`): «1 de 50 nós de MOD saíram
  do documento».
- **Conserto:** `mesmoFeitio` compara a etiqueta e a **primeira** classe, e o
  resto da classe chega pelo `vestirComo`. Escolhi a primeira classe, e não um
  `data-feitio`, porque não muda nenhuma linha montada: toda classe que varia com
  o estado nas duas listas reconciliadas põe a variante depois da base («pessoa
  falando», «voice_room-entrar aqui»). O ENTRAR que vira VER A CHAMADA passa a
  ser o mesmo nó também.
- **Verde:** o `div` do detalhe é o mesmo nó, e nenhum nó de MOD sai do
  documento.

### M1 — mover só o que precisa

- **Vermelho** (caso 8, antes do `ee24b4f3`): «a primeira de 19 pessoas foi para
  o fim e 18 linhas foram tiradas e postas de novo no grupo da sala».
- **Conserto:** fica no lugar a maior subsequência crescente das posições velhas
  (`maiorSequenciaCrescente`, O(n log n)), e só o resto se move, de trás para a
  frente, cada um para antes do seguinte. Cerca de 24 linhas de código, abaixo do
  teto de ~30 do pedido; nenhum caso que a bancada já prendia mudou.
- **Verde:** um nó inserido no grupo da sala e um na coluna, o foco no nome de
  quem está no meio fica, a ordem e a identidade se mantêm. A
  `um-redesenho-por-quadro.cjs` seguiu verde (34 ms de atraso máximo com 251
  cartões).

### M4, B e M3

- **M4:** o `vestirComo` diz, num comentário, que o celular só adapta de novo a
  coluna de salas porque o `data-celular` sai ali, e que o `open` é a única
  exceção (`fea04116`).
- **B:** os comentários de `camada-moderar.js` sobre as duas delegações diziam
  «as linhas são refeitas a cada snapshot»; agora dizem o porquê de agora — a
  linha é reaproveitada pelo id, e o ouvinte delegado lê os `data-*` que o
  `vestirComo` atualiza (`ec8ec2ce`, só comentário).
- **M3:** neste relatório e na pendência 63, a causa do vermelho mais brando
  virou hipótese; a reprodutibilidade frente a 11/10 e a CPU de 11/10 viraram
  «segundo as notas»; «nove pontos» virou «oito»; a interrupção ficou com as horas
  dos brutos (`interrompida-100-114328`, e a refeita `100-114455`). O I1 saiu do
  «ficou de fora».
- **A:** a pendência 64 (a lista de canais perde o foco do teclado a cada tique,
  dona a F5, num lote à parte), com o mecanismo e a medida; a 63 aponta para ela,
  e lista o celular (F8) e o `roster-mods`, o contador, a conversa e a telemetria
  (sem dona) como abertos.

### As reversões dos consertos da rodada

Na ponta `ec8ec2ce`, pelo mesmo `reverter.sh`; todas voltaram idênticas, e o
sha256 do `tela-sessao.js` antes e depois da série é `4e487980…1b6b`.

| | o mutante | o que reprovou |
|---|---|---|
| RI1 | o `open` volta a ser copiado como os outros atributos | caso 9: «o «⋯» de p-003 … fechou no tique» |
| RM2 | o feitio volta a ser a classe inteira | caso 5: «1 de 50 nós de MOD saíram do documento» |
| RM1 | a ordem volta ao cursor | caso 8: «18 linhas foram tiradas e postas de novo» |
| R1 | o arquivo inteiro do `1bb6450a` | caso 1: «1 de 1 mutações, e 50 linhas trocaram de nó» |

### A sonda de novo, na ponta

O app `verde-r1`: o fonte do `1bb6450a` com o `tela-sessao.js` e o
`camada-moderar.js` do `ec8ec2ce` (`tela-sessao.js` sha256 `4e487980…1b6b`;
`seele-app` sha256 `2c504453…836b`). O mesmo arranjo e os mesmos cuidados. Antes
de cada rodada o condutor grava o `pgrep -fl -i seele` inteiro e para se houver
o executável de um app, um `m3-servidor` ou um `m3-falante` (a lista casa também
com todo caminho que tenha SEELE: os shells, os testes e o próprio Claude de
outras sessões); e confere o `sessao`.

| hora | pessoas | a lista inteira | nomes achados | ENTRAR NA SALA | entrou | CPU do WebKit | consulta mais lenta (s) | assenta |
|---|---|---|---|---|---|---|---|---|
| 12:59 | 10 | 38 de 38 | 9 | 38 de 38 | sim | 6,4% | 0,1 | sim |
| 13:02 | 50 | 37 de 37 | 49 | 37 de 37 | sim | 18,3% | 0,2 | sim |
| 13:05 | 100 | 37 de 37 | 99 | 37 de 37 | sim | 51,1% | 0,4 | sim |
| 13:35 | 10 | 38 de 38 | 9 | 38 de 38 | sim | 5,4% | 0,3 | sim |
| 13:37 | 50 | 37 de 37 | 49 | 37 de 37 | sim | 16,6% | 0,2 | sim |

**Assenta em todas, em toda consulta.** As três primeiras estão em
`sonda-verde-r1/` (`10-125947`, `50-130228`, `100-130549`). As duas últimas são
repetições que a coordenadora pediu depois de a janela do app voltar (ela tinha
procurado as rodadas em `sonda-verde/`, que é a pasta do app de antes); a carga
pelo `uptime` antes delas foi 7,70 e 8,86, com um `node ajustes-v013.cjs` de
outra frente rodando.

**A CPU de 51% com 100.** De manhã, o verde (`d44f7c1f`) deu 28 a 33% com 100.
Para não atribuir a diferença ao código da rodada sem medida, rodei em seguida um
par com 100, o app de antes e o da ponta, um depois do outro:

| hora | app | a lista inteira | ENTRAR NA SALA | CPU do WebKit | assenta |
|---|---|---|---|---|---|
| 13:11 | verde (`d44f7c1f`) | 37 de 37 | 37 de 37 | 46,1% | sim |
| 13:16 | verde-r1 (`ec8ec2ce`) | 37 de 37 | 37 de 37 | 50,1% | sim |

O app de antes também subiu (de 28–33% de manhã para 46% à tarde), então a
maior parte da diferença não é da rodada; entre os dois, 4 pontos numa amostra de
cada não separam o código do ruído. Não medi a causa da subida. Depois do par, a
orquestradora pediu a janela do app, e não abri mais nada até ela voltar
(13:22:55); as repetições de 10 e 50, acima, vieram depois disso.

### O que rodou na rodada

Na ponta do código `ec8ec2ce` (os documentos vêm depois), com a HOME falsa por
script nos testes de Rust, todas terminando vazias:

- **`cargo test -p seele-app --test frontend`**: 296 de 296 (também depois do I1,
  com o `abertoPedido` esquecido na saída);
- **`cargo test -p xtask`**: 230 (88 + 4 + 121 + 1 + 3 + 10 + 1 + 2);
- **`cargo xtask check-runtime`**: passa;
- **as 22 bancadas de navegador do CI**: as 19 do job `bancadas` e as 3 do
  `bancadas-de-mod`, com o `.wasm` do `cargo xtask web`, todas verdes. A
  `linhas-pelo-id.cjs` com os nove casos e os oito pontos de MOD; a
  `um-redesenho-por-quadro.cjs` com 38 ms de atraso máximo;
- **os 40 roteiros**: «os 40 passaram», carga limpa;
- **a bancada do celular inteira**: 110 casos, o G3 IGUAL pixel a pixel no
  Chromium e no WebKit — depois do I1 (por causa do `data-celular`) e de novo na
  ponta;
- **a medida da 64**, no Chromium da bancada: o foco num canal cai no `BODY`
  depois de um tique, com o retrato igual e com outro canal mudando.
