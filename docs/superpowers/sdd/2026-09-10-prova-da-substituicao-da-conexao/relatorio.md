# A substituição da conexão, provada com um servidor que cai e volta

**Data:** 2026-09-10
**Worktree:** `/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/94b614e6-bb94-4639-89b4-7d83de2f92db`
**Branch:** `orbita/94b614e6` · **Base:** `d5628513cfefd385b9cc5b22b5cedd427f2dbf2e`

Nenhum commit foi criado, a `main` não foi movida, nada foi publicado, nenhuma
chave foi tocada, nenhum worktree de fora deste diretório foi modificado e
nenhum relatório histórico foi editado. **Nenhum arquivo de produção foi
alterado por esta tarefa** — `enlace.rs` e `session.rs` terminam byte a byte
iguais aos importados, conferidos por SHA-256 depois da última reversão.

Esta é a etapa que o §8 do relatório anterior nomeou e deixou aberta.

---

## 1 · A importação, conferida antes de mexer

`a695fe5` (ponta em que este worktree nasceu) é **ancestral** de `d562851` —
conferido, não suposto —, então a incorporação foi avanço rápido: nenhum commit
criado, nenhum merge refeito.

```
$ git merge-base --is-ancestor d5628513… HEAD   →  «NÃO é ancestral»
$ git merge-base HEAD d5628513…                 →  a695fe5  (logo, a695fe5 ⊂ d562851)
$ git merge --ff-only d5628513…
Updating a695fe5..d562851 · Fast-forward · 113 arquivos
```

Os quatro conteúdos não commitados vieram do worktree `c0d9355c` e bateram por
SHA-256 **antes** de qualquer edição:

| arquivo | sha256 | |
|---|---|---|
| `crates/seele-core/src/enlace.rs` | `c36cf800…7bb51c` | bate |
| `crates/seele-server/src/session.rs` | `bbb538b9…1de964` | bate |
| `crates/seele-conformance/tests/tela_por_um_par.rs` | `bc014dd5…f871b5801` | bate |
| `docs/…/2026-09-09-prova-de-rede-da-destruicao/relatorio.md` | `ee40b2f3…40d3c2fc` | bate |

Os três diretórios não rastreados (`2026-09-09-fecho-caminho-entre-pares/`,
`2026-09-09-fecho-tarefas-de-pares/` e `2026-09-09-prova-de-rede-da-destruicao/`)
foram copiados inteiros e conferidos com `diff -r` contra a origem: **idênticos**.
O worktree de origem **não foi modificado**.

Os patches alternativos de importação foram conferidos, e não supostos: aplicados
sobre `d562851` numa cópia à parte,
`00-base-importada-de-b326b0a6.patch` + `01-prova-de-rede-da-destruicao.patch`
reproduzem **exatamente** os três hashes acima.

## 2 · A lacuna, e o que este teste acrescenta

`Motor::cair` é o **único** caminho em que o motor sobrevive ao corte: a sessão
entra na bateria interna e volta noutra conexão. Todos os outros fins de caminho
de par deste arquivo matam o motor junto — `sair()`, a destruição do `Enlace`, a
morte do par —, e **nenhum dos onze testes existentes reconecta**. O guarda
escrito para a substituição (`Motor::largar_o_caminho_entre_pares`) tinha duas
provas de unidade e nenhuma costura.

Um teste novo, e só um, em `crates/seele-conformance/tests/tela_por_um_par.rs`:

**`a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo`**

Servidor de verdade que **cai e volta na mesma porta, com o mesmo banco e o
mesmo certificado** — a receita de `bateria_interna.rs` —, dois `Enlace`
públicos atravessando a bateria vivos, e o caminho por par tendo de existir de
novo do outro lado. O caminho: o par serve quem assiste (30 quadros acima do
piso com o servidor subindo uma cópia só), o servidor cai, os dois `Enlace`
entram em `InternalBattery`, o servidor volta, os dois emitem `Reconectado`, a
transmissão recomeça **por comandos explícitos**, e o par volta a servir.

