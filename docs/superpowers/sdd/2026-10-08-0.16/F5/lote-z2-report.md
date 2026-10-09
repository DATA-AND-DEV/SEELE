# Relatório do lote Z2 (F5, 0.16)

O fecho em documentos do que a frente F5 fez: as pendências 49, 50 e 51 item a
item, as pendências 52 a 56, o índice da 1.0, e uma pendência nova com o que a
frente deixou. São só documentos.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a42fe42c2f2b6830b`
- Branch: `0.16/lotes-z2`, aberto em `cd2227c5`.
- Brief: `.superpowers/sdd/lotes/lote-z2-brief.md`, com os insumos de
  `.superpowers/sdd/lotes/z2-insumos.md` e o que a mensagem do controlador
  acrescentou: os hashes do R, da 56, da 52, da 53 e da 54, e a decisão da
  pendência nova.
- Arquivos mudados: só `docs/pendencias.md` e
  `docs/superpowers/plans/2026-09-29-1.0-indice.md`, mais este relatório.
- Status: **DONE_WITH_CONCERNS**. As preocupações estão no fim, e nenhuma
  bloqueia.

## Os commits

| commit | o que entra |
|---|---|
| `7db4c445` | pendência 49 |
| `d4f501da` | pendência 50 |
| `e4900884` | pendência 51, item por item |
| `e1086cc1` | pendências 52, 53, 54, 55 e 56 |
| `909aa3b6` | pendência 58, nova |
| `ac7355d5` | o índice da 1.0 |
| `6b5b7a72` | pendência 51, item 15: a medida do «panicked» |
| este | o relatório |

Cada assunto começa com «docs:» e termina em `Co-Authored-By: Claude Opus 5.5
<noreply@anthropic.com>`. Nenhum assunto casa com as catorze cadeias do G1:
conferido com `grep -F -f` das linhas entre `CADEIAS` da receita do plano 1B
contra os assuntos de `cd2227c5..HEAD`.

**A regra das linhas históricas.** Nenhuma linha que estava foi reescrita,
com duas exceções que o brief pede ou que seguem um molde que já existe:
- os títulos de 49 e 50 mudam, como o brief manda;
- os títulos de 52, 53, 54 e 56 ganham o estado na frente («Fechada em …
  ·»), no molde do título da 39 (ver a preocupação 1);
- no índice, a linha do Plano 6, o item 2 de «Antes de cortar a release» e a
  lista «Seguem abertos» mudam, como o brief manda.

Todo o resto é texto novo, depois do bloco de comandos de cada item.

## O que mudou em cada pendência

### 49
- **Título:** «Consertada em 2026-10-01, medida em 07 e 08/10/2026, e toca no
  WKWebView e no WebView2».
- **5:** fechado em 2026-10-08, `82fd154` (K2), com a metade do fundo de tela
  em `75fb60f` e a prova do bloco na página em `4e4bc93d` (K3).
- **6:** cumprido em `563bc37`. O contrato de `tocando` está em
  `api/README.md`, e o guarda é `oSomQueTerminouVoltaComOTocandoDeclarado`, na
  `regiao-do-mod.cjs`. `repetir` fica como opção, e a D-api-2 o põe no contrato
  da API 6.
- **7a:** fechado em 2026-10-09, `9b43de1d` (K4), com `f9ef3d23`, `a9a47a8b` e
  `0b9029ec` da rodada de conserto.
- **7b:** fechado em 2026-10-09, `e8a485cc` (K4).
- **7c:** o som adiado fechou em 2026-10-08, `6be3d244` (K5). Fica com o dono o
  teto por MOD.
- **8:** fechado em 2026-10-08, `d9f481ee` (K5), com `108907f8`, `8f67e167`,
  `1d5c039f`, `8ff12f12`, `fe1bff3a` e `946a4c77` das rodadas.
- **9:** fechado em 2026-10-08, `66133eca` (K3), com `e569ae93` da revisão.
- **10:** perdeu o objeto com a API 6. A capacidade `cartoes` não existe na API
  6 (`4da1e4bd`, da F3), e a frase sai junto com o código das APIs 3 a 5. Não
  há «fechado» nele.
- **«Quando fecha»:** um parágrafo de 09/10 diz o que fechou e o que continua
  aberto.

### 50
- **Título:** «Consertada em 2026-10-08».
- **Uma seção nova,** «O conserto, na F5 da 0.16»:
  - **P50-1:** `dde763b5`;
  - **P50-2:** `fd10af6b`, com `8a38df53`, `72b57732`, `aa9e7f4c` e
    `c429302c` da revisão;
  - **P50-3:** `b7aeb749`, com `6a0d5bed` da revisão;
  - **P50-4:** a metade servidor em `271360dd` e `586c23de` (G-servidor,
    integrado em `0cad324b`); a metade casca vai com o Plano 2.
- **A medida.** A sonda `medida_do_disconnect.rs` nunca foi commitada, e o teste
  novo, `saida_com_despedida.rs`, é a medida. Antes eram 19,98 s e 20,98 s.
  - (a) e (b): 554 µs e 1,1 ms na primeira rodada, de 340 a 700 µs nas seis
    rodadas do fim do P50-1, e 0,5 a 0,6 ms no fim da rodada 1 do F.
  - O filho do P50-2: cerca de 157 ms (153,6 a 161,0 ms), dos quais 150 ms
    estacionado de propósito desde `8a38df53`. Os 611 µs eram da primeira
    versão do teste. *(Corrigido na rodada 1: a primeira versão deste
    relatório misturava os dois números.)*
- **O que cada espera prova,** pela decomposição do F: sem a espera do `sair`,
  ou sem a de `encerrar_e_esperar`, volta aos 20 s; sem só o `wait_idle`,
  fica verde.
- **O que fica sem prova,** como o doc diz (`b9adf2f8`): o `wait_idle` como
  reenvio de um `CONNECTION_CLOSE` perdido, e a espera pelas alças abortadas.

### 51, item por item

| item | commit que fecha | lote |
|---|---|---|
| 1 | `3f1d2e93` (+ `20404362`) | D |
| 2 | `663363d4` (+ `1c199b9b`) | H |
| 3 | `4807207b` (+ `4205d299`, a frase nova) | H |
| 4 | `b655edf` (já registrado) | K1 dos abertos |
| 5 | `0844129b` | C |
| 6 | `ef707c7` (já registrado) | K1 dos abertos |
| 7 | `75fb60f` (+ `86d0a2d`, `6ca4fdf`) | K2 |
| 8 | `37a5d902` (+ `6979c2b7`, `6e71e894`, `255fc574`) | C |
| 9 | `bece2bd9` (+ `a2d86649`, `0b6b9193`, `06256c27`; `9daa0c0` na integração) | E |
| 10 | `9f90888b` | C |
| 11 | `9abe0678` (+ `a2d86649`) | E |
| 12 | `53587a1f` | C |
| 13 | `4599e14` (já registrado) | K1 dos abertos |
| 14 | m8 `59d66aa0`; m9 sem commit, pelo C (`0844129b`, `37a5d902`); T3 M2 `6bf9ddd7`; T1 `bead90c2` (+ `8719c9ee`); T2 N1 `74a5c427` (+ `7817f871`, `047c40ea`, `b7ac8fc7`); T2 m1 `517afaa`, antes | D |
| 15 | `1345e0e` (+ `5f4e1bb`); a etapa do `publicar.sh` em `8b766e5`, antes; o «panicked» medido ausente, sem commit identificado (rodada 1) | ferramentas |
| 16 | `8669d99` (+ `9a46b77`); a 8384 como escolha documentada | M2 |
| 17 | C1 `2b20316`; T5a/T5c `09accad`; T5d `8e7175a`; T6f `5e977ae`; T7a `25b5aeb`; T1a `cc645fb`; T1b sem commit; T1c `6243f13` (+ `f6fefa6`); T2c/T3c/M3 `f8f85e3`; T4a/T8b/T8d `640240d`; o 1B `4b127e3` e `092dbfc` | M1 (e M2) |
| 18 | `aa106bc` | M2 |
| 19 | C4 `56b4d02` (+ `04a0ce3`, `fba4015`); T5e `e2a5b72`; M2 `d35488a`; T3d/T3e `d564d1d`; T5f `27f86af`, antes, sobrando o `Espiao`; T7b `07bd5c3`; os planos `9ddd404` | M1 |
| 20 | `70a0f12` (+ `9de50a7`) | M2 |
| 21 | `d7ecb3e` | ferramentas |
| 22 | `a77da2f`, antes | — |
| 23 | `f62ed74a`, `0db8cbc7`, `f7cb4fcf`, `19391f03`, `69f194f7` (Docs-m5 em `66133eca`) | K3 |
| 24 | aberto: outro repositório | — |
| 25 | aberto: outros repositórios; a parte da janela em `8cadbc27` | K5 |
| 26 | servidor `fbf2d0d`, `deed8a0` (já registrado); app `9f90888b` (+ `33401b5e`) | B dos abertos; C |
| 27 | `cd71f8e`, `380a745`, `34235fe` (já registrado) | B dos abertos |
| 28 | absorvido pela API 6 (F3) | — |

- **O item 3** registra que a frase de `ServidorTrocouNaVolta` mudou na revisão
  do H. A frase do plano era falsa por três motivos: o «nada», a origem e o
  remédio. A de hoje não diz de onde veio a impressão, e o guarda que a prende
  fica escrito.
- **O item 20** ganha os comandos que conferem o C-R1-m1 sem casar com a frase
  nova, e o que acha a sobra do C-M4. É o Menor do M2 sobre os comandos de
  fecho, resolvido aqui por ser texto.
- **«Quando fecha»** ganhou um parágrafo de 09/10.

### 52, 53, 54, 55 e 56
- **52:** decidida em 2026-10-08 (D-52) e fechada em 2026-10-09:
  - `d96fecc0` (I-a), com `e28cb84b`, `2b32795f` e `421d628d`;
  - a regra do assento em `a9e8c128`, `893632ab` e `d02743ea`;
  - o subitem do pânico em `520f880a`;
  - do I-b, `2296ee5b` e `eaba0ea6`.
  
  As medidas em aparelho ficam com o dono.
- **53:** fechada em 2026-10-08, `c6ca70e4` e `6038abbe` (P53), com `1a7011e7`,
  `8b6ea89a`, `14253634` e `2d2a914b` das rodadas.
- **54:** fechada em 2026-10-08, `0d68c95` (P54).
- **55:** o nome foi corrigido para `Batida::preparar`, num parágrafo novo, sem
  apagar o texto de antes. Fica com o Plano 5.
- **56:** fechada em 2026-10-09:
  - `a4fcd99e`, `cefb5c53` e `12bb1769`, o auxiliar nos três crates;
  - `f4cba99e`, as provas em processo filho;
  - `3230eb9a`, o guarda do xtask.
  
  A prova do app ficou em processo único, coberta pelo guarda de texto, e vai
  para a revisão final.

### A pendência nova: **58**, «O que a F5 da 0.16 deixou»
- **O número.** A 57 já existia (`7edb87f8`, da F7). Nenhum ramo do
  repositório tem uma 58: `git log --all -S'## 58 ·' -- docs/pendencias.md`
  sai vazio.
- **O conteúdo.** São 51 itens depois da rodada 1 (49 na primeira versão),
  cada um com o lote, o arquivo, o que falta e a fonte:
  - o registro e a tela, com as quatro portas sem balde do C no item 1;
  - os guardas e testes que provam menos;
  - o texto;
  - o que o dono confere em tela;
  - o que a revisão do R deixou, nos itens 46 a 51. O R entrou no
    `0.16/lotes` em `75446300`, e os oito commits dele ficam citados como o
    controlador passou.
- **O que é de outra frente** fica numa lista de referência, sem duplicar o
  registro dela: a sala cheia e o resgate no fio (F1), a janela do aviso
  (Plano 3), a telemetria fora da projeção da v6 e o rótulo «cartões pela API
  3» (F3). Fica também uma nota para o Plano 2, a forma (d) do G-servidor.
- **Nada foi consertado.**

### O índice da 1.0
- **A linha do Plano 6:** «o contrato de `tocando` (`api/README.md`, `563bc37`)
  entra no texto da v6; `repetir` é opção, não condição».
- **O item 2 de «Antes de cortar a release»:** as notas terminam em «Quanto
  custa voltar a um servidor pela lista», que `54f542f` escreveu no lugar do
  bloco «[PREENCHER…]».
- **«Fora desta especificação»:** G, J e I deixam de estar abertos.
  - G-servidor: `271360dd` e `586c23de`;
  - J: `307b18e0`, `092e6a5b` e `acbc1c29`;
  - I-b: `f0a2a74d`, `3722e851`, `e95214b` e `85805fa`;
  - I-a: `d96fecc0` e `520f880a`.
  
  Cada um vem com o merge que o integrou. O G-casca aponta o Plano 2, e a lista
  aponta a pendência 58.

## Como os hashes foram conferidos

- Os 152 hashes citados nas linhas acrescentadas dos dois arquivos passaram
  por `git cat-file --batch-check`, e todos são commits.
- Para saber quais estão na história de `cd2227c5`, comparei com
  `git rev-list cd2227c5`. Ficam fora só os oito do R (`211ed155`, `21471bfc`,
  `3ff15974`, `d521b41c`, `789729d5`, `a910b2e2`, `9b1c2473` e `6b23b694`), no
  ramo `0.16/lotes-r`, como o controlador disse. Desde `75446300`, eles estão
  também no `0.16/lotes`.
- Toda data de «fechado em» é a data do commit que fecha. Conferi cada uma com
  `git show -s --date=short`.

## O que foi medido de novo, em 09/10

Os comandos de cada item foram rodados de novo na árvore de `cd2227c5`, e o que
eles dizem hoje ficou escrito no item:
- **49, item 5:** o `grep -c anotarMidia` dá 2.
- **49, item 7:** «atravessa duas vezes» não acha nada.
- **49, item 9:** o comando de node imprime «automática: mod/a para todos,
  mod/b para quem declarou». A linha «10 ·» continua como estava.
- **51, item 7:** o comando antigo ainda dá 0, porque o ouvinte de `error` está
  a 54 linhas do começo de `montarRetrato`. Com `-A 60`, dá 1.
- **51, item 10:** «o formatador do registro escapa ANSI» não acha nada. Três
  `mod_id = %id` que o lote deixou como estavam continuam.
- **51, item 15:** `cargo test --offline -p xtask --bin xtask check_api` passa
  45 testes, sem nenhuma linha «panicked», com o `RUST_TEST_NOCAPTURE` do
  `.cargo/config.toml`.
  - O `target/` semeado de `abertos-da-1.0` não existe mais.
  - Por isso o xtask foi compilado do zero, sem rede: só ele e as três
    dependências dele.
- **51, itens 16 a 23:** cada comando foi rodado, e o item diz o que ele acha.
- **53 e 55:** a frase de `NaoCopiei` e `TemAtalho` está em `frases.js`, e
  `Batida::preparar` existe em `encontro.rs`.

**Nenhum guarda lê os dois arquivos.** Procurei em `xtask/tests/` e no resto do
código, e nenhum guarda lê `docs/pendencias.md` nem o índice. O
`textos_publicos.rs` lê outros docs, e o `adr_index.rs` lê `docs/adr/`. Por
isso nenhum guarda rodou para este lote.

## O que não fechei, e por quê

- **49:** continuam abertos:
  - o item 4, o disparo do `ci.yml`, que vale para toda publicação enquanto o
    workflow só rodar por `workflow_dispatch`;
  - o teto por MOD do 7c, que é decisão do dono;
  - o macOS 11.0 a 11.2, não medido.
- **50:** a metade casca do P50-4 é do Plano 2.
- **51:** o 24 e o 25 são de outros repositórios, e o 28 é da F3, na API 6.
- **55:** fica com o Plano 5.
- **58:** os 51 itens ficam abertos de propósito. O brief manda registrar, e
  não consertar.
- **O lote R** não fecha número de pendência. Os commits dele só aparecem na
  58, como contexto dos itens 46 a 51.

## Para a v6

Nada. Este lote não toca código nem `api/`.

## Preocupações

1. **Os títulos de 52, 53, 54 e 56 ganharam o estado na frente.** O brief pedia
   isso só para 49 e 50. Segui o molde da 39 («Fechada em 2026-09-14 · …»),
   para quem lê só os títulos. Se não servir, são quatro linhas a voltar.
2. **O item 3 da 51 não diz o que o brief sugeria.** O brief diz que a frase
   nova «não fala em link nem em convite», mas ela diz «O convite, a senha e o
   apelido não chegaram a ele». Escrevi o que vale: ela não diz de onde veio a
   impressão, e não diz nem «link» nem «servidor do convite», que eram da frase
   do plano.
3. **O «panicked» do item 15 da 51.** *(Resolvida na rodada 1: o item deixou
   de atribuí-lo a `4887565`.)*
   - O `4887565` (30/09) é ancestral do `c1cc3ae9`, o commit que escreveu o
     item em 01/10, e nenhum commit tocou o `check_api.rs` entre os dois e o
     `d4eb6f1`.
   - A atribuição a ele era a do plano da F5. A revisão do Z2 a mediu em onze
     versões do `check_api.rs`, e nenhuma imprime «panicked», nem a
     `4887565^`: a atribuição não se sustenta.
   - O que se mediu é que hoje não sai «panicked». O item diz só isso, e o
     fecho dele fica em `1345e0e` e `5f4e1bb`.
4. **Três comandos antigos da 51 não medem mais o que mediam:**
   - o do item 7 dá 0 com o conserto feito;
   - o do item 17 ainda acha o `sort_by_key`, agora dentro da função pura;
   - o do C-R1-m1, no item 20, casa com a frase nova.
   
   As linhas antigas ficaram, porque são a história. Cada item ganhou o comando
   que mede hoje, ou a explicação.
5. **Itens da 58 cuja fonte é só o ledger**, com a numeração da rodada 1: 2,
   3, 7, 16, 18, 24, 25, 26, 28, 30 e 49. O ledger mora em `.superpowers/`, que
   some com a árvore, e por isso o texto desses itens é a fonte. Onde o
   `F5/progress.md` também traz o achado, o item o cita.
6. **Os itens do R na 58.** *(Resolvida na rodada 1.)* Na primeira versão,
   eles vinham do ledger, porque o ramo do R não tinha relatório. O R entrou em
   `75446300` com o relatório, e os itens 46, 47, 48 e 50 citam o relatório
   dele. O 49 continua no ledger, o único que o registra.
7. **A linha do Plano 6 no índice ainda diz «PERFIS 3.4.0»**, e a D-mods-1 da
   0.16 trocou isso para a 4.0.0. Não mexi: o §9 do spec-mestre
   (`docs/superpowers/specs/2026-10-08-0.16-design.md`, «Documentos a emendar
   ou escrever») põe a linha 22 do índice na passagem de documentos da 0.16, e
   ela tem mais coisa obsoleta além do PERFIS.
8. **O item 6 da 49 junta duas fontes.** O brief diz «`repetir` fica como
   opção», e a D-api-2 do spec da 0.16 põe `repetir: true` no contrato da API 6.
   Escrevi as duas juntas: `repetir` é opção, e não condição para congelar a
   v6.

## Rodada 1

FIX_BASE `1a6840b3`. Brief: `.superpowers/sdd/lotes/lote-z2-rodada1.md`.

**O que a revisão disse.** O lote está conforme, e a exatidão é boa. Os 152
hashes existem, e as preocupações 1, 2, 4 e 8 deste relatório foram aceitas
com medida. Na resposta ao controlador, a lista tinha outra ordem, e o
`repetir` era a 7. Ficaram quatro Importantes e os Menores 5 a 8. As seções de cima foram acertadas onde
diziam o que esta rodada corrigiu, com a marca «rodada 1».

| commit | item | o que muda |
|---|---|---|
| `2cba26ac` | Importantes 1 e 2 | o 51-15 diz só o que foi medido do «panicked»; a medida da pendência 50 |
| `473fd682` | Menor 8 (a 54) | o fecho da 54 cita o `lotes.md`, e não o brief, e a linha quebrada à mão se junta |
| `9ccf8686` | Importantes 3 e 4, Menores 6, 7 e 8 (a 58) | a seção do R, o lide, os itens tocados, os três itens novos e a renumeração |
| `55798efb` | Menor 5 | o índice atribui a cláusula do «único lugar» a `e95214b` |
| este | o PERFIS, e o relatório | as correções de cima, o §9 do spec na preocupação 7, e esta seção |

### Importante 1: o «panicked» do 51-15
- **O que estava errado.** O item atribuía a saída do «panicked» a `4887565`.
  A revisão rodou `cargo test --offline -p xtask --bin xtask check_api` em
  onze versões do `check_api.rs`, a `4887565^` inclusive, e nenhuma imprime
  «panicked». Um `#[should_panic]` posto numa cópia imprime a linha, então a
  medida enxergaria.
