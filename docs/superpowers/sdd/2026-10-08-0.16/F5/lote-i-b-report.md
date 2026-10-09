# Lote I-b da F5: relatório

Base `22059e5c` (branch `0.16/lotes`, com o I-a integrado em `6a6f3aa2`).
Branch `0.16/lotes-i-b`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a48919a2551577b4b`. O I-b são
os passos 2 e 3 do lote I (F1-microfone-macos, parte 1, e F1-vazamento) e os
acréscimos A a D de `lote-i-b-acrescimos.md`. Sete commits de código e este
relatório, todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `2296ee5b` | D | `EntradasPendentes::ouvir` (`seele_core::assento`): um `PedidoRecusado` que nomeia `EnterVoiceRoom` desfaz a entrada como as recusas de sala; o de qualquer outro pedido não mexe na entrada no ar |
| `dc46a375` | passo 2 | `SilencioDigital` no laço de voz (`voice.rs`), sobre o quadro cru; `LocalTelemetry::microfone_entrega_silencio_digital` (`seele-audio`); `Telemetry::microfone_entrega_silencio_digital` (FFI); `MICROFONE_EM_SILENCIO_NO_MACOS` em `frases.js` e `fraseDoMicrofoneCalado` na faixa `#aviso-audio` |
| `f0a2a74d` | passo 3 (2) e B | o volume mora na conexão, por apelido (`VozDaSala::volumes`); `ganhos_a_aplicar`; `VozDaSala::seguir_os_volumes` no fim de cada volta de `drive`; `set_volume` grava e aplica, e fora de sala só grava; `Connection::volumes_locais` e o comando `volumes_locais`; o mapa `volumes` da janela sai, e as duas telas leem `volumesLidos` |
| `3722e851` | passo 3 (1) | `esquecerOEstadoDaSessao()` em `tela-sessao.js`, chamada em `ejetar` e em `limparSessaoEncerrada` (`tela-fim.js`); o doc de `rascunhos` |
| `eaba0ea6` | A | `vestir_com_o_disco` e `Connection::vestir_as_escolhas_gravadas`; `main.rs` veste a sessão com o modo, a supressão e a sensibilidade do disco logo depois do `connect` |
| `f161221e` | C | as frases de `MovedByOperator` e `VoiceRoomOverHostUplink` em `AVISOS`, o braço com os números em `fraseDoAviso`, `emKbps`; `AINDA_SEM_FRASE` vazia; a bancada `conversa-com-estado.cjs` ganha a prova das frases e a do esquecimento entre sessões |
| `790dbf3d` | passo 3 (2) | `set_volume` volta a recusar pelo nome antes de recusar por falta de áudio, como antes e como `acceptance_m5` espera |

**O que não mudou.** O fio: `crates/seele-proto` e `crates/seele-server`
intocados; entrar e sair da sala continuam as mensagens de hoje. Nada do
SEELE-ENC/1, dos arquivos da F6, de `api/` (`cargo xtask check-api` verde), da
CSP, nem de `apps/seele-app/src/executor.rs` (restrição da orquestradora durante
o lote). Nenhuma dependência nova, nada baixado. Sem `spawn_blocking`, sem
`unsafe`, sem `tccutil`. Volume nenhum, e nada que identifique a máquina, no
`Snapshot`. `docs/pendencias.md`, o índice da 1.0 e o objeto `ETAPAS` de
`frases.js` intocados. Nenhum `.cjs` novo em `bancada/`: as duas bancadas
mexidas (`telas.cjs`, `conversa-com-estado.cjs`) já existiam e já estão no
`ci.yml`.

As reversões foram feitas trocando o trecho exato por um script
(`troca.py`/`reverte.sh` no scratchpad), rodando o teste, devolvendo o arquivo
de uma cópia e conferindo com `cmp`. Todas voltaram idênticas.

## Passo 2 — F1-microfone-macos, parte (1)

