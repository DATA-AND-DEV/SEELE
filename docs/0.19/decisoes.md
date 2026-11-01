# 0.19 — decisões do dono

Uma entrada por decisão, com a data, as palavras do dono e o que mudou no
código. A decisão vale por cima de qualquer regra mais antiga que ela cite.

## 31/10/2026 — fechar com um pedido na portaria pergunta antes

> «Sim, deve confirmar com o usuário. Percebi que muita gente confunde a espera
> com não ter dado certo. Precisa ficar explícito que só está esperando a
> aprovação do host.»

- **Substitui, neste caso, a regra de 23/09** (`docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md`:
  «a confirmação ao fechar vale só para o servidor guardado»). Fechar a janela
  do app — o botão vermelho, o Alt+F4 e o Cmd+Q do macOS, cujo Sair fecha a
  janela (`trocar_o_sair_do_mac`) — com um pedido pendente na portaria de
  alguém (`esperaPendenteNaTela()`) mostra a caixa de `camada-sair-da-espera.js`:
  CONTINUAR ESPERANDO de padrão, «nada deu errado», e que fechar não cancela o
  pedido. Sem servidor no ar e sem pedido, fecha direto; com o servidor no ar,
  vale a pergunta dele, que diz quem cai junto.
- **O que o guarda da credencial continua protegendo** (`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`):
  as duas credenciais da tentativa são esquecidas antes de qualquer veto, e o
  `close-requested` cru segue proibido. Só a condição do veto mudou: servidor no
  ar, **ou** pedido pendente.
- **Não pergunta:** o Sair do Dock e o logout do macOS (vão direto ao
  `RunEvent::Exit`, sem janela), e o fechar da aba no navegador.
- **O navegador fica sem pergunta, de propósito.** O único jeito é o
  `beforeunload`, e ele só mostra a frase do navegador («sair do site? as
  alterações podem não ser salvas»), sem texto nosso. Ela diz o contrário do
  que é verdade — nada se perde: o pedido continua no servidor e o servidor na
  lista do aparelho — e reforça justamente a confusão que o dono quer tirar. O
  que o navegador ganha é a cabeça explícita da espera, abaixo.
- **A espera diz, acima de tudo e em letra grande, que só espera a aprovação**:
  «Esperando a aprovação de quem hospeda», e embaixo que não é carregamento nem
  erro. Na `#tela-auth` (desktop, casca larga) ela toma o lugar do «Entrar no
  servidor» na espera pendente; na «ENTRAR EM» do celular, vem acima do título,
  no laranja do que se lê, e não no vermelho da falha.

## 31/10/2026 — o pedido da reserva diz que é a reserva (API 8, parte 7)

Decisão da revisão de fechamento (sem palavras do dono; o pedido era «a máquina
de reserva precisa dizer que é a reserva»).

- **O papel vai no fio.** `ServerMessage::PedidoDeMaquina` ganhou `papel:
  PapelDaMaquina` no fim (`seele_proto::maquina`; o `Papel` do servidor passou a
  ser um apelido dele). O protocolo 12 não saiu em release nenhum (conferido com
  o `curl` do CLAUDE.md: a mais nova é a v0.18.3, protocolo 11), então o campo
  cabe, como o `quem` do `AlcanceTunelPor` (R6 da parte 2): só a linha
  `servidor:PedidoDeMaquina` de `fio_2_0.txt` foi trocada (agora com `Reserva`,
  para o byte do papel aparecer) e a semente `servidor-pedido-de-maquina` do
  fuzz regerada.
- **O papel entra no resumo do aceite** (`resumo_da_oferta(mod, hash, servidor,
  papel, oferta)`, rótulo `/2`). Ser a reserva muda o que a pessoa aceita: este
  computador guarda uma cópia do mundo e só roda o jogo se a principal sair. E
  o app **aceita sozinho** um pedido cujo resumo bate com o aceite guardado
  (`decidir`): sem o papel no resumo, o «sim» de reserva ligaria sozinho um
  pedido de principal com a mesma oferta. O aceite guardado leva o papel
  (`AceiteDeMaquina.papel`, ausente = principal), o FFI confere o resumo com ele
  (`PapelInvalido`, `ResumoNaoConfere`), e o `mudou` da tela diz «o papel deste
  computador». *Custo:* aceites guardados com o `/1` (só de builds de
  desenvolvimento) não batem mais, e a tela pergunta de novo uma vez.
