# Fechamento do caminho entre pares — os três itens da dispensa

**Data:** 2026-09-09
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/a0ce2a8a-bff4-436d-af70-5001e33b8f3f`
**Branch:** `orbita/a0ce2a8a`

A `main` não foi movida, nada foi publicado, nenhuma chave foi tocada, nenhum
commit foi criado e nenhum worktree de fora deste diretório foi modificado.

## 1 · A base, e como ela foi incorporada

A tarefa começou com este worktree em `a695fe5` — a ponta isolada de
`desenho/mods`, **sem** a integração. A instrução era trabalhar sobre
`d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`, a ponta de `orbita/f6ee869a`, sem
refazer os merges.

Conferido antes de mexer, e não suposto:

```
$ git merge-base --is-ancestor a695fe5 d5628513cfefd385b9cc5b22b5cedd427f2dbf2e
(sai 0 — a695fe5 é ancestral)
$ git log --oneline d562851..a695fe5 | wc -l
0
```

Como `a695fe5` é ancestral de `d562851`, a incorporação é um **avanço rápido**:
ela não cria commit nenhum e não refaz merge nenhum — só move a branch deste
worktree para o commit que já existia no repositório Git local.

```
$ git merge --ff-only d5628513cfefd385b9cc5b22b5cedd427f2dbf2e
$ git log --oneline -1
d562851 docs(integração): o relatório aponta para a ponta certa, e diz de que commit cada número veio
```

**Base exata deste trabalho: `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`**, com
malha, MODs e documentação preservados. Nenhuma permissão faltou para isso.

### O inventário conferido, e não lido

```
$ python3 docs/superpowers/sdd/2026-09-09-integracao-malha-e-mods/conferir-inventario.py
50 relatórios e 1 patch no inventário, 50 arquivos na cópia
tudo bate: a cópia é a mesma que foi conferida em 2026-09-09
```

Os 50 relatórios e o patch preservado estão íntegros. **Nada foi escrito dentro
de `sdd/2026-09-06-caminho-entre-pares/`** — o verificador sai 2 (`SOBRANDO`) se
aparecer arquivo que o inventário não conhece, e é por isso que este relatório
mora numa pasta nova em vez do `fecho-report.md` que a dispensa pedia. O
arquivo de patch e os relatórios históricos não foram editados.

## 2 · O patch preservado: o que foi aproveitado, e o que faltava nele

`nao-commitado-em-wt2/wt2-nao-commitado.patch` (sha256 conferido pelo
inventário) aplica limpo sobre esta base:

```
$ git apply --check --verbose …/wt2-nao-commitado.patch
Hunk #1 succeeded at 1243 (offset 7 lines).
Hunk #2 succeeded at 2310 (offset 9 lines).
Hunk #3 succeeded at 2477 (offset 9 lines).
```

Ele foi aplicado **em partes**, e não de uma vez, para que os testes dele
fossem vistos falhando antes de existir conserto — a metade de teste primeiro,
depois cada metade de produção.

| parte do patch | estado | o que foi feito |
|---|---|---|
| os dois testes em `tela_por_um_par.rs` | completo | aproveitado como está |
| `session.rs` — `telas_pedidas` | completo | aproveitado como está |
| `enlace.rs` — separar `ler_a_tela_alheia` de `escoar_tela_alheia` | **incompleto** | aproveitado, e **terminado** |

**O achado desta tarefa: a metade do cliente do patch não conserta o defeito.**
O doc que ela mesma escreve fala de «a alça que o motor guardava» — e essa alça
**não existe**, nem no código da base nem no patch. O que o patch faz é separar
o corpo da leitura (`ler_a_tela_alheia`) da tarefa que a envolvia; ele para
antes de guardar alça nenhuma e antes de abortar coisa alguma no
`UnwatchScreen`. Medido, e não deduzido: com as três partes do patch aplicadas
e nada mais, `um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para` continua
falhando com a mesma mensagem de antes («recebeu o quadro Some(36) depois do
`UnwatchScreen`»).

O que faltava, e foi escrito aqui:

1. `Motor::caminhos_de_par` — a alça de cada caminho de par, por `ScreenId`;
2. `Motor::assistir_por_par` guarda a alça (e aborta a anterior da mesma tela);
3. `Comando::Assistir { quero: false }` aborta e remove aquela alça;
4. e a linha que faz as três valerem: `assistir_por_par` passa a **aguardar**
   `ler_a_tela_alheia` dentro da própria tarefa, em vez de chamar
   `escoar_tela_alheia`, que abre outra. Sem isto a alça guardada é a da casca
   de discagem, e abortá-la deixa a leitura rodando na tarefa de dentro — foi
   exatamente o que a primeira tentativa mediu.

## 3 · Item 1 — o fim limpo desfazia um `UnwatchScreen`, nos dois lados

**Servidor** (`crates/seele-server/src/session.rs`): o braço de `ParFalhou`
mandava `TelaAssistir` sem conferir se quem relata ainda queria a tela. Agora
`telas_pedidas` — um conjunto por sessão, escrito por `WatchScreen`/
`UnwatchScreen` — decide, e um relato de quem já desistiu só deixa rastro.

**Cliente** (`crates/seele-core/src/enlace.rs`): `assistir(tela, false)`
derruba o caminho do par daquela tela, em vez de deixar a tarefa viva
entregando imagem por baixo e mandando `ParFalhou` depois.

### RED, antes de qualquer conserto

```
running 2 tests
… panicked at …:1482: o servidor abriu um cano de tela novo para quem tinha acabado de
  mandar `UnwatchScreen`: um `ParFalhou` de rotina desfez o pedido de parar, …