**O detector.** `SilencioDigital` (`crates/seele-core/src/voice.rs`) é uma
função pura sobre o quadro: conta quadros seguidos em que **toda** amostra é
zero exato (`== 0.0`, que vale para +0 e −0), e marca a partir de
`QUADROS_ATE_O_SILENCIO_DIGITAL` = 3000 ms ÷ 20 ms = 150 quadros. Nenhum limiar:
um limiar faria dele um medidor de «baixo», e o piso de um microfone de verdade
nunca é zero. O mudo zera a conta e apaga a marca; sem aparelho de captura não
há quadro, e nada conta.

**No laço.** `pipeline` chama `silencio.observar(&bruto, muted)` logo depois de
tirar o quadro cru, **antes** de `supressao.processar(&bruto, …)` (a supressão
zera de propósito o que julga ruído) e antes do ganho. A marca vai para a
telemetria por `LocalTelemetry::with_silencio_digital`, no mesmo molde de
`with_playout` e `with_pacing`, e de lá para o `Telemetry` da FFI
(`AudioState::silencio_digital` ← `telemetry.local.microfone_entrega_silencio_digital`;
falso sem voz).

**Na tela.** `MICROFONE_EM_SILENCIO_NO_MACOS`, em `frases.js` depois de
`ABERTURA_DO_AUDIO`, é a frase do brief, inteira. `fraseDoMicrofoneCalado(snapshot)`
a devolve só com `snapshot.telemetry.microfone_entrega_silencio_digital === true`
e `$("barra-janela").dataset.plataforma === "macos"` (a plataforma que
`arrumarBarraDaJanela` perguntou ao Rust). `desenharAberturaDoAudio` a usa
quando o estado da abertura não tem frase própria: a do «não abriu no prazo»
vence, porque sem voz não há microfone a acusar.

### Provas

- **(a) o detector**, `voice::silencio_digital` (6 testes): três segundos de
  zero exato marcam e nenhum quadro antes; ruído a −90 dBFS por 6 s não marca;
  uma única amostra `1e-9` no meio recomeça a conta; com o mudo não conta; o
  sinal que volta e o mudo apagam a marca; o prazo é 150 quadros.
  - Vermelho, com um detector-esboço que nunca marca: «três segundos de zero
    exato com o microfone aberto e sem mudo não marcaram: quem tem o microfone
    bloqueado pelo sistema fala para ninguém, e a tela não diz nada».
  - Verde.
- **a ligação ao laço**, `voice::tests::o_silencio_digital_le_o_quadro_cru_e_chega_a_telemetria`
  (lê o fonte de `pipeline`, como o guarda da ordem F01/F02 ao lado).
  - Vermelho antes da ligação: «o laço de voz não conta mais o silêncio
    digital do quadro cru…».
  - Verde.
- **a telemetria**, `telemetry::tests::o_silencio_digital_vem_de_quem_o_mediu`
  (`seele-audio`): falso ao montar, verdadeiro com `with_silencio_digital(true)`.
  O vermelho foi de compilação (o campo não existia).
- **a ponte**, `o_audio_segue_a_sala::o_silencio_digital_chega_na_telemetria_pelo_nome_que_a_tela_le`
  (o JSON do retrato tem `telemetry.microfone_entrega_silencio_digital: false`
  sem voz) e `o_retrato_com_voz_le_o_silencio_digital_da_telemetria_dela` (lê o
  fonte de `audio_state`: o caminho com voz pede uma `Voice`, que pede placa de
  som).
  - Vermelho do primeiro antes do campo: `left: None, right: Some(Bool(false))`.
  - Verde.
- **(b) a tela**, `frontend.rs::o_microfone_em_silencio_digital_e_dito_so_no_macos`:
  a frase existe inteira em `frases.js` (comparada por `literals_in`);
  `fraseDoMicrofoneCalado` passa por `snapshot.telemetry`, pelo campo, pela
  frase e por `dataset.plataforma`/`"macos"`; `desenharAberturaDoAudio` a
  chama; um fecho nomeia o campo em Rust, para um rename de um lado só não
  compilar.
  - Vermelho antes da frase: «`frases.js` não tem a frase do microfone que o
    macOS entrega mudo…».
  - Verde.

