# Lote V da F5: relatório

Base `9599bec5` (branch `0.16/lotes`). Branch `0.16/lotes-v`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-adcf8156f65fe1176`, com o
`target/debug` semeado do worktree de controle. Tudo local: sem push, tag nem
release.

| commit | item | o que muda |
|---|---|---|
| `9f18fb5f` | 1 (regressão do I-a) | a tecla de falar mora em `ControlesGuardados`; `set_talking` grava sempre e aplica na voz viva; a voz que entra no lugar a veste por cima da que `reopen` carregou; um guarda confere que a voz de verdade veste cada controle guardado |
| `00271fe6` | 2 | a voz reaberta da volta entra no lugar com os `aplicados` da velha; só a voz aberta do zero os zera |
| `bac92279` | M-1 | a subida do silêncio digital vai ao `seele.log`, uma vez por subida |
| `321d8b36` | M-2 | o volume e o mudo do som da tela moram em `ControlesGuardados`, são gravados sem voz viva, vestidos ao abrir e levados por `carregar_controles` na troca de aparelho |
| `bcfa2706` | M-4 | o fim de `drive` larga a voz antes da despedida sem esperar o cadeado dela; o comentário que dizia «nada» é corrigido |
| `9023db98` | 4 (doc) | o doc de `a_sessao_acabou_aqui` diz o que vale hoje; o código da função não muda |
| `4e8f6b69` | relatório | este arquivo, com a conformidade ainda pendente |
| `b0fead12` | integração | merge do `0.16/lotes` em `baed898c` (a `versao/0.16` da nona integração, verde em `ffd8e4fb`), antes da conformidade |

O merge saiu limpo: o `merge-tree` não acusou conflito, e só o
`crates/seele-core/src/enlace.rs` foi mesclado automaticamente. A mudança que
chegou nele é a da bomba da tela (`nascer_a_tela` passa o `PedidoDeChave`), longe
do doc do item 4. O `seele-ffi/src/lib.rs` e o `seele-core/src/voice.rs` não
mudaram do outro lado.

**O que não mudou.** O fio: nada em `crates/seele-proto`, `crates/seele-server`,
SEELE-ENC/1, `api/` ou nos arquivos da F6. Nada em `apps/`. Só três arquivos:
`crates/seele-ffi/src/lib.rs`, `crates/seele-core/src/voice.rs` e, no item 4,
`crates/seele-core/src/enlace.rs` (só o doc). Sem `spawn_blocking`, sem
`unsafe`, sem dependência nova. Nenhum aparelho abre fora de sala: as provas de
voz usam a `Voz` de mentira de `o_audio_segue_a_sala`. Ficou como estava: a regra
do assento (`seele_core::assento`); `PortaDaVoz`, `VozDaSala` e `vez_do_driver`;
o volume por apelido (`ganhos_a_aplicar`, `seguir_os_volumes`,
`renomear_volume`); a ordem dos cadeados (voz → escolha e volumes → `Room` →
aplicados); o aviso da thread antes do `drop(runtime)`; e o `encerrar_e_esperar`
do lote F.

**Para a F7, o tamanho do diff no `seele-ffi/src/lib.rs`:** 175 linhas novas
fora dos testes (a maior parte doc), 494 nos testes.

**Para a v6:** nada. Nenhum evento, campo no retrato ou frase de `api/README.md`.

As reversões foram feitas trocando o trecho exato com um script que exige uma
ocorrência só, rodando o teste, devolvendo o arquivo de uma cópia no
scratchpad e conferindo com `cmp`. Todas voltaram idênticas.

## 1. A soltura da tecla de falar durante a reabertura da volta (`9f18fb5f`)

**O defeito, como o brief o descreve e o código confirma.** `set_talking` só
mexia na voz viva. `PortaDaVoz::na_volta` tira a voz do lugar durante a
reabertura inteira, e `Voice::reopen` carrega o `key_held` da velha
(`carregar_controles`). O `vestir` não tocava na tecla, e a janela não manda a
soltura de novo (`segurarFala` deduplica). Soltar o Espaço durante a reabertura
se perdia, e a voz nova transmitia com a tecla segura.

**O conserto.**
- `ControlesGuardados.tecla_segura`.
- `VozDaSala::segurar_a_tecla(segura)`, que `Connection::set_talking` chama, é
  `escolher(|e| e.tecla_segura = segura)`: grava sempre e aplica na viva.
- `vestir_inteira(voz, guardados)` = `vestir` + `segurar(tecla_segura)`. É
  usada nos dois lugares que vestem: `escolher` (na viva) e `instalar` (na que
  entra no lugar).
  - Em `instalar` ela roda depois de `reopen` ter carregado a tecla da velha.
    Por isso a guardada vale **por cima** da herdada.
- A tecla é um método próprio do traço, `VozViva::segurar` (de verdade:
  `set_key_held`), e não um campo a mais no `vestir`.
  - A razão é a prova: a decisão de **quando** a tecla vale fica na parte
    genérica, que a `Voz` de mentira exercita.
  - Com a tecla dentro do `vestir`, a voz de mentira registraria o padrão
    `false` em toda instalação, e o vermelho não apareceria.
- O caminho quente segue curto. Uma pressão custa os dois cadeados e as
  escritas atômicas do `vestir`, e escrever de novo o mesmo valor não mexe em
  nada. Medido no código: `VoiceGate::set_mode` volta na hora com o mesmo modo,
  e os outros são `store`.

**As provas** (`o_audio_segue_a_sala`). O vermelho foi escrito primeiro, com a
costura `segurar_a_tecla` fazendo o que `set_talking` fazia: só na viva, nada
guardado. A `Voz` de mentira ganhou o campo `tecla`. `reaberta(&velha, ssrc)`
é a voz que `reopen` devolve, com a tecla e os ganhos da velha, como
`carregar_controles` os leva.

| teste | vermelho (antes) | verde |
|---|---|---|
| `soltar_a_tecla_durante_a_reabertura_da_volta_fecha_a_fala` | «a pessoa soltou a tecla de falar durante a reabertura da volta e a voz nova entrou com ela segura: o microfone transmite até a próxima pressão, com o roster dizendo que ela fala» | ok |
| `apertar_a_tecla_durante_a_primeira_abertura_vale_quando_a_voz_chega` (a variante do «ABRINDO») | «a pessoa apertou a tecla de falar no «ABRINDO» e a voz abriu com ela solta: …» | ok |
| `sem_voz_viva_a_conexao_guarda_a_tecla_de_falar` (pela `Connection`) | «`set_talking(true)` sem voz viva foi jogado fora: …» | ok |
| `a_voz_de_verdade_veste_cada_controle_guardado` (guarda de texto: cada campo de `ControlesGuardados`, fora `aparelhos` e a tecla, aparece como `escolhida.<campo>` em `impl VozViva for Voice`, e `segurar` chama `set_key_held`) | — | ok |

O primeiro teste tem uma segunda metade: quem segura a tecla durante a volta
inteira continua falando na voz nova.

**Reversões.**

| reversão | o que se desfez | reprovou, com a frase |
|---|---|---|
| R1a | `segurar_a_tecla` de volta a «só na viva, nada guardado» | os três: «`set_talking(true)` sem voz viva foi jogado fora», «…no «ABRINDO» e a voz abriu com ela solta», e a segunda metade da volta, «quem segurava a tecla pela volta inteira voltou calado» |
| R1b | `instalar` com `nova.vestir` em vez de `vestir_inteira` | a volta («…entrou com ela segura…») e o «ABRINDO»; o da conexão passa, como devia |
| R1c | `escolher` com `viva.vestir` em vez de `vestir_inteira` | a volta: «a tecla apertada com a voz viva não chegou a ela: a pessoa segura a tecla de falar e ninguém a ouve» |
| g1 | `segurar` de verdade vazio | o guarda: «a voz de verdade não segura a tecla de falar que a porta manda…» |
| g2 | `self.set_mode(escolhida.modo)` tirado do `vestir` de verdade | o guarda: «a voz de verdade não veste `modo`: …» |

## 2. O ganho que passa a outra pessoa depois de uma volta (`00271fe6`)

**A escolha: preservar os `aplicados` na reabertura, e não deixar de carregar
os ganhos por `ssrc`.** Por quê:
- `aplicados` quer dizer «o que está na voz viva». A reaberta traz os ganhos da
  velha (`reopen` → `carregar_controles`), então zerar o registro era afirmar
  que a voz não tinha nada, quando tinha tudo. Foi essa afirmação falsa que fez
  `seguir_os_volumes` pular a volta a 1.0.
- O conserto mora só na parte genérica da FFI. É um `Origem { Nova, Reaberta }`
  que viaja em `EmCurso` e chega a `instalar`, sob o cadeado da voz, como antes.
- A outra saída mexeria na lista única do núcleo. `carregar_controles` serve à
  troca de microfone, à troca de saída e à reabertura, e o doc dela é
  justamente «um item acrescentado vale para os três». Na troca de aparelho os
  ganhos **têm** de ir junto: `switch_device` não passa por `instalar`, e
  `seguir_os_volumes` acharia os aplicados iguais e não os poria de volta.
  Separar a reabertura obrigaria a uma segunda lista e a mudar
  `os_controles_atravessam_a_reabertura`.
- Sem carregar, a reaberta tocaria todo mundo a 1.0 entre abrir e o primeiro
  `seguir_os_volumes`. Preservando, ela não pula de volume.

**Medido antes de concluir, com uma sonda temporária na ordem do braço
`Reconectado` de `drive`** (`na_volta`, o `olhar` do fim da volta, os volumes, a
fotografia, a chegada):
- **reinício do servidor** (`ssrc` e sessão novos, sem resgate): `depois do olhar
  do Reconectado: ForaDeSala` … `ssrc Some(700) 900=None 901=Some(1.5)`.
  - O assento não é resgatado, a entrada refeita fica no ar, e o `olhar` do fim
    da volta **fecha** a voz e larga a reabertura.
  - A voz que abre depois do `Pong` é nova. Sem o defeito.
- **volta com o assento resgatado** em que o 900 passa a outra pessoa:
  `ssrc Some(701) 900=Some(1.5) 901=Some(1.5)`. É exatamente a medida da
  revisão.

O cenário do brief, «o servidor reinicia», portanto não chega a instalar a voz
reaberta. O que reproduz é a volta **com resgate** em que o número de alguém
muda de dono. O `seele-server` de hoje não faz isso num mesmo processo:
`Registry::issue` só conta para a frente. Mas o fio não promete isso, e o doc de
`seguir_os_volumes` promete a volta a 1.0. O teste usa o caminho que reproduz, e
o doc dele diz as duas coisas. A sonda foi apagada antes do teste.

**A prova:** `depois_da_volta_o_numero_que_mudou_de_dono_volta_a_um`.
- Vermelho antes do conserto: «depois da volta, o 900 passou a ser da Bia e
  ficou com o volume que a Marcela deu à Rafa: a Bia toca a 150 com a janela
  dizendo 100», `left: Some(1.5)`.
- Verde depois.

| reversão | o que se desfez | reprovou |
|---|---|---|
| R2a | `na_volta` marca a reabertura como `Origem::Nova` | o teste novo, com a mesma frase |
| R2b | `instalar` zera os aplicados sempre (`matches!(origem, Nova \| Reaberta)`) | o teste novo, com a mesma frase |
| R2c | `comecar` marca a voz do zero como `Origem::Reaberta` (a outra direção) | `o_volume_volta_quando_se_sai_da_sala_e_entra_de_novo`: «saiu da sala e entrou de novo, e a voz nova abriu com a Rafa a 100…» |

## 3. Os Menores

### M-1. O silêncio digital no `seele.log` (`bac92279`)

`SilencioDigital::observar` compara a marca de antes e a de depois do quadro.
Na subida, ele escreve um `tracing::warn!`:

> «o microfone entrega silêncio digital há 3 s, aberto e sem mudo: zero exato
> em toda amostra, e a sala não ouve nada. O sistema pode estar bloqueando o
> microfone (no macOS, Ajustes → Privacidade e Segurança → Microfone), ou o
> ganho da interface está no mínimo»

- Escreve uma vez por subida, no detector, que é o que o laço chama a cada
  quadro. O guarda `o_silencio_digital_le_o_quadro_cru_e_chega_a_telemetria`
  continua verde, porque a chamada no laço não mudou.
- A frase não depende de plataforma. A mensagem não leva o nome do microfone,
  porque o detector não o conhece. As linhas vizinhas do log o dizem, na
  abertura e na troca de aparelho.

**A prova:** `a_subida_do_silencio_digital_vai_ao_log_uma_vez_por_subida`
(`voice.rs`, `mod silencio_digital`). Captura pelo
`Rastro::a_partir_de(WARN)`, que já põe de pé o segundo despachante.
- Seis segundos de zero exato deixam uma linha. O sinal volta, e mais três
  segundos de zero exato deixam a segunda.
- Vermelho antes: «seis segundos de zero exato deixaram 0 linhas no
  `seele.log`, e eram uma…», `Rastro: []`.

| reversão | reprovou |
|---|---|
| sem o aviso (condição que nunca vale) | «deixaram 0 linhas» |
| um aviso por quadro (`agora \|\| antes`) | «deixaram 151 linhas» |

### M-2. O volume e o mudo do som da tela (`321d8b36`)

O conserto:
- `ControlesGuardados.volume_da_tela` (1.0) e `.tela_calada` (false).
- O `vestir` de verdade os aplica.
- `Connection::ajustar_volume_da_tela` e `calar_a_tela` passam por
  `VozDaSala::escolher_o_som_da_tela`, que é `escolher`: grava sempre e aplica
  na viva.
- No núcleo, `carregar_controles` passa a levar os dois. Sem isso, trocar de
  microfone ou de saída no meio de uma transmissão os devolvia ao padrão, porque
  `switch_device` não passa por `vestir`. O brief cita a lista como parte do
  defeito.
- O doc de `switch_capture`, que enumera o que sobrevive, ganhou os dois.

**O contrato mudou um pouco, e foi escolha.**
- Antes: `NotConnected` sem voz aberta.
- Agora: sem voz viva (a volta reabrindo, a sala abrindo) a escolha é **gravada**
  e devolve `Ok`, como o mudo e o volume das pessoas fazem desde a pendência
  52. `NotConnected` fica só para a sessão sem áudio.
- `som_da_tela()` continua `None` sem voz.
- A janela (`tela-chamada.js`) só faz `console.warn` com o erro e relê
  `som_da_tela`. Nenhum guarda nem a API dos MODs dependem do erro: conferido
  por `grep` em `crates/`, `apps/`, `xtask/` e `api/`.

**As provas.**
- `o_som_da_tela_atravessa_a_saida_e_a_entrada_na_sala` (a `Voz` de mentira:
  ajusta dentro da sala, sai, entra; a voz nova veste 0.3 e calada).
  - Vermelho com a costura fazendo o que a `Connection` fazia (só na viva, nada
    guardado): «saiu da sala e entrou de novo, e o som da transmissão voltou a 1
    com a pessoa o tendo deixado em 0.3».
- `sem_voz_viva_a_conexao_guarda_o_som_da_tela` (a `Connection`; e a sessão sem
  áudio recusa).
  - Vermelho: «sem voz viva, o volume da tela foi recusado e jogado fora:
    arrastado durante a volta, ele se perde», `left: Err(NotConnected)`.
- `o_som_da_tela_atravessa_a_reabertura` (núcleo,
  `mod controles_na_reabertura`).
  - Vermelho: «a troca de aparelho devolveu o som da transmissão a 100 a quem o
    tinha baixado», `left: 1000`.
- O guarda `a_voz_de_verdade_veste_cada_controle_guardado` do item 1 passou a
  cobrar os dois campos novos sozinho, porque lê os campos da struct.

| reversão | o que se desfez | reprovou |
|---|---|---|
| RM2a | a costura de volta a «só na viva, nada guardado» | os dois testes da FFI, com as frases acima |
| RM2b | `set_volume_da_tela` tirado do `vestir` de verdade | o guarda: «a voz de verdade não veste `volume_da_tela`: …» |
| RM2c | `tela_calada` tirada de `carregar_controles` | o do núcleo: «a troca de aparelho devolveu audível o som da transmissão que a pessoa tinha calado» |

### M-4. A voz no fim de `drive` (`bcfa2706`)

**Medido no código antes de mexer.**
- O `drop` da `Voice` não espera nada: `impl Drop for Voice` só marca o `stop`
  da thread de áudio, que é dona do aparelho.
- O que segurava a despedida é o **cadeado** da voz. `Connection::switch_device`
  o segura durante `switch_capture`/`switch_playback`, que abrem o aparelho
  novo no driver. Um driver preso ali prendia o fim de `drive` em
  `shared.voice.lock()`, antes de `client.sair().await`.
- Isso estourava o teto de 500 ms da casca, cujo aviso diz «a despedida não
  terminou» sem dizer por quê.

**O conserto: `largar_sem_segurar`, chamado no lugar do `lock()`, ainda antes da
despedida.** O microfone continua fechando antes de o servidor saber da saída.
- **Cadeado livre:** larga na hora, pelo `try_lock`. Envenenado, larga também.
- **Cadeado preso:** escreve um `warn!` que diz por quê, e larga numa thread com
  nome (`seele-larga-a-voz`, não `spawn_blocking`).
  - A thread espera a vez e larga a voz que estiver lá, inclusive a que a troca
    acabou de pôr.
  - A despedida sai sem esperar.
- **Se a thread não sobe,** larga ali mesmo, como era: uma despedida atrasada é
  melhor que um microfone aberto sem sessão.
- O comentário que dizia que nada segurava a despedida diz agora o que segurava.
- O aviso da thread antes do `drop(runtime)` e o `encerrar_e_esperar` não
  mudaram.

**As provas.**
- `o_fim_da_sessao_larga_a_voz_sem_esperar_a_troca_de_aparelho`.
  - Com o cadeado livre, larga.
  - Com uma thread fazendo de troca presa (segura o cadeado e só põe a voz nova
    e solta quando o teste manda), o fim volta em menos de 500 ms.
  - A voz, inclusive a que a troca pôs, cai assim que o cadeado solta.
  - O log diz por quê. A captura usa o `Espiao` e o `segundo_despachante` de
    `trilha_no_log`, abertos com `pub(super)` para isso.
  - Vermelho, com a costura fazendo o `lock()` de antes: «o fim da sessão esperou
    2.010023292s pelo cadeado da voz, preso por uma troca de aparelho no driver:
    …».
- `o_fim_de_drive_larga_a_voz_antes_da_despedida_e_sem_esperar` (guarda de
  texto sobre `drive`, por `pedacos_de_drive`).
  - Depois de `shared.running.store(false`, `largar_sem_segurar(&shared,` vem
    antes de `client.sair().await`, e não há `shared.voice.lock()`.

| reversão | o que se desfez | reprovou |
|---|---|---|
| RM4a | a costura volta a esperar (`largar()`) | «esperou 2.004610958s pelo cadeado da voz…» |
| RM4b | sem o `warn!` | «o fim da sessão largou a voz por fora e o log não diz por quê…», `Escrito: []` |
| RM4c | a thread de fallback não larga nada | «a troca de aparelho soltou o cadeado e a voz que ela pôs ficou de pé depois do fim da sessão…» |
| RM4d | `drive` de volta ao `if let Ok(mut voice) = shared.voice.lock()` | o guarda: «o fim de `drive` não larga mais a voz pelo caminho que não espera…» |

## 4. O doc de `a_sessao_acabou_aqui` (`9023db98`)

O doc dizia que «nenhum servidor» manda as três razões da sala pessoal.
Conferido no servidor:
- **`ChamadaEncerrada` chega no meio da sessão.** `Recepcao::desligar`
  (`crates/seele-server/src/recepcao.rs`, desde `9890e4eb`) a manda a cada
  convidado pelo `Event::ServidorEncerrando`, e a sessão a escreve como
  `Disconnecting`. É a que passa por `a_sessao_acabou_aqui`.
- **`NinguemAtendeu` e `ChamadaRecusada` chegam no aperto de mão.** O servidor as
  manda na fase do toque, antes de a sessão existir (`tocar` e
  `recusar_o_aperto`, em `session.rs`). Chegam como recusa do aperto,
  `ConnectError::Refused` (`client.rs`, braço `Disconnecting` do aperto), e não
  por aqui.

O doc agora diz as duas coisas, e diz por que as três ficam na lista. O código da
função não mudou. O `cargo doc --document-private-items` não acusa nada nas
linhas novas.

## O que rodou

Tudo com `unset CARGO_TARGET_DIR`, no fim do lote:
- `cargo test -p seele-ffi --lib`: **175 passaram, 1 ignorado** (eram 166 + 1;
  9 testes novos).
- `cargo test -p seele-core --lib`: **529 passaram** (eram 527; 2 novos).
- `cargo test -p xtask --test log_com_segundo_despachante`: **10 passaram**
  (rodado depois do M-1 e do M-4, os dois com captura de log nova).
- `cargo fmt --all -- --check`: limpo.
- `cargo clippy -p seele-core -p seele-ffi --all-targets --all-features -- -D
  warnings`: limpo. Rodado de novo com os arquivos tocados, para não confiar no
  cache.
- `cargo doc -p seele-core --no-deps`: nenhum aviso nas linhas do item 4.

**Sobre o estado mesclado (`b0fead12`)**, depois da liberação da coordenação:
- `cargo test -p seele-ffi --lib`: **175 passaram, 1 ignorado**.
- `cargo test -p seele-core --lib`: **523 passaram**. São 6 a menos que antes do
  merge porque o `tela.rs` que chegou perdeu testes (8 a menos, e ganhou um em
  outro arquivo). Os dois testes do lote no núcleo,
  `a_subida_do_silencio_digital_vai_ao_log_uma_vez_por_subida` e
  `o_som_da_tela_atravessa_a_reabertura`, rodaram e passaram.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-core -p seele-ffi
  --all-targets --all-features -- -D warnings`: limpos, com os arquivos tocados
  antes do clippy.
- **A conformidade**, um arquivo por vez, com `-- --test-threads=1` e nenhum
  outro binário da conformidade rodando na máquina (conferido com `ps`):
  - `voz_na_reconexao`: **1 passou**;
  - `a_recusa_nao_tira_da_sala`: **5 passaram**;
  - `saida_com_despedida`: **3 passaram**. Quem saiu pela FFI sumiu dos
    presentes em 522 µs, e o filho que morreu na linha seguinte em 154 ms.

## O que não rodou

- `seele-app`, nem `--bin` nem `tests/frontend.rs`. O lote não toca `apps/`, e
  nenhuma assinatura pública do `seele-ffi` mudou. Os guardas de `frontend.rs`
  que leem `seele-ffi/src/lib.rs` como texto procuram `drain_transfers()`,
  `Transfer::RefusedBecause`, `Transfer::Unavailable` e
  `consentir_no_caminho_entre_pares`, e nada disso foi tocado.
- Nenhum código `cfg(windows)` ou `cfg(target_os = "linux")` foi escrito. O aviso
  do M-1 não depende de plataforma, e o teste dele roda neste Mac.

## Observações

1. **O item 2, como pedido, não acontece com o `seele-server` de hoje.** Está
   medido acima: o reinício fecha e reabre do zero, e o resgate não muda o dono
   de número nenhum. O conserto vale para um servidor que reuse números, que o
   fio não proíbe, e deixa `aplicados` dizendo a verdade.
2. **A reaberta ainda roda um instante com a tecla da velha.** Fica entre o
   `reopen` voltar, na thread do áudio, e o `instalar`, no laço. O `reopen`
   carrega a tecla, o laço de áudio já está de pé, e o `drop` da velha não
   espera. Medido no código, não no relógio: são o envio pelo canal e a volta
   do `select!`, menos que um quadro de 20 ms no caso comum.
   - Fechar de vez pediria soltar a tecla da velha ao tirá-la do lugar em
     `na_volta` (`segurar(false)` antes de `reabrir`). Não foi feito, para
     manter o diff no que o brief pede. Fica como sugestão.
3. **No caso preso do M-4, a voz pode viver um pouco além do fim de `drive`:**
   até o driver soltar a troca. O microfone fica aberto nesse intervalo, sem
   sessão, e o log diz por quê. É a troca que o brief ofereceu («largue a voz
   fora do caminho da despedida»).
4. **`perl -e 'alarm N; exec …'` mata o `cargo`, e não o binário de teste.**
   Uma sonda minha travou (um `MutexGuard` temporário segurado dentro de um
   `eprintln!`), e o binário ficou órfão depois do alarme. Foi morto pelo PID.
   - Os testes novos têm prazo próprio: `timeout` nos `await`, `recv_timeout`
     na troca presa, laço de espera com teto.
   - Outro processo `cargo test --workspace --no-fail-fast -- --test-threads=1`
     rodava na máquina ao mesmo tempo, de outro worktree. Ninguém mexeu nele.
5. O traço `VozViva` ganhou `segurar`. As duas vozes de mentira do arquivo o
   implementam, e a de `a_voz_quando_o_enlace_volta` não faz nada com ele.
