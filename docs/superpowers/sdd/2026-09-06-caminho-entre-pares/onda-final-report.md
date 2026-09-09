# Onda final da revisão — relatório

Doze itens, na ordem do brief, um commit cada. Nenhum décimo terceiro conserto:
o que apareceu está na seção **Achado e não consertado**, no fim.

Base: `725be01`. Doze commits acima dela.

---

## 1 · I5 — o contador de cópias para de contar em silêncio

**Commit** `005b18d` · `crates/seele-conformance/tests/tela_por_um_par.rs`

`while let Ok(evento) = eventos.recv().await` saía do laço no primeiro
`RecvError::Lagged`, e `agora()` congelava. Agora `Lagged` continua lendo e só
`Closed` encerra.

`ContadorDeCopias::de` ganhou uma irmã, `seguindo(receiver, screen)`: o teste
precisa de um recebedor **já atrasado**, e um daemon de verdade não sabe
produzir um sob encomenda.

**TDD.** O teste
`o_contador_de_copias_sobrevive_a_um_atraso_do_barramento` foi escrito antes do
conserto e falhou com:

```
Error: a paciência acabou esperando: o contador enxergar o número que veio
depois do atraso do barramento
```

(trinta segundos de `PACIENCIA`, porque o laço tinha morrido no `Lagged` e o
número nunca chegava).

**Prova por reversão.** `match` devolvido para `while let Ok(..)`, suíte inteira
do arquivo: **3 passaram, 1 falhou** — só o teste novo, com a mesma mensagem.
Restaurado: 4 de 4.

---

## 2 · C2 — vazamento de conteúdo entre salas de voz

**Commit** `fcadb2e` · `crates/seele-server/src/pares.rs`,
`crates/seele-server/src/session.rs`

`Pares::escolher` passa a receber `na_sala: &HashSet<PersonId>` e a exigir que o
candidato esteja lá dentro. `apontar_um_par` recebe o `voice_room` (que o braço
de `WatchScreen` já tinha na mão) e monta esse conjunto.

### Como descobri a sala de cada pessoa, e por que essa fonte não fica velha

A fonte é `Server.occupancy` — `crate::server::Occupancy::in_voice_room`. Ela é
o registro que o daemon já usa para responder «quem está em qual sala»: `seat`
a reescreve a cada entrada (e tira a pessoa de onde estava, porque `seat` chama
`vacate_everywhere` antes de sentar), `vacate` a cada saída de sala,
`vacate_everywhere` a cada fim de sessão. **Não há um segundo lugar guardando a
mesma coisa**, e é por isso que ela não fica velha: não há cópia para
discordar.

Guardar a sala dentro de `QuemDeclarou` era a alternativa óbvia e é a errada.
Uma pessoa declara identidade **uma vez por sessão** (`EmprestarSubida`), e
continua a mesma pessoa ao trocar de sala — o campo seria escrito na declaração
e nunca mais. A primeira troca de sala já o deixaria mentindo, e mentindo
exatamente na direção do defeito: apontando alguém que já não está lá.

A leitura é feita **antes** de tomar `server.pares`, e não dentro: dois mutexes
tomados sempre na mesma ordem são uma ordem; tomados em ordens diferentes são
um travamento esperando o primeiro dia ruim.

**TDD.** Com o filtro neutralizado, os dois testes novos falharam:

```
quem_empresta_de_outra_sala_de_voz_nunca_e_escolhido
  alguém de outra sala de voz foi apontado para servir esta tela — o repasse
  dele carrega a tela da sala dele

entre_dois_que_emprestam_so_o_da_sala_da_tela_e_escolhido
  assertion `left == right` failed: a escolha saiu da sala da transmissão
    left: PersonId(3)   right: PersonId(4)
```

