# 0006 — Esquema de URI `seele://`

Status: proposto
Contexto: o protótipo em `design/` usa `seele://toquio-3.dogma.central:7743` como forma canônica de endereçar um Dogma. **Nenhuma spec define esse esquema.** `specs/05-cliente-tui.md` só prevê `:conectar <host>`.
Decisão: **pendente.** Recomendação: adotar `seele://host[:porta][/cage]` e especificar em `specs/02-protocolo.md`.
Alternativas: só host e porta, sem esquema. Funciona, mas perde a capacidade de um link clicável levar direto a um Cage, que é exatamente o tipo de coisa que faz um servidor auto-hospedado ser convidável.
Consequências: mais fácil — `:conectar` aceita uma coisa só, colável; convite por token (ADR 0004) ganha forma natural de transporte. Mais difícil — vira superfície de parsing com entrada não confiável, e portanto responsabilidade de `seele-proto`, com fuzzing junto.

Vence antes de M2. Decidir junto com ADR 0005.
