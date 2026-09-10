# A prova da reconexão, restaurada e medida por reversão

**Data:** 2026-09-10
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/496ba8c5-8040-415c-9032-b1420580f1ab`
**Branch:** `orbita/496ba8c5` · **Base:** `81edc3219f484219ed665385e76bf1b566ae677e`

Complemento. **Nenhum relatório histórico foi editado.** A `main` não foi movida,
nada foi publicado, nenhuma tag ou release foi criada, nenhum `push` foi feito,
nenhuma chave foi tocada, nenhuma verificação de permissão foi desativada,
nenhum worktree de fora deste diretório foi modificado e nenhuma sessão ou
processo externo foi controlado.

---

## 1 · O que esta tarefa é, e o que ela não é

A tarefa anterior, `858903f7`, terminou em *Request timed out* com uma **mutação
negativa viva** no worktree dela: `a_sessao_acabou_aqui` devolvendo `true` para
todo motivo. Ela deixou também o teste que faltava, sem commit. Esta tarefa não
repete aquela campanha: ela **restaura a candidata** — a base intacta mais o
teste — e fecha as quatro coisas que a revisão independente cobrou.

O que a base já tinha e continua tendo, conferido antes de editar:

```
$ git rev-parse HEAD
81edc3219f484219ed665385e76bf1b566ae677e
$ git diff --stat -- crates/seele-core/src/enlace.rs   # antes de qualquer edição
(vazio)
$ sed -n '269p' crates/seele-core/src/enlace.rs
    matches!(motivo, DisconnectReason::Kicked | DisconnectReason::Banned)
```

A base entrou por avanço rápido (`a695fe5 → 81edc32 (Fast-forward)` no
`logs/HEAD` do worktree): nenhum merge refeito, nenhuma reescrita.

## 2 · A origem, conferida por SHA-256 e não suposta

O worktree de `858903f7` **existe** — a revisão independente supôs que não, e
declarou isso como limitação sua. Ele foi lido, e só lido:

| arquivo em `858903f7` | SHA-256 medido | esperado pelo coordenador |
|---|---|---|
| `crates/seele-conformance/tests/bateria_interna.rs` | `e47a4b9a9612dc873b419e445931830d016c7059b8ca3278a8f60df1e416f932` | **igual** |
| `crates/seele-core/src/enlace.rs` | `2dd297aa814b289f30b08d84a60e717a2742eabbb87937305bf5d43f1961d1ad` | **igual** |

Os dois arquivos de origem continuam **intactos**: nada foi escrito naquele
diretório, inclusive porque um processo antigo pode ainda estar pendurado nele.

A única diferença de `enlace.rs` lá para a base é a mutação, e ela é exatamente
o que não veio para cá:

```diff
 fn a_sessao_acabou_aqui(motivo: DisconnectReason) -> bool {
-    matches!(motivo, DisconnectReason::Kicked | DisconnectReason::Banned)
+    // MUTAÇÃO NEGATIVA TEMPORÁRIA — restaurar antes de commitar.
+    let _ = motivo;
+    true
 }