O segundo deles **passou de primeira** na primeira redação, e não devia: com um
candidato de cada lado, `escolher` devolvia o certo por acaso. A ordem de
iteração de um `HashMap` é fixa enquanto o mapa vive, então um laço de cinquenta
rodadas sobre o mesmo mapa repetia cinquenta vezes a mesma ordem. Passou a
construir um `Pares` novo a cada rodada — aí a semente muda, a ordem varia, e o
guarda morde.

**Prova por reversão.** Filtro trocado por um `true`: **13 passaram, 2
falharam** — só os dois novos. Restaurado: 15 de 15 (`pares::`), 368 de 368 no
`seele-server`.

---

## 3 · C3 — fim limpo do repasse não avisa ninguém

**Commit** `eb96701` · `crates/seele-core/src/enlace.rs`,
`crates/seele-conformance/tests/tela_por_um_par.rs`

O `Ok(None)` de `escoar_tela_alheia` passa a mandar sempre
`ParFalhou { motivo: ParouDeMandar }` quando o fluxo veio de um par, antes de
decidir qualquer outra coisa.

Doc de `PEDACOS_A_ESPERA_DO_PAR` reescrito: ele afirmava «Quem assiste nota a
falta de imagem e cai para o servidor pelo caminho de sempre», e não caía.
Agora a seção **«Por qual mecanismo quem assiste volta ao servidor»** nomeia o
mecanismo: destino desligado → `Sender` cai → `par::repassar` termina o fluxo
**direito** → o `Ok(None)` reporta → o servidor religa o cano.

Comentário de `quando_o_par_morre_...` (`:735-739`) corrigido: ele chamava a
despedida limpa de «o caso fácil», e ela era o caso sem conserto.

### O que encontrei em `session.rs:2418-2436` quando a tela não existe mais

O braço manda `VoiceRoomCommand::TelaAssistir`, que chega em
`VoiceRoom::assistir` (`voice_room.rs:657`). A primeira linha de lá é:

```rust
let Some(curso) = self.telas.values_mut().find(|curso| curso.screen == screen) else {
    self.drops.tela_sem_dono += 1;
    return;
};
```

**Medido, não lido.** Escrevi um teste descartável no módulo de testes de
`voice_room.rs` — sala com duas pessoas, `TelaAssistir` para uma `ScreenId` que
nunca existiu — e li os contadores:

```
tela_sem_dono=1 copias=0
```

Sem pânico, sem erro, sem estado ruim: a sala conta um descarte e volta. **Não
consertei nada lá**, porque a condição do brief («se ele reclamar ou entrar em
estado ruim») não se verificou. O que ficou é uma ressalva sobre o contador, na
seção de achados no fim.

**TDD, com duas versões falsas antes da honesta** — vale registrar, porque as
duas passaram de primeira:

1. A primeira pedia «um quadro com `seq > pelo_par + 1`». Passou com o defeito
   no lugar: o canal de avisos é FIFO e `pelo_par` era o **primeiro** quadro da
   fila, com dezenas de posteriores já enfileirados. É o mesmo falso positivo
   que a Task 10 caçou no teste central.
2. A segunda drenou a fila antes de medir — e continuou passando, por um motivo
   pior: `empresta.assistir(screen, false)` era chamado **antes** de a ligação
   com o par sequer fechar, então o que o teste media era `AssistaTelaPor` sem
   par nenhum do outro lado. Instrumentei `repassar_a_tela` com um `eprintln!`
   temporário e vi: `repassar_a_tela COMECOU` nunca era impresso, e zero quadros
   chegavam pelo par.

A versão que ficou prova primeiro que o repasse está **mesmo** no ar
(`QUADROS_PARA_PROVAR` quadros acima de um piso drenado, com `copias == 1` do
primeiro ao último), só então encerra o repasse limpo, drena um piso novo, e
exige mais um segundo de imagem. Aí ela falhou pelo motivo certo:

```
Error: a paciência acabou esperando: um quadro chegar pelo servidor depois de o
repasse ter terminado limpo
```

