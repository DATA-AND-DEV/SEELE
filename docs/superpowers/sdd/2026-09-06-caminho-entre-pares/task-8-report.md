# Task 8 — relatório

## Status

Completa. `cargo test --workspace`, `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets` verdes. TDD seguido: o teste do
brief (`quando_o_par_nao_liga_a_resposta_e_o_servidor`) foi escrito, visto
falhar por erro de compilação (`por_onde` não existia), só então implementado.
O guarda foi provado por reversão no Step 5: com `por_onde` devolvendo
`Par` também no `Err` (via `unreachable!` temporário), o teste falhou com
panic; desfeito antes do commit.

## Commit

`81787f4` — "feat(par): pede a tela ao par, e cai para o servidor quando ele não vem"

## Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` de `cargo test --workspace`
(68 suítes, 0 falhas): **1639 testes**. `seele-core` sozinho: 266 (15 em
`par::testes`, incluindo o novo).

## Preocupações e decisões

**O braço de `state.rs`.** Decisão: `Room::apply` não faz nada com
`AssistaTelaPor`/`SirvaTelaPara` — substituí o `warn!` de andaime por um
comentário explicando o porquê, não por outro `warn!` permanente (a instrução
do brief é explícita contra isso). O `enlace::Motor` já viu a mesma
`ServerMessage` e agiu (`assistir_por_par`/`servir_par`) **antes** dela chegar
ao `Room`, via `Aviso::Mensagem` → `crates/seele-ffi/src/lib.rs::fold`. E o
`Room` é só o que se sabe do servidor para o roster desenhar; de onde os bytes
de uma transmissão vêm (servidor ou par) é detalhe de transporte que
`ComoChegou`/`ParLigado` guardam do lado do `enlace`, e nunca teve — nem
precisou de — um campo equivalente em `Tela`. Não é a mesma omissão do
andaime: aquele calava uma mensagem que ninguém tratava; esta é decidida e
está escrita.

**`por_onde` não devolve o motivo exato ao chamador, e isso limita o
`ParFalhou` enviado.** O brief pede "mande `ClientMessage::ParFalhou` com o
motivo que o tracing registrou" — mas a assinatura fixada de `por_onde`
(`Servidor` sem payload, para não violar `clippy::large_enum_variant` nem o
teste literal do Step 1, que casa `matches!(onde, PorOndeAssistir::Servidor)`
sem padrão interno) não deixa o `ErroDePar` sair para quem chamou. Optei por
mandar sempre `MotivoDeFalhaDePar::NaoAlcancou` no caso `Servidor` — a leitura
mais honesta disponível sem essa informação. O evento de segurança de uma
impressão que não bate continua sendo denunciado localmente (`classificar`,
em `par.rs`, via `tracing::warn!`); só não chega ao servidor distinguido de
um silêncio comum. Como a escolha do servidor (`Pares::escolher`, Task 7) é
deliberadamente burra e não reage diferente por motivo, o custo prático hoje
é zero — mas é uma imprecisão real do contrato, não uma correspondência
perfeita com a frase do brief. Registro para quem revisar decidir se
`PorOndeAssistir::Servidor` devia carregar o motivo (mudando o teste do Step 1
para `matches!(onde, PorOndeAssistir::Servidor(_))`).

**Quem chama `por_onde` disca sem identidade própria.** A assinatura fixada
de `por_onde` não recebe `identidade_propria`, então a discagem de quem
assiste sai com `None` para `ligar`. Se o par que está do lado de
`SirvaTelaPara` alguma vez exigir certificado de cliente de quem chega — e
`passar_a_atender` **sempre** exige (`ConfereQuemChega::client_auth_mandatory`
é `true`) —, essa discagem seria recusada como `SemCertificado`. Não bloqueei
nisso porque: (1) é a assinatura que o pré-voo do plano já vetou como
compatível com a Task 9; (2) `AssistaTelaPor`/`SirvaTelaPara` nunca são
emitidas em produção hoje — `Pares::escolher` não está ligada a nenhum
despacho de `WatchScreen` no servidor (conferi: zero chamadas reais, só nos
testes de `pares.rs`) —, então este caminho só será exercitado de verdade na
Task 10, que é onde o próprio plano diz que a ligação que faltar aparece. Se a
Task 10 achar isso quebrado, a correção é dar a `por_onde` um parâmetro de
identidade — mudança de assinatura que também violaria a interface fixada
desta tarefa, então não a fiz por conta própria.

**Infra nova além do brief, em `enlace.rs` e `client.rs`.** O brief só cita
`par.rs`, `enlace.rs` e `state.rs`; toquei também `client.rs` para dar a
`Client` um método `par_falhou` (mesmo padrão de `watch_screen`/
`unwatch_screen`) — sem ele, `Motor` não tinha como mandar
`ClientMessage::ParFalhou`. Além disso, como `por_onde`/`passar_a_atender`/
`ligar` levam até `PRAZO_DO_PAR` (3 s) e bloquear o laço de `Motor::rodar`
pausaria voz e ping da sessão inteira, as duas tentativas rodam em tarefas
soltas (`tokio::spawn`); o resultado de uma falha de `AssistaTelaPor` volta ao
laço por um canal novo (`ResultadoDoPar`, via `mpsc`) porque só o laço de
`rodar` é dono do `&mut Client`. `Motor` ganhou os campos privados
`ponta_de_pares`, `identidade_de_par` e `atendendo_pares` (nenhum é interface
pública do plano) para não recriar socket, identidade ou capacidade a cada
mensagem.

