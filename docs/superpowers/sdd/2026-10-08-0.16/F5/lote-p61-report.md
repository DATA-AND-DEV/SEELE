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
| este | o relatório |

O lote mexeu nestes arquivos:
- `crates/seele-audio` (o recurso de teste);
- `crates/seele-sessao` (`despedida.rs` novo e o braço `Disconnecting` do
  `state.rs`, com aviso prévio à F4);
- `crates/seele-core` (`enlace.rs`, `client.rs`, `lib.rs`);
- `crates/seele-ffi` (`volta.rs` novo e mudanças pontuais em `lib.rs`);
- `crates/seele-conformance` (dois arquivos novos);
- `crates/seele-server/tests/par_lento.rs`.

O fio não mudou. O lote não mexeu em `seele-proto`, `ui/` ou `main.rs`. O
caminho da tela é o mesmo de uma queda de rede, então não houve cena nova na
bancada.

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

**O pedido da F4.** O passo `Disconnecting { FellBehind }` em
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
  - Atenção: com a lista da 0.15, `ServerShuttingDown` vira «reconectando»
    por até cinco minutos. A 0.16 o pôs na lista no lote G-servidor. Se o
    servidor 0.15 manda essa razão, ela entra na lista no porte.
  - Atenção também ao `TooLong`: a D (`reenviar_mensagem` conferindo o
    tamanho) é independente, tem duas linhas e deve ir junto.

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

- **F4:** o passo `FellBehind` em `dois_produtores_de_snapshot.rs`, quando o
  `0.16/web` encontrar este ramo. O navegador passa a ver a queda recuperável
  como queda. A busca do buraco e a repetição estão na FFI e não chegam ao
  navegador: lá a queda recuperável fica como uma queda de rede.
- **Pendência 61, achado de passagem:** `Motor::executar` trata **qualquer**
  erro de `frame::write` como enlace caído e chama `cair()`, inclusive o de
  validação do `encode`. O `TooLong` era um caso. Outros comandos com campo
  validado (nome de canal ou de sala, carga de pedido de MOD) podem derrubar o
  enlace pelo mesmo caminho, se a casca não conferir antes. O conserto de fundo
  é separar o erro de validação do erro de transporte. Não medi.
- **A busca só cobre o canal aberto.** O núcleo só reassina o último canal na
  volta, então as não lidas dos outros canais param de andar depois de uma
  queda. Isso já existia antes e é registrado aqui.
- **A cerca pode se perder.** Se o limitador do servidor descartar o `Ping`
  da cerca acima da rajada, a busca para calada naquela volta. A volta gasta
  poucos quadros e a repetição fica abaixo do balde, então não aconteceu na
  medida. Não há prazo que a recomece.
- **F1:** o perdão do `par_lento` saiu. Ver acima, e o commit `2cec3729`.
