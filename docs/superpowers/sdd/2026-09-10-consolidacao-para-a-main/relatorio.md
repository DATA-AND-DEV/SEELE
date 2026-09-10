# A candidata para a `main`, com o workspace verde pela primeira vez

**Data:** 2026-09-10
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/87002ad5-5bea-4479-87c9-583d842db506`
**Branch:** `orbita/87002ad5` · **Base:** `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`
**Ponta de quando este texto foi escrito:** `4f942e7`
**Ponta da entrega fechada:** `0ddead3` — ver o §9, escrito depois
**Consertos da revisão:** `172738c` (código de teste) e o commit deste texto,
que é a ponta atual — ver o §10

A `main` não foi movida, nada foi publicado, nenhuma tag foi criada, nenhum
`push` foi feito, nenhuma chave foi tocada, nenhuma permissão foi alterada,
nenhum worktree de fora deste diretório foi modificado e nenhum relatório
histórico foi editado.

---

## 1 · O que a candidata é

Quatro commits sobre `d562851`, e a fronteira entre «o que veio de lá» e «o que
foi feito aqui» está no log e não só aqui:

| commit | o que é |
|---|---|
| `a839003` | **importação** — os conteúdos que quatro tarefas deixaram sem commit em `94b614e6` |
| `74056b8` | conserto: `fmt` e `clippy` do workspace paravam em `vetores_de_hash.rs` |
| `7079d6b` | conserto: quem é expulso não volta sozinho pela bateria interna |
| `4f942e7` | conserto: a queda que ninguém avisa também derruba o caminho entre pares |

## 2 · A cobertura, conferida e não suposta

`a695fe5` é ancestral de `d562851`, então a base entrou por **avanço rápido**:
nenhum merge refeito, nenhum commit de fusão novo.

As quatro frentes que o coordenador observou estão **todas contidas na
candidata**, por ancestralidade:

```
main                             (15a0406)  COBERTO
malha/caminho-entre-pares        (004322a)  COBERTO
bug/ficha-da-conexao-abandonada  (53c27d0)  COBERTO
desenho/mods                     (a695fe5)  COBERTO
```

E não só elas. Varridos **todos** os 40 ramos locais e os remotos:

```
$ git for-each-ref refs/heads/ refs/remotes/ | while read b; do
      git rev-list --count "HEAD..$b"; done | sort -u