```

Do `bateria_interna.rs` veio o teste novo (+142 linhas), com duas diferenças
deliberadas para o texto de lá: o `fmt` do repositório aplicado, e o doc
retificado (§4).

## 3 · O teste, revisado item a item

`uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao`, em
`crates/seele-conformance/tests/bateria_interna.rs`. Ele injeta
`Event::SessionEnded { reason: FellBehind }` **pelo mesmo braço de servidor que
a expulsão usa** — a única diferença entre ele e
`expulsar_acaba_com_a_sessao_e_deixa_voltar` é qual `DisconnectReason` viajou. E
cobra as quatro coisas:

1. **A despedida chegou pelo protocolo** — `ServerMessage::Disconnecting { reason: FellBehind }`
   recebido pelo `Enlace`. Sem isso o teste passaria com uma queda de transporte
   qualquer, que é outra coisa e já tem teste na mesma suíte.
2. **O mesmo `Enlace` público sobrevive** — espera `Reconectado` e trata
   `Encerrado` como falha explícita, com a mensagem que nomeia a regressão.
3. **A reconexão é efetiva** — `estado() == Online` e `ssrc` **diferente** do de
   antes. O `ssrc` é por conexão, então número novo é conexão nova, e não um
   aviso sobre nada.
4. **Ela serve** — `dizer(...)` e ouvir de volta na mesma Linha. Sem
   `join_channel` do outro lado o servidor aceita a mensagem e não a devolve
   para ninguém; voltar mudo seria voltar para nada.

## 4 · A retificação: onde a afirmação falsa morava

O doc do guarda unitário `toda_despedida_do_protocolo_escolhe_um_lado`, em
`crates/seele-core/src/enlace.rs`, afirmava que, com a função devolvendo `true`
para tudo, **«a suíte inteira do workspace continua verde»**. É falso, e quem
desmente é o próprio teste logo abaixo da afirmação: ele não confere a tabela
contra si mesma — escreve a decisão esperada variante a variante, à parte da
função, e compara.

A revisão tem razão em onde isso importa: o esclarecimento existia só no doc do
teste novo, em outro crate. Quem abrisse `enlace.rs` continuava lendo a
afirmação falsa. **A retificação agora está lá**, no doc do próprio guarda, sob
o título «Retificação de 10/09», com a distinção entre as duas evidências:

- **o guarda unitário falha** sob a mutação — é prova contra deriva, e não de
  comportamento;
- **a lacuna real, mais estreita**, era não haver nenhum teste de
  **comportamento** no lado de reconectar da fronteira, com um servidor de
  verdade escrevendo a despedida no fio. É essa que o teste novo fecha.

`docs/superpowers/sdd/2026-09-10-consolidacao-para-a-main/relatorio.md:216-220`
carrega o mesmo texto. **A frase histórica não foi reescrita** — continua lá,
letra por letra —, mas a revisão apontou que ela ficava legível sem caminho até a
correção. Passou a haver, logo abaixo dela, um bloco de retificação que diz o que
a medida mostrou e aponta para cá. Ver §9.3.

## 5 · Comandos, códigos de saída e o que cada um mediu

Todos rodados de dentro deste worktree, sem `|` que engula código de saída, com
`$?` capturado logo depois. `target/` é o deste worktree; nada foi compartilhado
com a origem.

### 5.1 · A candidata, limpa

| comando | exit | resultado |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | sem diferença |
| `cargo test -p seele-conformance --test bateria_interna` | **0** | `5 passed; 0 failed` — inclui o teste novo |
| `cargo test -p seele-conformance --test moderacao` | **0** | `6 passed; 0 failed` |
| `cargo test -p seele-core --lib` | **0** | `297 passed; 0 failed` — inclui `toda_despedida_do_protocolo_escolhe_um_lado` |
| `cargo test` / `cargo test --no-fail-fast` (workspace) | **101, 101, 0, 0** | quatro corridas da manhã; **1771 testes em 70 alvos**; ver §6 |
| `cargo test --workspace --no-fail-fast` (árvore final) | **0, 0, 0** | três corridas seguidas, `1771 passed; 0 failed` em cada; ver §6.2 |

```
running 5 tests
test sair_encerra_sem_esperar_a_bateria ... ok
test uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao ... ok
test o_server_cai_e_a_sessao_entra_na_bateria_em_vez_de_acabar ... ok
test o_que_a_pessoa_escolheu_volta_com_ela ... ok
test as_tentativas_aparecem_enquanto_a_bateria_corre ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.29s
```

### 5.2 · A prova por reversão — «existir não é funcionar»

A mutação foi aplicada **nesta árvore**, medida, e retirada. Hash antes e depois
da restauração, do arquivo inteiro:

```
$ shasum -a 256 crates/seele-core/src/enlace.rs   # antes de mutar
8b21356f1a3fa75062d1a48ae31902df8d83b864d2b0bc6877b81aa8bb841e67
$ shasum -a 256 crates/seele-core/src/enlace.rs   # depois de restaurar
8b21356f1a3fa75062d1a48ae31902df8d83b864d2b0bc6877b81aa8bb841e67
```

> **Procedência deste par (§9).** Ele foi medido na árvore de trabalho daquele
> momento, ainda antes da redação final do doc do guarda, e por isso **não bate
> com o arquivo entregue**. O que ele prova continua de pé — antes e depois são
> iguais, logo a mutação saiu inteira —, mas quem quiser reproduzir tem de usar o
> par do arquivo commitado, medido de novo hoje e igual em todos os quatro
> commits desta candidata: `6d34140f8535b61d59bb377ed0c5501a7ca90123b69a89926e3010f2976d6f4a`.

Com a mutação viva:

| comando | exit | o que disse |
|---|---|---|
| `cargo test -p seele-conformance --test bateria_interna uma_despedida_recuperavel_…` | **101** | `FAILED` |
| `cargo test -p seele-core --lib enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado` | **101** | `FAILED` |
| `cargo test -p seele-conformance --test moderacao` | **0** | `6 passed` — a mutação **não** é vista aqui |

O teste novo falha dizendo exatamente a regressão que ele existe para pegar
(saída **da árvore do commit `5146c04`**, onde o `panic!` morava na linha 306; na
árvore entregue ele está em 370 — ver §9):

```
thread 'uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao' panicked at
crates/seele-conformance/tests/bateria_interna.rs:306:43:
uma despedida recuperável acabou com a sessão (Moderado(FellBehind)): quem a bateria
interna existe para segurar foi posto para fora
```

E o guarda unitário falha na **primeira** variante da lista, o que fecha a §4:

```
thread 'enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado' panicked at
crates/seele-core/src/enlace.rs:4760:13:
assertion `left == right` failed: Incompatible mudou de lado sem que este teste mudasse junto
  left: true
 right: false
