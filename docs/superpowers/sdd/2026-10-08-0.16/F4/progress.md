# F4 — o web app do celular: andamento

**Branch:** `0.16/web`, saído de `versao/0.16` em `d4eb6f1`. O pedido dizia
`c0337b2`; a `versao/0.16` andou um commit desde então, só de documento (o
achado do ESTILO no spec-mestre).

**Conferido antes de medir** (`CLAUDE.md`): a última versão publicada é a
v0.15.1, do `54f542f`. Entre o `54f542f` e o `d4eb6f1`, nada mudou em
`crates/`, `apps/`, `xtask/` nem `tools/`. Tudo o que se mede aqui é o código
publicado.

**Plano:** `docs/superpowers/plans/2026-10-08-0.16-F4-web.md`.

## Etapa 0 — medições e spikes

| tarefa | estado | onde |
|---|---|---|
| 0.1 · medição 2: `seele-proto` em `wasm32` | **feita**: o `check` passa, e o codec roda no `.wasm` com os mesmos bytes do nativo | `spikes/proto-em-wasm/README.md` |
| 0.2 · medição 1: o Ed25519 do Safari no `verify` | **feita** no Mac e no Simulador: passa. O iPhone fica com o dono (`prova.py iphone`) | `spikes/ed25519-do-navegador/README.md` |
| 0.3 · preparar a medição 3 (ALPN no iPhone) | **feita**; a bancada (Chromium e Simulador) mediu, e o iPhone fica com o dono | `spikes/alpn-no-mesmo-socket/README.md` |
| 0.4 · spike do executor de MODs | **feito**: recomenda **AB**. O custo no iPhone fica com o dono (`prova.py iphone`) | `spikes/mods-no-navegador/README.md` |
| 0.5 · passos do dono, medições 3 a 6 | **feita**, com a página da voz preparada para vir de uma origem estática | `F4/medicoes-do-dono.md` |
| 0.6 · pedidos ao contrato de fio | **feita** (a `Certidao` só entra depois da medição 3 do dono) | `F4/pedidos-ao-fio.md` |

## Registro

### 08/10

- Plano escrito. As Etapas 2 e 3 estão marcadas: o que espera a F1, o Plano 3,
  a F3, a F5, a liberação e o dono.
- **Medição 2 feita.** `cargo check -p seele-proto --target wasm32-unknown-unknown`
  passa (saída 0, sem aviso, 3,8 s). O alvo foi instalado com `rustup`. Como um
  `check` não prova que funciona, o spike `proto-em-wasm` roda o codec num
  `.wasm` sem cola, no Node 24.16. Quatro mensagens, a recusa de versão e o
  `uri::analisar` produzem os mesmos bytes que o nativo. O `.wasm` tem 270 KB
  (73 KB com gzip).
- **Medição 1 feita** (`250aa83`). O verificador tem as linhas de
  `session.rs:550-599`, com o `ed25519-dalek` 3.0.0 do `Cargo.lock`. Rodou no
  Chromium 151, no Safari 27.0.1 do Mac e no Safari do Simulador iOS 27.0:
  - o Safari assina de forma **aleatória** (10 distintas em 10) e o Chromium
    não;
  - nos três, as 184 assinaturas passam no `verify` e no `verify_strict`, e as
    80 adulteradas são recusadas;
  - **o S2 `rótulo ‖ fp ‖ nonce` cabe no navegador**;
  - a `CryptoKey` relida do IndexedDB depois de recarregar assina e verifica.

  À parte: o `crypto.subtle.verify` do WebKit recusa a própria assinatura
  sobre 0 bytes, e o `ed25519-dalek` a aceita. Não toca o produto.
- **Medição 3 preparada** (`a3a79b1`). O spike do ALPN assina o longo e o
  curto com uma chave só, ganha o controle A2, o MUDO e a porta FECHADA, e
  registra cada conexão. A página do iPhone (`iphone.py`) foi percorrida no
  Simulador pelo IP da LAN. Na bancada:
  - a mesma chave abre nos dois navegadores, e 500 hashes também;
  - no Safari, a recusa e a porta fechada chegam iguais (3–7 ms, erro sem
    texto); o silêncio leva 30 s;
  - o servidor desiste de um navegador morto em 24,6 s (os 20 s mais o
    keepalive).
