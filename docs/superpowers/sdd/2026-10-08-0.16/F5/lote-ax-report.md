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
  commits só dele), `apps/seele-app/bancada/linhas-pelo-id.cjs` (nova),
  `.github/workflows/ci.yml` (a bancada nova no job de navegador), e os
  documentos: este relatório e a pendência 63.
- Status: **DONE_WITH_CONCERNS**. O verde passa com 10, 50 e 100 pessoas em
  toda consulta; as preocupações estão no fim, e nenhuma bloqueia.

## Os commits

| commit | o que entra |
|---|---|
| `c5691d78` | o conserto da lista de pessoas: `reconciliarALista`, as chaves, os marcadores de MOD, o esquecimento das chaves com a sessão (só `tela-sessao.js`) |
| `22a66313` | a bancada `linhas-pelo-id.cjs`, casos 1 a 5, e o passo dela no `ci.yml` |
| `d44f7c1f` | o conserto da coluna de salas: a mesma reconciliação, a sala e a pessoa como chaves, o RENOMEAR com o nome de agora (só `tela-sessao.js`) |
| `bede50d9` | a bancada, caso 6: a coluna de salas e o ENTRAR NA SALA |
| `c0d26666` | a bancada, caso 7: a saída da sala na coluna e o nome de agora no RENOMEAR |
| este | o relatório e a pendência 63, (b) |

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

As builds são reprodutíveis: o vermelho e o verde montados de novo hoje saíram
com os mesmos sha256 dos de 11/10.

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

A CPU do WebKit, nela, foi de 32,4% (vermelho) para 27,5% (verde) com 50 e de
52,0% para 44,2% com 100. **Hoje o vermelho reprova bem menos que em 11/10**
(com 100, 29 e 35 de 36 contra 0 de 38), e a CPU do WebKit no vermelho também é
menor (31% contra 52%): o mesmo app, o mesmo servidor e o mesmo falante, então é
a máquina que mudou — não medi o quê. A conclusão não depende disso: nas duas
campanhas o vermelho reprova com 50 e com 100, e o verde nunca perdeu a lista.

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
  filho velho da mesma etiqueta e da mesma classe, e recebe os atributos, o texto
  e o valor do novo. O valor de quem está sob o dedo (`document.activeElement`)
  não é trocado, como em `pintarVolumesDoRoster`.
- **A ordem.** O que já está no lugar fica; só o que falta ou mudou de lugar é
  inserido. Um nó velho com chave que não serve ao grupo de agora espera o fim,
  porque um grupo seguinte pode querê-lo; o que ninguém quis sai no fim.
- **Os nós de MOD não saem do documento.** Os cartões e as contribuições chegam
  montados e são o mesmo nó entre um retrato e o seguinte. Anexá-los à linha
  montada os tiraria da lista para voltar — para a árvore de acessibilidade,
  outro nó, e o som e a imagem recomeçando. Por isso a linha montada leva um
  **marcador** (`noDeMod`, um comentário vazio) no lugar de cada um, e a
  reconciliação põe o nó onde o marcador está, sem entrar nele. São nove pontos
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

- **A coluna de canais (`lista-linhas`) e as outras regiões repovoadas a cada
  quadro** — `roster-mods`, o `pessoas-conta` do cabeçalho, a conversa, a
  telemetria. A sonda não as mede, e esta parte da pendência é a lista e o
  ENTRAR NA SALA. Pelo mesmo mecanismo, o que elas têm sai e volta da árvore a
  cada quadro com muita gente na sessão.
- **No celular, a coluna de salas ainda gira.** `celular-servidor.js` adapta a
  coluna depois de cada quadro contando com ela refeita, e a reconciliação tira o
  que ele pôs para ele pôr de novo: o ENTRAR e o ENTRAR MUDO de lá continuam
  trocados a cada quadro, como antes. A aba PESSOAS do celular usa a
  `#lista-roster` do desktop e só ouve cliques, e ganha o conserto inteiro.
  Tornar a adaptação do celular idempotente é trabalho da F8.
- **O `<details>` que a pessoa abre ainda fecha no quadro seguinte.** O SINAL E
  ESTADO de uma linha que não está «interessante» nasce fechado, e a
  reconciliação copia o `open` da linha montada, como a lista refeita fazia. Com
  o foco agora preservado, quem navega pelo teclado chega ao resumo, abre, e o
  vê fechar em menos de meio segundo. Não mudei, porque muda comportamento, e
  não só identidade; o caminho é a reconciliação só mexer no `open` quando o que
  a linha montada deriva (mudo, isolado, faixa) mudar.
- **O que um VoiceOver de verdade faz com a árvore** — a sonda mede se a árvore
  tem a lista quando é consultada, como ela mesma diz.
- **A parte do servidor da pendência 63** (um PersonState de cada pessoa a todas
  as outras, por segundo) é outra medida, e não é deste lote.
- **Os arquivos brutos da primeira campanha** se perderam com o scratchpad; os
  números dela estão acima e na mensagem do `c5691d78`. Os da segunda estão no
  scratchpad desta sessão (`ax/sonda-vermelho`, `ax/sonda-verde`).

## Preocupações

1. **O vermelho de hoje é mais brando que o de 11/10**, com o mesmo app (sha256
   igual): com 100, 29 e 35 de 36 contra 0 de 38, e a CPU do WebKit em 31%
   contra 52%. Não medi a causa. O verde passou nas duas campanhas; a de hoje é a
   que tem o conserto inteiro.
2. **Duas interrupções que não vieram do produto.** (a) Na primeira campanha, a
   segunda rodada verde não começou: a F3 abriu a SEELE dela (`/tmp/m3-6`) na
   8393, e o condutor parou, como a regra pede. (b) Hoje, às 11:44, algo de fora
   terminou com SIGTERM os meus processos de fundo, o servidor e o falante de uma
   rodada vermelha de 100; a sonda ficou esperando os calados, o app não chegou a
   abrir, e eu a encerrei. A rodada foi refeita (11:44, na tabela).
3. **Um falso positivo na conferência de outra SEELE.** O `pgrep -fl SEELE.app`
   casou com a linha de comando de um laço meu de espera, que tinha esse nome no
   texto; o condutor parou, como devia, e passou a contar só o executável
   (`/Contents/MacOS/seele-app`).
4. **A consulta mais lenta do verde com 10 levou 4,3 s**, uma vez, com a carga da
   máquina em 7,6. As outras ficaram em 0,7 s ou menos.
