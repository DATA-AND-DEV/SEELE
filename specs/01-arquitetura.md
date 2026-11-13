# 01 — Arquitetura

> **Estado na 1.0** (conferido contra o código em 10/11/2026). Este documento é o desenho original, com emendas parciais da 0.17.
>
> - **Continua valendo:** toda a lógica no núcleo e nenhuma na interface; a topologia SFU (o servidor encaminha e não decodifica); o callback de áudio fora do async; SQLite com migrações no boot; Rust 2021 com MSRV fixado (`Cargo.toml`: 1.97).
> - **Mudou:**
>   - **uma casca, e não três.** A TUI saiu ([ADR 0039](../docs/adr/0039-o-produto-passa-a-ter-uma-casca-so.md)); `apps/desktop` é `apps/seele-app`; `apps/mobile` nunca existiu, e o celular entra pelo navegador (`crates/seele-web`, ADR 0057). O `quinn` não está mais no `Cargo.lock`; o `ratatui` sobra só como dev-dependency do `seele-conformance`;
>   - **o workspace é outro.** Os membros de hoje estão no `Cargo.toml` da raiz: além dos listados abaixo, `seele-enlace` (transporte), `seele-voz` (portão, jitter buffer, mixer), `seele-sessao`, `seele-video`, `seele-encontro`, `seele-lancador`, `seele-motor`, `seele-motor-web`, `seele-web`, `seele-conformance`, `apps/seele-instalador` e `xtask`. A regra de dependência que vale é a que `xtask/src/check_deps.rs` impõe, e o cabeçalho dele diz onde ela se afasta do texto abaixo;
>   - **o anfitrião é ICE completo e controlado, e não ICE-lite.** A seção «Transporte» abaixo ainda diz ICE-lite: vale a emenda de 26/10/2026 ao [ADR 0063](../docs/adr/0063-a-linha-2-fala-webrtc.md) e o cabeçalho de `crates/seele-enlace/src/ponta.rs`;
>   - **não há TURN.** A seção abaixo fala em «TURN opcional de quem usa»: a mesma emenda o tirou, e o produto não oferece relay. O que o caminho direto não atravessa fica sem caminho;
>   - **não há RTP.** O `str0m` serve de ICE, DTLS e SCTP, e tudo vai em canais de dados (`crates/seele-enlace/src/conexao.rs:103`). Além de `ctl`, `voz`, `u` e `b`, existem os canais de túnel `t` (confiável) e `j` (sem ordem e sem retransmissão), em `crates/seele-proto/src/transport.rs`;
>   - **as três decisões em aberto do fim foram tomadas:** TOFU (ADR 0003), `postcard` (ADR 0001) e a escada de alcance com IPv6, UPnP e furo de NAT (ADR 0022). A porta 8383 é `DEFAULT_PORT` em `crates/seele-proto/src/transport.rs`.
> - **Manda agora:** ADR 0063 com as emendas, `crates/seele-enlace` e `xtask/src/check_deps.rs`.

## Princípio central

**Um núcleo, três cascas.** Toda lógica de sessão, protocolo, áudio e estado vive em `seele-core`, que é headless e testável sem interface. TUI, desktop e mobile são camadas de apresentação sobre a mesma máquina de estados. Nenhuma delas contém regra de negócio.

Se uma funcionalidade precisa ser implementada duas vezes em interfaces diferentes, ela está no lugar errado.

## Workspace

```
seele/
├─ Cargo.toml              # workspace
├─ crates/
│  ├─ seele-proto/          # tipos do protocolo, serialização, versionamento
│  ├─ seele-core/           # cliente headless: sessão, canais, áudio, eventos
│  ├─ seele-audio/          # captura, codec, jitter buffer, mixagem
│  ├─ seele-server/         # o daemon (seeled)
│  ├─ seele-tui/            # binário `connection` — ratatui + crossterm
│  └─ seele-ffi/            # superfície C ABI + uniffi para Tauri/Flutter
├─ apps/
│  ├─ desktop/             # Tauri
│  └─ mobile/              # Flutter  [EM ABERTO: ver 06]
└─ specs/
```

**Regra de dependência:** `proto` não depende de ninguém. `audio` depende só de `proto`. `core` depende de `proto` e `audio`. Todo o resto depende de `core`. Nunca o inverso.

## Linguagem

Rust, edição 2021, MSRV fixado. Justificativa: precisamos de áudio sem GC pausando o thread de tempo real, de um binário único sem runtime, e o ecossistema de QUIC/Opus/TUI em Rust é maduro. (Desde a 0.17 o transporte é o WebRTC do `str0m`, e não o QUIC do `quinn`: ver «Transporte» abaixo.) `ratatui` e `quinn` são as duas bibliotecas que decidem a viabilidade e ambas são sólidas.

## Transporte: WebRTC desde a 0.17 (ADR 0063)