- **O spike dos MODs** (candidatas A, B e AB, no Chromium 151 e no Safari do
  Simulador, com o navegador fechado entre as duas fases):
  - **A e AB passam nos cinco critérios.** Nenhuma canária tocou, nada
    sobreviveu, a página não travou (13 ms no pior caso), e o laço sem fim e a
    memória foram cortados;
  - **B reprova em rede e em API**: 8 pacotes STUN saem pela CSP nos dois
    navegadores, e o `webrtc 'block'` é ignorado pelos dois. A página trava
    3 s, e a memória não tem teto;
  - **recomendação: AB**;
  - **três achados para o produto**:
    - no `wasm32-wasip1`, o teto de memória do QuickJS não existe sem o
      `RustAllocator`: o MOD alocou 128 MiB com o teto de 8 MiB;
    - o prelúdio pressupõe as voltas do anfitrião;
    - o `build.rs` do `rquickjs-sys` baixa o wasi-sdk sem checksum.
- O plano ganhou a regra «na tela, nunca linha» (aviso da orquestradora, vindo
  da F6) e a forma AB na Tarefa 3.2.
- **A página da voz vem de uma origem estática** (`f025fc7`). O servidor e o
  hash vêm do fragmento, que é guardado e apagado. A identidade conta as
  aberturas, e o web app aceita o link colado. Conferida no Chromium e no
  Simulador a partir de `localhost:8099`, e a `prova.py` antiga passa inteira.
- **Os passos do dono** estão em `F4/medicoes-do-dono.md`: a medição 1 no
  iPhone, a 3, a 4, a 5, a 6 e o custo dos MODs no iPhone. A origem provisória
  basta para medir.
- **O AB foi aceito** pela orquestradora (spec-mestre `00935dac`). As duas
  condições viraram tarefas com guarda: o `RustAllocator` (3.2, Step 2) e o
  wasi-sdk verificado (3.2b).
- **Achado de passagem:** um `spike-voz-no-navegador` de 07/10 ainda escuta a
  4433 no checkout principal (PID 70937). Não é desta sessão e ficou como
  estava.
- **Os pedidos ao contrato de fio** (`F4/pedidos-ao-fio.md`), escritos contra
  o `0.16/protocolo` em `99caa92`:
  - **o S2 cabe no navegador**: medido de novo com o rótulo de verdade da F1,
    50 de 50 nos três navegadores. Fica um pedido de doc em `Server::impressao`,
    porque no `h3` o TLS apresenta o curto;
  - **os hashes futuros viajam sozinhos**, em `HashesDoNavegador`: 60 janelas
    ocupam 2,5 KB;
  - **os DERs vão só os da janela vigente**, numa `Certidao` antes do `Hello`
    (1,4 KB), e não 60 DERs (21,6 KB, acima do teto);
  - o link `https` é o corpo do `seele://` no fragmento, mais `wh=` e `ate=`;
  - o item 20 já está feito pela F1 (`quadro.rs`);
  - fica um pedido ao Plano 3: conferir o apelido antes de gastar o convite.

### 08/10, depois da primeira integração

- `0.16/web` trazido a `versao/0.16` em `4fc31e9` (fast-forward), com o
  protocolo 9 da F1.
- **Tarefa 2.1, o núcleo seguro do `seele-web`** (`c19758e`):
  `codificar_do_cliente` e `decodificar_do_servidor` atrás de uma fronteira de
  JSON. Seis testes contra o `tests/fio_1_0.txt` da F1, com as provas por
  reversão. O crate entra em `RULES` só com o `seele-proto`, e o `check-deps`
  vale para 13 crates. Compila para `wasm32-unknown-unknown`.
- **Tarefa 3.2b, a conferência do wasi-sdk**: `cargo xtask wasi-sdk` baixa,
  confere o SHA-256 antes de descompactar e imprime o caminho para
  `WASI_SDK`. O wasi-sdk não publica checksum, então os hashes foram fixados na
  primeira descarga. Feito de ponta a ponta com o motor do spike.
