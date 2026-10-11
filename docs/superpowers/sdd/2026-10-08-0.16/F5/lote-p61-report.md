# Lote P61 da F5: a queda recuperável não é fim — relatório

O ramo é `0.16/lotes-p61`, criado a partir de `b101e385` (a ponta do
`0.16/lotes` com o Y, o W2 e o W3). O worktree fica em
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a708f0e38f09834c3`.

É o lado do cliente da pendência 61. O lado do servidor é da F1, no commit
`1ebf85a4`.

## Os commits

| commit | o que faz |
|---|---|
| `d83a9681` | fase A, o instrumento: o aparelho de mentira e a medida |
| `1f32b934` | fase B: uma regra só decide o fim; o `Room` deixa de mostrar fim numa queda |
| `4ea56edc` | fase C, «em curso, não rodado», escrito durante a pausa de CPU |
| `ad939e36` | fase C verde nos testes de crate, com as reversões |
| `fa76d574` | fase A, a medida: o instrumento lê o enlace no aviso; números no corpo |
| `85d2946b` | fase B, a prova pela FFI |
| `c5be8bb9` | fase D: o TENTAR DE NOVO confere o tamanho, e a voz fecha num fim de verdade |
| `2cec3729` | `par_lento` sem o perdão (exige o `1ebf85a4`; entra na mesma integração) |
| `eb840e60` | o relatório, primeira versão |
| `3beac691` | rodada 1, passo 0: o merge `--no-ff` do `0.16/lotes` em `3a60f121` (a décima terceira, com o `1ebf85a4`) |
| `cd619b4c` | rodada 1, I1: o eco de uma linha antiga não fecha a busca antes do buraco |
| `60738c94` | rodada 1, I2 e M1: a cerca perdida tem prazo; a página vazia não é começo sozinha; o teto diz a verdade |
| `0c636998` | rodada 1, I2: a medida pela FFI da página que o limitador descarta |
| `1516ab14` | rodada 1, M3: o passo `FellBehind` nos dois produtores de retrato |
| `6e6bdc13` | rodada 1, M2 (opção B): um campo grande demais é recusa, e não queda |
| este | o relatório com a rodada 1 |

O lote mexeu nestes arquivos:
- `crates/seele-audio` (o recurso de teste);
- `crates/seele-sessao` (`despedida.rs` novo e o braço `Disconnecting` do
  `state.rs`, com aviso prévio à F4);
- `crates/seele-core` (`enlace.rs`, `client.rs`, `lib.rs`);
- `crates/seele-ffi` (`volta.rs` novo e mudanças pontuais em `lib.rs`);
- `crates/seele-conformance` (dois arquivos novos, e o passo da F4 em
  `dois_produtores_de_snapshot.rs` na rodada 1);
- `crates/seele-server/tests/par_lento.rs`;
- na rodada 1, um comentário em `apps/seele-app/ui/index.html` e o item 55
  da pendência 58 em `docs/pendencias.md`.

O fio não mudou. O lote não mexeu em `seele-proto` nem em `main.rs`, e do
`ui/` só o comentário. O caminho da tela é o mesmo de uma queda de rede,
então não houve cena nova na bancada.

---

## Fase A: as medidas, antes do conserto

### O instrumento

O arquivo é `crates/seele-conformance/tests/medida_da_queda_recuperavel.rs`,
com os testes em `#[ignore]`. Para refazer a medida:

```sh
cargo test -p seele-conformance --test medida_da_queda_recuperavel -- --ignored --nocapture --test-threads=1
```

**O aparelho de mentira.** Fica em `crates/seele-audio/src/device/mentira.rs`,
atrás do recurso de teste `aparelho-de-mentira`.
- Entra pelo mesmo `device::open` de sempre, quando o id pedido começa com
  `seele-mentira:`.
- Tudo acima dele é o produto: a porta da voz da FFI, `start_preferring`,
  `reopen`, o laço, o reamostrador, o codificador e o canal de mídia.
- Uma linha faz o papel dos dois retornos do `cpal`. A cada 10 ms ela entrega
  um tom de 440 Hz e consome a saída, pelos mesmos `on_capture` e
  `on_playback`.
- O `cpal` nunca é tocado, nem para enumerar. Os dois lados são sempre de
  mentira juntos.
- `mentira::contagem(nome)` diz quantos aparelhos estão abertos e quantas
  vezes abriram.
- Um `compile_error!` recusa o recurso num build otimizado. Só a conformidade
  o liga, como dependência de teste, e o resolvedor 2 não o leva ao produto.

**O `FellBehind` encenado.** O instrumento manda 8 × 1 024 eventos ao
barramento de uma vez, sem `await`, que é o que faz um lote do escritor maior
que o anel.

**Como o enlace é lido.** O enlace é lido do retrato no próprio aviso da FFI,
como a janela faz. A faixa da bateria dura de 6 a 8 ms quando a primeira
volta dá certo, e uma amostra de 20 ms nunca a vê.

