# Lote F da F5: relatório

Base `411778ef` (branch `0.16/lotes`). Branch `0.16/lotes-f`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a6f43a1e6a2fb6adb`. Quatro
commits de código e este relatório, todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `dde763b5` | P50-1 | `Motor::despedir`, `Enlace::sair(&mut self)` esperando o motor, `TarefasDePar::largar_tudo_e_esperar`; `saida_com_despedida.rs` (a) e (b); (c) em `tela_por_um_par.rs`; o doc de `esperar_o_servidor_fechar` |
| `fd10af6b` | P50-2 | o sinal da thread da conexão e `Connection::encerrar_e_esperar`; o teste do filho em `saida_com_despedida.rs` |
| `b7aeb749` | P50-3 e o guarda | `despedir_da_sessao`, `despedir_se` passando por ela, «MOD nativo parou» em INFO; `encerramento.rs` reescrito |
| `fea3e7f8` | fecho de doc | o doc de `Enlace::sair` deixa de ligar a constante privada (o rustdoc reprova link de item público para privado) |

Nada no fio: `seele-proto` intocado, o fechamento é o `CONNECTION_CLOSE` do
QUIC com o `EJECTED` de `Client::disconnect`. Nada no SEELE-ENC/1, nos arquivos
da F6, em `api/`, na CSP ou em `ui/`. Nenhuma dependência nova, nada baixado.
`Hospedagem::despedir` não é chamada (é o G-casca, do Plano 2), e
`a_sessao_acabou_aqui` não mudou. `docs/pendencias.md` e o índice da 1.0
também não. Nenhuma promessa nova de API de MOD: não há seção «Para a v6» (as
duas linhas novas do `seele.log` são registro, não evento nem campo).

As reversões foram feitas trocando o trecho exato, rodando os testes,
devolvendo o arquivo de uma cópia no scratchpad e conferindo com `cmp` e
SHA-256. Todas voltaram idênticas.

## P50-1: o `Enlace::sair` espera a despedida (`dde763b5`)

### O que mudou

`crates/seele-core/src/enlace.rs`:
- **`ESPERA_DA_DESPEDIDA` (300 ms)**, com o doc pedido: o `CONNECTION_CLOSE`
  sai na primeira volta do driver, e a espera só limita o escoamento (3×PTO).
- **`ESPERA_DAS_TAREFAS_DE_PAR` (100 ms)** e **`ESPERA_DO_MOTOR_NA_SAIDA`
  (1 s)**, as duas outras esperas do brief, com nome e doc.
- **Os dois braços** `Some(Comando::Sair) | None` (com cliente e sem cliente)
  passam a `return self.despedir().await`.
- **`async fn despedir(&mut self)`**, na ordem do brief:
  1. guarda `self.cliente.as_ref().map(Client::endpoint)`;
  2. `self.tarefas_de_par.largar_tudo_e_esperar(ESPERA_DAS_TAREFAS_DE_PAR)`:
     tira as alças do mapa e a de quem serve, aborta todas e espera que
     devolvam, com teto. Vencido o teto, uma linha WARN;
  3. `self.encerrar(Motivo::Pedido)`;
  4. `tokio::time::timeout(ESPERA_DA_DESPEDIDA, ponta.wait_idle())`. Vencido,
     uma linha DEBUG: não é falha, o fechamento já saiu.
- **`pub async fn sair(&mut self)`**: tira a alça (`Option<JoinHandle>`, com
  `take()`), manda o `Sair` e espera a tarefa, os dois dentro de um só
  `timeout` de 1 s; vencido, `abort()` e uma linha WARN. Sem alça (segunda
  chamada), volta na hora.
- **`impl Drop for Enlace`** aborta a alça que sobrar.
- Os três chamadores já tinham `mut` (`drive` na FFI, `bateria_interna.rs`,
  `tela_por_um_par.rs`); `drive` não mudou.

`volta_pela_trilha.rs`: o doc de `esperar_o_servidor_fechar` deixa de dizer
«20,0 s (o `IDLE_TIMEOUT`)». A frase dos microssegundos ficou.

### As provas

**`seele-core`, um teste a mais que o brief** —
`enlace::tests::a_despedida_cala_as_tarefas_de_par_antes_do_encerrado`. Uma
tarefa de par que escreve `TelaQuadro` na fila sem parar, o motor de pé depois
de `despedir` (como fica durante o `wait_idle`), e a fila lida. Entrou porque
a conformidade estava presa quando o conserto foi escrito, e ele prova a ordem
sem rede.
- Vermelho antes do conserto (com `despedir` só chamando `encerrar`, que era o
  comportamento de antes): «41 quadros do par entraram na fila depois do
  `Encerrado`: a despedida avisou o fim com o caminho do par de pé, e a casca
  recebe imagem de uma sessão que ela já fechou».
- Verde depois.

**`saida_com_despedida.rs`** (novo, `vaga::minha()` na primeira linha de cada
teste). Um `Daemon` em memória em `127.0.0.1:0`, sem ponto de encontro; quem
fica é um `Enlace` que dobra os avisos num `seele_core::Room` e lê
`Room::presentes`. A espera por quem sumiu vai até `IDLE_TIMEOUT + 5 s`, para a
reprovação dizer quanto levou.
- **(a)** `quem_sai_pela_ffi_some_dos_presentes_dos_outros_em_menos_de_dois_segundos`:
  A entra por `seele_ffi::Connection::connect` (`audio: false`) em
  `spawn_blocking`; `a.disconnect(); drop(a)`.
- **(b)** `o_enlace_que_sai_num_runtime_que_cai_em_seguida_ainda_avisa`: o
  `Enlace` numa `std::thread` com runtime `current_thread`, `drop(runtime)` na
  linha depois do `block_on` que termina em `sair().await`.

Os dois foram escritos e compilados (`--no-run`) antes do conserto. A
conformidade foi liberada com o conserto já escrito, e o vermelho deles foi
provado pela reversão abaixo.

Verdes (`--test saida_com_despedida -- --test-threads=1`): (a) 554 µs e (b)
1,1 ms na primeira rodada; na HEAD, seis rodadas seguidas entre 340 e 700 µs
(a) e (b), todas verdes.

**(c)** `tela_por_um_par.rs`, `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para`:
depois de `sair().await`, a fila é lida com `proximo()` em prazo zero até o
`Encerrado` (fila vazia antes dele reprova: «`sair` voltou antes de o motor
dizer que a sessão acabou»), e depois dele, sem drenar, nenhum `TelaQuadro`
desta tela. O segundo de silêncio com quem empresta recebendo continua depois.
`DRENAGEM_DEPOIS_DO_FIM` e `drenar_depois_do_fim` saíram (ficariam sem uso); o
doc de `DRENAGEM_ANTES_DO_SILENCIO`, que apontava para o primeiro, passou a
dizer o próprio motivo. Verde: o arquivo inteiro, 19 de 19, em 51,7 s.

`bateria_interna.rs` inteiro, 7 de 7 (`sair_encerra_sem_esperar_a_bateria`
inclusive), em 25 s.

### Reversões (arquivo conferido: `crates/seele-core/src/enlace.rs`, SHA-256 `493564716e68…` antes e depois)

1. **Os braços de volta a `self.encerrar(Motivo::Pedido)` e `sair` sem a
   espera** (só o `mandar(Comando::Sair)`): (a) e (b) vermelhos, aos 20 s.
   - (a): «quem saiu pela FFI levou 20.002179125s para sumir dos presentes de
     quem ficou, e o prometido é 2s: o `CONNECTION_CLOSE` não saiu antes de a
     thread da conexão derrubar o runtime, e o servidor só viu a saída pelo
     prazo de ociosidade».
   - (b): «o enlace que saiu levou 20.00247875s para sumir dos presentes de
     quem ficou, e o prometido é 2s: `sair` voltou antes de o motor se
     despedir, …».
2. **Sem o cancelamento de antes do aviso** (a chamada a
   `largar_tudo_e_esperar` tirada de `despedir`): (c) vermelho, «o quadro
   Some(36) estava na fila de quem saiu **depois** do `Encerrado`: a despedida
   avisou o fim com o caminho do par de pé, …»; o teste do `seele-core` também,
   com 46 quadros.

## P50-2: a casca espera a thread da conexão (`fd10af6b`)

### O que mudou

`crates/seele-ffi/src/lib.rs`:
- a thread `seele-connection` faz `encerrada_tx.send(())` logo depois de
  `runtime.block_on(drive(...))` voltar, e **antes** de `drop(runtime)`, agora
  escrito. Um runtime que não sobe solta o remetente sem sinal;
- `Connection` ganha `encerrada: Mutex<Option<std::sync::mpsc::Receiver<()>>>`;
- **`pub fn encerrar_e_esperar(&self, teto: Duration) -> bool`**, com
  `#[must_use]`: `disconnect()` e `recv_timeout(teto)` com o cadeado na mão.
  `Ok` ou `Disconnected` dão `true` (e o recebedor sai: o fim já foi visto);
  `Timeout` dá `false` e deixa o recebedor, para outra chamada esperar de novo.
  `None` dá `true`: outra chamada já viu o fim.
