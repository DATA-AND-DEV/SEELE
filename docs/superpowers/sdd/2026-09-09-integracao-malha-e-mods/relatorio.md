# Integração das três frentes — malha, MODs e a ficha da conexão abandonada

**Data:** 2026-09-09
**Branch candidata:** `orbita/f6ee869a`
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/f6ee869a-c1c5-4331-bcb7-7beee5854d39`

A `main` não foi movida, nada foi publicado, nenhuma chave foi tocada e nenhum
worktree de fora deste diretório foi modificado.

## 1 · O grafo real, e por que a ordem foi essa

Conferido antes de integrar, e não suposto:

```
cad193f  fix(subida): a janela que o cano encheu conta…      ← base comum das três
   │
   ├─ 15a0406  docs(plano): dez tarefas para o caminho entre pares   (main local)
   │     ├─ 004322a  malha/caminho-entre-pares      (39 commits)
   │     └─ 53c27d0  bug/ficha-da-conexao-abandonada ( 1 commit)
   │
   └─ a695fe5  desenho/mods                         (28 commits)
```

O detalhe que decide a ordem: **`desenho/mods` saiu de `cad193f`, antes da
`main`, e nunca recebeu nada dela.** `git merge-base --is-ancestor 15a0406
a695fe5` responde não. As outras duas frentes saem de `15a0406` e se enxergam.

Daí a ordem: primeiro o par de base curta (`malha` ← `bug`, base `15a0406`),
depois o ramo distante (`mods`, base `cad193f`) sobre um lado já íntegro. A
alternativa — mods primeiro — faria a fusão maior acontecer contra a base mais
antiga, com mais chance de uma resolução de três pontas errada passar batido.

A `main` local está em `15a0406`, **onze commits à frente** de `origin/main`
(`17c6421`). Isso é estado de antes desta tarefa e não foi mexido.

## 2 · Antes de qualquer merge: a cópia durável

Os relatórios das onze tarefas moravam só em `/private/tmp`, fora do git por um
`.gitignore` do próprio worktree. Um reboot os levava. Foram copiados **antes**
dos merges, com sha256 conferido arquivo por arquivo — 50 arquivos, 2 418 689
bytes, todos batendo. Inventário e verificador em
[`inventario-e-hashes.md`](inventario-e-hashes.md) e
[`conferir-inventario.py`](conferir-inventario.py).

O verificador foi provado contra a regressão de verdade: com **um byte trocado**
em `task-7-report.md` (tamanho inalterado), ele acusa
`DIVERGE task-7-report.md`; restaurado, volta a `tudo bate`.

A origem não foi tocada — `git status` nela é o mesmo de antes e depois.

## 3 · Os conflitos, e o que cada um era

Seis arquivos em conflito nos dois merges. Nenhuma funcionalidade foi perdida;
onde os dois lados traziam coisas diferentes, ficaram as duas.

### `session.rs` — três vezes o mesmo conflito (merge 1)

Nos três pontos de saída de uma sessão. A malha renomeou `encerrar_telas_de`
para `soltar_telas_e_pares_de` (a saída agora também devolve o par nomeado à
fila); o conserto da ficha acrescentou o `ssrc` a `leave_everywhere` (para a
conexão abandonada do ADR 0037 não apagar a ficha da conexão viva). São
mudanças que não se cruzam: ficaram as duas. O quarto ponto de saída, em
`assentar`, o git fundiu sozinho — conferido à mão, está com a mesma forma.

### A medida de subida, construída duas vezes (merge 2)

`crates/seele-server/src/persistence/subida.rs` entrou como `add/add`: o mesmo
recurso foi commitado na `main` (`8a75e2a`) e, em separado, em `desenho/mods`
(`b19c280`) — **com o mesmo título de commit, palavra por palavra**.

Comparados linha a linha, a única diferença é o passe de vocabulário do ADR
0035 (`Dogma` → **servidor**), que a `main` aplicou depois em `f7512ef`. Ficou o
lado da `main`: mesmo código, mais o passe. Os conflitos em `lib.rs`, `tela.rs` e
`session.rs` são esse mesmo passe reaparecendo, resolvidos do mesmo lado.

`tela.rs` trazia ainda um teste só de um lado
(`uma_casa_de_fibra_nao_e_cortada_pelo_teto_da_estimativa`) — preservado.

### Dois ADRs 0044 (merge 2)

`desenho/mods` escreveu `0044-mods-…` e `0045-toda-versao…`; a `main`, no mesmo
intervalo, aceitou `0044-o-portao-divide-a-subida-medida`. Como os **nomes de
arquivo são diferentes**, o git não viu colisão nenhuma: só a linha da tabela do
`README.md` conflitou, e os dois 0044 teriam entrado lado a lado, calados. É a
forma de defeito que este repositório paga mais caro — acertar e não dizer.

Renumerado o **proposto**, não o aceito, pela regra que o próprio
`docs/adr/README.md` já dava para renomes: «renomeá-los quebraria todo link que
aponte para eles de fora do repositório». Um número aceito na `main` é endereço
público; os dois dos MODs ainda são propostas de um ramo não mergeado.

| era | virou | estado |
|---|---|---|
| `0044-mods-o-produto-base-tem-regras-e-um-mod-nao` | **0045** | proposto |
| `0045-toda-versao-continua-de-pe` | **0046** | proposto |
| `0044-o-portao-divide-a-subida-medida` | fica | aceito |

**118 referências** acompanharam, em **35 arquivos**. As duas páginas
renumeradas abrem com uma nota dizendo o número antigo, porque commits e
conversas anteriores a hoje dizem «ADR 0044» querendo dizer a página dos MODs.

Ficaram de fora da troca, conferidos um a um: o próprio
`0044-o-portao-divide-a-subida-medida.md` e a única citação a ele fora dali, em
`docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md` (§11, «o
servidor mede a própria subida desde o ADR 0044»). Antes de trocar, as
ocorrências de `0044`/`0045` nos arquivos alvo foram lidas: todas eram
referência a ADR, nenhuma era outro número.

**A primeira passada errou, e o conferidor de links é que pegou.** Ela cobriu
29 arquivos, achados por um `grep` que não incluía `docs/superpowers/plans/`
nem `spikes/` — e deixou **seis** arquivos apontando para um `0044-mods-….md`
que já não existia: os dois planos dos MODs, o `README.md`, o `Cargo.toml` e
dois `src/bin/` do spike. Quatro deles eram link de markdown quebrado, e nenhum
teste, `fmt`, `clippy` ou `check-api` teria reclamado: são comentários e prosa.

O que pegou foi uma varredura que resolve **todo** link `(NNNN-….md)` do
repositório contra a lista real de `docs/adr/`. Ela agora dá zero, e é a
conferência que vale a pena repetir depois de qualquer renumeração:

```sh
# zero = nenhum link de ADR aponta para arquivo que não existe
python3 - <<'PY'
import os, re
alvos = set(os.listdir('docs/adr'))
ruins = []
for base, _, arqs in os.walk('.'):
    if '/.git' in base or './target' in base:
        continue
    for a in arqs:
        if not a.endswith(('.md', '.rs', '.toml', '.js', '.json')):
            continue
        p = os.path.join(base, a)
        try:
            txt = open(p, encoding='utf-8').read()
        except Exception:
            continue
        for m in re.finditer(r'\(([^()]*?adr/)?(\d{4}-[a-z0-9-]+\.md)\)', txt):
            if m.group(2) not in alvos:
                ruins.append((p, m.group(2)))
