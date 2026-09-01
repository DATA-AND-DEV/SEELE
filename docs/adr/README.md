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
| [0016](0016-ptt-trava-quando-o-terminal-nao-reporta-soltura.md) | Push-to-talk vira trava onde o terminal não reporta soltura | aceito | `03` — push-to-talk |
| [0017](0017-identidade-e-pins-em-disco.md) | Identidade e pins gravados em disco, sem senha | aceito | `08` — guarda da identidade |
| [0018](0018-seele-ffi-sem-uniffi-por-enquanto.md) | `seele-ffi` com a forma que o `uniffi` exige, sem a dependência | aceito | `01` — ponte para o app |
| [0019](0019-frontend-sem-framework-e-sem-npm.md) | Frontend do desktop sem framework e sem npm | aceito | `05` — frontend desktop |
| [0020](0020-o-que-o-tauri-traz-junto.md) | O que o Tauri traz junto, e por que aceitamos | aceito | `01` — dependências do app |
| [0021](0021-admissao-em-um-dogma.md) | Quem entra num Dogma: convite de uso único, senha como alternativa | aceito | `08` — admissão |
| [0022](0022-alcancar-um-dogma-pela-internet.md) | Alcançar um Dogma pela internet | **aceito** | `01` — alcance fora da rede local |
| [0023](0023-idioma-dentro-do-seele-core.md) | Idioma dentro do `seele-core` | aceito | `10` — idioma |
| [0024](0024-faixas-de-sincronia-em-tres-e-a-media-no-core.md) | Faixas de sincronia em três, e a média do Cage no core | aceito | `03` — sincronia |
| [0025](0025-limitacao-de-taxa-em-dois-baldes.md) | Limitação de taxa: dois baldes, e um aviso antes da porta | aceito | `08` — limitação de taxa |
| [0026](0026-duas-assinaturas-e-um-botao-de-atualizar.md) | Duas assinaturas, e um botão de atualizar | aceito | `01` — distribuição |
| [0027](0027-anexos-com-teto-e-o-mais-velho-sai.md) | Anexos com teto total, e o mais velho sai | aceito | `02` — política de anexos (D14) |
| [0028](0028-a-reserva-do-anel-de-reproducao.md) | A reserva do anel de reprodução, e o que ela custa de latência | aceito | — (pendência 2, e revisão do `0009`) |
| [0029](0029-mods-declaram-valores-e-o-produto-mede.md) | MODs: declaram valores, e o produto mede antes de aplicar | **proposto** | — (pedido do dono; desfaz metade do não-objetivo de `00`) |
| [0030](0030-quem-bate-a-porta.md) | Quem bate à porta: TOFU aplicado a gente, e a portaria de quem hospeda | **proposto** | — (pedido do dono; estende o `0021`) |
| [0031](0031-varios-dogmas-ao-mesmo-tempo.md) | Vários Dogmas ao mesmo tempo: a sessão é do Dogma, o microfone é da máquina | **proposto** | — (pedido do dono; o `+` da trilha) |
| [0032](0032-personalizacao-de-um-dogma.md) | Personalização de um Dogma: nome, cor e ícone | **proposto** | — (pedido do dono; reusa a resolução do `0029`) |
| [0033](0033-o-vocabulario-sai-da-interface-a-estetica-fica.md) | O vocabulário de Evangelion sai da interface; a estética fica | aceito | `07` — o vocabulário na tela |
| [0034](0034-a-marca-abandona-as-duas-citacoes-do-anime.md) | A marca abandona as duas citações do anime | aceito | `07` — a marca na imagem |
| [0035](0035-o-codigo-deixa-de-falar-evangelion.md) | O código deixa de falar Evangelion | aceito | `07` — o vocabulário no código |
| [0036](0036-bitrate-adaptativo-em-faixas.md) | Bitrate adaptativo em faixas, sobre perda de subida medida no servidor | aceito | `03` — o bitrate adaptativo que a spec pede |
| [0037](0037-candidatos-do-convite-em-paralelo.md) | Um `Endpoint`, muitas conexões: os candidatos do convite correm juntos | aceito | `02` — alcançar um servidor pela internet |
| [0038](0038-o-teto-da-sala-e-contado-nao-declarado.md) | O teto da sala é contado, e quem hospeda é avisado | aceito | `04` — o dimensionamento da sala |
| [0039](0039-o-produto-passa-a-ter-uma-casca-so.md) | O produto passa a ter uma casca só | aceito | `05` e `06` — as cascas |
| [0040](0040-sessenta-quadros-entram-por-medida.md) | Sessenta quadros entram, por medida | aceito | design da tela, §6 item 10 |
| [0041](0041-o-codec-por-hardware-e-a-excecao-ao-unsafe.md) | O codec por hardware, e a exceção nomeada ao `unsafe` | **aceito** | — (reverte a recusa registrada no `Cargo.toml` do `seele-video`) |