- As dez `Connection` montadas à mão nos testes ganharam `encerrada:
  Mutex::new(None)`.

### As provas

- **`-p seele-ffi encerrar_e_esperar`**: dois testes de unidade.
  - `encerrar_e_esperar_sem_o_sinal_espera_o_teto_e_diz_que_nao_terminou`
    (o do brief: um `Receiver` que nunca recebe dá `false`, e só depois do
    teto). Vermelho contra um esboço que voltava `true` sem esperar: «a thread
    da conexão não deu sinal nenhum, e `encerrar_e_esperar` disse que ela
    terminou: a casca deixa o processo sair com a despedida no meio, calada».
  - `encerrar_e_esperar_volta_na_hora_com_o_sinal_dado_ou_a_thread_acabada`,
    a mais, barato: o sinal dado e o remetente largado dão `true` sem esperar o
    teto. Verde também contra o esboço (ele não prova a espera, prova o
    contrato do `true`).
- **`encerrar_e_esperar_volta_com_o_aviso_ja_dado`**, em
  `saida_com_despedida.rs`, como o controlador decidiu. O filho é o próprio
  binário (`current_exe`, `--exact … --test-threads=1`, variável
  `SEELE_SAIDA_COM_DESPEDIDA_FILHO` com o endereço do servidor, no molde do
  `CANARY`). Ele entra pela FFI, espera uma linha na entrada (a ordem do pai,
  ou a entrada fechada se o pai cair), faz
  `assert!(sai.encerrar_e_esperar(Duration::from_millis(500)))` e
  `std::process::exit(0)` na linha seguinte. A casa do filho é uma pasta do
  pai (`SEELE_SAIDA_COM_DESPEDIDA_CASA`), que o pai apaga: o filho morre por
  `exit` e não roda destrutor. O pai exige saída limpa do filho e os presentes
  perdendo a pessoa em menos de 2 s.
  - Vermelho antes do conserto, contra o esboço: «o filho que saiu por
    `encerrar_e_esperar` e morreu na linha seguinte levou 20.001844042s para
    sumir dos presentes de quem ficou, e o prometido é 2s: …».
  - Verde depois: 611 µs; na HEAD, seis rodadas entre 445 e 720 µs.

