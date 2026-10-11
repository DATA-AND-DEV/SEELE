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

### 09/10, a D-web-3: o web app também no PC, com a casca de desktop

- **`b727691b`, o plano e a revisão das recusas.** As 68 recusas da matriz da
  F8 (51 com nome e 17 com frase) foram revistas para o PC em
  `F4/ponte-no-pc.md`. O lado de cada comando foi decidido pelo `main.rs`: o
  que vai por `session.connection` é protocolo, e o que mexe na `hospedagem`
  local só existe para quem hospeda.
  - **Passam a ser atendidas:** a moderação **e a administração pela conexão**
    (canais, salas, nome e ícone do servidor), a tecla de falar com a página em
    foco, a tela cheia, a supressão de ruído, a saída de som e o
    compartilhamento de tela (3.10).
  - **Ficam recusadas:** hospedar e a portaria local, instalar e habilitar
    MODs, as versões, o atualizador e o que é do sistema.
  - A marca ganha dois valores, a Tarefa 3.10 entra, e a emenda D-web-3 do ADR
    fica marcada para quando o plano fechar.
- **`1e540682`, a medida da 3.10** (`spikes/tela-do-navegador/`): o Baseline
  restrito 4.0 em Annex-B (`avc1.42E028`) sai no Chromium 151 e no Safari
  27.0.1 do Mac, e o OpenH264 do desktop lê 60 de 60. **O `isConfigSupported`
  do Safari mente:** em quatro dos seis perfis que ele diz aceitar, sai zero
  pedaço e o `flush` não volta, sem erro. O botão só se oferece depois de uma
  prova com prazo. O Chrome, o Edge, o Firefox e o som do `getDisplayMedia`
  estão nos passos do dono.
- **Pendente de envio**, com as mensagens entre sessões pausadas:
  - **à orquestradora:**
    - o resumo acima;
    - um pedido de confirmação: estendi a «moderação» à administração pela
      conexão, que é da mesma natureza (protocolo com papel);
    - o achado do Safari;
  - **à F8:**
    - o critério da marca de dois valores;
    - o teste do microfone fora da sala (fica ou não na casca de desktop);
    - o `descrever_arquivo`, que no navegador recebe um `File`, e não um
      caminho;
  - **à F1**, o pedido de revisão do `354cf7c8` (registrado acima).
- **A Etapa A da F8, aprovada em 09/10.** Os valores finais da marca estão no
  plano (3.1), conferidos no §9.2 do spec dela (`0.16/celular`, `04c0f1a8`).
  O servidor que a Tarefa 19 dela pede para os 28 roteiros é um `seeled` com
  `h3` (as minhas 2.4 e 2.6), e não o servidor de mentira da 3.4. **Pendente
  de envio à orquestradora e à F8:** essa dependência, e que a F4 entrega o
  como subir e o link quando a 2.4 e a 2.6 existirem.

- **A oitava integração (`f7fb8ab9`), trazida em `36e3d307`.** O `0.16/web` não
  entrou nela, porque o módulo de quadros espera a revisão da F1, **que a
  orquestradora pediu**: esse item sai da lista do que falta mandar. Na `ui/`
  que entrou, a ponte só põe a marca (o `CELULAR` é da F8), e atende o
  `volumes_locais` novo do I-b da F5. As duas coisas estão no plano (3.1).
- **As mensagens pendentes foram mandadas** quando o envio entre sessões
  voltou: à orquestradora, o resumo da D-web-3, a confirmação pedida sobre a
  administração, o achado do Safari e o servidor dos roteiros; à F8, a marca,
  o PC e os três pontos a combinar (o teste do microfone, o
  `descrever_arquivo` e o servidor da Tarefa 19 dela).
- **Aviso de fila da orquestradora:** o lote W da F5 (I-4) mexe no
  `tela-boot.js` antes da 3.4, e prende a credencial ao alvo depois do
  `NicknameTaken`. A F4 não mexe nesse arquivo até lá, e confere o contrato
  quando o W entrar (no plano, 3.4).

## 09/10, noite: a `ui/` liberada, e o executor entra nela (3.2, passo 6)

- **A nona integração (`ffd8e4fb`) trazida em `4755c08a`**, sem conflito. A
  orquestradora liberou a `ui/` para a ponte (3.1) e para o passo 6 da 3.2,
  com um dono por arquivo: `base.js:36-37` e o `mods-runtime.js` são da F4; o
  `tela-boot.js` espera o lote W da F5.
- **O executor está na `ui/`**: `mods-no-navegador.js` e `executor-de-mod.html`
  em `apps/seele-app/ui/`, a bancada em `apps/seele-app/bancada/`.
  - **a escolha pelo lugar** é o `executorDoMod`, em `mods-runtime.js`. A
    `base.js` mudou **uma linha fora do 36-37**, a da chamada (`:678`), porque
    ela nomeava o `executorNativo`; o guarda do `frontend.rs` que exigia esse
    nome passou a exigir o `executorDoMod`. A F8 não toca esse trecho
    (conferido no `0.16/celular`);
  - **na carga, nada**: o ouvinte de `message` saiu do topo do arquivo e nasce
    com o `iframe`. A bancada de Node carrega os dois arquivos sem `window`, e
    um ouvinte de volta ao topo a reprova;
  - **o `<script>` vai depois do `base.js`**: o guarda de ordem do
    `frontend.rs` reprovou a primeira posição (antes dele), e o guarda das
    funções sem declaração reprovou o `addEventListener` solto. Os dois
    pegaram coisas reais, e mudou o código, não o guarda.