## O que ainda não tem ADR

Decisões em aberto que vencem depois de M1 e por isso não foram escritas ainda —
ver `docs/plano-m0-m1.md`, seção 4.2: política acima de
20 falantes (M3), endpoint de saúde (M3),
recarga a quente de config (M3), compressão de histórico (M3), PTT global (M4),
tecla de PTT (M4), leitor de tela (M4), framework do frontend desktop (M5),
plataforma mobile (M6).

**Anexos saíram desta lista**: viraram o [0027](0027-anexos-com-teto-e-o-mais-velho-sai.md),
aceito e construído, e ele desfaz metade da D14 — o teto de corpo de 4 KiB
continua, e o "sem anexos em v1" não. O que ele decide está de pé, com uma
exceção que ele mesmo registra: «nenhuma dependência nova» caiu, e o seletor de
arquivos nativo existe desde 2026-08-18 — o botão ARQUIVO não abria nada, e o
primeiro dono a usar o app clicou nele. A prévia embutida de imagem continua não
construída, anotada no alto dele. Ver pendência 18.

**IPv6/NAT traversal saiu desta lista**: virou o [0022](0022-alcancar-um-dogma-pela-internet.md),
aceito, com os degraus 2 (IPv6) e 3 (UPnP) construídos. O degrau 4 — furo de NAT
com ponto de encontro — continua sem decisão de propósito: ele custa uma
conversa sobre o metadado que o ponto de encontro aprende, e o 0022 existe para
que essa conversa aconteça antes do código.

**MODs nunca estiveram nesta lista, e o motivo importa**: `specs/00-visao-geral.md`
os punha como **não-objetivo** de v1 ("marketplace de plugins"), e a tela de
configurações registrava que um segundo tema seria "decisão de ADR, não de tela".
Viraram o [0029](0029-mods-declaram-valores-e-o-produto-mede.md), **proposto**,
que desfaz metade daquele não-objetivo. Nada foi construído; ver pendência 21.

**Várias sessões e personalização de Dogma também nunca estiveram nesta lista**,
e o motivo é que ninguém as tinha pedido: o `+` da trilha estava desabilitado com
a limitação escrita no `title`, e o nome de um Dogma hospedado pelo app era o
literal `"Casa"`. Viraram o [0031](0031-varios-dogmas-ao-mesmo-tempo.md) e o
[0032](0032-personalizacao-de-um-dogma.md), os dois **propostos**, e o segundo
cita o primeiro na parte que compartilham — o corte entre o que é do Dogma e o
que é desta máquina. Nada foi construído; ver pendências 24 e 25.

Postura de direitos sobre Evangelion (`07`) também não tem ADR: a recomendação é
repositório privado até M4, o que tira a decisão do caminho crítico.

## Status usados

`10-convencoes.md` prevê `aceito` e `substituído por NNNN`. Acrescentamos:

- **`proposto`** — descrita e recomendada, mas ainda não vale. Usado quando não
  existe default seguro.
- **`aceito por default`** — em vigor sem confirmação humana explícita, para não
  bloquear o milestone. Revisável; o custo de reverter está no próprio ADR.