- **Espera decisão:** a fronteira com o JavaScript (do `seele-web`) e o motor
  de MODs precisam de `unsafe`, e o workspace o proíbe. O precedente é o do
  `seele-video` e do `seele-instalador` (`deny` em vez de `forbid`, a folha de
  lints copiada e o guarda `folha_de_lints`). O guarda do `RustAllocator` espera
  o crate do motor, e portanto essa decisão.

### 08/10, a exceção do `unsafe` e os guardas do `.wasm`

A orquestradora aprovou a exceção no molde do 0041 e do 0043, com quatro
condições, e as quatro estão feitas:
1. **`deny` na folha, `allow` só em `fronteira.rs`**, nos dois crates. O
   `folha_de_lints` passa a esperar quatro folhas próprias. Provado: com
   `forbid` no motor, ele reprova.
2. **Nenhum `unsafe` fora da fronteira**: `xtask/tests/fronteira_do_celular.rs`.
   - Um `unsafe` no `lib.rs` não compila, pelo `deny`.
   - O mesmo com `#[allow(unsafe_code)]` compila, e o guarda o nomeia. O
     `folha_de_lints` sozinho não pegava isso: ele confere a folha, e não onde
     os `allow` estão.
3. **`// SAFETY:` em cada bloco**: `#![deny(clippy::undocumented_unsafe_blocks)]`
   dentro de cada `fronteira.rs`, e não na folha, que só pode divergir no
   `unsafe_code`. Provado: um bloco sem o comentário reprova no clippy.
4. **A emenda do ADR 0057**, com o porquê e o precedente, e a linha do índice.

Também feito:
- **O motor de MODs** (`crates/seele-motor-web`): o motor seguro, rodado no
  nativo, e a fronteira só no `wasm`. O guarda do `RustAllocator` roda dentro do
  `.wasm`: com `Runtime::new()`, o MOD aloca 64 blocos de 2 MiB e o guarda
  reprova.
- **As importações fixas do motor**: 14, todas da lista. Uma nova reprova até
  alguém a revisar.
- **Os bytes-ouro dentro do `.wasm`**: as 37 mensagens do cliente e as 46 do
  servidor cruzam a fronteira de verdade com o JSON passando pelo JavaScript
  (`xtask/web/ouro.mjs`).
- **O passo sem `WASI_SDK`**: o `.cargo/config.toml` aponta a variável para uma
  pasta que não existe. Provado: sem a linha, o `build.rs` baixou 464 MB sem
  conferir, e o passo reprovou.
- **`cargo xtask web`** roda os sete passos, e o job de MOD do CI passou a
  rodá-lo, com os dois alvos instalados. Um guarda prende isso.

**A descarga que fixou os hashes do wasi-sdk:**
- data: 08/10/2026;
- origem: `https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-24/`;
- arquivos: `wasi-sdk-24.0-arm64-macos.tar.gz` (108 199 177 B,
  `aeae9993…f51b0b3a`) e `wasi-sdk-24.0-x86_64-linux.tar.gz` (118 879 731 B,
  `c6c38aab…d4167a0bc5`);
- os tamanhos batem com os que a API do GitHub informa para o release;
- o release não publica checksum, e uma busca pelos hashes não achou segunda
  fonte.

### 08/10, depois da segunda integração (`dc001a0`)

- `0.16/web` trazido a `versao/0.16` em `dc001a0` (fast-forward).
- **As duas falhas que a bateria achou no meu branch**, consertadas pela
  orquestradora em `5122f39`:
  - **o clippy em `fronteira_do_celular.rs`.** Rodei o clippy com
    `--all-targets`, mas **antes** de acrescentar o último teste àquele arquivo,
    e não rodei de novo depois;
  - **o `todo_crate_do_produto_pode_falar_no_log`.** Ele percorre os crates do
    workspace, e eu não o rodei ao criar dois crates.

  Daqui em diante: `env RUSTFLAGS="-D warnings" cargo clippy -p <crate>
  --all-targets` depois da última edição, e os guardas que percorrem os membros
  do workspace sempre que um crate entra.