```

> A linha citada **dentro de `enlace.rs`** depende do tamanho da própria mutação:
> o `assert_eq!` mora em 4758 no arquivo entregue, e a mutação daquela corrida
> ocupava duas linhas a mais que o original, empurrando-o para 4760. A mensagem e
> a variante que cai são o que importa, e as duas conferem. Ver §9.

Que `moderacao` continue verde sob a mutação não é detalhe: é a medida de que a
expulsão e o banimento seguem encerrando a sessão como devem, e de que a suíte
que já existia era, sozinha, cega para o lado de reconectar.

### 5.3 · As quatro suítes do aceite, com a saída inteira e o `$?` de cada uma

A revisão cobrou isto com todas as letras: a aprovação não podia depender da
palavra de quem executou. Segue a saída **completa** das cinco verificações, na
árvore final desta candidata — cada `EXIT=` é o `$?` do comando imediatamente
acima, sem `|` no meio. Reexecutadas **depois** das mudanças de §6.2, para que
não sejam a medida de uma árvore anterior.

```
### 2026-09-10T15:37:34Z · load averages: 1.80 4.61 6.33

$ cargo fmt --all -- --check
EXIT=0

$ cargo test -p seele-conformance --test bateria_interna
running 5 tests
test sair_encerra_sem_esperar_a_bateria ... ok
test uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao ... ok
test o_server_cai_e_a_sessao_entra_na_bateria_em_vez_de_acabar ... ok
test o_que_a_pessoa_escolheu_volta_com_ela ... ok
test as_tentativas_aparecem_enquanto_a_bateria_corre ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.09s
EXIT=0

$ cargo test -p seele-conformance --test bateria_interna -- --exact \
      uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao
running 1 test
test uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.41s
EXIT=0

$ cargo test -p seele-core --lib -- --exact \
      enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado
running 1 test
test enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 296 filtered out; finished in 0.00s
EXIT=0

$ cargo test -p seele-conformance --test moderacao
running 6 tests
test banir_acaba_com_a_sessao_e_impede_de_voltar ... ok
test expulsar_acaba_com_a_sessao_e_deixa_voltar ... ok
test um_operador_modera_pessoas_e_nao_o_comandante ... ok
test mover_leva_a_pessoa_e_a_conta_na_sala_nova ... ok
test um_pessoa_comum_e_recusado_pelo_server_e_nao_pela_casca ... ok
test apagar_uma_mensagem_tira_ela_da_conversa_de_todo_mundo ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
EXIT=0
```

E a prova por reversão, refeita **na árvore entregue** — a corrida abaixo é a de
2026-09-10T16:19Z, feita depois da revisão, com o arquivo commitado e restaurado
ao fim (saída podada nas duas linhas que importam; os `EXIT=` são os capturados,
sem `|`):

```
$ cargo test -p seele-conformance --test bateria_interna -- --exact uma_despedida_…
panicked at crates/seele-conformance/tests/bateria_interna.rs:370:43:
uma despedida recuperável acabou com a sessão (Moderado(FellBehind)): quem a bateria
interna existe para segurar foi posto para fora
EXIT=101

$ cargo test -p seele-core --lib -- --exact enlace::tests::toda_despedida_…
panicked at crates/seele-core/src/enlace.rs:4758:13:
assertion `left == right` failed: Incompatible mudou de lado sem que este teste mudasse junto
  left: true
 right: false
EXIT=101