- **A tela:** chip RESERVA no alto (ao lado das portas), título «… pede que
  este computador seja a reserva de um servidor de jogo», e a primeira linha
  «como reserva: este computador só roda o jogo se a máquina principal sair,
  com uma cópia do mundo». A principal não muda. Bancada `aceite-da-maquina.cjs`,
  caso 22.
- `api/v8.json` ganhou uma frase no fim de `mundo.maquinas.pedir` (acréscimo),
  com a linha da v8 refeita em `api/congeladas.sha256` e a cópia do contrato no
  MOD de multiplayer.

## 31/10/2026 — «o jogo segue rodando aqui» só com o jogo de pé

A faixa da máquina sem quem hospeda (`sem-anfitriao`) dizia «o jogo segue
rodando aqui» logo abaixo de «o jogo não está rodando aqui» — a reserva que
esperava, ou o jogo que caiu. Agora ela lê `jogosRodando` (o `maquina-jogo` de
`d6fd09f1`): com o jogo de pé, a frase de antes; sem ele, «quem hospeda o SEELE
saiu, e o jogo não está rodando aqui; se ele não chamar esta máquina de volta
em 10 minutos, ela é desligada». O caso 4b de `aceite-da-maquina.cjs` afirmava
a frase contraditória e passou a provar as duas.

## 31/10/2026 — as quatro bancadas de navegador que reprovavam na base da 0.19

As quatro estão no job `bancadas` do `ci.yml` e na bateria do
`empacotar/publicar.sh` (o `release.yml` roda só as de Node puro): reprovadas,
seguravam a publicação. Cada uma decidida entre «a bancada ficou velha» e «o
produto regrediu»; nenhuma deixou de afirmar a promessa dela.

- **`nucleo.cjs` — o produto regrediu (três vezes).**
  1. *O tema alcançava o núcleo.* A pele da 0.19 (`tela-sessao.css`) repintou o
     MUDO e o fone (`.operador-glifo`), o MUDO aceso, o veredito e o «no ar»
     com `--seele-*`, que o tema do servidor escreve. Consertado lendo os
     `--nucleo-*` (dois novos: `--nucleo-tinta-no-acento`, `--nucleo-acento-claro`)
     e devolvendo ao «no ar» a placa opaca do núcleo. O desenho sem tema não
     muda: os `--nucleo-*` resolvem para os mesmos tokens.
  2. *O «no ar» fora da vista.* Desde `b8dec1f8` ele é só do leitor de tela no
     computador (`clip-path`, 1 px), e o guarda 11 o tratava como à vista. **Aqui
     a bancada ficou velha:** ela agora separa o item fora da vista e exige que
     seja só o «no ar», e como `role="status"`.
  3. *Um aviso de MOD cobria o DEPOIS da portaria.* A faixa da portaria ficou
     larga o bastante na 0.19 para entrar na coluna dos avisos (z 70). Os avisos
     sobem acima da faixa enquanto ela está acesa (`--portaria-altura`, medida
     por `camada-portaria.js`; `mods-superficies.css`), sem mexer no z dela (a
     razão de `87908e41` continua).
- **`tema-do-servidor.cjs` — a bancada ficou velha (seletor), e o produto
  regrediu (fundo).** QUEM DESENHA O QUE trocou os botões por um seletor por
  lugar (canvas `Config-mods`): a bancada lê as ações nas opções (`data-rotulo`)
  e escolhe pelo `change`, com as mesmas ações exigidas e o seletor ligado. Mas
  medir a coluna de canais mostrou o defeito: as colunas viraram vidro, e atrás
  delas estava o `body`, com a cor da raiz — **um tema claro deixava o texto
  escuro dele sobre o negro do produto**. `#tela-sessao` (computador) passou a
  ter o fundo do `body` (a cor e a grade), lido onde o tema escreve: sem tema,
  nada muda. A bancada mede «a região não vazou» contra o mesmo tema sem
  regiões, e exige que o fundo do tema chegue atrás da coluna.
- **`navegacao-0-17.cjs` — a bancada ficou velha.** A cena
  `app-entrada-detalhes` abria `.boot-detalhes` e `#boot-nome-publico`, que a
  0.19 tirou de propósito (`01364d19`); `entrada-simples.cjs` prova que não
  voltam. A cena saiu, com o motivo no lugar dela.
