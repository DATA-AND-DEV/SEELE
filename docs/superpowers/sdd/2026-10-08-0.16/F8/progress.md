# F8 — A tela do celular: andamento

Branch `0.16/celular`, saído de `versao/0.16` em `f7bc902b` (09/10). Spec-mestre:
`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F8 e D-celular-1.

## Etapa A — levantamento e desenho

| passo | estado | onde |
|---|---|---|
| o duble da casca volta a chegar à sessão (pedido da orquestradora) | **feito** em `5d2e6681`: os 28 roteiros de `tools/roteiros/` passam; a causa dos 17 que reprovavam era a pergunta do nome da `acbc1c29` | `tools/carga-da-casca.py` |
| a matriz de paridade | **feita**: 469 linhas (as duas da abertura do áudio vieram com a `versao/0.16` de `c8b3cd47`) e os 159 `invoke`, com o que a casca chama sem gesto medido no Chromium | `F8/paridade.md` |
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

Liberada pela revisão de 09/10. Da Tarefa 3 em diante, a bancada injeta numa cópia os pontos que a `ui/` ainda não tem, até a orquestradora liberar a `ui/`, depois das ondas 5 e 6 da F5. O P2 e o P3 (as linhas que carregam cada arquivo novo) estão liberados: cada arquivo novo entra com a linha dele, num commit isolado.

| tarefa | estado | onde |
|---|---|---|
| 1. os guardas de texto e a raiz do celular | **feita**: os cinco guardas de `tests/celular.rs` passam, e cada um reprova na sua reversão (seis); o `frontend` (272) e o `tokens` (3) passam com o P2 e o P3 | `790e4ab5`, `4ec186df` (o `index.html`, isolado) |
| 2. a bancada do celular | **feita, menos o CI**: o G3 igual pixel a pixel no Chromium e no WebKit, os pontos P1, P5 e P6 injetados na cópia, as medidas do protótipo e os temas pelo produto; o caso `sessao-entrou` passa a 390 e a 360; as reversões reprovam (a regra dos 44 sem a marca, o script sem a guarda, o `CELULAR` ausente); o duble ganha o `abertura_do_audio`, e os 28 roteiros passam. A bancada é a 13ª etapa da bateria da integração, com o WebKit | `b9abb0f1` |
| 3. a moldura: as abas e a faixa de voz | **feita**: o G6 passa; o caso `faixa-de-voz` passa a 390 e a 360, sem tema e nos quatro temas, com a abertura do áudio e o `#aviso-audio` colado na faixa; cinco reversões reprovam (uma só depois de a medida do aviso apertar); o P4 saiu, e a moldura nasce no `celular-voz.js`; o G3 continua igual nos dois motores, e os 28 roteiros passam | `ea514692`, `1bd06f45` (o `index.html`, isolado) |
| 4. SEGURAR PARA FALAR | **feita**: o caso `segurar` confere as cinco saídas pela sequência de `set_talking` (soltar, o toque cancelado, o dedo fora, a página oculta, o enlace caído), e cada saída tirada faz o caso reprovar na dela | `917b4289` |
| 5. a aba SERVIDOR | **feita**: o caso `aba-servidor` passa a 390 e a 360, sem tema, no PAPEL e no hostil (o claro espera a Tarefa 13); o topo com o estado da conexão entrou em toda aba; o ENTRAR MUDO espera o quadro dizer «mudo»; a folha SENHA DA SALA, com o P7 que cancela no cancelar; três reversões reprovam | `e4f1c853`, `d127a9a6` (o `index.html`, isolado) |
| 6. a aba CONVERSA | **feita**, menos o ARQUIVO e a FOTO OU VÍDEO de verdade, que esperam a ponte de arquivos da F4: o Return quebra a fileira (P8), a folha da mensagem com COPIAR e APAGAR, a folha ANEXAR, a mensagem recusada numa placa do núcleo; o caso `aba-conversa` passa a 390 e a 360, sem tema, no PAPEL e no hostil; três reversões reprovam | `b0874d0d`, `31cca51b` (o `index.html`, isolado) |
| 7. a aba PESSOAS | **feita**, menos o FORA DO AR (espera o snapshot, F1/F5): o sinal numa fileira, o volume e a frase de moderar na folha da pessoa; o caso `aba-pessoas` passa a 390 e a 360, sem tema, no PAPEL e no hostil; duas reversões reprovam | `17a3d285`, `30bd83a2` (o `index.html`, isolado) |
| 8. a CHAMADA e o palco | **feita**, menos o deitado (Tarefa 12), a capa (F3) e o «não decodifica» (F4): a CHAMADA entre o topo e a faixa, o MUDO no mesmo lugar, RECOLHER, a telemetria no lugar das abas, a grade em duas colunas e o cartão que abre a folha da pessoa; o caso `chamada` passa a 390 e a 360, sem tema, no PAPEL e no hostil; duas reversões reprovam | `03d36e1f` |
| 9. AJUSTES | **feita**, menos a capa e o `operador.barra` (F3) e ESTE APARELHO e a chave curta (F4): a medida passou sem P11; SEU PERFIL, a ordem do spec, CONEXÃO com as sete medidas, O QUE O CELULAR NÃO FAZ nos dois grupos, o modo SEGURAR PARA FALAR, SOBRE e SAIR DO SERVIDOR; os casos `ajustes-medida` e `aba-ajustes` passam a 390 e a 360; quatro reversões reprovam | `295ea5b6`, `9e430f9e` (o `index.html`), `0a4f9e3d`, `ed9fdbef` |
| (o «VER —» do desktop) | a escolha entre transmissões diz de quem é cada tela, nas duas linhas, a pedido da orquestradora | `c852df6c` (0.16), `6035d847` (`conserto/nome-na-transmissao-0.15`) |
| (consertos no caminho) | o SEGURAR lia o enlace como `"Up"`, que só o duble dizia, e soltava no primeiro quadro (`0385b08f`); o duble passa a guardar o volume, e a bancada deixa de reprovar às vezes na árvore da F3 (`da0b74d3`) | `0385b08f`, `da0b74d3` |
