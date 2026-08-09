# Registros de decisão de arquitetura

Formato e regra de criação em `specs/10-convencoes.md`: toda decisão marcada
`[EM ABERTO]` nas specs vira um ADR quando resolvida. Template em
`0000-template.md`.

| # | Título | Status | Decisão em aberto que fecha |
|---|---|---|---|
| [0001](0001-serializacao-postcard.md) | Serialização com `postcard` | aceito por default | `02` — serialização |
| [0002](0002-regra-de-dependencia.md) | Regra de dependência mais estrita que a spec | aceito por default | — (divergência encontrada em M0) |
| [0003](0003-certificados-tofu.md) | TOFU como padrão de certificado | aceito por default | `01`, `08` — certificados |
| [0004](0004-autenticacao-chave-publica.md) | Autenticação por chave pública Ed25519 | aceito por default | `08` — autenticação |
| [0005](0005-porta-padrao.md) | Porta padrão 8383/UDP | **proposto** | `01` — porta |
| [0006](0006-esquema-de-uri.md) | Esquema de URI `seele://` | **proposto** | — (lacuna encontrada no design) |
| [0007](0007-sem-dsp-externo-em-v1.md) | Sem DSP externo em v1 | aceito por default | `03` — AEC, AGC, supressão |
| [0008](0008-binding-opus.md) | `shiguredo_opus` como binding do codec | **aceito** | `03` — binding do codec |
| [0009](0009-orcamento-de-latencia.md) | Orçamento de latência boca-a-ouvido | **aceito** | — (contradição `00` × `03`) |
| [0010](0010-fec-do-opus.md) | FEC in-band do Opus desligado em v1 | aceito | `02` — FEC |
| [0011](0011-toolchain-e-msrv.md) | Toolchain fixado, MSRV igual ao toolchain | aceito por default | `01` — MSRV |
| [0012](0012-i18n.md) | i18n desde M0, sem milestone próprio | aceito por default | — (lacuna G4) |
| [0013](0013-idioma-de-manifestos-e-ci.md) | Manifestos e CI em inglês | aceito por default | — (lacuna em `10`) |
| [0014](0014-palheta-v2-canonica.md) | Palheta v2 como canônica | aceito por default | `07` — tokens de cor |
| [0015](0015-vad-sem-webrtc.md) | VAD por energia, sem `webrtc-vad` | aceito por default | `03` — ativação por voz |

## O que ainda não tem ADR

Decisões em aberto que vencem depois de M1 e por isso não foram escritas ainda —
ver `docs/plano-m0-m1.md`, seção 4.2: IPv6/NAT traversal (M2), política acima de
20 falantes (M3), limite de mensagem e anexos (M3), endpoint de saúde (M3),
recarga a quente de config (M3), compressão de histórico (M3), PTT global (M4),
tecla de PTT (M4), leitor de tela (M4), framework do frontend desktop (M5),
plataforma mobile (M6).

Postura de direitos sobre Evangelion (`07`) também não tem ADR: a recomendação é
repositório privado até M4, o que tira a decisão do caminho crítico.

## Status usados

`10-convencoes.md` prevê `aceito` e `substituído por NNNN`. Acrescentamos:

- **`proposto`** — descrita e recomendada, mas ainda não vale. Usado quando não
  existe default seguro.
- **`aceito por default`** — em vigor sem confirmação humana explícita, para não
  bloquear o milestone. Revisável; o custo de reverter está no próprio ADR.
