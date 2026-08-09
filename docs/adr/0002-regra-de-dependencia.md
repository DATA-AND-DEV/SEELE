# 0002 — Regra de dependência mais estrita que a spec

Status: aceito por default
Contexto: `specs/01-arquitetura.md` enuncia "proto não depende de ninguém. audio depende só de proto. core depende de proto e audio. **Todo o resto depende de core.** Nunca o inverso." Ao codificar isso em `cargo xtask check-deps`, a última frase se mostrou errada em dois pontos.
Decisão: a tabela em `xtask/src/check_deps.rs` encoda a leitura estrita:

| Crate | Pode depender de |
|---|---|
| `seele-proto` | nada do workspace |
| `seele-audio` | `seele-proto` |
| `seele-core` | `seele-proto`, `seele-audio` |
| `seele-server` | **`seele-proto` apenas** |
| `seele-tui`, `seele-ffi` | **`seele-core` apenas** |

Alternativas: seguir a spec ao pé da letra. Descartado por dois motivos concretos:

1. `seele-server` depender de `seele-core` é depender do **cliente headless**. Pior, arrastaria `seele-audio` — e portanto `cpal` e `libopus` — para dentro de um daemon que, por `specs/04-servidor-seele.md`, "nunca decodifica o Opus" e precisa caber em 1 vCPU / 512 MB.
2. Cascas poderem alcançar `seele-proto` e `seele-audio` diretamente é o mecanismo exato do vazamento que `01` diz querer evitar. Uma casca que sabe nomear um `ssrc` já tem lógica dentro.

Consequências: mais fácil — a fronteira núcleo/casca vira build vermelho, não revisão de código. O guarda pega duas classes que o Cargo não pega: dev-dependency invertida (o Cargo tolera o ciclo) e aresta lateral, como `seele-tui → seele-proto`, que não forma ciclo nenhum. Mais difícil — se a casca precisar de um tipo do protocolo, `seele-core` tem que reexportá-lo conscientemente. Isso é o ponto, não um efeito colateral.

Pendência: `specs/01-arquitetura.md` deveria ser corrigida. `specs/10-convencoes.md` diz que spec desatualizada é pior que spec ausente.