- **A bancada no CI**: no job `bancadas-de-mod`, depois do `cargo xtask web`,
  com o Playwright instalado ali. O `check_runtime.rs` lia só o job
  `bancadas`; passou a ler os dois, e um teste novo segura que a bancada vem
  depois do motor compilado e com o caminho que o `xtask web` usa. O
  `cargo xtask web` e o `rustup target add` ganharam `!cancelled()`, para uma
  bancada de Node vermelha não pular o motor.
- **Medido:** a bancada passa no Chromium no lugar novo (nove casos, motor
  recompilado: o `.wasm` local era de antes da última mudança do motor); a de
  Node, seis corridas; `tests/web.rs` 4, `frontend.rs` 282, `celular.rs` 22;
  clippy `-D warnings` limpo em `seele-app` e `xtask`.
- **As nove reversões**, cada uma reprovando o seu guarda: `allow-same-origin`;
  `connect-src` na CSP; o script do `iframe` mudado sem o hash; a `base.js`
  pedindo o nativo pelo nome; a escolha invertida; o ouvinte de volta ao topo;
  a bancada antes do motor; a bancada sem `!cancelled()`; a bancada fora do CI.

## 09/10, noite: a ponte, lote A (3.1)

- **A moldura da ponte** (`9e895748`, `52338c4f`): `ui/ponte-web.js`, o
  primeiro script do `<head>`. O `base.js:36-37` não precisou mudar.
- **Achado ao escrever: o duble.** As bancadas de `bancada/` e as ferramentas
  da F8 (`tools/carga-da-casca.py`, `tools/bancada-do-celular.py`) põem um
  `window.__TAURI__` antes do primeiro `<script src=`, que passou a ser a
  ponte. A ponte trocaria o duble e a marca escolhida pela bancada. Regra: **um
  `__TAURI__` sem o `__TAURI_INTERNALS__` é um duble**, e a ponte não faz
  nada. O `executorDoMod` passou a ler a decisão da ponte, e não o Tauri.
- **Os 160 comandos**: 89 atendidos nas duas cascas, 33 só no PC, 34
  recusados, 3 neutros e o `disconnect`. 24 com corpo; 99 em `AINDA_A_FAZER`,
  com a tarefa ao lado.
- **Medido no Chromium, sem duble e sem toque**: a casca chama sozinha 18
  comandos nas duas cascas (os 17 da F8 mais o `estado_da_porta`), e nenhum
  volta recusado.
- **Achado na integração, que não é da F4:** na `versao/0.16` de `ffd8e4fb`,
  os `celular-*.js` usam o `CASCA` que a F8 declara no `base.js:112` do
  `0.16/celular`, e esse ponto ainda não entrou. Sem a ponte, nada punha a
  marca num navegador de verdade, e ninguém via. Com a ponte, a casca do
  celular estoura cinco vezes na carga (`CASCA is not defined`), e a
  `ponte-sem-gesto.cjs` reprova no `0.16/web` sozinho. **Junto com o
  `0.16/celular`, o `0.16/api-6` e o `0.16/lotes`** (numa árvore temporária,
  fora do branch), a cobertura fecha nos 160, e as três bancadas passam.
- **Dois guardas do `frontend.rs`** ganharam a ponte como
  exceção, com o porquê: a ordem de carga (ela é lida pelo `base.js`, e não o
  contrário) e os tipos de imagem escritos (a cópia dela é conferida contra o
  `seele_ffi` em `tests/web.rs`).
- **Um conflito evitado:** a F3 acrescenta bancadas no fim do job `bancadas`
  do `ci.yml`, no mesmo ponto; a `ponte-sem-gesto.cjs` foi para o começo.
- **Doze reversões**, cada uma reprovando o seu guarda ou bancada.
- **Na árvore junta, os guardas de Rust**: `web.rs` 10, `frontend.rs` 284,
  `tokens.rs` e `fontes.rs` passam. O `celular.rs` reprova um,
  `o_texto_do_sistema_alcanca_todo_tamanho_do_desktop`: o `celular-texto.css`
  gerado da F8 fica atrás dos quatro CSS do desktop que o `0.16/api-6` muda.
  É do encontro da F3 com a F8 (o `0.16/web` não muda CSS nenhum), e a F8 o
  regera com `SEELE_REGERAR_TEXTO=1`. Avisado à orquestradora.

## 09/10, noite: a ponte, lote B (o que mora na origem)

- **Cinco comandos saem de `AINDA_A_FAZER`**: `escolher_apelido_local`,
  `consentir_previa`, `esquecer_previa`, `previas_consentidas` e
  `abrir_no_navegador`. Ficam 94.