### Reversão (arquivo conferido: `crates/seele-ffi/src/lib.rs`, SHA-256 `fd20419b991b…` antes e depois)

**`encerrar_e_esperar` voltando sem esperar o sinal** (o `match` passou a
começar de um `Ok(())` pronto): o teste do filho vermelho nas **três** rodadas,
aos 20,0 s (20.001787709s, 20.003472208s, 20.00194075s), e o teste de unidade
do teto vermelho com a frase acima.

## P50-3: a saída para os MODs, e diz isso no `seele.log` (`b7aeb749`)

### O que mudou

`apps/seele-app/src/main.rs`:
- **`fn despedir_da_sessao(sessao: &Session, teto: Duration)`**, sem
  `AppHandle`:
  1. `sessao.revogar()` primeiro, e `tracing::info!(mods = n, "saída do app:
     MODs nativos mandados parar")`, com `n` = o tamanho de `encerrando` logo
     depois da revogação (os que foram mandados parar e não confirmaram);
  2. espera, com teto `teto`, que `encerrando` esvazie, tomando o cadeado de
     `mods_nativos` só para contar e soltando antes de cada pausa de 10 ms. Se
     sobrar algum, uma linha INFO a mais: «saída do app: MODs nativos sem
     confirmar a parada dentro do teto», com `restantes`. Não estava no brief;
     entrou porque a saída sabe disso e, sem a linha, não contaria;
  3. `encerrar_e_esperar(Duration::from_millis(500))`; `false` dá o WARN com a
     frase do brief, «a despedida não terminou em 500 ms: quem ficou vai notar
     a saída pelo prazo de ociosidade, 20 s».
- **`ESPERA_DOS_MODS_NA_SAIDA` (200 ms)** é o `teto` que `despedir_se` passa.
- **`fn despedir_se`** passa a chamar `despedir_da_sessao`; o doc diz o que
  acontece de verdade, os ~700 ms de teto na thread principal inclusive.
- **`assentar_fala`**, braço `ParaOFora::Parou`:
  `tracing::info!(mod_id = %quem, instancia = numero, "MOD nativo parou")`.
  **Escolhi a instância, e não a geração**: a assinatura já a tem, o número
  nunca se repete na janela (é a chave de `ModsNativos`), e a linha «MOD
  reservado no executor nativo» já liga `instancia` à `geracao`. Passar a
  geração mudaria a assinatura e os sete chamadores para dizer o que o
  registro já diz. A linha DEBUG genérica «fala de MOD nativo» continua saindo
  para todas as falas, a de parada inclusive.
- Os docs que diziam que a saída do app não passava por `revogar`
  (`Session::dizer_as_contas_do_registro`, `conta_do_balde_dita`, e a frase de
  uma asserção de `dizer_as_contas_do_registro_diz_uma_vez_e_esvazia`) foram
  acertados.

### As provas (`-p seele-app --bin seele-app despedir`, e `a_parada_do_mod_nativo`)

Os três escritos antes do conserto e vermelhos contra um esboço de
`despedir_da_sessao` com o comportamento de antes (`disconnect` e `sleep`, sem
`revogar`), pela captura de `rastro_de_teste::capturar`:
- **(1)** `a_supervisao_dos_mods_nativos::despedir_da_sessao_manda_os_mods_nativos_parar_e_diz_quantos`:
  `Session::default()`, uma instância do ajudante `instancia(geracao)` posta
  como `Reservada`; depois de `despedir_da_sessao`, ela está em `encerrando`, e
  a captura tem a linha INFO com `mods=1`. Vermelho: «a saída do app não
  mandou parar o MOD nativo de pé: ele continua entre os vivos, e roda até o
  processo morrer».
- **Depois da onda 1**, `o_teto_de_vazao_do_registro::despedir_da_sessao_diz_a_conta_que_nenhuma_linha_levou`:
  um balde de `a/b` com 5 linhas seguradas, `despedir_da_sessao`, e a captura
  tem a linha da conta com `suprimidas=5`. Vermelho: «a saída do app não disse
  as 5 linhas seguradas do balde de `a/b`: a conta caiu com o processo, …».
- **(2)** `o_console_do_mod_chega_ao_registro::a_parada_do_mod_nativo_sai_em_info_com_o_id_e_a_instancia`:
  `assentar_fala` com `ParaOFora::Parou` dá `Fim { avisar: true }` e uma linha
  INFO com «MOD nativo parou», `mod_id=seele/perfis` e `instancia=7`.
  Vermelho: «a parada do MOD nativo não saiu em INFO com o id e a instância: …
  DEBUG seele_app: fala de MOD nativo mod_id=seele/perfis tipo="parou"».

Os dois primeiros foram renomeados para começar por `despedir_da_sessao`, para
o comando do brief (`--bin seele-app despedir`) rodá-los.