**A conexão bem-sucedida de `SirvaTelaPara` não é guardada para a Task 9.**
Quando `servir_um_par` liga com sucesso, o método só loga e deixa a conexão
sair de escopo (fechando-a): nada em `par.rs` ou no brief da Task 9 (que só
mexe em `par.rs`, com seu próprio `duas_pontas_ligadas()` de teste) pede que
`enlace.rs` guarde essa `ParLigado` para um `repassar` futuro, e a Task 10 é
quem o próprio plano aponta como dona de costurar o que faltar. Achei mais
honesto não inventar um campo de armazenamento sem saber a forma que a
Task 10 vai precisar dele do que guardar algo hoje que provavelmente seria
redesenhado.

**Testes automatizados cobrem só `par.rs`.** O brief não pede teste dedicado
para a fiação de `enlace.rs`/`state.rs` (o único `cargo test` mandado no Step
4 é `par::`), e não escrevi um por conta própria: a lógica nova ali depende de
duas conexões QUIC reais indo e voltando (como os testes de `dois_pares_*` em
`par.rs`), e montar isso contra o `Motor` inteiro é o que a Task 10
("costura, num servidor de verdade com dois clientes") já existe para fazer.

## Arquivos tocados

- `crates/seele-core/src/par.rs` — `PorOndeAssistir`, `por_onde`, teste.
- `crates/seele-core/src/enlace.rs` — `Motor::assistir_por_par`,
  `Motor::servir_par`, `servir_um_par`, `ResultadoDoPar`, `PRAZO_DO_PAR`,
  campos novos em `Motor`, braço novo no `select!` de `rodar`.
- `crates/seele-core/src/client.rs` — `Client::par_falhou`.
- `crates/seele-core/src/state.rs` — decisão explícita no braço de
  `Room::apply` para `AssistaTelaPor`/`SirvaTelaPara`.

---

## Fix round 1/5

### Status

Completo. `cargo test --workspace`, `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets` verdes.

### Commit

`32d6ace` — "fix(par): identidade não é o mesmo que emprestar, e por_onde diz o motivo"

### Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` de `cargo test --workspace`
(68 suítes, 0 falhas): **1640 testes**.

### O que mudou

- `crates/seele-server/src/pares.rs`: `QuemEmpresta` → `QuemDeclarou`, com
  campo `emprestando: bool`. `Pares::declarou` guarda impressão e endereços
  **sempre**; só `Pares::escolher` filtra por `emprestando`. `Pares::saiu`
  não mudou — continua apagando tudo na saída da sessão.
- `crates/seele-server/src/session.rs`: parou de traduzir
  `emprestando: false` em impressão vazia antes de chamar `declarou`; os
  quatro valores (`emprestando`, `impressao`, `locais`, `publico`) passam
  como vieram.
- `crates/seele-core/src/par.rs`: `por_onde` ganhou o parâmetro
  `identidade_propria: Option<&Identidade>` (repassado a `ligar`) e passou a
  devolver o motivo classificado dentro de `PorOndeAssistir::Servidor(motivo)`
  — função nova `motivo_de_falha` faz a tradução, só `ImpressaoNaoBate` vira
  o motivo de segurança homônimo, todo o resto (`Certificado`, `Escuta`,
  `NaoAlcancou`, `RecusadoDepoisDeLigar`, `ConfirmacaoNaoChegouATempo`) cai em
  `NaoAlcancou`.
- `crates/seele-core/src/client.rs`: `Client::emprestar_subida` novo, mesmo
  padrão de `watch_screen`.
- `crates/seele-core/src/enlace.rs`: `Motor::declarar_identidade_de_par`
  novo — gera (ou reaproveita) a identidade efêmera e manda
  `EmprestarSubida { emprestando: false, .. }` ao servidor. Chamado uma vez
  na conexão inicial (funil `Enlace::conectar_por`, antes de `tokio::spawn`)
  e de novo a cada reconexão bem-sucedida em `Motor::tentar` — a sessão
  anterior já foi apagada de `Pares` na saída, então a identidade não
  sobrevive a uma reconexão sem ser dita de novo. `Motor::assistir_por_par`
  agora busca a própria identidade antes de discar e a passa a `por_onde`; o
  motivo que chega em `PorOndeAssistir::Servidor(motivo)` vai direto para o
  `ResultadoDoPar::ParFalhou`, sem mais achatamento em `NaoAlcancou`.

### Testes novos, com guarda provado por reversão

- `pares::testes::quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada`
  (substitui `quem_nao_declarou_nunca_e_escolhido`, que afirmava a regra
  velha). Reversão: tirar `candidato.emprestando &&` do `find` de `escolher`
  → falha. Restaurado.
- `par::testes::quando_alguem_responde_no_lugar_do_par_o_motivo_nao_vira_naoalcancou`.
  Reversão: `motivo_de_falha` sempre devolvendo `NaoAlcancou` → falha.
  Restaurado.

### Preocupações