## Passo 3 — F1-vazamento

### (2) O volume vai para o Rust (e o acréscimo B)

**Onde mora.** `volumes: Mutex<HashMap<String, u16>>`, por apelido, em
`VozDaSala` — que é o campo `Shared::voice`. O brief diz «`Shared` guarda»; ele
está no `Shared`, dentro do campo da voz, e por uma razão: **a ordem dos
cadeados**. `VozDaSala::seguir_os_volumes` lê os volumes **com a voz presa**
(voz → volumes → `Room`), e `set_volume` grava antes de chamá-la, o mesmo acordo
que `escolher` e `instalar` já seguem para o mudo e os aparelhos: um volume
escolhido enquanto a voz abre é visto por uma das duas chamadas, em qualquer
ordem que as threads andem. Ninguém segura o `Room` enquanto pede a voz (conferi
os dez lugares que travam a voz na FFI).

**A tradução.** `ganhos_a_aplicar(volumes, room) -> Vec<(Ssrc, f32)>`, pura: o
`ssrc` que o apelido tem agora (`Room::ssrc_of`, sem distinguir maiúscula), o
ganho em `percent/100` com teto 400, ordenado pelo número. Quem não está no
`Room` fica guardado para quando estiver.

**Quando ela vale.** `seguir_os_volumes` compara o que `ganhos_a_aplicar` dá
agora com o que já está na voz (`aplicados`) e só toca na voz quando muda — o
mesmo desenho do gatilho da pendência 52: o antes contra o agora, e não a lista
das mensagens que mudam um `ssrc`. `drive` a chama **no fim de cada volta do
laço**, logo depois do olhar da porta. Isso cobre:
- **a abertura** (acréscimo B): `instalar` zera `aplicados` sob o cadeado da
  voz, e a voz nova recebe todos os ganhos no fim da mesma volta;
- **a troca de `ssrc`**: a mensagem que traz o número novo é dobrada no `Room`
  num braço do `select!`, e a volta termina ali — a reconexão inclusive (a voz
  reaberta passa por `instalar`, e os números dos outros chegam de novo pelo
  servidor).

O número que deixa de ser de alguém com volume volta a 1.0: o volume é da
pessoa, e um servidor que reinicia recomeça a contar os números.

**`set_volume`.** Recusa quem não está aqui (`UnknownPerson`), depois a sessão
sem áudio (`NoAudioDevice`), grava e aplica. Fora de sala só grava — antes ele
devolvia `NoAudioDevice` sem voz. **A ordem das duas recusas** foi trocada no
`f0a2a74d` e devolvida no `790dbf3d`: lendo a conformidade sem rodá-la (ela
estava pausada), achei
`acceptance_m5::the_volume_of_a_stranger_is_refused_by_name`, que conecta com
`audio: false` e espera `UnknownPerson`. A ordem errada a deixaria vermelha.

**Até a janela.** `Connection::volumes_locais()` e o comando `volumes_locais`
(registrado no `generate_handler!`), e **nunca** o `Snapshot`, que vai inteiro aos
MODs. O mapa `volumes` da janela sumiu de `tela-sessao.js` e de
`tela-chamada.js`. No lugar dele há `volumesLidos`, que é **a última resposta**
de `volumes_locais`: `lerVolumes()` a troca inteira a cada quadro (da operação,
depois de `desenharPessoas`; da chamada, antes de `desenharChamada`), e
`esquecerOEstadoDaSessao` a esvazia. O gesto escreve nela a posição que acabou
de mandar, para o redesenho seguinte não pular de volta antes de a leitura
confirmar — o problema que o mapa antigo existia para resolver. O comentário de
`tela-chamada.js` sobre o mapa dividido mudou junto.

#### Provas

- **(b)** `o_audio_segue_a_sala::o_volume_volta_quando_o_ssrc_muda`: 150 para
  «rafa» dá `(900, 1.5)`; a Rafa reapresentada com o `ssrc` 901 dá `(901, 1.5)`;
  apelido em maiúsculas e quem não está aqui.