### A voz debaixo da tela de fim (defeito 4)

Medido em `d83a9681`, antes de qualquer conserto:
- O `Ended { reason: FellBehind }` chega à janela **1,4 ms** depois da queda,
  e vêm mais **21 eventos** depois dele.
- O aparelho **reabriu em menos de 22 ms** (uma abertura a mais) e ficou
  aberto até o `disconnect`, que no app é o RECONECTAR ou o SAIR.
- O servidor continua com ela sentada na sala.
- **Quem entra depois a ouve falando**: 2 de 2 `PersonState` com `speaking`.

**A voz reabre e transmite com a tela dizendo «encerrado»**: o defeito
estava lido, e agora está medido.

Na v0.15.1 publicada, a regra é só `Kicked | Banned | ModsMudaram`. Pelo
código, então, o mesmo vale para `Timeout`, `ScheduledMaintenance`,
`ServerShuttingDown` e as outras razões recuperáveis (ver a nota de porte na
fase D).

### O TENTAR DE NOVO numa recusada por `TooLong` (defeito 5)

Medido em `d83a9681`:
- O corpo tem 6 000 bytes (3 000 `á`). A pendente estava em
  `Recusada(TooLong)`, e `reenviar_mensagem` devolveu `Ok(true)`.
- **O enlace caiu e voltou em menos de 22 ms**, sem fim, e ela continuou
  sentada. O caminho era o lido: o `encode` recusa o corpo, e
  `Motor::executar` chama `cair()`.
- **A pendente passou para `SemResposta` com os mesmos 6 000 bytes**, e nada
  foi gravado. A frase da recusa se perde, e a tela oferece como seguro um
  reenvio que derruba o enlace de novo.

**O laço que isso abriria.** Com a repetição da fase C sem a conferência de
tamanho, essa pendente seria repetida a cada volta, derrubaria o enlace,
voltaria a `SemResposta`, e assim para sempre. Por isso a D entrou antes da
prova da C, e a repetição passa pela mesma conferência.

### A mesma medida depois das fases B e C (`ad939e36`)

- Não há `Ended`. A janela lê `Bateria(0)` aos 2 ms e `Online` aos 8 ms. A voz
  volta e transmite, o que agora está certo, porque a tela não diz mais
  «encerrado».
- O `TooLong` ainda derrubava o enlace (bateria aos 0 ms, volta aos 5 ms).
  Mas a repetição da volta o recusou pela conferência de tamanho, e o laço não
  aconteceu. O conserto do caminho é a fase D.

---

## Fase B: uma regra só

**A regra.** É `seele_sessao::despedida::a_sessao_acabou_aqui`.
- A lista e a explicação saíram do `enlace.rs` sem mudança: `Kicked`,
  `Banned`, `ModsMudaram`, as três da sala pessoal e `ServerShuttingDown`.
- O teste exaustivo foi junto.
- O `seele-core` a reexporta como `seele_core::despedida`, e o motor a
  importa.
- O `seele-sessao` continua só com o `seele-proto`. O `check-deps` passa, e o
  crate compila para `wasm32-unknown-unknown`.

**O `Room`** (`state.rs`, braço `Disconnecting`):
- só a regra marca `ended`;
- a razão recuperável chama `envios_sem_resposta()` e marca
  `changed.messages`, o que a queda de rede faz com o que esta máquina
  esperava.

A faixa vem do `Aviso::Estado` do núcleo, logo atrás: o servidor fecha a
conexão em seguida à despedida.

**A FFI** não mudou. Sem `changed.ended`, ela não emite `Ended`, e o `Bridge`
não revoga a geração.

**Provas:**
- `state.rs`:
  - `so_as_despedidas_que_acabam_a_sessao_marcam_o_fim`: um caso por razão,
    com a tabela escrita à parte. Nas recuperáveis, exige `SemResposta` e
    `changed.messages`.
  - `toda_despedida_acaba_onde_o_motor_acaba`: o `Room` contra a regra.
- `enlace.rs`: `o_motor_acaba_a_sessao_onde_o_room_acaba`, o par, que confere
  contra o `Room` o nome que **o motor chama**.
- Pela FFI, em `queda_recuperavel.rs`:
  `ficar_para_tras_mostra_a_faixa_e_a_volta_e_nao_o_fim`.
  - O `FellBehind` é injetado pelo `Event::SessionEnded`, o caminho da
    expulsão.
  - A janela lê `Bateria` e depois `Online`, e não recebe `Ended`.
  - O retrato não diz fim, e o servidor continua com ela sentada.

**Reversões.** Todas voltaram idênticas, conferidas com `cmp`, e os arquivos
foram tocados depois.
- O `Room` marcando fim para toda `Disconnecting`:
  - reprovam os dois testes do `state.rs`;
  - reprova a prova pela FFI: «a janela recebeu a tela de fim numa queda que
    o núcleo reconecta», com `Some(Ended { reason: FellBehind })`.
