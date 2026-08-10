# 0023 — Idioma dentro do `seele-core`

Status: aceito
Contexto: `specs/10-convencoes.md` manda identificadores e comentários em inglês, e o `seele-core` obedece em `state.rs`, `battery.rs`, `tofu.rs` e `voice.rs` — mas `conhecidos.rs` e `enlace.rs`, os dois módulos mais recentes, são inteiramente portugueses, identificadores e doc. A deriva nunca foi decidida; aconteceu.
Decisão: módulos novos do `seele-core` seguem `specs/10` — inglês. `conhecidos` e `enlace` ficam como estão: renomeá-los agora tocaria as duas cascas e o `seele-ffi` por uma questão de coerência, e o custo não paga.
Consequências: o crate fica com dois sotaques por um tempo, e isto fica escrito para o próximo módulo não re-litigar. Quem renomear `conhecidos` ou `enlace` algum dia faz isso como trabalho próprio, não de passagem.
