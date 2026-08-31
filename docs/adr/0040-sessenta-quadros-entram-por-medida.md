# ADR 0040 — Sessenta quadros entram, por medida

**Estado:** aceito
**Data:** 2026-08-31

`Cadencia` ganha um quarto degrau, `Q60`. O padrão continua sendo 30.

## Contexto

O §6 item 10 do [design do compartilhamento de tela](../superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md)
punha três coisas fora da primeira versão numa linha só:

> **HDR, mais de 1080p, mais de 30 quadros.** Nada disso cabe no §2 nem no §3.

Duas razões, e elas não são a mesma. O §2 é codec e captura — CPU. O §3 é
transporte — banda.

O pedido veio de quem usa, depois de um teste em LAN: *«ainda tá meio ruim a
pixelização do vídeo, gostaria de algo mais fluido possível.»*

## O que mudou no §2, e é medida

A conta que fechava a porta era de CPU, e ela era categórica. No Ryzen 7
5800X3D — a máquina do teste — o caminho de captura do Windows gastava
**17,69 ms por quadro** convertendo 1440p em 1080p. A 60 quadros um quadro chega
a cada **16,6 ms**: a thread ficava permanentemente para trás. Não era difícil,
era **impossível**.

O commit que trocou aquele laço por `dcv-color-primitives` e
`fast_image_resize` pôs isso em **7,42 ms**, com 55% de folga no intervalo. O
codificador custa 0,105 de núcleo a 1080p30 na mesma máquina; dobrando, 0,21.

O macOS nunca teve esse gasto — pede `420v` à ScreenCaptureKit e o compositor
entrega pronto.

## O que **não** mudou no §3, e é o que torna isto um degrau e não uma promessa

O §5 do mesmo documento diz a frase que governa aqui:

> «1080p a 1 Mbps e 720p a 1 Mbps gastam **o mesmo**.»

Sessenta quadros não pedem mais banda. Pedem **metade dos bytes por quadro**
dentro do mesmo teto. Num caminho estreito, quem escolher 60 vai ver **mais**
pixelização, não menos — que é o oposto do que o pedido pede.

Isto não é um efeito colateral a esconder: é a troca, e ela tem nome no
produto. `Prioridade { Nitidez, Movimento }` existe desde o commit `1acbdb3`
para quem compartilha dizer qual das duas cede primeiro.

## Decisão

**`Q60` entra como escolha, não como padrão.**

- 30 continua o padrão, e continua servindo o caso que o §2 nomeia — mostrar
  texto. Texto a 60 quadros com metade dos bytes fica pior.
- A nota da interface diz o preço em vez de vender o número: *«Em 60, cada
  quadro recebe metade dos bytes — mais fluido, menos nítido.»*
- O teto automático continua mandando: o que a pessoa escolhe é **máximo**, e a
  faixa automática desce quando o caminho não sustenta.

**HDR e mais de 1080p continuam fora do item 10, intocados.** Nenhuma medida
nova diz nada sobre eles.

## Uma dívida que isto pagou de passagem

O `palco-imagem.js` carimbava cada quadro com `carimbo += 33_333` — o intervalo
de 30 quadros por segundo escrito à mão, do lado de quem **recebe**. No dia em
que alguém transmitisse a 60, aquele relógio andaria na metade da velocidade
dos quadros que chegam.

É a mesma forma do defeito que custou a 0.8.5 em campo: um lado supondo o que o
outro decide. Passou a ser `performance.now()`, que anda no ritmo certo sem esta
janela precisar saber qual é a cadência.

## Alternativas

- **Não abrir, e responder o pedido com mais banda.** Recusada porque não é a
  mesma pergunta: mais banda melhora nitidez, e «picotado» é sobre quadros. O
  §5 já separa os dois controles justamente por isso.
- **Abrir e tornar 60 o padrão.** Recusada pelo parágrafo do §3: seria piorar a
  imagem de todo mundo que compartilha texto para melhorar a de quem mostra
  jogo.
- **Abrir 120.** Nada foi medido, e o intervalo de 8,3 ms não cabe nos 7,42 ms
  medidos com folga nenhuma para o codificador.

## Consequências

Um degrau a mais na lista, e um caso novo a manter: `Cadencia::TODAS` passou de
três para quatro, e todo `match` sobre ela foi conferido pelo compilador.

E uma expectativa a administrar: quem escolher 60 num caminho estreito vai ver
pior, e a nota da interface é a única coisa entre a pessoa e essa surpresa.

## Custo de reverter

**Baixo.** Tirar a variante e a opção. Quem tiver escolhido 60 volta para o
degrau abaixo pela mesma regra que já trata teto que não se sustenta.