- Uma lista própria no motor, só com `Kicked` e `Banned`: reprova o par
  («ServerShuttingDown: o motor e o `Room` discordam»).
- `FellBehind` posto na regra: reprovam o exaustivo e o da tabela.

**O pedido da F4.** Entrou na rodada 1 (M3, `1516ab14`), depois do merge
que trouxe o arquivo. Na primeira versão, o passo `Disconnecting { FellBehind }` em
`dois_produtores_de_snapshot.rs` **não entrou**. Esse arquivo e o
`seele-sessao/src/retrato.rs` não estão na base `b101e385`. O passo fica para
depois do encontro com o `0.16/web`.

---

## Fase C: o buraco e a repetição

**No núcleo:**
- `Client::send_ping_marcado(carimbo)`.
- `Enlace::historico_com_cerca(linha, cursor, limite, carimbo)`: pede uma
  página atrás do cursor e manda logo atrás um `Ping` com o carimbo. O `Pong`
  desse carimbo diz que a página acabou.
- O comando não é lembrado para a volta.

**As duas conferências no servidor:**
1. **O `Pong` devolve o carimbo inteiro.** `ClientMessage::Ping` não tem
   validação, o `postcard` leva o `u64` inteiro, e
   `session.rs:2596` responde com o mesmo valor. O `Ping` de entrada, no bit
   63, já depende disso.
2. **O `FetchHistory` com cursor é o que supus.**
   - Traz as mensagens com `m.id < cursor`, da mais nova para a mais antiga,
     até `limit.clamp(1, 200)` (`MAX_PAGE`); com `limit == 0`, até 50.
   - Manda a página da mais antiga para a mais nova.
   - Escreve a página inteira antes de ler o próximo quadro, então o `Pong` da
     cerca chega depois dela.
   - Um pedido recusado por permissão também é seguido pelo `Pong`, e a busca
     o lê como página vazia.

**Em `crates/seele-ffi/src/volta.rs`**, só lógica, sem rede:
- `Buraco`: busca o histórico do canal aberto, de trás para a frente, com a
  cerca no **bit 62**.
  - O bit 62 não se confunde com o 63 da entrada, nem com o contador de
    rotina, que conta a partir de 1.
  - A busca para quando chega à última mensagem que esta sessão conhecia,
    quando uma página não traz nada mais antigo que o cursor (o começo do
    canal), ou no **teto de 10 páginas de 200**, ou seja, 2 000 mensagens.
  - Quem não conhecia nada do canal recebe só a página mais nova, como quando
    abre o canal.
  - A parada não conta mensagens, então um servidor que corte a página abaixo
    de 200 não a engana.
- `Repeticao`: repete as pendentes `SemResposta` e `Enviando`, na ordem do
  `Room`, que é a ordem em que foram escritas.
  - Saem **20 de uma vez e depois 1 a cada 125 ms** (8 por segundo). O balde
    do servidor deixa 60 de rajada e 20 por segundo, e acima disso descarta
    calado.
- `repetir`: usa sempre **a mesma chave**. Uma pendente grande demais é
  recusada ali, com a frase dela, e não sai.
- `grande_demais`: a conferência de tamanho, uma só. Usam a mesma o envio, o
  TENTAR DE NOVO (desde a D) e a repetição.

**No laço da FFI (`drive`):**
- na volta, abre a busca e enche a fila de repetição;
- depois de cada mensagem dobrada no `Room`, pergunta à busca se a página
  acabou;
- um braço novo do `select!` solta a vez de cada repetição;
- uma nova queda descarta as duas, que recomeçam do `Room` na volta seguinte.

**O eco duplo** («quem mandou recebe o eco duas vezes») não duplica na tela.
- O servidor devolve a linha original, com o mesmo id, para a mesma chave da
  mesma pessoa (`Messages::append_batch`).
- O `Room` ignora um id que já tem e confirma a pendente pela chave.

**Provas de crate (`volta::tests`):**
- a busca fecha na última mensagem conhecida (450 mensagens faltando, três
  páginas, e nada duplicado);
- mensagens que chegam ao vivo no meio da página não enganam a parada;
- o começo do canal, o teto e a página mais nova;
- o `Pong` de outra cerca (rotina, entrada, uma busca antiga) não fecha a
  página;
- a ordem;
- a rajada e o espaçamento;
- a pendente grande demais;
- a mesma chave não duplica a mensagem, com um servidor de mentira
  idempotente pela chave.

**Reversões de crate.** Todas reprovaram e voltaram idênticas (`cmp`):

| reversão | o que reprovou |
|---|---|
| sem a repetição | a ordem e o espaçamento |
| repetição fora da ordem | a ordem e o espaçamento |
| sem a busca (para na primeira página) | os três testes da busca |
| a repetição com uma chave nova | «a mensagem saiu duplicada na tela» (2 contra 1) |
| sem o espaçamento | o espaçamento |
| sem a conferência de tamanho | a pendente grande demais |
| «a mais antiga» trocada pela última que chegou | os três testes da busca |
| qualquer `Pong` fecha a página | a cerca |