**Prova por reversão.** `Ok(None) => break` restaurado, suíte inteira do
arquivo: **4 passaram, 1 falhou** — só
`o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor`, com a mesma mensagem.

---

## 4 · C1 (proporcional) — o repasse recusa a segunda tela em vez de fundi-la

**Commit** `36e75a3` · `crates/seele-core/src/enlace.rs`

`RepasseDeTela` passa a guardar de qual transmissão é o repasse em curso.
`abriu()` de uma segunda tela recusa com `warn!`; `pedaco()`, `fechou()` e a
abertura só agem quando a identidade bate. Comentário no struct diz que a
versão per-tela é do subprojeto B e por quê (exige marca de tela no fio entre
pares — mudança de protocolo).

Os três campos passaram a viver sob **um punho só** (`EstadoDoRepasse`):
«estes bytes vão para o par?» é uma pergunta sobre a tela em curso *e* sobre
haver destino, e respondê-la em dois instantes é deixar a transmissão trocar no
meio.

`abertura()` virou `abertura_de(screen)`: com duas transmissões no ar, servir o
pedido do servidor com a abertura da tela errada seria entregar ao par uma tela
com o nome de outra. `repassar_a_tela` recusa e avisa, e quem assiste continua
sendo servido pelo servidor.

**Desvio do brief, declarado.** O brief pediu identidade «dono + `ScreenId`». O
**dono não existe deste lado**: `ScreenHeader` não carrega pessoa,
`Aviso::TelaAbriu` não carrega, e não há mapa `screen → person` no cliente
(conferido por `grep` em `enlace.rs`, `client.rs` e `seele-ffi/src/lib.rs`). A
identidade é o `ScreenId`, que é atribuído pelo servidor e é único por
transmissão — que é exatamente a pergunta que o campo responde. Está escrito no
doc do campo.

**Prova por reversão.** As quatro conferências de identidade neutralizadas:
**287 passaram, 1 falhou** —
`so_uma_tela_por_vez_e_repassada_e_o_fim_da_outra_nao_a_derruba`, com
`assertion left == right failed: a segunda tela assumiu o repasse da primeira`.

---

## 5 · I1 — trava de versão nas duas mensagens v4 do servidor

**Commit** `6cd8d80` · `crates/seele-server/src/session.rs`

O `match entende` saiu do laço e virou `entende_a_mensagem(&message, versao)`,
função pura. Dentro do laço, exercitar a promessa exigiria montar uma sessão v3
inteira; extraída, ela é afirmável mensagem a mensagem — a mesma razão de
`pares::quem_desacreditar` ter saído do braço de `ParFalhou`.

`SirvaTelaPara` e `AssistaTelaPor` ganharam `versao >= 4`.

**O `>= 2` de `UplinkLoss` ficou**, com o comentário dizendo que é vácuo desde
que `oldest_supported_version()` virou 3, e com um teste
(`a_versao_mais_velha_ainda_aceita_recebe_tudo_o_que_nao_e_da_v4`) que afirma
esse 3 em voz alta e vai falhar no dia em que a janela alargar. Apagar a linha
custaria arqueologia para reconstruí-la nesse dia.

**Prova por reversão.** Braço `>= 4` removido: **370 passaram, 1 falhou** —
`as_duas_mensagens_da_v4_nao_saem_para_um_cliente_v3`, com
`SirvaTelaPara saiu para um cliente v3`.

---

## 6 · I2 — cada repasse bem-sucedido queima um par para sempre

**Commit** `ecfe474` · `crates/seele-server/src/pares.rs`,
`crates/seele-server/src/session.rs`

A nomeação passou a guardar **os dois lados** (`Nomeacao { empresta, assiste }`).
Sem quem assiste, não havia como desfazê-la quando ela vai embora — e nem a
saída da sala nem o fim da sessão mandam mensagem nenhuma sobre a tela.