- **As regras são as do desktop**, lidas do código e não da matriz:
  - o apelido aparado e sem tab nem quebra (`Preferences::set_nickname`), e
    vazio apaga;
  - o domínio em minúsculas ASCII, uma palavra só, e um subdomínio de um
    liberado já está liberado (`consentir_previa`, `previa_consentida`);
  - só `http` e `https`, até 2048 bytes, sem espaço nem controle
    (`endereco_que_pode_sair`), numa aba nova com `noopener,noreferrer` e sem
    `await` antes do `open`.
- **Gravar que falha é dito**: `NaoGravei` no apelido, como o desktop, e a
  frase no consentimento. Sem armazenamento (navegação privada, cota), a
  escolha não fica, e quem chama sabe.
- **Medido:** a `ponte-web.cjs` com 12 provas; `web.rs` 10, `frontend.rs` 282
  e `celular.rs` passam. **Cinco reversões**, cada uma reprovando o seu caso:
  abrir sem conferir, abrir levando a página junto, o apelido sem aparar, o
  subdomínio consentido de novo e a gravação que falha em silêncio.

## 09/10, noite: a ponte, lote C (o arquivo por alça)

- **Três comandos ganham corpo**: `escolher_arquivo`, `guardar_colado` e
  `descrever_arquivo` (este só no PC). O `enviar_anexo` passa do lote C para
  a 2.6: ele lê o arquivo da alça e o sobe pela sessão. Ficam 91 a fazer.
- **As quatro condições da `F4/ponte-no-pc.md`**, cada uma com prova:
  1. o mapa tem teto (16 alças, e 2 imagens coladas, que estão todas na
     memória) e solta tudo no fim da sessão. A casca não avisa quando desiste
     de um anexo (`tirarAnexo` não chama a ponte), e por isso o teto solta as
     mais velhas;
  2. uma alça desconhecida ou solta volta `NotConnected`, como o caminho que
     não existe no desktop; a casca diz «NÃO CONSEGUI LER ESSE ARQUIVO»;
  3. o seletor abre dentro da chamada, sem `await` antes do `click()`;
  4. desistir devolve `null` pelo `cancel`, e a reversão que tira o `cancel`
     reprova a bancada pela promessa pendurada.
- **O arraste é só do PC**: a ponte ouve o `drop` do documento quando o que se
  arrasta são arquivos, guarda cada um sob uma alça (até o teto, para a do
  primeiro não sair no meio) e emite o `tauri://drag-drop` que a casca ouve.
  Arrastar texto para um campo continua sendo do navegador.
- **As regras do desktop presas por guarda**: a tabela do `tipo_alegado`, a
  extensão da colagem e o teto de 25 MiB do `guardar_colado`
  (`as_regras_do_arquivo_da_ponte_sao_as_do_desktop`). O tipo sai da extensão,
  e não do `File.type`.
- **Medido:** a `ponte-web.cjs` com 15 provas; `web.rs` 11. No Chromium, o PC
  carrega limpo com o arraste instalado. **Nove reversões**, cada uma
  reprovando a sua prova.

## 09/10, noite: a ponte, lote D (a saída de som e a tela cheia)

- **Quatro comandos ganham corpo**, só no PC: `saidas`, `saida_escolhida`,
  `escolher_saida` e `tela_cheia`. Ficam 87 a fazer.
- **A saída de som**: o `enumerateDevices`, sem os pseudo-aparelhos
  (`default` e `communications` repetem um de verdade; o padrão é o do mesmo
  grupo do `default`) e sem os ids vazios de antes da permissão do microfone.
  Onde não há `setSinkId`, a lista é vazia. A escolha mora na origem; numa
  sessão, a voz a aplica.
- **A tela cheia** é a Fullscreen API, pedida dentro do clique; pedir duas
  vezes não pede de novo, e um pedido recusado é dito sem lançar na casca.
- **O teste do microfone foi para a 3.3**, porque é o mesmo portão da voz.
- **A lacuna do Esc**, registrada no plano: ele sai da tela cheia do navegador
  sem a casca saber, e o modo cinema dela fica ligado. É da casca (F8).
- **Medido:** a `ponte-web.cjs` com 17 provas, só em Node: a lista de
  aparelhos e a Fullscreen API não foram exercitadas num Chromium.
  **Cinco reversões**, cada uma reprovando a sua prova.

## 09/10, noite: o eco no navegador (pedido da F8)

- **A F8 perguntou** se a voz do PC liga o `echoCancellation`, para o aviso de
  fone da casca larga. A resposta: a voz-web ainda não existe, e pedir o
  cancelador não é cancelar (a nossa voz toca pelo Web Audio, e não por um
  `RTCPeerConnection`). A larga fica com «USE FONE» até a medida; a F8 também
  tirou do celular a afirmação do P9, que não tinha medida (`63089fe7`, no
  `0.16/celular`).