- **O conserto.** O item diz só o que foi medido: hoje não sai, com 45
  testes e nenhuma linha. O commit que o tirou não foi identificado. O fecho
  fica em `1345e0e` e `5f4e1bb`.

### Importante 2: a medida da pendência 50
- **O tempo do filho.** Os 611 µs e os 445 a 720 µs eram da primeira versão
  do teste, de antes de `8a38df53`. Hoje o filho some em cerca de 157 ms
  (153,6, 155,8 e 161,0 ms), e 150 ms disso são o estacionamento de propósito.
  Os números de (a) e (b) ficam, com o do fim da rodada 1 do F, de 0,5 a
  0,6 ms.
- **A frase das esperas.** «Com cada espera revertida, os três voltam aos
  20 s» era falsa para duas delas. O parágrafo novo, «O que cada espera
  prova», segue a decomposição do F (rodada 1, I2 e M1):
  - sem a espera do `sair` pelo motor, os três vão aos 20 s;
  - sem a de `encerrar_e_esperar`, o filho vai aos 20 s;
  - sem o cancelamento das tarefas de par, o (c) de `tela_por_um_par.rs`
    reprova;
  - sem só o `wait_idle`, ou com os braços de volta ao `encerrar`, fica verde.
  
  O parágrafo «O que fica sem prova» já dizia o do `wait_idle`, e agora os
  dois concordam.