**A prova pela FFI:**
`queda_recuperavel::a_volta_traz_o_buraco_e_o_que_ficou_sem_resposta_uma_vez_cada`.
- **O cenário:**
  - 450 mensagens de outra pessoa são gravadas direto no banco, sem anúncio,
    então só o histórico as alcança;
  - ela escreve 6 mensagens, e a despedida `FellBehind` sai logo atrás.
- **O ajuste aprovado:** não há mensagens «durante a bateria», porque ela dura
  milissegundos. As 6 do instante dependem da repetição de qualquer jeito: ou
  o servidor não as leu, ou gravou e o eco foi para uma sessão que já tinha
  acabado.
- **O resultado:** cada uma das 456 aparece **uma vez** na tela, as 6 dela
  aparecem confirmadas e na ordem em que foram escritas, e o servidor grava
  1 + 450 + 6.
- **Reversões pela FFI**, todas voltadas idênticas:
  - sem a repetição: «o buraco e as seis dela aparecerem confirmadas não
    aconteceu em 20s»;
  - sem a busca (a busca da volta desligada no `drive`): a mesma espera
    reprova;
  - a repetição com uma chave nova: «"no-instante-1" aparece 2 vezes na tela».

---

## Fase D: o `TooLong` e a voz

### O `TooLong`

`reenviar_mensagem` passou a usar `volta::grande_demais`, no molde do envio.
A recusa volta a ser `Recusada(TooLong)` e nada sai para o enlace.
- **Vermelho antes:**
  - `volta::tests::tentar_de_novo_uma_grande_demais_nao_manda_nada_ao_enlace`
    («TENTAR DE NOVO mandou ao enlace um corpo que o derruba»);
  - pela FFI,
    `queda_recuperavel::tentar_de_novo_uma_grande_demais_nao_derruba_o_enlace`
    (a janela leu a bateria).
- **Verdes depois.** A reversão, sem a conferência, reprova os dois, e o
  arquivo voltou idêntico.

### A voz

`queda_recuperavel::a_voz_fecha_no_fim_e_volta_na_queda`, com o aparelho de
mentira:
- **Com `FellBehind`:** a voz reabre e o servidor a ouve, sem `Ended`.
- **Com `Kicked`:** chega o `Ended`, o aparelho fecha e não reabre em 3 s.
- **Verde sem conserto.** O fim de verdade já fechava a voz:
  `Aviso::Encerrado` sai do laço, e `largar_sem_segurar` a solta.
- **Reversões**, voltadas idênticas:
  - sem `largar_sem_segurar` no fim do `drive`: «a voz fechar não aconteceu em
    20s»;
  - `Kicked` fora da regra: «a tela de fim não aconteceu em 20s» (a expulsão
    vira queda, e a voz voltaria).

### Nota para o porte na 0.15: o menor conserto que fecha a voz sob «encerrado»

Como está na v0.15.1:
- a regra do motor é só `Kicked | Banned | ModsMudaram`;
- o `Room` mora no próprio `seele-core` (`state.rs:1336`) e marca fim para
  toda `Disconnecting`;
- a FFI abre a voz no `connect` (não há pendência 52) e a reabre em
  `Aviso::Reconectado` com `reopen`, sem olhar o `Room`.

**(a) A regra da B.** É a que recomendo.
- **A mudança:** cerca de 10 linhas no `seele-core`.
  - `a_sessao_acabou_aqui` passa a `pub(crate)`.
  - O braço `Disconnecting` do `state.rs` passa a perguntar a ela, e a
    recuperável chama `envios_sem_resposta()`.
  - Mais o teste por razão.
- **O que conserta:**
  - a voz (a tela deixa de dizer «encerrado»);
  - a tela de fim falsa;
  - o «fantasma» (sentada na sala com a janela revogada);
  - os eventos jogados fora.
- **O que custa:**
  - A janela da 0.15 passa a mostrar a faixa em `Timeout`, `ScheduledMaintenance`,
    `FellBehind`, `ServerShuttingDown`, `RateLimited` e `ProtocolViolation`. A
    faixa já existe, porque é o caminho da queda de rede.
  - As pendentes ficam `SemResposta` com TENTAR DE NOVO, sem repetição
    automática (essa é a C, que não vai junto).
  - O buraco do canal não é buscado, como numa queda de rede da 0.15.
  - *(Corrigido pela revisão: a condição abaixo é falsa. O servidor 0.15 só
    usa `ServerShuttingDown` como recusa do aperto de mão, nunca como
    `Disconnecting` no meio da sessão, e o servidor 0.16 recusa o protocolo
    8. No porte (a), ela não precisa entrar na lista; pôr não faz mal.)*
  - Atenção: com a lista da 0.15, `ServerShuttingDown` vira «reconectando»
    por até cinco minutos. A 0.16 o pôs na lista no lote G-servidor. Se o
    servidor 0.15 manda essa razão, ela entra na lista no porte.
  - Atenção também ao `TooLong`: a D (`reenviar_mensagem` conferindo o
    tamanho) é independente e deve ir junto. *(Corrigido pela revisão: na
    0.15 não há `volta::grande_demais`; é a cópia das ~6 linhas do envio, e
    não duas.)*