### `apps/seele-app/tests/encerramento.rs`

Reescrito, sem apagar nenhum dos dois guardas. Um ajudante `corpo` recorta a
função pelo nome, da assinatura na coluna zero até a chave que a fecha — a
versão antiga de `a_despedida_espera_o_quadro_sair` lia **o resto do arquivo**
depois de `fn despedir_se`, e qualquer `sleep` em qualquer função abaixo a
deixava verde.
- `fechar_a_janela_avisa_o_servidor`: `RunEvent::ExitRequested` antes de
  `despedir_se(handle)`; o corpo de `despedir_se` chama `despedir_da_sessao(`, e
  o de `despedir_da_sessao` chama `encerrar_e_esperar(`.
- `a_despedida_espera_o_quadro_sair`: o corpo de `despedir_se` sem `sleep` e
  chamando `despedir_da_sessao(`, e o de `despedir_da_sessao` com
  `encerrar_e_esperar(…Duration::from_millis(`, que é o teto.
- O doc do módulo diz que a prova de comportamento mora em
  `saida_com_despedida.rs`.

### Reversões (arquivo conferido: `apps/seele-app/src/main.rs`, SHA-256 `9f33b19b10ba…` antes e depois de cada uma)

1. **Sem o `revogar()`** em `despedir_da_sessao`: (1) e o teste da conta
   vermelhos, com as frases acima.
2. **`info!` de volta a `debug!`** no braço `Parou`: (2) vermelho, «… DEBUG
   seele_app: MOD nativo parou mod_id=seele/perfis instancia=7».
3. **`despedir_se` de volta ao `disconnect()` com `sleep`**: os dois guardas
   de `encerramento.rs` vermelhos. `fechar_a_janela_avisa_o_servidor`: «a
   despedida da janela deixou de passar por `despedir_da_sessao`, que é quem
   manda os MODs parar e espera a thread da conexão».
   `a_despedida_espera_o_quadro_sair`: «a despedida voltou a dormir às cegas em
   vez de esperar a thread da conexão. Um `sleep` não sabe se o
   `CONNECTION_CLOSE` saiu, e o defeito volta sem nada mudar de aparência.»

As reversões foram feitas sobre o `main.rs` de `9f33b19b`. Depois delas vieram
o `cargo fmt` (dois testes reformatados) e os docs acertados acima; a bateria
do crate rodou de novo inteira em cima disso.

## O que rodou, na HEAD

- `--test saida_com_despedida -- --test-threads=1`: 3 de 3, seis rodadas.
- `--test tela_por_um_par -- --test-threads=1`: 19 de 19.
- `--test bateria_interna -- --test-threads=1`: 7 de 7.
- `-p seele-core`: 445 de 445 (antes do último commit, que é só doc).
- `-p seele-ffi`: 126, mais 1 ignorado que já era.
- `-p seele-app` inteiro: o binário (271), `frontend` (268), `encerramento`
  (2), e `empacotamento`, `fontes`, `manifesto_hibrido`, `marca`,
  `permissoes`, `tokens`, todos verdes.
- `cargo fmt --all -- --check` limpo; `cargo clippy -p seele-core -p seele-ffi
  -p seele-conformance -p seele-app --all-targets --all-features -- -D
  warnings` limpo (o `ld: duplicate -rpath` é ambiental).
- `cargo doc` dos três crates: nenhum aviso novo meu; os que aparecem já
  estavam lá.

## O que não rodou

- **Windows e Linux.** Nenhum código com `cfg` foi escrito; o que muda de
  plataforma para plataforma é o tempo do escoamento e o do `ExitRequested`, e
  nada disso foi medido fora deste Mac.
- **A bateria inteira**, `quarto_pelo_link` e os outros arquivos de
  conformidade: só os três liberados rodaram. Outros arquivos chamam
  `Connection::disconnect` (`volta_pela_trilha.rs`, `acceptance_m3.rs`) e
  agora veem a despedida de verdade; rodam na integração.
- **A voz.** Os testes usam `audio: false`. Com voz, `drive` solta a `Voice`
  antes de `sair`, e esse tempo entra nos 500 ms de `encerrar_e_esperar`; não
  foi medido.
- `cargo xtask check-api`: `api/` não foi tocado.

## Escolhas, além das que o brief deixou abertas

- `despedir_da_sessao(sessao, teto)`: o `teto` é o dos MODs; os 500 ms da
  conexão ficaram literais, como o brief os escreve, e a frase do WARN os
  repete.
- O `timeout` de `sair` cobre o envio do `Sair` **e** a espera da tarefa: um
  motor preso com a fila de comandos cheia não segura quem sai.
- `encerrar_e_esperar` segura o cadeado durante o `recv_timeout`: uma segunda
  chamada espera a primeira, e não vê um `None` falso no meio.
- O teste (c) lê em prazo zero a fila inteira até o `Encerrado` e, depois
  dele, até a fila esvaziar ou o canal fechar (um segundo `Encerrado`); o
  silêncio de um segundo de antes continua depois.

## Preocupações

1. **A reversão do P50-2 é uma corrida, ganha por larga margem aqui.** O
   vermelho depende de o `exit(0)` do filho matar o processo antes de a thread
   da conexão escrever o fechamento no soquete. Aqui foram 3 de 3, aos 20 s; a
   thread ainda tem de acordar pelo `Shutdown`, limpar e chamar `sair` antes do
   driver mandar o pacote. Numa máquina onde isso ganhasse do `exit`, a
   reversão passaria verde sem que a espera tivesse deixado de importar. O
   teste de unidade do teto é a segunda parede, e não depende de corrida.
