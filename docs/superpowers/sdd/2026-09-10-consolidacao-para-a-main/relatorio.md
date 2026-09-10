# A candidata para a `main`, com o workspace verde pela primeira vez

**Data:** 2026-09-10
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/87002ad5-5bea-4479-87c9-583d842db506`
**Branch:** `orbita/87002ad5` · **Base:** `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`
**Ponta desta entrega:** `4f942e7`

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