0
```

Zero commits exclusivos em qualquer ramo do repositório — inclusive nos 34
`worktree-agent-*` e em `varredura-0035`/`origin/main` (`17c6421`). Nada foi
mergeado às cegas: nada precisou ser mergeado. A pilha de `stash` está vazia.

**O publicado está contido:** o último release é `v0.10.5-1`, do commit
`12a6401a6` (05/09), e ele é ancestral desta ponta — 90 commits atrás dela.

### A importação, por conteúdo

Os três arquivos Rust bateram por SHA-256 **antes** de serem copiados, e os
quatro diretórios de relatório vieram inteiros (`diff -r` contra a origem:
idênticos, e a origem não foi modificada).

| arquivo | sha256 | |
|---|---|---|
| `crates/seele-core/src/enlace.rs` | `c36cf800…7bb51c` | bate |
| `crates/seele-server/src/session.rs` | `bbb538b9…1de964` | bate |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | `2e3ac36d…c0f551a9` | bate |
| `docs/…/2026-09-10-…/relatorio.md` | `9f4e1ed0…83f70e60a4` | bate |

A alternativa conferível dos patches foi conferida:
`git apply --check --cached 00-base-importada-de-c0d9355c.patch` sai 0 sobre
`d562851`, e `git apply --check -R 01-prova-da-substituicao-da-conexao.patch`
também — ou seja, `00` + `01` reproduzem a árvore importada.

O inventário preservado de 09/09 continua íntegro: `conferir-inventario.py`
sai 0, «50 relatórios e 1 patch, tudo bate».

## 3 · A lacuna que fechou: a queda assimétrica

`Motor::largar_o_caminho_entre_pares` foi escrito para a queda e **nunca teve
teste de rede que o prendesse**. O relatório de 10/09 mediu por quê e deixou
escrito: no cenário que existia — o servidor caindo — ele é cinto sobre
suspensório, porque aquela queda mata os dois lados do caminho de par.

Montado o cenário que precisa dele — um relé UDP por onde **só** quem assiste
fala com o servidor, cortável sem tocar em mais nada —, a medida foi outra coisa
e maior:

```
relé cortado
13,80s  estado InternalBattery
16,03s  relé de volta
17,00s  RECONECTADO
→ 1336 quadros da geração velha depois de `Reconectado`
```

**`Motor::cair` nunca corria.** Há duas portas para a bateria interna e só uma
passava por ele: `cair` trata a queda que o transporte *avisa*, e uma conexão que
some sem avisar não avisa nada — ela produz silêncio, contado pelo `Ping`. Três
sem resposta e `Battery::poll_online` põe a bateria de pé por dentro, devolvendo
`Action::Wait`. É a porta que uma queda de verdade usa: a rota que some, o NAT
que reescreve, a máquina que dorme.

O conserto é uma linha: a porta dos pings passa pela mesma soltura que a do erro
(`Motor::soltar_a_conexao`, partido de `cair` para não reiniciar os cinco
minutos da bateria).

O teste é `a_queda_de_uma_conexao_so_derruba_o_caminho_do_par_com_o_par_ainda_vivo`,
e a prova é **a janela da bateria**: entre `InternalBattery` e `Reconectado` quem
assiste não tem conexão com o servidor, então qualquer quadro que chegue só pode
ter vindo do par. É discriminador mais forte que uma marca de mídia — que separa
duas transmissões, não duas origens — e a geração continua marcada para a
mensagem de falha dizer de qual mídia era o quadro que não devia estar ali.
Nenhum `ScreenId` e nenhum `SessionId` entram em afirmação nenhuma.

| # | guarda retirado | resultado |
|---|---|---|
| A | a porta dos pings em `Motor::passo` | **FAILED** — 58 quadros da geração 0 durante a bateria |
| B | **`largar_o_caminho_entre_pares`** em `soltar_a_conexao` | **FAILED** — os mesmos 58 |

A **B** é a que fecha a lacuna do §9 de 10/09: é a primeira vez que a proteção
pertinente falha por rede quando retirada, e passa restaurada.

## 4 · As duas falhas conhecidas, as duas fechadas

### `moderacao.rs::expulsar_acaba_com_a_sessao_e_deixa_voltar`

Vermelho desde `53c27d0`, que o deixou assim de propósito. **A causa não era a
que aquele commit supôs.** Com sonda nos dois lados:

```
P2: SessionEnded  alcançou person=2 ssrc=2     ← a expulsão chega e é obedecida
P0: sessão de pé  person=2 ssrc=3              ← o cliente RECONECTOU sozinho
P1: assentar      person=2 ssrc=3 destino=1    ← e a bateria redeclarou a sala
P3: occupancy: ["sala=1 person=2 ssrc=3"]
```

A sessão do expulso **termina**. O que não acontece é a sala esvaziar: a bateria
interna do cliente trata a despedida como queda de rede, sobe outra conexão e
redeclara a sala guardada; e como o assento passa a ser da conexão nova, a
limpeza da conexão expulsa — chaveada por `ssrc` desde `53c27d0`,
corretamente — não esvazia nada e ninguém na sala é avisado. Expulsar era
desfeito por um recurso feito para túnel de trem.

Conserto: `Kicked` e `Banned` **acabam** com a sessão em vez de começarem a
bateria, e `Motivo::Moderado(DisconnectReason)` leva o motivo enumerado até o
FFI em vez de virar `CredentialRejected`. Todos os outros motivos continuam na
bateria, que é o conserto deles.

Reversão: retirar o `encerrar` derruba o teste na mesma linha 322.
**A segunda reversão é o achado**: alargar para *toda* `Disconnecting` não
derruba nada — nenhum teste de comportamento segura a fronteira entre acabar e
reconectar. O que existe agora é um guarda contra deriva
(`toda_despedida_do_protocolo_escolhe_um_lado`, `match` exaustivo sem braço `_`),
e ele foi provado de verdade: acrescentar uma variante a `DisconnectReason`
derruba o build com `non-exhaustive patterns`. Ele impede que uma despedida nova
caia calada no lado errado; ele **não** prova comportamento, e isto está no §6.

### `vetores_de_hash.rs` — `fmt` e `clippy`

O custo não era estético: o `clippy` do workspace **parava** nesse erro, então
nenhuma execução de `--workspace` provava que os outros dez crates estavam
limpos, e dois relatórios anteriores tiveram de medir crate a crate. Três
mudanças, nenhuma tocando afirmação: o `allow` de teste que outros dezessete
arquivos já têm, `write!`→`writeln!` (mesma saída — o próprio teste prova, o
`vetores-de-hash.json` não mudou) e um nome para o tipo do caso.

## 5 · Verificações

Todas nesta candidata, com os guardas restaurados e o hash do diff conferido
depois de cada reversão.

| comando | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **1769 passaram, 0 falharam**, 74 alvos |
| `cargo test -p seele-conformance --test tela_por_um_par` | **13 passaram** — 5 voltas seguidas, 18,6 s cada |
| `cargo test -p seele-core --lib` | **297 passaram** |
| `cargo test -p seele-server --lib` | **413 passaram** |
| `cargo test -p seele-conformance --test moderacao` | **6 passaram** (era 5 + 1 falha) |
| `cargo test -p seele-conformance --test bateria_interna` | **4 passaram** |
| `cargo fmt --all -- --check` | **limpo** |
| `cargo clippy --workspace --all-targets` | **limpo, zero avisos** |
| `cargo xtask check-deps` | passa — 11 crates |
| `cargo xtask check-api` | passa — 1 versão de MOD |
| `conferir-inventario.py` | sai 0 |

A conta fecha com a base: `d562851` mais a importação media **1766 passando e 1
falhando** nos mesmos 74 alvos. Esta entrega acrescentou dois testes (o guarda de
deriva e a queda assimétrica) e transformou a falha em passagem: 1766 + 2 + 1 =
**1769, e zero falhas**. É a primeira vez nesta linhagem que o workspace inteiro
está verde.

### Pertinentes à futura versão de Windows

| comando | resultado |
|---|---|
| `xtask/tests/plataforma.rs` | passa — nenhum import de plataforma sem guarda |
| `xtask/tests/empacotamento.rs` | passa — os scripts declaram encoding |
| `apps/seele-app` (`tests/frontend.rs`, `permissoes.rs`) | passam, dentro da suíte |
| `bash -n` em `empacotar/*.sh` e `install.sh` | todos ok |
| `python3 -m py_compile` em `empacotar/*.py` | compila |
| `cargo check --workspace --target x86_64-pc-windows-msvc` | **falha por ambiente**, não por produto |

A última merece o nome exato, porque é a diferença que este pedido manda
separar: o alvo está instalado, mas o `cc` mirando MSVC não acha `assert.h` — o
SDK do Windows não existe nesta máquina. O erro é de `ring`, na compilação C, e
não diz nada sobre o código deste repositório.

**Nada aqui afirma build ou teste de Windows.** `empacotar/windows.ps1` não pôde
sequer ser conferido sintaticamente: não há `pwsh` nesta máquina. A pendência 7 —
«a matriz de três SOs nunca foi verde por inteiro» — continua aberta, e o
`docs/windows.md` continua dizendo, corretamente, que nada dele foi executado.

## 6 · Limitações

- **A fronteira entre acabar e reconectar não tem teste de comportamento.**
  Medido: com `a_sessao_acabou_aqui` devolvendo `true` para tudo, a suíte inteira
  do workspace continua verde. O estreitamento para `Kicked | Banned` segue o que
  os docs de `FellBehind`, `ScheduledMaintenance` e `ServerShuttingDown` já
  dizem, e está preso só contra deriva.

  > **Retificado em 2026-09-10 — as duas frases acima estão erradas, e ficam
  > aqui como estavam.** A segunda é falsa por medida: sob aquela mutação o
  > guarda unitário `enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado`
  > falha, então a suíte do workspace **não** continua verde. A primeira deixou
  > de valer: existe teste de comportamento desde `5146c04ba617` —
  > `uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao`, em
  > `crates/seele-conformance/tests/bateria_interna.rs`, que sob a mutação cai
  > dizendo qual regressão pegou. A retificação inteira, com comandos e códigos
  > de saída, está em
  > `docs/superpowers/sdd/2026-09-10-prova-da-reconexao-restaurada/relatorio.md`
  > (§4, §5.2 e §9).
  >
  > **O SHA que carrega a frase errada é `7079d6b`.** A tabela «As provas por
  > reversão» da mensagem daquele commit diz, na linha B, «**passou** — e a
  > suíte inteira do workspace também». A segunda metade é falsa pela mesma
  > medida: sob aquela mutação caem `bateria_interna.rs`
  > («uma despedida recuperável acabou com a sessão (`Moderado(FellBehind)`)») e
  > o guarda unitário `toda_despedida_do_protocolo_escolhe_um_lado` que o próprio
  > `7079d6b` acrescentou. Mensagem de commit não se reescreve sem reescrever o
  > histórico, e o histórico desta candidata não vai ser reescrito — então fica
  > o ponteiro nos dois sentidos: quem chegar pelo `git log` a `7079d6b` tem a
  > correção aqui, e quem chegar aqui sabe qual commit ler com ressalva.
- **`acceptance_m2` e `acceptance_m3` falham de vez em quando por prazo de
  conexão, e o mecanismo não tem nome.** Registrado com as medidas em
  `docs/superpowers/sdd/2026-09-10-prova-da-reconexao-restaurada/relatorio.md`
  (§6.1 e §6.2): duas de sete corridas do workspace terminaram em `exit 101` com
  `SemResposta` — `quinn::ConnectionError::TimedOut`, prazo estourado, e não
  asserção violada. Quatro hipóteses foram medidas e refutadas lá; contenção de
  CPU vinda de fora continua sendo a mais simples e não pôde ser confirmada nem
  descartada. **Não é falha de produto conhecida, e também não está provada como
  ambiente** — entra na `main` de olhos abertos, e nenhum teste foi enfraquecido
  para escondê-la. O §10 mede de novo, nesta ponta.
- **A prova da fila da conexão velha continua sendo de unidade.** A queda
  assimétrica prende as tarefas de par e o repasse; a troca das pontas de
  `resultados_do_par` é a rede de segurança de uma corrida entre o `abort` e o
  `await`, e uma corrida não se reproduz sob encomenda.
- **Uma máquina, um servidor local, uma sala, um par.** Nada aqui mede rede de
  verdade, NAT, ou mais de um par emprestando. O relé é `127.0.0.1`.
- **Os quatorze segundos da queda assimétrica são o contrato, não folga.** São
  três `Ping` de 5 s (`MISSES_BEFORE_RECONNECT`); o teste custa 18,5 s por isso.
- **`duas_maquinas.rs` e `soak_audio.rs` não foram exercitados de verdade**: são
  alvos que a suíte compila e que exigem rig de duas máquinas ou placa de som.
- **Nenhum recurso novo foi implementado**, por instrução: launcher, telas de
  consentimento, catálogo e o indexador cancelado de outro repositório ficaram
  de fora inteiros.
- **Nada foi acessado em `/private/tmp`** a não ser a saída de comandos em
  segundo plano criada pelo próprio harness. As sessões externas `d15590de` e
  `5a35cf92` continuam observadas bloqueadas, motivo desconhecido; não foram
  controladas. O checkout principal foi lido e não escrito: está limpo em
  `a695fe5`, então não há trabalho sem commit delas para sobrepor.
- **Nenhum impedimento por permissão apareceu nesta tarefa.** Nada foi
  contornado.

## 7 · Inventário: o que está pronto para a `main`

**Pronto, provado e verde:**

- o caminho entre pares inteiro (11 tarefas, onda de consertos, revisão final);
- a ficha da conexão abandonada (`53c27d0`) — e agora **com a expulsão
  funcionando de verdade**, que era a condição que aquele commit pôs por escrito
  para não ser mergeado;
- os MODs (`desenho/mods` inteiro), com `check-api` verde;
- o portão da subida medida (ADR 0044) e `subida_no_arranque.rs`;
- os quatro relatórios preservados de 09/09 e 10/09, mais este;
- os três consertos desta entrega.

**Fora da futura versão, e continua fora:**

- launcher, telas de consentimento e catálogo — desenhados, não construídos;
- o indexador de MODs de outro repositório — cancelado, e os links já apontam
  para lá (`eef59ca`);
- o empréstimo de subida **não tem exposição de interface**: conferido agora,
  `emprestar` não aparece em `apps/seele-app/src`, em `apps/seele-app/ui` nem em
  `crates/seele-ffi/src`. O opt-in existe em `Enlace` e no protocolo, e não
  atravessa o FFI — ou seja, a malha inteira é inalcançável pelo aplicativo
  publicado. Está registrado desde a revisão final de 07/09, e é o que impede a
  medida de três máquinas;
- a medição de três máquinas do caminho entre pares (`docs/m1-medicoes.md`,
  tarefa 11) — o procedimento existe, a medida não;
- pendências 7 (matriz de três SOs), 11 (roster ao reconectar rápido),
  31 (trocar fone no Windows exige reiniciar) e 33 (tela entre duas máquinas
  Windows) continuam abertas e são exatamente o que o teste no Windows vai
  tocar.

## 8 · Para a conferência final

```
branch   orbita/87002ad5
ponta    4f942e7
base     d562851  (que já contém 15a0406, 004322a, 53c27d0, a695fe5)
árvore   limpa
commits  4  (1 de importação, 3 de conserto)
```

Nada a fazer antes da conferência: a suíte está verde, o `fmt` e o `clippy`
estão limpos, e os dois `check` do `xtask` passam. A geração da versão e o merge
pertencem a quem pediu.

> **O §8 acima descreve a ponta de quando ele foi escrito, e fica como estava.**
> A ponta desta entrega é a do §9.

## 9 · O fecho, sobre a candidata de `496ba8c5`

Escrito depois de o aplicativo reiniciar no meio da execução. Nada foi refeito: o
worktree foi inspecionado, estava limpo em `81edc32`, e a continuação já havia
entregado uma candidata **descendente**. Ela entrou aqui por **avanço rápido** —
`81edc32` é ancestral de `0ddead3`, conferido —, sem merge e sem commit de fusão.

```
branch   orbita/87002ad5
ponta    0ddead3
base     d562851
árvore   limpa
commits  11 desde d562851  (1 de importação, 3 de conserto, 2 de prova nova,
                            5 de relato)
```

### O que a continuação acrescentou, e por que ela estava certa

Ela fechou a limitação que o §6 desta entrega tinha registrado, e **corrigiu uma
afirmação falsa que eu havia deixado nele**. As duas coisas foram remedidas aqui,
por conta própria, e não aceitas de palavra:

| medida | resultado |
|---|---|
| `a_sessao_acabou_aqui` devolvendo `true` para tudo → `seele-core --lib` | **FALHA** — «`Incompatible` mudou de lado sem que este teste mudasse junto» |
| a mesma mutação → `seele-conformance --test bateria_interna` | **FALHA** — «uma despedida recuperável acabou com a sessão (`Moderado(FellBehind)`)» |

A minha frase — «sob essa mutação a suíte inteira continua verde» — era uma
medida tirada **antes** de eu acrescentar o guarda de deriva, carregada adiante
sem ser refeita. O guarda escreve a decisão esperada variante a variante, à parte
da função, então ele pega a mutação na primeira delas. Foi a forma exata de erro
que o `CLAUDE.md` desta casa manda desconfiar, na direção contrária:
não «existir não é funcionar», mas «medi antes e supus que continuava valendo».

A lacuna verdadeira era mais estreita, e agora está fechada: faltava um guarda
que pegasse a mutação **por fora**, com servidor de verdade escrevendo a
despedida no fio. É o que
`uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao` faz, pelo
mesmo caminho de servidor que a expulsão usa — só o motivo muda —, e ele é o
lado de reconectar da fronteira que nenhum teste de comportamento segurava.

Os textos anteriores não foram reescritos: a retificação está anexada ao §6, e o
relatório inteiro dela está em
`docs/superpowers/sdd/2026-09-10-prova-da-reconexao-restaurada/relatorio.md`.

### As verificações, refeitas nesta ponta

| comando | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **1770 passaram, 0 falharam**, 74 alvos |
| `cargo fmt --all -- --check` | limpo |
| `cargo clippy --workspace --all-targets` | limpo, zero avisos |
| `cargo xtask check-deps` / `check-api` | passam |
| `conferir-inventario.py` | sai 0 |

A conta fecha: 1769 na ponta anterior mais o teste de comportamento novo = 1770,
e zero falhas.

### O estado, conferido de novo e não herdado

As quatro frentes, a base `d562851`, o release publicado `12a6401a6` e a ponta
anterior `81edc32` continuam **todos contidos**. Nenhum ramo do repositório tem
commit exclusivo. `main` continua em `15a0406`, não há tag `0.11`, a pilha de
`stash` está vazia, o worktree de origem `94b614e6` está como estava e o
checkout principal segue limpo em `a695fe5`.

O inventário do §7 continua valendo inteiro — nada saiu de «fora da futura
versão» para dentro, e nada de dentro saiu.

## 10 · A revisão de fora, e o que ela mudou aqui

A revisão independente aprovou a candidata e levantou cinco achados, todos
declarados não bloqueantes. Nenhum deles era falso, e quatro pediam mudança.

| achado | o que foi feito |
|---|---|
| espera vazia em `tela_por_um_par.rs` | **as três** foram removidas |
| a mensagem de `7079d6b` carrega a frase errada | ponteiro escrito no §6 |
| instabilidade de `acceptance_m2`/`m3` sem mecanismo | trazida para o §6 deste relatório e medida de novo aqui |
| a ponta não é nomeada por documento nenhum | nomeada abaixo, com a regra que impede nomear a si mesma |
| nenhuma verificação executa código de Windows | nada a corrigir: já estava dito assim |

### As esperas vazias, e por que eram três

O achado apontou `ate("as duas declarações chegarem ao servidor", || true)`: a
condição é o literal `true`, então a linha volta na primeira conferência e não
espera nada. Varrido o arquivo inteiro atrás da mesma forma, havia **mais duas**
— `"a declaração de quem empresta chegar ao servidor"` e `"o servidor soltar a
nomeação do par"`, no teste do `ParFalhou` depois do `UnwatchScreen`.

Removidas as três. **Nenhuma asserção foi enfraquecida, e nada passou a esperar
menos:** em cada um dos três lugares a conferência de verdade — o laço que
segura a paciência e afirma com a mesma frase da mensagem — já vinha na linha
seguinte, e a chamada vazia custava zero (com `|| true` ela retorna antes do
primeiro `sleep`). O que sai é exatamente a linha que promete uma sincronização
que não existe: «existir não é funcionar», na letra do `CLAUDE.md` desta casa.

`cargo test -p seele-conformance --test tela_por_um_par`, três voltas depois da
remoção: **13 passaram** em cada uma, 18,45 s / 18,46 s / 18,65 s.

### A falha de validação relatada: não reproduzida, e a medida está aqui

O pedido veio com uma saída de `cargo test` anexada e a instrução de corrigir «a
falha de validação». **Não achei falha nenhuma para corrigir, e o que posso
afirmar é o que medi:**

| corrida | resultado |
|---|---|
| `cargo test --no-fail-fast` × 4 (nesta ponta) | **verde nas quatro**, nenhuma linha `FAILED`, nenhum `panicked` |
| `cargo test -p seele-conformance --test tela_por_um_par` × 3 | 13 passaram em cada |

Sobre a saída anexada, uma leitura e não um palpite: ela não contém um único
`test result: FAILED`, e termina nos *doc-tests*, que rodam por último. Como
aquele comando é `cargo test` sem `--no-fail-fast`, um alvo vermelho teria
parado a corrida ali — os *doc-tests* no fim dizem que nenhum alvo daquela
corrida caiu. O que a saída tem é **buraco**: faltam blocos inteiros no meio
(`tela_por_um_par`, `moderacao`, `bateria_interna`, `acceptance_m2`/`m3`/`m5`),
o que é feitio de saída cortada por tamanho, não de suíte interrompida.

Duas coisas, então, ficam ditas com todas as letras. A primeira: **se houve
falha, ela não estava na saída que me chegou, e não a reproduzi em quatro
corridas do workspace inteiro.** A segunda: **não enfraqueci, apaguei nem
afrouxei prazo de teste nenhum para produzir verde** — o único código de teste
que este conserto toca são as três linhas que não esperavam nada.

A suspeita mais provável, se a falha existiu, é a instabilidade que o §6 agora
registra: `acceptance_m2`/`m3` caindo por `SemResposta` (prazo de conexão
estourado, não asserção violada), documentada com quatro hipóteses medidas e
refutadas em
`docs/superpowers/sdd/2026-09-10-prova-da-reconexao-restaurada/relatorio.md`
§6.1 e §6.2. Ela **não foi consertada aqui, de propósito**: apertar prazo ou
serializar suíte é mudança de infraestrutura fora deste escopo, e sem
reprodução seria conserto às cegas — a terceira coisa que o `CLAUDE.md` desta
casa manda não fazer. Se quem revisar quiser que ela feche antes do merge, é um
pedido novo e legítimo, e ele muda o escopo.

### A ponta, nomeada

O achado está certo: os relatórios paravam em `0ddead3`, e a ponta era o commit
que escreve o §9 — que nenhum documento nomeava. **Um commit não pode conter o
próprio SHA**, então a regra que fecha isso não é escrever mais um número, é
escrever a regra:

> A ponta desta candidata é sempre o **último commit de relato**, e ele só toca
> este arquivo. O último commit de **código** está nomeado no parágrafo acima
> dele. `git log --oneline -1` diz o resto, e não há nada entre os dois além de
> texto.

Nesta rodada de consertos são dois commits. O de **código de teste** é
`172738c` — «três esperas que não esperavam nada saem do teste», e é o único
que toca `crates/`. O de **relato** é esta seção, ele só toca este arquivo, e
é a ponta.

### As verificações, refeitas depois do conserto

| comando | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **1770 passaram, 0 falharam**, 74 alvos |
| `cargo test --no-fail-fast` (o comando da validação) | verde, 4 corridas |
| `cargo test -p seele-conformance --test tela_por_um_par` | 13 passaram, 3 voltas |
| `cargo fmt --all -- --check` | limpo |
| `cargo clippy --workspace --all-targets` | limpo, zero avisos |
| `cargo xtask check-deps` | passa — 11 crates |
| `cargo xtask check-api` | passa — 1 versão de MOD |
| `conferir-inventario.py` | sai 0 — «50 relatórios e 1 patch, tudo bate» |

A conta continua fechando em **1770**: tirar três chamadas que não esperavam
nada não tira teste nenhum, e nenhum alvo mudou de contagem.

O estado do §9 continua valendo inteiro: `main` intacta em `15a0406`, nenhuma
tag `0.11`, pilha de `stash` vazia, árvore limpa, nada publicado, nada com
`push`, nenhuma permissão tocada e nenhum arquivo fora deste worktree
modificado. Nenhum impedimento por permissão apareceu nesta rodada.

## 11 · A segunda revisão de fora: um achado virou conserto, três não tinham o que consertar

A revisão aprovou a candidata («nenhuma trava para o merge») e levantou quatro
achados, os quatro declarados não bloqueantes. Nenhum era falso. **Um pedia
código, e virou o commit `5133fb0`; os outros três não têm conserto a fazer, e
esta seção diz por quê em vez de fingir que fez.**

| achado | o que foi feito |
|---|---|
| `Pares::apontou` guarda uma nomeação por `ScreenId` | **consertado** — `5133fb0`, com prova por reversão |
| a malha não tem exposição no aplicativo | nada a consertar: é instrução desta tarefa, e já está no §7 |
| a instabilidade de `acceptance_m2`/`m3` segue sem mecanismo | nada a consertar sem reprodução; medida de novo aqui |
| a mensagem de `7079d6b` carrega uma frase falsa | não se conserta sem reescrever histórico; o ponteiro já está no §6 |

### 11.1 · A nomeação era por transmissão, e tinha de ser por espectador

O registro de quem o servidor apontou para servir cada tela era um
`HashMap<ScreenId, …>`. Uma transmissão tem um dono e **vários** espectadores, e
pela malha cada espectador ganha um par próprio — então dois espectadores da
mesma tela cabiam numa entrada só, e o segundo `WatchScreen` apagava o registro
do primeiro. O revisor chamou de observação de desenho e não a reproduziu como
defeito. **Reproduzi**, e por isso ela virou conserto:

```
$ cargo test -p seele-server --lib   # dois espectadores, mesma tela
o par do primeiro espectador sumiu de ja_servindo: {PersonId(5)}
```

Três consequências, do mesmo ponto:

1. o par que servia o primeiro espectador saía de `ja_servindo` **enquanto ainda
   repassava**, e `escolher` voltava a oferecê-lo — dois repasses na mesma
   subida emprestada, que é exatamente o que `ja_servindo` existe para impedir;
2. `desapontou(screen)` soltava a vaga dos dois: quem fechasse a janela
   devolvia à fila um par que continuava servindo outra pessoa;
3. um `ParFalhou` com `ImpressaoNaoBate` se resolveria contra a nomeação de
   outro espectador — e desacreditaria o par errado, tirando da malha quem não
   fez nada.

O conserto é a chave: `(transmissão, quem assiste) → quem empresta`.
`desapontou` passa a receber quem assiste — é o fim do repasse de **uma**
pessoa — e o fim da transmissão inteira ganha função própria,
`a_transmissao_acabou`, usada onde quem compartilha para, onde a sala é apagada
e onde a sessão de quem compartilha acaba. `quem_foi_apontado` passa a receber
quem relatou, que a sessão já conhece.

**As provas por reversão, com os dois vermelhos medidos:**

| o que foi revertido | resultado |
|---|---|
| a chave só por `ScreenId` — o código de antes | **FAILED** — `ja_servindo` = `{PersonId(5)}`, sem o par do primeiro |
| a chave colapsada em `(screen, PersonId(0))` | **FAILED** — 4 dos 21 testes de `pares`, entre eles os dois guardas novos |

A segunda reversão é mais larga do que o defeito original (ela também apaga o
registro de **quem** assiste, que o código antigo guardava), e por isso derruba
dois testes que já existiam além dos novos. Está dito assim de propósito: o
vermelho que corresponde ao achado é o **primeiro**.

Restaurado, `pares::` fecha em 21 passando e o arquivo volta ao SHA-256
`08cbbedf…` que tinha antes da mutação — conferido, não suposto.

Três testes novos, todos de unidade e todos por medida: dois espectadores da
mesma tela ocupam dois pares; quem fecha a janela não solta o par de quem
continua assistindo; a transmissão acabando solta o par de todo espectador (e
não o de outra transmissão).

**O que este conserto não é.** Ele não é a escolha boa de par — isso continua
sendo do subprojeto B, e o módulo continua dizendo que a escolha é
deliberadamente burra. E ele **não tem caminho de usuário hoje**: sem exposição
no aplicativo, ninguém chega a um segundo espectador pela malha. É conserto de
correção, não de sintoma relatado.

### 11.2 · Os três que não tinham conserto

**A malha sem interface no aplicativo.** Conferido de novo aqui, por busca
direta: `emprestar` não aparece em `apps/seele-app/src`, `apps/seele-app/ui`
nem `crates/seele-ffi/src`. É o §7 desde a primeira entrega, e construir essa
interface é justamente o que esta tarefa manda **não** fazer («não implementar
agora launcher, interfaces de consentimento ou catálogo»). Consertar aqui seria
desobedecer, não entregar.

**A instabilidade de `acceptance_m2`/`m3`.** Não reproduzida nesta rodada:

| corrida | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | 1773 passando, 0 falhando, 74 alvos |
| `cargo test` (o comando exato da validação) | **exit 0**, 1774 passando, 70 alvos |
| `acceptance_m2` + `acceptance_m3`, isolados | **5 voltas, 5 verdes** |
| `tela_por_um_par` | 13 passando, 3 voltas |

O revisor também não a reproduziu. Quatro hipóteses já foram medidas e
**refutadas** em `2026-09-10-prova-da-reconexao-restaurada/relatorio.md` §6.2, e
esta casa tem regra escrita contra repetir campanha sem mudança que a
justifique. Sem reprodução, apertar prazo ou serializar a suíte seria conserto
às cegas — e apertar prazo é, além disso, enfraquecer teste. **Continua
pendência aberta, de olhos abertos**, e entra na `main` declarada.

**A frase falsa na mensagem de `7079d6b`.** Continua verdade que ela é falsa, e
continua verdade que mensagem de commit não se reescreve sem reescrever o
histórico desta candidata. O ponteiro nos dois sentidos está no §6 e não foi
tocado.

### 11.3 · A «falha de validação», pela segunda vez: não existe na saída anexada

A saída de `cargo test` que veio com o pedido **não contém um único
`test result: FAILED`**, não contém `panicked`, e termina nos *doc-tests* — que
rodam por último. Como o comando é `cargo test` sem `--no-fail-fast`, um alvo
vermelho teria parado a corrida antes deles. O que a saída tem é buraco no
começo (falta o bloco de `seele-core`, entre outros), que é feitio de texto
cortado por tamanho.

Rodei o mesmo comando nesta árvore e registro o número que importa, que a saída
anexada não trazia: **`EXIT=0`**. Duas vezes — antes e depois do conserto.

Se houve falha, ela não estava na saída que me chegou e não a reproduzi. **Não
enfraqueci, apaguei nem afrouxei prazo de teste nenhum**: o diff desta rodada
acrescenta três testes e não remove nenhum.

### 11.4 · Verificações desta rodada

| comando | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **1773 passando, 0 falhando**, 74 alvos |
| `cargo test` | **exit 0** — 1774 passando, 70 alvos |
| `cargo test -p seele-server --lib pares::` | 21 passando (eram 18) |
| `cargo test -p seele-conformance --test tela_por_um_par` | 13 passando × 3 |
| `cargo test -p seele-conformance --test acceptance_m2 --test acceptance_m3` | verdes × 5 |
| `cargo fmt --all -- --check` | limpo |
| `cargo clippy --workspace --all-targets` | limpo, zero avisos |
| `cargo xtask check-deps` / `check-api` | passam — 11 crates, 1 versão de MOD |
| `conferir-inventario.py` | sai 0 — «50 relatórios e 1 patch, tudo bate» |

A conta fecha: 1770 na ponta anterior + 3 testes novos = **1773** com
`--all-targets`; 1771 + 3 = **1774** no comando da validação, que conta o
doc-test e não conta os alvos de bancada.

**Windows: nada mudou e nada foi tentado de novo.** O ambiente é o mesmo (sem
SDK do Windows, sem `pwsh`), e este conserto não toca empacotamento. Nada aqui
afirma build ou teste nesse sistema.

### 11.5 · A ponta

A regra do §10 continua valendo: a ponta é o **último commit de relato**, e ele
só toca este arquivo. O último commit de **código** desta rodada é `5133fb0` —
«a nomeação é por espectador, e não por transmissão» —, e entre os dois não há
nada além deste texto.

`main` intacta em `15a0406`, nenhuma tag `0.11`, pilha de `stash` vazia, árvore
limpa, nada publicado, nada com `push`, nenhuma chave tocada, nenhuma permissão
alterada e nenhum arquivo fora deste worktree modificado. Nenhum impedimento
por permissão apareceu nesta rodada.