**Sobre minha preocupação nº 1 do relatório anterior** — resolvida como
pedido: `por_onde` agora devolve o motivo junto do resultado, sem devolver
`Err`. A classificação (`motivo_de_falha`) só distingue `ImpressaoNaoBate` de
tudo o mais; as outras três variantes de `ErroDePar` que não tinham
correspondência óbvia (`Certificado`, `Escuta`, `ConfirmacaoNaoChegouATempo`)
caem em `NaoAlcancou` por serem rotina de rede sem nada a provar sobre a
declaração publicada — nenhuma delas é o `RecusadoDepoisDeLigar` que também
poderia, em tese, sinalizar identidade rejeitada pelo lado de quem atende;
mapeei esse caso também para `NaoAlcancou` por não ter certeza de que é
impostura (pode ser uma declaração desatualizada por uma corrida de
reconexão) e por `MotivoDeFalhaDePar` não ter uma variante própria para
"meu certificado foi recusado". Fica registrado para quem revisar julgar se
essa distinção merece uma variante nova no protocolo.

**Sobre minha preocupação nº 2** — corrigida na causa raiz, em
`pares.rs`/`session.rs`, e não só na sintoma em `enlace.rs`: agora quem só
assiste declara a própria identidade ao servidor assim que conecta
(`Motor::declarar_identidade_de_par`), e `por_onde`/`assistir_por_par`
apresentam essa identidade ao discar. O caminho ainda não é exercitado em
produção — `Pares::escolher` continua sem nenhum despacho de `WatchScreen`
que a chame — então a prova real só vem com a Task 10.

**Nova, achada ao implementar este fix.** `Enlace::conectar_por` é o funil
que **cada candidato** de uma conexão com múltiplos destinos (`entre`/
`avisar_pelo_candidato`) atravessa, não só o vencedor da corrida — então
`declarar_identidade_de_par` roda uma vez por candidato, e candidatos que
perdem a corrida têm a conexão deles fechada logo depois de declarar.
Inofensivo (o servidor limpa a declaração dele na saída da sessão, como
qualquer sessão que cai), mas é uma mensagem a mais por candidato perdedor —
mesmo padrão de desperdício que o resto da corrida já aceita hoje (a
restauração de sala de voz/canal também seria refeita por candidato, se
`conectar_por` fizesse isso na conexão inicial). Não mexi nisso: não é
regressão desta tarefa, e otimizar a corrida está fora do escopo do fix
pedido.

**`Pares` ainda não tem como devolver a declaração de quem só assiste.**
Para o servidor montar `SirvaTelaPara.enderecos`/`.impressao` (a identidade
de `quem_quer`) para o candidato escolhido, alguém vai precisar ler a
declaração de uma pessoa específica independente de `emprestando` — e
`Pares` hoje só expõe `escolher` (filtrado) e `declarou`/`saiu`. Não
acrescentei um `obter`/`get`: não foi pedido neste fix round, e é a Task 10
quem vai descobrir a forma exata de que precisa ao ligar `Pares::escolher` a
um despacho real de `WatchScreen`.

---

## Fix round 2/5

### Status

Completo. `cargo test --workspace`, `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets` verdes.

### Commit

`51f6a0f` — "fix(par): o servidor não pune a vítima, e a ponta é a que já fala com ele"

### Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` de `cargo test --workspace`
(68 suítes, 0 falhas): **1645 testes**.

### Os cinco consertos

**1 · O servidor pune a vítima.** `session.rs`'s `ParFalhou` chamava
`saiu(session.person)` em `ImpressaoNaoBate` — mas `session.person` é quem
relatou, não quem falhou, e `ParFalhou` não carrega identidade nenhuma do
impostor. `crates/seele-server/src/pares.rs` ganhou `Pares::apontou`/
`Pares::quem_foi_apontado` (a própria nomeação do servidor, chaveada por
`ScreenId`) e `Pares::saiu` agora também esquece nomeações que apontavam para
quem saiu. `session.rs` resolve `ParFalhou{screen}` contra
`quem_foi_apontado(screen)` e só então chama `saiu` no apontado — nunca em
`session.person`. Hoje `quem_foi_apontado` sempre devolve `None` em produção
(nada chama `apontou` ainda — isso é o despacho de `WatchScreen` que a
Task 10 escreve), então o efeito prático imediato é parar de punir a vítima;
o mecanismo fica pronto para a Task 10 popular.

**2 · A ponta era nova, contra a spec.** `Motor::ponta_de_pares` chamava
`crate::client::local_endpoint(None)`, abrindo um socket com porta nova — o
§3.1 diz o contrário: "não é escuta nova, socket novo nem porta nova [...]
aquela porta já tem mapeamento de NAT vivo". `Client` ganhou o campo
`endpoint: quinn::Endpoint` (guardado na construção) e o método público
`Client::endpoint()`; `ponta_de_pares` agora é `fn ponta_de_pares(&self) ->
Option<quinn::Endpoint>` que só clona `self.cliente.as_ref().map(Client::endpoint)`.
O campo `Motor::ponta_de_pares` (o cache antigo) saiu — cachear a ponta
seria reintroduzir o mesmo bug depois de uma reconexão trocar de socket.
`EmprestarSubida.locais`, que ia vazio, agora leva os endereços de rede local
desta máquina: `locais_de_pares`/`e_endereco_de_rede_local`, novos em
`enlace.rs`, usam o crate `if-addrs` (adicionado a `seele-core/Cargo.toml`,
já na árvore via `seele-server`) para uma enumeração simples — sem a
ordenação por heurística de VPN que `seele-server::alcance::interfaces` faz
para o convite, que aqui não faz falta.