- **`hospedagem-config-0174.cjs` — o produto regrediu.** O painel de vidro de
  «Antes de hospedar» crescia com o conteúdo (`flex: 1 0 auto`), a tela inteira
  rolava, e o HOSPEDAR e o VOLTAR ficavam abaixo da janela nas cinco cenas
  longas. Contra o esqueleto `.area` de `base.css` («o conteúdo rola na
  própria linha»). O painel ocupa a altura da janela, e não mais, e quem rola é
  o corpo; a decisão fica no pé dele, como o desenho da 0.19 pede. Só com
  420px de altura ou mais: abaixo disso (o 1024×768 a 200%) a cabeça e a barra
  comeriam o corpo, e a tela rola inteira como antes — `area-util.cjs` mede
  isso, e reprova as mesmas 29 medidas com e sem a mudança.

**As outras bancadas do job `bancadas`.** Rodadas as 88 desta máquina: além das
quatro, dez reprovam igual na base `2f163b18` (`conversa-com-estado`,
`qualidade-da-voz`, `avatares-do-mod`, `diagnostico-de-mods`,
`tema-claro-legivel`, `um-redesenho-por-quadro`, `linhas-pelo-id`,
`aviso-antes-de-atualizar`, `palco-na-troca-de-vista`, `vista-pessoal-0-17`), e
as de navegador com `.wasm`, `seeled` ou o Playwright do repositório não rodam
aqui sem os artefatos do CI. Ficam fora deste lote: seguram a publicação do
mesmo jeito, e cada uma pede a mesma decisão (bancada velha ou produto
regredido).

## 31/10/2026 — as bancadas que ainda reprovavam, com os artefatos do CI

Rodadas aqui como o CI as roda: o `.wasm` da página e o do motor
(`cargo xtask web`, o wasi-sdk conferido copiado de outro `target/`), o
`servidor_da_bancada`, a `portaria-da-bancada`, o `eco`, o `seele-encontro`,
o `cargo xtask pacote-web`, e o Playwright de Node que o job instala
(`npm install --no-save playwright`: hoje a 1.64.1, Chromium 156 e WebKit 27.2).
Cada uma decidida entre «a bancada ficou velha» e «o produto regrediu»; cada
conserto de produto foi provado revertendo-o e vendo a bancada dele reprovar.

**Bancadas velhas (o produto mudou de propósito; a promessa continua medida):**

- `avatares-do-mod.cjs` — esperava «três avatares quaisquer»; na 0.19 a lista
  de pessoas também veste o avatar, e o três passava antes de a conversa chegar.
  Espera as três superfícies por nome.
- `conversa-com-estado.cjs`, `tags.cjs` — o volume por pessoa saiu da linha do
  roster para a ficha da pessoa (`6445d953`); o gesto passa pela ficha.
- `qualidade-da-voz.cjs` — a leitura do teste de microfone é da geração que a
  pediu desde `fec5e0c4`; a bancada passa a geração.
- `diagnostico-de-mods.cjs` — `pessoa.acoes` mora na ficha (abre a ficha para o
  contorno); e o retrato não aparece na conversa do computador na 0.19, então o
  recorte é medido no corpo que um MOD desenha (`conversa.mensagem`).
- `um-redesenho-por-quadro.cjs` — a CSP do produto só deixa Worker de `blob:`
  (`95f670d4`); a rajada sai de um `blob:`.
- `linhas-pelo-id.cjs` — as linhas planas da 0.19 são mais baixas: cem
  mensagens, e não sessenta, dão a rolagem que o caso 14 prepara.
- `aviso-antes-de-atualizar.cjs` — a limpeza de textos (`959e49ae`) tirou o
  «Servidor 1.0/2.0» e o «WebRTC»; as frases novas são as conferidas.
- `palco-na-troca-de-vista.cjs` — «SEM COMPARTILHAMENTO» desde a 0.18.
- `vista-pessoal-0-17.cjs` e `proporcao-tela-cheia.cjs` — sem transmissão, o
  palco não existe (0.19, pedido do dono): a vista pessoal exige o palco
  escondido e o COMPARTILHAR na barra; a tela cheia é medida com uma tela na
  sala. A saída é um ícone (o nome no `aria-label`); «Recebendo ligações.»; as
  colunas da entrada contam-se pela borda esquerda (o CONECTAR desceu ao pé); o
  cartão «Ligações diretas» alinha pela borda com o formulário (`display:
  contents`), e não pelo `padding`.