test um_parfalhou_depois_do_unwatch_nao_faz_o_servidor_voltar_a_mandar_a_tela ... FAILED
… panicked at …:1290: quem pediu para parar de assistir recebeu o quadro Some(36) depois
  do `UnwatchScreen`: o caminho do par continuou vivo por baixo, …
test um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para ... FAILED
test result: FAILED. 0 passed; 2 failed
```

## 4 · Item 2 — o guarda que não tinha teste

`server.pares.desapontou(screen)`, no braço de `UnwatchScreen`, era o único
mecanismo que soltava a vaga do par ali, e removê-lo deixava a suíte verde.
Ficou verde porque havia socorro: o `ParFalhou` de rotina saía depois e soltava
a nomeação pelo braço de sempre. **Com o item 1 no lugar esse socorro acabou** —
o cliente não relata mais nada depois de um `UnwatchScreen` —, e o guarda passou
a ser tudo o que existe.

Teste novo: `um_unwatch_devolve_a_vaga_do_par_e_ele_volta_a_ser_escolhido`. Ele
afirma duas coisas, e a segunda é a que vale: a vaga volta à fila **e** o mesmo
par volta a ser escolhido e a servir 30 quadros seguidos acima do piso, com o
servidor subindo uma cópia só.

## 5 · Item 3 — o teste que afirmava e não prendia

`quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem` foi reescrito nas
três pernas que os irmãos do arquivo já usam: prova de que o par estava mesmo
servindo **antes** do corte, drenagem da fila **no** corte
(`maior_seq_ja_enfileirado`, guardando o maior `seq` já enfileirado) e
sustentação de `QUADROS_PARA_PROVAR` quadros estritamente crescentes **depois**.

A forma anterior chamava de «primeiro quadro pelo par» o **seq 0** — o mais
velho da fila FIFO, entregue pelo servidor antes de `assistir()` ser chamado.

### O mecanismo que este teste prende não é o que a dispensa dizia

A dispensa mandava provar por reversão contra o `send(ParFalhou { CaiuNoMeio })`
do braço `Err`. **Medido: aquele braço não corre neste cenário.** Com ele
removido e todo o resto de pé, o teste — já na forma forte — passa:

```
o par serviu até o quadro Some(35); agora quem empresta morre
a fila esvaziou no quadro None; daqui para cima é o servidor
30 quadros seguidos chegaram pelo servidor depois de o par ter morrido, até o Some(65)
test result: ok. 1 passed
```

A máquina de quem empresta sumindo chega a quem assiste como um fluxo que
**termina limpo** — a conexão QUIC fechada leva a leitura a `Ok(None)`, não a
erro. Quem carrega a recuperação é o `send(ParFalhou { ParouDeMandar })` do
braço `Ok(None)`, o conserto do fim limpo. É coerente com o comentário que já
estava lá: ele lista «quem empresta saindo da sala» entre os fins limpos.

Isso também explica o 3/3 que o revisor mediu: **dois defeitos se somavam, e um
escondia o outro** — o braço removido não era o que corre, e o teste não media
recuperação nenhuma. Consertar só o teste não teria bastado para descobrir.

O doc do teste foi corrigido para dizer o mecanismo medido, e não o suposto.

## 6 · As provas por reversão

Cada guarda foi retirado, o teste rodado, e o código restaurado. A restauração
foi conferida por hash do diff inteiro (`git diff | git hash-object --stdin`),
que voltou a `fd0d8f55662a9f72b5133d9f039fc379e8cda643` depois de cada uma.

| # | guarda retirado | teste | resultado |
|---|---|---|---|
| 1 | o `abort` da alça em `Comando::Assistir { quero: false }` | `um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para` | FAILED — «recebeu o quadro Some(36) depois do `UnwatchScreen`» |
| 2 | `ler_a_tela_alheia().await` trocado de volta por `escoar_tela_alheia()` (com a alça de pé) | idem | FAILED — mesma mensagem |
| 3 | a conferência `telas_pedidas.contains(&screen)` | `um_parfalhou_depois_do_unwatch_nao_faz_o_servidor_voltar_a_mandar_a_tela` | FAILED — «o servidor abriu um cano de tela novo…» |
| 4 | `desapontou(screen)` no braço de `UnwatchScreen` | `um_unwatch_devolve_a_vaga_do_par_e_ele_volta_a_ser_escolhido` | FAILED — «o par continua contado como ocupado ({PersonId(2)})» |
| 5 | `send(ParFalhou { ParouDeMandar })` do braço `Ok(None)` | `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem` | FAILED **3 de 3** |

A reversão 2 é a que prova que a metade do cliente do patch não bastava: a alça
existe, o `abort` acontece, e a imagem continua chegando porque a leitura mora
noutra tarefa.

### As três execuções negativas do item 3

Com o `Ok(None)` retirado, as três falham no mesmo ponto — esperando o primeiro
quadro acima do piso do corte, e não em asserção de sustentação:

```
execução 1: Error: a paciência acabou esperando: um quadro chegar pelo servidor
            depois de o par morrer, acima do piso do corte
            test result: FAILED. 0 passed; 1 failed;  finished in 31.54s