2. **O `ExitRequested` segura a thread principal** por até ~700 ms (200 dos
   MODs e 500 da conexão), como o brief previa. No caso comum medido aqui são
   microssegundos para a conexão; os MODs não foram medidos com uma janela de
   verdade.
3. **No (c), a fila chegou vazia de quadros antes do `Encerrado` em toda rodada
   verde** («0 avisos vieram antes do `Encerrado`»): o par para de entregar
   antes de o pedido chegar ao motor. A ordem foi provada pela reversão
   (quadro 36 depois do fim), mas o laço de antes do fim só é exercitado com
   avisos de verdade quando há quadro em voo no instante do `sair`.
4. **A pendência 56** (aviso do coordenador): nenhum teste meu lê log no
   `seele-core`, `seele-server` ou `seele-ffi`; os três do `main.rs` passam por
   `capturar`. Nenhum vermelho intermitente apareceu: o binário do app rodou
   inteiro três vezes, todas verdes, e cada vermelho e verde dos três foi
   rodado também pelo nome.

## Rodada 1 de conserto (FIX_BASE `76227050`)

A revisão achou o lote conforme e o comportamento bom, com lacunas na prova:
dois Importantes e seis Menores. Esta rodada conserta os oito, e nada além.

| commit | itens | o que muda |
|---|---|---|
| `72b57732` | I1, M5 | o guarda de `encerramento.rs` lê o corpo sem comentários e exige teto que não seja zero; o doc de `encerrar_e_esperar` diz o dobro do teto com duas chamadas |
| `6a0d5bed` | M3, M4 | `despedir_da_sessao` conta e espera só a geração que revogou; dois testes com um MOD que confirma noutra thread |
| `8a38df53` | M1 | o filho do P50-2 estaciona a thread da conexão antes de sair |
| `29a31553` | M6 | o (c) lê a fila por `ja_na_fila`, sem o orçamento cooperativo |
| `b9adf2f8` | I2, M2 | os docs de `ESPERA_DA_DESPEDIDA`, `Motor::despedir` e `largar_tudo_e_esperar` dizem o que cada passo compra e o que não tem prova |

Como antes, cada reversão trocou o trecho exato, rodou os testes e devolveu o
arquivo de uma cópia no scratchpad, conferida com `cmp`.

### I1 — o guarda de `encerramento.rs` casava com comentário

- **O vermelho medido, antes do conserto:** com o guarda de `76227050`, duas
  trocas do corpo de `despedir_se` deixaram os dois guardas verdes (2 de 2
  cada), com o app de volta ao defeito da P50:
  - **A**, a da revisão: `// era: despedir_da_sessao(...)` em cima de um
    `connection.disconnect()`;
  - **B**: o mesmo `disconnect()`, com a chamada certa num comentário de fim
    de linha.
- **O conserto:** `corpo` devolve o corpo passado por `sem_comentarios`, que
  tira o `//` até o fim da linha e o `/* … */`, e deixa intacto o que está
  entre aspas. A asserção combinada de `a_despedida_espera_o_quadro_sair` foi
  separada em duas (o `sleep` e a passagem por `despedir_da_sessao`), para cada
  reversão ler a frase que a nomeia.
- **A prova, com o guarda novo:** A e B vermelhas nos dois guardas.
  `fechar_a_janela_avisa_o_servidor`: «a despedida da janela deixou de passar
  por `despedir_da_sessao`, que é quem manda os MODs parar e espera a thread da
  conexão». `a_despedida_espera_o_quadro_sair`: «a despedida da janela deixou de
  passar por `despedir_da_sessao`, e com ela pela espera da thread da conexão:
  …». O `main.rs` voltou a `e57b33fb…`, conferido com `cmp`.

### I2 — a espera de escoamento sem vermelho, e o doc que a medida desmentia

A espera fica, por decisão do controlador, e os docs dizem o que ela compra
(`b9adf2f8`). `ESPERA_DA_DESPEDIDA` diz:
- que o `CONNECTION_CLOSE` sai na primeira volta do driver, com ou sem ela;
- que ela mantém o runtime e a ponta vivos no período de fechamento do QUIC,
  para reenviar o fechamento se o primeiro se perder;
- que nenhum teste prova isso, porque o laço local não perde pacote;
- e o custo, de até 300 ms, medido em ~86 ms no laço local.

`Motor::despedir` diz que o conserto da P50 é o `sair` esperar o motor, e qual
passo tem prova. Nenhum proxy com perda foi construído.

**A reversão 1 da rodada 0, decomposta e medida por mim** (`enlace.rs` em
`4290ed0d…` antes e depois de cada parte, conferido com `cmp`):

| parte revertida | (a) | (b) | filho do P50-2 | (c) |
|---|---|---|---|---|
| os dois braços de volta a `self.encerrar(Motivo::Pedido)`, com o `sair` esperando | verde 3 de 3 (388–436 µs) | verde 3 de 3 (361–503 µs) | verde 3 de 3 (152–158 ms) | verde 3 de 3 |
| o `sair` sem esperar o motor, com os braços em `despedir` | vermelho, 20,0028 s | vermelho, 20,0016 s | vermelho, 20,0016 s | — |
| só o `wait_idle` tirado | verde 3 de 3 | verde 3 de 3 | verde 3 de 3 | — |