- **A página da medida** (`spikes/eco-no-navegador/`): um tom de 1 kHz pelo
  Web Audio e o nível dele no microfone, com e sem o cancelador, em quatro
  rodadas alternadas; o veredito diz «sem laço» quando o tom não chega ao
  microfone, em vez de inventar um número. Conferida no Chromium mudo e com
  microfone falso: quatro rodadas, o cancelador aplicado igual ao pedido, e
  «sem laço».
- **A medida de verdade é do dono** (alto-falante e microfone de verdade), nos
  três navegadores do Mac e no iPhone: os passos estão em
  `F4/medicoes-do-dono.md`. O Simulador não serve: ele usa o áudio do Mac.
- **Também neste trecho:** a regra do duble ficou escrita na ponte com o
  porquê; os 28 roteiros e o G3 da F8 (o desktop igual pixel a pixel no
  Chromium e no WebKit) passaram no `0.16/web` em `792dc2a4`.

## 09/10, noite: a 2.2 liberada, e a medida do retrato

- **A orquestradora liberou a 2.2 e a 3.3**, nesta ordem. Os lotes B, C e D da
  ponte entram na décima primeira integração; o `index.html` não precisa de
  commits isolados.
- **Mapeado** (por um agente de leitura): o `Room` é puro fora de três peças
  (o `SessionInfo`, num arquivo com o quinn; o `assento.rs`, puro; e o
  `teto_de_video`, que só os testes chamam). A projeção do `Snapshot` e os
  tipos dele moram no `seele-ffi` (`lib.rs` e `types.rs`), os arquivos mais
  disputados com a F5.
- **Medido no spike** (`3e733d3e`): os números estão no plano (2.2). Uma sala
  cheia custa 0,11 ms no Chromium e 0,17 ms no Safari do Simulador; o produtor
  não pesa.
- **Achado: as chaves do `HashMap` são fixas no navegador**, conferido por um
  hash de prova igual em duas instâncias do módulo e nos dois motores. Com ids
  escolhidos pelo servidor, isso é uma porta para deixar a aba quadrática; a
  proposta é `BTreeMap` nos seis mapas do `Room`.
- **A proposta de corte foi à orquestradora**, em duas fases (o `seele-core`
  primeiro; o `seele-ffi` numa janela dela). Esperando a resposta.

## 09/10, noite: a 2.2, fase 1 (o `seele-sessao`)

- **A orquestradora liberou a fase 1** e os seis mapas em `BTreeMap` também no
  desktop; a fase 2 espera a sala viva da F7. A saída do `ScreenFrame` saiu do
  plano, porque o produtor não pesa.
- **O corte** (`4e1139fc`): o `crates/seele-sessao`, só com o `seele-proto`.
  O `state.rs` e o `assento.rs` foram para lá, e o `SessionInfo` saiu do
  `client.rs` sem tocar no aperto de mão, onde a P8 parou. O `seele-core` os
  reexporta, e nenhum outro arquivo do workspace mudou. No texto movido,
  mudaram só o `use` do `SessionInfo`, dois links para o vídeo, que viraram
  texto, e o `teto_de_video`, que ficou no `tela.rs` como o trait
  `TetoDaSala`, com os dois testes dele de texto igual. O `assento.rs` é byte a
  byte o mesmo.
  - **conferido:** 58 testes no `state.rs` mais 2 no `tela.rs` são os 60 de
    antes, e o `assento.rs` tem os mesmos 15; o `seele-core` inteiro passa; o
    `cargo xtask web` compila o `seele-sessao` para o navegador; o
    `check_deps.rs` o prende ao protocolo (teste novo);
  - **reversões:** o `seele-audio` pendurado nele reprova o `check-deps` e a
    compilação para o navegador; o teto sem a perna dos espectadores reprova o
    teste movido.
- **Os seis mapas em `BTreeMap`**, num commit à parte: o `seele-sessao`, o
  `seele-core` inteiro e o `seele-ffi` (166 testes, com os do retrato) passam.
  Um guarda novo (`tests/sem_hash_nos_ids_do_servidor.rs`) reprova um
  `HashMap`, um `HashSet` ou um `RandomState` no crate, e a reversão (o mapa
  das telas de volta ao `HashMap`) o reprova. No spike, o retrato ficou igual
  ou mais rápido (250 pessoas: 0,107 ms no Chromium, 0,117 ms no Safari do
  Simulador), e o `.wasm` foi de 147 KB para 204 KB.
- **Três comentários citam o caminho velho do `state.rs`**, e não são da F4:
  `ui/base.css:160`, `ui/tela-sessao.js:35` e
  `crates/seele-conformance/tests/sincronia.rs:34`. Ficam para a fila da
  orquestradora.

## 09/10, noite: a 3.3, lote B (o `seele-voz`) e as preferências da voz na ponte

- **O corte do `seele-voz`** (`a4330b2d`): o `gate.rs`, o `jitter.rs`, o
  `mixer.rs` e o `netsim.rs` movidos byte a byte do `seele-audio`, com as três
  constantes e o teste delas; o `seele-audio` os reexporta. Passam o
  `seele-voz` (77), o `seele-audio` e o `seele-core` inteiros e a
  `qualidade-da-voz.cjs`. Reversões: o `seele-proto` pendurado reprova o
  `check-deps`; a libopus pendurada reprova a compilação para o navegador.