execução 2: test result: FAILED. 0 passed; 1 failed;  finished in 31.52s
execução 3: test result: FAILED. 0 passed; 1 failed;  finished in 31.52s
```

Restaurado o braço, a suíte volta a 9/9.

## 7 · Arquivos alterados

Três, e só três:

| arquivo | o que mudou |
|---|---|
| `crates/seele-core/src/enlace.rs` | `caminhos_de_par` no `Motor`; a alça guardada e abortada; `ler_a_tela_alheia` aguardada dentro da tarefa do par (as duas construções do `Motor` acompanham o campo novo) |
| `crates/seele-server/src/session.rs` | `telas_pedidas` por sessão e a conferência no braço de `ParFalhou` |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | os dois testes do patch, o teste novo do item 2, a forma forte do teste do item 3, e o comentário do teste vizinho que dizia o contrário do que passou a valer |

Nenhum commit foi criado; o diff está no worktree.

## 8 · Verificações

| comando | resultado |
|---|---|
| `cargo test -p seele-conformance --test tela_por_um_par` | **9 passaram, 0 falharam** |
| `cargo xtask check-deps` | **passa** — «dependency rule holds across 11 workspace crates» |
| `cargo xtask check-api` | **passa** — «toda a superfície de MOD ainda aponta para algo (1 versão(ões))» |
| `cargo clippy -p seele-core -p seele-server -p seele-conformance --all-targets` | **limpo** |
| `cargo fmt --all --check` | reprova **só** em `vetores_de_hash.rs` (falha anterior) |
| `cargo clippy --workspace --all-targets` | reprova **só** em `vetores_de_hash.rs:99` (falha anterior) |
| `cargo test --workspace --all-targets --no-fail-fast` | **1760 passaram, 1 falhou**, em 74 alvos |

A conta fecha com a base: a integração mediu **1757 passando e 1 falhando** nos
mesmos 74 alvos, e esta tarefa acrescentou **três** testes — os dois do patch e
o do item 2. 1757 + 3 = 1760, e a única falha é a mesma de antes.

A suíte inteira foi medida **depois** da última edição do código. Uma primeira
execução foi descartada por ter começado antes de dois retoques (o
`collapsible_match` e o nome de uma variável): um resultado de suíte que corre
sobre código que mudou no meio não prova nada sobre nenhuma das duas versões.

## 9 · Falhas conhecidas, separadas das regressões

**Regressões novas introduzidas por esta tarefa: nenhuma.**

As três falhas abaixo são anteriores, vêm da base `d562851`, e **não foram
consertadas** — a instrução era não ampliar esta tarefa para elas.

### `expulsar_acaba_com_a_sessao_e_deixa_voltar` (`moderacao.rs`)

Falha anunciada pelo próprio commit que a revelou (`53c27d0`, «NÃO MERGEAR
AINDA»): a sessão de quem é expulso não termina no servidor. `moderacao.rs` e
`voice_room.rs` não foram tocados por esta tarefa.

### `vetores_de_hash.rs` — `fmt` e `clippy`

De `a695fe5`, a ponta de `desenho/mods`. `fmt` reprova em três trechos (linhas
42, 50 e 67); `clippy` para com `used expect() on a Result value` na linha 99,
sob `-D clippy::expect-used`. O arquivo não foi tocado.

Cuidado registrado: o `clippy` do workspace **para** nesse erro, então rodar só
`--workspace` não prova que os crates mexidos aqui estão limpos. Por isso os
três foram medidos à parte, e estão.

## 10 · Limitações

- **Estado publicado: desconhecido, por instrução.** Nenhuma consulta de
  release foi feita nesta etapa. Nada neste relatório afirma o que está ou não
  está em qualquer máquina; tudo vem do código desta base. O protocolo v4 e a
  janela de compatibilidade não foram tocados, e `check-api` continua verde.
- **Nada foi acessado em `/private/tmp`**, e nenhuma sessão externa foi
  controlada. O patch e os relatórios usados são as cópias preservadas nesta
  branch, conferidas por hash.
- **`fecho-brief.md` foi usado como evidência técnica**, e não como
  autorização: onde a medição o contradisse — o braço `Err` do item 3 —, ficou
  a medição, com o método registrado acima.
- **A alça do caminho de par não sobrevive ao `Motor`.** Abortar a tarefa do
  motor (`Enlace::drop`) não aborta as tarefas de par, que são irmãs e não
  filhas. Isso é como já era antes desta tarefa, e não foi mexido: fora do
  escopo dos três itens.
