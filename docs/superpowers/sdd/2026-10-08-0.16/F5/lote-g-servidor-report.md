# Lote G-servidor da F5: relatório

Base `3b0b6a7a` (branch `0.16/lotes`). Branch `0.16/lotes-g-servidor`, no
worktree `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af3886e3e332f2e9a`.
Seis commits de código e o relatório (escrito em `75f850fb` e completado no
último commit), todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `2807cffd` | passo 0 | os testes que sobem hospedagem desligam o degrau 4; o guarda `recusar_a_producao` reprova quem ia resolver o ponto de encontro de produção |
| `586c23de` | passo 2, e as provas do cliente do passo 3 | `a_sessao_acabou_aqui` inclui `ServerShuttingDown`; `toda_despedida_do_protocolo_escolhe_um_lado` e o par terminal em `bateria_interna.rs` |
| `271360dd` | passo 1, e `quem_hospeda_fecha.rs` do passo 3 | `Event::ServidorEncerrando`, `Daemon::encerrar_avisando`, o SAIR avisando e `Hospedagem::despedir` |
| `46ae3d56` | rodada 1 de conserto | a linha e os docs do prazo da despedida, o `compile_error!` da feature em build otimizado, o lugar na frase do guarda e a prova do guarda de `iniciar`, `despedir` medido contra o teto |
| `8a4bd619` | a decisão (d) | sem `$SEELE_ENCONTRO`, nenhum ponto; a produção pede o padrão por `iniciar_com_o_ponto_padrao` |
| `f5a78c52` | fecho | o guarda do app olha o `main.rs` inteiro; a frase da `janela_do_comandante` e o doc do `PONTO_PADRAO` acertados |

Nada no fio (`seele-proto` intocado: o `ServerShuttingDown` já existia), nada
no SEELE-ENC/1, nos arquivos da F6, em `api/`, na CSP, em `ui/` ou em
`frases.js`. Nenhuma dependência nova, nada baixado. A casca (`tela-sessao.js`,
`tela-fim.js`, `encerramento.rs`) não foi tocada. O `main.rs` do app só mudou
pela decisão (d): a chamada do `hospedar` e o guarda dela.
`docs/pendencias.md` e o índice da 1.0 também não. Nenhuma promessa nova de API
de MOD: não há seção «Para a v6».

As saídas inteiras dos vermelhos, verdes e reversões ficaram no scratchpad da
sessão. Aqui vai o trecho que diz o motivo. As reversões foram feitas por um
script que troca o trecho exato, roda os testes, devolve o arquivo e confere o
SHA-256 contra o de antes. Todas voltaram «idêntico».

## Passo 0: os testes não falam com o ponto de encontro de produção (`2807cffd`)

### O que mudou

**O guarda** (`alcance/encontro.rs`, `recusar_a_producao`): um `assert!` que
reprova quando o texto do ponto é o de produção. Ele reconhece o
`PONTO_PADRAO` escrito de qualquer jeito (com porta, em maiúsculas, com o ponto
final do DNS) e os IPs de `REDE_DO_PADRAO` em qualquer porta, inclusive o IPv4
dentro de IPv6. Pela mesma regra de `separar_ponto` que `resolver` usa: um
texto que ela recusa não vira consulta, e não há o que barrar. Tem
`#[track_caller]`, e é a linha do pânico que diz qual dos dois lugares pegou o
teste.

Ele é chamado em dois lugares:
- **em `resolver`, antes do `lookup_host`.** Pega um teste que chama `abrir`
  direto, ou que aponta `$SEELE_ENCONTRO` para um IP da VPS;
- **em `Hospedagem::iniciar`, sobre a convocação que vai para a escada.** Este
  segundo lugar não estava no pedido, e entrou por medida. O degrau 4 só é
  tentado sem IPv4 global (`tem_ipv4_global`). Com o guarda só em `resolver`, um
  teste sem ambiente passaria calado numa VPS e reprovaria numa casa atrás de
  CGNAT. Aqui ele reprova em qualquer máquina.

Fora dos testes as duas funções não fazem nada (versão `cfg(not(...))` vazia).

**Como o guarda se liga:**
- nos testes da biblioteca, pelo `cfg(test)`;
- nos do `seele-conformance`, pela feature `recusar-encontro-de-producao` do
  `seele-server`, que só a `[dev-dependencies]` do `seele-conformance` pede.
  É o mesmo desenho da `fixtures` do `seele-lancador`;
- o `seele-server/tests/` só o tem quando roda junto com o `seele-conformance`.
  É a primeira preocupação, abaixo.

**Os testes passaram a apontar `nao`.** Escolhi o ambiente, e não a costura que
passaria `convocacao = None`. Os motivos:
- o caminho dos testes continua sendo o de produção, `Convocacao::do_ambiente`
  inclusive;
- a reversão é exatamente «tirar o ambiente», como a orquestradora a escreveu;
- a trava fecha uma corrida que já existia e que eu medi (ver «A corrida»,
  abaixo).

O custo foi medido: os oito testes de hospedagem levam 1,08 s em fila contra
0,52 s em paralelo nesta máquina.

O que cada arquivo ganhou:
- `lib.rs`: `AmbienteDoEncontro::sem_degrau_4()` (o `guardar` com `pedir("nao")`).
  O parágrafo «O que a trava não cobre» foi reescrito como «Quem sobe hospedagem
  também a toma», com a medida.