**Decisão vigente:** WebRTC, pelo `str0m`, no crate `seele-enlace`, que é comum ao `seele-core` e ao `seele-server` (emenda ao ADR 0002).
- É **um transporte só**, no nativo e no navegador.
- O anfitrião é ICE-lite, com DTLS passivo, num socket UDP só (8383), separado pelo primeiro byte: STUN, DTLS e o texto do ENC/3.
- Os canais:
  - `ctl` é confiável e ordenado: é o controle;
  - `voz` não tem ordem nem retransmissão: são os datagramas de voz;
  - `u` e `b` são os fluxos extras de antes, com `protocol = "seele/2"`.
- O DTLS do `seeled` usa o mesmo certificado longo P-256 que o QUIC usava, então a impressão do convite não muda.
- **Hospedar continua só nos nativos.** O navegador só entra.
- A travessia de NAT é pelo ponto de encontro (ENC/3), com o TURN opcional de quem usa.

O texto abaixo é a decisão da linha 1 (até a 0.16), preservado como registro. Os motivos dele valem para o WebRTC menos dois:
- não há migração de conexão: a «bateria interna» reconecta por um aperto de mão novo;
- não há 0-RTT.

## Transporte: QUIC (linha 1, até a 0.16)

**Decisão (linha 1):** QUIC via `quinn`, uma única conexão por cliente.

Motivos:
- **Streams confiáveis** para controle e texto; **datagrams não confiáveis** para voz, na mesma conexão. Sem head-of-line blocking entre as duas coisas.
- **TLS 1.3 obrigatório** e embutido. Não há caminho não criptografado.
- **Migração de conexão**: trocar de Wi-Fi para rede móvel não derruba a sessão — o `connection ID` sobrevive à mudança de IP. Isso é o que torna a "bateria interna" (ver `07`) tecnicamente elegante em vez de gambiarra.
- **Handshake de 1-RTT**, 0-RTT em reconexão.

Alternativa descartada: TCP para controle + UDP para mídia. Dobra o trabalho de NAT traversal, keepalive e criptografia, e entrega menos.

Porta única, UDP. Padrão: **8383** [EM ABERTO: confirmar].

## Topologia de mídia: SFU, não MCU

O servidor **encaminha** pacotes de áudio; não decodifica, não mistura, não recodifica. Cada cliente recebe N streams e mistura localmente.

Consequências positivas:
- CPU do servidor fica quase constante independente do número de falantes.
- Volume e mudo por usuário são possíveis (impossível com mixagem no servidor).
- Áudio espacial fica viável depois.
- O servidor encaminha o Opus sem decodificar nem reescrever (specs/02), mas o recebe em claro: quem hospeda pode, em tese, gravar a voz. Por não tocar no payload, E2EE é um incremento, e não uma reescrita (specs/08).

Consequência negativa: banda cresce com O(n²) no pior caso. Para o alvo (VoiceRooms de até ~15 pessoas), é irrelevante. Mitigação embutida: o VAD faz com que só quem está falando transmita, e na prática 2–3 pessoas falam por vez.

**Limite rígido:** acima de 20 participantes ativos em uma sala de voz, o servidor passa a encaminhar apenas os N falantes mais altos [EM ABERTO: definir N e a política].

## Modelo de concorrência

- `tokio` como runtime, tanto no servidor quanto no cliente.
- **O thread de áudio nunca é async.** Callback do `cpal` é tempo real: sem alocação, sem lock que bloqueie, sem I/O. Comunicação com o mundo async por ring buffer lock-free (`rtrb` ou similar).
- Estado do servidor por VoiceRoom em uma task própria, com canais `mpsc`. Nada de `Mutex` global.

## Fluxo de eventos no cliente

`seele-core` expõe uma máquina de estados que consome comandos e emite eventos:

```
Comando  →  [ seele-core ]  →  Evento
  Conectar                     SincronizacaoAlterada
  EntrarNoVoiceRoom                 UsuarioEntrou / UsuarioSaiu
  EnviarMensagem               MensagemRecebida
  DefinirATField               TelemetriaAtualizada
  Ejetar                       ConexaoPerdida / Reconectando
```

Cada casca (TUI, desktop, mobile) só traduz eventos em pixels e input em comandos. Essa fronteira é o contrato mais importante do projeto — se ela vazar, o projeto vira três aplicativos.

## Persistência

SQLite via `rusqlite` no servidor. Nada no cliente além de configuração e cache de histórico opcional. Migrações versionadas e aplicadas no boot.

## Decisões em aberto

- **[EM ABERTO]** Certificados: auto-assinado com pinning (TOFU) por padrão, ou exigir Let's Encrypt? TOFU é mais amigável para auto-hospedagem; pinning precisa de UX clara para troca de chave.
- **[EM ABERTO]** Formato de serialização: `postcard` (compacto, Rust-nativo) vs `protobuf` (interoperável com clientes de terceiros). Ver `02`.
- **[EM ABERTO]** IPv6 e NAT traversal: assumimos servidor com IP público em v1?