- **Este relatório repetia a mistura** («340 a 720 µs»). A seção «50», acima,
  foi acertada.

### Importante 3: a seção do R na 58
- **O título** passou a «O que a revisão do lote R deixou». A abertura diz
  que o R entrou no `0.16/lotes` em `75446300`, depois da conformidade
  (`volta_do_aceite`, 2 de 2), e aponta o relatório dele,
  `docs/superpowers/sdd/2026-10-08-0.16/F5/lote-r-report.md`. Li o relatório
  com `git show 0.16/lotes:…`, porque este ramo não o tem.
- **As fontes:**
  - os itens do sim gravado, do segredo no Rust e do `NicknameTaken` (46, 47 e
    48) citam o relatório do R, «Observações»;
  - o do guarda que casa com `reconectar(` (50) cita o relatório do R, mas na
    «Rodada 1 de conserto», M5, que é onde ele está, e não nas «Observações».
    Cita também o conserto do guarda do R, `8097d310`;
  - o dos outros tipos com `derive(Debug)` (49) continua no ledger, o único que
    o registra.
- **O item do `NicknameTaken`** voltou a dizer que a pessoa tem de colar o
  link de novo, porque as «Observações» do R dizem isso.

### Importante 4: o lide da 58
O lide diz que a 52, a 53, a 54 e a 56 fecharam, cada uma com os commits
escritos nela, e que a 49, a 50 e a 51 fecharam só em parte. Diz também o que
fica aberto em cada uma, com o parágrafo «Em 09/10/2026» de cada uma como a
referência.