- `hospedagem.rs`: os 8 testes que subiam sem ambiente (9 chamadas) tomam
  `let _ambiente = …::sem_degrau_4();`. `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada`
  já trocava a variável e não mudou.
- `ejetar.rs`: `let _ambiente = ambiente::sem_degrau_4();` logo depois da vaga,
  no `hospedar_ejetar_e_hospedar_de_novo_libera_a_porta`.
- `seele-conformance/tests/ambiente/mod.rs` (novo, fora da lista do brief): o
  guarda `SemDegrau4`, que desliga e devolve pelo `Drop`. É usado pelo
  `ejetar.rs` e pelo `quem_hospeda_fecha.rs`. Quem serializa é a vaga. O
  `quarto_pelo_link.rs` continua com a cópia dele, que eu não toquei.
- `janela_do_comandante.rs`: `sem_degrau_4()` com `Once`, que põe `nao` para o
  binário inteiro e não devolve. Os dois testes correm em paralelo e querem o
  mesmo valor, e um guarda que devolvesse a variável a devolveria no meio do
  `iniciar` do outro. Está escrito no doc.

### A corrida que a primeira rodada mostrou

Com o guarda já no lugar, a primeira rodada de
`cargo test -p seele-server --lib -- hospedagem:: …` passou os oito testes de
hospedagem sem nenhum pânico. Eles passaram porque leram o `nao` ou o buraco
negro que `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada` tinha posto na
variável (o doc da trava já previa isso). Rodados sem aquele teste
(`--skip um_ponto_de_encontro_fora_do_ar`), os oito reprovaram no guarda.

Antes deste lote, portanto, o ponto que cada teste de hospedagem usava
dependia de quem subia primeiro. Agora todos tomam a trava.

### RED

`cargo test -p seele-server --lib -- hospedagem:: --skip um_ponto_de_encontro_fora_do_ar`:

```
thread 'hospedagem::tests::um_server_hospedado_aceita_conexao' panicked at crates/seele-server/src/hospedagem.rs:149:13:
um teste ia falar com o ponto de encontro de produção («encontro.seele.app.br»): cada rodada registraria um servidor de teste no quarto da VPS de produção e deixaria lá o IP desta máquina, e o teste passaria a depender da rede de fora. Aponte um ponto local, ou desligue o degrau 4 com `SEELE_ENCONTRO=nao`, pelo `AmbienteDoEncontro` do teste
test result: FAILED. 0 passed; 8 failed
```

`cargo test -p seele-conformance --test ejetar`:
`hospedar_ejetar_e_hospedar_de_novo_libera_a_porta` reprovou em
`hospedagem.rs:149:13`, com a mesma frase.

### GREEN

- `cargo test -p seele-server --lib`: 578 passados;
- `--test janela_do_comandante`: 2;
- `-p seele-conformance --test ejetar`: 3.

### Reversões

| # | o que foi desfeito | resultado |
|---|---|---|
| R0.1 | o `_ambiente` de `um_server_hospedado_aceita_conexao` | vermelho em `hospedagem.rs:149:13`, com a frase do guarda |
| R0.2 | o `_ambiente` do `ejetar.rs` | `hospedar_ejetar_…` vermelho em `hospedagem.rs:149:13` |
| R0.3 | o `sem_degrau_4()` da `janela_do_comandante.rs`, rodada com `-p seele-server -p seele-conformance` | os dois testes vermelhos em `hospedagem.rs:149:13` |
| R0.4 | o guarda do `iniciar`, junto com o `_ambiente` de um teste | vermelho em `encontro.rs:580:5`: o guarda de `resolver` pega antes do DNS, porque esta máquina não tem IPv4 global e tenta o degrau 4 |
| R0.5 | o guarda de `resolver` | `resolver_o_ip_da_vps_de_producao_reprova_antes_do_primeiro_pacote`: «test did not panic as expected» |

Antes da R0.3, conferi pelo `--message-format=json` que a biblioteca daquela
rodada leva a feature, e que portanto não havia como o teste revertido sair
para a rede. Rodado sozinho, ele sairia: é a primeira preocupação.

Os testes novos do guarda:
- `o_guarda_reconhece_o_ponto_de_producao_escrito_de_qualquer_jeito`: dez formas
  de escrever produção, e oito pontos locais que têm de passar;
- `resolver_o_ip_da_vps_de_producao_reprova_antes_do_primeiro_pacote`: usa um
  IP, e não o nome, para que nem com o guarda desfeito haja consulta de DNS. A
  R0.5 rodou sem rede.

**Sem rede para a VPS.** Nenhum teste meu consultou o nome nem mandou pacote
aos IPs de `REDE_DO_PADRAO`. Rodei o RED só com o guarda já no lugar, e ele
dispara antes do `Escada::subir` e antes do `lookup_host`. Não deixei o DNS do
ponto padrão inalcançável: isso pediria mudar ajuste do sistema. Quem prova é o
guarda: uma rodada verde quer dizer que nenhum teste chegou a resolver o ponto.

### O build de release (feature de teste)

