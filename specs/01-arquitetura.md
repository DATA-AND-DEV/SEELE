# 01 — Arquitetura

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
│  ├─ seele-tui/            # binário `plug` — ratatui + crossterm
│  └─ seele-ffi/            # superfície C ABI + uniffi para Tauri/Flutter
├─ apps/
│  ├─ desktop/             # Tauri
│  └─ mobile/              # Flutter  [EM ABERTO: ver 06]
└─ specs/
```

**Regra de dependência:** `proto` não depende de ninguém. `audio` depende só de `proto`. `core` depende de `proto` e `audio`. Todo o resto depende de `core`. Nunca o inverso.

## Linguagem

Rust, edição 2021, MSRV fixado. Justificativa: precisamos de áudio sem GC pausando o thread de tempo real, de um binário único sem runtime, e o ecossistema de QUIC/Opus/TUI em Rust é maduro. `ratatui` e `quinn` são as duas bibliotecas que decidem a viabilidade e ambas são sólidas.

## Transporte: QUIC

**Decisão:** QUIC via `quinn`, uma única conexão por cliente.

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
- O servidor nunca vê áudio em claro → E2EE é um incremento, não uma reescrita.

Consequência negativa: banda cresce com O(n²) no pior caso. Para o alvo (VoiceRooms de até ~15 pessoas), é irrelevante. Mitigação embutida: o VAD faz com que só quem está falando transmita, e na prática 2–3 pessoas falam por vez.

**Limite rígido:** acima de 20 participantes ativos em um VoiceRoom, o servidor passa a encaminhar apenas os N falantes mais altos [EM ABERTO: definir N e a política].

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
