# SEELE — Especificações

Sistema de comunicação por voz e texto operado via terminal, com clientes gráficos opcionais.
Estética de console de operação: densidade de informação e hierarquia de comando. O vocabulário é do `docs/glossario.md` desde o ADR 0033.

## Como usar esta pasta

Estes documentos são a fonte de verdade para o planejamento. Eles descrevem **o que** construir e **por quê**; não contêm código de produção. Leia na ordem numérica na primeira passada.

| Arquivo | Assunto |
|---|---|
| `00-visao-geral.md` | Produto, escopo, não-objetivos, critérios de sucesso |
| `01-arquitetura.md` | Workspace de crates, decisões técnicas e trade-offs |
| `02-protocolo.md` | Formato de mensagens, handshake, canais QUIC |
| `03-audio.md` | Pipeline de captura, codec, jitter buffer, mixagem |
| `04-servidor-seele.md` | Estado, permissões, subsistemas, persistência |
| `05-cliente-tui.md` | Layout, atalhos, modelo de interação do terminal |
| `06-clientes-gui.md` | Desktop e mobile, camada FFI |
| `07-estetica.md` | Densidade, tokens de cor, tipografia, movimento, voz da interface |
| `08-seguranca.md` | Transporte, autenticação, caminho para E2EE |
| `09-roadmap.md` | Milestones M0–M6 com critérios de aceite |
| `10-convencoes.md` | Estilo, testes, CI, commits, versionamento |

## Primeira tarefa sugerida ao Claude Code

> Leia toda a pasta `specs/`. Produza um plano de implementação para o milestone **M0** e **M1** descritos em `09-roadmap.md`: divisão em tarefas, ordem de dependência, o que precisa de prova de conceito antes de virar código definitivo, e uma lista das decisões em aberto que precisam de resposta minha antes de começar. Não escreva código ainda.

## Estado

Pré-alfa. Nada implementado. Todas as decisões marcadas com **[EM ABERTO]** precisam de definição.
