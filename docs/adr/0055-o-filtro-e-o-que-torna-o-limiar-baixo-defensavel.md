# 0055 — O filtro é o que torna o limiar baixo defensável

Status: **aceito**
Data: 2026-09-22
Revisado em 22/09/2026 por `docs/auditoria-f01-f02-2026-09-22.md`.
Sobre `docs/features-v15.md`, F01 e F02.

> **A primeira versão deste ADR defendia −60 dBFS como número fixo, e estava
> errada.** Uma auditoria independente mediu o portão aberto em **200 de 200
> quadros** de ruído sem fala nenhuma, com o filtro ligado. O erro não era o
> número: era escolher um número fixo para o ruído da sala de outra pessoa. O que
> este ADR registra agora é o padrão passando a ser um **alvo** e o limiar passando
> a **medir**. A seção «O padrão depende do filtro, e o limiar depende da sala»
> tem a medida e a correção.

> **F01 e F02 são uma entrega, e não duas.** O pedido de −60 dBFS foi recusado uma
> vez com um argumento correto: sem supressão de ruído, aquele limiar fica abaixo
> do piso da sala e o ventilador segura o canal aberto o dia inteiro. O que este
> ADR registra é a supressão entrando — e, com ela, o limiar passando a caber.

## O argumento que recusou −60 dBFS, e por que ele valia

`gate::OPEN_RMS` carrega o registro: o pedido original era −60 dBFS, o gate foi
para −42, e a razão está escrita ali. Um gate de −60 dBFS é o padrão de gates que
rodam **depois** de supressão, onde o piso da sala já foi removido. O deste
produto era a única defesa, e `room_tone_does_not_open_the_gate` modela piso de
sala em −48 a −43 dBFS: −60 dBFS fica 17 dB **abaixo** disso.

Aquele parágrafo terminava em «noise suppression is what buys the rest». É o que
mudou.

## A forma

`crates/seele-audio/src/supressao.rs`: subtração espectral com rastreamento de
mínimos por raia. STFT com janela de um quadro, salto de metade, raiz de Hann nas
duas pontas.

**Rust puro, zero crate novo na árvore.** O `realfft` já vem pelo `rubato`, que
`seele-audio` usa desde sempre. Isso não é economia — é o que permite a supressão
existir nesta v1: o ADR 0007 tira DSP em C do escopo, e o ADR 0015 registra a
mesma decisão para o `webrtc-vad`.

### O que foi recusado, e o que custaria

Uma porta de RNNoise — `nnnoiseless` — seria mais eficaz contra ruído **não
estacionário**, que é onde a subtração espectral é fraca. Ela traz um modelo, uma
licença e um arquivo a empacotar em três sistemas. Não está descartada; está fora
desta entrega, e a costura para trocar o mecanismo é a fronteira de
`Supressao::processar`.

### O que ela mede, e o que ela não mediu

`o_chiado_cai_mais_que_o_tom`, com chiado de banda larga e um tom de 300 Hz por
cima: **sobra 29% do ruído** — 10,6 dB de atenuação — e **95% do tom**. Os números
saem impressos no teste, e é deles que `EXCESSO` e `ALISAMENTO` vieram.

**Isto é a razão entre dois sinais sintéticos, e não uma medida de voz.** «95% da
voz» é o que este ADR dizia, e a frase é generosa com ela mesma: o que sobreviveu
foi um tom senoidal de 300 Hz, que é o caso mais fácil que existe para uma
subtração espectral — energia numa raia só, estacionária, sem consoante nenhuma.
Fala de verdade tem transiente, e transiente é onde a subtração espectral é fraca.

Os outros números medidos, todos impressos pelos testes e todos sintéticos:

| O quê | Medida |
|---|---|
| ruído que sobra depois de meio segundo de silêncio digital | 21% |
| ruído que sobra quando ele reaparece depois de sumir | 19% |
| fala que sobra quando começa depois do silêncio | 102% — nada foi comido |
| nível residual do ruído depois do filtro | **−59,0 dBFS** |

A última linha é a que derrubou o padrão de F01, e está abaixo.

Isto é sinal sintético. A avaliação que `features-v15.md` pede — gravações reais
de fala baixa, normal e distante, teclado, ventilador, headset, microfone de
notebook, com CPU, memória e latência medidas por plataforma, inclusive durante
compartilhamento de tela — **não aconteceu**. Este ADR não a substitui, e o
critério 3 de F02 continua aberto.

### Duas medidas que estão no código porque foram medidas

O caminho até os números atuais tem duas reprovações registradas nos docs:

1. **média exponencial partindo de zero** subindo 0,2% por salto: em meio segundo
   de chiado o piso alcançava 9% do ruído, e o teste reprovava com 82,5% do ruído
   intacto. Partir de zero e subir devagar é não estimar nada;
2. **rastrear o mínimo da magnitude crua**: o mínimo de um sinal ruidoso fica
   muito abaixo da média dele, e sobravam 79,5%. Alisar antes de rastrear é o que
   aproxima o mínimo da média.

## Latência