**(b) A FFI não reabrindo a voz quando o `Room` já marcou fim.**
- **A mudança:** no braço `Reconectado`, se `room.ended.is_some()`, a FFI
  larga a voz (`*slot = None`) em vez de chamar `reopen`. São cerca de 5
  linhas, só na FFI.
- **O que custa:**
  - Trata o sintoma. A voz fica aberta entre o `Ended` e o `Reconectado`
    (milissegundos, mandando para a conexão morta).
  - A sessão segue viva por baixo da tela de fim: sentada na sala, calada,
    vista pelos outros. A janela continua revogada, e os eventos continuam
    jogados fora.
  - Qualquer caminho futuro que reabra a voz (troca de aparelho, por exemplo)
    precisa lembrar da mesma condição.

**(c) Uma terceira, menor ainda:** a FFI tratar o fim do `Room` como fim e
sair do laço do `drive`.
- **A mudança:** 2 ou 3 linhas no braço que emite `Ended`.
- **O que conserta:** a voz cai junto, o núcleo se despede, e o servidor tira
  a pessoa da sala. Tela e sessão concordam.
- **O que custa:** as razões recuperáveis passam a exigir o RECONECTAR
  manual, que é o que a tela da 0.15 já diz. Perde-se a volta automática de
  `ScheduledMaintenance` e de `Timeout`.

**A recomendação.** (a) é o conserto certo e é pequeno. (c) é o menor que
fecha a voz **e** o fantasma. (b) fecha só a voz.

---

## O perdão do `par_lento`

O conserto da F1, `1ebf85a4`, **está em `versao/0.16`**: veio na décima
terceira integração, `c55269d5`. Medi com carga, com 12 processos do teste ao
mesmo tempo, em rodadas seguidas:

| servidor | teste | rodadas | falante desligado | cauda faltando |
|---|---|---|---|---|
| com `1ebf85a4` (`c55269d5`) | com o perdão | 72 (36 com 6 paralelos, 36 com 12) | 0 | 0 (1 160/1 160) |
| com `1ebf85a4` | **sem o perdão** | 108 | 0 | 0, todas verdes |
| sem o conserto (este ramo) | com o perdão | 36 | 6 rodadas (até 16 falantes) | 4 rodadas (1 102 a 1 155 gravadas) |
| sem o conserto | **sem o perdão** | 108 | 8 reprovam | «desligou o falante 15 … faltam [52..57]» |

**O perdão é desnecessário com o conserto, e saiu** (`2cec3729`).
- Um falante desligado reprova, e o teste volta a exigir as 1 160.
- A reversão que prova que ele pega a perda é o servidor sem o `1ebf85a4`:
  8 de 108 reprovam.
- **Neste ramo, sozinho, esse teste reprova às vezes sob carga**, porque o
  conserto do servidor não está aqui. O commit está à parte para entrar na
  mesma integração, ou ser deixado.
- O ouvinte que parou de ler continua desligado com `FellBehind`. Esse é o
  contrato.

---

## Os testes

**De crate:**

| alvo | resultado |
|---|---|
| `seele-ffi --lib` | 203 passaram (1 ignorado) |
| `seele-core --lib` (em série, com o codec) | 461 passaram |
| `seele-sessao` | 77 + 1 passaram |
| `seele-audio --features aparelho-de-mentira`, só `device::mentira` | 2 passaram |
| `xtask --test folha_de_lints` | 1 passou |
| `cargo xtask check-deps` | passou |

fmt e clippy estão limpos nos crates tocados, com e sem o recurso.

**O resto do `seele-audio` não rodou.**
`device::tests::a_listed_output_opens_and_reports_itself` não é `#[ignore]` e
abre o microfone e uma saída de verdade numa máquina que os tem. A mudança ali
é um campo e um ramo, ambos atrás do recurso.

**Conformidade**, um arquivo por vez, com `--test-threads=1`:

| arquivo | resultado |
|---|---|
| `queda_recuperavel` | 4 passaram |
| `bateria_interna` | 7 passaram |
| `volta_pela_trilha` | 6 passaram |
| `voz_na_reconexao` | 1 passou |
| `reconexao_fantasma` | 1 passou |
| `moderacao` | 6 passaram |
| `quem_hospeda_fecha` | 2 passaram |
| `acceptance_m5` | 15 passaram |
| `a_recusa_nao_tira_da_sala` | 5 passaram |
| `saida_com_despedida` | 3 passaram |
| `estados` | 10 passaram |
| `medida_da_queda_recuperavel` (`--ignored`) | 2, rodados duas vezes |

---

## O que fica para a F1, para a F4 e para a pendência 61