`+420 / −4` linhas, num arquivo só; nenhum outro arquivo tocado por esta tarefa.
As quatro linhas removidas são substituições de assinatura, não remoção de
prova: `use`, a porta fixa de `servidor_com`, o `seq` inicial de `compartilhar`
e a chamada dele em `cenario`.

## 3 · O achado que decidiu a forma do teste: o `ScreenId` não distingue nada

Um daemon reiniciado **reemite a mesma numeração**. `Registry::issue_screen` é
um `fetch_add` sobre um contador em memória, e o teste mede o resultado:

```
a transmissão nova é a tela 1 (a de antes era a 1), com mídia da geração 1
```

Duas mídias diferentes com o mesmo rótulo é a receita exata do falso-verde que
este repositório paga mais caro: um quadro entregue por uma tarefa que
sobrevivesse à conexão que a criou entraria pelo braço certo, com o nome certo, e
passaria por prova de recuperação. **A marca vai no corpo do quadro**, que é a
única coisa conferida byte a byte — `PASSO_DA_GERACAO`, um milhão por geração,
e `geracao_de` lendo de volta. Um quadro da geração de antes da queda chegando
depois da volta **derruba o teste**.

O mesmo vale para o `SessionId`, e aqui a primeira versão deste teste errou:
`Registry::issue` é o mesmo tipo de contador, e a conexão nova pode receber
**exatamente o id da que caiu** — depende só da ordem em que as três reconexões
chegam. Um `assert_ne!` sobre ele reprovava **uma volta em cada oito**, e
reprovava por motivo errado. Medido, e consertado: o que prova a substituição é
o par de avisos do caminho público — `InternalBattery` seguido de `Reconectado`,
que `Motor::tentar` só emite depois de um `Client` novo ter apertado a mão. Os
ids ficam no rastro, e não na afirmação.

**Reconexão, e não troca de identidade:** o banco em arquivo preserva o
certificado, e o `PersonId` é o mesmo dos dois lados (afirmado). Um certificado
novo seria troca de chave para o TOFU do cliente — o alerta do ADR 0003 — e a
volta teria sido recusada em vez de reconectada.

## 4 · O que **não** volta sozinho, e é assim que tem de ser

`Motor::cair` para a captura e esquece o pedido de tela de propósito. O teste
respeita isso e não introduz retomada automática nenhuma:

- **quem compartilhava reaperta o botão** — conexão nova, `StartScreenShare`
  novo, geração de mídia nova;
- **quem assistia pede a tela de novo** — `assistir(tela, true)`; o pedido
  anterior morreu com a conexão anterior.

O que o contrato manda voltar sozinho volta sozinho, e é **afirmado**: a sala de
voz e a declaração de quem empresta, que `Motor::tentar` redeclara com a escolha
guardada.

## 5 · As reversões — inclusive a que não falhou

Cada guarda retirado, o teste rodado, o código restaurado. A restauração foi
conferida por `git diff | git hash-object --stdin`, que voltou a
`dfe5b4502c6d3b9569a3ef74c6491d705ce25d46` depois de cada uma, e por SHA-256 dos
dois arquivos de produção.

| # | guarda retirado | efeito no teste novo | efeito nos 11 antigos | unidade (`seele-core --lib`) |
|---|---|---|---|---|
| A | `self.largar_o_caminho_entre_pares()` em `Motor::cair` | **passou** (3 voltas) | passaram | **2 FALHAS** |
| B | `declarar_identidade_de_par(self.emprestando)` → `false`, em `Motor::tentar` | **FAILED** | passaram | passaram |
| C | a linha inteira de `declarar_identidade_de_par` em `Motor::tentar` | **FAILED** | passaram | passaram |

As mensagens das duas que falham são distintas e dizem a verdade:

- **B** — «quem empresta reconectou e o servidor novo não o tem como quem
  empresta (declaração: `Some(false)`): a escolha não sobreviveu à substituição
  da conexão»
- **C** — a mesma linha, com «declaração: `None`»

Os onze testes antigos passam nas três reversões, e `bateria_interna` e
`voz_na_reconexao` também passam na B: **este teste é o único guarda que existe
para a escolha de emprestar sobreviver à reconexão.**

### A reversão A é o achado, e ela está aqui porque não falhou

Retirar `largar_o_caminho_entre_pares` **não** derruba este teste — e derruba os
dois testes de unidade escritos para ele
(`cair_devolve_a_vaga_de_quem_estava_servindo_um_par` e
`cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta`: 294 passaram, 2
falharam). Não é defeito do teste: é a medida de que, **neste** cenário, o guarda
é cinto sobre suspensório, e o suspensório é a queda do servidor.

Medido, e não deduzido: com A aplicada, o primeiro quadro pelo par continua
chegando em **172, 173 e 173 ms** — contra 171 ms com o guarda de pé. Se a vaga
de atendimento tivesse ficado presa, o pedido seria recusado em silêncio e a
imagem só viria depois de `PRAZO_DO_PAR`, três segundos, que é o que
`SEM_IMAGEM_TOLERAVEL` reprova. A vaga voltou por outro caminho.

O caminho é este: **a queda do servidor mata os dois lados**. O fluxo do servidor
para quem empresta termina, `ler_a_tela_alheia` chama `RepasseDeTela::fechou`, o
destino é largado, `par::repassar` termina o fluxo do par direito, a tarefa que
serve acaba e a `VagaDeAtendimento` volta pelo `Drop` dela. Do outro lado, a
tarefa que lia do par vê `Ok(None)` e termina sozinha. E o relato velho que ela
enfileira é entregue à conexão nova **antes de existir nomeação nova** — o braço
de `resultados_do_par` roda na primeira volta do laço depois de `tentar()`, e a
tela nova só nasce depois disso —, então ele resolve contra `None` e não muda
estado nenhum.

Onde o guarda A é o único mecanismo, então, é onde a queda é **assimétrica**: a
conexão com o servidor morre e o par continua vivo do outro lado. É exatamente o
que as duas provas de unidade montam à mão, e o §6 diz por que a costura desse
caso ficou de fora.

## 6 · Limitações

- **A queda assimétrica não foi provada ponta a ponta, e é o que resta.** Só a
  queda do servidor foi exercitada, e ela derruba os dois lados: nela as
  proteções de `cair` são redundantes (§5). Para prender o guarda A por
  integração seria preciso cortar **só** a conexão de quem assiste com o par
  vivo — um relé UDP entre cliente e servidor, com a queda custando os 20 s de
  `IDLE_TIMEOUT` do QUIC, ou um ponto de sincronização exclusivo de teste para
  produzir um `ResultadoDoPar` **tardio**. Nenhum dos dois foi feito aqui:
  o primeiro sai da forma de queda que o escopo pede, e o segundo não foi
  necessário para nada que este teste afirma. O guarda continua provado por
  unidade, e agora com a redundância medida em vez de suposta.
- **A prova da fila da conexão velha continua sendo de unidade.** Ela é a rede
  de segurança para a corrida entre o `abort` e o `await` em que a tarefa morre,
  e uma corrida não se reproduz sob encomenda. O que este teste acrescenta a ela
  é o cenário completo em volta: uma reconexão de verdade em que nada da conexão
  anterior chega à substituta.
- **A ausência de quadros da geração antiga é um guarda contra regressão
  futura.** Hoje nenhuma reversão medida a faz disparar, porque a tarefa velha
  morre sozinha no cenário do §5. Ela está aqui porque é a única forma de a
  interferência aparecer se algum dia ela existir — e o `ScreenId` não a
  acusaria.
