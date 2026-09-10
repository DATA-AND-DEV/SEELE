# As tarefas de pares acabam junto com a conexão que as criou

**Data:** 2026-09-09
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/b326b0a6-ca2d-411b-a076-8f1705b1593b`
**Branch:** `orbita/b326b0a6`
**Base:** `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`

A `main` não foi movida, nada foi publicado, nenhuma chave foi tocada, nenhum
commit foi criado, nenhum worktree de fora deste diretório foi modificado e
nenhum relatório histórico foi editado.

---

## 1 · A base, e como ela foi incorporada

Este worktree começou em `a695fe5` — a ponta isolada de `desenho/mods`, sem a
integração. Conferido antes de mexer, e não suposto:

```
$ git merge-base --is-ancestor d5628513cfefd385b9cc5b22b5cedd427f2dbf2e HEAD
(sai 1 — não é ancestral)
$ git rev-list --count d562851..HEAD
0
```

`a695fe5` é ancestral de `d562851`, e não o contrário: a incorporação é um
**avanço rápido**. Ela não cria commit nenhum e não refaz merge nenhum — move a
branch deste worktree para um commit que já existia no repositório Git local.

```
$ git merge --ff-only d5628513cfefd385b9cc5b22b5cedd427f2dbf2e
$ git log --oneline -1
d562851 docs(integração): o relatório aponta para a ponta certa, e diz de que commit cada número veio
```

Nenhuma permissão faltou para isso. O inventário preservado continua íntegro
depois de tudo o que está abaixo:

```
$ python3 docs/superpowers/sdd/2026-09-09-integracao-malha-e-mods/conferir-inventario.py
50 relatórios e 1 patch no inventário, 50 arquivos na cópia
tudo bate: a cópia é a mesma que foi conferida em 2026-09-09
(sai 0)
```

## 2 · A importação, registrada à parte da correção nova

Os quatro arquivos não commitados do worktree `e250b652` foram conferidos por
SHA-256 **antes** de serem copiados, e os quatro bateram com o que a tarefa
mandou conferir:

| arquivo | sha256 | |
|---|---|---|
| `crates/seele-core/src/enlace.rs` | `d3f5b3fb…4d5935` | bate |
| `crates/seele-server/src/session.rs` | `bbb538b9…1de964` | bate |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | `20d06211…c47ff587` | bate |
| `docs/…/2026-09-09-fecho-caminho-entre-pares/relatorio.md` | `55e0673c…5e33602d` | bate |

Nenhuma diferença inesperada, e nada foi sobrescrito às cegas. A conferência
foi feita com `python3 -c` e `hashlib`, **não** com `shasum` — a sessão anterior
registrou que `shasum` exigia aprovação, e este pedido autoriza retomar a
tarefa, não conceder a permissão.

O estado imediatamente depois da importação, antes de qualquer linha nova, está
preservado em:

- **`00-importacao-e250.patch`** — a entrega anterior, exatamente como veio.

E o que esta tarefa acrescentou por cima dela, separado:

- **`01-fecho-tarefas-de-pares.patch`** — só a correção nova.

`crates/seele-server/src/session.rs` está **idêntico ao importado**: esta tarefa
não o tocou, e por isso ele não aparece no segundo patch.

## 3 · O defeito, e onde ele estava escrito

A limitação que o relatório importado deixou por escrito no §10 é exatamente
este escopo:

> **A alça do caminho de par não sobrevive ao `Motor`.** Abortar a tarefa do
> motor (`Enlace::drop`) não aborta as tarefas de par, que são irmãs e não
> filhas.

Investigado antes de consertar, e são **três** defeitos e não um:

### 3.1 · Uma alça largada desprende a tarefa; não a cancela

`Motor::caminhos_de_par` guardava `JoinHandle`s num `HashMap` cru. Os dois
caminhos que matam o motor largam esse mapa sem tocá-lo — `Enlace::drop` aborta
a tarefa de `Motor::rodar`, e um `Comando::Sair` a faz voltar —, e **largar um
`JoinHandle` desprende a tarefa em vez de cancelá-la**. Cada tarefa de par
continuava viva: lendo do par, com a conexão QUIC de pé, gastando a subida de
quem empresta por uma sessão que já tinha acabado.

E não em silêncio. A tarefa escreve no mesmo canal de avisos que o `Enlace`
continua segurando depois de `sair()`, então a casca recebia **quadro de tela
depois do `Encerrado` que ela mesma pediu**.

### 3.2 · A tarefa que serve um par não tinha alça em lugar nenhum

`Motor::servir_par` fazia `tokio::spawn` e jogava fora o `JoinHandle`. Ela não
tinha sequer o acidente que salvava as outras — descobrir no primeiro `send`
que ninguém escuta —, porque não fala com a casca: repassa bytes ao par até a
conexão morrer.

### 3.3 · A fila da conexão que caiu falava pela que a substituía

`resultados_do_par` não tem fundo, e o braço que o lê em `Motor::rodar` **só
existe quando há cliente**. Durante a bateria não há: tudo o que as tarefas de
par disseram fica parado na fila. Quando `Motor::tentar` põe um cliente novo no
lugar do que morreu, a primeira volta do laço entrega esses relatos **à conexão
nova** — um `ParFalhou` de uma nomeação que morreu com a sessão anterior,
mandando o servidor desfazer um caminho que ele acabou de montar para a
substituta. `Motor::cair` mexia em `tela_pedida`, na bomba e na sonda, e não
tocava em nada disto.

### O que **não** era defeito, e foi medido em vez de suposto

`escoar_tela_alheia` — a tarefa que lê a tela vinda do **servidor** — também é
solta e também não tem alça. Ela não precisa: a fonte dela é a conexão de
controle, e `Client::disconnect` fecha essa conexão, o que faz o fluxo dela
morrer. É a assimetria inteira do defeito: a conexão com o par **não** é fechada
por `disconnect`, e por isso só as tarefas de par sobreviviam.

## 4 · Os consertos

Três, todos em `crates/seele-core/src/enlace.rs`.

### 4.1 · `TarefasDePar` — um dono, e não um mapa

As alças passaram a morar atrás de um dono com `Drop`, que cancela tudo ao ser
solto. Ele guarda as duas coisas que existiam soltas: o caminho de cada tela
(por `ScreenId`) e a tarefa que serve um par.

A escolha é de **posse**, e não de disciplina: trocar «alguém tem de se lembrar
de cancelar» por «cancelar é o que acontece quando isto some». `Motor::encerrar`
não precisou de linha nenhuma — `rodar` devolve, o `Motor` é solto, e o dono vai
junto.

É a escolha oposta à de `TelaViva`, e o contraste ficou escrito no código: a
bomba tem um fim direito a esperar (o `Fim` que fecha o fluxo) e abortá-la
cortaria um quadro no meio; estas tarefas passam a vida paradas num `read` do
par, sem ponto onde conferir um pedido de parada.

### 4.2 · `VagaDeAtendimento` — a vaga é um punho, não duas escritas

**Este conserto existe por causa do anterior.** A vaga de `atendendo_pares` era
tomada no laço e devolvida na **última linha** do corpo da tarefa que serve —
e um `abort` faz a última linha nunca correr. Cancelar as tarefas fecharia um
defeito e abriria outro, pior: uma vaga que não volta é esta máquina **fora da
malha para sempre e sem erro em lugar nenhum** — o servidor continua apontando
este par, porque a vaga dele voltou na queda, e o cliente recusa cada pedido em
silêncio pela vaga que ficou.

Como guarda com `Drop`, devolver a vaga passou a ser o que acontece de todo
jeito: pelo fim do corpo, pelas saídas antecipadas de `servir_par`, e pelo
`abort`. `compare_exchange` no lugar de `load`+`store` faz conferir e tomar
virarem um ato só.

A ordem que o comentário antigo exigia — «devolvida depois do repasse, e não
depois da ligação» — está preservada: o guarda é ligado no topo do corpo da
tarefa e morre no fim dele.

### 4.3 · `Motor::largar_o_caminho_entre_pares` — a queda derruba o caminho

`Motor::cair` é o único caso em que o motor **sobrevive** ao corte, e por isso o
único em que alguém tem de pedir. Ele agora cancela as tarefas de par, desliga o
repasse, e **troca as duas pontas** de `resultados_do_par`.

Trocar o canal, em vez de drenar a fila, é o que fecha a corrida junto: uma
tarefa abortada ainda pode escrever entre o pedido de cancelamento e o `await`
em que ela morre, e drenar antes disso deixaria esse relato passar. Com as
pontas trocadas ela escreve para um recebedor que já não existe. O que vier
**depois** da queda continua chegando — quem for criado da reconexão em diante
clona a ponta nova.

### O que foi preservado, e conferido

`UnwatchScreen` (o `parar_de_assistir`, agora pelo dono), a proteção contra
`ParFalhou` tardio em `session.rs` (não tocada) e a recuperação por
`ParouDeMandar` no fim limpo continuam de pé: os nove testes anteriores passam
sem alteração nenhuma.

## 5 · Os testes novos

Quatro, e cada um afirma uma coisa só.

| teste | onde | o que prende |
|---|---|---|
| `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` | `tela_por_um_par.rs` | quadro de tela **depois** do `Encerrado`, com um segundo de silêncio provado contra quem empresta continuando a receber |
| `destruir_o_motor_aborta_as_tarefas_de_par_que_ele_guarda` | `enlace.rs` | a tarefa continuava correndo depois de o motor sumir |
| `cair_devolve_a_vaga_de_quem_estava_servindo_um_par` | `enlace.rs` | a vaga volta dentro de um segundo, e pode ser tomada de novo |
| `cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta` | `enlace.rs` | fila velha morre, atividade nova passa |

Os auxiliares de recepção sustentada foram reaproveitados, e o que se repetia
nos irmãos do arquivo virou `ate_o_par_estar_servindo`: piso drenado antes do
pedido, contador caindo para uma cópia, e `QUADROS_PARA_PROVAR` quadros
estritamente crescentes com o contador em uma cópia do primeiro ao último.

Um auxiliar novo, `drenar_depois_do_fim`, existe porque
`maior_seq_ja_enfileirado` **não serve depois de `sair()`**: ele para quando o
canal fica quieto, e com o caminho do par derrubado não sobra remetente nenhum —
`Enlace::proximo` passa a devolver `Encerrado` na hora, para sempre, e aquele
laço nunca desistiria.

### O teste que foi escrito, medido e descartado

Um quinto teste — «a saída devolve a vaga de quem empresta, e o próximo
espectador é servido pelo mesmo par» — **passou sem conserto nenhum**, e por
isso foi removido em vez de mantido como se provasse algo. A sonda que explicou
por quê:

```
SONDA: depois do sair(), o caminho do par ainda entrega? Some(44)
o mesmo par serviu o espectador novo por 30 quadros seguidos, até o Some(164)
```

O caminho órfão **sobrevive** à saída (é o que o primeiro teste prende), mas a
vaga de quem empresta volta assim mesmo dentro de poucos segundos, por um
caminho que não é o cancelamento. A vaga presa não se reproduz por ali; ela se
reproduz onde o motor sobrevive ao corte, e é lá que ela é provada.

## 6 · As provas por reversão

Cada guarda foi retirado, o teste rodado, e o código restaurado. A restauração
foi conferida por hash do diff inteiro (`git diff | git hash-object --stdin`),
que voltou a `a6e748b65e75ab94dddabe2accfd05825514bf14` depois de cada uma.

| # | guarda retirado | teste | resultado |
|---|---|---|---|
| 1 | `impl Drop for TarefasDePar` | `destruir_o_motor_aborta_as_tarefas_de_par_que_ele_guarda` | FAILED — «continuou correndo… (subiu de 28 para 56)» |
| 1b | idem | `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` | FAILED — «recebeu o quadro Some(45) depois do `Encerrado`» |
| 2 | `impl Drop for VagaDeAtendimento` | `cair_devolve_a_vaga_de_quem_estava_servindo_um_par` | FAILED — a vaga não voltou |
| 3 | `tarefas_de_par.servir(tarefa)` | `cair_devolve_a_vaga_de_quem_estava_servindo_um_par` | FAILED — a vaga não voltou |
| 4 | a troca das pontas de `resultados_do_par` | `cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta` | FAILED — «o relato da conexão que caiu continuou na fila» |
| 5 | a chamada a `largar_o_caminho_entre_pares` em `cair` | os dois testes de `cair` | FAILED **os dois** |

### A reversão 3 é a que mais ensinou, e ela falhou duas vezes

Na primeira tentativa, **retirar `tarefas_de_par.servir(tarefa)` deixava a suíte
inteira verde**: o teste da vaga registrava a tarefa ele mesmo, pela porta de
`TarefasDePar`, e nunca passava por `servir_par`. O guarda existia e não
funcionava — a forma exata que o `CLAUDE.md` deste repositório manda desconfiar.

O que fechou o buraco não foi mudar o teste: foi partir `servir_par` em duas.
Tudo o que precisa de um `Client` vivo ficou lá em cima; `Motor::passar_a_servir`
— que precisa só de uma ponta QUIC, uma identidade e endereços — ficou embaixo,
e é por ela que o teste entra. Com a costura de verdade no caminho, a mesma
reversão passou a falhar.

O discriminador desse teste é um prazo, e ele é escolhido e não arbitrário: o
alvo é um buraco negro (uma ponta bindada sem `ServerConfig`, que nunca
responde), contra o qual `servir_um_par` gasta `PRAZO_DO_PAR` inteiro — três
segundos — antes de desistir e devolver a vaga pelo fim do corpo. A vaga voltar
**dentro de um segundo** só pode ter vindo do cancelamento.

## 7 · Arquivos alterados por esta tarefa

| arquivo | o que mudou |
|---|---|
| `crates/seele-core/src/enlace.rs` | `TarefasDePar` e `VagaDeAtendimento`; `caminhos_de_par` virou `tarefas_de_par`; `servir_par` partida em duas com `passar_a_servir`; `largar_o_caminho_entre_pares` chamada por `cair`; três testes de unidade |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | o teste da saída voluntária, e os dois auxiliares que ele precisava (`ate_o_par_estar_servindo`, `drenar_depois_do_fim`) |

`crates/seele-server/src/session.rs` foi **importado e não tocado**.

Nenhum commit foi criado; o diff está no worktree, e os dois patches acima o
separam em importação e correção nova.

## 8 · Verificações

| comando | resultado |
|---|---|
| `cargo test -p seele-conformance --test tela_por_um_par` | **10 passaram, 0 falharam** (9 anteriores + 1 novo) |
| `cargo test -p seele-core --lib` | os três testes novos passam |
| `cargo xtask check-deps` | **passa** — «dependency rule holds across 11 workspace crates» |
| `cargo xtask check-api` | **passa** — «toda a superfície de MOD ainda aponta para algo (1 versão(ões))» |
| `cargo clippy -p seele-core -p seele-server -p seele-conformance --all-targets` | **limpo** |
| `cargo fmt --all --check` | reprova **só** em `vetores_de_hash.rs` (linhas 42, 50, 67 — falha anterior) |
| `cargo clippy --workspace --all-targets` | reprova **só** em `vetores_de_hash.rs` (`expect()` on a `Result`, falha anterior) |
| `cargo test --workspace --all-targets --no-fail-fast` | **1764 passaram, 1 falhou**, em 74 alvos |
| `conferir-inventario.py` | **sai 0** — os 50 relatórios e o patch preservados continuam íntegros |

A conta fecha com a base: a entrega importada mediu **1760 passando e 1
falhando** nos mesmos 74 alvos, e esta tarefa acrescentou **quatro** testes e
removeu zero. 1760 + 4 = 1764, e a única falha é a mesma de antes.

A suíte inteira foi medida **depois** da última edição do código e depois de
todas as reversões terem sido desfeitas — com o hash do diff conferido em
`a6e748b6`, o mesmo de antes da primeira reversão.

## 9 · Falhas conhecidas, separadas das regressões

**Regressões novas introduzidas por esta tarefa: nenhuma.**

As duas abaixo são anteriores, vêm da base `d562851`, e **não foram
consertadas** — a instrução era mantê-las separadas deste escopo.

- **`expulsar_acaba_com_a_sessao_e_deixa_voltar` (`moderacao.rs`)** — a sessão
  de quem é expulso não termina no servidor. Anunciada pelo próprio commit que a
  revelou (`53c27d0`, «NÃO MERGEAR AINDA»). `moderacao.rs` e `voice_room.rs` não
  foram tocados.
- **`vetores_de_hash.rs` — `fmt` e `clippy`** — de `a695fe5`, a ponta de
  `desenho/mods`. O arquivo não foi tocado.

Cuidado que continua valendo: o `clippy` do workspace **para** nesse erro, então
rodar só `--workspace` não prova que os crates mexidos aqui estão limpos. Por
isso os três foram medidos à parte, e estão.

## 10 · Limitações

- **A substituição da conexão é provada por unidade, e não ponta a ponta.** Os
  dois testes de `cair` montam o `Motor` à mão. Um teste de conformidade
  equivalente precisaria derrubar e ressubir o servidor na mesma porta — o que
  `bateria_interna.rs` sabe fazer — **com a transmissão de tela sobrevivendo ao
  reinício**, e quem compartilha nestes testes é um par cru que teria de refazer
  o aperto de mão inteiro. Não foi tentado, e a lacuna é esta: nada aqui prova
  que a reconexão de verdade, contra um servidor de verdade, não traz relato
  velho junto. O mecanismo está provado; a costura dele, não.
- **A destruição é provada pela posse, e não pela rede.** O teste da destruição
  mede que o dono cancela o que guarda. Que `Enlace::drop` solta esse dono é
  propriedade da linguagem, e não tem teste próprio — o caminho de rede
  equivalente já é coberto por `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`,
  que derruba quem empresta à força.
- **O corpo da tarefa que serve continua sem prova de unidade.** `passar_a_servir`
  é testada até o ponto de pôr a tarefa de pé e guardar a alça; o que ela faz
  contra um par de verdade (`servir_um_par` + `repassar_a_tela`) é provado em
  `seele-conformance` e nos testes de `crate::par`.
- **Estado publicado: desconhecido, por instrução.** Nenhuma consulta de release
  foi feita. Nada neste relatório afirma o que está ou não está em qualquer
  máquina; tudo vem do código desta base. O protocolo v4 não foi tocado, e
  `check-api` continua verde.
- **Nada foi acessado em `/private/tmp`**, nenhuma sessão externa foi
  controlada, e nenhum comando negado na sessão cancelada foi repetido. A
  conferência de SHA-256 foi feita com `python3`/`hashlib` em vez de `shasum`;
  nenhuma permissão foi alterada e nenhum bloqueio foi contornado.
- **Nenhum impedimento por permissão nesta tarefa.** Tudo o que era necessário
  coube no que já estava autorizado.

## 11 · Como aproveitar o que ainda não está commitado

Todo o trabalho está no worktree, sem commit. Três formas, da mais direta à mais
granular:

**1 · Usar o worktree como está.** Ele já tem a base `d562851` mais tudo. É o
caminho sem intermediário.

**2 · Levar o conjunto inteiro para outro lugar** (a partir de `d562851`):

```sh
git diff > /onde/quiser/fecho-tarefas-de-pares.patch
# e do outro lado, com HEAD em d562851:
git apply /onde/quiser/fecho-tarefas-de-pares.patch
```

Os arquivos novos de `docs/superpowers/sdd/2026-09-09-fecho-*/` não entram num
`git diff` (são não rastreados) e precisam ser copiados à parte.

**3 · Separar importação de correção nova**, que é o que os dois patches desta
pasta existem para permitir:

```sh
# com HEAD em d562851 e a árvore limpa:
git apply docs/superpowers/sdd/2026-09-09-fecho-tarefas-de-pares/00-importacao-e250.patch
git apply docs/superpowers/sdd/2026-09-09-fecho-tarefas-de-pares/01-fecho-tarefas-de-pares.patch
```

O primeiro reproduz a entrega `a0ce2a8a` exatamente como ela veio do worktree
`e250b652` — os três arquivos Rust, conferidos por SHA-256 antes de serem
copiados. O segundo é só o que esta tarefa acrescentou, e toca dois arquivos:
`enlace.rs` e `tela_por_um_par.rs`.

Aplicar só o `00` dá uma árvore que compila e passa 9/9 nos testes de pares —
foi assim que a linha de base deste relatório foi medida.

Os dois foram conferidos, e não supostos:

```
$ git apply --check --cached 00-importacao-e250.patch
(sai 0 — aplica limpo sobre d562851)
$ git apply --check -R 01-fecho-tarefas-de-pares.patch
(sai 0 — desfaz exatamente a árvore atual, ou seja: 00 + 01 a reproduzem)
```

O `01` usa caminhos relativos de propósito. A primeira versão dele saiu de um
`git diff --no-index` entre os dois worktrees e trazia o caminho absoluto do
`e250b652` gravado no lado `a/` — um patch que só se aplicaria nesta máquina, e
que a conferência acima pegou.
