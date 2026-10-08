# F1 — andamento

Plano: `docs/superpowers/plans/2026-10-08-0.16-F1-protocolo.md`. Branch `0.16/protocolo`, de `versao/0.16` em `d4eb6f1` (a orquestradora citou `c0337b2`; entre os dois só há `d4eb6f1`, um commit de documento do spec-mestre).

| tarefa | estado | commit | nota |
|---|---|---|---|
| 0 o plano | feito | `1f76d16`, `bbc6953` | a 13b entrou a pedido da orquestradora |
| 1 o adendo de negociação | feito | `fafe105` | |
| 2 o ADR 0058 | feito | `2569bc6` | o guarda `adr_index` do xtask passa |
| 3 o enquadramento no `seele-proto` | feito | `ce90b19` | reversão: `to_le_bytes` reprova 4 de 8; `wasm32` compila |
| pedidos da F6 (a), (b), (e) | feito | `aa8574e` | o ffi reexporta pelo `seele_core` (ADR 0002) |
| 4 a faixa, a linha e o 9 | feito | `a435b93` | reversão: `VERSAO_MINIMA = 8` reprova 3; fuzz `version_negotiation` compilado e rodado |
| (lock do fuzz) | feito | `1277d85` | o lock não tinha o `serde_json` do `seele-proto` |
| 5 o carimbo e a recusa legível | feito | `ce70c50` | reversão: carimbar sempre o próprio número reprova |
| 6 a mensagem desconhecida | feito | `4b4be35` | duas reversões: sem o pulo, reprovam 2; `CONHECIDAS = 40`, reprova 1 |
| 7 o número da voz | feito | `f3aaf9c` | reversão: comparar com o protocolo reprova 3; fuzz `media_header` rodado num corpus de rascunho |

## Medidas desta sessão

- `cargo check -p seele-proto --target wasm32-unknown-unknown` passa em `d4eb6f1` (3,7 s). Medido em 08/10.
- Publicada, pela API de releases em 08/10: v0.15.1, do commit `54f542f`. Lá, `PROTOCOL_VERSION = 8` (`version.rs:124`) e `COMPATIBILITY_WINDOW = 1` (`:164`).

## Pendências para a orquestradora

- O portão do Plano 2: a orquestradora o trocou pelas Tarefas 13 e 13b (08/10).
- O lugar do assento do E2EE (`Hello` e `PersonProfile`) é escolha técnica minha (adendo §10), aberta até o 9 congelar.
- O ADR novo leva o número 0058. Nenhuma branch tinha 0058 em 08/10.