$ cargo test -p seele-conformance --test moderacao
test result: ok. 6 passed; 0 failed
EXIT=0
```

## 6 · O `exit 101` da validação independente de `858903f7`, esclarecido

O relatório daquela tarefa registra, às 05:55:08Z:

> `Falha: /Users/dev-alexandre/.cargo/bin/cargo encerrou com exit status: 101`

e o trecho salvo contém **apenas linhas de `Compiling`** — não chega a nenhum
`test result`. O trecho, sozinho, não diz a causa. Não foi presumida; foi
medida, e por dois lados:

1. **O que a árvore validada continha.** O comando rodou em
   `.orbita/worktrees/858903f7-…`, e o `enlace.rs` de lá **tem a mutação até
   hoje** (SHA-256 `2dd297aa…`, §2). A mutação foi aplicada às ~05:51Z; a
   validação disparou às 05:55:08Z, depois disso, e nada a restaurou.
2. **O que a mutação faz com o `exit`.** Reproduzido aqui, nesta árvore:
   `exit 101` sob a mutação, em duas suítes distintas (§5.2). É o código que o
   `cargo test` usa para *teste que falhou*, e é o mesmo número relatado lá.

Como a mutação derruba **sempre** os dois testes de §5.2, ela é causa
**suficiente e determinística** do `exit 101` naquela árvore: com ela viva,
nenhuma corrida de `cargo test` ali podia terminar em 0.

Uma segunda causa possível — falha de **compilação** — foi considerada e não se
sustenta: o mesmo código compila aqui, e a mutação (`let _ = motivo; true`) não
quebra compilação; produz teste vermelho.

**Uma terceira, que só apareceu porque foi medida — e que corrige o que eu ia
escrever.** A primeira versão desta seção afirmava que a candidata limpa
terminava em exit 0 e que, portanto, não havia falha atual nenhuma a separar.
**Isso estava errado**, e a medida é §6.1: existe uma instabilidade de tempo
nesta suíte que também produz `exit 101`. Ela não substitui a explicação — a
mutação basta sozinha, e estava lá —, mas impede dizer que a mutação é a *única*
coisa que pode ter contribuído naquela corrida.

**O que continua indisponível, dito com precisão:** a saída completa daquela
execução não foi preservada — o relatório guarda só o prefixo de compilação. Não
existe, portanto, a linha `test … FAILED` *daquela* corrida, e por isso não há
como dizer **quais** testes falharam lá, nem separar a contribuição da mutação
da contribuição da instabilidade de §6.1. A leitura direta exigiria rodar
`cargo test` **dentro** do worktree de `858903f7` — o que não foi feito de
propósito: aquele diretório é de leitura só, e escrever nele (inclusive em
`target/`) mexeria numa árvore que a tarefa manda preservar e que pode ter
processo pendurado.

### 6.1 · A instabilidade que a medida encontrou, e que não é desta tarefa

O workspace desta candidata foi rodado **quatro** vezes. Duas terminaram em
`101`, com **um** teste vermelho cada — e testes **diferentes**, os dois com o
mesmo erro:

| corrida | comando | exit | quem falhou |
|---|---|---|---|
| 1 | `cargo test` | 101 | `acceptance_m3::a_returning_person_reclaims_their_seat_and_their_ssrc` — `Error: SemResposta` |
| 2 | `cargo test --no-fail-fast` | 101 | `acceptance_m2::a_forged_ssrc_is_refused` — `Error: SemResposta` |
| 3 | `cargo test --no-fail-fast` | **0** | `1771 passed; 0 failed` |
| 4 | `cargo test --no-fail-fast` | **0** | `1771 passed; 0 failed` |

`SemResposta` é `quinn::ConnectionError::TimedOut` traduzido em
`client.rs:1509`: **prazo de conexão estourado**, e não asserção violada. Os dois
testes conectam em `127.0.0.1` e dependem de tempo de parede
(`sleep(150ms)`/`sleep(300ms)` em `acceptance_m3.rs:343,350`). Rodada isolada, a
suíte passa: 3/3 no teste, 2/2 na suíte.

> **Retificação medida, escrita depois dos dois commits.** Esta seção dizia:
> «sob o workspace inteiro a suíte `acceptance_m3` leva 20,2 s; sozinha, 1,7 s —
> doze vezes mais devagar». **A frase estava errada, e o erro era de leitura do
> próprio número.** Os 20,15 s são a duração da suíte na corrida **vermelha** —
> quer dizer, o tempo que o prazo levou para estourar, e não o tempo que a suíte
> leva quando roda. Usar esse número para explicar por que o prazo estourou é
> circular: ele **é** o prazo. E 20 s é exatamente `IDLE_TIMEOUT`
> (`crates/seele-proto/src/transport.rs:33`).
>
> O tempo real da suíte sob o workspace inteiro foi medido nas duas corridas
> verdes: `acceptance_m3` em **1,74 s** e **1,77 s**; `acceptance_m2` em
> **0,95 s** e **0,97 s** — contra 1,7 s e ~0,9 s isoladas. Ou seja: **não há
> lentidão de doze vezes**; sob o workspace a suíte custa praticamente o mesmo
> que sozinha. A única suíte que leva ~20 s numa corrida verde é `acceptance_m5`,
> e de propósito: ela conecta em `127.0.0.1:1`, onde nada escuta.
>
> O que sobra, e é menos do que a frase antiga prometia: as duas falhas são
> prazos de conexão estourados, os dois testes dependem de tempo de parede, e a
> causa dessa espera **não foi identificada** por medida nenhuma feita aqui.
> Contenção continua sendo a hipótese mais simples, e §6.2 registra as tentativas
> de confirmá-la — todas negativas.

Nada disso toca a fronteira entre acabar e reconectar, e **o diff desta entrega
não altera uma linha de produção** — só acrescenta um teste em outro crate e
corrige comentários. Ainda assim, a pergunta honesta é se o teste novo, que
custa ~21 s de servidor e reconexão em paralelo, **aumenta** a chance da
instabilidade. Foi medida, e a medida não fecha:

| árvore | corridas | verdes |
|---|---|---|
| candidata (com o teste novo, 1771 testes) | 4 | 2 |
| base `81edc32` (sem o teste novo, 1770 testes) | 3 | **3** |

Três verdes contra duas falhas em quatro **não decide nada** com esta amostra, e
não vou fingir que decide. O que se pode afirmar: as duas falhas foram as duas
corridas **iniciais**, as cinco seguintes (2 da candidata + 3 da base) foram
verdes, os testes que caíram são diferentes entre si e nenhum tem relação com o
escopo. **Evidência indisponível, declarada:** a carga externa da máquina no
momento das duas primeiras corridas não foi registrada — é a máquina do
operador, com outros worktrees e sessões possíveis —, então a hipótese mais
simples (contenção de CPU vinda de fora) não pôde ser confirmada nem descartada.

**Não foi consertado aqui, de propósito:** apertar prazo ou serializar suíte é
mudança de produção/infra fora deste escopo, e a tarefa manda não ampliar. Fica
registrado como pendência: *`acceptance_m2` e `acceptance_m3` dependem de tempo
de parede e falham por `SemResposta` sob carga.* Note que o relatório histórico
de 10/09 registra `1769 passaram, 0 falharam` numa única corrida — uma corrida
verde não mede uma falha intermitente.

### 6.2 · O que foi medido **depois** dos dois commits

A revisão apontou, com razão, que a entrega estava congelada num estado anterior
às medições que vieram depois — e que a seção que o aceite manda registrar com
precisão era justamente a que dependia delas. Esta seção é essa lacuna fechada, e
o commit que a carrega é o terceiro desta candidata.

#### O teste novo ficou vermelho duas vezes, e as duas no mesmo lugar

Nas duas, em `bateria_interna.rs`, na primeira asserção: a despedida recuperável
não apareceu no `Enlace` dentro de 10 s.

| quando | em que corrida | máquina |
|---|---|---|
| 11:48 | workspace inteiro, sob saturação provocada (carga ~31) | junto com `tela_por_um_par`, da base, que caiu na mesma corrida |
| 12:20 | `bateria_interna` sozinha | carga 1 min ≈ 2,3 |

A segunda é a que desmonta a explicação fácil: **não havia carga**. Depois dela,
o teste passou **44 vezes seguidas**, em quatro regimes diferentes — e nenhum
deles reproduziu a falha:

| regime | corridas | vermelhas |
|---|---|---|
| suíte em paralelo, máquina ociosa | 3 | 0 |
| suíte com `--test-threads=1` | 3 | 0 |
| suíte sob saturação de CPU e E/S (30 processos + 3 de disco, carga até **32,6**) | 6 | 0 |
| **4 cópias da suíte ao mesmo tempo** (contenção de runtime, ~300 threads em 15 núcleos) | 24 | 0 |

Nas corridas D e E do workspace sob saturação, o teste novo passou e quem caiu
foi `tela_por_um_par`, da base — testes **diferentes** a cada vez, sempre por
prazo de espera. É o mesmo desenho de falha de §6.1, e ele não é desta entrega.

#### Quatro hipóteses, medidas e **refutadas**

Nenhuma sobreviveu à medida, e isso é o que há de mais sólido a dizer aqui:

| hipótese | como foi testada | resultado |
|---|---|---|
| a conexão morre antes de o quadro sair: `despedir` espera 1 s pelo `stopped()` e fecha (`seele-server/src/session.rs:458`) | a espera foi **zerada** nesta árvore (`from_secs(1)` → `from_millis(0)`), o pior caso possível | **refutada** — 3/3 verdes; o quadro chega mesmo com o fechamento imediato |
| a janela do barramento: a sessão do servidor só assina os eventos **depois** do handshake (`session.rs:1243`), e um evento injetado antes disso se perde | teste temporário com 20 rodadas injetando **logo depois** de `conectar`, sem barreira nenhuma | **refutada** — 20/20 chegaram (e 20/20 com barreira) |
| saturação de CPU e disco | 6 corridas da suíte com carga até 32,6 | **refutada** — 6/6 verdes |
| contenção de runtime como a de uma corrida de workspace | 4 cópias simultâneas da suíte, 6 rodadas | **refutada** — 24/24 verdes no teste novo |

A mutação de `despedir` foi retirada e o arquivo conferido por SHA-256
(`bbb538b9…`, igual antes e depois); o teste temporário foi apagado.

**O mecanismo continua sem nome, e está dito assim de propósito.** O que há é
uma falha rara — duas em cerca de cinquenta execuções medidas hoje — que
nenhuma das quatro hipóteses explica. Concluir mais do que isso seria a terceira
vez que uma hipótese confiante custa mais do que a medida teria custado.

#### O que mudou no teste por causa disso

Duas coisas, as duas em teste; **nenhuma linha de produção foi alterada**.

1. **Um vermelho que diz qual das duas coisas aconteceu.** A asserção antiga
   dizia só «a despedida recuperável não chegou ao Enlace» — e sob essa frase
   cabem duas situações opostas: o servidor não ter escrito a despedida no fio
   (regressão do lado de lá) ou a conexão ter caído levando o motivo junto
   (a explicação perdida, não a decisão). O teste agora **mede** o estado do
   `Enlace` e o `ssrc` no instante da desistência e nomeia qual das duas é. É o
   critério que faltava para separar falha de ambiente de regressão futura: quem
   ler o próximo vermelho não vai precisar adivinhar.
2. **Uma barreira antes da injeção.** `abrir_linha` e `entrar_na_voice_room` não
   esperam resposta, e `estado()` é leitura local: nada disso provava que o
   servidor já havia processado alguma coisa desta conexão. O teste agora diz
   uma frase na Linha e **espera ela voltar** antes de injetar a despedida. Isso
   fecha por construção a classe inteira da segunda hipótese — a mensagem só
   volta se o laço da sessão a leu, e o laço começa depois de a sessão assinar o
   barramento. **Não é apresentada como conserto da falha observada:** a
   hipótese que ela fecha foi refutada, e nada aqui prova que era essa a causa.

A prova por reversão foi **refeita depois destas mudanças**, para que a
sensibilidade à mutação não fosse tomada como herdada: com
`a_sessao_acabou_aqui` devolvendo `true` para tudo, o teste volta a cair em
`Moderado(FellBehind)`, o guarda unitário cai em `Incompatible` e `moderacao`
segue verde. `enlace.rs` foi restaurado e conferido por SHA-256
(`6d34140f…`, igual antes e depois).

#### O conjunto inteiro, três vezes seguidas, verde

A revisão cobrou que a candidata não tinha corrida de workspace verde
**repetível**. Na árvore final, depois de tudo o que esta seção descreve:

| corrida | começou | exit | resultado |
|---|---|---|---|
| F | 12:38 | **0** | `1771 passed; 0 failed`, 70 alvos |
| G | 12:42 | **0** | `1771 passed; 0 failed`, 70 alvos |
| H | 12:45 | **0** | `1771 passed; 0 failed`, 70 alvos |

Com as quatro da manhã, a conta do dia nesta candidata é **cinco verdes em
sete**, e as duas vermelhas foram as **duas primeiras**. Três verdes seguidas não
provam que a instabilidade de §6.1 sumiu — ela é intermitente, e sumir não é uma
coisa que três corridas mostrem —, mas desfazem a leitura de que a candidata não
fecha verde de forma repetível: fecha, e fechou três vezes seguidas com os 1771
testes, sem `--no-fail-fast` escondendo nada (ele estava ligado nas três).

#### Um comando do aceite estava medindo nada

Registrado porque é exatamente o defeito que esta casa mais paga caro:

```
$ cargo test -p seele-core --lib toda_despedida_do_protocolo_escolhe_um_lado -- --exact
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 297 filtered out
EXIT=0
```

**Exit 0 com zero teste executado.** Com `--exact`, o filtro tem de ser o caminho
inteiro; o nome curto não casa com nada, e o `cargo` chama isso de sucesso. O
comando correto é `-- --exact enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado`,
e é ele que está em §5.3 — onde a linha `running 1 test` prova que o guarda
rodou. A tabela de §5.1 não dependia disso: lá o guarda foi medido dentro de
`cargo test -p seele-core --lib`, com os 297.

## 7 · Limitações desta entrega

- **Nada de Windows.** Nenhuma tentativa foi repetida: o ambiente não mudou
  desde o registro da consolidação (sem SDK do Windows, sem `pwsh`). A pendência
  da matriz de três SOs continua aberta, como já estava escrito.
- **A prova é de uma máquina.** Servidor local, `Location::Memory`, uma sala,
  uma Linha, `127.0.0.1`. Nada aqui mede rede de verdade, NAT ou latência.
- **O `exit 101` de `858903f7` está explicado, não relido.** Ver o parágrafo de
  indisponibilidade em §6.
- **O workspace desta candidata não é verde em toda corrida.** Duas de sete
  terminaram em `101` por `SemResposta`, sem relação com este escopo (§6.1) — as
  duas de manhã; as três da árvore final foram verdes (§6.2). Não foi
  consertado, e não deve ser lido como se fosse.
- **O mecanismo da falha intermitente do teste novo continua sem nome.** Quatro
  hipóteses foram medidas e **refutadas** (§6.2), e nenhuma das duas ocorrências
  foi diagnosticada — o diagnóstico só passa a existir daqui para a frente, na
  própria mensagem do teste. Dizer «era contenção» seria repetir o erro que §6.1
  corrige. O risco entra na suíte **de olhos abertos**: um teste de ~21 s com
  intermitência conhecida, mitigado só pela mensagem que separa as duas leituras
  possíveis do vermelho. A revisão `5d3668ea` mediu por conta própria e o teste
  passou nas três execuções dela (suíte, teste isolado e workspace); somadas às
  desta entrega, isso não fecha o mecanismo, e não é apresentado como se
  fechasse.
- **Um caminho do servidor descarta o erro da despedida, e isso não foi medido
  como causa de nada.** `frame::write(... Disconnecting ...)` sai sob `let _ =`
  e `despedir` desiste depois de 1 s (`seele-server/src/session.rs:458`): se o
  motivo se perder, ninguém fica sabendo dos dois lados. Zerar aquela espera
  **não** derrubou o teste (§6.2), então isto fica anotado como observação sobre
  o produto — fora deste escopo, e sem evidência de ter causado as duas
  vermelhas.
- **Os 1769 testes da candidata anterior não foram recontados como se fossem
  desta tarefa.** Os números de §5 são de agora, deste worktree, e estão
  separados de propósito.
- **A campanha do crate inteiro sob mutação não foi repetida.** A prova negativa
  específica já existia e foi refeita aqui de forma dirigida (§5.2); rodar tudo
  vermelho de novo custaria minutos e não acrescentaria evidência.

## 8 · O que a revisão `5d3668ea` deve consumir

- **Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/496ba8c5-8040-415c-9032-b1420580f1ab`
- **Branch:** `orbita/496ba8c5`
- **Commit da candidata:** `5146c04ba61742fb21db0513ed34506ee5c7b128` —
  `test(enlace): a despedida recuperável reconecta, e agora há prova por fora`.
  Pai: `81edc3219f484219ed665385e76bf1b566ae677e`, a base, intacta.
  Diff: `+457 −4` em três arquivos — o teste novo (+142 −0), a retificação do doc
  de `enlace.rs` (**+18 −4**) e este relatório (+297 −0). Os números são de
  `git diff --numstat`; o `22` que aparece no `--stat` é o **total de linhas
  mexidas**, não de adições (ver §9).
