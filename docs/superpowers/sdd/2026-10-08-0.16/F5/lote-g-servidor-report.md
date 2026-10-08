# Lote G-servidor da F5: relatório

Base `3b0b6a7a` (branch `0.16/lotes`). Branch `0.16/lotes-g-servidor`, no
worktree `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af3886e3e332f2e9a`.
Três commits de código e este relatório, todos locais: sem push, tag nem
release.

| commit | item | o que muda |
|---|---|---|
| `2807cffd` | passo 0 | os testes que sobem hospedagem desligam o degrau 4; o guarda `recusar_a_producao` reprova quem ia resolver o ponto de encontro de produção |
| `586c23de` | passo 2, e as provas do cliente do passo 3 | `a_sessao_acabou_aqui` inclui `ServerShuttingDown`; `toda_despedida_do_protocolo_escolhe_um_lado` e o par terminal em `bateria_interna.rs` |
| `271360dd` | passo 1, e `quem_hospeda_fecha.rs` do passo 3 | `Event::ServidorEncerrando`, `Daemon::encerrar_avisando`, o SAIR avisando e `Hospedagem::despedir` |

Nada no fio (`seele-proto` intocado: o `ServerShuttingDown` já existia), nada
no SEELE-ENC/1, nos arquivos da F6, em `api/`, na CSP, em `ui/` ou em
`frases.js`. Nenhuma dependência nova, nada baixado. A casca (`main.rs`,
`tela-sessao.js`, `tela-fim.js`, `encerramento.rs`) não foi tocada.
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

## Os comandos e o que deram (último estado, `271360dd`)

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

Hoje há 19 chamadas de `Hospedagem::iniciar`: as 17 do brief e as 2 do
`quem_hospeda_fecha.rs`. As 18 de teste apontam `nao` ou um ponto local, e a de
produção (`main.rs`) não mudou.

## Preocupações

1. **Decisão pedida: o guarda não alcança o `seele-server/tests/` rodado
   sozinho.** Com `cargo test -p seele-server --test janela_do_comandante`, a
   biblioteca sai com `features=[]`, e medi isso pelo JSON. Junto com o
   `seele-conformance` ela sai com a feature: é o caso do `cargo test
   --workspace` da CI e da bateria da integração, e foi assim que fiz a R0.3. A
   única forma de o Cargo ligar a feature para os testes do próprio crate é ele
   se pôr nas próprias `[dev-dependencies]`, e eu medi:
   `cargo xtask check-deps` recusa essa aresta («`seele-server` declares a
   dev-dependency on `seele-server`»). Eu a desfiz. A `janela_do_comandante.rs`
   já não fala com a VPS (`sem_degrau_4`). O que falta é o guarda pegar um teste
   futuro de `tests/` que esqueça o ambiente e rode sozinho. As opções:
   - (a) o `check_deps` passa a ignorar a aresta de um crate para si mesmo (uma
     linha e um teste no `xtask`, fora da lista deste lote), e o `seele-server`
     ganha a autodependência;
   - (b) aceitar a lacuna, que está escrita no `Cargo.toml` e no doc do guarda;
   - (c) levar os testes de hospedagem de `seele-server/tests/` para o
     `seele-conformance`.

   Não escolhi.
2. **A janela do aviso ainda aceita conexão nova.** A R2 mostrou isso: com o
   cliente revertido, a bateria reconectou dentro dos 500 ms do
   `encerrar_avisando` e voltou `Reconectado`. Um cliente 0.15, para quem
   `ServerShuttingDown` é recuperável, faz o mesmo, e a sessão nova é cortada
   sem aviso pelo `shutdown`. Uma saída é recusar conexões depois da difusão
   (`set_server_config(None)`, ou recusar no `run`). Não fiz: o brief não pede,
   e eu não medi como o quinn trata o `Initial` sem configuração.
3. **A trava serializa os testes de hospedagem da biblioteca.** Nesta rede
   custa +0,56 s. Numa rede sem UPnP, cada `iniciar` paga até 3 s de
   `PROCURA`, e os oito em fila podem passar de 20 s. Isso é estimativa: não
   medi numa rede sem UPnP.
4. **Fechar o app hospedando continua sem aviso** até o Plano 2 chamar o
   `despedir`, porque o `Drop` faz o `shutdown()` cru. É a parte do P50-4 que
   vai com o Plano 2, para o Z registrar.
5. **O SAIR passa a esperar até 500 ms** quando há alguém no meio do aperto de
   mão, ou um par que não confirma a entrega. Com sessões normais leva
   milissegundos: no teste, a sessão fechou em menos de 1 ms.
6. **Três guardas de ambiente em `seele-conformance`.** O
   `tests/ambiente/mod.rs` novo repete a forma do `AmbienteDoEncontro` de
   `quarto_pelo_link.rs`, que eu não toquei (8384). Quem mexer nele pode juntar
   os dois.
7. **O guarda é do lado de quem hospeda.** O cliente (`seele-core`) que pergunta
   ao quarto pelo ponto de um bilhete não tem guarda. Num teste, esse ponto vem
   de quem hospedou, que agora nunca é o de produção.
8. **A linha de medida do `quem_hospeda_fecha`** sai por `eprintln!` em toda
   rodada (o `RUST_TEST_NOCAPTURE=1` do `.cargo/config.toml` a mostra). Ela foi
   deixada de propósito, como registro do tempo do fechamento.

Nada com `cfg(windows)` nem `cfg(target_os = "linux")` foi escrito ou mexido.
