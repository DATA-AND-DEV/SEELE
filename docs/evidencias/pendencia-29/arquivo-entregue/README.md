# As medidas feitas sobre o arquivo que está sendo entregue

**Por que este diretório existe.** A segunda revisão independente conferiu o
SHA-256 do `crates/seele-conformance/tests/vaga/mod.rs` entregue contra o dos
registros vizinhos e achou uma diferença real: toda a evidência de `rodadas/` e
`reversao/` foi produzida sobre a versão **anterior** do módulo
(`73bdeb645b92…`), sem o prazo da fila, sem o abandono coletivo e sem o aviso
por `stderr`. Números medidos sobre um arquivo não valem para outro. Tudo o que
está aqui foi medido sobre o arquivo entregue:

```
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  crates/seele-conformance/tests/vaga/mod.rs
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  (backup vaga_ARMADO7.backup.rs)
```

Cada rodada imprime o hash do arquivo sob medição no seu próprio `*_driver.log`,
para que a confusão não possa se repetir em silêncio.

## As duas cargas, e por que elas medem coisas diferentes

| gerador | carga | o que mede |
| --- | --- | --- |
| `motor/rodada5.sh` | 8 queimadores de processador | a suíte sob máquina saturada, **dentro** do alcance do conserto |
| `motor/rodada4.sh` | 8 queimadores **+ 6 laços rodando binários de conformidade fora do `cargo`** | o **limite** do conserto |

A diferença não é de grau, é de natureza, e é o ponto que faltava estar escrito:
a permissão é um **semáforo de processo**. Ela governa as 139 threads de teste
do binário em que vive, e não pode governar — nem em princípio — servidores QUIC
levantados por **outros processos**. Os laços do `rodada4.sh` rodam cópias dos
próprios binários de conformidade fora do `cargo`: eles reintroduzem, de fora, a
contenção que o conserto remove de dentro. Essa carga serve para achar onde o
conserto para de valer, e é útil por isso; ela não serve para decidir se ele
funciona. O critério 1 do aceite é medido com a primeira; o limite, com a
segunda, e está dito abaixo com nome e número.

## `reversao7_driver.log` — o critério 3, sobre o arquivo entregue

Ciclo completo, com o hash conferido dos dois lados, todo sob a carga do
`rodada4.sh` e todo em `cargo test --workspace` (gerador: `motor/reversao7.sh`):

| rodada | permissão | saída | parede | o que mostra |
| --- | --- | --- | --- | --- |
| `revws7_1` | desarmada | 0 | 210,81 s | — |
| `revws7_2` | desarmada | 0 | 185,29 s | — |
| `revws7_3` | desarmada | 0 | 186,37 s | — |
| `revws7_4` | desarmada | **101** | 114,39 s | `tela_por_um_par` reprova, **conjunto em 20,32 s** |
| *restauração* | — | — | — | `cmp` sem diferença, SHA-256 igual ao backup |
| `pos_rev7` | restaurada | 0 | 284,83 s | volta a passar |

A reprovação de `revws7_4` é a forma que o aceite pedia e que os ciclos
anteriores não tinham conseguido: **em `--workspace`**, **com a permissão
desarmada**, e **por prazo** — `Error: SemResposta`, com o conjunto encerrando
em **20,32 s**, encostado no `IDLE_TIMEOUT` de 20 s do transporte. O ciclo 7
substitui, para efeito do critério 3, as duas metades que a versão anterior
desta evidência oferecia.

## O diferencial armado/desarmado, medido no mesmo arquivo e no mesmo dia

É a prova que não depende de sorte: os mesmos binários, a mesma carga, a mesma
máquina, mudando só a largura da permissão.

| binário | desarmada (`revws7_1..3`) | armada (`verde8_1..4`, `pos_rev7`) |
| --- | --- | --- |
| `acceptance_m2` (9 testes) | 0,95 / 0,98 / 0,97 s | **3,17 – 3,19 s** |
| `acceptance_m5` (15 testes) | 20,01 / 20,02 / 20,04 s | **22,92 – 23,06 s** |
| soma dos 28 binários | 183,4 / 181,3 / 182,4 s | **254,3 – 275,8 s** |

Desarmado, o `acceptance_m5` para em **20,0 s** nas três rodadas — não é um
tempo, é uma parede: o prazo do transporte. Armado, vai a **23,0 s**, que é o
número da série medido em 2026-08-31, com dez centésimos de margem. A permissão
age, e age sobre o arquivo entregue.

