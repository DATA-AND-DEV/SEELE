# 0001 — Serialização com `postcard`

Status: aceito por default
Contexto: `specs/02-protocolo.md` deixa em aberto entre `postcard` (binário compacto, derivado de `serde`, esquema implícito, amarra clientes a Rust) e `protobuf`/`prost` (esquema explícito, permite cliente em qualquer linguagem). A decisão molda o layout de `seele-proto` e é critério de aceite de M0.
Decisão: `postcard`, com todo o encoding confinado a um único módulo de `seele-proto`. Os tipos de domínio não carregam nenhum atributo específico do formato.
Alternativas: `prost`. Descartado porque cliente de terceiros é **não-objetivo declarado** em `specs/00-visao-geral.md`; pagar boilerplate de `.proto` por um objetivo que a spec rejeita é custo sem contrapartida.
Consequências: mais fácil — zero boilerplate, round-trip trivial de testar, frames menores no datagrama de mídia, onde 11 bytes de cabeçalho para ~80 de payload já é apertado. Mais difícil — se abrir para clientes de terceiros virar objetivo, é preciso migrar; o isolamento em um módulo torna isso mecânico, não uma reescrita.

Custo de reverter: **baixo**, por desenho. Revisar se e quando cliente de terceiros entrar no escopo.