- **`cargo xtask web` na bateria do `publicar.sh` e no `validar` do
  `release.yml`**, a pedido da orquestradora:
  - a etapa fica ao lado do `check-runtime`, com a queixa e o conserto no
    molde das outras. O resto do script, que é da F6, ficou intocado;
  - **antes de qualquer build**, o `xtask web` diz o que falta na máquina: os
    alvos wasm (com o `rustup` a rodar), o Node, ou o hash fixado do wasi-sdk.
    Uma máquina sem hash fixado (Windows, Mac Intel) é recusada dizendo por quê.
    Seis testes, e um provado por reversão;
  - dois testes em `xtask/tests/empacotamento.rs`, no molde do
    `check-runtime`: um `xtask web` vermelho para a publicação antes de
    empacotar, com a queixa e o porquê, e o `--sem-bateria` o pula;
  - o guarda de `fronteira_do_celular.rs` passa a exigir o passo nos dois
    workflows.

### 08/10, os casos do link no `.wasm` (Tarefa 2.3)

Liberado pela orquestradora depois do `642a4d4`.
- O `seele-web` ganha:
  - `link_do_navegador`, sobre o `analisar_no_navegador` da F1;
  - na fronteira, `analisar_link`, **sempre com a `ORIGEM_DO_NAVEGADOR` do
    produto**: a página não escolhe a origem;
  - `analisar_link_em`, a versão com origem, só para teste.
- Os 16 casos da F1, um por asserção, numa tabela em JSON, passam no nativo e
  dentro do `.wasm`. O `cargo xtask web` passa a ter oito passos.
- Provado por reversão, nas duas pontas. Na do `.wasm`, a primeira tentativa de
  reversão não se aplicou: o `rustfmt` tinha quebrado a linha e a substituição
  não casou, e o «verde» dela não provava nada. Refeita contra o texto real,
  reprovou como devia.
- Clippy com `RUSTFLAGS="-D warnings"` e `--all-targets` depois da última
  edição. Ele pegou a indexação em `serde_json::Value` no teste novo, já
  consertada.

### 09/10, a sexta integração e o executor do navegador

- `0.16/web` trazido a `cb426b19` (fast-forward). A bateria pegou um teste
  meu: `pelo_fio_1_0.rs` contava as **linhas** dos bytes-ouro contra o
  `CONHECIDAS`, e a F1 acrescentou um segundo exemplar do `Alert`. A
  orquestradora o consertou em `3f8babe0` (conta as variantes). Era um guarda
  que casava com o arquivo, e não com o fio.
- **Achado de paridade, levado à orquestradora:** o motor do celular roda o
  mesmo prelúdio do desktop, mas não as ~1000 linhas do `executor.rs` fora
  dele. Ficam de fora:
  - o rastreador de rejeições (P51-01);
  - o balde do console (P51-14);
  - os tetos de relógio;
  - a frase do teto e o `Interrompido`.

  Recomendei extrair a volta para um crate comum. Espera a ordem dela, porque o
  `executor.rs` teve 10 commits desde 07/10, e a F3 e os lotes C, D e E ainda
  vão mexer nele.
- **O executor JS, pronto para a `ui/`** (em
  `spikes/mods-no-navegador/executor/` até a liberação):
  - `executorNoNavegador` com a forma do `executorNativo`: os quatro verbos, o
    `entregar` que devolve a recusa, o `encerrou` com prazo e conclusão tardia,
    e a ordem da colheita;
  - um `iframe` opaco (`executor-de-mod.html`) com a CSP por `<meta>` e o hash
    do script. O anfitrião do motor mora **dentro** dele, como uma função que
    vira o `blob:` do Worker;
  - os tetos das filas do desktop (64 mensagens e 256 KiB) nas três pontas: o
    Worker recusa o `postar`, a página recusa o `entregar`, e o `iframe` mata
    o Worker que passar do teto que o motor garante.
