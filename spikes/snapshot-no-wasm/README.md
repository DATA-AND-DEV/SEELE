# O retrato da sessão no navegador (Tarefa 2.2 da F4)

**A pergunta** (ADR 0057, «quanto do núcleo vai para `wasm`», e o achado 2 da
área 4 da matriz da F8): o `Room` do `seele-core` compila e roda no
`wasm32-unknown-unknown`? E quanto custa um retrato (`Snapshot`) de sala cheia
no navegador? A casca pede um a cada evento da sessão; com uma tela assistida a
30 quadros por segundo, são 30 retratos por segundo.

## O que este spike roda

**O texto de verdade, sem cópia à mão.** O `build.rs` lê do `seele-core`:
- o `state.rs` inteiro, **menos o `teto_de_video`** (o único método que puxa o
  vídeo, e que só os testes dele chamam) **e os testes**;
- o `SessionInfo`, recortado do `client.rs` (o resto do arquivo puxa o quinn e o
  tokio);
- o `assento.rs` inteiro, pelo caminho (`#[path]`).

Cada corte tem de casar uma vez só, ou o build para dizendo qual.

**A sala cheia** (`src/sala.rs`) é montada como a sessão a monta: o `adopt` do
`SessionInfo` (e não o `apply(Session)`, que daria «pessoa 1» como apelido) e um
`PersonJoined` mais um `PersonState` por pessoa, enchendo as salas até o teto
de 250 (`MAX_VOICE_ROOM_LIMIT`).

**O retrato** (`src/retrato.rs`) espelha o `Snapshot` do `seele-ffi`, com os
mesmos campos, nomes e projeção (`presentes_de`, `voice_rooms_of`, `lines_of`,
`transmissoes_de`). Os campos de áudio, enlace e caminho saem com o que o
desktop diz sem voz: o tamanho do JSON é o de verdade. O custo medido é o do
retrato montado e serializado no `wasm`, mais a cópia e o `JSON.parse` na
página, que é o que a ponte faria a cada `invoke("snapshot")`.

## Medido em 09/10/2026

`cargo build --release --target wasm32-unknown-unknown`: **147 KB, e nenhuma
importação**.

A média de lotes de 300 retratos (o relógio dos navegadores sem isolamento anda
de 0,1 ms em 0,1 ms, e a mediana por volta some nele):

| pessoas (salas, canais) | JSON | Chromium 151 | Safari do Simulador (iOS 27) | Node (V8) |
|---|---|---|---|---|
| 10 (3, 5) | 3 KB | 0,013 ms | 0,017 ms | 0,024 ms |
| 50 (5, 10) | 10 KB | 0,031 ms | 0,043 ms | 0,061 ms |
| **250 (10, 20): uma sala cheia** | **41 KB** | **0,114 ms** | **0,167 ms** | 0,238 ms |
| 500 (10, 40) | 80 KB | 0,226 ms | 0,323 ms | 0,478 ms |
| 1000 (10, 40) | 156 KB | 0,454 ms | 0,607 ms | 1,069 ms |

**A conta:** a 30 retratos por segundo, uma sala cheia custa de 3,4 ms a 5 ms
por segundo de CPU (de 0,3% a 0,5%), e mil pessoas no servidor, de 14 a 18 ms.
**O produtor não pesa**, e a saída exata do `ScreenFrame` (devolver o último
retrato enquanto só chegaram quadros) não é necessária por causa dele.

**O que esta medida não alcança:**
- **o iPhone de verdade.** O Simulador roda o JavaScriptCore do iOS na CPU do
  Mac. Um aparelho seria mais lento, de algumas vezes, e a conta acima ainda
  cabe com folga; mas é estimativa, e não medida;
- **o redesenho da casca.** O `atualizar()` refaz a tela a partir do retrato, e
  esse custo é do DOM, e não do produtor. Se 30 retratos por segundo pesarem,
  é ali que pesam, e a saída exata do `ScreenFrame` continua sendo a resposta
  (ela poupa o redesenho também).

## Os achados

1. **O `Room` é puro.** Fora o `teto_de_video`, o `SessionInfo` e o
   `assento.rs`, nada nele toca relógio, rede, disco, áudio ou vídeo, e ele
   compila e roda sem importação nenhuma.
2. **No navegador, as chaves do `HashMap` são fixas.** O `RandomState` do
   `wasm32-unknown-unknown` não tem de onde tirar aleatoriedade: o primeiro
   hash de um mesmo valor é igual em duas instâncias do módulo, e no Chromium e
   no Safari (`hash_de_prova`). O `Room` guarda pessoas, salas e telas por ids
   que **o servidor escolhe**: um servidor hostil poderia mandar ids que colidem
   e deixar a aba quadrática. No navegador, os mapas por id do servidor têm de
   ser `BTreeMap` (determinístico, sem hash, e a ordem não muda o retrato: a
   projeção já ordena o que mostra).
3. **O `adopt`, e não o `apply(Session)`.** Dobrar a mensagem `Session` põe
   «pessoa N» no lugar do apelido de quem entrou; o desktop usa o `adopt`, e o
   navegador tem de usar o mesmo.

## Rodar

    cargo build --release --target wasm32-unknown-unknown
    node medir.mjs target/wasm32-unknown-unknown/release/snapshot_no_wasm.wasm
    python3 servir.py chromium
    SEELE_SIMULADOR=<UDID> python3 servir.py simulador