- **Uma máquina, um servidor local, uma sala, um par.** Nada aqui mede rede de
  verdade, NAT, ou mais de um par emprestando.
- **Estado publicado: desconhecido, por instrução.** Nenhuma consulta de release
  foi feita; nada aqui afirma o que está em máquina nenhuma.
- **Nada foi acessado em `/private/tmp`**, nenhuma sessão externa foi
  controlada, nenhuma permissão foi alterada e **nenhum impedimento por
  permissão apareceu** nesta tarefa. As sessões SEELE `d15590de` e `5a35cf92`
  continuam observadas bloqueadas, com motivo desconhecido, e não foram tocadas.

## 7 · Verificações, com os guardas restaurados

| comando | resultado |
|---|---|
| `cargo test -p seele-conformance --test tela_por_um_par` | **12 passaram, 0 falharam** — 10 voltas seguidas, 4,33 s cada |
| `cargo test -p seele-core --lib` | **296 passaram, 0 falharam** |
| `cargo test -p seele-server --lib` | **413 passaram, 0 falharam** |
| `cargo test -p seele-conformance --test bateria_interna` | **4 passaram, 0 falharam** |
| `cargo xtask check-deps` | passa — «dependency rule holds across 11 workspace crates» |
| `cargo fmt -p seele-conformance -- --check` | limpo |
| `cargo clippy -p seele-conformance --all-targets` | limpo |

Os 11 testes de pares anteriores continuam passando, sem alteração nenhuma nas
afirmações deles. As dez voltas seguidas são deliberadas: a primeira versão deste
teste reprovava uma em oito (§3), e três voltas não teriam bastado para vê-lo.

## 8 · Como aproveitar o que está sem commit

Tudo está no worktree, sem commit. Da mais direta à mais granular:

**1 · Usar este worktree como está.** Ele já tem `d562851` mais a importação mais
o teste novo.

**2 · Levar o conjunto inteiro** (a partir de `d562851`):

```sh
git diff > /onde/quiser/tudo.patch
# do outro lado, com HEAD em d562851:
git apply /onde/quiser/tudo.patch
```

Os quatro diretórios de `docs/superpowers/sdd/2026-09-09-*` e
`2026-09-10-prova-da-substituicao-da-conexao/` são não rastreados e **não entram**
num `git diff`: copiar à parte.

**3 · Separar a importação da prova nova**, com HEAD em `d562851` e a árvore
limpa:

```sh
git apply docs/superpowers/sdd/2026-09-10-prova-da-substituicao-da-conexao/00-base-importada-de-c0d9355c.patch
git apply docs/superpowers/sdd/2026-09-10-prova-da-substituicao-da-conexao/01-prova-da-substituicao-da-conexao.patch
```

O `00` é a entrega de `c0d9355c` exatamente como veio (três arquivos Rust). O
`01` é só esta tarefa: **um arquivo**, `tela_por_um_par.rs`, +420/−4.

Os dois foram conferidos, e não supostos: aplicados sobre `d562851` numa cópia à
parte, o `00` sozinho reproduz os três SHA-256 do §1, e `00` + `01` reproduzem a
árvore atual byte a byte (`cmp` nos três arquivos, sem diferença). Foram gerados
com `diff -u --label a/… --label b/…` justamente para sair com **caminho
relativo**: a armadilha do `--no-index`, que grava caminho absoluto e produz um
patch que só se aplica nesta máquina, está registrada nos relatórios anteriores.

## 9 · Onde continuar

A queda assimétrica do §6 — a conexão com o servidor morrendo enquanto o par
continua vivo do outro lado. É o cenário em que
`Motor::largar_o_caminho_entre_pares` deixa de ser redundante, e o único em que
a interferência de uma tarefa de par sobrevivente teria como aparecer na rede. O
teste desta entrega já traz metade do andaime pronto: as gerações de mídia, que
são o que distingue imagem velha de recuperação quando o `ScreenId` repete.
