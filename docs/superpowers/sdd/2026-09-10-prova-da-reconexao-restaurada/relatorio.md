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
carrega o mesmo texto e **não foi tocado**: é relatório histórico, e a
retificação é este documento.

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
| `cargo test` / `cargo test --no-fail-fast` (workspace) | **101, 101, 0, 0** | quatro corridas; **1771 testes em 70 alvos**; ver §6 |

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

Com a mutação viva:

| comando | exit | o que disse |
|---|---|---|
| `cargo test -p seele-conformance --test bateria_interna uma_despedida_recuperavel_…` | **101** | `FAILED` |
| `cargo test -p seele-core --lib enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado` | **101** | `FAILED` |
| `cargo test -p seele-conformance --test moderacao` | **0** | `6 passed` — a mutação **não** é vista aqui |

O teste novo falha dizendo exatamente a regressão que ele existe para pegar:

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

Que `moderacao` continue verde sob a mutação não é detalhe: é a medida de que a
expulsão e o banimento seguem encerrando a sessão como devem, e de que a suíte
que já existia era, sozinha, cega para o lado de reconectar.

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
(`sleep(150ms)`/`sleep(300ms)` em `acceptance_m3.rs:343,350`). Sob o workspace
inteiro a suíte `acceptance_m3` leva **20,2 s**; sozinha, **1,7 s** — doze vezes
mais devagar. Rodada isolada, ela passa: 3/3 no teste, 2/2 na suíte.

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

## 7 · Limitações desta entrega

- **Nada de Windows.** Nenhuma tentativa foi repetida: o ambiente não mudou
  desde o registro da consolidação (sem SDK do Windows, sem `pwsh`). A pendência
  da matriz de três SOs continua aberta, como já estava escrito.
- **A prova é de uma máquina.** Servidor local, `Location::Memory`, uma sala,
  uma Linha, `127.0.0.1`. Nada aqui mede rede de verdade, NAT ou latência.
- **O `exit 101` de `858903f7` está explicado, não relido.** Ver o parágrafo de
  indisponibilidade em §6.
- **O workspace desta candidata não é verde em toda corrida.** Duas de quatro
  terminaram em `101` por `SemResposta`, sem relação com este escopo (§6.1). Não
  foi consertado, e não deve ser lido como se fosse.
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
  Diff: `+457 −4` em três arquivos — o teste novo (+142), a retificação do doc
  de `enlace.rs` (+22 −4) e este relatório.
- **Ponta a consumir:** este segundo commit, que só acrescenta o SHA acima a
  esta seção. `git log --oneline -3` no worktree confirma os dois.
- **Estado Git:** limpo — nada pendente na árvore de trabalho.

Nada foi feito fora deste worktree: sem `push`, sem `main`, sem tag, sem
release, sem chave, sem tocar em `858903f7` ou em qualquer outra origem.