- **Correção da revisão:** `d3522a2c6969734eefa96e1327cb333e94ad1c1b` —
  `test(enlace): o vermelho que faltava dizer, e a medida que desfaz a explicação errada`.
  Diff: `+301 −11` em dois arquivos — o teste (**+68 −4**: a barreira e o vermelho
  que se explica) e este relatório (+233 −7: §5.1, §5.3, §6.1, §6.2 e §7). O `72`
  publicado antes era o mesmo engano de leitura do `--stat`. **Nenhuma
  linha de produção**: `git show --stat d3522a2` mostra os dois arquivos, e
  nenhum deles está em `crates/*/src`.
- **Retificação dos números (§9):** `a022f06641a54e7984853d1194b60c5fe4c37e08` —
  `docs(retificação): os números medidos de novo, e o ponteiro que faltava no histórico`.
  Diff: `+139 −14` neste relatório e `+12 −0` no histórico. **Nenhuma linha de
  código** — nem de produção, nem de teste: `git show --numstat a022f06` não lista
  nada em `crates/`.
- **Ponta a consumir:** o sexto commit, que só acrescenta a esta seção o SHA do
  quinto. `git log --oneline -6` no worktree confirma a sequência, e o sexto é a
  ponta de `orbita/496ba8c5`.
- **Estado Git:** limpo — nada pendente na árvore de trabalho. A base
  `81edc32` continua sendo ancestral direta, sem merge refeito e sem reescrita.