Atualizado na rodada 1. O que a primeira versão listava aqui e a rodada 1
consertou está riscado com o commit.

- **F4:** ~~o passo `FellBehind` em `dois_produtores_de_snapshot.rs`~~ —
  entrou (`1516ab14`). A busca do buraco e a repetição estão na FFI e não
  chegam ao navegador: lá a queda recuperável fica como uma queda de rede.
- **A janela não mostra a recusa local de um campo grande demais.** O enlace
  não cai mais (`6e6bdc13`), mas a frase é de outro lote. A pendência 58
  ganhou o item 55, com os campos e onde cada um é escrito na janela (ver a
  rodada 1, M2, abaixo). *A primeira versão deste item listava «a carga de
  pedido de MOD», e estava errado: ela já é conferida em bytes no `base.js`.*
- **A busca só cobre o canal aberto.** O núcleo só reassina o último canal na
  volta, então as não lidas dos outros canais param de andar depois de uma
  queda. Isso já existia antes e é registrado aqui.
- ~~**A cerca pode se perder.**~~ — consertado (`60738c94`): prazo por
  página, pedido de novo uma vez, e desistência dita no `seele.log`.
- **O que passa do teto de páginas não volta nesta sessão**: a janela não tem
  pedido de histórico mais antigo. O teto vai ao `seele.log` como aviso.
- **A ordem da drenagem** (observação da revisão, sem achado): com mais de 20
  pendentes, a cauda da repetição sai a 8 por segundo, e uma mensagem nova
  escrita nesse meio-tempo é gravada antes das mais velhas que ainda esperam
  a vez. A ordem «em que foi escrita» vale entre as repetidas.
- **F1:** o perdão do `par_lento` saiu (`2cec3729`). Com o `1ebf85a4` no ramo
  desde o passo 0 da rodada 1, o teste sem o perdão passa 72 de 72 sob carga.

---

## Rodada 1 da revisão (`lote-p61-review-report.md`): Crítico 0, Importante 2, Menor 3

### Passo 0: o merge do `0.16/lotes` em `3a60f121`

`git merge --no-ff` (`3beac691`). Um conflito só, trivial, em
`seele-sessao/src/lib.rs`: ficam `pub mod despedida;` (deste lote) e
`pub mod retrato;` (da F4). Testes de crate verdes depois do merge.

### I1: o eco de uma linha antiga não fecha mais a busca antes do buraco (`cd619b4c`)

**O defeito, medido pela revisão.** Quem repete uma mensagem que o servidor
já gravou faz o servidor anunciar a linha original, com o id antigo, ao
canal inteiro (`Gravacao::Feita` → `MessagePosted`). O eco caía na janela de
uma página, `Buraco::ouvir` o tomava pela «mais antiga da página», e a busca
pulava para trás dele e dizia «fechou». Pela FFI, com 30 ms entre as
páginas, ficavam 1 200 de 1 500 na tela.

**O conserto.** A página é medida pelo que o servidor manda: as
`PAGINA_DO_BURACO` (200) maiores abaixo do cursor.
- A busca guarda os ids do canal abaixo do cursor que chegaram na janela, e
  a página vai até a menor das 200 maiores (`NoAr::ate_onde`).
- Com a página cheia, um eco antigo abaixo delas fica de fora. Com ela
  incompleta, o servidor chegou ao começo e mandou tudo abaixo do cursor, e o
  eco é uma delas.
- O cursor nunca fica mais antigo que a página de verdade. Um cursor mais
  novo que o devido só pede de novo o que já chegou, e o `Room` não duplica.
- Uma mensagem ao vivo nunca empurra o cursor para trás.
- **A suposição, escrita:** o servidor manda 200 por página quando tem 200
  (`MAX_PAGE`). Um servidor que cortasse abaixo disso faria toda página
  parecer incompleta. Todo servidor que este cliente alcança é 0.16 ou mais
  novo, com `MAX_PAGE = 200`.

**Provas:**
- vermelho antes, no crate, com o cenário da sonda da revisão:
  `o_eco_de_uma_linha_antiga_nao_fecha_a_busca_antes_do_buraco` («faltam 50
  do buraco: [102, 103, …]»);
- vermelho antes, pela FFI:
  `queda_recuperavel::o_eco_de_uma_linha_antiga_no_meio_da_busca_nao_a_fecha_antes_do_buraco`
  («a busca parou com 400 de 1 500»). O eco é o `MessagePosted` da linha
  velha de outra pessoa, posto no barramento a cada 2 ms durante a volta: no
  loopback, a busca anda rápido demais para cair por acaso na janela do lote
  de 200 ms do escritor. É o caso que a prova C não cobria;
- **reversão** (a página até a menor de todas): os dois reprovam («200 de
  1 500»).

### I2: a cerca perdida (`60738c94`, medida em `0c636998`)