- `visual-0174.cjs --extras` — o `<select>` da cor do cargo é `aria-hidden` e
  fora do Tab (quem escolhe são as amostras): fora do leitor e do Tab ao mesmo
  tempo não é controle medido.
- `tema-claro-legivel.cjs` — cada pastilha pelo piso dela (o EM ESCUTA é o
  apagado secundário, 3:1 pelo contrato), e o `color(srgb …)` do `color-mix` é
  lido em 0–1 (lido como `rgb()`, um fundo claro virava negro e o contraste
  mentia).
- `area-util.cjs` — o início do celular é a `.boot-caixa` de `#tela-boot` desde
  a 0.18 (a `.cel-entrada[data-inicio]` sai), e a lista de servidores dela vem do
  `conhecidos` da ponte web; o perfil é um diálogo de 560px no desenho
  (`Perfil.dc.html`), medido como diálogo.
- `entrada-simples.cjs` — o início do celular (o painel «Conectar»): o campo e
  o CONECTAR à vista a 390 e a 320, e o campo aceita o link.
- `cargos-pela-tela.cjs`, `sessao-no-navegador.cjs` — o apelido do retrato
  leva a tag da identidade desde a 0.18 («gi#VC8C»); o volume é escolhido pelo
  apelido inteiro.
- `pacote_web::tests` — a Saira Condensed saiu com a pele da 0.19; o
  versionamento é conferido na Inter.
- `tools/bancada-do-celular.py` — a marcação da 0.19 cita os arquivos das
  cascas em comentário; o guarda olha só o que carrega.

**O produto regrediu (consertos mínimos, no desenho da 0.19):**

- *O vidro tingia com o texto da região* (`tema-claro-legivel`): o compositor e
  a tira do operador misturavam `--seele-osso` lido no próprio contêiner, que é
  onde uma região escreve o texto. A tinta do vidro (`--seele-tinta-do-vidro`)
  é resolvida em `#tela-sessao`, onde o tema escreve.
- *O SAIR, o MUDO e o fone sumiam sob um tema claro* (1,09:1): o vidro
  translúcido deixava o fundo do tema passar sob o creme do núcleo. Ganharam a
  placa opaca do núcleo (`--nucleo-vidro`), no mesmo tom de antes sem tema.
- *O MUDO da pastilha a 1,15:1 sob tema claro*: o laranja-claro e o azul-claro
  da 0.19 não eram derivados. Sob tema, seguem o acento e o azul derivados.
- *Duas barras na ligação pessoal*: a cópia da barra da chamada (`74732140`)
  aparecia acima da do operador, que na ligação é a barra («sem cópia»). Sai na
  vista da ligação.
- *O modo de desenvolvedor não contornava `pessoa.acoes`*: na ficha, a caixa só
  existia com ações. No modo, ela existe vazia — é o contorno que mostra onde o
  MOD entra.
- *Os avisos do celular a 32% e 26% da altura* (`area-util`): a 320px o
  `dispensar` descia (o resumo já é de uma linha: não quebra mais), e deitado o
  alvo de 44 `!important` passava por cima dos 32px de lá (o token vale 32 ali).
- *«Antes de hospedar» numa coluna de 69%* com parágrafo de 100ch: o painel
  toma a largura (como no canvas), e o parágrafo volta à medida de leitura.
- *O foco por baixo do rodapé do perfil*: `scroll-padding-bottom` na altura do
  rodapé preso.
- *O teto do `.wasm` da página* (`cargo xtask pacote-web`, D-web-5): medido de
  novo — v0.18.1 2 215 156, hoje 2 406 940 (gzip -9 664 188 → 703 545), o
  protocolo 12 da API 8 e dos perfis. Teto a 2 455 000, ~2% sobre a medida.
  **Pede o aval do dono**: o teto existe para esta conversa.

## 31/10/2026 — as bancadas de navegador, segunda leva: roteiros, CI e a versão do Playwright

- **Os roteiros da casca (`tools/carga-da-casca.py`) — treze ficaram velhos**, e
  os 53 passam: a saída da sala é ícone (o nome no `aria-label`); a recusa do
  nome vai ao campo do nome (`#perfil-apelido-erro`); um código colado chama
  primeiro a ligação direta (0.18) — o duble responde a ausência dela, e o
  caminho medido é o da sala da 0.17.3; atender soma uma sessão (E3 da 0.17), sem
  `disconnect`; a pergunta de outra versão e a caixa de apagar canal saíram
  da limpeza de textos; as pílulas da caixa de compartilhar são a mesma
  pergunta do `<select>`; o estado da sala pessoal só fala no perfil (decisão de
  31/10); o banco de uma versão mais nova abre ANTES DE HOSPEDAR (E2 da 0.17); o
  BLOQUEAR da agenda acha-se pelo rótulo.
- **`pessoal-web-integrada.cjs`** — entrar na chamada pede o microfone desde
  `1df8ed98` (0.19): a bancada exige a captura no gesto, e o DESLIGAR à vista.
- **`pessoal-web-layout-0181.cjs` — o produto regrediu**: o respiro de vidro da
  0.19 cortava o CRIAR CONVERSA (320×360), o COPIAR LINK (568×320) e o
  «MICROFONE» transbordava o botão deitado (844×390). Abaixo de 400px de altura o
  palco perde o respiro, e deitado cada botão tem a largura do rótulo. A bancada
  passou a medir dois quadros depois da troca de fase (o rodapé medido no mesmo
  instante estava a 249px, e a 747px um quadro depois).
- **`aceite-dos-mods.cjs`, `tons-do-tema.cjs`** (Node puro, que o `release.yml`
  também roda) — a linha do MOD desenha o ícone da 0.19 (o contexto leva
  `iconeDoMod`); e o guia (`docs/como-se-faz-um-mod.md`) ensinava as cores da
  0.18 na função de referência: agora as de `tokens.css`.
- **`rtc-chamada-resiliencia.cjs`, `rtc-diagnostico.cjs`, `saida-pessoal-web.cjs`
  não rodavam em lugar nenhum** (`toda_bancada_da_pasta_roda_em_algum_lugar`
  reprovava). As duas de `node:test` entraram em `BANCADAS`; a de navegador, no
  `bancadas-de-mod` e no `publicar.sh`. A `rtc-diagnostico` ganhou o
  `createElement`/`replaceChildren` que a lista de etapas da 0.19 usa.
- **`compartilhar-pessoal-web.cjs` morria com «uso:»** no CI e no `publicar.sh`:
  o `.wasm` da página é argumento obrigatório dela, e agora vai.
- **O Playwright de Node fica fixo na 1.62.1** (`ci.yml`, e a frase do
  `publicar.sh`). O `npm install --no-save playwright` pegava a mais nova (hoje a
  1.64.1: Chromium 156, WebKit 27.2), e duas bancadas reprovavam só por isso: o
  Chromium 156 aceita o `WebAssembly.Module` pela porta do worklet, que a
  `voz-no-navegador.cjs` usa para provocar o `messageerror`, e o WebKit 27.2
  desenha diferente a captura redimensionada da `compartilhar-pessoal-web.cjs`
  (e a `pessoal-codigo-web.cjs` passa na 1.62.1 e não na 1.64.1). Na 1.62.1 as
  três passam. Subir a versão pede a rodada inteira medida, e uma provocação
  nova para o `messageerror`.
- **`area-util.cjs` no WebKit** — o `<select>` escondido da cor do cargo
  (`aria-hidden`, fora do Tab) não é medido como controle, o mesmo critério da
  `visual-0174.cjs`; as amostras, que são o controle, são.

**Ainda vermelho (31/10):**

- ~~`sessao-no-navegador.cjs`, caso 14~~ — resolvido: era a bancada. Ver
  «o caso 14 da sessão no navegador» abaixo.
- ~~`tools/bancada-do-celular.py`: 329 medidas reprovam~~ — triada abaixo
  («a bancada do celular, tela por tela»).
- ~~`chamada-web.cjs` não roda nesta máquina~~ — o órfão era da própria bancada:
  um erro no servidor estático matava o node sem passar pelo `finally`, e o
  `seele-encontro` ficava com a 19885. Agora ele morre em qualquer saída (0615e327).

## 31/10/2026 — o caso 14 da sessão no navegador: era a bancada (D-tocar)

Decidido na triagem, sem palavra do dono: **bancada velha, e não regressão do
produto.** O produto faz o que foi desenhado; a bancada media um instante.

**O que se mediu.** Cinco rodadas da `sessao-no-navegador.cjs` na d79ef846 (0.19
+ API 8): quatro verdes, uma vermelha no caso 14 (`parar: false`, sem aviso,
figura `pronta`, `midia_do_mod` e `som_do_mod` uma vez cada). Uma cópia com um
`MutationObserver` em volta do clique deu a linha do tempo:

```
0ms    clicado: o botão da FIGURA (do produto), e não o do MOD
3ms    figura PAUSAR · MOD TOCAR
110ms  figura PAUSAR · MOD PARAR      ← o MOD ouviu `tocando`
203ms  figura TOCAR  · MOD PARAR      ← o toque de 0,2 s acabou
204ms  figura TOCAR  · MOD TOCAR      ← o MOD ouviu `terminou`
```

**A causa, em duas partes, as duas da bancada:**

1. O `find(b => b.textContent === "TOCAR")` pega o **primeiro** TOCAR do
   painel. Com o som já carregado, é o botão que o produto põe na figura da
   mídia (`montarSom`, o que substitui o `<audio controls>`), que vem antes do
   botão `tocar` do MOD na árvore. O comentário dizia que o botão «vira PARAR
   pelo próprio MOD», e o clique não era no do MOD.
2. O PARAR que aquele clique produz vive do `tocando` ao `terminou`: uns 95 ms,
   num som de 0,2 s. A bancada amostrava o texto a cada 100 ms. Quando a
   amostra caía fora da janela, ela via TOCAR até o prazo de 10 s.

**Não é da API 8.** O 15fbd5f3 só trocou `"api": 7` por `8` no `mod.json`. O
cliente do MOD, o `toque.wav`, o caminho do som em `mods-regiao.js` e a conferência
da bancada são os mesmos desde a v0.18.1 (`ae0f2715c`, o commit gravado no
release; o da v0.18.3 não está no corpo dele). Com o `mod.json` de volta na API
7, a mesma cópia reprovou com a mesma linha do tempo (PARAR de 106 a 202 ms).
A corrida já estava lá antes. Ela passava quase sempre porque a janela é quase
do tamanho da amostra.

**O comportamento que fica, e por que é o certo.** O MOD de referência segue a
mídia pelo que o produto diz (`case "midia"`: `tocando` só com `tocando`). Um
toque que termina volta a TOCAR, e um botão que ficasse em PARAR depois do fim
seria um botão mentindo, que é o que aquele comentário do vetor proíbe.

**O que mudou na bancada.** O caso 14 aperta o botão do MOD
(`button[data-chave-do-mod="tocar"]`) e grava cada rótulo dele por
`MutationObserver` instalado antes do clique. Ele exige o ciclo inteiro,
`["TOCAR","PARAR","TOCAR"]`, sem `som: recusada|falhou`, com a figura `pronta`
(ou `tocando`). A volta a TOCAR é a prova de que o som tocou até o fim: a
conferência velha passava com um produto que nunca dissesse `terminou`.

**A prova, pelas reversões do produto, com a bancada nova:**

- sem o `avisar("terminou")` do `TocadorDeSomDeMod`: reprova,
  `rotulos: ["TOCAR","PARAR"]`;
- sem os bytes do `som_do_mod` (o `carregarSom` lançando): reprova,
  `som: 0`, figura `falhou`;
- com o produto intacto: três rodadas verdes seguidas.

## 31/10/2026 — a bancada do celular, tela por tela

`tools/bancada-do-celular.py` (CI, job `bancadas`, e o `publicar.sh`) reprovava
249 medidas nesta máquina (Playwright de Python 1.62.0, Chromium). A bancada não
rodava em lugar nenhum desde a 0.17, então parte do que ela afirmava já era da
0.17 e da 0.18. Cada grupo decidido entre «a bancada ficou velha» e «o produto
regrediu»; cada conserto de produto foi provado revertendo-o e vendo a bancada
reprovar (a medida que volta está entre parênteses). Nenhum teto foi mexido.

**O produto regrediu (consertos mínimos):**

- *A faixa de voz na barra de gestos durante a CHAMADA* (`chamada`, `deitado`).
  A fileira da telemetria que tomava o lugar das abas saiu na 0.18, e a faixa
  desceu ao pé, em cima da barra de gestos (fim 835 a 844×34). Com a CHAMADA
  aberta, a faixa leva a área segura de baixo na margem. (Revertido: «na barra
  de gestos: … fim=835».)
- *O aviso do `persist()` vazio comia 8 px* (`chamada`, `ajustes-medida`,
  `persistencia`): a margem dos avisos da 0.19 valia também para a região
  vazia. Vazia, sem margem. (Revertido: «a CHAMADA não cobre a sessão entre o
  topo e a faixa».)
- *A tinta sobre o acento cheio a 3,01:1 sob tema claro* (`chamada`,
  `aba-conversa` no claro: o ENVIAR e o «NÃO VER»). A pele da 0.19 usa um
  negro fixo (`--seele-tinta-no-acento`) sobre o acento; sob tema, a tinta é o
  fundo do tema, que o contrato (`PARES_DO_TEMA`) já põe a 4,5:1 do acento
  (`escreverOTemaDaSessao`, junto dos claros). Sem tema nada muda. (Revertido:
  «#botao-enviar 3.01:1».)
- *«juli fala · sinal 92» a 2,72:1 sob o claro* (`faixa-de-voz`,
  `aba-servidor`, `aba-pessoas`, `chamada`): a pele trocou o apagado do tema
  pelo segundo apagado (3:1 pelo contrato) num texto que se lê. Volta ao
  `--seele-rotulo-painel`. (Revertido: «voz-sala-quem 2.72:1».)
- *O «negada» do `persist()` cortado numa linha e dado por dito*
  (`persistencia`): a faixa «N AVISOS», que abria o texto inteiro, saiu na
  0.18, e o resumo de uma linha ficou para sempre — o `IntersectionObserver`
  gravava `seele.persistencia-dita` com a frase cortada. O aviso da
  persistência não é mais resumido (o do veredito continua, com o «ver
  inteira»). (Revertido: «'inteiro': False».)
- *Deitado, o palco com o respiro do cartão* (`deitado`): 4 px 16 px em volta
  da tela que deveria tomar o espaço acima da faixa. Sem margem ali, como o
  palco da ligação pessoal abaixo de 400 px. (Revertido: «palco [4, 282, 812]».)
- *O volume da folha da pessoa não chegava ao áudio* (`aba-pessoas`): a ficha
  da 0.19 no celular mora em `#cel-folha-pessoa`, fora de `#ficha-pessoa`, onde
  o ouvinte do volume estava; o deslizante andava e nenhum `set_volume` saía.
  Um ouvinte só (`mandarOVolumeDaFicha`) nos dois, e o guarda de `frontend.rs`
  passou a exigir os dois. (Revertido: «set_volume []».)
- *A queda nunca aparecia no celular, e o `status.barra` nunca avisava*
  (`queda`, `regioes-do-celular`): o celular tira o rodapé da telemetria desde
  a 0.18, e `desenharTelemetria` retornava logo sem ele — levando junto o
  `desenharEnlace` (a página «Conexão perdida») e o `regiaoAusente` do
  `status.barra`. Sem o rodapé, os dois continuam. (Revertido: «a queda não é a
  página no alto: [0, 0]» e «não recebeu regiao-ausente».)
- *A página da queda empurrava a conta para fora da tela* (`queda`): o
  `flex: 1 0 100%` do desktop, numa coluna, fazia a frase ter 575 px, o enlace
  zero, e a conta (y 836) e as tentativas (y 967) sob a faixa. (Revertido:
  «conta não fica à vista acima da faixa».)
- *Com o texto do sistema a 2, quatro coisas passavam da tela ou da caixa*
  (`texto-maior`): as iniciais do cartão da CHAMADA (46 > 44; ganham o teto de
  22 px que o `TEXTO_COM_TETO` já declarava), «SALA DE VOZ · TÓQUIO-3» (quebra),
  ABRIR COMO COMPUTADOR e INSTALAR MOD DE UMA PASTA (quebram, até a largura de
  quem os contém). (Cada um revertido reprova a medida dele.)

**Bancadas velhas (o produto mudou de propósito; a promessa continua medida):**

- *A telemetria da CHAMADA e a seção CONEXÃO* saíram na 0.18 (`5dfe9c59`). A
  CHAMADA confere que as abas saem, o MUDO fica na mesma coluna e tamanho, e a
  faixa não desce além do pé das abas; AJUSTES sem CONEXÃO, com CARGOS E
  PERMISSÕES (E6 da 0.17) na lista; «VOZ» virou «AO FALAR» (`49758837`).
- *O aviso de fone* não existe no celular desde a 0.18 (frase vazia): confere-se
  que não acende. *O «concedida» do `persist()`* é calado pelo celular desde a
  0.18 (`faixaDeAvisos`): confere-se que não aparece nem fica dito.
- *O respiro de 8 px da 0.19* (`120ea48c`) entre os avisos e a faixa, e entre o
  compositor e o teclado: «colado» é colado pela margem.
- *A pessoa* (`6445d953`, `352bed15`): uma linha por pessoa, sem o sinal por
  pessoa; a folha é a ficha da 0.19 (`.ficha-volume-valor`,
  `.ficha-moderar-fora` com a frase do produto); o volume confere o
  `set_volume`, e não o deslizante da linha, que saiu. Os detalhes de MOD na
  folha são irmãos da linha do produto, e a ficha é montada ao abrir (no
  computador também): o que chega com ela aberta aparece na abertura seguinte,
  nunca em duplicata nem fora dela.
- *O palco só existe com alguém transmitindo* (0.19): o `deitado` põe a tela de
  juli na sala. *A queda é uma página* (`Cel-reconectando.dc`, `49758837`): o
  título, a conta e as tentativas à vista acima da faixa, e a faixa com o MUDO.
  *A confirmação é um cartão no meio* a 12 px das bordas desde a 0.18.
- *A coluna de 480 px* saiu na 0.17 (`fec5e0c4`, «a coluna estreita centrada
  que o dono recusou»; `--cel-coluna` não existe mais): `tela-larga` exige a
  superfície inteira e as abas como grupo de até 160 por alvo, no meio;
  `lados-das-superficies` mede as áreas laterais sem teto.
- *A entrada:* o início do celular é a `.boot-caixa` desde a 0.18 (o mesmo de
  `area-util` e `entrada-simples`): o campo do link e o CONECTAR à vista, com
  44. Numa aba também se entra (`8284b642`). O analisador não tem mais o
  `SemHashesDoNavegador`, e o estado `servidor-0.15` saiu do caso. Os
  servidores guardados são conferidos pela identidade (`conhecidos`,
  `lembrarONome`, `entrarNoConhecido` sem convite); o «LINK VENCIDO» de um
  guardado saiu na 0.17. A casca larga: o `.boot-proposito` saiu na 0.18.
- *A sala pessoal no navegador* existe desde a 0.17.4 (`5dfe9c59`: «Código
  pessoal e agenda também estão disponíveis no navegador»): o caso agora
  reprova se a casca voltar a esconder o código ou a agenda.
- *A capa no cartão da CHAMADA*: o cartão simples da 0.19 põe o nome sobre o
  véu da capa, e não numa placa. O contraste é medido através do véu (o
  `::after` da `.capa-camada`, composto sobre a capa), com o piso de 4,5:1 sob a
  branca (6,34), a preta e a da tinta; o «contraste igual com qualquer capa»
  era da placa, que saiu.
- *Duas medidas da bancada ficaram mais exatas, e não mais frouxas*: «texto
  passa da caixa» exige que um nó (texto ou filho) passe da caixa de conteúdo —
  o selo de quem fala, um `::after` posto 5 px fora do avatar de propósito,
  não é texto (antes: «RA 49>44»); e uma caixa só do leitor de tela
  (`clip-path: inset(50%)`, o rótulo do compositor) não é «à vista». As duas
  continuam pegando o transbordo de verdade: as reversões do `texto-maior`
  acima reprovam por elas.

**O G3 oscilava, e não por causa da 0.19** (o desktop igual pixel a pixel com
os arquivos das cascas): no Chromium, duas rodadas em cinco diferiam em 33
pixels, de 1 a 4 níveis, nas bordas do minimizar e do maximizar da barra da
janela — igual na base `d79ef846`, com as duas cópias de layout idêntico. A
barra ainda assentava depois do `entrar`; com 1,5 s a mais antes da foto, oito
rodadas em oito iguais. Nenhuma máscara nova nem tolerância: continua pixel a
pixel.

**Fica para o dono:** a ficha da pessoa (computador e celular) é montada ao
abrir, e um `pessoa.detalhes` que chega com ela aberta só aparece na próxima
abertura; e o `celular.rs` reprova na base (`d79ef846`) em
`o_texto_do_sistema_alcanca_todo_tamanho_do_desktop` («regere com
SEELE_REGERAR_TEXTO=1»), fora desta triagem.