Nada foi feito fora deste worktree: sem `push`, sem `main`, sem tag, sem
release, sem chave, sem tocar em `858903f7` ou em qualquer outra origem.

## 9 · Retificação pedida pela revisão `5d3668ea`

A revisão aprovou a candidata duas vezes — uma por leitura, outra remedindo tudo
por conta própria — e deixou quatro travas. Três eram números **errados neste
relatório**; a quarta é risco já declarado. Nenhuma tocava o comportamento do
produto, e a correção abaixo também não toca: ela mexe em texto e num ponteiro.

**Todos os números publicados aqui foram medidos de novo hoje, na árvore
entregue.** O padrão do erro era sempre o mesmo — número colhido numa árvore de
trabalho intermediária, ou lido do `--stat` como se fosse adição — e é ele que
esta seção fecha.

| o que a revisão apontou | o que estava escrito | o que a medida diz | onde |
|---|---|---|---|
| linha do vermelho da prova por reversão | `bateria_interna.rs:366:43` | **`370:43`** — corrida de hoje, `EXIT=101` | §5.3 |
| linha do vermelho do guarda unitário | `enlace.rs:4760:13` | **`4758:13`** com mutação do mesmo tamanho do original; `4760` sai quando a mutação ocupa duas linhas a mais | §5.2, §5.3 |
| tamanho da retificação de `enlace.rs` | `+22 −4` | **`+18 −4`** (`git diff --numstat 81edc32 5146c04`); `22` é o total de linhas mexidas do `--stat` | §8 |
| tamanho do teste no commit da correção | `+72 −0` | **`+68 −4`** — mesmo engano, mesmo `--stat` | §8 |
| par de SHA-256 de `enlace.rs` na prova por reversão | `8b21356f…` | não bate com o arquivo entregue: o par é de uma árvore intermediária. O do arquivo commitado, igual nos quatro commits, é **`6d34140f…`** | §5.2 |