`desapontou` passou a ser chamada em todo caminho que encerra o repasse:

| caminho | onde |
|---|---|
| fim de `WatchScreen` (`UnwatchScreen`) | braço de `Watch/UnwatchScreen` |
| quem compartilha para | braço de `StopScreenShare` |
| a sala é apagada | `Event::VoiceRoomDeleted` → `encerrar_voice_room` |
| quem assiste sai da sala, ou a sessão acaba | `soltar_telas_e_pares_de` |
| a sessão de quem assiste fecha | `Pares::saiu` (fora do `if` da conexão) |

`encerrar_telas_de` virou `soltar_telas_e_pares_de`: é o funil por onde toda
saída de sala já passava (cinco pontos de chamada), e o nome passou a dizer as
duas coisas que ele faz.

**Prova por reversão — e o que ela mostrou que eu não esperava.** A primeira
versão do teste ponta a ponta encerrava o repasse com `UnwatchScreen`. Reverter
a linha de `desapontou` daquele braço **não fez teste nenhum falhar**: 6 de 6
verdes. O motivo é o item 3 — depois dele, o fim limpo do repasse já vira um
`ParFalhou`, e o braço de `ParFalhou` já chamava `desapontou`. Para um cliente
vivo e educado, aquele caminho é redundante.

Então retarget: o teste passou a encerrar com **quem assiste saindo da sala**,
que é o caminho que nenhuma mensagem de cliente cobre. Com
`pares.quem_assiste_saiu(person)` removido:

```
um_repasse_encerrado_normalmente_devolve_o_par_a_fila ... FAILED
o repasse terminou bem e o par continua contado como ocupado ({PersonId(2)}) —
o servidor nunca mais vai escolhê-lo, e o cliente dele já devolveu a vaga
```

**5 passaram, 1 falhou** — só ele.

**As outras três chamadas de `desapontou` ficam sem prova por reversão**, e digo
isso por extenso em vez de deixar passar: depois do item 3, um cliente vivo
sempre relata, e o `ParFalhou` chega ao mesmo instante. Mantive-as porque o
servidor não deve depender de uma mensagem de cliente para sua própria
escrituração — um cliente que trave entre o `UnwatchScreen` e o fim do fluxo
não manda nada, e a vaga ficaria queimada. É defesa em profundidade declarada,
não guarda provado.

Três testes de unidade em `pares.rs` prendem a decisão em si:
`quem_assiste_indo_embora_devolve_o_par_a_quem_pode_escolher`,
`a_saida_de_quem_assiste_nao_derruba_a_nomeacao_de_outra_pessoa` (o `retain`
escrito ao contrário) e `a_sessao_que_acaba_devolve_o_par_que_servia_esta_pessoa`.

---

## 7 · I3 — quem empresta desiste de atender quando a própria discagem falha

**Commit** `4408ea8` · `crates/seele-core/src/enlace.rs`,
`docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`

O braço da discagem virou `discagem_so_pelo_furo(disca)`, que espera a discagem,
manda o resultado para o rastro, e **nunca resolve**. O `select!` passa a ser
decidido só por `atender`; o prazo global continua sendo o `PRAZO_DO_PAR` que
`par::atender` já recebe.

**TDD.** `a_discagem_que_falha_na_hora_nao_cancela_o_atendimento`: `servir_um_par`
com lista de endereços **vazia** — o erro instantâneo mais limpo que existe,
porque `par::ligar` não tem candidato para tentar e devolve `NaoAlcancou` sem
esperar um milissegundo — e quem assiste discando de verdade logo depois.

**Prova por reversão.** `discado = disca => discado.ok()` restaurado: **288
passaram, 1 falhou**, com

```
a discagem de quem empresta falhou na hora e levou o `atender` junto: quem
empresta desistiu de servir alguém que estava chegando
```