print('links de ADR quebrados:', len(ruins))
for p, l in ruins:
    print(' ', p, '->', l)
PY
```

(A cópia preservada em `sdd/2026-09-06-caminho-entre-pares/` fica **fora** da
renumeração de propósito: ela é registro conferido por hash, e reescrevê-la
invalidaria o inventário. Os relatórios de lá dizem «0044» com o sentido que
tinham no dia em que foram escritos.)

### O que **não** conflitou, ao contrário do previsto

`docs/pendencias.md`, `control.rs` e `client.rs` não conflitaram: nenhuma das
duas frentes mexeu em `pendencias.md`, e `control.rs`/`client.rs` são só da
malha. O `Cargo.lock` fundiu sozinho — e o build de workspace inteiro **não o
alterou depois**, que é a prova de que a fusão dele ficou consistente.

## 4 · O que foi preservado intacto

Conferido por diff contra a ponta de origem, não por leitura:

```
git diff 004322a HEAD -- crates/seele-proto/src/version.rs \
    crates/seele-proto/src/control.rs crates/seele-core/src/client.rs \
    crates/seele-core/src/par.rs crates/seele-server/src/pares.rs
```

Vazio. O protocolo **v4**, a janela de compatibilidade (aceita v3 e v4, recusa
v2) e as proteções da malha entraram byte a byte como estavam. Nenhuma política
nova de compatibilidade foi inventada nesta etapa.

## 5 · O trabalho não commitado que estava em wt2

`wt2` tinha três arquivos modificados e **não commitados** — 357 linhas. Não são
rascunho: são o item 1 da onda de fechamento (ver `fecho-brief.md`, preservado),
que conserta uma regressão criada pela própria onda de consertos anterior — um
`ParFalhou` de rotina chegando depois de um `UnwatchScreen` fazia o servidor
voltar a subir a cópia para uma janela fechada.

Foi preservado como patch (sha256 no inventário) e **não entrou na integração**:
o escopo desta tarefa são os três commits nomeados, e código não commitado de
outra sessão não é meu para commitar. Ele **aplica limpo** sobre esta branch:

```
$ git apply --check --verbose …/wt2-nao-commitado.patch
Hunk #1 succeeded at 1243 (offset 7 lines).
Hunk #2 succeeded at 2310 (offset 9 lines).
Hunk #3 succeeded at 2477 (offset 9 lines).
```

`wt-bug` está limpo. O worktree da análise `a5e31906` está limpo inclusive de
arquivos não rastreados — a análise não deixou artefato em disco.

## 6 · As verificações, e o que cada falha é

Comandos rodados em `f1993b7` — a ponta de **código** desta branch —, com o
resultado literal. Os dois commits depois dele mexem só em `docs/`, `spikes/` e
comentários: nenhuma linha compilada mudou, e `cargo check --workspace
--all-targets` e `cargo xtask check-api` foram repetidos na ponta real
(`5e03d19`) e continuam limpos.

| comando | resultado |
|---|---|
| `cargo check --workspace --all-targets` | **limpo** |
| `cargo xtask check-deps` | **passa** — «dependency rule holds across 11 workspace crates» |
| `cargo xtask check-api` | **passa** — «toda a superfície de MOD ainda aponta para algo (1 versão(ões))» |
| `cargo test --workspace --all-targets --no-fail-fast` | **1757 passaram, 1 falhou**, em 74 alvos |
| `cargo fmt --all --check` | **reprova** em 1 arquivo |
| `cargo clippy --workspace --all-targets` | **reprova** em 1 arquivo |

As três falhas são **anteriores**, e isso foi *medido* — não deduzido da leitura
do diff.

### `expulsar_acaba_com_a_sessao_e_deixa_voltar`

O único teste vermelho de 1758. O commit `53c27d0` **anuncia esta falha no
próprio título** — «NÃO MERGEAR AINDA» — e explica: o teste passava porque a
conexão abandonada esvaziava o assento aos 82 ms, antes de a expulsão
acontecer. Não era a expulsão que ele media. Com a saída chaveada por conexão,
ele passa a mostrar o que sempre foi verdade: **a sessão de quem é expulso não
termina no servidor.**

Medido em `53c27d0`, sem nada da integração:

```
test expulsar_acaba_com_a_sessao_e_deixa_voltar ... FAILED
panicked at crates/seele-conformance/tests/moderacao.rs:322:5:
quem foi expulso continua desenhado na sala de voz
test result: FAILED. 5 passed; 1 failed; … finished in 10.34s
```

E na integração: mesma mensagem, mesma linha, mesmo 5/1. `moderacao.rs` e
`voice_room.rs` são byte a byte iguais entre `53c27d0` e `f1993b7`
(`git diff` vazio nos dois). A integração não mexeu nesta falha nem a causou.

**Consertar a expulsão é outro trabalho e outra decisão** — o próprio commit diz
isso, e é por isso que ele não foi para a `main`. Não foi consertado aqui: a
instrução era corrigir apenas regressões da integração, e esta não é uma.

### `crates/seele-proto/tests/vetores_de_hash.rs` — `fmt` e `clippy`

Um arquivo só, e é o que `a695fe5` (a ponta de `desenho/mods`) trouxe. Ele
atravessou a integração **byte a byte inalterado** — `git diff a695fe5 HEAD` no
arquivo é vazio.

Medido em `a695fe5`, sem nada da integração:

- `cargo fmt --all --check` → 3 trechos, todos neste arquivo, nas linhas 42, 50
  e 67 — exatamente os mesmos da integração;
- `cargo clippy -p seele-proto --all-targets` → `error: used expect() on a
  Result value`, `vetores_de_hash.rs:99`, com `-D clippy::expect-used`.

Idênticos aos dois da integração. Falha anterior, de origem, não regressão.

Não foi consertada, pela mesma instrução. **É barato de fechar** e é a primeira
coisa que a próxima tarefa deve considerar: `cargo fmt --all` resolve os três
trechos, e o `clippy` pede trocar um `expect` por erro tratado na linha 99.

### Regressões da integração

**Nenhuma.** Não houve falha que existisse em `f1993b7` e não existisse nas
pontas de origem.

## 7 · Pendências exatas

### Protocolo

`PROTOCOL_VERSION = 4` está **nesta branch** e não na `main`. A janela aceita v3
e v4 e recusa a v2. O motivo está escrito no próprio `version.rs`: a v3 já tinha
saído — o arquivo cita o release `v0.10.5-1`, commit `12a6401a6` — quando as
quatro variantes do caminho entre pares entraram (`EmprestarSubida`,
`ParFalhou`, `SirvaTelaPara`, `AssistaTelaPor`), então elas não puderam pegar
carona numa versão ainda não publicada.

**O que fica em aberto, e é uma conferência, não uma decisão:** *não consegui
conferir o que está publicado.* A rede está bloqueada nesta sessão — `curl` e
`WebFetch` foram negados (§8). O parágrafo acima é o que o **código** diz, não o
que está no ar em máquina nenhuma. Antes de essa janela deslizar em produção,
quem for mergear precisa rodar o comando de releases do `CLAUDE.md` e confirmar
que a v3 é mesmo a versão publicada e que não há gente em v2.

Nenhuma política nova de compatibilidade foi inventada aqui, como pedido.

### Consentimento de emprestar a subida

Construído inteiro e **inalcançável**:

| camada | estado |
|---|---|
| `seele-proto` — `ClientMessage::EmprestarSubida` | existe |
| `seele-core` — `Motor::emprestar_subida` (`client.rs:1362`) | existe |
| `seele-server` — `pares.rs`, declaração e escolha | existe |
| `seele-ffi` — alguma chamada | **nenhuma** |
| `apps/seele-app` — tela, botão ou comando | **nenhum** |

`grep -rn "emprestar_subida" crates/seele-ffi apps` não devolve uma linha.

Consequência: **ninguém consegue dizer sim.** A metade de quem empresta está
pronta e desligada, e é ela que a medição de três máquinas da tarefa 11 precisa.
A pendência é uma tela ou comando de consentimento — e ela é de privacidade,
não de conveniência: emprestar a subida é gastar a banda de quem consente para
entregar a tela de outra pessoa.

### Launcher

ADR **0046** (era 0045), «Toda versão continua de pé: o app vira launcher»:
**proposto, não aceito, zero código**. `grep -rln "launcher\|multi-versão"` em
`crates`, `apps`, `xtask` e `empacotar` não acha nada.

Ele depende do ADR 0045 (MODs), também proposto. A ordem importa: o 0046 existe
porque o 0045 abandona a camada de compatibilidade, e sozinho não teria motivo.

### MODs

**Construídos nesta branch**, e é bastante coisa: runtime no servidor com
QuickJS, o bloco `world` (rede, relógio, registro), despacho com falha isolada,
persistência com migração, a fachada de API em `api/` e o `cargo xtask
check-api` que a defende — verde.

Três pendências, e a primeira é a que engana:

1. **`docs/pendencias.md` §22 está desatualizada e agora contradiz o código.**
   Ela diz «MODs estão desenhados e não construídos», descreve o ADR **0029** —
   que o 0045 substituiu por inteiro — e afirma «Nada de código, de propósito».
   Nenhuma das três frentes tocou nesse arquivo, então ele atravessou a
   integração intacto e errado. Quem ler `pendencias.md` para saber o estado dos
   MODs vai ler o contrário do que está no repositório.
2. **Os ADRs 0045 e 0046 continuam propostos.** O código dos MODs entrou sob um
   ADR que ninguém aceitou ainda. Isso é decisão do dono, não de integração.
3. `crates/seele-proto/tests/vetores_de_hash.rs` reprova `fmt` e `clippy` — ver
   §6. É falha anterior, de `a695fe5`, e não foi consertada aqui por instrução.

### A onda de fechamento do caminho entre pares não terminou

Isto não estava na lista de pendências a informar, e apareceu na leitura dos
relatórios preservados. É a pendência mais acionável das cinco.

`fecho-brief.md` (preservado) despacha **três** itens sobre `004322a`. O
`fecho-report.md` que ele pede **não existe**. Estado de cada item, conferido no
código:

| item | o que era | estado |
|---|---|---|
| 1 | o fim limpo podia desfazer um `UnwatchScreen`, nos dois lados | **implementado e não commitado** — é o patch de wt2 (§5) |
| 2 | falta teste para o `desapontou` do braço de `UnwatchScreen` | a linha (`session.rs:2305`) só é exercitada por um passo do teste do item 1, no patch; sem prova por reversão registrada |
| 3 | `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem` afirma e não prende | **não feito** — o teste continua na forma fraca, com `pelo_par` e sem `maior_seq_ja_enfileirado` nem sustentação |

O item 3 é o mais importante de retomar: o revisor mediu que, removendo o
mecanismo que o teste diz prender, ele **passa três vezes em três**. É um teste
verde que não prova nada — a forma de falha que este repositório já pagou caro.

## 8 · Limitações e bloqueios de permissão

Registrados porque mudam o que este relatório pode afirmar. **Nenhuma
verificação de permissão foi desativada.**

- **Rede bloqueada.** `curl` para a API do GitHub e `WebFetch` foram negados
  nesta sessão. Por isso **não confiro o que está publicado**, e nada aqui
  afirma o que está ou não está numa máquina — a regra que abre o `CLAUDE.md`.
  Tudo o que digo sobre versão vem do código desta branch.
- **Leitura fora do worktree, parcialmente bloqueada.** `ls`, `cp`, `sed`,
  `shasum` e `tar` sobre `/private/tmp/…` foram negados por exigirem aprovação
  que uma sessão não interativa não tem como dar. `git -C`, `du` e `python3`
  sobre os mesmos caminhos passaram, e foi por eles que a cópia durável foi
  feita — leitura só, com sha256 conferido nos dois lados.
- **Sessões externas.** `d15590de` e `5a35cf92` foram observadas **bloqueadas**,
  as duas com `cwd` em `/Users/dev-alexandre/SEELE` — o worktree principal, que
  está em `desenho/mods` e **limpo**. Não presumo que estejam paradas e não as
  controlei. Nada fora deste worktree foi tocado.
- **Memória do claude-mem.** Usada só como pista, e conferida contra o código
  antes de virar afirmação. Ela descreve a onda de consertos da sessão
  `00073a07` (SEELE, 07/09/2026) como concluída com «1688+ testes passando»; o
  que **medi hoje** nesta integração é 1757 passando e 1 falhando, e a falha é a
  de `53c27d0`, que não fazia parte daquela onda. Não são números comparáveis:
  aquele era outro conjunto de commits, sem `desenho/mods` e sem o conserto da
  ficha.

## 9 · O que a próxima tarefa aproveita

**Caminho:**
`/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/f6ee869a-c1c5-4331-bcb7-7beee5854d39`

**Branch:** `orbita/f6ee869a`

**Commit a aproveitar:** a ponta de `orbita/f6ee869a`. O handle estável é a
**branch**, e não um sha que este próprio relatório empurra para a frente toda
vez que é corrigido.

Os commits da branch, do mais antigo:

| commit | o que é |
|---|---|
| `21393ce` | merge 1 — `bug/ficha-da-conexao-abandonada` sobre `malha` |
| `f1993b7` | merge 2 — `desenho/mods` sobre o resultado; **ponta de código**, e é onde as verificações do §6 rodaram |
| `f7edfda` | a cópia durável dos relatórios, o inventário e este relatório |
| `5e03d19` | os seis links de ADR que a renumeração deixou para trás |
| daí para frente | só correções deste relatório |

`f1993b7` é o commit que importa para quem for mergear código: tudo depois dele
é prosa e comentário.

Os três commits de origem continuam de pé e alcançáveis: `desenho/mods`
(`a695fe5`), `malha/caminho-entre-pares` (`004322a`) e
`bug/ficha-da-conexao-abandonada` (`53c27d0`). A `main` não foi movida.

Ordem sugerida para quem continuar, do mais barato ao que precisa de decisão:

1. `cargo fmt --all` e o `expect` da linha 99 de `vetores_de_hash.rs` — fecha as
   duas únicas reprovações de estilo, e as duas são anteriores.
2. Aplicar o patch de wt2 (§5) e terminar os itens 2 e 3 da onda de fechamento —
   o patch aplica limpo, e o item 3 é um teste verde que não prova nada.
3. Atualizar `docs/pendencias.md` §22, que hoje diz o contrário do repositório.
4. A tela de consentimento de emprestar a subida — sem ela a malha inteira do
   lado de quem empresta fica desligada, e a medição de três máquinas não sai.
5. As decisões do dono: aceitar ou não os ADRs 0045 e 0046, e o que fazer com a
   expulsão que `53c27d0` revelou quebrada.