### Menores
- **5.** No índice, a cláusula «o único lugar que esquece» foi para
  `e95214b`, que é o commit que a diz no assunto. `3722e851` só cria a função
  e a chama nas saídas.
- **6.**
  - O item do sim ganha «(e o MOD instalado)».
  - O do segredo perde o «Toca a F1».
  - No dos tipos com `Debug`, o «Falta» vale para os três do app e do
    `seele-proto::uri`. O `ClientMessage::Hello.join_secret`, de `control.rs`,
    fica só como referência, por ser da F1.
  - Ganham arquivo:
    - a trava (`hospedagem.rs` e `lib.rs` do `seele-server`);
    - o `NicknameTaken` (`tela-boot.js`);
    - o `git tag` e o passo `--locked` (`xtask/tests/empacotamento.rs`);
    - a prova do app (`main.rs`, com o nome do teste e o guarda que a cobre).
  - O item da gestão que não cita quem perdeu sai de «Para o dono conferir em
    tela» e vai para «O registro e a tela», por ser lacuna de produto.
- **7.** Entram três itens:
  - na referência, o rótulo «cartões pela API 3», em `modsDoCaminhoAntigo`
    (`camada-mods.js`), da F3;
  - na 58, as metades das partes 4 e 5 do guarda do R, que ficaram sem
    reversão (relatório do R, «3. O guarda»);
  - na 58, o `Espiao` do `seele-ffi`, o resto do T5f do 51-19.
- **8.** O item do padrão duas vezes, o fecho da 54 e o item da gestão citam o
  plano da F5 (`lotes.md`), e não o brief. A linha quebrada à mão do fecho da
  54 se juntou.

**A renumeração da 58.** Com o item da gestão mudando de seção e os dois itens
novos, os itens foram renumerados na ordem de leitura, de 1 a 51. O item do
comentário do H aponta o guarda de `drive` pelo número novo, 17. Nenhum outro
documento cita a 58 por número de item. Este relatório usa a numeração nova
nas seções de cima.

**O PERFIS (preocupação 7).** Não mexi na linha 22 do índice. O §9 do
spec-mestre a põe na passagem de documentos da 0.16, e a preocupação 7, acima,
passou a apontá-lo.

**As conferências da rodada:**
- os 18 hashes citados nas linhas novas da rodada passaram por
  `git cat-file --batch-check`, e todos são commits. Os novos são `75446300`
  e `8097d310`, que estão no `0.16/lotes`, e não na história deste ramo;
- nenhum assunto da rodada casa com as cadeias do G1;
- nenhum arquivo além dos três foi tocado.