**Spec.** Emenda `### 3.3.1 · Um disca, o outro atende — revisão de 07/09/2026`,
no mesmo formato do `3.2.1` que já existia. Ela escreve o desenho real (quem
empresta atende e disca; quem assiste só disca; a discagem de quem empresta
existe pelo efeito, não pelo resultado) e corrige a primeira das «três coisas
boas» do §3.3: o caso assimétrico se resolve sozinho **enquanto quem assiste
conseguir sair**.

---

## 8 · M1 — o `# Errors` de `ligar` mente

**Commit** `d71993a` · `crates/seele-core/src/par.rs`

Passou a listar os cinco motivos, cada um com a pergunta que responde:
`NaoAlcancou`, `ImpressaoNaoBate`, `RecusadoDepoisDeLigar`,
`ConfirmacaoNaoChegouATempo`, `Escuta` — e diz também quais duas variantes de
`ErroDePar` **não** saem daqui (`Certificado`, de `passar_a_atender`; `Repasse`,
de `repassar`).

Conferido no fonte, e não na lista da revisão: `config_de_cliente` mapeia para
`Escuta`, `connect_with` também, a tarefa que morre sem responder também;
`confirmar_com_quem_atende` devolve `RecusadoDepoisDeLigar`; o bloco final
troca `NaoAlcancou` por `ConfirmacaoNaoChegouATempo` quando alguém apertou a
mão.

Mudança de doc: sem teste e sem prova por reversão.

---

## 9 · M2 — a impressão digital confere só o teto

**Commit** `64a9b01` · `crates/seele-proto/src/control.rs`

`MAX_IMPRESSAO_LEN` virou `IMPRESSAO_LEN` (não é teto, é tamanho), e a
validação virou `check_impressao`: exatamente 64 dígitos hexadecimais. A mesma
regra que `crate::uri` aplica ao `fp=` do `seele://`, maiúsculas incluídas — um
formato só para a mesma coisa. Motivo enumerado:
`ControlError::FieldOutOfRange { field: "impressao" }`.

Aplicada às **três** mensagens que carregam impressão, e não só à de entrada:
uma saída que aceitasse o que a entrada recusa seria uma segunda regra esperando
para discordar.

**TDD.** Os testes foram escritos primeiro e falharam:

```
uma_impressao_que_nao_e_um_sha256_em_hex_e_recusada_no_fio
  uma impressão de 0 caracteres que não é um SHA-256 em hex passou pela validação
as_duas_mensagens_do_servidor_conferem_a_impressao_com_a_mesma_regra ... FAILED
```

Cobertos `""`, 63, 65, `"z".repeat(64)` e 63 hex + `g`; e a outra metade
(`uma_impressao_de_verdade_continua_passando`), porque um guarda que recusasse
tudo passaria no primeiro teste e desligaria a malha inteira.

**Efeito colateral, e ele é o guarda funcionando.**
`o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou` construía
`AssistaTelaPor { impressao: String::new(), .. }` e passou a falhar em `encode`
— que valida antes de serializar. A fixture ganhou uma impressão de verdade,
com o comentário dizendo por quê; a pergunta daquele teste é sobre o ordinal, e
um campo inválido a trocaria por outra.

**Prova por reversão.** Ver a linha de TDD acima: os testes foram rodados contra
o código sem `check_impressao` e falharam pelo motivo certo; o conserto é a
própria função, e removê-la é o estado em que eles falharam.

---

## 10 · A ressalva de `stable_id`

**Commit** `464fe41` · `crates/seele-server/src/pares.rs`

Doc de `QuemDeclarou::id_da_conexao` ganhou a seção **«A ressalva, e o que quebra
se ela cair»**: `stable_id()` é o endereço de alocação do estado interno da
conexão no `quinn` 0.11 — estável enquanto ela vive, reusável depois que ela
morre. Se a suposição cair, `Pares::saiu` passa a apagar a declaração **viva** de
outra conexão, caladamente, porque não há erro a dar nesse caminho. O doc diz
também o que pôr no lugar nesse dia: um identificador de sessão que o servidor
mesmo atribua.

