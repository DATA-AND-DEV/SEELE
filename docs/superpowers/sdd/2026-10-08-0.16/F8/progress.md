# F8 — A tela do celular: andamento

Branch `0.16/celular`, saído de `versao/0.16` em `f7bc902b` (09/10). Spec-mestre:
`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F8 e D-celular-1.

## Etapa A — levantamento e desenho

| passo | estado | onde |
|---|---|---|
| o duble da casca volta a chegar à sessão (pedido da orquestradora) | **feito** em `5d2e6681`: os 28 roteiros de `tools/roteiros/` passam; a causa dos 17 que reprovavam era a pergunta do nome da `acbc1c29` | `tools/carga-da-casca.py` |
| a matriz de paridade | **feita**: 467 linhas e os 159 `invoke`, com o que a casca chama sem gesto medido no Chromium | `F8/paridade.md` |
| as perguntas de desenho | **respondidas** pela orquestradora em 09/10 (navegação, movimento, tema, o que fica fora, SEGURAR PARA FALAR, `operador.barra`, arquitetura híbrida) | mensagens de 09/10 |
| o protótipo | **feito**: 96 casos (31 estados, 390 e 360, quatro temas, texto a 150 %, a casca do celular numa tela grande), com as medidas de toque, rolagem, áreas seguras, texto fora da caixa, contraste de todo texto e núcleo; os 96 passam | `spikes/casca-do-celular/` |
| a prova sobre a casca | **feita**: o desktop igual pixel a pixel com os arquivos do celular, no Chromium e no WebKit; a aba PESSOAS da `#lista-roster` (980 px sem a meta de viewport, 390 com); o MUDO da faixa pelo `#botao-mudo`; a casca larga sem a barra da janela e com HOSPEDAR AQUI virando frase | `spikes/casca-do-celular/prova-sobre-a-casca/` |
| o spec | **feito** | `specs/2026-10-09-0.16-celular-design.md` (`001b032d` e o commit do plano) |
| o plano | **feito**: 19 tarefas, com os guardas conferidos num crate de rascunho (os sete passam e cada um reprova na sua reversão) | `plans/2026-10-09-0.16-F8-celular.md` |
| as fotos no Safari do Simulador | **feitas em parte** (iPhone 17, iOS 27, numa aba): fontes, glifos, área segura de cima, temas e o texto do sistema passam; a foto com o tamanho de acessibilidade achou os rótulos do núcleo fixos, e eles passaram a crescer até 1,4×. Faltam o web app instalado, o teclado de verdade e o deitado | spec §12, `b32b2043` |
| o PC no navegador (D-web-3) | **no desenho**: a marca com dois valores (`celular`, `larga`), a coluna PC na matriz, o que fica de fora em dois grupos, a §2.8 do spec e as Tarefas 17 a 19 do plano | `aba56719` |
| a revisão da orquestradora | **aprovada** em 09/10, com três decisões (o link `https://seele.app.br/#baixar`, a ordem da Etapa B, a medida antes da Tarefa 9) e três ajustes: a foto da casca larga, a frase do hospedar na tela de início, e a meta de viewport no WebKit | mensagem de 09/10 |
| os ajustes da revisão | **feitos**: a casca larga montada e fotografada sobre a `ui/` de hoje; as fotos da entrada, da sessão, do aparelho e dos temas refeitas, com o link para baixar o app; as frases do grupo SÓ NO APP DO SEELE alinhadas na matriz; a meta de viewport inerte no Chromium e no WebKit (até `width=600` dá igual), e por isso o P1 fica no `index.html` | este commit |

## Etapa B — a casca

Liberada pela revisão de 09/10. As Tarefas 1 e 2 (os guardas e a bancada) andam já, só com arquivos novos. Da 3 em diante, a bancada injeta os pontos P1 a P10 numa cópia até a orquestradora liberar a `ui/`, depois das ondas 5 e 6 da F5.
