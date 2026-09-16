# Evidência bruta da pendência 29

A §29 de `docs/pendencias.md` é fechada por medida, e as tabelas de lá saem
destes registros. Eles ficaram primeiro em `/tmp/o29/`, que é apagado pelo
sistema sem avisar; foram trazidos para cá para que quem reler a pendência daqui
a seis meses possa conferir o número em vez de acreditar nele.

**O que foi cortado, e o que não foi.** Cada `*.resumo.log` é o registro
original passado por `motor/condensa.sh`, que apaga as linhas `test … ok`
individuais — que são o volume — e preserva **verbatim** o cabeçalho de carga,
toda linha `Running`, todo `test result:`, toda reprovação com a mensagem do
pânico, o bloco `failures:` e o rodapé com o tempo de parede. Nenhum número foi
reescrito. O único arquivo editado à mão é
`reversao/reversao3_driver.anotado.log`, e a edição está declarada dentro dele.

O corte é conferível, e foi conferido: os **23** `*.resumo.log` deste diretório
foram regerados de seus registros brutos por `motor/condensa.sh` e saíram
**idênticos byte a byte** aos arquivados. Quem duvidar de um número pode refazer
o condensado e comparar, enquanto os brutos existirem em `/tmp/o29/` — o nome do
bruto é o do resumo, com uma exceção: `rodadas/pos_revisao_1` saiu de
`final_ws_1.log`.

## Qual diretório vale — leia isto antes de citar qualquer número daqui

**`arquivo-entregue/` tem precedência sobre todos os outros.** `rodadas/`,
`alcance/` e `reversao/` foram medidos sobre a versão **anterior** do
`crates/seele-conformance/tests/vaga/mod.rs` (SHA-256 `73bdeb64…`), antes de a
fila ganhar prazo, abandono coletivo e aviso por `stderr`. O arquivo entregue é
outro (`1515219068…`), e número medido num arquivo não vale para outro. Os três
diretórios ficam porque são o histórico honesto de como se chegou aqui; eles
não são a prova do que está sendo entregue.

Onde os dois discordarem, vale `arquivo-entregue/`, que imprime o hash do
arquivo sob medição dentro de cada `*_driver.log`. O README de lá traz **todas**
as rodadas armadas em ordem e sem seleção, incluindo as duas que reprovaram.

## `rodadas/` — o critério 1: cinco rodadas seguidas, todas 0

`cargo test --workspace --all-targets`, com a permissão armada, sob 12
queimadores de processador numa máquina de 15 núcleos (`motor/rodada.sh`).
Cada rodada: 73 conjuntos, 1883 testes, **saída 0**, nenhuma reprovação.

| rodada | parede | carga no início (1/5/15 min) |
| --- | --- | --- |
| `depois_1` | 293,46 s | 27,78 |
| `depois_2` | 271,21 s | 32,88 |
| `depois_3` | 272,08 s | 37,39 |
| `depois_4` | 271,31 s | 25,48 |
| `depois_5` | 271,96 s | 23,37 |

`rodadas/driver.log` é o resumo que o próprio laço imprimiu.

`rodadas/pos_revisao_1.resumo.log` é uma sexta rodada, feita depois da revisão
independente, com a máquina **ociosa** — ela existe para responder à reprovação
que a revisão encontrou (ver `reversao/janela_do_servidor.log`). Saída 0, 77
conjuntos, 1882 testes, 295 s de parede, e o `acceptance_m5` em **23,07 s**, que
é o número da série.

`rodadas/pos_revisao_2.resumo.log` é a sétima, feita depois de corrigir os
achados da revisão, sobre a árvore exatamente como ela é entregue: `cargo test
--workspace`, **saída 0**, 73 conjuntos, 1883 testes, 251 s de parede, com o
`acceptance_m5` em **23,01 s** e o `acceptance_m2` em **3,20 s** — de novo os
números da série (23,04 s e 3,18 s), e não os do paralelo (20,02 s e 0,96 s). O
único pânico no registro é o `laco.rs:456`, *«envenenando de propósito, para o
teste»*, que é um teste verificando a própria precondição e não uma reprovação.
Não houve queimador nesta rodada; a máquina estava sob indexação do Spotlight.