Mudança de doc: sem teste.

---

## 11 · `locais_a_publicar(true, ..)` não tem caminho de produção

**Commit** `103f1b3` · `crates/seele-core/src/enlace.rs`

Conferido antes de escrever: `grep -rn emprestar_subida crates apps xtask tools`
devolve seis linhas, e nenhuma delas está em `apps/` ou no `seele-ffi` — os
únicos chamadores são a API pública (`Enlace::emprestar_subida`), o `Motor` que
a serve, e o teste de integração `tela_por_um_par.rs`.

`locais_a_publicar` ganhou **«O ramo `true` não tem caminho de produção hoje»** e
`locais_de_pares` ganhou **«Sem caminho de produção hoje»**, as duas apontando
para `docs/teste-duas-maquinas.md`, que já registrava o mesmo. O que está preso
por teste é o guarda do opt-in; o que **não** foi exercitado é o caminho de LAN
que ele destranca.

Mudança de doc: sem teste.

---

## 12 · M5 — a spec se contradiz sobre o tipo da impressão

**Commit** `004322a` · `docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`

As três assinaturas do §4 diziam `impressao: [u8; 32]`; o parágrafo logo antes
delas e o código dizem `String`. As três foram corrigidas, e a emenda
`### 4.1 · A impressão é `String` nas assinaturas também — emenda de 07/09/2026`
diz por que não é cosmética: o texto que elas contradiziam é justamente o que
explica **por que** `String` — «dois formatos para o mesmo hash é o começo de os
dois discordarem» —, e a spec carregando os dois já era a primeira discordância.

A emenda registra também o item 9: «exatamente 64 caracteres» virou validação de
fio, e não descrição.

A menção a `[u8; 32]` que sobra no §4 (linha 238) é prosa deliberada — «`String`
e não `[u8; 32]` porque…» — e ficou.

---

## Achado e não consertado

Nada aqui foi tocado. Ordem de quanto me preocupa.

### 1 · O contador `tela_sem_dono` passou a contar um caso de rotina

`VoiceRoom::assistir` (`voice_room.rs:659`) conta um `drops.tela_sem_dono`
quando a tela pedida não existe mais. O doc daquele campo diz outra coisa — «Um
fluxo de tela chegou de quem não estava registrado transmitindo» — e o outro
ponto de chamada (`tela_bytes`, `:769`) emite um `warn!` alto falando de «dois
lados que discordam sobre o que é uma transmissão aberta».

A confusão é **anterior** a esta onda (a corrida entre `WatchScreen` e o fim de
uma transmissão já a produzia), mas o item 3 a alarga: depois dele, todo fim
normal de uma transmissão servida por par vira um `ParFalhou`, e o `ParFalhou`
que chega depois de a tela ter acabado de verdade cai exatamente ali. Um
contador de discordância que conta rotina deixa de servir para achar
discordância.

Não consertei porque a condição do brief não se verificou — não reclama, não
entra em estado ruim — e porque o conserto certo é ou um contador próprio ou um
`debug!` naquele ramo, e nenhum dos dois é um dos doze. **Sugestão:** um campo
`tela_ja_acabou` ao lado, com o doc dizendo que é rotina.

### 2 · O piso de `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`

Ele pede um quadro com `seq > pelo_par + 1`, onde `pelo_par` é o **primeiro**
quadro da fila FIFO de avisos. É a mesma fraqueza que a primeira redação do meu
teste do item 3 tinha, e que só apareceu porque eu a reproduzi: a asserção pode
ser satisfeita por um quadro que já estava enfileirado antes de o par morrer.

