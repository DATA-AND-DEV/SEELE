# Lote I-a da F5: relatório

Base `02f1ecad` (branch `0.16/lotes`). Branch `0.16/lotes-i-a`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af70a76820363a837`. O I-a é o
passo 1 do lote I: a D-52 (pendência 52) e o F1-audio-panico. Os passos 2 e 3
(microfone mudo pelo macOS; volume e rascunhos) são o I-b, e nada deles foi
feito aqui. Cinco commits de código e este relatório, todos locais: sem push,
tag nem release.

| commit | item | o que muda |
|---|---|---|
| `e28cb84b` | D-52 (controles) | `carregar_controles` (`voice.rs`) passa a carregar a supressão e a sensibilidade, como já carregava mudo, isolamento, modo, tecla e ganhos |
| `d96fecc0` | D-52 | o áudio abre na sala e fecha fora dela (`PortaDaVoz`, `VozDaSala`), numa thread própria com `PRAZO_DO_AUDIO`; os controles moram em `Shared`; `AberturaDoAudio` no `Snapshot`; `Aviso::Reconectado` traz o assento que o motor refez e a FFI senta a pessoa nele (`readotar_na_volta`) |
| `520f880a` | F1-audio-panico | `instalar_gancho_de_panico()` (`Once`) logo depois do assinante do log em `fn main()`; o guarda de que `main` o instala |
| `2b32795f` | D-52 (tela) | `ABERTURA_DO_AUDIO` em `frases.js`; o CODEC do rodapé, o medidor da configuração e a faixa `#aviso-audio` leem o estado; fora de sala nenhuma das três telas diz «sem áudio» |
| `421d628d` | D-52 (gatilho) | o motor esquece o assento que a volta refaz pelos mesmos caminhos que o `Room` da casca: sala apagada e entrada recusada (`Motor::ouvir_o_assento`) |

**O que não mudou.** O fio: `crates/seele-proto` intocado, entrar e sair da
sala continuam as mensagens de hoje; `Aviso` é tipo interno do `seele-core`,
não fio. Nada no SEELE-ENC/1, nos arquivos da F6, em `api/` (o `cargo xtask
check-api` segue verde), na CSP. Nenhuma dependência nova, nada baixado. Sem
`spawn_blocking`, sem `unsafe`, sem `tccutil`. `docs/pendencias.md` e o índice
da 1.0 intocados. O objeto `ETAPAS` de `frases.js` não foi tocado: o dicionário
novo mora depois de `PERMISSAO_DE_MICROFONE`, a umas trinta linhas dele.
`crates/seele-core/src/state.rs`, `crates/seele-audio`, `tela-chamada.js` e
`tela-fim.js` também ficaram como estavam (ver «A tela», abaixo).

**O que a onda 4 deixou e continua de pé.** O aviso da thread da conexão antes
do `drop(runtime)` e `Connection::encerrar_e_esperar` (lote F): a voz não segura
esse caminho — a abertura em curso é largada antes da despedida, a thread que
abre o áudio é solta (`std::thread`, nunca a piscina de bloqueio) e
`saida_com_despedida.rs` segue verde. `despedir_da_sessao` no `main.rs`, a
`Esperada` da F7 no `enlace.rs` e a faixa de fone do lote J, intocadas.

As reversões foram feitas trocando o trecho exato por um script, rodando o
teste, devolvendo o arquivo de uma cópia no scratchpad e conferindo com `cmp`.
Todas voltaram idênticas, e as do `lib.rs` da FFI e do `enlace.rs` foram
refeitas sobre os arquivos já commitados.

## A medida da reconexão, e o gatilho escolhido

**A medida, antes de escolher.** O primeiro teste escrito foi
`o_audio_segue_a_sala::a_volta_mostra_a_pessoa_na_sala_em_que_o_motor_a_sentou_de_novo`
(FFI), que roda o que o braço `Aviso::Reconectado` de `drive` fazia com o
`Room` — extraído sem mudar nada para `readotar_na_volta`, que então só chamava
`room.adopt` — sobre uma pessoa sentada numa sala com a Lia. Depois da volta, o
servidor reapresenta a Lia, e nunca a própria pessoa (`session.rs` pula
`session.person` na fotografia de ocupação, e confirma a entrada por silêncio).
O vermelho, com o que a tela recebe:

```
depois da volta a FFI tira a pessoa da sala em que o motor a sentou de novo: a
tela a desenha FORA DE SALA enquanto a voz dela está lá, e um gatilho do áudio
lido deste espelho fecharia a voz de quem só caiu e voltou. O que a tela recebe:
[VoiceRoom { id: 1, name: "Sala", ..., occupied_by_us: false, ...,
  people: [Person { id: 9, nickname: "lia", ..., is_self: false }], ... }]
  left: None
 right: Some(VoiceRoomId(1))
```

**O achado se confirmou, e é anterior à 52.** Hoje, depois de uma queda e
volta, quem estava numa sala é desenhado FORA DE SALA na própria tela
(`occupied_by_us: false` em todas, ausente da lista da própria sala), enquanto o
motor já a sentou de novo (`restabelecer(…, self.voice_room, …)`) e a voz dela
está lá. Com o gatilho no espelho da FFI, a 52 ainda fecharia o áudio dela na
primeira volta do laço depois da reconexão.

**O gatilho.** Um só, no fim de cada volta do laço de `drive`: o assento de
agora (`room.current_voice_room`) contra o da volta anterior
(`PortaDaVoz::olhar` → `passo_da_voz`). Ele cobre os cinco caminhos, porque
todos terminam no mesmo campo do `seele-core`: a entrada pedida
(`run_command` → `Room::enter_voice_room`), o operador que move
(`MovedToVoiceRoom`), a saída pedida, a recusa por `Alert`
(`VoiceRoomEntryRefused | VoiceRoomFull`) e a sala apagada (`VoiceRoomDeleted`).
O primeiro olhar é o do aperto de mão, com o servidor adotado e ninguém sentado.

**Para que ele siga o assento que o motor restabelece**, a volta passou a
trazê-lo: `Aviso::Reconectado` ganhou `voice_room: Option<VoiceRoomId>`, o
`self.voice_room` com que `restabelecer` acabou de refazer a entrada, e
`readotar_na_volta(room, sessao, apelido, assento)` adota a sessão e senta a
pessoa nele. O assento antes e depois da volta é o mesmo, e a porta não fecha
nada; a voz reabre na conexão nova por `PortaDaVoz::na_volta`.

**E o motor passou a esquecer o assento pelos mesmos caminhos que o `Room`**
(`421d628d`). Ele só ouvia o `MovedToVoiceRoom`: uma sala apagada ou uma
entrada recusada ficavam guardadas, e a volta refazia a entrada nelas. Com o
assento do motor decidindo onde a casca senta a pessoa — e, pela 52, onde o
áudio abre —, isso abriria o microfone numa sala que não existe, ou que acabou
de dizer não, até a recusa do servidor chegar.

Verde: a pessoa volta sentada, e quem estava fora volta fora
(`a_volta_de_quem_estava_fora_de_sala_continua_fora`).

## D-52: o áudio abre na sala, numa thread própria, com prazo

### O que mudou

`crates/seele-ffi/src/lib.rs`:
- **Nada abre no `connect`.** O bloco `if config.audio { Voice::start_preferring(…) }`
  de `drive` saiu. A única abertura é `abrir_a_voz`, que só a porta chama.
- **`esperar_o_audio(abrir, prazo) -> Result<V, Atraso>`**, genérica: abre numa
  `std::thread::Builder::new().name("seele-abre-audio")`, devolve por
  `tokio::sync::oneshot`, e espera com `tokio::time::timeout(prazo, rx)`.
  Vencido o prazo: `tracing::warn!("o áudio não abriu em {prazo:?}; a sala segue
  sem voz")` (com o prazo de verdade, «10s»). A voz que chega tarde volta no
  `Err` do `send` e cai na própria thread, e o `drop` fecha o aparelho. A
  thread que acaba sem entregar (não subiu, ou pânico) também vira `Atraso`,
  com a linha dela.