- O que cada parte do `despedir` prova:
  - **o cancelamento** é provado pelo (c);
  - **o `encerrar`** é o de antes;
  - **o `wait_idle`** não tem prova, pelo motivo acima.
- **O custo do `wait_idle`, medido:** com `sair` cronometrado no (b) (uma linha
  provisória, tirada depois, com `saida_com_despedida.rs` conferido em
  `4d9bf9ca…`), `sair` levou 85,9–87,4 ms em 5 rodadas com o `wait_idle`, e
  0,08–0,13 ms sem ele.
- **A janela de 300 ms é a que força um quadro a caminho no (c) revertido.**
  Sem o cancelamento, o (c) reprova (abaixo, em M6) porque o motor fica de pé
  no `wait_idle` depois do `Encerrado`, com o par entregando. Com os braços de
  volta a `encerrar`, sem a janela, o motor cai e aborta as tarefas
  microssegundos depois do aviso, e o (c) passa verde 3 de 3.

### M1 — o vermelho do P50-2 dependia de uma corrida

- **O conserto (`8a38df53`):** a alternativa que a revisão prototipou.
  - O filho assina um `EventListener` (`Estaciona`). Armado, no primeiro
    `RosterChanged` ele avisa por um canal e dorme `ESTACIONADA` (150 ms) na
    thread da conexão, que é onde `on_event` roda.
  - O filho arma o ouvinte e chama `leave_voice_room()`, que avisa
    `RosterChanged` de dentro de `run_command`.
  - Ele espera o «estacionei» (até 5 s) e só então chama
    `encerrar_e_esperar(500 ms)` e `exit(0)`.
  - Armar só depois da ordem do pai impede que um `RosterChanged` da chegada
    estacione a thread cedo demais.
- **Verde:** 3 de 3, em 153,6, 155,8 e 161,0 ms (os 150 ms estacionados
  inclusive).
- **Reversão** (`encerrar_e_esperar` voltando sem esperar o sinal): vermelha 3
  de 3, por construção, em 20,0029, 20,0026 e 20,0020 s, com a frase de
  sempre. `seele-ffi/src/lib.rs` voltou a `ba79ab59…`, conferido com `cmp`.
- **Se o gatilho mudar, falha alto:** com o ouvinte desarmado, o filho reprova
  em 5 s com «a thread da conexão do filho não estacionou em 5 s:
  `leave_voice_room` deixou de avisar `RosterChanged` de dentro da thread da
  conexão, …», e o pai mostra essa frase em «o filho não saiu limpo (exit
  status: 101)». O arquivo de teste voltou ao que era, conferido com `cmp`.

Isto substitui a preocupação 1 da rodada 0.

### M2 — a espera de `largar_tudo_e_esperar` sem vermelho

- **Medido:** com `largar_tudo()` (o `abort` sem esperar) no lugar de
  `largar_tudo_e_esperar`:
  - `a_despedida_cala_as_tarefas_de_par_antes_do_encerrado` ficou verde 5 de 5;
  - `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` ficou verde
    3 de 3.
- **O que mudou:** o doc da função e o de `Motor::despedir` dizem que a espera
  é plausível e não provada. O que os dois testes provam é o cancelamento antes
  do aviso. Sem teste novo, como pedido.

### M3 — a espera dos MODs sem o cadeado não tinha guarda

- **O teste (`6a0d5bed`):** `despedir_da_sessao_espera_os_mods_sem_segurar_o_cadeado`.
  Uma thread (escopo de `std::thread::scope`) toma o cadeado da supervisão e
  chama `confirmar_parada` 30 ms depois. O teste exige:
  - que `despedir_da_sessao`, com teto de 2 s, volte antes de 1 s;
  - que nenhuma linha «sem confirmar a parada» saia.
  Verde.
- **Reversão** (o guarda de `mods_nativos` preso durante toda a espera):
  vermelho, «a saída do app levou 2.012082208s com um MOD que confirmou a
  parada em 30 ms: a espera segurou o cadeado da supervisão, e a confirmação
  ficou parada atrás dela até o teto». O `main.rs` voltou a `0db5b5b1…`,
  conferido com `cmp`.
- A mesma reversão também reprova o teste do M4. A frase de tempo dele foi
  acertada para nomear as duas causas, e indicar este teste como o que as
  separa.

### M4 — `parando` contava o `encerrando` inteiro

- **O vermelho, antes do conserto:**
  `despedir_da_sessao_conta_e_espera_so_a_geracao_que_revogou` ficou vermelho
  com «… contou também a instância de uma sessão anterior que nunca
  confirmou: … mods=2», e a rodada levou 2,01 s (o teto inteiro). O teste põe
  em `encerrando` uma instância de uma geração anterior que nunca confirma, e
  uma da geração atual que confirma em 30 ms noutra thread.
- **O conserto:** `parando` conta só as instâncias de `encerrando` da geração
  fechada. **Escolhi a geração que o `revogar` devolve**, menos um: é a
  geração que ele fechou, e `revogar_em` a calcula assim, inclusive com a
  tabela envenenada. A linha INFO e a espera usam a mesma conta. O doc diz
  por quê.
- **Verde:** `mods=1`, de volta em menos de 1 s, e a instância antiga continua
  em `encerrando`.
