# F8 — A tela do celular: andamento

Branch `0.16/celular`, saído de `versao/0.16` em `f7bc902b` (09/10). Spec-mestre:
`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F8 e D-celular-1.

## Etapa A — levantamento e desenho

| passo | estado | onde |
|---|---|---|
| o duble da casca volta a chegar à sessão (pedido da orquestradora) | **feito** em `5d2e6681`: os 28 roteiros de `tools/roteiros/` passam; a causa dos 17 que reprovavam era a pergunta do nome da `acbc1c29` | `tools/carga-da-casca.py` |
| a matriz de paridade | **feita**: 467 linhas e os 159 `invoke`, com o que a casca chama sem gesto medido no Chromium | `F8/paridade.md` |
| as perguntas de desenho | **respondidas** pela orquestradora em 09/10 (navegação, movimento, tema, o que fica fora, SEGURAR PARA FALAR, `operador.barra`, arquitetura híbrida) | mensagens de 09/10 |
| o protótipo | em andamento: 27 estados × 2 tamanhos × 3 temas, com as medidas de toque, rolagem, áreas seguras e núcleo | `spikes/casca-do-celular/` |
| o spec | a fazer | `specs/2026-10-09-0.16-celular-design.md` |
| o plano | a fazer | `plans/2026-10-09-0.16-F8-celular.md` |

## Etapa B — a casca

Espera a revisão da orquestradora e a liberação da `ui/` pela F5.
