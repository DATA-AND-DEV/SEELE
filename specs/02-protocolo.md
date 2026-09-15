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
   │◀── Sessao { id, server, voice_rooms, papeis }│   → conexão segura
```

Num servidor com MOD habilitado entra mais uma volta, **depois da `Resposta` e antes da `Sessao`** (ADR 0045):

```
   │◀── ModsExigidos { mods, conjunto } ───│   (só se houver MOD habilitado)
   │── AceitarMods { conjunto } ──────────▶│   ou RecusarMods
   │◀── Sessao … ─────────────────────────│   (só para quem aceitou)
```

Depois da assinatura porque a lista de MODs é configuração de quem hospeda; antes da `Sessao` porque a `Sessao` é o fluxo protegido, e «não entra» tem de querer dizer «não recebe nada». Ver `docs/superpowers/specs/2026-09-10-anuncio-e-aceite-de-mods.md`.

Esta volta **sai no fio desde a versão 5 do protocolo** (14/09/2026), que é a subida conjunta da malha e dos MODs — o contrato de integração que `seele_proto::mods::VERSAO_DO_ANUNCIO` carregava desde que nasceu. Antes dela o anúncio viajava numa versão que a global não alcançava, e o servidor admitia como admitia antes, avisando quem hospeda pelo log: um portão que nenhum par pode atravessar recusaria todo mundo sem dar a ninguém a chance de aceitar.

Esse estado dormente continua implementado, para um servidor cujo limiar de anúncio esteja acima da versão global — hoje só por configuração, amanhã pela próxima variante que nascer adiantada. Ver `seele_server::mods::anuncio::o_anuncio_alcanca_alguem`.

**Quem não alcança a versão do anúncio é recusado com `Incompatible`** por um servidor com MOD habilitado, mesmo estando dentro da janela de compatibilidade (N−1). E a v3 da última release publicada não chega nem a essa recusa: ela está fora da janela, e o carimbo de versão no primeiro byte de cada quadro a barraria de qualquer forma — **subir a versão do protocolo tira do ar quem está na versão publicada**, com ou sem MOD. Ver `seele_proto::version::COMPATIBILITY_WINDOW` e a pendência #42.

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
| `AceitarMods` | identidade do conjunto | Resposta ao `ModsExigidos`. ADR 0045 |
| `RecusarMods` | — | «Li e não quero.» Verbo próprio, e não ausência |

## Mensagens de controle — servidor → cliente

| Mensagem | Payload |
|---|---|
| `Sessao` | id da sessão, descrição do servidor, árvore de VoiceRooms e Linhas, papéis |
| `UsuarioEntrou` / `UsuarioSaiu` | `voice_room_id`, perfil do usuário |
| `EstadoUsuario` | mudo, presença, sinal |
| `MensagemRecebida` | mensagem completa |
| `MensagemEditada` / `MensagemRemovida` | id + novo corpo |
| `Telemetria` | RTT, jitter, perda, estado dos subsistemas |
| `Alerta` | severidade, motivo, texto |
| `Pong` | eco do timestamp |
| `Desconectando` | motivo enumerado |
| `ModsExigidos` | os MODs habilitados — identidade, versão, hash, repositório, alcance declarado, e se rodam na máquina de quem hospeda — mais a identidade do conjunto |

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

## Cálculo do sinal

A métrica assinatura do produto (`07-estetica.md`). É derivada, não inventada:

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
