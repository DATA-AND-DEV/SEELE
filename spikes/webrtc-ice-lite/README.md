# Spike: WebRTC ICE-lite com str0m contra navegadores reais (0.17)

Descartável. Mede, num navegador real e antes do porte, as hipóteses de que o
transporte da 0.17 depende (contrato D1–D5): ICE-lite, DTLS passivo, credencial
`U:U` descoberta no primeiro Binding, resposta SDP sintetizada pela página, os
canais `ctl`/`voz`/«u» e a contrapressão.

Resultado e números: [`docs/evidencias/spike-webrtc-0.17.md`](../../docs/evidencias/spike-webrtc-0.17.md).

## O que tem aqui

- `src/main.rs`, com dois subcomandos:
  - `anfitriao`: str0m ICE-lite e DTLS passivo, dono de um `UdpSocket`, com o demux pelo
    1º byte (RFC 7983). Um HTTP mínimo em 127.0.0.1 serve a página, `/fp` e `/furar`.
  - `cliente`: str0m controlling, nativo.
- `src/cripto.rs`: cópia do provedor RustCrypto de `crates/seele-enlace/src/cripto.rs`, com
  uma correção de tipos em `ecb`. O original não compilou nesta cópia: ver o cabeçalho do
  `Cargo.toml` e a evidência.
- `pagina.html`: a página.
  - Faz o `RTCPeerConnection`, o munge da oferta (ufrag = pwd = U) e a resposta sintetizada
    (`a=ice-lite`, `a=setup:passive`, `a=fingerprint`, candidato host).
  - Modos `eco`, `adulterar`, `sobe` e `desce`.
- `correr.cjs`: abre a página no Playwright e imprime JSON.
- `Cargo.lock`: as versões exatas da medição (str0m 0.24.1, dimpl 0.7.6, rcgen 0.14.8).

`node_modules/`, `pw162/` e `target/` não são versionados (ver `.gitignore`).

## Versões medidas (24/10/2026, macOS arm64)

| | versão | de onde |
|---|---|---|
| Chromium | Chrome for Testing 149.0.7827.55 (`chromium-1228`), headless | playwright **1.61** |
| WebKit | WebKit 26.5 (`webkit-2336`), headless | playwright **1.62** |
| Rust | 1.97.1 | — |

Os dois Playwright são necessários porque cada versão aponta uma revisão de navegador, e
essas duas eram as que já estavam em `~/Library/Caches/ms-playwright`. Nada foi baixado.

## Comandos

```sh
cd spikes/webrtc-ice-lite

# o Playwright, sem baixar navegador (as revisões acima já precisam estar no cache)
npm install --no-save --no-audit --no-fund playwright@1.61
npm install --prefix pw162 --no-save --no-audit --no-fund playwright@1.62
#   ou aponte instalações existentes:
#   SPIKE_PLAYWRIGHT_CHROMIUM=/caminho/node_modules/playwright
#   SPIKE_PLAYWRIGHT_WEBKIT=/caminho/node_modules/playwright

# o anfitrião (UDP 18383, HTTP 18384; o correr.cjs espera essas portas)
cargo run --release -- anfitriao --porta 18383 --http 18384 [--lapide] [--alto 262144 --baixo 131072] [--sctp-max 1048576]

# noutro terminal: os navegadores
node correr.cjs chromium eco
node correr.cjs webkit eco
node correr.cjs chromium adulterar - 127.0.0.1 0 segura=30000   # impressão adulterada, pc aberto 30 s
node correr.cjs webkit sobe                                      # 32 MiB página→anfitrião, voz a 50 msg/s
node correr.cjs webkit desce                                     # 32 MiB anfitrião→página (DCEP aberto pelo anfitrião)
node correr.cjs chromium eco - 127.0.0.1 1                       # com Binding Indication antes dos checks

# o nativo controlling contra o mesmo anfitrião (FP= que o anfitrião imprimiu)
cargo run --release -- cliente --alvo 127.0.0.1:18383 --fp <FP>
cargo run --release -- cliente --alvo 127.0.0.1:18383 --fp <FP> --adulterar
cargo run --release -- cliente --alvo 127.0.0.1:18383 --fp <FP> --furar-http 18384
```

- `--lapide` liga a correção do laço de ressurreição do U:
  - o U fica proibido por 30 s depois que o Rtc dele sai;
  - nunca dois Rtc para o mesmo U;
  - no máximo 4 criações por IP em 10 s;
  - o Rtc que não fecha o DTLS em 10 s cai.
- Sem `--lapide`, o anfitrião reproduz o defeito.
- O anfitrião registra, a cada segundo, os Bindings aceitos e os recusados (`RELATORIO`).

## Limitações

- **Só loopback (127.0.0.1).** Não há tradução de endereço, mapeamento nem filtro.
  **NAT real não foi medido**: se o furo (Binding Indication pela 8383) abre um NAT, só se
  mede entre duas redes reais.
- As vazões (120–240 Mbit/s) e as latências são de loopback. A latência da voz numa rede real
  é estimativa, não medida.
- Só o WebKit do Playwright, e não o Safari.app nem o iOS; Firefox não foi testado.
- O anfitrião tem um candidato só (o `--ip`, padrão 127.0.0.1) com o socket em 0.0.0.0.
  Várias interfaces e IPv6 não foram tratados.
- O backend oficial `str0m/rust-crypto` (que traz o AWS-LC) não foi medido. Só o provedor
  do enlace.