- **(b), o segundo**: `o_laco_aplica_os_volumes_na_abertura_e_na_troca_de_ssrc`
  (com `volta_do_laco`, a volta de `drive` com a voz de mentira: fora de sala só
  grava; a voz abre com 1.5; uma volta sem mudança não toca na voz; o `ssrc`
  novo recebe 1.5 e o velho volta a 1.0) e o guarda de texto
  `os_volumes_sao_seguidos_no_fim_de_cada_volta_de_drive` (o fim da volta de
  `drive` chama `shared.voice.seguir_os_volumes(&shared.room)`).
- **B**, `o_volume_volta_quando_se_sai_da_sala_e_entra_de_novo`: 150 para
  «rafa» dentro da sala, sair, entrar de novo — a voz nova tem 1.5.
- **na conexão**, `o_volume_mora_na_conexao_e_volumes_locais_o_devolve`: uma
  conexão nova não tem volume; fora de sala `set_volume` dá `Ok` e
  `volumes_locais` devolve `{"rafa": 150}`; quem não está aqui é
  `UnknownPerson`, com e sem áudio; a sessão sem áudio é `NoAudioDevice`; e
  nenhuma chave do JSON do retrato contém «volume».
- **a janela**, `frontend.rs::o_volume_chega_a_janela_por_volumes_locais_e_a_janela_nao_guarda_o_dela`:
  nenhum script declara `volumes`; algum invoca `volumes_locais`; `lerVolumes`
  substitui a leitura inteira; a operação e a chamada releem a cada quadro; o
  deslizante do roster e o controle da chamada desenham `volumesLidos`.
- **Vermelhos**, contra esboços que não faziam nada (os cinco da FFI): «o volume
  pedido fora de sala foi recusado…» (`left: Err(NoAudioDevice)`); «o volume de
  150 da Rafa não virou o ganho 1.5…» (`left: []`); «a voz abriu e o volume
  escolhido fora de sala não foi posto nela…» (`left: None`); o B (`left:
  None`); «o fim da volta de `drive` não segue mais os volumes…». Da janela:
  «a janela voltou a guardar o volume de cada pessoa num mapa dela (`const
  volumes =`)…». Da ordem das recusas, no `f161221e`: «numa sessão sem áudio, o
  volume de quem não está aqui deixou de ser recusado pelo nome…» (`left:
  Err(NoAudioDevice), right: Err(UnknownPerson)`).
- **Verdes.** E na bancada `telas.cjs`, com `volumes_locais: { Lia: 150 }` no
  roteiro, uma sonda temporária (devolvida com `cmp`) leu o deslizante da Lia em
  `150`.

### (1) `esquecerOEstadoDaSessao()`

Em `tela-sessao.js`, antes de `ejetar`: esvazia `rascunhos`, `previas`,
`previasFechadas`, `previasDeLink` e `volumesLidos`, e chama `esquecerRetratos()`
e `limparBusca()` (os casamentos da busca, com o cursor e o contador). `ejetar`
a chama no lugar onde chamava só `esquecerRetratos()`; `limparSessaoEncerrada`
(`tela-fim.js`), que o RECONECTAR e a volta à entrada atravessam, a chama antes
de `encerrarBusca`. A reconexão automática da bateria não passa por nenhuma das
duas. O doc de `rascunhos` diz agora que «por sessão» quer dizer que ele acaba
com ela, RECONECTAR inclusive.

#### Provas

- **(a)** `frontend.rs::todo_mapa_de_topo_da_sessao_e_esquecido_ao_sair`: lê
  todo `const|let X = new Map()/new Set()` de topo de `tela-sessao.js` (sete
  hoje: `volumesLidos`, `casamentosPorMensagem`, `previasDeLink`, `rascunhos`,
  `previas`, `previasFechadas`, `retratos`) e exige o `X.clear()` ou a chamada
  nomeada dentro de `esquecerOEstadoDaSessao`; exige a chamada em `ejetar` e em
  `limparSessaoEncerrada`. As exceções entram em `EXCECOES` com o motivo escrito,
  e uma exceção que já não é mapa reprova; a lista está vazia — nenhum mapa de
  topo atravessa sessões de propósito (a marca do fone do lote J não é mapa, e
  o guarda dela continua o dele).
  - Vermelho sem a função; depois, com ela fazendo só o que `ejetar` fazia
    (`esquecerRetratos()`): «`volumesLidos` não é esquecido em
    `esquecerOEstadoDaSessao`: o que ele guardou de um servidor aparece no
    seguinte, sob o mesmo número».
  - Verde.