- **O mixer em `BTreeMap`** (`1c915683`), medido no nativo como a orquestradora
  pediu: 765 a 792 ns por quadro de oito fontes, contra 788 a 877 com o
  `HashMap`, em rodadas alternadas. Um guarda contra mapas de hash no crate
  pegou um `HashSet` nos testes do `netsim.rs`.
- **A emenda do ADR 0057** (`beb1e851`): a exceção em JavaScript não foi
  preciso, com os números.
- **O relógio de mídia e a tabela de fontes moram dentro do laço do
  `voice.rs`.** Escrevê-los no `seele-web` seria uma segunda implementação. A
  extração deles para o `seele-voz` foi perguntada à orquestradora, e o lote A
  espera a resposta.
- **As preferências da voz na ponte**, enquanto isso: o modo (sem escolha, o
  VOZ), o microfone escolhido e a lista de microfones, a sensibilidade e a
  redução de ruído (em milésimos, como o desktop) com os controles da voz
  derivados pela regra do desktop sem sessão, a tecla de falar do PC e a
  permissão do microfone pela Permissions API. Um guarda prende a abertura
  sem supressão ao `seele_ffi`. Seis reversões; a da tecla no celular não
  reprovava de primeira, porque a bancada começava sem tecla guardada, e a
  bancada foi corrigida. Ficam 78 a fazer na ponte.
- **Para a F8:** a permissão negada pelo navegador volta
  `NegadaPeloNavegador` (o achado 3 da matriz), que a tabela
  `PERMISSAO_DE_MICROFONE` da casca ainda não tem; até ela ter a frase, o
  bloco fica escondido.

## 09/10, noite: a 3.3, o relógio de mídia e a tabela de quem fala no `seele-voz`

- **Liberado pela orquestradora**: as duas metades da mídia saem do laço do
  `voice.rs` para o `seele-voz`, como estruturas puras com o relógio passado
  por argumento, e o desktop passa a usá-las.
- **O laço não roda em teste** (um aparelho do cpal e uma conexão QUIC). Então
  o «teste gravado antes» foi feito assim: uma tabela fixa de eventos com os
  valores esperados escritos à mão, produzida por uma réplica das instruções
  do laço; um segundo teste confere que cada instrução replicada está no
  `pipeline` com o mesmo texto (mudar o laço reprova). Depois da extração, o
  corpo do teste da sequência é idêntico (conferido por texto), quem produz é
  a estrutura chamada sozinha, e o guarda passa a exigir que o laço a use e
  não faça conta própria.
- **A metade que manda** (`e3ce87aa`, `9c0d8f61`): o `RelogioDeMidia` (um
  quadro passou, um pacote saiu, o salto) e o `carimbo_do_quadro`, movido com
  a documentação. O `Voice::salto_do_relogio` ficou como atalho, e os testes
  antigos não mudaram. Reversão: sem o salto de dois quadros, reprovam o teste
  gravado e mais três testes antigos da reabertura.
- **A metade que recebe** (`ae50d6f0`, `59c569e5`): a `TabelaDeFontes<T>` e a
  `SILENCIO_ATE_ESQUECER_MS`, movida com a documentação. Reversão: o próprio
  SSRC deixando de ser descartado reprova o teste gravado.
- **Passam** o `seele-voz`, o `seele-audio`, o `seele-core` (452), o
  `seele-ffi` (166), o clippy, a compilação para o navegador e a
  `qualidade-da-voz.cjs`. **A conformidade de áudio por arquivo espera o aviso
  da orquestradora** do fim da bateria da décima.
- **A seguir, o lote A**: a fronteira da voz no `seele-web`, com o cabeçalho, o
  relógio e a tabela (a alça opaca no lugar do SSRC) pelo `wasm`.


## 09/10, noite: a 3.3, lote A (a voz pela fronteira do `seele-web`)

- **`af0bea6d`.** Quem manda: `voz_envio`, `voz_reabrir` e o datagrama
  montado no `.wasm` com o `RelogioDeMidia`. Quem recebe: `voz_recepcao`,
  `voz_chegou` e `voz_esquecer` com a `TabelaDeFontes`, e a página recebe uma
  alça u32 no lugar do SSRC (ADR 0057).
- **Provas:** quatro testes nativos repetem as sequências gravadas do desktop,
  e o `xtask/web/voz.mjs` roda os mesmos casos dentro do `.wasm`, como passo
  do `cargo xtask web`. As duas reversões reprovaram nos dois lugares.

## 10/10, madrugada: a 3.3, lote C (a voz no navegador)

- **Passo 1, a régua do portão no `seele-voz`** (`f1f5dfd5`):
  `GateConfig::dos_controles`. A conversa e o teste do microfone do desktop
  montavam a régua cada um com o seu corpo, e a voz do navegador seria o
  terceiro. Os dois do desktop passam a chamá-la, e o guarda exige que nenhum
  monte a sua (as agulhas vêm de `concat!`, para não casar com o próprio
  texto). Três reversões.
