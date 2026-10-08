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
| 0.3 · preparar a medição 3 (ALPN no iPhone) | a fazer | `spikes/alpn-no-mesmo-socket/` |
| 0.4 · spike do executor de MODs | a fazer | `spikes/mods-no-navegador/` |
| 0.5 · passos do dono, medições 3 a 6 | a fazer | `F4/medicoes-do-dono.md` |
| 0.6 · pedidos ao contrato de fio | depois da 0.2 e da medição 3 | `F4/pedidos-ao-fio.md` |

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