Um salto: **10 ms**. É o preço da sobreposição de 50%, e
`Supressao::atraso_em_amostras` é o número em código. Desligada, a supressão
devolve o que entrou sem passar pela FFT — zero atraso, zero diferença no sinal.

## A ordem, e ela é a coisa toda

`captura → supressão → portão → ganho → codificação`, que é a ordem que
`features-v15.md` propõe. Presa por
`voice::o_caminho_do_microfone_esta_na_ordem_de_f01_e_f02`, e cada fronteira tem
um sintoma próprio:

- **supressão depois do portão** desfaz F01 inteiro: o portão volta a medir a
  sala, e o padrão novo mantém o canal aberto o dia inteiro — pior que antes;
- **ganho antes do portão** faz ruído de sala virar fala, que é o defeito que
  aquele teste já existia para impedir.

## O padrão depende do filtro, e o limiar depende da sala

`ABERTURA_COM_SUPRESSAO_DBFS = -60` e `ABERTURA_SEM_SUPRESSAO_DBFS = -42`, e é
`configuracao_do_portao` que escolhe entre os dois. Desligar a redução de ruído
**devolve o limiar antigo**, porque ali o portão volta a ser a única defesa.

### A conta que eu fiz, e a medida que a desmentiu

A primeira versão deste ADR raciocinava assim: um piso de sala de −45 dBFS vira
−58 dBFS depois de 12,8 dB de atenuação, então −60 dBFS deixa de ser 17 dB abaixo
do ruído e passa a ser 2 dB abaixo. «É pouca folga», dizia a frase, e seguia em
frente.

Dois decibéis de folga **não são folga**: são a margem de erro da própria conta. A
auditoria mediu o que acontece com ela, com ruído de amplitude 0,01 e o filtro
inteiro ligado:

> Após um segundo de aquecimento, o gate permaneceu aberto em **200/200 quadros**,
> com nível médio residual de **−58,03 dBFS**. Não havia tom nem fala no sinal.

Este ADR media 12,8 dB de atenuação e concluía «cabe». O que faltava era medir o
**residual em dBFS** em vez de a razão entre entrada e saída — e o residual é o
número que o portão compara. É a terceira linha do `CLAUDE.md`: medir antes de
concluir, e a razão não era a medida.

### O padrão virou um alvo

`GateConfig::automatico` é o caminho do padrão, e nele −60 dBFS é o **mais
sensível que o portão chega**. O limiar de verdade é o ruído medido mais
`gate::MARGEM_SOBRE_O_RUIDO_DB`, que são **9 dB** — a distância entre «o que
sobrou do ventilador» e «alguém falando baixo».

O ruído é medido como a supressão mede o dela: o **mínimo do último segundo**,
agora do nível do quadro inteiro em vez de por raia. Todo quadro entra na janela,
aberto ou fechado. Medir só os fechados parece óbvio e trava: no caso da
auditoria o portão fica aberto em 200 de 200 quadros, e sem quadro fechado não há
medida, e sem medida o limiar fica fixo — que é o defeito. O mínimo é o que
permite receber tudo: quadro de fala alta não estraga o mínimo do último segundo,
porque o vale entre duas sílabas está lá dentro.

Os dois limites do limiar medido não são detalhe: por baixo ele nunca fica mais
sensível que o alvo, e por cima nunca passa do extremo surdo da faixa. Sem eles a
medida realimentaria a si mesma.

**Uma escolha manual continua fixa.** `GateConfig::de_dbfs` não mede nada: F01
pede o ajuste manual, e um ajuste que o produto corrige por cima não é ajuste. A
faixa é de −72 a −24 dBFS.

### Medido depois, com o par junto

`crates/seele-audio/tests/supressao_e_portao.rs` roda o caminho de F01 e F02 sem o
codec — `captura → supressão → portão` — sobre o sinal da auditoria:

| O quê | Antes | Agora |
|---|---|---|
| quadros abertos em 200 de ventilador puro | **200/200** | **0/200** |
| limiar valendo | −60,0 dBFS, fixo | **−50,5 dBFS**, medido |
| quadros de fala baixa transmitidos | — | todos, com os retidos na frente |
| quadros abertos nos 100 de ventilador depois da fala | — | **0/100** |

O arquivo existe porque **os dois módulos passavam em separado**: a supressão
entregava a atenuação que prometia e o portão abria onde foi mandado. O defeito
estava na junta, e um teste de unidade tem um módulo só.

O aquecimento custa até 30 quadros — 600 ms — de ventilador na primeira abertura
de cada sessão: dez quadros até haver medida e os quinze da sustentação que o
portão já tinha aberto. A sustentação não é negociável nem aqui, porque é ela que
protege o fim de uma frase.

### E a tela conta o corte que está valendo

São dois números — o alvo na régua e o corte que decide —, e o medidor desenha a
marca no **segundo**. Uma marca no alvo enquanto o corte está 10 dB acima engana
exatamente quem abriu o teste de microfone para descobrir por que ele não abre: a
barra passaria de uma marca que não corta nada. A frase do medidor diz o número
por extenso quando ele subiu, com a razão — «pelo ruído da sala». Preso pela
bancada `qualidade-da-voz.cjs`.