- **Achado medido, consertado:** no Safari do Simulador iOS 27.0, um Worker do
  `iframe` opaco abre o `caches`, e o que ele grava **sobrevive** à abertura
  seguinte da página. Um Worker aninhado o abre de novo, e a CSP não alcança
  armazenamento.

  Só importa se um MOD sair do motor, mas é armazenamento que sobrevive à
  sessão. Um preâmbulo no `blob:` tira `caches`, `indexedDB`, `Worker`,
  `SharedWorker` e `navigator.storage`, confere, e falha fechado.

  Com ele, nos três navegadores (Chromium 151, Safari 27.0.1 do Mac e Safari
  do Simulador), nenhuma canária tocou e nenhuma porta abriu.
- **A bancada** (`executor-no-navegador.cjs`, no Chromium) tem nove casos:
  - o MOD comum;
  - o encerramento;
  - o laço cortado, com a página parada 11 ms no pior caso;
  - as duas filas;
  - a parada antes do motor;
  - o MOD hostil no motor;
  - o Worker fugido;
  - o Worker que inunda.

  As oito provas por reversão reprovam, cada uma no caso dela
  (`spikes/mods-no-navegador/README.md`, adendo de 09/10).
- **Para a integração na `ui/`:**
  - o guarda `the_page_loads_only_files_that_are_shipped` exige que todo `.js`
    da `ui/` seja carregado pelo `index.html`. Foi por isso que o anfitrião
    saiu de um `.js` próprio;
  - medido numa cópia temporária, que não ficou na árvore, com os dois
    arquivos na `ui/`: dos 270 guardas do `frontend.rs` e do `tokens.rs`,
    269 passam. O que reprova é justamente esse, até o `index.html` carregar
    o `mods-no-navegador.js`;
  - a bancada precisa do `.wasm` do motor e do Playwright no mesmo job.

### 09/10, a volta comum (o `executor.rs` extraído)

Decidido pela orquestradora: extrair agora, com o `executor.rs` com a F4 até
a integração. `0.16/web` trazido a `fd4d298f` (merge `b6c7eca1`) antes de
começar.
- **`a35e78ae`, o desktop.** A volta saiu do `executor.rs` para o
  `crates/seele-motor`: folha do grafo, `forbid(unsafe_code)`, sem thread e sem
  E/S. O `rodar` virou o `Motor` (`subir`, `codigo`, `resposta`, `diagnostico`,
  `vencer`, `proximo_prazo`, `parar`), e a thread, o canal e a fila de entrada
  ficam no `executor.rs`.
  - Os 55 testes passam: 49 ficam no `executor.rs` e 6 vão para o motor (os
    que leem campos privados). Os 71 blocos do módulo são idênticos byte a
    byte, conferidos por script contra o arquivo de antes.
  - O `seele-app` troca o `rquickjs` pelo `seele-motor`, e o filtro do log
    ganha `seele_motor=info`.
  - Os leitores do prelúdio (`imagens-por-volume.cjs`, `mod_de_referencia.rs`
    e o spike) leem `crates/seele-motor/src/preludio.rs`.
  - Ao virar biblioteca, apareceu um defeito antigo: a doc da `Fila` tinha
    ficado grudada no `AVISOS_NA_FILA`.
- **`c313e3e4`, o celular.** O `seele-motor-web` perde o `rquickjs`, o
  `build.rs` e o `motor.rs`. Uma `Instancia` segura embrulha o `Motor`, e o
  `.wasm` deixa de importar o módulo `seele`; o `sched_yield` do `mpsc` entra na
  lista fixa depois de revisado.
  - A tabela `crates/seele-motor/tests/casos.json` tem 13 casos e passa no
    nativo (`casos.rs`) e no `.wasm` (`motor.mjs`).
  - **Achado medido:** a única diferença entre os alvos é o `Atomics`, que o
    QuickJS do `wasm32-wasip1` não expõe. A tabela a declara e as duas pontas a
    cobram. Se a API o quiser fora do desktop também, é decisão da F3.
  - O guarda `o_desktop_e_o_celular_embrulham_a_mesma_volta_sem_reimplementar_nada`:
    nenhum dos dois embrulhos depende do `rquickjs`.
- **`13450681`, o executor JS do spike.** O anfitrião só espera o prazo do
  motor; as frases do laço cortado passam a ser as do desktop. Os nove casos
  passam no Chromium, e os do Safari no Simulador.