**3 · O funil não é inofensivo.** `declarar_identidade_de_par` rodava dentro
de `Enlace::conectar_por`, que **cada candidato** de uma conexão com
múltiplos destinos atravessa (`Enlace::tentar_entre`), não só o vencedor da
corrida. Um candidato perdedor chegava a declarar pela própria conexão, e ao
perder a corrida sua conexão fechava — o que apagava (`Pares` é chaveado por
`PersonId`) a declaração que o vencedor tinha acabado de fazer para a MESMA
pessoa. A chamada saiu de `conectar_por`; `Enlace` ganhou
`declarar_identidade_de_par(&self) -> Result<(), Fechado>`, que manda um
`Comando::DeclararIdentidadeDePar` novo pelo canal de comandos — só chamada
nos três pontos de retorno que já sabem quem vai ser o `Enlace` final
(`conectar`, e as duas saídas de `tentar_entre`: candidato único e vencedor
da corrida).

**4 · O guarda que faltava.** Novo teste
`pares::testes::a_impressao_sobrevive_a_emprestando_false`, usando um leitor
novo (`Pares::declaracao_de`) que a Task 10 vai precisar de qualquer forma
para montar `SirvaTelaPara`/`AssistaTelaPor`. Também acrescentei
`saiu_apaga_tambem_a_declaracao` e dois testes para `apontou`/
`quem_foi_apontado` (ver item 1), todos provados por reversão.

**5 · `RecusadoDepoisDeLigar` ganha variante própria.**
`MotivoDeFalhaDePar::NaoFuiAceito`, acrescentada ao **fim** do enum (postcard
indexa por posição). `crate::par::motivo_de_falha` mapeia
`ErroDePar::RecusadoDepoisDeLigar` para ela em vez de `NaoAlcancou`.
`session.rs` trata `NaoFuiAceito` separado de `ImpressaoNaoBate`: não
desacredita ninguém (o conserto certo — pedir a quem relata que redeclare —
ainda não tem mensagem própria no protocolo, e isso fica registrado no
código, não implementado às pressas).

### Saída dos testes das reversões

**Guarda 1 — `a_impressao_sobrevive_a_emprestando_false`** (reintroduzido
`if !emprestando { self.quem.remove(&pessoa); return; }` em `declarou`):

```
running 8 tests
test pares::testes::um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata ... ok
test pares::testes::saiu_apaga_tambem_a_declaracao ... ok
test pares::testes::quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada ... ok
test pares::testes::quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha ... ok
test pares::testes::quem_compartilha_nunca_e_escolhido_para_servir_a_si_mesmo ... ok
test pares::testes::quem_ja_esta_servindo_nao_e_escolhido_de_novo ... ok
test pares::testes::o_endereco_publico_vem_do_servidor_e_nao_do_cliente ... ok
test pares::testes::a_impressao_sobrevive_a_emprestando_false ... FAILED

failures:
---- pares::testes::a_impressao_sobrevive_a_emprestando_false stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:293:14:
a declaração de quem só assiste desapareceu

test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 353 filtered out
```

Confirma exatamente o achado do revisor: o teste do round 1
(`quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada`) continua
verde sob o bug reintroduzido — só o novo morde.

**Guarda 2 — `um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata`**
(`quem_foi_apontado` forçado a devolver sempre `None`):

```
running 8 tests
...
test pares::testes::um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata ... FAILED

failures:
---- pares::testes::um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:323:9:
assertion `left == right` failed
  left: None
 right: Some(PersonId(5))

test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 353 filtered out
```

**Guarda 3 — `quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha`**
(`Pares::saiu` sem a linha `self.nomeacoes.retain(...)`):

```
running 8 tests
...
test pares::testes::quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha ... FAILED

failures:
---- pares::testes::quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:331:9:
assertion `left == right` failed: quem já foi embora continuou sendo a resposta de uma nomeação
  left: Some(PersonId(5))
 right: None

test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 353 filtered out
```

**Guarda 4 — `quando_o_anfitriao_recusa_minha_identidade_o_motivo_e_naofuiaceito`**
(`motivo_de_falha` sem o braço `RecusadoDepoisDeLigar => NaoFuiAceito`, caindo
no braço genérico `NaoAlcancou`):

```
running 1 test
test par::testes::quando_o_anfitriao_recusa_minha_identidade_o_motivo_e_naofuiaceito ... FAILED

failures:
---- par::testes::quando_o_anfitriao_recusa_minha_identidade_o_motivo_e_naofuiaceito stdout ----
thread '...' panicked at crates/seele-core/src/par.rs:1603:9:
o anfitrião recusou a identidade de quem discou, e o motivo não foi NaoFuiAceito: Servidor(NaoAlcancou)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 267 filtered out
```

Todas as quatro reversões foram desfeitas antes do commit; as suítes `pares::`
(8 testes) e `par::` (17 testes) voltaram a verde depois de cada uma.

### Preocupações