- **o comportamento**, na bancada `conversa-com-estado.cjs` (no `ci.yml`): pela
  página de verdade, escreve um rascunho no canal 1, põe uma prévia de link e um
  volume lido, sai por `ejetar` e por `limparSessaoEncerrada`, e abre o canal 1
  da sessão seguinte: o campo vem vazio e os dois mapas, vazios.

### Reversões do passo 3

| reversão | reprovou | com |
|---|---|---|
| `seguir_os_volumes` só aplica numa voz nova (`aplicados.is_some()`) | a troca e o B | «a Rafa voltou com outro `ssrc` e o laço não levou o volume dela para o número novo» |
| `instalar` sem zerar `aplicados` | B | «saiu da sala e entrou de novo, e a voz nova abriu com a Rafa a 100 enquanto a janela mostra 150» |
| `drive` sem `seguir_os_volumes` no fim da volta | o guarda de `drive` | «o fim da volta de `drive` não segue mais os volumes…» |
| o número velho sem voltar a 1.0 | a troca | «o número velho da Rafa ficou com o ganho dela…» (`left: Some(1.5), right: Some(1.0)`) |
| `set_volume` antigo (só o ganho do número de agora, sem guardar) | na conexão | «o volume pedido fora de sala foi recusado…» (`Err(NoAudioDevice)`) |
| a ordem das recusas invertida | na conexão | «…deixou de ser recusado pelo nome…» |
| `const volumes = new Map()` de volta na janela | a janela | «a janela voltou a guardar o volume…» |
| cada uma das sete linhas de `esquecerOEstadoDaSessao`, uma por vez | (a) | «`rascunhos`», «`previas`», «`previasFechadas`», «`previasDeLink`», «`volumesLidos`», «`retratos`», «`casamentosPorMensagem`» «não é esquecido…» |
| `ejetar` com `esquecerRetratos()` no lugar | (a) e a bancada | «SAIR DO SERVIDOR não esquece o estado da sessão…»; na bancada: `{"campo":"rascunho do servidor de antes","previas":2,"volumes":1}` |
| `limparSessaoEncerrada` sem a chamada | (a) e a bancada | «o fim de sessão não esquece o estado dela…»; na bancada: `{"campo":"rascunho do servidor de antes","previas":1,"volumes":1}` |

## Acréscimo A — a supressão e a sensibilidade do disco valem no connect

`vestir_com_o_disco(voz, ajustes)` (FFI, genérica na voz) grava em `VozDaSala`
o modo, a supressão (`clamp(0, 1)`) e a sensibilidade (`abertura_na_faixa`, o
mesmo recorte que `set_abertura_da_voz_dbfs` passou a usar) que estiverem
gravados; o que nunca foi escolhido fica no padrão. `Connection::vestir_as_escolhas_gravadas`
a chama na voz da conexão, e `main.rs` a chama logo depois do `connect`, no
lugar do `set_voice_mode` sozinho. O modo continua valendo do mesmo jeito: era
um `escolher` do modo, e continua sendo.

- **Prova**, `o_audio_segue_a_sala::a_supressao_e_a_sensibilidade_gravadas_valem_na_primeira_abertura`:
  preferências gravadas num arquivo temporário (supressão 0.25, sensibilidade
  −48 dBFS, ativação por voz), relidas do disco, `vestir_com_o_disco` depois do
  aperto de mão, entrar na sala: a voz abre vestida com as três.
  - Vermelho com o esboço: «a supressão gravada em disco não valeu na primeira
    abertura: a pessoa a baixou para 25% e a sessão abre com a redução inteira
    (vestiu 1)».
  - Verde.