- **`PRAZO_DO_AUDIO` = 10 s**, com o doc: cobre o driver são; a primeira
  abertura com o pedido do TCC e a do WASAPI são medidas do dono.
- **`VozDaSala<V>`** (é o campo `Shared::voice`, com um `lock()` que delega, para
  que telemetria, som da tela e malha de bitrate continuem lendo como liam):
  a voz viva, os controles guardados (`ControlesGuardados`: mudo, isolamento,
  modo, supressão, sensibilidade e aparelhos), o estado da abertura e se a
  sessão pediu áudio. A ordem dos cadeados é a voz, depois a escolha.
- **`PortaDaVoz<V>`**, no laço: o assento visto e a abertura em curso
  (`EmCurso`, com os aparelhos que ela pediu). Um braço novo no `select!`,
  `aberta = porta.chegada()`, entrega a voz a `PortaDaVoz::chegou`; sem abertura,
  `chegada` nunca volta. Sair da sala larga a abertura em curso e a voz viva.
  Uma escolha de aparelho feita enquanto a voz abre faz a abertura recomeçar
  com o pedido novo.
- **A reabertura da reconexão** usa a mesma espera: a voz viva sai do lugar
  (sem o cadeado preso enquanto o aparelho abre) e vai inteira para a thread,
  onde `reopen` abre a nova antes de largar a velha.
  `reabrir_voz_na_reconexao` continua, com nova forma (`slot.take().map(reabrir)`),
  e os três testes dela continuam, agora pelo caminho de `drive`
  (`na_volta` + `chegou`). Uma abertura em curso na hora da volta recomeça na
  conexão nova.
- **Os controles moram em `Shared`.** `set_muted`, `set_total_isolation`,
  `set_voice_mode`, `set_supressao_de_ruido` e `set_abertura_da_voz_dbfs`
  gravam e aplicam na voz viva, se há uma (`VozDaSala::escolher`); a voz que
  abre os veste (`VozDaSala::instalar`). `controles_da_voz` devolve os
  guardados sem voz, em vez de `None`. `set_capture_device` e
  `set_playback_device` sem voz só gravam e devolvem `Ok` (antes,
  `NoAudioDevice`, que a casca escreve como «o aparelho sumiu»); com a voz
  viva, trocam como antes e só gravam se a troca deu certo. Numa sessão sem
  áudio (`audio: false`) continuam devolvendo `NoAudioDevice`. `set_volume` não
  mudou: é do I-b.
- **O `Snapshot`** ganha `abertura_do_audio: AberturaDoAudio` (`types.rs`), com
  `ForaDeSala`, `Abrindo`, `Aberto`, `SemAparelho` e `NaoAbriuNoPrazo`
  (`snake_case` no JSON, como `EstadoDoAparelho`). `audio_available` continua,
  com o doc dizendo que fora de sala ele é falso sem faltar nada. Sem voz, o
  retrato mostra o mudo, o isolamento e o modo **escolhidos**, e não os
  padrões de uma voz que não existe. `SemAparelho` cobre também a sessão que
  entrou sem áudio.

`crates/seele-core/src/voice.rs` (`e28cb84b`): `carregar_controles` não levava
a supressão nem a sensibilidade, e cada reabertura e troca de aparelho as
devolvia ao padrão. Com a 52 a reabertura deixa de ser rara.

`crates/seele-core/src/enlace.rs`: o campo `voice_room` de
`Aviso::Reconectado`, e `Motor::ouvir_o_assento` (`421d628d`).

### A casca

`main.rs` continua aplicando `set_voice_mode` logo depois do `connect`, e isso
continua valendo: o modo é guardado e vestido na abertura (provado pela
conexão de verdade em
`fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra`).

### A tela

- `frases.js`: **`ABERTURA_DO_AUDIO`**, por estado, com `rodape` (a palavra do
  CODEC), `medidor` (a linha do medidor em MICROFONE E SOM) e `frase` (a
  faixa). A de `nao_abriu_no_prazo` é a do brief, inteira: «O ÁUDIO NÃO ABRIU A
  TEMPO. A conversa por escrito já funciona; para tentar a voz de novo, saia da
  sala e entre de novo.». Ela não manda escolher o microfone.
- `tela-sessao.js`: o CODEC sem voz sai do estado (fora de sala, travessão);
  `desenharAberturaDoAudio(snapshot)`, chamada por `desenhar`, acende a faixa
  `#aviso-audio` (nova em `index.html`, com as classes do veredito e
  `role="status"`, sem botão de dispensar: é estado, e apaga ao sair da sala);
  `ejetar` a apaga.
- `tela-server.js`: `desenharNivel` sem voz diz `— SEM SESSÃO` sem sessão,
  `— FORA DE SALA` fora de sala (com a nota de sempre), e a linha e a frase do
  estado numa sala.
- `tela-chamada.js` não mudou: ela só desenha a sala em que se está
  (`occupied_by_us`), e fora de sala diz «VOCÊ NÃO ESTÁ EM NENHUMA SALA» sem
  chegar a `pintarCartao`. O guarda (f) prende isso.
- `bancada/telas.cjs`: o retrato de mentira ganha `abertura_do_audio: "aberto"`.

## F1-audio-panico

`apps/seele-app/src/main.rs`: `instalar_gancho_de_panico()`, num `Once`, guarda
o gancho anterior, escreve `tracing::error!(target: "seele_app", thread = nome,
local = %local, "pânico: {mensagem}")` e chama o anterior. Chamado logo depois
do `init()` do assinante em `fn main()`. A linha que o teste captura:

```
ERROR seele_app: pânico: o driver caiu no meio da abertura thread="seele-teste-do-panico" local=apps/seele-app/src/main.rs:12092:21
```

## As provas: vermelho e verde

**(a) Fora de sala, nenhum aparelho aberto.**
`o_audio_segue_a_sala::fora_de_sala_nenhum_aparelho_abre_e_a_sala_abre_e_fecha`
(FFI, voz de mentira e abridor injetado, nenhum CoreAudio): o olhar do aperto
de mão não chama o abridor, a voz fica `None` e o estado `ForaDeSala`; entrar
abre (`Abrindo`, depois `Aberto`); sair fecha. Junto:
`o_audio_abre_e_fecha_pelos_cinco_caminhos_da_sala` (entrada, saída, operador
que move, `VoiceRoomEntryRefused`, `VoiceRoomFull`, `VoiceRoomDeleted`, pelo
`Room` de verdade) e `a_voz_so_abre_pela_porta_da_sala`, que conta no programa
sem comentários uma só chamada a `Voice::start_preferring(`, dentro de
`abrir_a_voz`, e nenhuma a `Voice::start(`/`start_on(`. Os testes da decisão não
veem `drive`; este guarda é o que pega uma abertura escrita de volta no aperto
de mão.
- Vermelho (reversão «abrir no connect», ver abaixo): «a voz abre por fora de
  `abrir_a_voz`: uma abertura no aperto de mão segura o microfone a sessão
  inteira, dentro de sala ou não» (`left: 2`), e «entrar no servidor abriu o
  áudio: quem fica só pela conversa por escrito segura o microfone e o repouso
  da máquina a sessão inteira» (`left: 1`).
- Verde.