**O conserto, em `volta::Buraco`:**
- **Prazo por página** (`PRAZO_DA_PAGINA`, 5 s). Vencido, a busca pede a
  mesma página de novo, uma vez, com `warn` no `seele.log` («não teve
  resposta; pedindo de novo»). Se vence de novo, ela para em
  `Parada::SemResposta`, com `warn`, o canal e o cursor.
- **O começo do canal só vale quando o servidor responde.** Uma página
  incompleta é resposta: o servidor não tinha mais. Uma página vazia sozinha
  não é: ela é pedida de novo depois de `ESPERA_DA_PAGINA_VAZIA` (1 s, vinte
  fichas do balde), e só vazia de novo vale como começo.
- A busca não tem relógio próprio. `quando()` diz até quando dormir, `vez()`
  diz o que fazer quando o instante chega, e o `drive` ganhou um braço do
  `select!` que a acorda. `volta::seguir` faz o que a busca manda e registra a
  parada, para os dois braços fazerem o mesmo.

**O que acontece em cada descarte, medido:**
- **O `Ping` da cerca descartado** — pela FFI (`a_pagina_que_o_limitador_descarta`,
  `#[ignore]`). No instante em que a janela lê a volta, 80 `SetMuted` pela FFI
  esgotam o balde de 60 da conexão nova. O `FetchHistory` e o `Ping` da
  página 2 caem juntos no `Descarta` do `Vigia`. O buraco tinha 1 500.
  - **Antes** (`cd619b4c`): 200 de 1 500 na tela, por 12 s. A busca ficou
    aberta e calada.
  - **Depois:** 200 aos 52 ms, nada até os 5 021 ms, quando vence o prazo e
    a página sai de novo; 1 500 aos 5 075 ms.
- **O `FetchHistory` descartado com o `Ping` passando** — não dá para
  provocar pela FFI: com o balde vazio, os dois caem juntos. Medido no crate,
  com um simulador que descarta por número de pedido e um relógio que anda
  só quando a busca espera:
  - **antes:** a busca lia «começo do canal» e parava, com o buraco de fora;
  - **depois:** a página vazia é pedida de novo depois de 1 s, e o buraco
    fecha inteiro (`o_pedido_descartado_e_pedido_de_novo_e_o_buraco_fecha_inteiro`).

**Provas e reversões:**
- vermelho antes: `uma_pagina_vazia_sozinha_nao_e_o_comeco_do_canal` («a busca
  leu "começo do canal" numa página que o servidor não respondeu»);
- novas: a cerca descartada tem prazo, e duas vezes a mesma dá `SemResposta`;
  o começo numa divisa de página é dito duas vezes;
- **reversões:** sem o prazo da cerca, «a busca não pediu, não parou e não
  espera nada: ficou calada»; a vazia lida como começo na hora reprova três.

### M1: o teto diz a verdade (`60738c94`)

O doc de `TETO_DE_PAGINAS` prometia «o histórico, para quem rolar a
conversa», e a janela não tem pedido de histórico mais antigo. Agora diz que
o que passa do teto não volta nesta sessão, e o teto vai ao `seele.log` como
`warn`, com o canal e o cursor de onde parou.

### M2: um campo grande demais é recusa, e não queda (opção B, `6e6bdc13`)

**O defeito, medido pela revisão:** o apelido, o nome de canal e a senha da
sala acima do teto do fio (em bytes) faziam o `encode` falhar, e
`Motor::executar` chamava `cair()`. Pela leitura, o nome do servidor e o
motivo do banimento também.

**O conserto, no núcleo:**
- `recusa_local`: um erro de comando que é `ControlError` é recusa; qualquer
  outro erro de escrita é do transporte, e é queda.
- `Motor::recusar_aqui`: `warn` no `seele.log` com o comando (`Comando::nome`)
  e a recusa (o campo, o tamanho e o teto), **nunca o valor**. O enlace segue.
- **A entrada numa sala com senha grande demais** é desfeita como o servidor
  desfaz uma senha errada: com a mesma recusa, `VoiceRoomEntryRefused`.
  - Ela desfaz a entrada pendente do motor. Sem isso, a volta a refaria a
    cada queda.
  - **O `Room` precisa saber, e fica sabendo pela mesma porta.** Sem ela, a
    tela punha a pessoa na sala pedida, pelo assento especulativo, enquanto o
    servidor a tinha na de antes. A recusa vai à casca como a do servidor, e
    o `Room` desfaz o assento por `seele_sessao::assento`, mostrando o aviso
    que já existe para a entrada recusada.
  - Sem tipo novo, sem mexer no `state.rs` nem na `NoticeReason`.
- O comentário em `ui/index.html` (o campo do nome do servidor) dizia que a
  recusa «é do servidor e chega como aviso». Agora diz a verdade de hoje e
  aponta para a pendência 58.

**Provas:**
- vermelho antes, pela FFI: `queda_recuperavel::um_campo_grande_demais_nao_derruba_o_enlace`
  (nome de canal de 60 B, apelido de 40 B e senha de 40 B): «um campo grande
  demais derrubou o enlace». Depois: o enlace não cai em 2 s, ela segue
  sentada na sala de antes, e uma mensagem depois sai e é confirmada;