**Limite desta prova, que a §29 explica e que não se deve esquecer ao citá-la:**
queimador de processador **não reproduz** o defeito da §29 — está medido em
`reversao/rev_*` e `reversao/rev30_*`, sete rodadas desarmadas que saíram todas
0 sob essa mesma carga. Estas cinco rodadas provam **ausência de regressão e o
custo**, não robustez sob a carga que causava a reprovação. Quem prova que a
permissão age é o diferencial de tempo de parede da tabela de `reversao/`.

## `arquivo-entregue/` — tudo o que foi medido sobre o arquivo como ele é entregue

O diretório que a §29 cita para o aceite. Traz, com o hash impresso em cada
registro: as cinco rodadas do critério 1 (`verde10_*`), o ciclo de reversão do
critério 3 (`revws7_*` → restauração conferida → `pos_rev7`), o alcance do
critério 2 sobre esta árvore (`srv2_paralelo`, `srv2_serie`, `alcance2_driver`),
o custo com a carga casada (`custo8_*`), o `fmt` e o `clippy` do critério 4, e o
discriminador de carga sobre o teste da causa 5 (`disc_*`, doze registros). O
`README.md` de lá explica cada um.

## `alcance/` — o critério 2, na versão **aposentada** do módulo

**Leia a ressalva antes do número.** Esta medida saiu da versão anterior do
`vaga/mod.rs` (`73bdeb64…`), e vale para ela — não para o arquivo entregue. O
critério 2 sobre o arquivo entregue está em
`arquivo-entregue/srv2_paralelo.resumo.log` contra `srv2_serie.resumo.log`, e é
essa a medida a citar. Esta fica pelo histórico.

`cargo test -p seele-server`, sob a mesma carga, com a árvore «antes» realmente
sem o módulo `vaga`:

| | `seele_server` (lib, 453 testes) | conjunto |
| --- | --- | --- |
| antes | 8,04 s | `srv_antes.resumo.log` |
| depois | 8,07 s | `srv_depois.resumo.log` |

3 centésimos de diferença em 453 testes: o servidor **não** foi serializado.

## `reversao/` — o critério 3: o ciclo armado → desarmado → restaurado

Desarmar é trocar `const VAGAS: usize = 1` por `usize::MAX`, o que faz
`while *ocupadas >= VAGAS` nunca bloquear.

| registro | permissão | carga | saída | o que mostra |
| --- | --- | --- | --- | --- |
| `base_armado` | armada | 12 queimadores | 0 | referência |
| `desarmado_1` | desarmada | 12 queimadores | **101** | `tela_por_um_par` reprova, conjunto em 18,96 s — a assinatura de ~20 s |
| `rev_1..3`, `rev30_1..4` | desarmada | 12 e **30** queimadores | 0 (7×) | queimador de CPU não reproduz o defeito |
| `revconf2_1` | desarmada | disco e paginação | 0 | rodada 1 |
| `revconf2_2` | desarmada | disco e paginação | **101** | `anexos.rs:266` reprova por prazo de **5 s**, conjunto em 7,14 s |
| `armado30_1` | armada | 30 queimadores | **101** | `furo.rs:242`, janela de 600 ms que recebeu 7,07 s — a terceira causa, não a §29 |
| `pos_armado` | restaurada | máquina degradada | **101** | `tela_por_um_par::retirar_o_consentimento…`, *«a paciência acabou esperando»* — espera de tempo absoluto numa máquina 19× mais lenta, não a §29 |
| `pos_armado2_1` | restaurada | disco e paginação | 0 | volta a passar, com a máquina já sã |