- **Reversão** (`mods.encerrando.len()` de volta): vermelho com `mods=2`, em
  2,01 s. O `main.rs` voltou a `0db5b5b1…`, conferido com `cmp`.
- Depois das duas reversões, a frase de tempo do M4 foi acertada (M3) e o
  `cargo fmt` reformatou os testes: o commit tem o `main.rs` em `f337eac1…`,
  com o binário inteiro verde (273).

### M5 — teto zero no guarda, e o doc de «bloqueia»

- **Medido:** com `encerrar_e_esperar(std::time::Duration::from_millis(0))` no
  `main.rs`, os dois guardas passaram (2 de 2).
- **O conserto (`72b57732`):** `teto_da_espera` lê o número de
  `Duration::from_millis(N)` ou `Duration::from_secs(N)` depois de
  `encerrar_e_esperar(` no corpo sem comentários. O guarda exige um número
  maior que zero. Qualquer outra forma dá `None` e reprova: um teto que o
  guarda não sabe ler é um teto que ele não confere.
- **A prova:** com o teto zero, `a_despedida_espera_o_quadro_sair` reprova:
  «a despedida deixou de esperar a thread da conexão com um teto que não seja
  zero (lido: Some(0) ms)». O `main.rs` voltou a `e57b33fb…`, conferido com
  `cmp`.
- **O doc de `encerrar_e_esperar`:** bloqueia até `teto` quando sozinha; com
  duas chamadas ao mesmo tempo, a segunda espera a primeira soltar o cadeado e
  depois o próprio teto, até o dobro quando a thread não termina.

### M6 — o (c) lia com `timeout(Duration::ZERO)`

- **Medido antes de consertar**, com um teste provisório no `seele-core`
  (tirado depois, `enlace.rs` conferido em `4290ed0d…`):
  - 500 avisos numa fila do `tokio` 1.53.1, a mesma `mpsc` do `Enlace`, lidos
    com `timeout(Duration::ZERO, rx.recv())` numa volta só;
  - resultado: **500 de 500**, no corpo do `#[tokio::test]` (o `block_on` em
    que o (c) roda) e numa tarefa, com e sem `unconstrained`.
  - Com o orçamento gasto, o `Timeout` consulta o relógio sob o mesmo
    orçamento e devolve `Pending`: a tarefa cede e a leitura continua, em vez
    de devolver `Elapsed`.
  - **A premissa da revisão não se confirmou neste `tokio`.** Não há vermelho
    a mostrar, nem antes nem por reversão.
- **O conserto, mesmo assim, como pedido (`29a31553`):** `ja_na_fila`
  embrulha a leitura em `tokio::task::unconstrained`, e o comentário diz a
  medida. Ele tira do teste a dependência desse detalhe do `Timeout`: se ele
  mudar, uma fila cheia não vira «a fila esvaziou».
- **O (c) com `ja_na_fila`:** verde. A reversão da rodada 0 (sem o
  cancelamento antes do aviso) continua vermelha, 3 de 3: «o quadro Some(36)
  estava na fila de quem saiu **depois** do `Encerrado`: …».

### O que rodou, na HEAD da rodada

- `--test saida_com_despedida -- --test-threads=1`: 3 de 3. O filho em 157 ms
  (150 deles estacionado); (a) e (b) em 0,5–0,6 ms.
- `--test tela_por_um_par -- --test-threads=1`: 19 de 19.
- `--test bateria_interna -- --test-threads=1`: 7 de 7.
- `-p seele-core --lib`: 445.
- `-p seele-ffi --lib`: 126, mais o 1 ignorado de antes.
- `-p seele-app`: `--bin` 273, `--test encerramento` 2 e `--test frontend` 268.
- `cargo fmt --all -- --check` limpo; o clippy dos quatro crates, com `-D
  warnings`, limpo. O `cargo doc` do `seele-core` só tem os avisos que já
  estavam lá.
- As corridas de conformidade terminaram antes da pausa da integração. Nenhuma
  começou depois dela.

### O pedido da F1: vermelho que não se repete perto dos 20 s

Não vi nenhum. Todos os vermelhos de ~20 s desta rodada e da anterior foram
reversões de propósito, e se repetiram em toda rodada em que foram medidos.
Nenhum verde ficou perto do `IDLE_TIMEOUT`.

## Rodada 2 de conserto (FIX_BASE `6688241f`)

A re-revisão deu os oito itens da rodada 1 como resolvidos e achou dois pontos
no mesmo arquivo, `apps/seele-app/tests/encerramento.rs`. Os dois estão em
`aa9e7f4c`. Sem conformidade (pausada): rodaram `--test encerramento`,
`--bin seele-app despedir`, `--test frontend`, o fmt e o clippy do app.

Antes de cada reversão, o `main.rs` estava em `f337eac1…` e o guarda em
`53b37f92…`; depois de cada uma, o `main.rs` voltou conferido com `cmp`, e o
guarda continuou idêntico à cópia.

### 1. (Importante) `fechar_a_janela_avisa_o_servidor` casava com comentário

- **O vermelho medido, com o guarda de `6688241f`:** as três trocas do braço
  do `ExitRequested` deixaram os dois guardas verdes (2 de 2 cada):
  - **R1**, a da re-revisão: `// despedir_se(handle);`;
  - **R2**: a chamada só num comentário de fim de linha
    (`let _ = handle; // despedir_se(handle);`);
  - **R3**: a chamada fora do braço, com o `if` vazio e `despedir_se(handle);`
    depois dele.