**(b) Os controles fora de sala valem ao entrar.**
`os_controles_escolhidos_fora_de_sala_valem_ao_entrar`: mudo, isolamento, modo,
supressão e sensibilidade escolhidos fora de sala, depois a entrada: a voz abre
vestida com os cinco. E `fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra`,
por uma `Connection` montada à mão: nenhum controle é recusado fora de sala,
`set_capture_device`/`set_playback_device` devolvem `Ok`, o retrato mostra o
escolhido, `controles_da_voz` responde.
- Vermelho (reversão): «a voz abriu sem vestir o que a pessoa escolheu fora de
  sala: ela se mutou antes de entrar, o roster a mostra muda, e o microfone está
  aberto»; e «o retrato esqueceu o mudo e o isolamento escolhidos fora de sala:
  o botão volta solto na tela, e a voz abre sem eles».
- Verde.

**(c) A reconexão.** A medida acima, e
`a_volta_de_quem_esta_na_sala_reabre_a_voz_e_a_de_quem_esta_fora_nao_abre`, na
ordem do braço de `drive` (`readotar_na_volta` → `na_volta` → o olhar do fim da
volta): quem estava na sala fica com a voz da conexão nova (`ssrc` novo,
`Aberto`); quem estava fora não abre nada. Junto: os três de
`a_voz_quando_o_enlace_volta` (a voz viva reabre carregando os controles; texto
puro continua texto puro; a que não reabre dá lugar ao texto),
`a_abertura_em_curso_na_volta_recomeca_na_conexao_nova`, e no núcleo
`a_volta_avisa_a_voice_room_em_que_o_motor_sentou_a_pessoa_de_novo` (o aviso
traz `Some(sala)` e `None`) e
`a_voice_room_que_a_volta_refaz_segue_o_que_o_servidor_disse`.
- Vermelho: a medida (acima); «quem caiu e voltou numa sala ficou sem a voz da
  conexão nova: o motor a sentou de novo, e o áudio fechou como se ela tivesse
  saído» (`left: None`); «a volta não avisou a sala que o motor refez nela…»
  (`left: Some(None)`); «a volta refaria a entrada numa sala que foi apagada»
  (`left: Some(VoiceRoomId(2))`, antes do conserto do motor).
- Verde.

**(d) A espera com prazo.**
`a_abertura_do_audio::um_audio_que_nao_abre_no_prazo_volta_atrasado_sem_esperar_o_driver`:
um `abrir` que dorme 2 s, prazo de 200 ms; a asserção de tempo vem antes da do
valor. Junto: `a_voz_que_abre_no_prazo_chega_inteira_e_pela_thread_com_nome`
(a abertura roda em `seele-abre-audio`) e
`a_voz_que_chega_tarde_e_largada_na_thread_que_a_abriu` (o `Drop` da voz tardia
roda).
- Vermelho, com a versão síncrona escrita primeiro: «a espera pelo áudio levou
  2.009787375s com um prazo de 200 ms: ela esperou o driver, e um driver preso
  prende a conexão inteira junto». A bateria não pendurou.
- Verde.

**(e) O pânico.** `gancho_de_panico::um_panico_numa_thread_deixa_linha_no_log`
(binário): chama `rastro_de_teste::segundo_despachante()` (pendência 56),
instala o gancho, e numa `std::thread` com nome faz
`tracing::subscriber::with_default(captura, || panic!(…))`. Depois do `join`,
exige «pânico:», a mensagem e o nome da thread. Junto, em `frontend.rs`,
`o_gancho_de_panico_e_instalado_logo_depois_do_assinante_do_log`.
- Vermelho, com o gancho vazio escrito primeiro: «um pânico numa thread não
  deixou linha no log: quem relata «o áudio não abre» não tem o que mostrar.
  Rastro: ""».
- Verde. O binário inteiro (274) rodou três vezes seguidas, verde: o gancho é
  do processo e não atrapalhou nenhum outro teste.

**(f) A tela.**
`fora_de_sala_a_tela_nao_diz_sem_audio_e_a_frase_de_quando_nao_abriu_sai_do_estado`
(`frontend.rs`): todo estado de `AberturaDoAudio` (lista conferida por `match`
sem coringa, nomes pela serialização de verdade) tem entrada em
`ABERTURA_DO_AUDIO`; a de `fora_de_sala` não diz «SEM ÁUDIO», «SEM SESSÃO DE
ÁUDIO» nem «SEM SOM»; a de `nao_abriu_no_prazo` tem a frase inteira;
`desenharAberturaDoAudio` passa por `snapshot.abertura_do_audio`,
`ABERTURA_DO_AUDIO`, `#aviso-audio` e `.frase`, e `desenhar` a chama; a página
tem a faixa; o rodapé e o medidor leem o estado; `tela-sessao.js`,
`tela-server.js` e `tela-chamada.js` não escrevem a acusação por conta própria;
a chamada só pinta cartões com sala ocupada.
- Vermelho antes da tela: «`const ABERTURA_DO_AUDIO = ` is gone from the screen
  that had it».
- Verde.

**`voz_na_reconexao.rs`** (`nenhuma_casca_reabre_a_voz_jogando_fora_os_controles`):
verde. O braço da reconexão continua chamando `reopen`, e não `start*` nem
`switch_*`.

**Um a mais, no núcleo**: `voice::controles_na_reabertura::a_supressao_e_a_sensibilidade_atravessam_a_reabertura`.
- Vermelho: «a reabertura devolveu a redução de ruído inteira a quem a tinha
  baixado, e a tela de F02 passa a mostrar o padrão como escolha»
  (`left: 1000, right: 250`).
- Verde.

## As reversões

| reversão | o que se desfez | reprovou | com |
|---|---|---|---|
| abrir no connect (literal) | o bloco antigo `if config.audio { Voice::start_preferring(…) }` de volta em `drive`, depois do `remember_media` | `a_voz_so_abre_pela_porta_da_sala` | «a voz abre por fora de `abrir_a_voz`…» (`left: 2`) |
| abrir no connect (decisão) | `passo_da_voz` abrindo sem assento | (a) | «entrar no servidor abriu o áudio…» (`left: 1`) |
| tirar a aplicação dos controles guardados | sem `nova.vestir(&self.escolhida())` em `instalar` | (b) | «a voz abriu sem vestir o que a pessoa escolheu fora de sala…» |
| o mudo só na voz viva | `set_muted` de volta ao `if let Some(voice)` | `fora_de_sala_a_conexao_…` | «o retrato esqueceu o mudo e o isolamento…» |
| voltar ao gatilho do espelho da FFI | `readotar_na_volta` ignorando o assento (só `adopt`) | (c), os dois | «depois da volta a FFI tira a pessoa da sala…» e «quem caiu e voltou numa sala ficou sem a voz da conexão nova…» |
| o aviso sem o assento | `voice_room: None` em `Aviso::Reconectado` | `a_volta_avisa_a_voice_room_…` | «a volta não avisou a sala que o motor refez nela…» |
| o motor só ouve o operador | sem os braços de `VoiceRoomDeleted` e `Alert` em `ouvir_o_assento` | `a_voice_room_que_a_volta_refaz_…` | «a volta refaria a entrada numa sala que foi apagada» |
| a abertura síncrona | `return Ok(abrir())` no começo de `esperar_o_audio` | (d) | «a espera pelo áudio levou 2.00248625s com um prazo de 200 ms…» |
| o gancho sem linha | sem o `tracing::error!` | (e) | «um pânico numa thread não deixou linha no log…» |
| `main` sem o gancho | sem `instalar_gancho_de_panico();` | `o_gancho_de_panico_e_instalado_…` | «`main` não instala o gancho de pânico…» |
| o rodapé por conta própria | `: "SEM ÁUDIO"` de volta no CODEC | (f) | «o CODEC do rodapé não lê o estado da abertura…» |
| a faixa sem chamada | `desenhar` sem `desenharAberturaDoAudio` | (f) | «`desenhar` não chama `desenharAberturaDoAudio`, e a faixa nunca acende» |
| fora de sala acusando | `medidor: "— SEM SESSÃO DE ÁUDIO"` em `fora_de_sala` | (f) | «fora de sala a tela diz «SEM SESSÃO DE ÁUDIO»…» |
| a supressão / a sensibilidade fora da lista | cada uma das duas linhas de `carregar_controles` | o teste do núcleo | as duas frases dele, uma por reversão |