- **Passo 2, o portão, o jitter e o mixer pela fronteira** (`b550f452`):
  - **o portão mora no `Envio`, na thread da página**, e não no worklet: cada
    quadro solto precisa do carimbo do mesmo relógio, e vai para o
    `AudioEncoder`, que não existe no worklet. O `capturou` segue a ordem do
    laço do desktop, e o `reabrir` monta outro portão com a mesma régua, o
    mesmo modo e a mesma tecla;
  - **o ganho automático do desktop não roda no navegador.** O DSP da
    plataforma foi aceito pelo ADR, e o ganho do navegador vem **antes** do
    portão, que é a ordem que o doc do `ganho` desaconselha. Não está medido;
  - **o jitter e o mixer moram na `Reproducao`, no worklet**, sobre som já
    decodificado: o `AudioDecoder` não existe lá nem oculta quadro. Um quadro
    que falta vira silêncio com fade de 2,5 ms (a emenda do ADR 0057 ao ADR
    0010);
  - nove testes nativos e o `voz.mjs` dentro do `.wasm`. Sete reversões
    reprovaram no nativo, e três delas também no `.wasm`.
- **Passo 3, a `ui/voz-web.js` e o `ui/voz-worklet.js`** (`47a46827`), na
  ordem da rodada 5. A bancada `voz-no-navegador.cjs` (no job
  `bancadas-de-mod`, depois do `cargo xtask web`) prova, no Chromium com
  microfone falso e um laço local que troca o SSRC:
  - a voz de ponta a ponta;
  - o mudo e o isolamento;
  - **os controles depois de uma reconexão** (o Step 1 da 3.3): mudo,
    isolamento, modo e régua. O mudo escondia o isolamento, e por isso eles
    são conferidos um de cada vez;
  - o `messageerror` dito.
  O `.wasm` compila no worklet em 2 a 6 ms.
- **Duas medidas que mudaram o código:**
  - **o `usedtx: true` atrasa a voz 340 ms no Chromium.** O `AudioEncoder`
    segura 17 quadros antes do primeiro; sem o DTX, zero. A voz do navegador
    fica sem DTX, porque o portão fechado já não manda nada. A bancada reprova
    acima de três quadros no codificador;
  - **o Chromium devolve o instante do codec um microssegundo antes**
    (259 999 por 260 000), no codificador e no decodificador. O casamento
    passou a ser pelo número do quadro.
- **Os guardas da janela:**
  - a `voz-web.js` entra na `index.html` depois da `mods-no-navegador.js`.
    **Para a F8**, a dona da fila da `index.html`: uma linha e um comentário;
  - o worklet e a tag `<audio>` da voz são exceções conferidas;
  - a página não nomeia mais SSRC nem datagrama: a exportação virou
    `voz_empacotar`.
- **O que falta na 3.3:**
  - o ganho por pessoa, que precisa do `Room` (o `ganhos_a_aplicar` do
    `seele-ffi` no `.wasm`, com a 2.6);
  - os comandos da ponte (entrar e sair da voz, que esperam a sessão, 2.6);
  - o teste do microfone do PC;
  - a medida do eco, que é do dono;
  - a conformidade de áudio, que espera o aviso da orquestradora.
- **O guarda da hospedagem** (`c6fca33e`) aprende o `servidor_do_mod` da F3 e
  o `sala::` da F7, a pedido das duas. Provado por injeção, porque nesta
  branch nenhum corpo os usa ainda.
- **Passo 4, o teste do microfone do PC pela ponte** (`832be563`): o portão do
  `Envio` na ativação por voz, à parte da voz da sala. O monitor toca pela
  mistura do worklet, e fechar fora da sala para as faixas. A bancada abre e
  fecha duas vezes pela ponte de verdade, na casca larga. Reversões: fechar sem
  `track.stop()`, o monitor calado e o fechar de antes da ponte reprovam a
  bancada; o teste fora da ativação por voz e sem a régua reprovam os nativos.
- **A sala viva da F7**: os comandos e os canais novos, com o lado proposto,
  em `F4/ponte-no-pc.md`. Decididos quando a casca os chamar.

## 10/10: a 3.4 no que não depende do domínio (a identidade do navegador)

- **Pedida pela orquestradora** depois do passo 4 da 3.3, sem tocar no
  `tela-boot.js` e seguindo o contrato do guarda da credencial do lote R.
- **A `ui/identidade-web.js`** entrega à tela da F8 o objeto de leitura e as
  ações: `entrar`, `aceitarOsMods`, `agoraNao`, `copiarLink` e `instalar`. O
  fragmento é lido e apagado na carga. O token e o link vivem só em
  `segredos`, na memória da tentativa. A chave Ed25519 nasce no primeiro
  `entrar`, depois do `persist()`, e o servidor é guardado no IndexedDB pela
  impressão só quando a entrada dá certo. Quem já é conhecido entra pela
  chave, sem gastar o convite.
