# ADR 0038 — O teto da sala é contado, e quem hospeda é avisado

**Estado:** aceito
**Data:** 2026-08-31

`MAX_VOICE_ROOM_LIMIT` vale 250, e cada sala carrega um `limit: u16` que quem
hospeda escolhe. Os dois são números escritos à mão, e nenhum deles sabe nada
sobre o cano da casa onde o servidor está.

`specs/04-servidor-seele.md` dimensiona o servidor em «50 sessões e 5 VoiceRooms
ativos em 1 vCPU / 512 MB» — uma frase sobre **CPU e memória**, que é honesta
para um VPS e não diz nada sobre subida. Numa casa, a subida é o que acaba
primeiro, e o produto não avisava.

## Contexto

A conta que decide é fechada e o servidor tem todas as parcelas. Cada quadro de
voz é copiado para todo mundo menos quem falou, então com **N** na sala e **K**
falando ao mesmo tempo, a subida de quem hospeda é:

```
K × (N−1) × (payload + cabeçalho + rede) × 8 × 50 quadros/s
```

Com dez pessoas todas falando, a 48 kbps, isso é da ordem de **6,5 Mbps**.

Duas coisas mudaram desde que aquele `limit` foi escrito, e as duas empurram na
mesma direção:

- o [ADR 0036](0036-bitrate-adaptativo-em-faixas.md), deste mesmo dia, subiu o
  padrão de 32 para 48 kbps. O pior caso de qualquer sala **subiu 50%** junto, e
  esta é a primeira conta em que isso aparece;
- o commit `cb45bc1` deu ao servidor uma medida da própria subida — `Subida`,
  alimentada pelos contadores de cada conexão. Antes dele não havia com que
  comparar; agora há.

## Decisão

**Quando a sala cresce além do que a subida medida comporta, quem hospeda é
avisado — e ninguém é impedido de entrar.**

**1 · Avisar, e nunca recusar.** A subida é uma estimativa. Recusar entrada por
estimativa tranca a pessoa fora do servidor dela por causa de um número que o
servidor deduziu. Um número errado que avisa custa um aviso; um número errado que
recusa custa a sala. O `limit` que quem hospeda escreveu continua sendo o único
que barra alguém.

**2 · O gatilho é o pior caso passar do orçamento.** Pior caso é `K = N`, todo
mundo falando junto. Parece alarmista e não é: é exatamente o instante que
quebra. A sala funciona bem enquanto três falam, e pica **para todos ao mesmo
tempo** no momento em que os dez falam — que é um momento comum, não uma
patologia.

O orçamento é `subida medida × FRACAO_DO_CAMINHO`, os mesmos 60% que o §5.1 do
compartilhamento de tela já usa. Reusado, e não inventado: uma segunda margem
para a mesma máquina seria dois orçamentos discordando sobre o mesmo cano.

**3 · O aviso leva os dois números.** `AlertReason::VoiceRoomOverHostUplink
{ precisa_bps, medido_bps }`, enumerado como todo motivo deste protocolo, com os
valores em campos para a casca escrever a frase e traduzi-la. «Esta sala precisa
de 6,5 Mbps no pior caso, e a medida daqui é 4 Mbps» é acionável; «a sala está
grande» não é.

É o gêmeo de [`AlertReason::ScreenShareOverHostUplink`], que já existe pelo mesmo
raciocínio numa mídia diferente — com a diferença de que aquele **para** a
transmissão e este não para nada.

**4 · Só quem pode agir recebe.** O aviso vai a quem tem `AdministerServer`.
Quem entrou numa sala não decide nada sobre o cano da casa, e um alerta sobre
banda na tela de quem não pode fazer nada a respeito é ruído com aparência de
informação.

**5 · Só quando a resposta muda.** O aviso sai quando a sala **cruza** o limite,
e não a cada pessoa que entra depois disso.

**6 · Sem medida, sem aviso.** `Subida::medida()` devolve `None` até uma janela
cheia mover a estimativa. Ali o servidor não sabe o que tem, e a regra da casa é
que «não sei» nunca vira número inventado — o mesmo `——` que o resto do produto
mostra onde não mediu.

## Alternativas

- **Recusar a entrada** quando o pior caso não cabe. Recusada pela razão 1: é
  uma estimativa decidindo quem entra na casa de alguém.
- **Ajustar o `limit` da sala sozinho**, escrevendo no banco o que a conta diz.
  Recusada por ser pior que recusar: muda um valor que quem hospeda escolheu, e
  no dia em que a medida estiver baixa por um motivo passageiro, a sala encolhe
  sem que ninguém tenha pedido — e não volta ao que era.
- **Usar `GetCommonLinkProperties` do UPnP**, que devolve a taxa da WAN de
  graça. Recusada, e esta merece explicação: aquele número é a taxa **nominal**
  que o roteador declara, e roteador declara a porta Ethernet. Uma casa com 20
  Mbps de subida real anuncia 1 Gbps. Seria um segundo medidor discordando do
  primeiro, que é o defeito que o §3.2 do compartilhamento de tela nomeia — e
  discordando **para mais**, que é o pior sentido: o aviso deixaria de sair
  exatamente nas casas que mais precisam dele.
- **Avisar sobre o caso esperado** (dois ou três falando) em vez do pior. Diria
  menos e avisaria menos vezes, mas o instante que quebra é o pior caso, e um
  aviso que não cobre o instante que quebra não serve para planejar.

## Consequências

Quem hospeda numa casa passa a saber, no momento em que a sala cresce, que ela
não cabe — com os dois números para decidir o que fazer: cortar gente, cortar
qualidade, ou mover o servidor para um VPS.

Ninguém é impedido de nada, e nenhum valor escolhido por uma pessoa é
sobrescrito.

O custo é um `AlertReason` a mais no protocolo. **Não custa versão nova:** o
`PROTOCOL_VERSION` subiu para 2 hoje, pelo ADR 0036, e a v2 ainda não saiu em
release — a última tag é `v0.6.1`. As duas mudanças de fio entram na mesma
versão.

Fica registrado o que **não** foi feito: a conta supõe que todo mundo fala no
mesmo bitrate, o do teto. Com o ADR 0036 no lugar, quem está com rede ruim já
está mandando menos, e o pior caso real é um pouco menor que o calculado. Errar
para cima num aviso é o lado seguro de errar, e medir o bitrate de cada um para
refinar isso seria estado por participante para melhorar um número que já é
conservador de propósito.

## Custo de reverter

**Baixo.** Uma função pura, um `AlertReason` e um braço no laço de eventos.
Nada é persistido, nada muda de valor, e nenhum caminho de entrada depende
disto: arrancar tudo devolve o comportamento de hoje, que é não avisar.