Todas: arquivo devolvido da cópia, `cmp` idêntico.

## O que rodou

- `cargo test -p seele-ffi audio`: 18 verdes. `--lib` inteiro: 141 verdes, 1
  ignorado, anterior ao lote.
- `cargo test -p seele-core voice`: 42 verdes. `--lib` inteiro: 462 verdes.
- `cargo test -p seele-app --bin seele-app panico`: 3 verdes; o binário
  inteiro, 274, três vezes.
- `cargo test -p seele-app --test frontend`: 270 verdes.
- Conformidade, um arquivo por vez, `--test-threads=1`: `voz_na_reconexao` (1)
  e `saida_com_despedida` (3), verdes, depois do último commit de código.
- `cargo xtask check-runtime` e `cargo xtask check-api`: verdes. A bancada de
  navegador `qualidade-da-voz.cjs` (Playwright local): verde. `node --check`
  nos scripts mexidos.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-core -p seele-ffi -p
  seele-conformance -p seele-app --all-targets --all-features -- -D warnings`:
  limpos (o `ld: duplicate -rpath` é o ambiental).

## O que não rodou

- **Windows**: nada compilado nem rodado lá. O código novo não tem `cfg` de
  plataforma, mas a abertura do WASAPI dentro do prazo e o WebView2 desenhando
  a faixa e o medidor não foram vistos.
- **Aparelho de verdade**: nenhuma prova abre CoreAudio. Que a voz abre e fecha
  ao entrar e sair, que o `PreventUserIdleSystemSleep` some fora de sala e que
  10 s bastam ao driver são estão por medir pelo dono.
- **A primeira abertura com o TCC**: se a pessoa demora mais de 10 s para
  responder ao pedido do macOS, a sala fica em `NaoAbriuNoPrazo` e a frase
  manda sair e entrar de novo. Abrir e fechar a cada entrada pode também pedir
  a permissão de novo. Medida do dono.
- A bateria inteira e o resto da conformidade (pela regra do lote). Nenhum
  teste de conformidade conecta pela FFI com `audio: true`, então nenhum deles
  passava pela abertura no `connect` que saiu.

## Para a v6

- `Snapshot.abertura_do_audio` é um campo novo do retrato, e o retrato inteiro
  chega aos MODs por `SeeleMods.snapshot()` (`base.js`, `case "snapshot"`). É
  estado e nada da máquina: cinco valores, sem nome de aparelho nem volume. A
  projeção é da F3; não mexi nela.

## Preocupações

- **A recusa de mudar de sala desencontra o servidor, agora com áudio.** Numa
  sala A, pedir B e ser recusado por permissão, senha ou sala que não existe:
  o servidor mantém a pessoa em A (`session.rs` faz `continue` antes de
  `assentar`), e `Room::apply` a tira de toda sala (`leave_voice_room`).
  Recusado pelo teto, `assentar` já tirou a mídia de A (`leave_everywhere`) e
  mantém a ocupação — outro desencontro, do servidor. Os dois são anteriores a
  este lote, mas pela 52 o áudio fecha nesse instante, com a pessoa ainda
  sentada em A para os outros. O motor agora acompanha o `Room` nisso.
- **Escolher o aparelho numa sala em que o áudio não abriu só grava.** A voz
  não tenta de novo até a pessoa sair e entrar; é o que a frase diz, e o brief
  pede para ela não mandar escolher o microfone. Uma nova tentativa ao escolher
  pediria um comando à porta.
- **A supressão e a sensibilidade do disco não são aplicadas no `connect`**,
  só o modo. É de antes (com a voz sempre de pé, `controles_da_voz` já mostrava
  os padrões da sessão), e continua: a sessão começa nos padrões até a pessoa
  mexer.
- **O primeiro quadro de quem fala logo ao entrar** pode se perder enquanto o
  aparelho abre, e a thread presa no driver vive até o driver soltar: os dois
  custos que a 52 aceitou.
- **Um comentário velho na conformidade**: `reconexao_fantasma.rs` diz que o
  braço de `Aviso::Reconectado` «chama só `room.adopt`». O teste exercita
  `adopt` direto e segue certo; o comentário ficou para quem integra, porque a
  conformidade deste lote é só a dos dois arquivos.
- **`Aviso::Reconectado` ganhou um campo.** É tipo interno do núcleo; os
  padrões da conformidade já usavam `{ .. }` ou `{ sessao, .. }` e compilam.

## Rodada 1

FIX_BASE `fc6d0346`. Brief: `.superpowers/sdd/lotes/lote-i-a-rodada1.md`. Uma
Crítica, dois Importantes e os Menores M-b, M-c, M-e e M-f; o M-d entrou porque
o mecanismo da C1 o deu por uma volta de rede. Cada item foi commitado assim que
ficou provado. Nada no fio nem no servidor: o `Ping` e o `Pong` são os de
sempre, e o servidor devolve o carimbo sem lê-lo.

| commit | item | o que muda |
|---|---|---|
| `a9e8c128` | C1, I1 e M-d | `seele_core::assento` (novo): a regra única, `EntradasPendentes`, `MARCA_DO_PING_DE_ENTRADA` e `assento_resgatado`; o `Room` (`state.rs`) e o motor (`enlace.rs`) passam pela mesma porta, `EntradasPendentes::ouvir`; `Client::send_ping_de_entrada`; o motor manda o `Ping` marcado atrás de cada entrada, guarda o `ssrc` e o assento confirmado da queda, e `restabelecer` refaz a entrada (ou a saída) sobre o assento resgatado; na FFI, `readotar_na_volta` refaz o que o motor refez, e o áudio abre pelo assento confirmado (`sala_do_audio`); a conformidade nova, `a_recusa_nao_tira_da_sala.rs` |
| `ba37e44b` | I2 | o guarda de texto sobre `drive`, `a_porta_da_voz_esta_ligada_ao_laco_de_drive` |
| `432576e2` | M-b | a conferência do aparelho dentro de `VozDaSala::instalar`, sob o cadeado da voz |
| `37cc2c60` | M-c | `VozDaSala::vez_do_driver`: uma abertura por vez no driver, e a abertura abandonada desiste sem abrir |
| `f0fc934f` | M-e | `controles_da_voz` devolve `None` numa sessão sem áudio, e a casca volta ao disco |
| `502eaef3` | M-f | o comentário de `reconexao_fantasma.rs` |

### C1: o mecanismo, e por que este

**A regra**, em `crates/seele-core/src/assento.rs`: uma recusa
(`VoiceRoomEntryRefused | VoiceRoomFull`) desfaz só o pedido de entrada no ar e
devolve o assento de antes dele; sem pedido no ar, não move ninguém. A fila é
FIFO, porque o servidor decide os pedidos na ordem em que os lê. O pedido sai da
fila quando chega o `Pong` do `Ping` que o motor manda logo atrás dele, e a
recusa, quando há, chega antes desse `Pong`. Um pedido que outra coisa
atropelou — um pedido novo, a saída, o operador que move, a sala apagada —
deixa de definir o assento, e a recusa dele não move ninguém. Um segundo pedido
que partia de um recusado passa a partir do que havia antes dele.

**O fim da pendência é o `Pong` de um `Ping` marcado**, com o carimbo `1 << 63`
(`MARCA_DO_PING_DE_ENTRADA`). Medi o caminho antes de escolher:
- o servidor responde o `Ping` na mesma volta em que o lê
  (`session.rs`, `ClientMessage::Ping` → `Pong { timestamp }`), e lê o fluxo de
  controle em ordem, então o que ele diz sobre o pedido sai antes do `Pong`;
- o carimbo dos `Ping` de rotina é um contador que **recomeça em 1** a cada
  `Pong` recebido (`Client::send_ping`), e `pending_ping` guarda só o último.
  Um `Pong` de rotina pode ser de um `Ping` de antes do pedido, e confirmaria a
  entrada cedo demais; o bit alto nunca colide com o contador. O `Ping` marcado
  não mexe na medida do ida-e-volta;
- a confirmação pelo assento anunciado não serve: o servidor não anuncia o
  assento à própria pessoa (`PersonJoined` pula `session.person`);
- uma `Aviso::EntradaConfirmada` vinda do motor também daria certo, mas só para
  quem a repassa: o `Room` dobrado à mão (a conformidade inteira) nunca
  confirmaria. Com o `Pong` marcado, `Room::apply` resolve sozinho.

**A volta.** Um `ssrc` igual na sessão nova prova que o servidor resgatou o
assento (`assento_resgatado`) — *na rodada 2, só com um `SessionId` maior que
o de antes (o N1, abaixo)*. `Room::adopt` (e `apply(Session)`, que passa pelo
mesmo `reabrir`) guarda então o assento confirmado de antes; o motor guarda o
`ssrc` e o confirmado ao soltar a conexão (`Queda`), e `restabelecer` refaz a
entrada sobre o resgatado — no ar, com o `Ping` marcado — ou manda a saída, se
a pessoa deixou a sala durante a queda e o servidor a sentou de novo sem saber.
A FFI espelha: `readotar_na_volta` adota e refaz a entrada, ou a saída.

**M-d.** O áudio abre pelo assento confirmado (`Room::sala_confirmada`, lido por
`sala_do_audio`): o microfone não abre numa sala que ainda pode recusar. Custa
uma volta de rede na entrada; o rodapé mostra o travessão nesse intervalo. Numa
volta sem assento resgatado (`ssrc` novo), o áudio fecha e reabre uma volta de
rede depois. *(Corrigido na rodada 2.)* Isso não é raro: acontece em toda queda
de rede entre ~15 s e ~20 s, em toda troca de rede e em toda volta depois de
5 min. O efeito é pequeno — uma volta de rede sem áudio, e os volumes por pessoa
voltando ao padrão até o I-b (M-a) —, mas é de toda volta dessas.

**Onde o `PedidoRecusado` da F1 vai encaixar:** no braço de `Alert` de
`EntradasPendentes::ouvir`, a única porta das recusas para o `Room` e para o
motor — um `PedidoRecusado` que nomeia outro pedido que não a entrada cai no
`_ => false` e não desfaz nada.

**Por que não o M4.** O motor de verdade só volta depois de três `Ping` sem
resposta (cerca de 15 s) e o servidor só solta a conexão pelo prazo de
ociosidade (20 s); o corte da revisão levou 26 s. O M3 emulado — dois `Enlace`
com a mesma chave, o assento e o `ssrc` resgatados de verdade pelo servidor, e a
entrada refeita sem a senha — cobre a decisão; a do motor (`Queda` e
`restabelecer`) tem o teste
`a_volta_com_o_assento_resgatado_nao_o_perde_para_a_reentrada_recusada`, com o
servidor de teste do `enlace.rs`.

### As provas: vermelho e verde

**A conformidade nova**, `crates/seele-conformance/tests/a_recusa_nao_tira_da_sala.rs`,
com `vaga::minha()` em cada teste. M1 e M5 pela FFI (`audio: false`: numa sala
o estado é `SemAparelho`, fora dela `ForaDeSala`, e é ele que diz onde o
gatilho do áudio acha que a pessoa está); o M3 emulado; o de controle pela FFI.
Rodada no `fc6d0346`, antes do conserto:
- **M1** (`quem_e_recusado_noutra_sala_continua_na_sua_e_com_o_audio`): «M1: o
  servidor manteve a pessoa em VoiceRoomId(1) e a tela dela a tirou da sala:
  todos a veem lá, e ela se vê fora» (`left: None, right: Some(1)`);
- **M3** (`a_volta_numa_sala_com_senha_nao_tira_quem_o_servidor_resgatou`): «M3:
  a pessoa caiu e voltou numa sala com senha, o servidor a sentou de novo, e a
  recusa da entrada refeita a tirou da sala na própria tela» (`left: None`);
- **M5** (`um_apelido_tomado_nao_tira_ninguem_da_sala`): a mesma frase do M1;
- **o de controle** (`quem_esta_fora_e_e_recusado_continua_fora`): verde antes e
  depois, e é o papel dele — prova que a regra nova não desmontou o conserto da
  recusa de quem estava fora (`senha_da_sala.rs`). Com o M-d o áudio dele não
  chega a abrir; a prova disso é do teste da FFI, abaixo, porque a janela entre
  a entrada e a recusa é de menos de um milissegundo no laço local.

Nos três vermelhos, as asserções de sanidade passaram antes: o servidor tinha a
pessoa em A. Depois do conserto, os quatro verdes, três vezes seguidas. A
asserção do M3 sobre `sala_confirmada` entrou depois do vermelho, porque o
método não existia no `fc6d0346`.

**Os testes de crate**, escritos antes de cada conserto:
- `assento::testes` (9): a regra sozinha — a troca recusada volta à de antes,
  a recusa sem pedido não move, o `Pong` de rotina não confirma, dois pedidos no
  ar, a saída e o operador atropelando o pedido, a sala apagada, o `ssrc`;
- `state::tests`: `uma_recusa_sem_pedido_de_entrada_no_ar_nao_tira_da_sala` (M5),
  `a_troca_de_sala_recusada_devolve_a_sala_de_antes` (M1),
  `a_volta_com_o_assento_resgatado_continua_na_sala_mesmo_com_a_reentrada_recusada`
  (M3), `a_volta_com_ssrc_novo_comeca_fora_de_sala`;
- **I1**, `enlace::tests::uma_recusa_esquece_so_o_pedido_e_nao_a_sala_que_a_volta_refaz`.
  Vermelho no motor de `421d628d`: «uma recusa sem pedido de entrada no ar fez a
  volta esquecer a sala em que o servidor manteve a pessoa» (`left: None, right:
  Some(VoiceRoomId(2))`). O teste de `421d628d`
  (`a_voice_room_que_a_volta_refaz_segue_o_que_o_servidor_disse`) continua, agora
  com o `Pong` de cada pedido, como o servidor manda;
- na FFI, o teste dos cinco caminhos passa a mandar a resposta do servidor a
  cada pedido, e ganha as três recusas que não tiram ninguém da sala; o (a)
  ganha o M-d: «o áudio abriu com a entrada ainda no ar» se o áudio abrir antes
  do `Pong`; o (c) ganha o M3 na porta da voz.

### As reversões

| reversão | o que se desfez | reprovou |
|---|---|---|
| a regra da C1 | o braço de `Alert` de `EntradasPendentes::ouvir` de volta a «toda recusa esvazia o assento» | 11 em `seele-core` (assento, `state`, os dois do motor), 2 em `seele-ffi` (os cinco caminhos e o (c)), e M1, M3 e M5 na conformidade |
| o assento resgatado | `assento_resgatado` devolvendo sempre `None` | `o_ssrc_igual_e_o_assento_resgatado`, o M3 do `Room` e o do motor, e o (c) da FFI |
| I1 | o braço de `Alert` do motor de volta a `self.voice_room = None` | `uma_recusa_esquece_so_o_pedido_…` e `a_volta_com_o_assento_resgatado_…` |
| o `Ping` marcado | o motor sem `send_ping_de_entrada` depois da entrada | a conformidade: M1 e M5 em «a entrada em VoiceRoomId(1) não assentou: … abertura ForaDeSala» (nenhuma entrada se confirma), e o M3 |
| M-d | `sala_do_audio` lendo `current_voice_room` | (a): «o áudio abriu com a entrada ainda no ar: o microfone abre numa sala que ainda pode recusar (M-d)» |
| R7 | sem o olhar do fim da volta do laço | I2: «o olhar do fim da volta do laço sumiu: é o único ponto que abre o áudio ao entrar numa sala e o fecha ao sair» |
| R8 | `porta.comecar(…)` depois do olhar do aperto de mão | I2: «antes do laço, `drive` mexe na porta da voz mais de uma vez — ou nenhuma…» |
| M-b | sem a conferência dentro de `instalar` | `instalar_recusa_a_voz_aberta_com_os_aparelhos_de_antes` e `o_aparelho_escolhido_enquanto_a_voz_abre_nao_se_perde` |
| M-c, a vez | a thread sem tomar a vez do driver | `duas_aberturas_da_mesma_sessao_nunca_ficam_no_driver_ao_mesmo_tempo` («… ficaram no driver ao mesmo tempo», `left: 2`) e `a_abertura_que_ja_desistiu_…` |
| M-c, a desistência | a thread abrindo mesmo sem ninguém esperando | `a_abertura_que_ja_desistiu_nao_abre_quando_chega_a_vez` («… abriu o aparelho quando chegou a vez») |
| M-e | `controles_da_voz` sem o `None` da sessão sem áudio | `a_sessao_sem_audio_deixa_os_controles_da_voz_para_o_disco` |

Todas: arquivo devolvido da cópia, `cmp` idêntico, sobre os arquivos já
formatados.

### Os Menores

- **M-b.** A conferência do aparelho mora em `VozDaSala::instalar`, sob o
  cadeado da voz, e é o mesmo cadeado que `switch_device` segura ao gravar a
  escolha. A corrida em si não tem como ser forçada num teste sem uma costura
  no meio; o teste barato prende que a conferência está em `instalar`.
- **M-c.** `VozDaSala::vez_do_driver`: a thread de `esperar_o_audio` toma a vez
  antes de abrir, e a espera conta no prazo de quem pediu. Uma abertura de que
  ninguém espera mais (o prazo venceu, ou a pessoa saiu) desiste sem abrir
  quando a vez chega. O custo: com o driver preso, cada nova entrada deixa uma
  thread parada na vez até ele soltar.
- **M-e.** `controles_da_voz` devolve `None` com `audio: false`, e
  `controles_da_voz` do `main.rs` volta a cair no disco.
- **M-f.** O comentário de `reconexao_fantasma.rs` diz que o braço da volta
  adota a sessão e refaz o pedido do motor.

### O que rodou

- `seele-core --lib` 477, `seele-ffi --lib` 146 (1 ignorado, anterior),
  `seele-app --bin seele-app` 274, `--test frontend` 270.
- Conformidade, um arquivo por vez, `--test-threads=1`: `a_recusa_nao_tira_da_sala`
  (4, e mais três vezes), `voz_na_reconexao` (1), `saida_com_despedida` (3),
  `reconexao_fantasma` (1).
- `cargo fmt --all -- --check` e o clippy de `seele-core`, `seele-ffi`,
  `seele-conformance` e `seele-app` com `-D warnings`: limpos.

### O que não rodou, e as preocupações

- **O resto da conformidade.** O motor manda agora um `Ping` a mais por entrada
  numa sala (e um na volta), e o `Room` trata o `Pong` dele. Os testes que
  li — `senha_da_sala.rs`, `teto_e_permissao_da_sala.rs`, `reconexao_fantasma.rs`
  — não contam mensagens nem dependem de uma recusa sem pedido; os demais
  (`bateria_interna`, `ocupacao`, `moderacao`, `desassentar`, …) rodam na
  integração.
*Corrigida na rodada 2: só os dois primeiros abaixo são raros, porque só eles
pedem que duas coisas caiam dentro da mesma volta de rede de um pedido.*

- **Raro — uma recusa que não é de entrada, chegando com uma entrada no ar,** é
  tomada pela entrada (o apelido tomado no mesmo instante em que se pede uma
  sala). É o encaixe do `PedidoRecusado` da F1, acima. *Desde o merge da
  rodada 2 (`9f87889c`), deixou de existir: ver «O merge do `0.16/lotes`».*
- **Raro — o operador que move a pessoa no instante em que ela pede outra
  sala**: a mudança do operador vale, e a resposta ao pedido é descartada. Se o
  servidor leu o pedido depois da mudança e o aceitou, a tela fica na sala do
  operador e o servidor, na pedida.
- **A sala de antes apagada com um pedido no ar** *(achado e consertado na
  rodada 2, `4f737cc6`)*: em A, pedir B; o servidor apaga A antes de ler o
  pedido e recusa B, e a recusa devolvia a pessoa a A, que já não existe.
- **A volta depois de sair e entrar noutra sala durante a queda**, com a
  reentrada recusada: o `Room` cai fora de sala e o motor volta ao assento
  resgatado. O `Room` confere o resgate com o confirmado dele na hora da volta,
  e uma saída durante a queda o zera; o motor confere com o da hora da queda.
  Não depende de uma volta de rede: a janela é a queda inteira, e basta a
  pessoa trocar de sala enquanto o enlace tenta voltar e a sala nova recusar.
- **Um servidor reiniciado** recomeça a contar os `ssrc` em 1, e uma sessão nova
  pode ganhar o mesmo número da antiga: seria tomada por resgatada. *Não era
  raro: é o N1 da rodada 2, consertado lá.*


## Rodada 2

FIX_BASE `8c62d559`. Brief: `.superpowers/sdd/lotes/lote-i-a-rodada2.md`. Os dois
achados que a rodada 1 abriu — N1 (Importante) e N2 (Menor) — e a palavra
«raro». Nada no fio nem no servidor: o carimbo do `Ping` marcado continua um
`u64` que o servidor devolve sem ler, e o `SessionId` já vinha na sessão.

| commit | item | o que muda |
|---|---|---|
| `d02743ea` | N1 | `assento_resgatado(antes: Option<Conexao>, agora: Conexao, confirmada)`, com `Conexao = (Ssrc, SessionId)`: resgate é o mesmo `ssrc` **e** um `SessionId` maior; o `Room` guarda o `SessionId` ao lado do `ssrc` e `reabrir` recebe a conexão inteira; o motor guarda o `SessionId` na `Queda`; o M6 em `a_recusa_nao_tira_da_sala.rs`, com `servidor_em(porta, banco)` |
| `893632ab` | N2 | `carimbo_da_entrada(n)` (bit alto mais `n`) e `numero_do_carimbo`; `EntradasPendentes::pedir` numera os pedidos da conexão e devolve o número, e `esquecer` recomeça do 1; `Client::send_ping_de_entrada(numero)`; o `Pong` de número `n` tira da fila o pedido `n` e todos os mais velhos |
| `4f737cc6` | a sala de antes apagada | no braço de `VoiceRoomDeleted` de `EntradasPendentes::ouvir`, o `antes` de um pedido no ar que aponta para a sala apagada passa a `None`, menos quando a sala apagada é ela mesma um pedido ainda no ar (autorizado pela coordenação durante a espera da conformidade) |

### N1: o resgate pede também uma sessão maior

**A regra**, em `assento_resgatado`: conta como resgatado só o mesmo `ssrc` numa
sessão de número **maior** que a de antes. O comentário junto dela aponta as
linhas do servidor em `8c62d559` (`seele-server/src/session.rs`):
- `Registry::new` (202–208) começa os dois contadores em 1, e `Registry::issue`
  (221–226) avança os dois juntos, uma vez por sessão;
- o resgate (1188–1200) põe o `ssrc` guardado no lugar do recém-emitido e fica
  com o `SessionId` recém-emitido.

Daí os dois lados. Num resgate, o servidor é o mesmo processo, e o contador de
sessões nunca volta: a sessão nova é sempre maior. Depois de um reinício, a
sessão nova sai do mesmo `issue` que o `ssrc`, então um `ssrc` repetido `s` vem
com a sessão `s`; antes da queda a pessoa tinha a sessão `k ≥ s` (`k = s` sem
resgate nenhum, `k > s` depois de um). A sessão nova nunca é maior, com uma
pessoa ou com muitas. O comentário diz que isto se apoia em como o servidor
numera, e não no que o fio promete, e que dizer o resgate no fio seria da F1.

**As provas.**
- **Vermelho em `8c62d559`:**
  `state::tests::um_ssrc_repetido_depois_de_um_reinicio_do_servidor_nao_e_resgate`.
  Ele aplica uma `Session` nova com o mesmo `ssrc` e o mesmo número de sessão e
  reprovou com «um `ssrc` repetido depois de um reinício do servidor foi tomado
  por assento resgatado: a pessoa se vê na sala e o servidor não a tem em sala
  nenhuma» (`left: Some(VoiceRoomId(1)), right: None`). Os outros dois testes
  da regra não compilam em `8c62d559`, porque a assinatura mudou, e o vermelho
  deles é o da reversão:
  - `assento::testes::o_ssrc_igual_com_sessao_igual_ou_menor_e_um_servidor_que_subiu_de_novo`;
  - o caso `(Ssrc(1), SessionId(1))` de
    `enlace::tests::a_volta_com_o_assento_resgatado_nao_o_perde_para_a_reentrada_recusada`.
- **Os testes de resgate de antes** passam a dar uma sessão maior, como o
  servidor dá: `sessao_resgatada()` no `state.rs`, `sessao_numero(700, 701)` no
  (c) da FFI, e a `Queda` com a sessão 0 no motor.
- **A conformidade M6:**
  `depois_de_um_reinicio_do_servidor_a_reentrada_recusada_deixa_fora_de_sala`.
  Ela usa um banco em arquivo temporário e uma sala com senha. P entra com a
  senha. Então o `Daemon` é derrubado (`shutdown`, `wait_idle`, `drop`) e outro
  sobe na mesma porta com o mesmo banco. O teste espera a recusa da reentrada e
  confere três coisas: o servidor sem a pessoa em sala, a tela fora de sala e a
  abertura `ForaDeSala`. **Coube estável:**
  - **vermelho com `crates/seele-core` e `crates/seele-ffi` de `8c62d559`** e o
    arquivo de teste novo: «M6: o servidor subiu de novo sem assento nenhum,
    e um `ssrc` repetido foi tomado por resgate: a pessoa se vê na sala e o
    servidor não a tem em sala nenhuma» (`left: Some(1), right: None`). As
    duas asserções de sanidade passaram antes: a recusa chegou e o servidor
    não tinha a pessoa em sala. Os outros quatro do arquivo, verdes;
  - **verde no topo**, antes e depois do merge, cinco vezes ao todo (três
    seguidas), com o arquivo inteiro em ~6,3 s;
  - **vermelho com a condição do `SessionId` revertida**, sobre o estado
    mesclado, com a mesma frase e os mesmos valores; o arquivo voltou com
    `cmp` idêntico.

  A dúvida do `shutdown` ficou respondida: o `Daemon` cai sem uma despedida
  que segure a FFI, e ela volta sozinha para o servidor novo na mesma porta.
  Não precisei de outra forma de queda.

### N2: o `Ping` marcado leva o número do pedido

**O número.** `EntradasPendentes` numera os pedidos de entrada da conexão a
partir de 1, e `pedir` devolve o número. O motor manda o `Ping` marcado com
`carimbo_da_entrada(n)`, que é o bit alto mais `n`. O bit alto continua o que
separa o carimbo dos `Ping` de rotina (o contador deles recomeça em 1 a cada
`Pong`).

**Por que «e todos os mais velhos».** O servidor lê o fluxo de controle em
ordem. Ao devolver `n`, ele já decidiu tudo o que veio antes, e um pedido cujo
`Ping` o limitador descartou não terá resposta nenhuma.

**O `Room` e o motor contam cada um os seus pedidos,** um a um e na mesma ordem.
A FFI só chama `Room::enter_voice_room` depois de mandar o comando ao motor. Na
volta os dois recomeçam do 1: `reabrir` no `Room`, a fila nova de `tentar` no
motor. Se um dia contassem diferente, um pedido ficaria no ar no `Room` e o
áudio não abriria. É o que o M1 e o M5 da conformidade pegam, porque a
asserção deles é a abertura `SemAparelho` na sala confirmada.

**As provas.**
- **Vermelho em `8c62d559`:**
  `assento::testes::um_ping_marcado_perdido_no_limitador_nao_desalinha_a_fila`,
  com a sequência da re-revisão:
  1. o `Ping` de A se perde;
  2. a pessoa sai e pede B;
  3. chega o `Pong` de B;
  4. vem uma recusa sem pedido.

  Reprovou com «o `Pong` de B tirou da fila o pedido velho de A, e B ficou no ar
  para sempre: o áudio nunca abre em B» (`left: None, right:
  Some(VoiceRoomId(2))`). Em `8c62d559` o carimbo era um só, e o `pong_do_pedido`
  do teste devolvia a marca sozinha; o verde é com o número.
- **Os auxiliares que confirmam «tudo o que está no ar»** (`entrada_decidida`
  no `state.rs` e no motor, `decidida` na FFI) passam a mandar o maior número
  que um teste usa: `carimbo_da_entrada(u64::from(u32::MAX))`.

**O que sobra.** Depois de um `Ping` marcado descartado, uma recusa que chegue
antes do próximo `Pong` marcado é tomada pelo pedido velho da frente. Ele foi
atropelado, então a recusa não move ninguém, e o pedido recusado fica na tela
até o `Pong` dele o fechar como aceito. Só acontece com o limitador cortando:
mais de 60 quadros de controle de uma vez e depois mais de 20 por segundo. O
limitador avisa uma vez quando começa a cortar, e esse aviso não chega à regra.

### A sala de antes apagada com um pedido no ar

Achado ao corrigir a palavra «raro», e consertado com a autorização da
coordenação durante a espera da conformidade (`4f737cc6`).

**O caso.** A pessoa está em A e pede B, e o servidor apaga A antes de ler o
pedido. O braço de `Event::VoiceRoomDeleted` de `session.rs` tira a pessoa de A
ao apagar, e depois o servidor recusa B. O `VoiceRoomDeleted { A }` chegava com
o assento especulativo em B e não mexia em nada. A recusa de B devolvia a
pessoa a A, que já não existe, e o áudio abria lá.

**O conserto**, no braço de `VoiceRoomDeleted` de `EntradasPendentes::ouvir`:
o `antes` de um pedido no ar que aponta para a sala apagada passa a `None`, e a
recusa deixa a pessoa fora de sala. Se o servidor leu o pedido antes de
apagar, a recusa chega antes do aviso e devolve a pessoa a A ainda de pé, e o
aviso a tira de lá, como antes.

**Com uma exceção.** Se a sala apagada é ela mesma um pedido ainda no ar,
zerar o `antes` do pedido seguinte quebrava a corrente de `recusar`. Exemplo:
em A, pedir B e logo C; o servidor apaga B antes de ler o pedido de B, recusa B
(a sala não existe) e recusa C. A pessoa nunca saiu de A, e quem devolve o
`antes` de C para A é a recusa de B. A forma que a coordenação autorizou,
aplicada sem a exceção, deixava essa pessoa fora de sala. Pus a exceção e um
teste para ela.

**As provas.**
- **Vermelho em `893632ab`:**
  `assento::testes::a_recusa_nao_devolve_a_sala_de_antes_que_foi_apagada_com_o_pedido_no_ar`.
  Reprovou com «a recusa de B devolveu a pessoa a A, que o servidor apagou
  antes de ler o pedido: a tela a mostra numa sala que não existe» (`left:
  Some(VoiceRoomId(1)), right: None`).
- **O guarda da exceção:**
  `assento::testes::a_sala_apagada_que_ainda_e_um_pedido_no_ar_fica_para_a_resposta_dele`.
  Ele é verde em `893632ab`, porque lá nada zerava o `antes`. Reprovou contra
  a forma sem a exceção, com «o servidor recusou B e C e manteve a pessoa em
  A, e a tela a tirou de toda sala» (`left: None, right: Some(VoiceRoomId(1))`).

**Dois casos da mesma família que ficam.** Achei-os lendo o servidor para
este conserto; não os medi nem consertei, porque estão fora do autorizado.
Os dois são raros, da mesma janela de uma volta de rede, e pedem o operador
apagando uma sala no meio dela:
- **A sala pedida apagada antes de o servidor ler o pedido.** Em A, pedir B.
  O servidor apaga B, recusa B (a sala não existe) e mantém a pessoa em A. O
  `VoiceRoomDeleted { B }` chega com o assento especulativo em B; o braço o
  esvazia e atropela o pedido, e a recusa de um pedido atropelado não devolve
  ninguém. A tela fica fora de sala, e o servidor a tem em A.
- **Dois pedidos, o do meio aceito e apagado entre o pedido e o `Ping` dele.**
  Em A, pedir B e C. O servidor aceita B, apaga B antes de ler o `Ping` marcado
  de B (e tira a pessoa dela), e recusa C. O aviso chega com B ainda no ar, a
  exceção acima deixa o `antes` de C em B, e a recusa de C devolve a pessoa a
  B, que não existe. Se o `Pong` de B chega antes do aviso, o conserto já
  cobre. É o comportamento de `893632ab`, sem piora.

Os dois pedem o mesmo conserto: marcar o pedido no ar cuja sala foi apagada e
decidir na resposta dele. Uma recusa devolve o de antes; um `Pong` sem recusa
deixa fora de sala quem dependia daquela sala. **Por decisão da coordenação,
os dois ficam documentados aqui e vão para o Z.**

### As reversões

| reversão | o que se desfez | reprovou |
|---|---|---|
| N1 | `sessao_agora > sessao_antes` → `(sessao_agora > sessao_antes \|\| true)` | 3 em `seele-core`: `o_ssrc_igual_com_sessao_igual_ou_menor_…` («um `ssrc` repetido numa sessão SessionId(2) — o servidor subiu de novo — foi tomado por resgate»), o caso `(Ssrc(1), SessionId(1))` do motor («… a recusa da entrada refeita deixou a volta em Some(VoiceRoomId(5))») e `um_ssrc_repetido_depois_de_um_reinicio_…` |
| N2 | o laço do `Pong` → um `pop_front` por `Pong`, como em `8c62d559` | `um_ping_marcado_perdido_no_limitador_nao_desalinha_a_fila`, com a frase do vermelho |
| a sala de antes apagada | sem o laço que zera o `antes` (o braço de `893632ab`) | `a_recusa_nao_devolve_a_sala_de_antes_…`, com a frase do vermelho; o guarda da exceção passa |
| a exceção | `&& !pedida_no_ar` → `&& (!pedida_no_ar \|\| true)` | `a_sala_apagada_que_ainda_e_um_pedido_no_ar_…`, com a frase acima; o outro passa |

Nas quatro: arquivo devolvido da cópia, `cmp` idêntico. A reversão do N1 na
conformidade reprovou o M6, como está na seção do N1.

### O relatório: a palavra «raro»

Corrigi a rodada 1 no lugar:
- **O M-d** passa a dizer que a volta sem assento resgatado, que fecha e reabre
  o áudio, é de toda queda entre ~15 s e ~20 s, de toda troca de rede e de toda
  volta depois de 5 min. O efeito é uma volta de rede sem áudio e os volumes
  por pessoa no padrão até o I-b.
- **O servidor reiniciado** deixa de ser «raro»: era o N1.
- **Ficam como raros só os dois que pedem duas coisas na mesma volta de rede de
  um pedido:** a recusa que não é de entrada e o operador que move. A sala de
  antes apagada com um pedido no ar, que achei nesta correção, saiu da lista:
  está consertada (acima). Os dois casos da mesma família que ficam estão na
  seção dela.
- **A volta com troca de sala durante a queda** ganhou a frase do tamanho da
  janela: é a queda inteira, e não uma volta de rede.

### O merge do `0.16/lotes`

A coordenação pediu o merge de `17db18d7` antes da conformidade, para ela
rodar uma vez só, sobre o estado final. Ele traz a sexta integração: da F1, o
`AlertReason::PedidoRecusado { pedido, causa }` no lugar do
`VoiceRoomEntryRefused` do `nao_deu` e o guarda
`todo_motivo_de_aviso_tem_frase_na_pagina`; da F3, o tema no servidor; da F7,
a sala. Merge em `9f87889c`, com dois conflitos, os dois resolvidos com os
dois lados:
- **`crates/seele-ffi/src/lib.rs`:** a lista do `pub use types::{…}` ganhou o
  `AberturaDoAudio` do I-a e o `FalhaAoLocalizar` da F7;
- **`apps/seele-app/tests/frontend.rs`:** os dois lados acrescentaram testes
  no fim do arquivo. Ficam os do I-a e, depois deles, os da F1.

**A semântica do encontro:**
- **O braço de `Alert` de `EntradasPendentes::ouvir`** compila com a variante
  nova e a deixa cair em `_ => false`: o `PedidoRecusado` não move ninguém. O
  uso do `pedido` fica para o I-b.
- **O M5 mudou no merge.** Ele esperava a recusa do apelido como
  `VoiceRoomEntryRefused`, que o `nao_deu` deixou de mandar. Sem a mudança, a
  primeira asserção reprovaria com «a recusa não chegou, e o teste mediu
  outra coisa». Agora `continua_em` recebe a recusa que espera, e o M5 espera
  `PedidoRecusado { pedido: "SetNickname", causa: NomeTomado }`. Ele
  continua verde e continua medindo a mesma coisa: uma recusa que não é de
  entrada não tira ninguém da sala.
- **Com o `PedidoRecusado`, o primeiro raro da rodada 1 deixa de existir.**
  Era a recusa que não é de entrada, tomada pela entrada. Depois do merge, as
  quatro vezes em que o servidor manda `VoiceRoomEntryRefused` ou
  `VoiceRoomFull` estão todas no braço de `ClientMessage::EnterVoiceRoom` de
  `session.rs`. Dos raros da rodada 1 fica só o operador que move.

### O que rodou

- Antes do merge: `seele-core --lib` 482 (480 depois do N2, e os dois da
  sala apagada), `seele-ffi --lib` 146 (1 ignorado, anterior), `seele-app
  --bin seele-app` 274, `--test frontend` 270.
- Sobre o merge: `seele-core --lib` 502, `seele-ffi --lib` 148 (1 ignorado,
  anterior), `seele-app --bin seele-app` 274, `--test frontend` 272.
- `cargo fmt --all -- --check` e o clippy de `seele-core`, `seele-ffi`,
  `seele-conformance` e `seele-app` com `-D warnings`: limpos.
- `cargo test -p seele-conformance --test a_recusa_nao_tira_da_sala --no-run`:
  compila.

### A conformidade

Sobre o estado mesclado, um arquivo por vez, com `-- --test-threads=1`:
- `a_recusa_nao_tira_da_sala`: 5 verdes, quatro vezes (uma antes do merge, e
  três seguidas depois), e as duas rodadas vermelhas do M6 acima;
- `voz_na_reconexao`: 1 verde;
- `reconexao_fantasma`: 1 verde;
- `limite_de_taxa`: 2 verdes;
- `senha_da_sala`: 2 verdes. A coordenação o pediu porque o `PedidoRecusado`
  mexe nas recusas.