## As rodadas armadas que reprovaram, e por que elas estão aqui e não numa nota

**Esta seção é o conserto de uma omissão minha, apontada pela terceira revisão
independente.** A versão anterior deste arquivo montava a tabela do
diferencial com «`verde8_1..4`, `pos_rev7`» e não dizia em lugar nenhum que
existia um `verde8_5` — que reprovou. A escolha das quatro não foi feita para
esconder nada: elas eram as que tinham o binário inteiro medido. Mas o efeito
foi o do `CLAUDE.md` deste repositório, aplicado à evidência do próprio
conserto: **o produto sabe e não conta.** Fica escrito do jeito que saiu.

Todas as rodadas de `cargo test --workspace` com a permissão **armada** sobre
este arquivo, em ordem, sem seleção:

| rodada | carga | saída | parede | o que aconteceu |
| --- | --- | --- | --- | --- |
| `verde8_1` | `rodada4.sh` | 0 | 286,28 s | — |
| `verde8_2` | `rodada4.sh` | 0 | 260,51 s | — |
| `verde8_3` | `rodada4.sh` | 0 | 261,63 s | — |
| `verde8_4` | `rodada4.sh` | 0 | 279,95 s | — |
| `verde8_5` | `rodada4.sh` | **101** | 184,84 s | `tela_por_um_par::a_reconexao…` — conjunto em 52,35 s |
| `pos_rev7` | `rodada4.sh` | 0 | 284,83 s | fecha o ciclo 3 da reversão |
| `verde9_1` | `rodada4.sh` | **101** | 202,23 s | `tela_por_um_par::a_reconexao…` — `Error: SemResposta`, conjunto em 68,59 s |
| `verde9_2` | `rodada4.sh` | 0 | 265,27 s | — |

**Correção de rótulo, feita ao arquivar as rodadas do aceite.** A versão
anterior desta tabela dizia «8 queimadores» nas sete primeiras linhas. Está
errado, e conferido linha a linha no cabeçalho de cada registro bruto: as oito
rodadas acima saíram do `verde8.sh` e do `verde9.sh`, e **as duas chamam o
`rodada4.sh`** — a carga que este mesmo arquivo declara, duas seções acima,
**fora do alcance do conserto**, porque levanta servidores QUIC em outros
processos. O erro não era inofensivo: ele fazia parecer que as duas reprovações
tinham acontecido sob a carga do critério 1, e portanto que o conserto falhava
onde ele deveria valer.

São **2 reprovações em 8 rodadas armadas**, e as duas no **mesmo teste**, e as
duas **sob a carga que a permissão não alcança**. Não é a §29 voltando: a §29
reprovava *um teste diferente a cada rodada*, e é isso que as outras seis
mostram não acontecer mais. É uma causa **nova**, que a fila não alcança, e que
agora tem nome — está registrada na §29 como **causa 5**.

As duas reprovações são de naturezas diferentes dentro do mesmo teste, e isso
importa para quem for consertá-lo:

- `verde8_5` reprovou na **barreira de encenação**, em
  `tela_por_um_par.rs:3155`: *«o servidor apontou um par e não o contou como
  ocupado: não havia caminho de par para a queda substituir»*. Os 30 quadros
  pelo par já tinham chegado, com uma cópia só — o caminho existia. O que falhou
  foi a leitura de `pares.ja_servindo()` **depois** dele.
- `verde9_1` reprovou por **prazo de conexão**: `Error: SemResposta`, com o
  `libtest` avisando «running for over 60 seconds» antes. É estouro de aperto de
  mão, dentro de um teste que já é o mais longo do crate.

## `srv2_*` — o critério 2 sobre este arquivo: a fila só alcança um crate

O par `antes/depois` de `alcance/` foi medido na versão aposentada do módulo, e
por isso não serve aqui. Refeito com um desenho melhor, que não depende de
comparar duas árvores (gerador: `motor/alcance2.sh`):

1. **Prova estrutural.** `git diff --stat 3c59eec -- crates/seele-server/
   .cargo/ Cargo.toml Cargo.lock` não imprime uma linha. Nenhum byte que o
   `seele-server` compila mudou; os dois lados da comparação abaixo rodam o
   **mesmo binário**, `seele_server-b79839397f752fd8`, impresso nos dois
   registros.
2. **O contrafactual, que é o que realmente prova.** Em vez de «antes contra
   depois», mede-se o preço que o critério 2 **proíbe** pagar: o mesmo binário,
   a mesma carga, com `--test-threads=1`.