### 9.1 · A prova por reversão, refeita na árvore entregue

Não bastava trocar o número: publicar linha que não foi medida é o defeito que
se está corrigindo. A mutação foi reaplicada **no arquivo commitado**, medida, e
retirada — `a_sessao_acabou_aqui` devolvendo `true` para todo motivo, três linhas
como as três originais:

```
$ shasum -a 256 crates/seele-core/src/enlace.rs        # antes de mutar
6d34140f8535b61d59bb377ed0c5501a7ca90123b69a89926e3010f2976d6f4a

$ cargo test -p seele-conformance --test bateria_interna \
    -- --exact uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao
panicked at crates/seele-conformance/tests/bateria_interna.rs:370:43:
uma despedida recuperável acabou com a sessão (Moderado(FellBehind)): quem a bateria
interna existe para segurar foi posto para fora
EXIT_A=101

$ cargo test -p seele-core --lib \
    -- --exact enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado
panicked at crates/seele-core/src/enlace.rs:4758:13:
assertion `left == right` failed: Incompatible mudou de lado sem que este teste mudasse junto
EXIT_B=101

$ cargo test -p seele-conformance --test moderacao
test result: ok. 6 passed; 0 failed
EXIT_C=0

$ git checkout -- crates/seele-core/src/enlace.rs
$ shasum -a 256 crates/seele-core/src/enlace.rs        # depois de restaurar
6d34140f8535b61d59bb377ed0c5501a7ca90123b69a89926e3010f2976d6f4a
$ git status --porcelain                               # vazio
```

