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
