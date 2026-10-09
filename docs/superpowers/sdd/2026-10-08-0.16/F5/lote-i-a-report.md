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
