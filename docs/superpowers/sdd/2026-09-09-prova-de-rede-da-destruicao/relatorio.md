# A destruição do `Enlace` provada pela rede, e não pela posse

**Data:** 2026-09-09
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/c0d9355c-be90-4198-a592-f2e1ddd1c10f`
**Branch:** `orbita/c0d9355c` · **Base:** `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`

Nenhum commit foi criado, a `main` não foi movida, nada foi publicado, nenhuma
chave foi tocada, nenhum worktree de fora deste diretório foi modificado e
nenhum relatório histórico foi editado.

---

## 1 · A base, conferida antes de mexer

`a695fe5` (ponta de `desenho/mods`) é ancestral de `d562851`, e não o contrário
— conferido, não suposto —, então a incorporação é **avanço rápido**: nenhum
commit criado, nenhum merge refeito.

```
$ git merge-base --is-ancestor a695fe5 d5628513…  →  sai 0
$ git merge --ff-only d5628513cfefd385b9cc5b22b5cedd427f2dbf2e
Updating a695fe5..d562851 · Fast-forward · 113 arquivos
```

Os quatro conteúdos não commitados vieram do worktree `b326b0a6` e bateram por
SHA-256 **antes** de qualquer edição:

| arquivo | sha256 | |
|---|---|---|
| `crates/seele-core/src/enlace.rs` | `c36cf800…7bb51c` | bate |
| `crates/seele-server/src/session.rs` | `bbb538b9…1de964` | bate |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | `a4aa38c3…89a3aa` | bate |
| `docs/…/2026-09-09-fecho-tarefas-de-pares/relatorio.md` | `193a60e5…56f730` | bate |

Os dois diretórios não rastreados (`2026-09-09-fecho-caminho-entre-pares/` e
`2026-09-09-fecho-tarefas-de-pares/`) foram copiados inteiros. O worktree de
origem **não foi modificado**: o `git status` dele continua o mesmo.

## 2 · A lacuna, e o que este teste acrescenta

O §10 do relatório importado a nomeia:

> **A destruição é provada pela posse, e não pela rede.** O teste da destruição
> mede que o dono cancela o que guarda. Que `Enlace::drop` solta esse dono é
> propriedade da linguagem, e não tem teste próprio.

Um teste novo, e só um, em `crates/seele-conformance/tests/tela_por_um_par.rs`:

**`destruir_o_enlace_encerra_o_caminho_do_par_e_quem_emprestava_volta_a_servir`**
(+245 linhas, nenhuma removida; nenhum outro arquivo tocado por esta tarefa).

Servidor de verdade, três `Enlace` de verdade e uma transmissão no ar do começo
ao fim. O caminho: o par serve quem assiste (30 quadros acima do piso com o
servidor subindo uma cópia só), o `Enlace` de quem assiste é **destruído** —
`drop`, e não `sair()`: sem `Comando::Sair`, sem `disconnect`, sem `Encerrado`
—, e a prova sai de **quem emprestava**, porque quem sumiu não tem mais canal
para falar.

Quatro pernas, e cada uma fecha uma forma de a prova ser vazia:

1. **O ambiente não acabou.** Quem empresta recebe 30 quadros novos depois da
   destruição. Sem isto, tudo abaixo poderia estar medindo um servidor morto.
2. **O piso é tirado depois da nomeação.** Quadro enfileirado não conta como
   atividade nova — a perna 2 dos irmãos deste arquivo.
3. **O contador do servidor fica em uma cópia** do primeiro quadro ao último: um
   segundo de imagem que o servidor não subiu só pode ter vindo do par.
4. **A imagem chega dentro de um segundo.** É a perna que faltava na primeira
   versão, e sem ela o teste **passava com os guardas retirados** (§4).

## 3 · O prazo de um segundo é medido, e não escolhido a esmo

| | primeiro quadro pelo par |
|---|---|
| com os guardas de pé (3 medições) | 232 ms · 235 ms · 232 ms |
| com a vaga presa | 3,12 s |
| com as tarefas de par soltas | 3,02 s |

Os 232 ms são quase todos `ESPERA_DO_FURO` — 200 ms fixos, não trabalho de
processador —, o que torna o número estável numa máquina carregada. Os 3 s são
`PRAZO_DO_PAR` inteiro, gasto por uma discagem contra quem recusou o pedido em
silêncio. Um segundo é 4× o caso bom e ⅓ do custo da recusa.

**E o que ele prende não é velocidade, é tela preta**: ao apontar o par, o
servidor já desligou o cano daquela pessoa.

## 4 · As reversões — inclusive a que não falhou

Cada guarda retirado, o teste rodado, o código restaurado. A restauração foi
conferida por `git diff | git hash-object --stdin`, que voltou a
`a673e6e858fd442d3f9be57a7d26179c2ebb986c` depois de cada uma.

| # | guarda retirado | resultado |
|---|---|---|
| A | `impl Drop for VagaDeAtendimento` | **FAILED** — «o espectador novo passou 3.122475208s sem um quadro depois de pedir a tela» |
| B | `impl Drop for TarefasDePar` | **passou** (231 ms) |
| C | B **mais** o `avisos.send(…).is_err() → return` de `ler_a_tela_alheia` | **FAILED** — 3.017242209s |

### A reversão B é o achado, e ela está aqui porque não falhou

Retirar `impl Drop for TarefasDePar` **não** derruba este teste, e isso não é
defeito do teste: na destruição a tarefa que lê do par morre de todo jeito, no
primeiro `send` a um canal cujo recebedor foi junto com o `Enlace`. São **dois
guardas independentes** para a mesma propriedade, e a reversão C — os dois fora
— é o que prova que são eles, e não a sorte.

Dito na direção que importa: **no caminho da destruição, `TarefasDePar` é cinto
sobre suspensório.** Onde ele é o único guarda é em `sair()` (o `Enlace`
sobrevive, o recebedor também, e o acidente do `send` nunca acontece) e em
`cair()` — os dois já provados por reversão na entrega anterior.

O guarda pertinente **a esta prova** é o A: `VagaDeAtendimento::drop` é o único
mecanismo que devolve a vaga de quem empresta, e sem ele aquela máquina fica
fora da malha para sempre, recusando cada pedido em silêncio.

## 5 · Verificações, com os guardas restaurados

| comando | resultado |
|---|---|
| `cargo test -p seele-conformance --test tela_por_um_par` | **11 passaram, 0 falharam** — 3 voltas seguidas, 4,3 s cada |
| `cargo test -p seele-core --lib` | **296 passaram, 0 falharam** |
| `cargo xtask check-deps` | passa — «dependency rule holds across 11 workspace crates» |
| `cargo fmt -p seele-conformance -- --check` | limpo |
| `cargo clippy -p seele-conformance --all-targets` | limpo |

Os 10 testes de pares anteriores continuam passando, sem alteração nenhuma.
`enlace.rs` e `session.rs` terminam **byte a byte iguais aos importados** —
`c36cf800…` e `bbb538b9…` conferidos depois da última reversão.

## 6 · Limitações

- **A substituição da conexão continua sem prova ponta a ponta**, por instrução:
  não foi tentada aqui, e nada neste relatório a declara validada. Os dois
  testes de `cair` seguem provados só por unidade.
- **Esta prova é indireta, e o relatório diz por quê.** Um `Enlace` destruído
  não tem canal para observar; a afirmação sai de quem emprestava voltar a
  servir. Ela prende o efeito de rede da destruição — a conexão QUIC com quem
  assistia morreu e a vaga voltou —, não o instante do `abort`.
- **Uma máquina, um servidor local, uma sala.** Nada aqui mede rede de verdade,
  NAT, ou mais de um par emprestando.
- **`quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem` já cobria o
  outro lado** (destruir o `Enlace` de quem empresta). Este teste é o lado que
  faltava: destruir o de quem assiste.
- **Estado publicado: desconhecido, por instrução.** Nenhuma consulta de release
  foi feita; nada aqui afirma o que está em máquina nenhuma. Protocolo v4, MODs
  e relatórios históricos não foram tocados.
- **Nada foi acessado em `/private/tmp`**, nenhuma sessão externa foi
  controlada, nenhuma permissão foi alterada e **nenhum impedimento por
  permissão apareceu** nesta tarefa.

## 7 · Como aproveitar o que está sem commit

Tudo está no worktree, sem commit. Da mais direta à mais granular:

**1 · Usar este worktree como está.** Ele já tem `d562851` mais a importação
mais o teste novo.

**2 · Levar o conjunto inteiro** (a partir de `d562851`):

```sh
git diff > /onde/quiser/tudo.patch
# do outro lado, com HEAD em d562851:
git apply /onde/quiser/tudo.patch
```

Os três diretórios de `docs/superpowers/sdd/2026-09-09-*` são não rastreados e
**não entram** num `git diff`: copiar à parte.

**3 · Separar a importação da correção nova**, com HEAD em `d562851` e a árvore
limpa:

```sh
git apply docs/superpowers/sdd/2026-09-09-prova-de-rede-da-destruicao/00-base-importada-de-b326b0a6.patch
git apply docs/superpowers/sdd/2026-09-09-prova-de-rede-da-destruicao/01-prova-de-rede-da-destruicao.patch
```

O `00` é a entrega de `b326b0a6` exatamente como veio (três arquivos Rust,
conferidos por SHA-256). O `01` é só esta tarefa: **um arquivo**,
`tela_por_um_par.rs`, +245/−0, com caminho relativo.

Os dois foram conferidos, e não supostos:

```
$ git apply --check --cached 00-base-importada-de-b326b0a6.patch   → sai 0
$ git apply --check -R 01-prova-de-rede-da-destruicao.patch        → sai 0
```

O segundo diz que o `01` desfaz exatamente a árvore atual — ou seja, `00` + `01`
a reproduzem. Os patches foram gerados pelo índice do Git (`hash-object -w` +
`update-index`, com `git reset` no fim) justamente para sair com caminho
relativo: a armadilha do `--no-index`, que grava caminho absoluto e produz um
patch que só se aplica nesta máquina, está registrada no relatório anterior.

## 8 · Onde continuar

O próximo passo já nomeado, e fora deste escopo: a prova ponta a ponta da
**substituição da conexão** — derrubar e ressubir o servidor na mesma porta
(`bateria_interna.rs` sabe fazer) com a transmissão sobrevivendo ao reinício, e
conferir que a fila da conexão que caiu não alcança a substituta. O mecanismo
está provado por unidade em `enlace.rs`
(`cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta`); a costura, não.