## Ou um quadro inteiro, ou nada

`Supressao::processar` entregava «o que couber», e no primeiro quadro couberam
**480 amostras** — metade. A sobreposição de 50% retém um salto, e a auditoria
mediu o resto: o codificador Opus exige exatamente 960 e recusa o resto com
`WrongFrameSize`, então o primeiro trecho de fala de cada sessão era **descartado
com um erro que ninguém lia**. «O produto sabe e não conta», na forma em que ele
joga áudio fora.

O anel de entrada passou a nascer com um salto de silêncio adiante do sinal —
`entrada_semeada`. Com ele o primeiro quadro que entra já completa dois blocos e
sai inteiro; o atraso é o mesmo, e aparece como 10 ms de silêncio na frente do
fluxo em vez de como um pedaço que não cabe em lugar nenhum. A conta de amostras
passou a fechar **exata**, e `nao_acumula_nem_perde_amostras` mede isso — antes
faltava um salto para sempre.

A entrega também deixou de ser parcial por contrato: ou sai o tamanho que entrou,
ou não sai nada. Nenhum chamador de hoje alimenta em pedaços tortos; o contrato
vale para o próximo, e o teste cobre os dois casos.

## Cada quadro com o instante dele

Os quadros que a retenção da primeira sílaba entrega **aconteceram antes** do
quadro que abriu o portão, e saíam todos com o mesmo carimbo de tempo. Três
quadros no mesmo instante é o que o `playout` de quem recebe lê como salto de
relógio: ele ressincroniza, e a ressincronização cai no primeiro quadro de cada
fala — que é exatamente o começo de palavra que F01 foi recuperar.

`voice::carimbo_do_quadro` recua um quadro de amostras por posição. Não é
aproximação: o quadro retido *é* de 20 ms antes.

## A primeira sílaba

F01 pede «preservar início e fim das palavras». O fim já estava resolvido pelos
300 ms de sustentação; o **início** não tinha nada. A energia de uma consoante
surda no começo de uma palavra está abaixo do limiar: quando o nível sobe o
bastante para abrir, o ataque já passou.

`VoiceGate::quadros_a_transmitir` guarda os dois quadros anteriores e os entrega
**junto** com a abertura. 40 ms, que é o tempo de um ataque, e só na ativação por
voz — na tecla e no modo aberto não há instante em que a voz existe e o portão
está fechado.

## Ouvir antes de entrar

`crates/seele-core/src/teste_de_microfone.rs`. Ele exercita o **mesmo caminho** de
uma conversa menos o codec e a rede, e isso é a decisão: um teste do microfone cru
responderia a pergunta errada, porque a fala que o portão descarta não aparece
numa conversa — e é justamente ela que F01 existe para recuperar.

Não é uma sessão de voz. `Voice` pede um `MediaChannel` e um `Ssrc`, quer dizer um
servidor, e a pergunta que este módulo responde é anterior a haver servidor.

**Ouvir a si mesmo começa desligado.** Tocar o microfone no alto-falante
realimenta; o nível e a marca de abertura respondem a maior parte da pergunta sem
som nenhum, e a tela diz isso ao lado do controle.

## O que isto custa

- **10 ms de latência** com o filtro ligado, e 40 ms no primeiro quadro de cada
  fala com a retenção. Os dois estão medidos acima e nenhum foi medido em CPU.
- **Uma voz com textura** se o filtro for agressivo demais para um microfone
  específico. É por isso que a força é um número de 0 a 1 e não um interruptor:
  quem achar a voz «de rádio» abaixa em vez de abrir mão do resto.
- **Digitação continua passando**, atenuada. O rastreador de mínimos não aprende
  transiente. Está escrito no cabeçalho do módulo e na tela.
- **Um som contínuo por mais de um segundo vira piso.** É o preço do mínimo de
  janela deslizante, e ele vale para os dois rastreadores — o da supressão, por
  raia, e o do portão, por quadro. Um tom sustentado sem vale nenhum é
  indistinguível de um ventilador para qualquer estimador de mínimos, e um dos
  testes desta entrega precisou ser reescrito com sílabas por causa disso: com tom
  contínuo ele exigia do código uma distinção que não existe. Fala tem vale; nota
  longa de instrumento, não.
- **Até 600 ms de ventilador na primeira abertura de cada sessão**, enquanto não
  há medida. Está medido no teste do par e tem teto asserido.
- **Uma constante de tela saiu de uma lista de proibidos.** `RUÍDO` estava em
  `the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead`
  porque o produto não sabia fazer aquilo. Agora sabe, e a asserção inverteu de
  lado em vez de ser apagada.

## O que continua fora

**Cancelamento de eco acústico.** `features-v15.md` é explícito: ele exige
avaliação própria e não está implicitamente resolvido pela supressão de ruído. O
que este ADR entrega trata o **microfone**.

O retorno dos participantes pelo áudio do compartilhamento de tela é outro
problema e está no ADR 0054: ele pede separar o áudio capturado do áudio da
conversa, e a supressão de ruído não o alcança — aquele retorno não passa pelo
microfone, ele é capturado direto da saída.