O desenho continua o mesmo, e agora com par de hash que qualquer um reproduz no
arquivo entregue: o teste novo e o guarda unitário caem, `moderacao` não vê nada.

### 9.2 · O aceite, remedido depois da restauração

Para que a retificação não fosse entregue sobre uma árvore só presumida limpa
(2026-09-10T16:24Z, carga 1 min ≈ 4,1):

| comando | exit | resultado |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | sem diferença |
| `cargo test -p seele-conformance --test bateria_interna` | **0** | `5 passed; 0 failed` — inclui o teste novo, 21,09 s |
| `cargo test -p seele-conformance --test moderacao` | **0** | `6 passed; 0 failed` |
| `cargo test -p seele-core --lib -- --exact enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado` | **0** | `running 1 test`, `1 passed`, 296 filtrados |

E o produto inteiro mais uma vez, porque nesta sessão houve código mutado e
restaurado, e restaurar sem remedir seria supor:

```
$ cargo test --workspace --no-fail-fast        # 16:27Z → 16:30Z
EXIT_WS=0
1771 testes passaram, em 70 alvos; nenhuma linha `FAILED`, nenhum `panicked at`
```

Corrida **I**, na mesma numeração de §6.2: a quarta verde seguida na árvore
final. A validação independente que acompanhou a revisão também fechou verde,
sem um único alvo vermelho — o `exit 101` que abriu esta tarefa não voltou, e §6
já explica de onde ele vinha.

### 9.3 · A divergência documental, fechada dos dois lados

A revisão observou, com razão, que a frase falsa continuava legível **sem
caminho até a correção**: quem abrisse
`docs/superpowers/sdd/2026-09-10-consolidacao-para-a-main/relatorio.md` leria «a
suíte inteira do workspace continua verde» sob a mutação e não teria como saber
que isso foi medido como falso.

O relatório histórico **não foi reescrito**: a frase original continua ali, letra
por letra. O que se acrescentou foi um ponteiro imediatamente abaixo dela,
dizendo o que a medida mostrou e para onde ir. Corrigir o texto antigo apagaria o
erro; o ponteiro o deixa legível junto com sua retificação, que é o que a §4
deste relatório existe para registrar.

### 9.4 · O que **não** mudou

- Nenhuma linha de `crates/*/src` e nenhuma linha de teste. O diff desta
  retificação é este relatório e o ponteiro no histórico.
- A base `81edc32` continua ancestral direta; a origem `858903f7` continua
  intocada; nada de `push`, `main`, tag, release ou chave.
- A trava do mecanismo intermitente **continua aberta** e está em §7. Ela não
  foi fechada aqui, e nada nesta seção deve ser lido como se tivesse fechado.