- **A chamada**, `frontend.rs::a_casca_veste_a_sessao_com_o_disco_logo_depois_do_connect`:
  `connect` chama `connection.vestir_as_escolhas_gravadas(&…)` depois de
  `Connection::connect_watching(`.
  - Vermelho antes de `main.rs`: «`connect` não veste mais a sessão com o
    disco…». Verde.
- **Reversões:** sem a linha da supressão (vermelho, «vestiu 1»); sem a da
  sensibilidade (`left: None, right: Some(-48.0)`); `main.rs` de volta a só o
  modo (o guarda).

## Acréscimo B

Está no passo 3 (2), acima: `o_volume_volta_quando_se_sai_da_sala_e_entra_de_novo`,
e a reversão «`instalar` sem zerar `aplicados`».

## Acréscimo C — os dois avisos que chegavam como «AVISO» seco

O guarda da F1 estava na base, e `AINDA_SEM_FRASE` ficou **vazia**.

- `MovedByOperator`: «ALGUÉM QUE MODERA ESTE SERVIDOR MOVEU VOCÊ PARA OUTRA
  SALA DE VOZ. Você já está nela; para voltar, entre de novo na sala de antes.»
- `VoiceRoomOverHostUplink`, em `AVISOS`: «UMA SALA DE VOZ PASSOU DO QUE A
  INTERNET DESTE SERVIDOR CONSEGUE ENVIAR», e o braço em `fraseDoAviso`, como o
  da retenção, completa com os números em kbps (`emKbps`, em `frases.js`: inteiros,
  ponto dos milhares escrito à mão para não depender da língua do sistema):
  «… Com todos falando juntos, ela pede 6.480 kbps, e a internet do servidor
  envia cerca de 4.000 kbps. Ninguém foi barrado, mas a voz pode picar para
  todos: menos gente falando junto, ou o servidor numa internet que envie mais,
  resolve.» Sem os números, a frase vai sem eles, e não com um buraco.
  «Cerca de» porque a subida é uma estimativa (ADR 0038); o aviso só chega a
  quem administra o servidor, que pode não estar na máquina que hospeda — daí
  «a internet do servidor», e não «daqui».
- As duas numa linha só: `.alerta-frase` não preserva `\n`.

**Provas.**
- `todo_motivo_de_aviso_tem_frase_na_pagina` (F1) com a lista vazia.
  - Vermelho antes das frases: «estes motivos de aviso chegam à faixa sem frase
    escrita, e ela diz só AVISO: ["MovedByOperator", "VoiceRoomOverHostUplink"]».
  - Verde.
- `frontend.rs::o_aviso_da_sala_alem_da_subida_diz_os_dois_numeros_em_kbps`: o
  braço existe e lê `precisa_bps`, `medido_bps` e `emKbps(`; um fecho nomeia os
  dois campos de `seele_ffi::NoticeReason`.
- Na bancada `conversa-com-estado.cjs`, pela `fraseDoAviso` de verdade: as duas
  frases existem; a da subida tem «6.480 kbps» e «4.000 kbps»; sem os números,
  nem `undefined` nem `NaN`.
  - Vermelho antes: «o aviso movido chega à faixa sem frase: null». Verde.
- **Reversões:** sem a frase de `MovedByOperator` (o guarda da F1:
  `["MovedByOperator"]`); sem o braço (o guarda novo, «`fraseDoAviso` não tem o
  braço da sala além da subida…», e a bancada, «a sala além da subida não diz
  os dois números em kbps legíveis…»).

## Acréscimo D — o `PedidoRecusado` na regra do assento

O `PedidoRecusado` estava na base. O que a F1 escreveu sobre o ordinal
(`AlertReason::PedidoRecusado`, em `control.rs`): `pedido` é o ordinal do pedido
em `ClientMessage`, e «com dois pedidos do mesmo tipo em voo, a recusa é do mais
antigo… quem recebe desfaz o mais antigo dos pedidos daquele ordinal que ainda
não teve resposta». É a regra de `recusar`, que já desfaz a frente da fila.
Então:

- **um `PedidoRecusado` que nomeia `EnterVoiceRoom` desfaz a entrada como as
  recusas de sala**: novo braço em `EntradasPendentes::ouvir`, com
  `e_a_entrada(pedido)` = `nome_do_pedido(pedido) == Some("EnterVoiceRoom")` —
  pelo nome, com a função que a F1 deixou «para achar o pedido a desfazer»;
- **o de qualquer outro pedido** (o apelido tomado, o canal que sumiu, a falha
  do servidor) cai no `_` e não mexe na entrada no ar — e não toma o lugar da
  resposta dela na fila.

O servidor da 0.16 não manda `PedidoRecusado` para a entrada: as recusas de
entrada dele continuam `VoiceRoomEntryRefused` e `VoiceRoomFull`, e um erro de
`assentar` sobe com `?`. O braço vale para o que o fio permite e uma 1.x pode
mandar. O M5 da conformidade (`PedidoRecusado { "SetNickname", NomeTomado }`
não tira ninguém da sala) continua com a mesma expectativa.

**Provas**, em `assento::testes`, com o ordinal lido de um quadro de verdade
(`encode` + `ordinal_do_quadro`, como o servidor o lê):
- `a_recusa_de_outro_pedido_com_uma_entrada_no_ar_nao_desfaz_a_entrada`
  (`SetNickname`/`NomeTomado`, `RenameVoiceRoom`/`NaoExiste`,
  `RemoveMessage`/`FalhaDoServidor`; e a recusa de sala que chega depois ainda é
  da entrada). Verde já na base, porque o I-a deixava o `PedidoRecusado` em
  `_ => false`; o vermelho é o da reversão: «a recusa de SetNickname { name:
  "lia" } (NomeTomado) desfez a entrada em B que estava no ar…».
- `a_recusa_que_nomeia_a_entrada_a_desfaz_como_as_recusas_de_sala` (as três
  causas; e sem pedido no ar, ninguém se move).
  - Vermelho na base: «o servidor recusou a entrada em B (NaoExiste) e a tela
    deixou a pessoa em B…» (`left: Some(VoiceRoomId(2)), right:
    Some(VoiceRoomId(1))`).
  - Verde.
- **Reversões:** todo `PedidoRecusado` tomado por recusa de entrada (`|| true`)
  reprova o primeiro; o braço desligado (`&& false`) reprova o segundo, com as
  frases acima.

## O que rodou

No topo (`790dbf3d`):
- `cargo test -p seele-audio --lib`: 254 verdes.
- `cargo test -p seele-core --lib`: 511 verdes (`voice`: 49; `assento`: 18).
- `cargo test -p seele-ffi --lib`: 156 verdes, 1 ignorado, anterior ao lote
  (`audio`: 31; `volume`: 5).
- `cargo test -p seele-app --bin seele-app`: 274 verdes. `--test frontend`: 277
  verdes (272 na base, mais os cinco guardas deste lote).
- `cargo test -p seele-conformance --test voz_na_reconexao --test
  a_recusa_nao_tira_da_sala --test saida_com_despedida --test acceptance_m5
  --no-run`: compila.