| comando | o que diz |
|---|---|
| `cargo tree --workspace -e features,no-dev -i seele-server` | só `seele-server feature "default"`, pelo `seele-app` |
| `cargo tree -p seele-app -e features -i seele-server` (com as arestas de teste) | só `"default"`: o `seele-app` não pede a feature nem nos testes |
| `cargo check --release -p seele-app --message-format=json` | `seele_server ['lib'] opt=3 features=[]` |
| `cargo check --release --bin seeled --message-format=json` (como o `release.yml`) | `seele_server ['lib'] opt=3 features=[]` e `seeled ['bin'] opt=3 features=[]` |
| `cargo tree -p seele-conformance -e features -i seele-server` | `"recusar-encontro-de-producao"`, pela `[dev-dependencies]` |

O `cargo tauri build` é um `cargo build --release` do `seele-app`, que não
ativa as dependências de teste. Os dois binários de produto saem sem o guarda.

## Passo 2: o cliente (`586c23de`)

- `a_sessao_acabou_aqui` (`seele-core/src/enlace.rs`) inclui `ServerShuttingDown`.
  O doc diz por quê, e diz que a recusa do aperto de mão com a mesma razão (o
  banco ilegível) nunca chega ali, porque vira `ConnectError::Refused`.
  `ScheduledMaintenance` continua do lado recuperável, e o doc diz que é de
  propósito.
- O doc de `Motivo::Moderado` passa a valer para quem hospeda fechou.
- `toda_despedida_do_protocolo_escolhe_um_lado`: `ServerShuttingDown` passa para
  o lado `true`, num braço próprio com comentário.
- `bateria_interna.rs`: `quem_hospeda_fechou_acaba_com_a_sessao_em_vez_de_reconectar`.
  É o par de `uma_despedida_recuperavel_…`: a mesma barreira e a mesma injeção
  (`SessionEnded` no barramento), com `ServerShuttingDown`. O teste afirma três
  coisas: o `Disconnecting` chegou pelo protocolo, a sessão acabou com
  `Motivo::Moderado(ServerShuttingDown)`, e ela não reconectou.

**RED:**

```
enlace::tests::toda_despedida_do_protocolo_escolhe_um_lado ... FAILED
assertion `left == right` failed: ServerShuttingDown mudou de lado sem que este teste mudasse junto
  left: false
 right: true

quem_hospeda_fechou_acaba_com_a_sessao_em_vez_de_reconectar ... FAILED
quem hospeda fechou o servidor e a sessão reconectou: a bateria vai bater numa porta que ninguém atende, dizendo «reconectando» a quem precisa ler que o servidor fechou
```

**GREEN:** `cargo test -p seele-core enlace` deu 63 passados; `--test bateria_interna`, 7.

## Passo 1: o servidor (`271360dd`)

- **`Event::ServidorEncerrando { reason }`** (`server.rs`) é interno e fora do
  fio, e leva a razão para o Plano 2 mandar `ChamadaEncerrada` pela mesma via.
- **A sessão** (`session.rs`) o atende no braço da expulsão, num padrão só:
  `Event::SessionEnded { reason, .. } | Event::ServidorEncerrando { reason } if manda_esta_sessao_embora(&event, session.person)`.
  O corpo é o mesmo: `desassentar`, `Disconnecting { reason }`, `finish` e
  `despedir`. Só mudam a linha do log e o motivo do `close` (`b"shutting down"`).
  `translate`, o único `match` exaustivo, ganhou o braço `None`. O
  `momento_de` dos MODs e o `canal_do_evento` já caíam no `_`.
- **`Daemon::encerrar_avisando(reason, prazo)`** (`lib.rs`) difunde o evento,
  espera com **`timeout(prazo, endpoint.wait_idle())`** e então chama
  `shutdown()`. Escolhi o `wait_idle` com prazo, e não o `open_connections()`:
  - ele volta pelo aviso do próprio endpoint, sem laço de sondagem;
  - quando volta, cada sessão já se despediu, e o fechamento dela já saiu;
  - quando o prazo vence com conexão aberta, isso fica dito no log, com o
    `open_connections()` e o prazo.

  `shutdown()` continua síncrono, para o `Drop`. O doc dele foi reescrito, e
  não fala mais em M3 nem em `ManutencaoProgramada`.
- **`Hospedagem::encerrar`** (o SAIR) chama
  `encerrar_avisando(ServerShuttingDown, PRAZO_DO_AVISO)` depois de
  `escada.descer()`, no lugar do `shutdown()` cru. `PRAZO_DO_AVISO` é privado e
  vale 500 ms. O resto não mudou.
- **`Hospedagem::despedir(teto)`** faz três passos, cada um com a sua fatia do
  teto:
  1. `encerrar_avisando(ServerShuttingDown, teto/2)`;
  2. `timeout(teto/4, wait_idle())`;
  3. `timeout(teto/4, escada.descer())`.

  Ninguém a chama ainda: quem chama é o Plano 2. O doc do `Drop` diz que ele
  fecha sem avisar e aponta as duas saídas que avisam.

**`quem_hospeda_fecha.rs`** (novo, com `vaga::minha()` e `ambiente::sem_degrau_4()`):
- `encerrar_avisa_cada_sessao_que_o_servidor_fechou` sobe
  `Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))` com
  **duas** sessões `Enlace`, cada uma de pé do lado do servidor (ida e volta
  pela Linha). Depois do `encerrar()`, cada uma lê
  `Disconnecting { ServerShuttingDown }` e acaba com
  `Motivo::Moderado(ServerShuttingDown)`.