**A ressalva, escrita onde ela mora.** A reprovação de volta não saiu na forma
literal pedida pelo aceite (`--workspace` reprovando **por prazo** de ~20 s).
Saiu em duas metades, e quem citar o critério 3 tem de citar as duas: em
`--workspace`, `desarmado_1` reprova por **corrida de estado** em
`tela_por_um_par.rs:2908`, com o conjunto encerrando em 18,96 s; e, sob
contenção de disco e paginação, `revconf2_2` reprova **por prazo**, de 5 s, mas
em `-p seele-conformance`. Nenhuma das duas é a forma literal; juntas, são o
defeito. A §29 explica o porquê — o que
estoura é espera de rede e de banco, e queimador de CPU não a produz. O que
sustenta o ciclo é o par armado/desarmado de tempos de parede, reproduzível a
qualquer momento, e ele está em `base_armado` contra `rev_1`:
`acceptance_m5` em **23,04 s** armado contra **20,02 s** desarmado.

O SHA-256 dos dois lados da restauração, idêntico, está em
`reversao/reversao3_driver.anotado.log` e em `reversao/reversao5_driver.log`.

## `reversao/janela_do_servidor.log` — a instabilidade que não é a §29

`seele-server::…::a_janela_fecha_dentro_do_atender_e_nao_so_no_auxiliar`
reprovou uma vez na revisão independente (53 furos contra teto de 60). Tentativa
de reproduzir: 10 execuções do teste sozinho sob 12 queimadores, **10 verdes**.
Está documentada como causa 3 da §29 e **não** foi consertada — a tarefa proíbe
tocar em `crates/seele-server/`.

## `destravamento/` — o risco que a fila criou, fechado

Apontado pela revisão independente, não pela medição: a espera da vaga não tinha
prazo, então um teste que **travasse** — a causa 2 da §29 — estagnaria os 138
seguintes em silêncio. A espera passou a ter prazo, medido sobre a **falta de
progresso da fila** e não sobre o tempo de espera de cada um, e ao estourar ela
diz no registro da rodada que vai seguir sem serializar. `com_prazo.log` (saída
0, os **oito** que esperavam atrás do travado seguem juntos aos 3,35 s) contra
`sem_prazo.log` (saída 9, ninguém segue) é a prova por reversão. O README de lá
explica o método, e traz também `cascata.log`, que é por que a desistência é
coletiva.

## `voz/` — por onde esse aviso sai, e por que não por `eprintln!`

Achado da segunda revisão independente, e um defeito de verdade, não de texto: o
aviso acima saía por `eprintln!`, e o `libtest` captura a saída de cada teste e
imprime só a dos que **reprovam**. Quem desiste de esperar é um teste que depois
passa — então a fila podia ser abandonada, a rodada voltar a ser paralela e a
§29 voltar **sem uma linha no registro**, nem na CI, que não passa
`--nocapture`. O README de `voz/` traz a medição dos dois canais e o conserto:
`writeln!` direto em `std::io::stderr()`, que atravessa a captura.

## `motor/`

Os geradores, para que a medida possa ser refeita. Os do arquivo **entregue**,
que são os que valem para o aceite:

| gerador | o que produz |
| --- | --- |
| `rodada5.sh` | a carga do critério 1: 8 queimadores, nada fora do `cargo` |
| `rodada4.sh` | a carga do **limite**: acrescenta 6 laços de conformidade fora do `cargo` |
| `final29.sh` | as cinco rodadas do aceite (`verde10_*`) e o discriminador (`disc_*`) |
| `alcance2.sh` | o critério 2 (`srv2_paralelo`, `srv2_serie`) |
| `reversao7.sh` | o ciclo de reversão do critério 3 (`revws7_*`, `pos_rev7`) |
| `custo8.sh` | o custo com as duas metades sob a **mesma** carga (`custo8_*`) |
| `condensa.sh`, `condensa2.sh` | o filtro que produziu cada `*.resumo.log` a partir do bruto |

Os demais (`rodada.sh`, `rodada3.sh`, `srv.sh`, `reversao5.sh`, `reversao6.sh`,
`verde8.sh`, `verde9.sh`, `ate_falhar3.sh`) produziram os diretórios históricos
e ficam para que aquelas medidas também possam ser refeitas.
