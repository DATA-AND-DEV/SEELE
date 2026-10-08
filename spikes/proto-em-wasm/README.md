# spike `proto-em-wasm` — o codec do `seele-proto` roda no `wasm32`?

**Descartável.** Responde a medição 2 do
[ADR 0057](../../docs/adr/0057-o-celular-entra-pelo-navegador.md) («O que fica
pendente», item 2) e morre com a resposta. O `seele-proto` é da F1 e entra aqui
só por caminho, só lido.

## A pergunta

O ADR pede `cargo check -p seele-proto --target wasm32-unknown-unknown`, e diz
que, se falhar, o codec em `wasm` reabre. **Um `check` que passa diz que o
crate compila, e não que ele funciona.** No `wasm32-unknown-unknown`, o
`SystemTime::now()` e o `ToSocketAddrs` compilam e entram em pânico na
primeira chamada. Por isso, além do `check`, este spike roda o codec dentro de
um `.wasm` e compara os bytes com os do nativo.

## Como roda

```sh
rustup target add wasm32-unknown-unknown
cd spikes/proto-em-wasm
cargo build --release --target wasm32-unknown-unknown --lib
cargo build --release
./target/release/spike-proto-em-wasm > nativo.hex
node prova.mjs target/wasm32-unknown-unknown/release/spike_proto_em_wasm.wasm > wasm.hex
cmp nativo.hex wasm.hex
```

O `.wasm` sai sem cola: nenhum `wasm-bindgen`, nenhuma importação. O Node o
instancia com `{}`.

`vetores()` (em `src/lib.rs`) faz o seguinte:
- codifica com `control::encode` um `Hello` (com convite, apelido com acento e
  ideograma, e chave de 32 B), um `Response` de 64 B, um `LeaveVoiceRoom` e um
  `Challenge` de 32 B;
- decodifica cada quadro com `control::decode` e confere que a mensagem voltou
  igual;
- troca o byte de versão e guarda a recusa;
- passa um link `seele://` e um `https://` por `uri::analisar`.

O resultado sai num buffer com o prefixo de 4 bytes do `frame.rs`.

## O que mediu — 08/10/2026

No Mac (aarch64), com o Rust 1.97.1 do `rust-toolchain.toml`, no `d4eb6f1`
(`versao/0.16`). O `seele-proto` é o mesmo da v0.15.1 publicada (`54f542f`): a
`versao/0.16` ainda não mudou nada nele.

| pergunta | resposta |
|---|---|
| `cargo check -p seele-proto --target wasm32-unknown-unknown` | **passa**, saída 0, sem aviso, 3,8 s do zero (o alvo foi instalado agora) |
| há no código de produto do crate chamada que compila e entra em pânico no `wasm32`? | **não**. `std::net` só aparece como tipo de dado (`SocketAddr`, `IpAddr`) e `std::time` só como `Duration`. O único `ToSocketAddrs` está num `#[test]` (`uri.rs:1224`) |
| o codec roda no `.wasm` e escreve os mesmos bytes que o nativo? | **sim**: os dois buffers são iguais byte a byte (o SHA-256 do hex é `05680610c41d049f…` nos dois) |
| a ida e volta `encode` → `decode` devolve a mesma mensagem dentro do `wasm`? | **sim**, nas quatro |
| a recusa de versão é a mesma? | **sim**: `UnsupportedVersion { found: 108, expected: 8 }` nos dois |
| o `uri::analisar` roda no `wasm`? | **sim**. Ele analisa o `seele://` e recusa o `https://` com `EsquemaDesconhecido`, como faz hoje (`uri.rs:686-691`) |
| tamanho do `.wasm` (`opt-level = "s"`, LTO, `panic = "abort"`) | **270 386 B**, ou **73 015 B** com gzip -9. O número inclui a formatação `Debug` que o spike usa para comparar texto; o crate de produto não a usaria |

**Conclusão:** o item do codec no ADR 0057 («o codec é o `seele-proto`
compilado para `wasm32`») não reabre. A medição 2 está feita.

## O que não mede

- **O `wasm-bindgen`.** Este `.wasm` fala por ponteiro e tamanho. A cola que o
  crate `seele-web` vai precisar, e o `wasm-bindgen-cli` sob o `cargo deny`,
  continuam em aberto no ADR.
- **O Safari e o Chromium.** O motor que rodou foi o V8 do Node 24.16. Nenhum
  dos dois navegadores deve se comportar diferente com um `.wasm` sem importação
  nenhuma, mas isso não foi medido aqui.
- **O protocolo da 1.0.** A F1 vai mudar o `Hello`, os ordinais e o S2. A prova
  vale para o crate de hoje. O teste de produto (os bytes-ouro do `wasm` contra
  os do nativo, no plano da F4) é que vale para o crate que sair.
- **O `state.rs` e o resto do núcleo** («quanto do núcleo vai para `wasm`», EM
  ABERTO no ADR).