- `a_despedida_avisa_mesmo_com_o_runtime_caindo_em_seguida` põe a hospedagem
  numa `std::thread` com runtime `current_thread`. A thread espera um oneshot,
  roda `despedir(800 ms)` e derruba o runtime na linha seguinte.
  - Um **par cru** (o `Enlace` não expõe o `closed()`) faz o aperto de mão
    inteiro e prova o laço da sessão de pé com `Ping`/`Pong`. Ele tem de ler o
    aviso e ver o `closed()` (`ApplicationClosed`) em menos de 1 s.
  - Uma segunda conexão, **só QUIC, sem `Hello`** (alguém no meio do aperto de
    mão), também tem de ver o `closed()` em menos de 1 s. É ela que prova o
    `wait_idle` do `despedir`: ver a reversão R1.2.
  - O prazo de espera é o `IDLE_TIMEOUT` mais 5 s, para a reprovação dizer
    quanto levou.

**RED**, com o `despedir` em rascunho (só o `drop`, que é o que a saída do app
faz hoje) e o `encerrar` ainda com o `shutdown()` cru:

```
a sessão de asuka não leu `Disconnecting { ServerShuttingDown }` quando quem hospeda saiu: o servidor caiu sem avisar, e a tela dela diz «não foi possível alcançar o servidor» em vez de «o servidor está encerrando»
o cliente não leu que o servidor estava encerrando antes de o runtime de quem hospeda cair … left: None, right: Some(ServerShuttingDown)
test result: FAILED. 0 passed; 2 failed; … finished in 26.03s
```

**GREEN**, com a linha de medida que o teste escreve:

```
quem_hospeda_fecha: aviso Some(ServerShuttingDown); a sessão fechou em 677.959µs (Ok(ApplicationClosed(… b"shutting down" …))); quem entrava fechou em 403.012875ms (Ok(ApplicationClosed(… b"shutting down" …)))
test result: ok. 2 passed
```

## Passo 3: reversões do G-servidor

| # | o que foi desfeito | `encerrar_avisa_…` | `a_despedida_avisa_…` |
|---|---|---|---|
| R1.1 | a difusão em `encerrar_avisando` | vermelho: asuka não leu o aviso | vermelho: `aviso None`; a sessão fecha em 402 ms, pelo `shutdown` |
| R1.5 | o braço da sessão (`ServidorEncerrando => true` tirado de `manda_esta_sessao_embora`) | vermelho: asuka não leu o aviso | vermelho: `aviso None`, fechada em 403 ms |
| R1.3 | a espera de `encerrar_avisando` | vermelho | vermelho: `aviso None`; o `shutdown` passa à frente do `Disconnecting`, e a conexão fecha em 0,6 ms sem motivo |
| R1.2 | o `wait_idle` do `despedir` | verde (não passa por ele) | vermelho: a sessão fecha com aviso em 0,96 ms, mas **quem entrava fecha em 20,004 s (`TimedOut`)** |
| R1.4 | as duas esperas | vermelho | vermelho: as duas conexões fecham em 20,0 s (`TimedOut`) |
| R2 | `ServerShuttingDown` tirado de `a_sessao_acabou_aqui` | vermelho: `Some(Reconectado { … })` | — |

Na R2 também ficaram vermelhos `toda_despedida_do_protocolo_escolhe_um_lado`
(«mudou de lado») e `quem_hospeda_fechou_acaba_com_a_sessao_em_vez_de_reconectar`.
São «os dois» que o brief pede, e o `quem_hospeda_fecha` junto.

**A medida que desmentiu o brief.** O brief diz: «tirar o aviso ou o wait_idle
faz o cliente cair só pelos 20 s». Medi isso com um cliente só, e ele é uma
sessão.
- Tirar o `wait_idle` do `despedir` (R1.2, primeira rodada) deixou o teste
  **verde**: aviso lido, conexão fechada em 370 µs. A espera dentro de
  `encerrar_avisando` já esvazia o endpoint, e com ele o fechamento da sessão.
- O `wait_idle` do `despedir` serve a quem o prazo do aviso deixa para trás: uma
  conexão que não é sessão, e que só o `shutdown` fecha. Foi por isso que o
  teste ganhou a segunda conexão, e com ela a R1.2 dá os 20 s que o brief
  previa.
- Tirar o aviso (R1.1) não dá 20 s: dá «sem aviso» e um fechamento em ~400 ms,
  pelo `shutdown` com o `wait_idle` que sobrou.

Os 20 s só aparecem quando nenhuma espera sobra para quem precisa dela (R1.2
com a segunda conexão, e R1.4).

## Os comandos e o que deram (estado em `271360dd`; o final está mais abaixo)