- **Quem conecta é passado na criação**: o `connect` da ponte é da 2.6.
- **O servidor responde `CredentialRejected` igual para todo convite que não
  vale** (spec 08, `admissao.rs`). `convite-gasto` e `convite-vencido` se
  separam pela validade do link.
- **A bancada `identidade-no-navegador.cjs`** (no job `bancadas-de-mod`,
  depois do `cargo xtask web`): os três casos do plano, os estados, o que segue
  e o que encerra a tentativa, a ordem do `persist()`, o fragmento e o
  servidor já conhecido.
- **O guarda da credencial** passa a ler a `identidade-web.js` (cada linha que
  nomeia `credencial`, `segredos` ou `linkDaCarga`). A chamada de `conectar`
  dela é a quarta que leva credencial, com o porquê.

## 10/10: a décima primeira integração trazida, e a conformidade da voz

- **A `versao/0.16` de `330f3ab4`** (a décima primeira, verde de primeira)
  entrou em `a6af3539`. Teve um conflito só, no guarda da credencial do
  `frontend.rs`, e os dois lados ficam: a `recusaDoApelido` e a parte 6 do lote
  W da F5, e a identidade do navegador. Conferido no código que a identidade
  segue o contrato do W. Na árvore integrada passam os guardas da janela, o
  `tests/web.rs`, os crates da F4, o `empacotamento`, o `check-deps`, o
  `cargo xtask web` e as bancadas da ponte, da voz e da identidade. A
  `ponte-sem-gesto` ficou limpa nas duas cascas: o `CASCA is not defined` do
  celular era a ordem da integração da F8.
- **A conformidade que toca o laço da voz, liberada pela orquestradora, um
  arquivo por vez**: passam o `voz_na_reconexao`, o `soak_audio`, o
  `troca_de_aparelho`, o `som_da_tela_segue_o_aparelho`, o `acceptance_m2`, o
  `ejetar`, o `desassentar` e o `sala_cheia_nao_se_escuta` (38 testes). É a
  prova de que a régua do portão no `seele-voz` (`f1f5dfd5`), que veio depois do
  que a bateria cobriu, não mudou a voz do desktop.

## 10/10: a fase 2 da 2.2, o retrato comum

- **Passo 1, a mudança de lugar** (`d0f72c7c`). Os tipos de vista, as
  conversões e as seis funções da projeção saem do `seele-ffi` para
  `seele-sessao/src/retrato.rs`, e o `seele-ffi` os reexporta pelo
  `seele-core`. Os guardas de texto do `frontend.rs` que liam o `types.rs`
  leem os dois arquivos juntos (`tipos_da_ponte`). Um `#[cfg(test)]` no topo do
  `lib.rs` cortou o «programa» que quatro guardas de texto do `seele-ffi` leem,
  e os quatro reprovaram; o import foi para dentro dos módulos de teste.
- **Passo 2, os dois produtores** (`f087a500`). O `RetratoDaSala` com os nomes
  de campo do `Snapshot`; a `Sessao` do `seele-web`, que entra pelo `adopt` e
  dobra quadros; o `seele_ffi::sem_rede`, atrás da feature `conformidade`. O
  `dois_produtores_de_snapshot.rs` compara os 19 campos depois de cada uma de
  onze mensagens, com o `TemaMudou` que chega e o que sai. Três reversões no
  navegador reprovaram, cada uma no campo e no passo certos.
- O `.wasm` do `seele-web` com a `prova` ficou em 985 KB (era 885 antes da
  voz e da sessão).
- **A fechar** depois da décima segunda, como a orquestradora pediu: os campos
  novos do `Snapshot` da sala viva, cada um com o seu lado.

## 10/10, tarde: a décima segunda, o jitter zero da M3 e o fecho da fase 2

- **A décima segunda trazida** (`bb1944bc`). O conflito foi o `bare_shared`:
  fica o `sem_rede::shared`, com o `impressao_do_servidor` do lote X. Ela não
  trouxe campo novo ao `Snapshot`.
- **A pausa da M3** (cerca de duas horas sem compilar): o código foi escrito, e
  rodado e commitado só depois dela.
- **O jitter zero da M3**, achado lendo, sem rodar. O falante de medição
  gravava `PROTOCOL_VERSION` (9) no cabeçalho de mídia, e desde o `f3aaf9ce`
  (fora da v0.15.1) o primeiro byte da voz é 1. Quem ouvia jogava fora toda a
  voz dele, e o jitter ficava no zero inicial. A orquestradora consertou o
  falante e mediu de 1,06 a 1,58 ms. As duas pontas que ficavam no produto
  mudam em `67bfeee1`:
  - o laço de voz conta, por motivo, os datagramas que o cabeçalho recusa, e
    diz o primeiro no log (`ler_o_datagrama`, `CabecalhosRecusados`);
  - o `encode` do `MediaHeader` grava sempre o número do layout. A F1 revisa.
  O teste da cadeia de recepção vai dos bytes à telemetria, e com o byte do
  falante reprova como a M3 viu.
