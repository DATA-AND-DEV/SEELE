# F8 — A tela do celular: andamento

Branch `0.16/celular`, saído de `versao/0.16` em `f7bc902b` (09/10). Spec-mestre:
`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F8 e D-celular-1.

## Etapa A — levantamento e desenho

| passo | estado | onde |
|---|---|---|
| o duble da casca volta a chegar à sessão (pedido da orquestradora) | **feito** em `5d2e6681`: os 28 roteiros de `tools/roteiros/` passam; a causa dos 17 que reprovavam era a pergunta do nome da `acbc1c29` | `tools/carga-da-casca.py` |
| a matriz de paridade | **feita**: 467 linhas e os 159 `invoke`, com o que a casca chama sem gesto medido no Chromium | `F8/paridade.md` |
| as perguntas de desenho | **respondidas** pela orquestradora em 09/10 (navegação, movimento, tema, o que fica fora, SEGURAR PARA FALAR, `operador.barra`, arquitetura híbrida) | mensagens de 09/10 |
| o protótipo | **feito**: 95 casos (31 estados, 390 e 360, quatro temas, texto a 150 %, tela larga), com as medidas de toque, rolagem, áreas seguras, texto fora da caixa, contraste de todo texto e núcleo; os 95 passam | `spikes/casca-do-celular/` |
| a prova sobre a casca | **feita**: o desktop igual pixel a pixel com os arquivos do celular; a aba PESSOAS da `#lista-roster` (980 px sem a meta de viewport, 390 com); o MUDO da faixa pelo `#botao-mudo` | `spikes/casca-do-celular/prova-sobre-a-casca/` |
| o spec | **feito** | `specs/2026-10-09-0.16-celular-design.md` (`001b032d` e o commit do plano) |
| o plano | **feito**: 16 tarefas, com os guardas conferidos num crate de rascunho (os sete passam e cada um reprova na sua reversão) | `plans/2026-10-09-0.16-F8-celular.md` |
| as fotos no Safari do Simulador | **espera o dono**: o acesso ao iPhone 17 | spec §12 |

## Etapa B — a casca

Espera a revisão da orquestradora. Os arquivos novos entram com ela; os existentes, pelos pontos P1 a P10, com a liberação da `ui/` pela F5.