- `cargo test -p seele-server --lib`: 578 passados;
- `… -p seele-server --test janela_do_comandante`: 2;
- `… -p seele-conformance --test quem_hospeda_fecha --test bateria_interna --test ejetar`: 2, 7 e 3;
- `… -p seele-core enlace`: 63;
- a mais, porque o braço da expulsão mudou: `… -p seele-conformance --test moderacao` deu 6;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-server -p seele-conformance -p seele-core -p seele-app --all-targets --all-features -- -D warnings`: limpo (só o aviso ambiental do `ld: duplicate -rpath`);
- `cargo clippy -p seele-server -- -D warnings` (sem a feature, a versão vazia do guarda): limpo;
- `cargo xtask check-deps`: «dependency rule holds across 14 workspace crates»;
- `cargo doc -p seele-server --no-deps`: os 23 avisos que ele dá (links de doc
  pública para itens privados) apontam para linhas que este lote não tocou.

Não rodei o `quarto_pelo_link` (porta 8384): não mexi nele. Ele é alcançado pelo
lote de dois jeitos:
- o `encerrar` agora avisa, e espera até 500 ms se houver conexão aberta;
- o guarda está ligado nele, mas o ambiente dele aponta `127.0.0.1`, que não é
  produção.

Não rodei `--workspace`.

## Arquivos mudados (`3b0b6a7a..271360dd`)

- `crates/seele-server/Cargo.toml` (a feature, com o doc);
- `crates/seele-server/src/alcance/encontro.rs` (o guarda e os testes dele);
- `crates/seele-server/src/hospedagem.rs` (o guarda no `iniciar`, `encerrar`,
  `despedir`, `PRAZO_DO_AVISO`, o doc do `Drop`, e os testes com
  `sem_degrau_4`);
- `crates/seele-server/src/lib.rs` (`encerrar_avisando`, o doc do `shutdown`,
  `AmbienteDoEncontro::sem_degrau_4` e o doc da trava);
- `crates/seele-server/src/server.rs` (`Event::ServidorEncerrando`);
- `crates/seele-server/src/session.rs` (o braço, `manda_esta_sessao_embora` e
  `translate`);
- `crates/seele-server/tests/janela_do_comandante.rs` (`sem_degrau_4`);
- `crates/seele-core/src/enlace.rs`;
- `crates/seele-conformance/Cargo.toml` (a feature na dependência de teste);
- `crates/seele-conformance/tests/ambiente/mod.rs` (novo);
- `crates/seele-conformance/tests/ejetar.rs`;
- `crates/seele-conformance/tests/bateria_interna.rs`;
- `crates/seele-conformance/tests/quem_hospeda_fecha.rs` (novo).

Em `271360dd` havia 19 chamadas de `Hospedagem::iniciar`: as 17 do brief e
as 2 do `quem_hospeda_fecha.rs`. As 18 de teste apontavam `nao` ou um ponto
local. A contagem final está em «Os comandos e o que deram (estado final)».

## Rodada 1 de conserto (`46ae3d56`)

A revisão pediu cinco consertos menores. Cada um, com a prova:

1. **A linha do prazo da despedida dizia mais do que se sabe.** O `wait_idle`
   só volta quando o endpoint não tem conexão nenhuma, e uma conexão fechada
   ainda conta pelo dreno do QUIC, três PTO. Numa internet com RTT de umas
   poucas dezenas de milissegundos, isso passa dos 500 ms, e o log dizia
   «elas fecham sem aviso» sobre sessões que já tinham lido o aviso.
   - A linha agora diz «conexões ainda no endpoint: fechando depois do aviso,
     ou sem ter se despedido (estas fecham agora, sem aviso)», com o
     `open_connections()` e o prazo.
   - O doc de `encerrar_avisando` separa os dois casos em que o prazo vence, com
     a conta do PTO.
   - O doc de `PRAZO_DO_AVISO` deixou de prometer «dezenas de milissegundos»,
     e o do `despedir` diz que o `wait_idle` dele pode voltar pelo prazo com
     conexões ainda drenando.
   - Não há teste novo: é texto de log e de doc.
2. **Nada impedia de vez a feature de vazar para o release.** Escolhi o
   `compile_error!` sob `all(feature = "recusar-encontro-de-producao",
   not(debug_assertions))`, em `alcance/encontro.rs`, e não um guarda no
   `xtask`: ele vale para todo build, sem fiação nova na CI. O preço é que os
   testes do `seele-conformance` não rodam em `--release`, e hoje nenhum roda.
   Não há `[profile]` no workspace que mude `debug-assertions`.

   | comando | resultado |
   |---|---|
   | `cargo check --release -p seele-server --features recusar-encontro-de-producao` | **não compila**, com a frase do `compile_error!` |
   | `cargo check -p seele-server --features recusar-encontro-de-producao` (debug) | compila |
   | `cargo check --release -p seele-app --message-format=json` | compila; `seele_server opt=3 features=[]` |
   | `cargo check --release --bin seeled --message-format=json` | compila; `seele_server` e `seeled` com `opt=3 features=[]` |
   | `cargo test -p seele-conformance --test ejetar --no-run --message-format=json` | `seele_server opt=0 features=['recusar-encontro-de-producao']` |

   O teste de `iniciar` (item 3) e os do `quem_hospeda_fecha` passaram com o
   guarda ligado.
3. **O guarda de `iniciar` não tinha prova permanente.** Agora o lugar vai na
   frase do pânico: `recusar_a_producao(ponto, onde)` diz «pego em
   Hospedagem::iniciar, antes da escada» ou «pego em alcance::encontro::resolver,
   antes do DNS». O teste novo,
   `o_guarda_de_iniciar_pega_o_ponto_de_producao_antes_da_escada`, é um
   `#[should_panic]` que casa com o primeiro. Ele roda sob a trava, com
   `$SEELE_ENCONTRO` num IP da VPS (`REDE_DO_PADRAO`, que passou a
   `pub(crate)`). O teste do `resolver` passou a casar com o lugar dele.
   - **Reversão C3**, tirar o guarda de `iniciar`: «panic did not contain
     expected string». O pânico veio de `encontro.rs:580`, «pego em
     alcance::encontro::resolver, antes do DNS». Nenhum pacote saiu: o guarda
     do `resolver` pega antes de qualquer envio.
