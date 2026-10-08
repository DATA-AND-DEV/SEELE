# F1 — andamento

Plano: `docs/superpowers/plans/2026-10-08-0.16-F1-protocolo.md`. Branch `0.16/protocolo`, de `versao/0.16` em `d4eb6f1` (a orquestradora citou `c0337b2`; entre os dois só há `d4eb6f1`, um commit de documento do spec-mestre).

| tarefa | estado | commit | nota |
|---|---|---|---|
| 0 o plano | feito | — | |

## Medidas desta sessão

- `cargo check -p seele-proto --target wasm32-unknown-unknown` passa em `d4eb6f1` (3,7 s). Medido em 08/10.
- Publicada, pela API de releases em 08/10: v0.15.1, do commit `54f542f`. Lá, `PROTOCOL_VERSION = 8` (`version.rs:124`) e `COMPATIBILITY_WINDOW = 1` (`:164`).

## Pendências para a orquestradora

- O portão do Plano 2 (o teste com três partes) não foi escrito nesta sessão. Ver a seção «O portão do Plano 2» do plano.
- O lugar do assento do E2EE (`Hello` e `PersonProfile`) é escolha técnica minha (adendo §10), aberta até o 9 congelar.
- O ADR novo leva o número 0058. Nenhuma branch tinha 0058 em 08/10.