- **O fecho da fase 2 da 2.2** (`1c712014`): cada campo do `Snapshot` tem um
  lado (19 da sala, 14 medidos, 3 contados), e um campo novo reprova até
  alguém decidir.
- **O guarda da hospedagem** (`935ea588`) lê `.sala` e `.toques` com fronteira
  de palavra. A marca de fim de linha deixava passar calada a leitura numa
  linha só (provado pela injeção).
- **A fronteira da identidade com a F8**, fechada pela orquestradora e escrita
  durante a pausa:
  - `identidadeDaPagina()`;
  - o `{ ok: true, resposta, persistencia }` (achado da F8);
  - `conhecidos()`, `entrarNoConhecido` com `link-vencido`, e `lembrarONome`;
  - o guarda da credencial com as linhas novas, e nenhum outro script tocando
    no link da carga;
  - a bancada com a página inteira sem casca injetada (pedido da orquestradora
    na décima segunda), os servidores do aparelho e a identidade da página.

## 10/10, noite: a revisão do E54 da F5, e uma tentativa por vez na identidade

- **A revisão do E54** (`e05e1681`, pedida pela orquestradora): as duas pontas
  da F5 não conflitam com o que eu tenho.
  - O `soltar_a_entrada` entra em `COMANDOS_DA_PONTE` e no `AINDA_A_FAZER` com
    a 3.4.
  - O `corpo_do_comando` de `tests/web.rs` passa a achar as formas genéricas.
- **A mesma corrida, do lado do navegador**, achada lendo a minha `entrar`.
  Ela atravessava quatro esperas lendo a tentativa de agora. Medida antes do
  conserto, no caso 12 da bancada, com o link novo aberto no meio:
  - durante o `persist()`, a velha entrava **no servidor do link novo**, com o
    token dele e o apelido da velha;
  - o aceite de MODs da velha entrava no servidor novo, sem apelido;
  - a velha aceita ou recusada encerrava a nova, e a aceita guardava o servidor
    dela com a chave da nova (nula);
  - com o banco respondendo fora de ordem a dois links, o token do segundo ia
    ao servidor do primeiro.
- **O conserto:**
  - a `vez` conta cada tentativa encerrada e é conferida depois de cada
    espera;
  - o `abrir` só põe os segredos depois de o banco responder;
  - a velha aceita fica guardada com a chave dela e é solta (`soltar`, no
    produto o `soltar_a_entrada` da ponte);
  - o desfecho é o `sem-tentativa`, que a tela já tem.
  Na tela da F8 de hoje a corrida não se alcança (a de conectar não tem
  ação), mas o contrato é da identidade.
- **Dez reversões**, cada conferência tirada sozinha, e as dez reprovam no
  cenário dela. Duas passavam verdes na primeira rodada:
  - sem a conferência do `persist()`, a velha fazia nascer a chave da nova;
  - sem a da chave, ela chamava o `connect` do servidor novo com a chave nula,
    e o servidor de mentira estourava antes de anotar.
  A bancada passou a anotar a entrada sem chave e a conferir a ordem do
  `persist()` e da chave, e as duas reprovam.

## 10/10, noite: o que ninguém pegou vai ao `seele.log` (pedido da orquestradora)

- **O achado da F7:** o botão vermelho não fechava o app. O `destroy()` da
  janela era recusado pela ACL do Tauri, a recusa caía numa promise sem ninguém
  olhando, e nada ia ao `seele.log`: a página não tinha ouvinte global.
- **O `base.js` ganha dois ouvintes**, o de `unhandledrejection` e o de
  `error`. Eles mandam ao `registrar_da_janela` (`onde=sem-dono`, nível
  `erro`) o motivo, com a pilha quando há e com o lugar quando o solto não é um
  erro. A tela não muda, e o console segue dizendo o que já dizia.
- **O teto:** dez linhas por minuto. As seguintes são contadas e ditas numa
  linha só, no fim do minuto. É no JS também porque, no Rust, a linha da janela
  sem MOD gasta do balde do catálogo, o dos outros avisos da janela.
- **O roteiro** `tools/roteiros/recusa-sem-dono.js` reprova sem os ouvintes, e
  cada peça foi provada por reversão (as seis reprovam).
- **Os 35 roteiros que já existiam** seguem verdes, e nenhum deixa linha
  `sem-dono`, lidos por uma cópia do aparelho que diz o que o duble recebeu.
  Também verdes: `check-runtime`, `ponte-sem-gesto` (o `registrar_da_janela`
  não está entre os comandos que a carga chama sozinha), a bancada da voz, a da
  identidade e os guardas da janela (294), do celular (8) e da web (12).
- **Achado latente no aparelho dos roteiros:** o `carga-da-casca.py` lê o
  relato do console até a primeira aspa dupla. Um relato com aspas antes de um
  `REPROVADO` esconde a reprovação, e o aparelho diz «carga limpa» com saída 0
  (medido com um roteiro de duas linhas). Hoje não esconde nada: lidos até o
  `", source:`, os 36 dão o mesmo resultado.