**Item 1 (nomeação) fica sem chamador de `apontou` até a Task 10.** O
mecanismo (`Pares::apontou`/`quem_foi_apontado`) está pronto e testado, mas
nada em produção chama `apontou` hoje — o despacho de `WatchScreen` que
decidiria "quem serve quem" e apontaria via `Pares::escolher` ainda não
existe (confirmado nos rounds anteriores: `Pares::escolher` não tem chamador
real). Isso significa que, na prática, `ImpressaoNaoBate` hoje não desacredita
ninguém (cai no braço `else` que só loga em `debug!`) — o que é estritamente
melhor do que punir a vítima, mas ainda não é o comportamento final. Deixei
isso escrito no código (`tracing::debug!` com a explicação) para quem for
ligar a Task 10 não achar um silêncio misterioso.

**`NaoFuiAceito` não aciona nenhuma ação corretiva no servidor.** Por design
deste round — a "resposta certa" (pedir a quem relata que redeclare) exigiria
uma mensagem nova de servidor para cliente, que não foi pedida e mudaria o
protocolo além do que este fix round autorizou. O braço em `session.rs` está
vazio de propósito, com a razão escrita — não é a mesma omissão do andaime
antigo (aquele calava sem dizer por quê).

**A enumeração de `locais_de_pares` é mais simples que
`seele-server::alcance::interfaces`.** Não tenta detectar VPN/túnel nem
ordenar candidatos — devolve tudo que não é loopback/não-especificado/
link-local, na ordem que `if_addrs` devolver. Isso é suficiente porque
`par::ligar` já tenta todos os endereços em paralelo (não há "primeira
tentativa" a otimizar como há no convite), mas significa que um endereço de
VPN/túnel entra na lista como candidato comum — não incorreto, só sem a
rotulagem "isto veio de uma VPN" que o convite tem. Não portei essa heurística
porque ela existe para a **tela mostrar** a origem a quem hospeda, e não há
tela nenhuma aqui.

**Sem teste de integração para o novo comportamento de `session.rs`.**
`um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata` prova
a peça de `Pares` isoladamente; não escrevi um teste de sessão completa (client
real mandando `ParFalhou`, servidor reagindo) confirmando que `session.rs`
de fato chama `quem_foi_apontado` e não mais `saiu(session.person)` —
montar isso pediria o mesmo tipo de andaime de servidor-com-dois-clientes que
a Task 10 já existe para construir. Revisei o código manualmente linha a
linha em vez disso.

---

## Fix round 3/5

### Status

Completo. `cargo test --workspace`, `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets` verdes.

### Commit

`ea9d2bd` — "fix(par): a conexão importa, a decisão vira função pura, e a ponta desarma"

### Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` de `cargo test --workspace`
(68 suítes, 0 falhas): **1653 testes**.

### Os quatro consertos

**1 · O achado 3 fechou pela metade — chavear por pessoa era o erro.**
`session.rs:403` chamava `saiu(session.person)` em todo encerramento,
chaveado só por `PersonId`. Uma corrida de candidatos pode deixar duas
conexões vivas com o mesmo `PersonId`; o encerramento de uma que perdeu a
corrida, chegando **depois** de a vencedora já ter declarado, apagava a
declaração viva. `crates/seele-server/src/pares.rs`: `QuemDeclarou` ganha
`id_da_conexao: u64` (`quinn::Connection::stable_id() as u64`, o mesmo padrão
que `session.rs` já usa para `Subida::esquecer` — reaproveitado, não
inventado); `Pares::saiu(pessoa, id_da_conexao)` só apaga se a conexão que
está saindo ainda for a que publicou a declaração. `Pares::desacreditar`
(novo) apaga independente da conexão — para o caso em que a identidade
publicada é que está em xeque (`ImpressaoNaoBate`), não a sessão que a fez.
`session.rs` calcula `id_da_conexao` uma vez em `run_session` (mesma leitura
de `connection.stable_id()` que `handle_connection` já fazia) e passa para
`declarou`/`saiu`.

**2 · O achado 1 estava no código e nada o segurava — extraída função
pura.** O revisor reintroduziu `saiu(session.person)` no braço
`ImpressaoNaoBate` e os 361 testes do `seele-server` passaram: o teste do
round 2 só exercitava `apontou`/`quem_foi_apontado` isolados, nunca o braço
de `session.rs`. `pares::quem_desacreditar(motivo, apontado) -> Option<PersonId>`
é a decisão extraída como função pura — testável sem sessão, sem conexão e
sem `Pares`. `session.rs`'s `ParFalhou` agora só lê `quem_foi_apontado`,
chama `quem_desacreditar`, e age (`desacreditar`) ou não age no que ela
decidiu.

**3 · `locais_de_pares` vazava a topologia de quem só assiste.** O round 2
publicava `locais` (endereços de rede local) em toda declaração, inclusive
`emprestando: false` — o §5 da spec nomeia privacidade como a primeira das
duas razões independentes do opt-in, e um servidor não é necessariamente
entre amigos (ADR 0021). `Motor::declarar_identidade_de_par` ganhou o
parâmetro `emprestando: bool`; `locais_de_pares` só é chamada quando
`emprestando` é `true`. Os dois pontos que chamam este método hoje
(conexão inicial e cada reconexão) sempre passam `false` — não há ainda um
comando de "emprestar a subida" no `enlace` — e a função fica pronta
(referenciada, não morta) para quando ele existir.

**4 · A porta de controle ficava armada para sempre.** `passar_a_atender`
nunca era desfeita: sem `set_server_config(None)` depois de `atender` servir
a única vaga, a ponta continuava aceitando conexões pelo resto da sessão,
com o verificador fixado na impressão do último par — e, sem `use_retry`,
funcionando como refletor de amplificação (até 3×) para qualquer pacote
`Initial` que chegasse, de qualquer origem alegada. `par::parar_de_atender`
novo, chamado sempre ao fim de `servir_um_par` (sirva ou não sirva). E o
teste que faltava: a propriedade central do §3.1 — a mesma ponta disca e
atende ao mesmo tempo, sem derrubar a conexão de controle — estava provada
só pela leitura da doc do `quinn`; `a_ponta_atende_um_par_sem_derrubar_a_conexao_de_controle`
agora monta uma conexão de controle de verdade contra um "servidor" QUIC de
teste, chama `passar_a_atender` na mesma ponta, aceita um terceiro par, e só
então confere que a conexão de controle original ainda troca bytes nos dois
sentidos.

### Saída dos testes das reversões

**Guarda 1 — `a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova`**
(`Pares::saiu` revertido para apagar sem checar `id_da_conexao`):

```
running 1 test
test pares::testes::a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova ... FAILED

failures:
---- pares::testes::a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:425:9:
o encerramento de uma conexão que perdeu a corrida apagou a declaração da que venceu

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 365 filtered out
```

**Guarda 2 — `motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem`**
(`quem_desacreditar` revertido para também desacreditar em `NaoFuiAceito`):

```
running 1 test
test pares::testes::motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem ... FAILED

failures:
---- pares::testes::motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:517:13:
assertion `left == right` failed: NaoFuiAceito desacreditou alguém, e só ImpressaoNaoBate deveria
  left: Some(PersonId(9))
 right: None

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 365 filtered out
```

**Guarda 4 — `parar_de_atender_desarma_o_que_passar_a_atender_armou`**
(`parar_de_atender` revertido para um no-op):

```
running 1 test
test par::testes::parar_de_atender_desarma_o_que_passar_a_atender_armou ... FAILED

failures:
---- par::testes::parar_de_atender_desarma_o_que_passar_a_atender_armou stdout ----
thread '...' panicked at crates/seele-core/src/par.rs:1669:9:
a ponta continuou atendendo depois de parar_de_atender: Ok(ParLigado { conexao: Connection(...), como: Local, ida_e_volta: 1.068109ms })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 270 filtered out
```

(A ligação que devia ter sido recusada — ou melhor, nunca nem tentada de
verdade — fechou com sucesso: a prova de que a ponta continuava armada.)

Todas as três reversões foram desfeitas antes do commit; `pares::` (13
testes) e `par::` (19 testes) voltaram a verde depois de cada uma.

### Sobre o teste do item 4 que não tem reversão

`a_ponta_atende_um_par_sem_derrubar_a_conexao_de_controle` não vem com uma
saída de reversão porque não há um bug **nosso** para reverter nele: ele
prova uma propriedade do `quinn` (dial e accept convivendo na mesma ponta
sem se atrapalharem), não uma decisão que este código toma. Não existe uma
versão "errada" de `passar_a_atender`/`ligar` cuja reversão faria só esta
propriedade cair — o teste existe para que a suíte pare de depender da
documentação do `quinn` estar certa, e não para prender uma regressão
específica deste código.

### Preocupações

**A checagem de `id_da_conexao` em `saiu` interage com `apontou` de um jeito
que um teste meu do round 2 escondia.** `saiu` só apaga a nomeação
(`nomeacoes`) quando também apaga a declaração — ou seja, quando a conexão
bate. Isso é o comportamento certo no caso real (uma nomeação para uma
identidade que continua viva sob uma sessão mais nova não devia sumir), mas
quebrou um teste meu do round anterior que fazia `apontou` sem `declarou`
antes — um cenário sintético que não acontece em produção (`apontou` só
seria chamado, pela Task 10, sobre alguém que `escolher` já escolheu, e
`escolher` só devolve quem declarou). Corrigi o teste para declarar antes de
apontar, e não mudei a implementação — mas registro a interação porque não
foi óbvia na hora, e quem tocar `saiu`/`apontou` de novo devia saber que ela
existe.

**`locais_de_pares` continua sem chamador real.** Com `emprestando` sempre
`false` nos dois pontos que chamam `declarar_identidade_de_par`, a função
nunca roda em produção hoje — só nos testes. Isso é intencional (ver item 3
acima), mas significa que a filtragem de endereços em si
(`e_endereco_de_rede_local`) só está coberta por teste unitário direto, não
por um caminho de produção que a exercite de ponta a ponta.

**Diferidos, como pedido:** não mexi em `locais_de_pares` não filtrar oper
status; no ordinal de `MotivoDeFalhaDePar` sem guarda em
`o_vocabulario_e_a_versao`; nem na ponta velha ficando armada por um
instante durante a reconexão.

---

## Fix round 3/5

### Status

Completo. `cargo test --workspace` (68 suítes, 0 falhas), `cargo fmt --all --
--check` e `cargo clippy --workspace --all-targets` verdes.

### Commit

`ea9d2bd` — "fix(par): a conexão importa, a decisão vira função pura, e a
ponta desarma"

`86aea6d` — "style(par): rustfmt no guarda do opt-in de `locais`". Só espaço
em branco: o commit acima congelou `locais_a_publicar` com o `if`/`else`
numa linha só, forma que o rustfmt desta toolchain não produz, e
`cargo fmt --all -- --check` ficava vermelho no `ea9d2bd`. Verificação final
(`cargo test --workspace` 1654/68 verdes, `fmt --check` saída 0, `clippy
--workspace --all-targets` saída 0, zero avisos) roda sobre `86aea6d`.

**Nota de processo (importante, e não é detalhe):** este round teve **duas
pessoas na mesma árvore ao mesmo tempo**, de novo. O implementador anterior
— dado como travado — acordou no meio do meu trabalho e commitou `ea9d2bd`
às 00:38:36, varrendo junto o que eu já tinha acrescentado (o guarda do item
3, ver abaixo). Conferi linha a linha que o conteúdo commitado é o estado
**bom**: `Pares::saiu` com a checagem de `id_da_conexao`,
`quem_desacreditar` com `ImpressaoNaoBate => apontado`, `parar_de_atender`
chamando `set_server_config(None)` e `locais_a_publicar` com o `if
emprestando`. Nenhuma reversão de teste sobreviveu ao commit — `git status`
limpo contra a árvore restaurada, e as 68 suítes verdes depois dele. O risco
que isso corre é real: se a captura tivesse caído dois segundos antes, o
commit teria congelado uma reversão. Não repetir.

### Total de testes do workspace

Somando todas as linhas `test result: ok. N passed` (68 suítes, 0 falhas):
**1654 testes**. Eram 1645 no round 2; os 9 novos são os deste round.

### Os quatro consertos, e o teste que faltava

**1 · A declaração carrega qual sessão a fez.** `QuemDeclarou` ganha
`id_da_conexao: u64` (`quinn::Connection::stable_id`, o mesmo identificador
que `session.rs` já usa para `Subida::esquecer`); `Pares::declarou` o
recebe; `Pares::saiu(pessoa, id_da_conexao)` só apaga se ainda for a mesma
conexão. `Pares::desacreditar(pessoa)` é a porta separada para o caso de
segurança — ali o motivo é a **declaração** ter sido provada falsa, e a
conexão que a fez pode continuar viva. As duas compartilham `esquecer`, que
apaga a declaração e as nomeações que apontavam para ela. Guarda:
`a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova`.

**2 · `quem_desacreditar` é função pura em `pares.rs`.** Assinatura
`(MotivoDeFalhaDePar, Option<PersonId>) -> Option<PersonId>`. Só
`ImpressaoNaoBate` desacredita, e só quem o servidor apontou —
`session.person`, que é quem **relata**, não entra na função nem como
argumento, que é a forma mais forte de o defeito do round 2 não voltar por
onde voltou. O braço de `session.rs` virou executor: resolve `screen` contra
`quem_foi_apontado` e chama o que a função decidir. Três guardas:
`impressaonaobate_desacredita_quem_foi_apontado_nunca_quem_relata`,
`sem_nomeacao_guardada_ninguem_e_desacreditado`,
`motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem`.

**3 · `locais` só sai de quem optou por emprestar.**
`Motor::declarar_identidade_de_par` ganha o parâmetro `emprestando`, e a
regra foi **isolada em `locais_a_publicar(emprestando, todos)`** — este
pedaço foi o que faltava quando peguei a árvore: o `if emprestando` estava
embutido na chamada e nenhum teste o prendia. `todos` é `FnOnce` de
propósito: quem não empresta não chega nem a enumerar as interfaces da
máquina. Guarda:
`so_quem_empresta_publica_os_enderecos_da_propria_maquina`, com endereços
inventados para a decisão não depender da LAN de quem roda o teste.

**4 · A ponta desarma quando a vaga acaba.** `par::parar_de_atender`
(`set_server_config(None)`), chamada em `servir_um_par` **sempre** que
`atender` devolve, sirva ele ou não. Guarda:
`parar_de_atender_desarma_o_que_passar_a_atender_armou` — as duas
identidades do teste combinam de propósito, para que a única razão de a
discagem falhar seja a ponta ter desarmado.

**5 · O §3.1 provado em código, não na doc do `quinn`.**
`a_ponta_atende_um_par_sem_derrubar_a_conexao_de_controle`: um servidor QUIC
de teste, uma conexão de controle viva contra ele saindo de `ponta`,
`passar_a_atender` **naquela mesma ponta**, um terceiro par discando e sendo
atendido — e só então a prova que importa, um ida-e-volta de bytes pela
conexão de controle. O teste antigo
(`uma_ponta_de_cliente_passa_a_atender_sem_socket_novo`) fica: ele confere a
porta, que é outra coisa, e o contraste entre os dois está na reversão 5.

### Saída das reversões

**Reversão 1 — `Pares::saiu` volta a apagar por pessoa** (a checagem de
`id_da_conexao` trocada por `self.esquecer(pessoa)` incondicional):

```
test pares::testes::a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova ... FAILED

---- pares::testes::a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:426:9:
o encerramento de uma conexão que perdeu a corrida apagou a declaração da que venceu

test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 353 filtered out
```

**Reversão 2 — `quem_desacreditar` com o mapeamento trocado**
(`ImpressaoNaoBate => None`, os outros três `=> apontado`):

```
test pares::testes::motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem ... FAILED
test pares::testes::impressaonaobate_desacredita_quem_foi_apontado_nunca_quem_relata ... FAILED

---- motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:518:13:
assertion `left == right` failed: NaoAlcancou desacreditou alguém, e só ImpressaoNaoBate deveria
  left: Some(PersonId(9))
 right: None

---- impressaonaobate_desacredita_quem_foi_apontado_nunca_quem_relata stdout ----
thread '...' panicked at crates/seele-server/src/pares.rs:496:9:
assertion `left == right` failed
  left: None
 right: Some(PersonId(9))

test result: FAILED. 11 passed; 2 failed; 0 ignored; 0 measured; 353 filtered out
```

**Reversão 3 — `locais_a_publicar` publica sempre** (o `if emprestando`
trocado por `todos()` incondicional):

```
test enlace::tests::so_quem_empresta_publica_os_enderecos_da_propria_maquina ... FAILED

---- enlace::tests::so_quem_empresta_publica_os_enderecos_da_propria_maquina stdout ----
thread '...' panicked at crates/seele-core/src/enlace.rs:3769:9:
quem só assiste publicou a topologia de rede interna da máquina

test result: FAILED. 23 passed; 1 failed; 0 ignored; 0 measured; 248 filtered out
```

**Reversão 4 — `parar_de_atender` vira no-op** (o `set_server_config(None)`
trocado por `let _ = ponta;`):

```
test par::testes::parar_de_atender_desarma_o_que_passar_a_atender_armou ... FAILED

---- par::testes::parar_de_atender_desarma_o_que_passar_a_atender_armou stdout ----
thread '...' panicked at crates/seele-core/src/par.rs:1671:9:
a ponta continuou atendendo depois de parar_de_atender: Ok(ParLigado { conexao:
Connection(...), como: Local, ida_e_volta: 1.594726ms })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 271 filtered out
```

(O `Ok(ParLigado { … })` inteiro do `quinn` foi cortado; a parte que importa
é o `Ok` — a discagem completou o aperto de mão contra uma ponta que devia
estar desarmada.)

**Reversão 5 — atender e falar com o servidor deixam de caber na mesma
ponta.** Esta precisou de mira: a primeira tentativa (`ponta.close()` no
começo de `passar_a_atender`) derrubava também o **servidor de teste**, que
usa a mesma função, e o teste morria na primeira `ligar` — vermelho pelo
motivo errado. A reversão que isola a propriedade fecha a ponta só na
**segunda** chamada do processo, que no cenário do teste é a da `ponta` que
já carrega a conexão de controle, com o teste rodado sozinho:

```
---- par::testes::a_ponta_atende_um_par_sem_derrubar_a_conexao_de_controle stdout ----
thread 'tokio-rt-worker' panicked at crates/seele-core/src/par.rs:1741:18:
o servidor de teste não recebeu o fluxo de prova: ApplicationClosed(ApplicationClose
{ error_code: 0, reason: b"reversao" })

thread '...' panicked at crates/seele-core/src/par.rs:1776:10:
o terceiro par não conseguiu ligar para a ponta que também atende: NaoAlcancou

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 271 filtered out
```

E o contraste que é o motivo de este teste existir — **o teste antigo, sob a
mesma reversão, continua verde**:

```
running 1 test
test par::testes::uma_ponta_de_cliente_passa_a_atender_sem_socket_novo ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 271 filtered out
```

Todas as cinco reversões foram desfeitas; as suítes voltaram a verde depois
de cada uma, e o `cargo test --workspace` final (1654 testes) rodou sobre a
árvore restaurada.

### Preocupações

**O braço de `session.rs` continua sem guarda que morda.**
`quem_desacreditar` é pura e testada, e `session.person` não é sequer
argumento dela — mas nada impede alguém de escrever
`pares.desacreditar(session.person)` naquele braço e ver os 361 testes do
`seele-server` passarem. A extração reduz a superfície do erro (a decisão
não pode mais ser reimplementada errada por engano), não a fecha. Fechar de
verdade pede o harness de servidor-com-dois-clientes que a Task 10 constrói.

**`emprestando` é sempre `false` hoje.** Os dois pontos que chamam
`declarar_identidade_de_par` — a conexão inicial e cada reconexão — passam
`false`, porque não existe ainda no `enlace` um comando de "empreste a
subida". `locais_a_publicar(true, …)` é exercitado por teste e por mais
nada. Isso é andaime esperando consumidor, como o `Pares::apontou` do round
2: aceitável enquanto é andaime, defeito se sobreviver ao consumidor.

**`parar_de_atender` desarma a ponta inteira, e a ponta é uma só.** Se um
dia `servir_um_par` puder rodar duas vezes concorrentemente na mesma
`Motor`, o `set_server_config(None)` do primeiro a terminar desarma a
escuta do segundo no meio do aperto de mão. Hoje não acontece — `servir_um_par`
é chamada de um ponto só e serve uma vaga só —, mas é uma invariante que
mora na forma do chamador, não no tipo, e vai sobreviver mal a quem
paralelizar isso sem ler o doc.

**A reversão 5 mediu o que devia depois de ajustada, não de primeira.** Fica
registrado porque o primeiro vermelho parecia prova e não era: falhava na
conexão de controle **antes** de `passar_a_atender` sequer entrar em cena.
Um vermelho no lugar errado é tão pouco informativo quanto um verde.