- `cargo xtask check-runtime` e `cargo xtask check-api`: verdes.
- Bancadas de navegador (Playwright local): `conversa-com-estado.cjs` (R03, R05,
  R06, C, F1-vazamento), `qualidade-da-voz.cjs`, `tema-sai-com-a-sessao.cjs`,
  `contribuicoes-e-camadas.cjs` e `telas.cjs`: verdes. Os «erros» que `telas.cjs`
  lista (MODs desconectados, `controles_da_voz` sem resposta combinada) são do
  roteiro dela e anteriores ao lote. `node --check` nos scripts mexidos.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-audio -p seele-core -p
  seele-ffi -p seele-app -p seele-conformance --all-targets --all-features -- -D
  warnings`: limpos (o `ld: duplicate -rpath` é o ambiental).

## A conformidade

**Pendente.** A orquestradora pausou a conformidade (a porta 8384 é uma só)
antes de eu chegar a ela; nenhum teste de `seele-conformance` rodou neste lote.
Falta, um arquivo por vez, com `-- --test-threads=1`, sobre o topo:
`voz_na_reconexao`, `a_recusa_nao_tira_da_sala`, `saida_com_despedida`. Pela
leitura, `acceptance_m5` (que mede a recusa do volume de um estranho) também
toca este lote; ela não está na lista que me foi dada, e fica a critério da
orquestradora.

## O que não rodou

- **Windows**: nada compilado nem rodado lá. A frase do microfone calado não
  existe no Windows por construção (o teste de plataforma é da casca), e o
  WebView2 desenhando as faixas e os deslizantes não foi visto.
- **O TCC negado**: não rodei `tccutil` nem desliguei o SEELE nos Ajustes. Que o
  macOS entrega zero exato com o microfone bloqueado é a premissa do brief; que
  o detector o vê no aparelho de verdade, e em quanto tempo a faixa acende, é
  medida do dono. A parte (2) do item (a via AVFoundation) é do dono.
- **Aparelho de verdade**: nenhuma prova abre CoreAudio. O detector, os ganhos e
  a abertura são provados com quadros sintéticos e com a voz de mentira; que o
  ganho de 1.5 se ouve na voz que reabre, e que a interface USB com ganho mínimo
  dá o falso positivo previsto, não foram medidos.

## Para a v6

- `Snapshot.telemetry.microfone_entrega_silencio_digital` é um campo novo no
  retrato, e o retrato inteiro chega aos MODs por `SeeleMods.snapshot()`
  (`base.js`, `case "snapshot"`). É um sim ou não, sem nome de aparelho. A
  projeção do retrato na API 6 é da F3; não mexi nela.
- O volume de cada pessoa **não** está no retrato (um teste da FFI varre as
  chaves do JSON). `volumes_locais` é comando da casca, e não superfície de MOD.

## Preocupações

- **O volume mora em `VozDaSala`**, que é um campo do `Shared`, e não num campo
  solto do `Shared` como o brief escreve. É para a ordem dos cadeados (acima).
- **A janela tem um espelho, `volumesLidos`.** Não é o mapa antigo: ela não é a
  autoridade, a leitura do Rust o substitui a cada quadro, e a sessão que acaba
  o esvazia. Mas é um mapa com o volume de cada um na janela, e o guarda (a) o
  cobra como os outros.
- **Uma janela de um quadro no arrasto**: se a leitura de `volumes_locais` de um
  quadro sair antes do `set_volume` do gesto chegar ao Rust, o deslizante volta
  ao valor anterior por um quadro e se corrige no seguinte. Não medi se a ponte
  do Tauri entrega os dois comandos em ordem.
- **O fim de sessão não zera `mensagens` nem `revisaoDesenhada`** (`tela-fim.js`,
  `limparSessaoEncerrada`), e `ejetar` zera — o comentário de `ejetar` diz por
  quê: guardar a revisão da sessão anterior pode fazer a primeira sincronização
  da seguinte concluir que nada mudou. Não são mapas, não estão no brief e não
  medi se a conversa de um servidor aparece no seguinte pela volta à entrada.
  Fica como achado para quem integra.
- **`.alerta-frase` não preserva `\n`**: o `LastVoiceRoom`, que tem uma segunda
  linha, sai na caixa de alerta como uma linha só. Anterior ao lote; as duas
  frases novas foram escritas numa linha por isso.
- **O detector não vê o bloqueio com o mudo ligado**, por desenho: quem está
  mudo não recebe a faixa, e ela acende três segundos depois de desmutar.
- **A ordem das recusas de `set_volume`** escapou dos meus testes no
  `f0a2a74d` e só apareceu lendo a conformidade. Está consertada e presa por
  teste de unidade, mas é o tipo de coisa que a conformidade pendente confirma.