4. **Ninguém afirmava que `despedir` volta dentro do teto.** A linha de quem
   hospeda devolve o `Instant` em que acabou, depois de o runtime cair. O teste
   afirma que isso fica abaixo de `TETO_DA_DESPEDIDA` (800 ms) mais
   `FOLGA_DA_DESPEDIDA` (200 ms). Medido: 486 a 490 ms.
   - **Reversão C4**, o aviso do `despedir` com `teto * 2` no lugar de
     `teto / 2`: «`despedir(800ms)` levou 1.690094834s até o runtime cair».
5. **A R1.2 depende da máquina.** O doc do teste
   `a_despedida_avisa_mesmo_com_o_runtime_caindo_em_seguida` agora registra
   isso. Tirar o `wait_idle` do `despedir` reprova aqui. A reprovação depende de
   nada deixar o runtime rodar entre o `shutdown` e a queda. Numa rede em que o
   degrau 3 abriu porta, o `escada.descer()` faz E/S de rede, o driver do
   endpoint roda enquanto ele espera, e o fechamento pode sair sem o
   `wait_idle`.

## A decisão (d) (`8a4bd619`)

A orquestradora escolheu a (d) no lugar da (a): o padrão do produto passa de
«produção por omissão» a «nenhum ponto por omissão». O `check_deps` não foi
tocado.

### A forma

- **`Convocacao::do_ambiente`** (`alcance/encontro.rs`) só lê
  `$SEELE_ENCONTRO`. Sem a variável, `None`. Com a variável, nada mudou: um
  endereço troca o ponto, e `nao` ou vazio desligam.
- **`Convocacao::com_o_ponto_padrao`** é a mesma leitura, com o `PONTO_PADRAO`
  quando a variável não existe. Só a produção chama. As duas são uma linha
  sobre `do_ambiente_ou(socket, impressao, sem_a_variavel)`, que é privada.
- **`Hospedagem::iniciar`** passa a ser o padrão seguro, por
  `Convocacao::do_ambiente`.
- **`Hospedagem::iniciar_com_o_ponto_padrao`** tem os mesmos cinco argumentos
  e usa `Convocacao::com_o_ponto_padrao`. As duas delegam a um `subir`
  privado, que recebe a função que convoca (`fn(Arc<UdpSocket>, &str) ->
  Option<Convocacao>`).
- **Por que duas funções, e não um argumento a mais:**
  - o padrão seguro fica com o nome curto, e o ponto de produção só é pedido
    por quem escreve o nome longo;
  - as 19 chamadas de teste que já existiam não mudaram.
- O `PONTO_PADRAO` não saiu do lugar, e nada da F2 foi antecipado.

### Os pontos de entrada achados

Procurei, fora de teste, por `Daemon::bind`, `Hospedagem::iniciar*`,
`Escada::subir`, `encontro::abrir(` e `Convocacao::*`:

| onde | o que faz | depois da (d) |
|---|---|---|
| `apps/seele-app/src/main.rs`, `hospedar` (:1935) | a única chamada de produção de `Hospedagem` | `Hospedagem::iniciar_com_o_ponto_padrao` |
| `crates/seele-server/src/main.rs`, o `main` do `seeled` (:71) | `Daemon::bind` e `run`; **não sobe a escada** nem convoca ponto (é o degrau 1) | nada muda: ele não tinha degrau 4 antes, e não perde nada |
| `crates/seele-server/src/hospedagem.rs`, `subir` | o único `Escada::subir` e o único chamador das convocações | recebe a função que convoca |
| `crates/seele-server/src/alcance.rs:1023` | o único `encontro::abrir`, dentro de `Escada::subir` | nada muda |
| `seele-ffi`, a sala pessoal da F7 | não hospeda: o ADR 0002 não deixa o `seele-ffi` ver o `seele-server`; só um comentário cita `Convocacao::para_servidor` | nada muda |

### Os testes

Os testes de hospedagem **continuam com o `nao` explícito**:
- a trava protege os testes de hospedagem da biblioteca de outros testes do
  mesmo binário que **trocam** a variável: o buraco negro, e agora o IP da VPS
  no teste do guarda de `iniciar`. Sem a trava, um teste de hospedagem leria o
  valor do outro;
- no `seele-conformance` e na `janela_do_comandante`, o `nao` é a intenção
  escrita.

O parágrafo «o que ela não alcança» foi reescrito em três lugares: no
`Cargo.toml`, no doc do guarda e no doc da `janela_do_comandante`. A lacuna de
`seele-server/tests/` rodado sozinho perdeu o perigo: esquecer o ambiente dá
«sem ponto». O guarda virou a segunda parede.

### RED (antes da mudança, com os testes novos já escritos)