- **Provas:**
  - o clippy do workspace passa com `-D warnings`;
  - o `seele-app` passa inteiro (267 de unidade e todas as suítes);
  - o `xtask` e o `check-deps` passam (15 crates);
  - a conformidade de MOD por arquivo passa: `mod_de_referencia`,
    `aceite_dos_mods`, `ponte_de_pedidos` e `imagens_de_mod` (este no
    `seele-server`);
  - o `cargo xtask web` passa nos oito passos, assim como o `check-runtime`, o
    `cargo deny` e o `cargo fmt --check`.
- **Reversões:**
  - o `rquickjs` de volta no motor do celular reprova o guarda;
  - a tabela fora do `web.rs` também o reprova;
  - sem o `RustAllocator`, o MOD passa do teto com 64 blocos no `.wasm`;
  - sem a conferência da fila no `postar`, o caso da fila reprova.
- **De passagem, não tocado:** `crates/seele-server/src/mods/mod.rs:191` ainda
  diz que o molde do `Lancado` está no `executor.rs`. É só doc, e o
  `seele-server` está com a F1.
- **A 3.5 passou para a F8** (aviso de 09/10). A marca `data-casca`, que a F8
  quer antes da primeira pintura, fica a combinar com ela, e a orquestradora
  recomenda que saia da `ponte-web.js`.

### 09/10, a tela no celular (D-celular-2) e o enquadramento no proto

- **D-celular-2:** o celular assiste à tela compartilhada, com som. Ficou como
  Tarefa 3.9 do plano (`a66005a3`), com o caminho testado de quando o aparelho
  não decodifica: o cartão diz «esta tela não abre neste aparelho», e a página
  deixa de receber a transmissão.
- **O lugar do enquadramento:** um módulo no `seele_proto::screen`, e não um
  crate novo. Aprovado pela orquestradora e pela F1, que pediu três correções:
  - o `Enquadramento` vai junto;
  - os dois leitores leem o cabeçalho por uma função só;
  - os nomes são `LidoDaTela` e `ErroDeQuadro`.
- **O teto, medido** (`94d4dfd8`, `medidas/idr-no-teto.md`): com conteúdo
  realista a 1080p, um quadro passa de 512 KiB a partir de 12,4 Mbps, e chega
  a 973 KiB a 100 Mbps. A orquestradora decidiu congelar em 4 MiB (D-fio-9),
  acima do I420 cru de 1080p.
- **`354cf7c8`, o módulo:**
  - o `Enquadramento` (portado do `seele-core` com os oito testes), o
    `LeitorDeTela` e a `ler_cabecalho_de_quadro`;
  - o guarda da ordem pelo `maior_reserva()`. O alocador contador não coube: o
    `forbid(unsafe_code)` do workspace não se afrouxa nem num teste;
  - os dois guardas do teto: no proto e em
    `seele-core/tests/teto_do_quadro_de_tela.rs`;
  - os bytes-ouro `fluxo:quadro-de-tela:*` e o teste que os lê de volta;
  - **as reversões:** sem a conferência do teto, as duas leituras reprovam;
    reservando antes de conferir, o guarda da ordem reprova; sem a regra do
    cabeçalho partido, os dois testes dela reprovam; com o teto em 512 KiB, os
    dois guardas reprovam.
- **Pendente de envio** (as mensagens entre sessões pararam até o usuário
  escrever de novo nesta sessão):
  - **à F1, o pedido de revisão do `354cf7c8`.** Ele explica a troca do
    alocador contador pelo `maior_reserva`. Ele também leva os dois pontos da
    orquestradora para a troca das cópias, sobre o descarte `GrandeDemais` do
    lado que envia (`seele-core/src/tela.rs:1347`, `:1447`): o descarte é dito
    no log e no diagnóstico, e depois dele o próximo quadro é um quadro-chave.
    Primeiro o teste que mostra como é hoje. As constantes do core e do
    servidor sobem para 4 MiB na troca;
  - **à orquestradora, o aviso de que o módulo está pronto** para a revisão da
    F1.