O ledger já registrou `alvo = pelo_par + 1` como piso frágil e o re-revisor
recomendou agendar em vez de bloquear; a revisão final concordou. Continuo
concordando — mas agora com uma medida em vez de uma opinião: **o meu teste
irmão passou duas vezes com o defeito no lugar por causa desse mesmo padrão.**
O conserto é o que o teste central já faz e o meu passou a fazer: drenar a fila
com `maior_seq_ja_enfileirado` e exigir sustentação.

### 3 · Ninguém manda quem empresta parar de repassar quando quem assiste sai

Depois do item 6 o servidor solta a **nomeação** quando quem assiste sai da
sala, mas não manda nada a quem empresta. Do lado do cliente, `RepasseDeTela`
continua ligado e `atendendo_pares` continua tomado até a conexão QUIC com o par
morrer sozinha. O servidor e o cliente voltam a discordar — na direção oposta à
do I2, e por menos tempo, mas discordam.

O conserto pede um verbo novo do servidor para quem empresta («pare de servir
aquele par»), que é mudança de protocolo, e portanto subprojeto B. Registro
aqui para o desenho dele.

### 4 · Achados da revisão final que o brief não listou

Não os toquei, e listo para o coordenador não supor que sumiram: **I4** (o
endereço de quem assiste é entregue a outro cliente sem opt-in nem aviso do lado
dele), **I6** (três leitores do mesmo formato de quadro sem vetor de bytes
comum), **M3** (150 ms de `recusar_sobras` no início de todo repasse) e **M4**
(`escolher` não exige que o par apontado esteja assistindo à tela — o servidor
tem `Telas` e não a usa).

### 5 · A promessa dos dois números do §2 continua sem cumprir

A revisão final abriu isso e o brief não pediu conserto. Registro que continua
verdade depois desta onda: o A1 não entrega «quanto custa um salto» nem «com que
frequência o furo funciona», e a spec continua dizendo que entrega. É decisão de
dono, não de implementação.

---

## Os quatro comandos

Rodados na árvore final, com os doze commits aplicados.

```
$ cargo test --workspace
1688 passaram, 0 falharam, 4 ignorados, em 69 suítes
```

Os quatro ignorados são os que a branch já ignorava. **Dezesseis testes novos**,
em cinco lugares: `tela_por_um_par` foi de 3 para 6, `pares::testes` de 13 para
18, `enlace::tests` ganhou 2, `session::versao_no_fio` nasceu com 3, e o módulo
de testes de `control.rs` ganhou 3.

```
$ cargo clippy --workspace --all-targets
Finished `dev` profile in 13.31s — nenhum aviso
```

```
$ cargo fmt --check
sem diferenças
```

```
$ cargo xtask check-deps
check-deps: dependency rule holds across 11 workspace crates.
```

O ADR 0002 continua valendo: nada do que esta onda mexeu criou dependência nova
entre `seele-server` e `seele-core`. O `na_sala` do item 2 atravessa como
`HashSet<PersonId>`, que é `seele-proto`; a decisão do item 5 é função pura
dentro do próprio daemon.

---

## Os doze commits

| # | item | commit |
|---|---|---|
| 1 | I5 · o contador de cópias | `005b18d` |
| 2 | C2 · vazamento entre salas de voz | `fcadb2e` |
| 3 | C3 · fim limpo do repasse | `eb96701` |
| 4 | C1 · a segunda tela é recusada, não fundida | `36e75a3` |
| 5 | I1 · trava de versão nas duas mensagens v4 | `6cd8d80` |
| 6 | I2 · o par volta à fila | `ecfe474` |
| 7 | I3 · a discagem não cancela o atendimento | `4408ea8` |
| 8 | M1 · o `# Errors` de `ligar` | `d71993a` |
| 9 | M2 · a impressão conferida inteira | `64a9b01` |
| 10 | a ressalva de `stable_id` | `464fe41` |
| 11 | o empréstimo sem caminho de produção | `103f1b3` |
| 12 | M5 · a spec e o tipo da impressão | `004322a` |

