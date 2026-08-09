# 0014 — Palheta v2 como canônica

Status: aceito por default
Contexto: o Claude Design entregou dois protótipos e um arquivo de tokens. `seele-tokens.*` estava em sincronia com o **v1**; o **v2**, artefato mais recente, revisou três cores. Duas palhetas concorrentes no mesmo repositório garantem que alguém construa contra a errada.
Decisão: a palheta do v2 é canônica. `design/seele-tokens.json` e `.css` foram regenerados a partir dela, com índices ANSI 256 recalculados e contraste medido.

| token | v1 | v2 |
|---|---|---|
| `laranja-nerv` | `#FF6B00` | `#F2521F` |
| `vermelho-alerta` | `#E01B24` | `#FF1A1A` |
| `fosforo` | `#3DF57A` | `#6BFFB6` |

Alternativas: manter v1, que era o que os tokens já diziam. Descartado por um motivo que só apareceu ao medir: em v1 o `vermelho-alerta` ficava em **4,14:1, reprovando WCAG AA** — e vermelho é justamente a cor que `specs/07` reserva para erro e queda. O v2 leva para 5,16:1. A revisão não foi só de gosto.

Consequências:

- Mais fácil: uma palheta só; os índices ANSI passam a ser calculados em vez de declarados, e um deles estava errado — o arquivo entregue dizia `ansi256: 208` para `#FF6B00`, quando o vizinho é 202.
- Mais difícil: `laranja-nerv` perdeu contraste, de 7,00:1 (AAA) para 5,71:1 (AA). Continua aprovado, mas não há mais folga.
- Método registrado nos próprios tokens: vizinho mais próximo em CIELAB, **restrito aos índices 16–255**, porque 0–15 são retematizáveis pelo usuário em qualquer terminal.

Quatro achados viraram trabalho de M4 e estão em `docs/tokens-achados.md`: `osso-apagado` reprovando AA para texto pequeno, painel e fundo colapsando no mesmo ansi 232, linhas abaixo do critério de componente não textual, e a ausência de modo sem cor.

Não adotado: a varredura animada do v2 (`seeleVarredura`), por contrariar "movimento é diagnóstico" de `specs/07`. A textura estática de scanline foi mantida.

Custo de reverter: **baixo** agora, **médio** depois que M4 começar a construir telas.
