# 02 — Protocolo

## Camadas

Uma conexão QUIC carrega três coisas:

| Canal | Tipo QUIC | Conteúdo |
|---|---|---|
| Controle | Stream bidirecional #0, longa duração | Handshake, estado, presença, comandos |
| Texto | Streams bidirecionais efêmeros | Mensagens, histórico, edições |
| Mídia | Datagrams | Quadros de voz Opus |

Separar texto de controle evita que um `fetch` de 5 mil mensagens de histórico atrase um evento de presença.

## Serialização

**[EM ABERTO — decidir em M0]** Duas opções, com trade-off claro:

- **`postcard`** — binário compacto, deriva de `serde`, zero boilerplate, esquema implícito. Rápido de construir. Amarra clientes de terceiros a Rust.
- **`protobuf` (`prost`)** — esquema explícito em `.proto`, versionável, permite cliente em qualquer linguagem.

Recomendação: `postcard` para M0–M3, com os tipos isolados em `seele-proto` de forma que a troca depois seja mecânica. Se abrir para clientes de terceiros virar objetivo, migrar.

## Versionamento

Primeiro byte de todo frame de controle é a versão do protocolo. Servidor recusa versão maior que a sua com `Incompatível`. Versão menor: aceita se estiver dentro da janela de compatibilidade declarada (N−1).

## Handshake

```
Cliente                                Servidor
   │── QUIC ClientHello ──────────────────▶│   (TLS 1.3)
   │◀───────────────── ServerHello ────────│
   │── Ola { versao, cliente, apelido } ──▶│
   │◀── Desafio { nonce } ─────────────────│
   │── Resposta { prova } ────────────────▶│   (ver 08)
   │◀── Sessao { id, server, voice_rooms, papeis }│   → PADRÃO: AZUL
```

Antes da `Sessao`, o cliente está em **PADRÃO: LARANJA** — conectado, não verificado. A interface deve refletir esse estado, não escondê-lo.

Timeout de handshake: 10 s. Falha → `PadraoAzulNaoEstabelecido` com motivo específico (nunca genérico).

## Mensagens de controle — cliente → servidor

| Mensagem | Payload | Notas |
|---|---|---|
| `Ola` | versão, nome do cliente, apelido pretendido | |
| `Resposta` | prova de autenticação | |
| `InserirPlug` | `voice_room_id`, senha opcional | Entrar em canal de voz |
| `EjetarPlug` | — | |
| `EntrarNaLinha` | `linha_id` | Assinar canal de texto |
| `EnviarMensagem` | `linha_id`, corpo, `responde_a` opcional | Idempotente por `client_msg_id` |
| `BuscarHistorico` | `linha_id`, cursor, limite | Paginação por cursor, nunca offset |
| `DefinirATField` | bool | Mudo local, anunciado ao servidor |
| `DefinirEstado` | enum presença | |
| `Ping` | timestamp | Base para o cálculo de sincronização |

## Mensagens de controle — servidor → cliente

| Mensagem | Payload |
|---|---|
| `Sessao` | id da sessão, descrição do servidor, árvore de VoiceRooms e Linhas, papéis |
| `UsuarioEntrou` / `UsuarioSaiu` | `voice_room_id`, perfil do usuário |
| `EstadoUsuario` | A.T. Field, presença, taxa de sincronização |
| `MensagemRecebida` | mensagem completa |
| `MensagemEditada` / `MensagemRemovida` | id + novo corpo |
| `Telemetria` | RTT, jitter, perda, estado dos subsistemas |
| `Alerta` | severidade, motivo, texto |
| `Pong` | eco do timestamp |
| `Desconectando` | motivo enumerado |

**Todos os motivos de erro são enumerados.** Nada de string livre chegando na interface — a casca decide como apresentar cada variante.

## Frames de mídia (datagram)

Datagram QUIC tem entrega não confiável e sem ordem, que é exatamente o desejado para voz. Estrutura:

```
┌─────────┬──────────┬────────────┬─────────┬──────────────┐
│ ver (1) │ ssrc (4) │ seq (2)    │ ts (4)  │ opus payload │
└─────────┴──────────┴────────────┴─────────┴──────────────┘
```

- `ssrc` — identificador da fonte, atribuído na entrada da sala de voz.
- `seq` — sequencial, com wrap. Detecta perda e reordenação.
- `ts` — timestamp em amostras a 48 kHz. Detecta gaps de silêncio.
- Payload Opus de 20 ms.

**Overhead:** 11 bytes de cabeçalho para ~80 bytes de payload a 32 kbps. Aceitável. Não adicionar campos sem necessidade demonstrada.

O servidor reescreve apenas o `ssrc` ao encaminhar? **Não** — encaminha íntegro e o cliente resolve `ssrc` → usuário pela tabela recebida no controle. Isso mantém o servidor sem tocar no payload, o que é pré-requisito para E2EE.

## Cálculo da Taxa de Sincronização

A métrica assinatura do produto (`07-tema-evangelion.md`). É derivada, não inventada:

```
sync = 100
     − penalidade_rtt(rtt_ms)          # 0 acima de 40 ms, cresce até 40 pontos
     − penalidade_jitter(jitter_ms)    # até 30 pontos
     − penalidade_perda(perda_pct)     # até 30 pontos, mais agressiva
```

Suavizada com média móvel exponencial (α ≈ 0,2) para não piscar. Faixas: ≥ 85 nominal · 60–84 degradado · < 60 crítico. Cada faixa tem cor própria — ver `07`.

## Keepalive e queda

- `Ping` a cada 5 s. Três perdidos consecutivos → estado `Reconectando`.
- QUIC tenta migração de conexão automaticamente.
- Cliente mantém a sessão viva localmente por **5 minutos** (a "bateria interna") com backoff exponencial de reconexão. O servidor guarda o slot pelo mesmo período.
- Após 5 min: sessão encerrada, histórico preservado.

## Decisões em aberto

- ~~FEC do Opus e retransmissão seletiva~~ — **resolvido**: só jitter buffer com
  PLC em v1. FEC in-band custa +20 ms de profundidade de buffer e foi recusado
  para v1; retransmissão seletiva continua fora de escopo. Ver
  `docs/adr/0010-fec-do-opus.md` e `03-audio.md`.
- **[EM ABERTO]** Compressão do histórico de texto em transferências grandes.
- ~~Limite de tamanho de mensagem e política de anexos~~ — **resolvido**: as
  duas metades foram decididas separadamente, e uma delas mudou de resposta. O
  teto de corpo é 4 KiB e continua sendo (`MAX_BODY_LEN`); "sem anexos em v1"
  deixou de valer. **O servidor guarda anexo, com teto total fixo, e ao encher o
  mais velho sai** — 1 GiB por padrão, escolhido por quem hospeda com `seeled
  anexos`. Cada transferência abre um fluxo QUIC unidirecional próprio e nunca
  o de controle; a resposta volta pelo controle como razão enumerada. Ver
  `docs/adr/0027-anexos-com-teto-e-o-mais-velho-sai.md`.