- no crate: `enlace::tests::um_campo_grande_demais_e_recusa_local_e_nao_queda`
  (o que é recusa e o que é queda, também com contexto em cima; o `warn` com o
  comando e o campo, e **sem a senha**; a entrada desfeita no motor). O `warn`
  é provado aqui: a conformidade não tem como ler o registro de outro
  processo de runtime;
- **reversões:** voltar a chamar `cair()` — «um campo grande demais derrubou
  o enlace»; sem desfazer a entrada — «a entrada recusada aqui não desfez o
  assento: a tela a põe noutra sala».

**A metade da janela, para o lote seguinte** (também na pendência 58, item
55). A forma mais barata é a janela conferir o tamanho antes de mandar, em
bytes, com os limites do `seele-proto`, como o envio de mensagem já faz em
`tela-sessao.js` (`limiteDaMensagem` e `bytesDe`).

| campo | teto | onde é escrito | o que manda |
|---|---|---|---|
| apelido | 32 B (`MAX_NICKNAME_LEN`) | `#perfil-apelido` (`camada-nomear.js`) | `escolher_apelido` |
| nome de canal | 48 B (`MAX_CHANNEL_NAME_LEN`) | `#nomear-valor`, `maxlength="48"` em caracteres | `criar_linha` (`tela-sessao.js`), `renomear_linha` (`tela-sessao.js`, `camada-moderar.js`) |
| nome de sala | 48 B (`MAX_CHANNEL_NAME_LEN`) | `#nomear-valor` | `criar_voice_room`, `renomear_voice_room` (`tela-sessao.js`) |
| limite da sala | 1 a 250 (`MAX_VOICE_ROOM_LIMIT`) | o mesmo pedido | `criar_voice_room` |
| nome do servidor | 64 B (`MAX_CLIENT_NAME_LEN`) | `#server-nome-valor`, `maxlength="64"` | `renomear_server` (`tela-server.js`, `tela-boot.js`) |
| senha da sala | 32 B (`MAX_NICKNAME_LEN`) | a caixa de senha antes da entrada | `enter_voice_room` (`tela-sessao.js`) |
| motivo do banimento | 512 B (`MAX_ALERT_TEXT_LEN`) | `#moderar-motivo`, `maxlength="200"` (200 letras CJK dão 600 B) | `banir_pessoa` (`camada-moderar.js`) |

Não precisam: a carga de MOD, já conferida em bytes no `base.js`, e os
ícones, por `check_server_icon`. A senha da sala grande demais a janela já
mostra, como entrada recusada.

### M3: o passo da F4 (`1516ab14`)

`Disconnecting { FellBehind }` antes do `Kicked` em
`dois_produtores_de_snapshot.rs`, com o `ended` nulo nos dois produtores
depois dele. Verde 2/2. **Reversão** do `Room` (fim para toda
`Disconnecting`): «o desktop marcou fim numa queda recuperável:
"FellBehind"».

### Os testes da rodada 1, uma vez só, depois do M2

| alvo | resultado |
|---|---|
| `seele-sessao` | 77 + 1 passaram |
| `seele-ffi --lib` | 208 passaram (1 ignorado) |
| `seele-core --lib` (em série, com o codec) | 466 passaram |
| `seele-audio --features aparelho-de-mentira`, só `device::mentira` | 2 passaram |
| `seele-app --test frontend`, com `HOME` falsa | 295 passaram |
| `xtask --test folha_de_lints`, `check-deps` | passaram |
| `par_lento` sem o perdão, 12 processos × 6 rodadas, com o `1ebf85a4` | 72 de 72, 0 falante desligado, 1 160 de 1 160 |

**Conformidade**, um arquivo por vez, com `--test-threads=1` (sem o
`quarto_pelo_link`):

| arquivo | resultado |
|---|---|
| `queda_recuperavel` | 6 passaram |
| `dois_produtores_de_snapshot` | 2 passaram |
| `bateria_interna` | 7 passaram |
| `volta_pela_trilha` | 6 passaram |
| `voz_na_reconexao` | 1 passou |
| `reconexao_fantasma` | 1 passou |
| `moderacao` | 6 passaram |
| `quem_hospeda_fecha` | 2 passaram |
| `acceptance_m5` | 15 passaram |
| `a_recusa_nao_tira_da_sala` | 5 passaram |
| `senha_da_sala` | 2 passaram |
| `saida_com_despedida` | 3 passaram |
| `estados` | 10 passaram |

fmt e clippy limpos nos crates tocados, com e sem o recurso do aparelho de
mentira. O resto do `seele-audio` continua sem rodar: um teste dele, sem
`#[ignore]`, abre o microfone de verdade.

Todas as reversões desta rodada voltaram idênticas, conferidas com `cmp`, e
os arquivos foram tocados depois.