| rodada | como | `seele_server` (lib, 453 testes) | conjunto |
| --- | --- | --- | --- |
| `srv2_paralelo` | como a árvore é entregue | **8,01 s** | 29,34 s |
| `srv2_serie` | `-- --test-threads=1` | **24,47 s** | 46,63 s |

Serializado, aqueles 453 testes custam **três vezes mais** (8,01 s → 24,47 s), e
o conjunto do crate ganha 17,3 s. A árvore entregue marca o número do paralelo.
Se a fila tivesse escapado para o `seele-server`, a linha de cima seria a de
baixo. Os dois saíram 0, sob 8 queimadores, e o hash do `vaga/mod.rs` entregue
está no cabeçalho de cada registro.

## `disc_*` — o discriminador da causa 5, e o que ele **não** mostrou

A §29 prometia esta medida e ela não existia quando a terceira revisão leu o
diretório: a bateria seguia correndo. Ela terminou, e está aqui inteira — doze
rodadas, sem seleção. `cargo test -p seele-conformance --test tela_por_um_par`,
o conjunto das duas reprovações armadas, rodado **sozinho** seis vezes sob cada
carga (gerador: `motor/final29.sh`, parte B):

| carga | rodadas | saída | conjunto (19 testes) |
| --- | --- | --- | --- |
| `rodada5.sh` (8 queimadores) | `disc_queimador_1..6` | 0 nas seis | 51,22 – 52,06 s |
| `rodada4.sh` (+6 laços fora do `cargo`) | `disc_fora_do_processo_1..6` | 0 nas seis | 51,71 – 54,57 s |

**O discriminador não discriminou, e isso está escrito porque é o resultado.**
A hipótese que ele ia testar era que a carga de fora do processo é o que faz
esse conjunto reprovar. As duas cargas saíram **indistinguíveis** — 12 verdes,
com as faixas se sobrepondo — então ela não foi confirmada. O que ele mostrou é
outra coisa, e é útil:

- O conjunto **sozinho nunca reprovou**, sob nenhuma das duas cargas. A
  reprovação exige a bateria inteira correndo junto; carga de máquina, de
  qualquer das duas espécies, não basta.
- O conjunto leva **~52 s sozinho**, e `verde8_5` reprovou com o conjunto em
  **52,35 s** — dentro da faixa normal. Aquela reprovação **não** foi lentidão:
  foi a corrida de leitura descrita na §29. Só `verde9_1` (68,59 s) saiu da
  faixa.

Logo a atribuição «é a carga que a permissão não alcança» continua **apoiada
apenas** em 0 reprovações em 6 rodadas sob `rodada5.sh` contra 2 em 8 sob
`rodada4.sh` — correlação com amostra pequena, e não uma causa medida. Quem for
consertar a causa 5 começa daqui, e não de uma conclusão que eu não tenho.

## `final29_driver.log` — as cinco rodadas do aceite, refeitas sobre este arquivo

A tabela das cinco rodadas que a §29 publicava foi medida sobre a versão
anterior do módulo, o que este mesmo arquivo declarava inválido duas seções
acima — e mesmo assim a §29 continuava citando. Refeitas aqui, com o hash
impresso no próprio registro, gerador `motor/final29.sh`.

| rodada | saída | parede | conjuntos | testes | `acceptance_m5` | `acceptance_m2` |
| --- | --- | --- | --- | --- | --- | --- |
| `verde10_1` | 0 | 260,49 s | 73 | 1.883 | 15/15 em 22,97 s | 3,20 s |
| `verde10_2` | 0 | 260,06 s | 73 | 1.883 | 15/15 em 22,96 s | 3,21 s |
| `verde10_3` | 0 | 260,46 s | 73 | 1.883 | 15/15 em 23,01 s | 3,22 s |
| `verde10_4` | 0 | 261,18 s | 73 | 1.883 | 15/15 em 23,06 s | 3,23 s |
| `verde10_5` | 0 | 259,76 s | 73 | 1.883 | 15/15 em 23,00 s | 3,22 s |

Cinco de cinco, saída 0, **zero linhas `FAILED` nos cinco registros**, com carga
média entre 9,9 e 20,5 no minuto de cada rodada. As cinco correm sob o
`rodada5.sh` — a carga que está **dentro** do alcance do conserto, que é a que o
critério 1 pede.

Duas coisas nesta tabela valem mais que o «saída 0»:

- O `acceptance_m5` fecha em **22,96 – 23,06 s com 15 de 15** nas cinco rodadas.
  É o número da **série** medido em 2026-08-31 (23,02 s), com dez centésimos de
  margem, e não o do paralelo (~20,01 s, encostado no prazo). A permissão está
  agindo, e está agindo nas cinco.
- A dispersão de parede entre as cinco é de **1,4 s** (259,76 a 261,18). A §29
  descrevia uma suíte cujo resultado era uma distribuição; aqui ela voltou a ter
  um tempo.

## O backup, agora dentro do repositório

A quarta revisão independente apontou que o backup contra o qual a restauração
era conferida — `/tmp/o29/vaga_ARMADO7.backup.rs` — não estava no repositório.
Uma conferência byte-a-byte contra um arquivo em `/tmp` não é reproduzível por
terceiros, e `/tmp` é apagado sem aviso: é o mesmo motivo pelo qual os registros
brutos foram trazidos para cá, aplicado tarde demais ao backup. Ele está agora
em `arquivo-entregue/vaga_ARMADO7.backup.rs`, e é byte-a-byte o arquivo
entregue:

```
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  crates/seele-conformance/tests/vaga/mod.rs
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  arquivo-entregue/vaga_ARMADO7.backup.rs
```

## A árvore apanhada desarmada, e o que isso ensina do método

A mesma revisão encontrou a árvore com `const VAGAS: usize = usize::MAX;` — a
permissão **desligada** — e reprovou a entrega por isso. Ela estava certa: com
aquele valor, `cargo clippy` reprova em `absurd_extreme_comparisons`, a
serialização não existe, e toda esta evidência descreve um arquivo que não é o
que está na árvore.

O que aconteceu não foi esquecimento de uma edição manual: o
`motor/custo8.sh` **desarma, mede três rodadas e restaura do backup**, e a
sessão anterior terminou com ele em voo. A revisão fotografou o meio do ciclo. O
script terminou depois e restaurou sozinho — `custo8_driver.log` registra o
`RESTAURADO: identico byte-a-byte ao backup` e a conferência final `IDENTICO`.

A lição não é «foi só um susto», é de método: **um gerador que edita a árvore de
trabalho deixa a entrega refém do seu próprio relógio.** Quem for medir esta
pendência de novo confere `const VAGAS` e o SHA-256 da árvore contra o backup
**antes** de dar qualquer coisa por entregue. É uma linha de `shasum`.

## `custo8_driver.log` — o custo com as duas metades sob a mesma carga

As rodadas desarmadas que existiam (`revws7_1..3`) correram sob o `rodada4.sh`,
mais pesado que o `rodada5.sh` das rodadas armadas do aceite: comparar parede
entre elas compara duas cargas, não duas permissões. Refeito com as duas metades
sob `rodada5.sh` e sobre o arquivo entregue:

| | parede do `--workspace` |
| --- | --- |
| desarmada (`custo8_desarmada_1..3`) | 204,59 / 190,27 / 181,95 s — média **192,3 s** |
| armada (`verde10_1..5`) | 259,76 – 261,18 s — média **260,4 s** |
| **custo da serialização** | **+68,1 s (+35 %)** |

## `custo8_pos` — a sexta rodada armada, que reprovou

A rodada de referência armada do `custo8.sh`, logo depois da restauração
conferida por hash, saiu **101** em 160,74 s:

```
     Running tests/moderacao.rs
Error: SemResposta
test um_operador_modera_pessoas_e_nao_o_comandante ... FAILED
test result: FAILED. 5 passed; 1 failed; ... finished in 21.28s
```

É a assinatura da §29. O registro **não** contém o aviso de fila abandonada,
logo a permissão estava agindo. E o conjunto **passa sozinho sob a mesma carga**
(`motor/moderacao3.sh`): 1,48 / 1,39 / 1,45 / 1,40 s, saída 0, quatro de quatro
(`moder_so_*.resumo.log`). Não é instabilidade do teste: é contenção residual —
a permissão serializa a conformidade contra ela mesma, e não contra os ~489
testes do `seele-server` e os binários de áudio e vídeo rodando ao lado, que o
critério 2 exige que continuem em paralelo.

A taxa honesta desta entrega, portanto, é **1 reprovação em 6 rodadas armadas**
sob a carga dentro do alcance do conserto, contra cerca de **2 em 3** antes
dele. A §29 está muito reduzida; não está eliminada.