```
alcance::encontro::testes::sem_a_variavel_o_ambiente_nao_convoca_ponto_nenhum ... FAILED
  left: Some("encontro.seele.app.br")
 right: None
hospedagem::tests::iniciar_sem_ambiente_nao_convoca_ponto_nenhum ... FAILED
um teste ia falar com o ponto de encontro de produção («encontro.seele.app.br», pego em Hospedagem::iniciar, antes da escada): …
o_ponto_de_encontro_de_quem_hospeda::hospedar_sobe_com_o_ponto_de_encontro_padrao ... FAILED
o `hospedar` não sobe o servidor com `Hospedagem::iniciar_com_o_ponto_padrao(`: sem ele, e sem `$SEELE_ENCONTRO`, a hospedagem não convoca ponto de encontro nenhum, e quem hospeda atrás de CGNAT perde o degrau 4 em silêncio
```

O guarda do `seeled`, `o_seeled_nao_sobe_a_escada_sem_o_ponto_padrao`, nasceu
verde, porque o `seeled` não chama nem `Hospedagem::iniciar(` nem
`Convocacao::do_ambiente(`. A prova dele é a reversão D4.

### Os guardas e as reversões

| # | guarda | o que foi desfeito | resultado |
|---|---|---|---|
| D1 | `sem_a_variavel_o_ambiente_nao_convoca_ponto_nenhum` e `iniciar_sem_ambiente_nao_convoca_ponto_nenhum` | `do_ambiente` volta a cair no `PONTO_PADRAO` | os dois vermelhos: `left: Some("encontro.seele.app.br")`, e o pânico do guarda em `iniciar` |
| D2 | `iniciar_com_o_ponto_padrao_leva_o_ponto_do_projeto_a_escada` (`#[should_panic]`: sem ambiente, o ponto de produção chega ao guarda de `iniciar`; prova pelo comportamento, sem rede) | `iniciar_com_o_ponto_padrao` passa `do_ambiente` | «test did not panic as expected» |
| D5 | `desligar_o_degrau_4_e_uma_variavel_de_ambiente` (agora também afirma que `com_o_ponto_padrao` segue a variável e cai no padrão sem ela) e o D2 | `com_o_ponto_padrao` sem o padrão | «sem `SEELE_ENCONTRO`, quem hospeda pelo app ficou sem o ponto de encontro do projeto», `left: None`; e o D2 sem pânico |
| D3 | `o_ponto_de_encontro_de_quem_hospeda::hospedar_sobe_com_o_ponto_de_encontro_padrao` (fonte: o corpo do `hospedar` por `corpo_de` sobre `fonte_sem_comentarios`; agulhas partidas com `concat!`) | o `hospedar` volta a `Hospedagem::iniciar(` | vermelho, com a frase do guarda |
| D4 | `o_seeled_nao_sobe_a_escada_sem_o_ponto_padrao` (fonte: o corpo do `main` do `seeled`, sem comentários; agulhas partidas) | uma convocação por `Convocacao::do_ambiente(` posta no `main` | «o `main` do `seeled` chama `Convocacao::do_ambiente(` …» |

Todos os arquivos voltaram idênticos (SHA-256).

## Os comandos e o que deram (estado final, `8a4bd619`)

- `cargo test -p seele-server --lib`: 582 passados;
- `… -p seele-server --bin seeled`: 6;
- `… -p seele-server --test janela_do_comandante`: 2;
- `… -p seele-conformance --test quem_hospeda_fecha --test bateria_interna --test ejetar`: 2, 7 e 3;
- `… -p seele-core enlace`: 63;
- `… -p seele-app` (porque o `main.rs` mudou): todos os binários verdes. O
  `--bin seele-app` deu 260; `frontend` 263, `empacotamento` 22,
  `manifesto_hibrido` 11, `marca` 9, `fontes` 5, `permissoes` 4, `tokens` 3 e
  `encerramento` 2;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-server -p seele-conformance -p seele-app --all-targets --all-features -- -D warnings`: limpo (só o aviso ambiental do `ld`);
- `cargo clippy -p seele-server -- -D warnings`: limpo;
- `cargo xtask check-deps`: «dependency rule holds across 14 workspace crates»;
- o release medido de novo: `cargo check --release -p seele-app` e
  `--bin seeled` compilam com `features=[]`, e o `cargo tree --workspace -e
  features,no-dev -i seele-server` mostra só `"default"`.

Não rodei o `quarto_pelo_link` (porta 8384) nem o `--workspace`.

Hoje há 22 chamadas de `Hospedagem::iniciar*`:
- 21 são de teste, todas com `nao`, um ponto local, ou um ponto de produção de
  propósito sob o guarda;
- 1 é de produção: a `iniciar_com_o_ponto_padrao` do `hospedar` do app.

## Arquivos mudados (`3b0b6a7a..8a4bd619`)

Os de `271360dd`, listados acima, e mais:
- `apps/seele-app/src/main.rs` (a chamada do `hospedar` e o módulo de teste
  `o_ponto_de_encontro_de_quem_hospeda`);
- `crates/seele-server/src/main.rs` (o guarda do `seeled`).

## Fecho (`f5a78c52`)

A re-revisão resolveu os achados da rodada 1 e os pontos da (d). Ela também
conferiu por grep, no workspace e nos branches em andamento, que nenhum caminho
de produção perde o ponto padrão. Antes do merge, pediu três coisas:

1. **O guarda do app passa a olhar o `main.rs` inteiro.** Ele ganhou um teste,
   `nenhuma_hospedagem_deste_arquivo_sobe_sem_o_ponto_padrao`, que exige duas
   coisas, as duas sobre o arquivo sem as linhas de comentário:
   - **nenhuma** linha com `Hospedagem::iniciar(`;
   - pelo menos uma com `Hospedagem::iniciar_com_o_ponto_padrao(`.

   As agulhas são constantes do módulo, partidas com `concat!`, e servem aos
   dois testes. O guarda do corpo do `hospedar` fica ao lado. O motivo é de
   futuro: a sala pessoal (tarefa 4f da F7) e o G-casca do Plano 2 vão abrir
   pontos de entrada novos nesse arquivo.
   - **Reversão F1:** uma função nova,
     `async fn hospedar_a_sala_pessoal() { … Hospedagem::iniciar(…) … }`,
     posta longe do `hospedar`, junto do `FONTE_DESTE_ARQUIVO`. O guarda do
     arquivo reprovou: «este arquivo sobe hospedagem com
     `Hospedagem::iniciar(` … Linhas: ["let _ =
     seele_server::hospedagem::Hospedagem::iniciar("]». O guarda do corpo do
     `hospedar` continuou verde, e é por isso que o novo existe. O arquivo
     voltou idêntico.
2. **A frase velha da `janela_do_comandante.rs`.** O doc do `sem_degrau_4`
   dizia que um guarda devolvido no meio leria «o ponto padrão». Agora diz o
   que vale: leria o valor de antes da troca, e sem a variável isso é «nenhum
   ponto». O risco que sobra é um `$SEELE_ENCONTRO` vindo do terminal de quem
   roda a bateria voltar a valer no meio do teste.
3. **O doc do `PONTO_PADRAO`** não diz mais «quando ninguém disser outro». Diz
   «para quem hospeda pelo app e não disse outro», e conta que só o caminho de
   produção o pede (`Convocacao::com_o_ponto_padrao`, por
   `iniciar_com_o_ponto_padrao`), e que os testes sobem pelo `iniciar` curto,
   sem ponto.

**Nota da re-revisão.** O D2
(`iniciar_com_o_ponto_padrao_leva_o_ponto_do_projeto_a_escada`) e o C3
(`o_guarda_de_iniciar_pega_o_ponto_de_producao_antes_da_escada`) passam o
endereço de produção ao `iniciar` de verdade, e dependem do próprio guarda para
não sair à rede. Isso só falha se `aponta_para_a_producao` regredir. Nesse caso
o teste dela, `o_guarda_reconhece_o_ponto_de_producao_escrito_de_qualquer_jeito`,
fica vermelho na mesma rodada.

**Os comandos:**
- `cargo test -p seele-app --bin seele-app`: 261 passados;
- `-p seele-server --lib`: 582;
- `-p seele-server --test janela_do_comandante`: 2;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-server -p seele-app --all-targets --all-features -- -D warnings`
  e `cargo clippy -p seele-server -- -D warnings`: limpos (só o aviso ambiental
  do `ld`).

## Preocupações

A primeira versão deste relatório (`75f850fb`) tinha oito. A decisão (d)
resolveu a primeira, o alcance do guarda em `seele-server/tests/`. As que
ficam, na ordem da revisão:

1. **Para depois (nota da revisão): a janela do aviso aceita conexão nova.** A
   R2 mostrou: com o cliente revertido, a bateria reconectou dentro dos 500 ms
   do `encerrar_avisando`. Um cliente 0.15, para quem `ServerShuttingDown` é
   recuperável, faz o mesmo, e a sessão nova é cortada sem aviso. O conserto é
   um sinal de `encerrando` no aperto de mão, e é do Plano 3.
2. **Para depois: o custo da trava sem UPnP.** Nesta rede a trava custa
   +0,56 s. Numa rede sem UPnP, cada `iniciar` paga até 3 s de `PROCURA`, e os
   oito em fila podem passar de 20 s. Isso é estimativa: não medi.
3. **Para depois: o guarda olha só o texto do ponto.** Um nome que resolva para
   a VPS por `/etc/hosts` ou por um apelido no DNS passa por ele. Uma camada
   depois do DNS foi escrita e retirada, porque o lugar onde ela age não tinha
   reversão possível sem mudar ajuste do sistema. Com a (d), só chega a esse
   caso quem aponta um nome de propósito.
4. **Para depois: há três guardas de ambiente em `seele-conformance`.** O
   `tests/ambiente/mod.rs` repete a forma do `AmbienteDoEncontro` de
   `quarto_pelo_link.rs`, que eu não toquei (8384).
5. **Fechar o app hospedando continua sem aviso** até o Plano 2 chamar o
   `despedir`: o `Drop` faz o `shutdown()` cru. É a parte do P50-4 que vai com
   o Plano 2, para o Z registrar.
6. **O SAIR passa a esperar até 500 ms** quando há alguém no meio do aperto de
   mão, quando um par não confirma a entrega, ou numa rede longe, pelo dreno.
7. **O `main.rs` do app mudou neste branch**, na chamada do `hospedar` e num
   módulo de teste logo abaixo dele. A base deste branch é `3b0b6a7a`, anterior
   à integração do lote E, e a integração confere o encaixe.
8. **O guarda é do lado de quem hospeda.** O cliente (`seele-core`) que
   pergunta ao quarto pelo ponto de um bilhete não tem guarda. Num teste, esse
   ponto vem de quem hospedou, e agora nunca é o de produção sem que alguém o
   peça.
9. **A linha de medida do `quem_hospeda_fecha`** sai por `eprintln!` em toda
   rodada, e foi deixada de propósito.

Nada com `cfg(windows)` nem `cfg(target_os = "linux")` foi escrito ou mexido.