- **O conserto:** `fecho_do_app_run` recorta o `main.rs` do `app.run(|` até a
  chave que fecha `main` na coluna zero, passa só esse trecho pelo leitor de
  comentários e tira dele o corpo do fecho, contando chaves.
  - O guarda acha o `RunEvent::ExitRequested` ali, pula o padrão `{ .. }` do
    `matches!` até o `)` dele, e exige `despedir_se(handle)` dentro do bloco do
    `if`.
  - O `main.rs` inteiro não passa pelo leitor: ele tem `'"'` e strings cruas.
- **A prova:** R1, R2 e R3 vermelhas, com «o braço do `ExitRequested` não chama
  mais `despedir_se(handle)` fora de um comentário: a última janela fecha e o
  processo morre sem avisar o servidor, ou avisa a cada evento em vez de no
  fim.»

### 2. (Menor) O doc de `sem_comentarios` prometia o que ela não fazia

- **O vermelho medido, com o guarda de `6688241f`:** `let _aspa = '"';` antes
  de `// despedir_da_sessao(...)` e de um `disconnect()`, no corpo de
  `despedir_se`, deixou os dois guardas verdes.
- **A primeira tentativa, descartada pela medida:** procurar `'"'`, `r"` e
  `r#"` como texto no trecho recortado. O guarda reprovou o próprio `main.rs`
  certo, porque a frase «saída do app: MODs nativos mandados parar» termina em
  `r"`.
- **O conserto:** a detecção mora no leitor.
  - `sem_comentarios` passou a devolver `Result`. Fora de aspas e de
    comentário, ela recusa o caractere `'"'` (e `'\"'`) e a string crua que
    começa palavra: `r"`, `r#…#"`, `br"` e `br#…#"`, pela função
    `string_crua`. O `r#tipo`, identificador cru, passa.
  - `sem_comentarios_lido` faz o guarda reprovar com frase própria, e os dois
    recortes (`corpo` e `fecho_do_app_run`) passam por ela. O doc diz isso.
- **A prova:** a reversão da re-revisão ficou vermelha nos dois guardas, com
  «o corpo de `despedir_se` tem `'"'`, e o leitor de comentários deste guarda
  não lê esse trecho com segurança: um comentário pode passar por código. …».
  Também vermelhas, com a mesma frase para o perigo de cada uma:
  `let _crua = r"x";` e `let _crua = r#"x"#;`.
- **Conferido:** nenhum dos três recortes lidos tem algum desses perigos no
  código: os corpos de `despedir_se` e de `despedir_da_sessao`, e o fim de
  `main` desde `app.run`. Os dois guardas passam no `main.rs` de `f337eac1…`.

### O que rodou

- `--test encerramento`: 2 de 2.
- `--bin seele-app despedir`: 4 de 4.
- `--test frontend`: 268.
- `cargo fmt --all -- --check` limpo; `cargo clippy -p seele-app --all-targets
  --all-features -- -D warnings` limpo (o aviso do linker é ambiental).

## Rodada 3 de conserto (FIX_BASE `0c6133d1`)

Dois furos Menores no leitor de comentários de `encerramento.rs`, os dois
consertados em `c429302c`.

- **O vermelho medido, com o guarda de `0c6133d1`:** as três reversões da
  re-revisão deixaram os dois guardas verdes (2 de 2 cada):
  - **N1:** `/* era: /* nota */ despedir_da_sessao(…); */` no corpo de
    `despedir_se`, com só um `disconnect()` no código;
  - **N2:** `/* era: /* nota */ despedir_se(handle); */` no braço do
    `ExitRequested`;
  - **C:** `let _c = cr"\";` antes de `// despedir_da_sessao(…)`. O leitor não
    via a string C crua, o `\` escapava a aspa que fecha, e o comentário
    ficava «dentro das aspas».
- **O conserto:**
  - o laço do `/*` conta a profundidade, e o comentário só termina quando ela
    volta a zero, como no Rust;
  - `string_crua` aceita o prefixo `c` antes do `r` (`cr"`, `cr#"`), como já
    aceitava o `b`. Ela devolve o prefixo exato, e a frase do guarda o nomeia.
- **A prova, com o guarda novo** (cada reversão sobre o `main.rs` de
  `f337eac1…`, de volta conferido com `cmp` depois; o guarda em `820e9c2d…`):
  - N1: os dois guardas vermelhos, «a despedida da janela deixou de passar por
    `despedir_da_sessao`, …»;
  - N2: `fechar_a_janela_avisa_o_servidor` vermelho, «o braço do
    `ExitRequested` não chama mais `despedir_se(handle)` fora de um
    comentário: …»;
  - C: os dois vermelhos, «o corpo de `despedir_se` tem `cr"`, e o leitor de
    comentários deste guarda não lê esse trecho com segurança: …».
- **As reversões das rodadas anteriores continuam vermelhas**, todas rodadas de
  novo contra o guarda novo:
  - da rodada 0: `despedir_se` de volta ao `disconnect()` com `sleep`;
  - da rodada 1: A, B e o teto zero;
  - da rodada 2: R1, R2, R3, `'"'`, `r"x"` e `r#"x"#`.
- **O que rodou:**
  - `--test encerramento`: 2 de 2;
  - `--bin seele-app despedir`: 4 de 4;
  - `--test frontend`: 268;
  - fmt e o clippy do app, limpos.
  
  Sem conformidade, que segue pausada.
