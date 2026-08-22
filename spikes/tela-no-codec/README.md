# spike `tela-no-codec`

**Descartável.** Fora do workspace, como `device-latency`, `plug-cli`,
`voice-link` e `tela-no-transporte`. Existe para responder **uma** pergunta
antes de `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`
fixar a lista de resoluções e de quadros que a interface oferece. Nada pode
depender dele.

## A pergunta

> Quantos quadros por segundo o OpenH264 por software entrega, e a que custo de
> CPU, em 1080p e em 720p, com conteúdo de tela de verdade?

A spec escreve no §2, sobre o custo de CPU: *«Não medido aqui, e este documento
não vai fingir que foi.»* O §5 fecha dizendo que a lista de opções da interface
fica em aberto até que alguém meça, porque *«oferecer 1080p antes desse número é
oferecer uma opção que pode não ter CPU para acontecer»*. Este é o número.

O irmão dele, `tela-no-transporte`, mediu transporte e não encostou em codec.
Aqui é o contrário: **não há rede nenhuma**, e o único relógio que importa é o
que separa a entrada e a saída de `encode()`.

## O que está em prova, e o que não está

Duas coisas a spec já decidiu, e este spike **não** as reabre: o codec é H.264
baseline pelo `shiguredo_openh264`, que faz `dlopen` do módulo binário do Cisco;
e a v1 não promete 1080p30 — a resolução é escolhida e segura, e quem cede é o
quadro, entre 5 e 30 por segundo.

O que ele mede é o que decide a lista:

- **quadros por segundo sustentados** em 1080p, 720p, 540p e 360p;
- **CPU**: quanto de um núcleo, e quantos núcleos;
- **o que acontece quando a máquina não dá conta** — atrasa, acumula ou
  descarta?

## O módulo do Cisco não estava na máquina, e isso é metade do achado

Não há `libopenh264` em lugar nenhum deste sistema, e não há como haver: **o
binário deste produto não vem com codec**, que é a decisão do §2 e a licença que
a impõe. Foi preciso buscá-lo em `ciscobinary.openh264.org` — o mesmo endereço
de onde o Firefox busca o dele desde 2013 — para que existisse o que medir:

```text
https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-arm64.dylib.bz2
  482.124 bytes  sha256 6db362ee5abdab572311aeadb96d3f44b0617d9a4a4b9f4db4cb5ac4d968da71
descompactado:
  1.207.136 bytes  sha256 052e98bfcf7a9167d22f3bbb3f5988ef79065591f36af8b52924b22b13624551
  Mach-O 64-bit dynamically linked shared library arm64
```

O `dlopen` funcionou de primeira, sem assinatura, sem quarentena e sem
`Entitlements` — o `runtime_version()` responde `v2.6.0` e bate com a versão dos
cabeçalhos de onde as bindings foram geradas.

**A busca é de 471 KiB comprimidos**, que viram 1,15 MiB em disco: o «~1 MB» do
§2 está certo para o arquivo que fica e generoso para o que trafega. É o
primeiro número que a tela de consentimento tem de dizer.

Nada disso entra no repositório: o `.dylib` fica em `target/`, que já é
ignorado.

### Dois custos de build que a spec não previu

O §2 lista como primeira razão da escolha *«zero crates novos e zero ferramentas
de build»*. A árvore de execução realmente é uma linha só, mas a de build não é
grátis:

1. **O `build.rs` faz `git clone --depth 1 --branch v2.6.0` de
   `github.com/cisco/openh264`** para pegar `codec_api.h`. Ou seja, o
   `cargo build` precisa de **`git` e de rede** — não compila C nenhum, mas também não compila
   sem internet. Numa máquina de CI sem saída, ou num build reprodutível
   offline, isso falha, e falha no build.rs, que é o pior lugar para descobrir.
2. **`bindgen` precisa de `libclang`.** Aqui ele achou o das Command Line Tools
   sem ajuda; numa máquina sem elas, não acha.

Nenhum dos dois derruba a decisão do §2 — continuam sendo muito menos que nasm
mais CMake —, mas «zero ferramentas de build» está otimista por dois itens, e
quem for empacotar precisa saber antes.

## O conteúdo, e por que ele é o que é

Um codec não tem um custo: tem um custo **por conteúdo**.

- **Ruído aleatório é o pior caso de todo encoder** — nenhuma predição acerta,
  tudo vira resíduo — e não é o caso de ninguém. Medir sobre ele daria um número
  pessimista e inútil.
- **Vídeo natural** é quase o oposto: gradientes suaves, movimento contínuo,
  bordas raras.
- **Uma tela de trabalho não é nem um nem outro.** Fica quase parada, tem
  regiões de texto de contraste altíssimo — a borda mais cara que existe para
  uma DCT — e muda em rajada: uma rolagem, uma janela que ganha foco.

Então a textura aqui é **capturada da tela de quem roda**, com
`screencapture(1)`, e o movimento é sintetizado em cima dela a 30 quadros por
segundo. Nas corridas abaixo, a captura foi de um terminal com o próprio
trabalho deste ramo na tela: texto denso, fonte de largura fixa, fundo escuro. É
literalmente o público que `specs/00-visao-geral.md` descreve.

O roteiro dá a volta em 90 quadros — 3 s — e é **deliberadamente
desequilibrado**, porque uma tela de trabalho fica parada a maior parte do tempo:

```text
 0..30   parada, com o cursor piscando a 2 Hz
30..60   rolagem de texto, 3 px por quadro
60       uma janela ganha foco: um retângulo inteiro troca de conteúdo
60..75   parada de novo, com a janela por cima
75..90   rolagem de volta
```

A janela em foco é composta com pixels de outra altura da **mesma** captura, de
modo que os dois lados da borda são tela de verdade. `--amostra <dir>` escreve
alguns quadros como BMP para conferir com o olho que o conteúdo é o que este
texto diz que é — a alternativa seria acreditar, e uma sequência com um defeito
de recorte encodaria depressa e devolveria um número bonito e falso.

**Nenhuma imagem é escrita no repositório.** A captura vai para um arquivo
temporário do sistema e é apagada assim que é lida.

## A máquina, e o que ela não representa

**Uma máquina só, e é a mais forte que este projeto vai encontrar:**

```text
Apple M5 Pro (Mac17,9) · 5 núcleos de desempenho + 10 de eficiência · 24 GB
macOS 26.5.2 (25F84) · rustc 1.97.1 · OpenH264 v2.6.0 do Cisco
```

**Não extrapole daqui para «máquina fraca».** O que está medido é Apple Silicon
com NEON, e o OpenH264 tem assembly de ARM otimizado. As máquinas que faltam
estão listadas no fim.

O que ajuda um pouco a atravessar máquinas é a coluna **`nucleo/30`**: quanto de
**um** núcleo custaria sustentar 30 quadros por segundo. `fps` e `nucleos`
descrevem esta máquina; `nucleo/30` é uma razão, e numa máquina três vezes mais
lenta por núcleo ela triplica.

E há uma segunda medida que não é extrapolação: a mesma matriz rodada com
`taskpolicy -b`, ou seja em **prioridade de fundo**, que no Apple Silicon
significa os núcleos de eficiência. É exatamente a configuração que o §2 manda
usar (*«prioridade abaixo do normal»*) e, de quebra, é o núcleo mais lento que
esta máquina tem.

## Como rodar

```text
curl -L -o /tmp/m.bz2 https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-arm64.dylib.bz2
bunzip2 /tmp/m.bz2 && mv /tmp/m target/libopenh264.dylib

cargo run --release                              # a matriz inteira, duas tabelas
cargo run --release -- --segundos 20             # amostra maior
cargo run --release -- --modo "720p, 1 fatia"    # um cenário (casa com o nome)
cargo run --release -- --chave-s 0               # sem quadro-chave periódico
cargo run --release -- --amostra /tmp/quadros    # os quadros, para conferir
taskpolicy -b cargo run --release -- --chave-s 0 # em prioridade de fundo
```

Sem o módulo, o binário não inventa caminho nenhum: diz que o produto não vem
com codec e imprime o `curl`.

## A resposta

Teto de 1200 kbps em todas as linhas — não é um número escolhido aqui, é o do
`tela-no-transporte`: os 60% de um caminho de 2000 kbps que o §3.2 mediu como o
único ponto em que a voz volta para a linha de base. `pulados` é a fração de
quadros que o **próprio controle de taxa do OpenH264** jogou fora para não
estourar esse teto. 8 s por cenário; aquecimento de 30 quadros descartado.

### Em prioridade normal, núcleo de desempenho

```text
quadro-chave só no início, que é o desenho do §3.3:
cenario               fps  nucleos nucleo/30  p50 ms  p95 ms  pior ms    kbps  pulados
1080p, 1 fatia        498     1.00     0.060    2.11    3.45     6.05    1146    16.2%
1080p, 4 fatias      1192     2.54     0.064    0.84    1.87     4.07    1151    23.9%
720p, 1 fatia        1092     1.00     0.027    0.61    1.77     4.11     872    11.1%
720p, 4 fatias       2428     2.42     0.030    0.34    0.74     4.84     920    15.9%
540p, 1 fatia        1567     1.00     0.019    0.34    1.32     3.16     796    12.4%
360p, 1 fatia        3479     1.00     0.009    0.29    0.58     2.16     416     2.2%

quadro-chave forçado a cada 2 s (o pessimista):
cenario               fps  nucleos nucleo/30  p50 ms  p95 ms  pior ms  idr ms  idr KiB    kbps  pulados
1080p, 1 fatia        392     1.00     0.076    2.77    3.84     5.59    8.37     65.4    1191    17.6%
1080p, 4 fatias      1029     2.69     0.078    1.04    1.96     4.96    2.86     65.4    1178    27.0%
720p, 1 fatia         882     1.00     0.034    1.07    1.98     4.39    4.46     39.7     992    13.3%
720p, 4 fatias       2162     2.52     0.035    0.40    1.01     1.93    1.60     39.8    1012    19.1%
540p, 1 fatia        1324     1.00     0.023    0.71    1.37     3.18    2.78     25.9     921    11.7%
360p, 1 fatia        2893     1.00     0.010    0.31    0.72     2.48    1.45     19.3     597     2.2%
```

Uma segunda corrida concordou linha por linha: 489 / 1188 / 1060 / 2297 / 1543 /
3423 fps na primeira tabela, e 386 / 994 / 864 / 2072 / 1273 / 2836 na segunda.
Nenhuma conclusão daqui depende da terceira casa.

### Em prioridade de fundo, núcleo de eficiência

Duas corridas, e elas **não** concordam entre si — prioridade de fundo divide o
núcleo com tudo o mais que o sistema resolveu pôr ali, e essa instabilidade é
parte da resposta, não ruído a esconder:

```text
cenario               fps  nucleos nucleo/30  p50 ms  p95 ms  pior ms    kbps  pulados
1080p, 1 fatia        119     0.84     0.211    7.53   14.71   614.90    1097    15.1%
1080p, 1 fatia        169     0.99     0.175    5.66   10.19    15.46    1022    11.2%
720p, 1 fatia         324     0.98     0.091    2.57    6.31    10.38     773     5.2%
720p, 1 fatia         311     0.98     0.095    2.73    5.79    17.31     673     3.2%
540p, 1 fatia         508     0.98     0.058    1.61    4.20    16.00     652     5.2%
360p, 1 fatia        1120     0.97     0.026    0.75    2.03     9.81     435     0.4%
```

### 1 · CPU não é o gargalo desta máquina, e não é nem perto

**1080p a 30 quadros custa 0,060 de um núcleo de desempenho** — o encoder
entrega 498 quadros por segundo quando ninguém o segura, dezesseis vezes o que a
cadência de captura pede. Em 720p são 0,027 de núcleo e 1092 quadros por
segundo.

Mesmo empurrado para o núcleo mais lento da máquina, com prioridade de fundo,
1080p30 custa **0,18 a 0,21 de um núcleo** — ainda com quatro a seis vezes de
folga. Não existe, nesta máquina, resolução de produto em que o encoder por
software não dê conta de 30 quadros.

Isso muda o tom do §2. A frase *«encode por software de 1080p a 30 quadros é
trabalho de núcleo inteiro numa máquina de escritório»* **não vale aqui**: é 6%
de um núcleo, não 100%. O que sobra em pé do §2 é a cautela sobre máquinas que
não foram medidas, e é só isso que sobra.

### 2 · O que aperta é o orçamento de bits, e ele aperta antes da CPU

A coluna que decide a lista de opções não é a de CPU: é `pulados`. Com o teto de
1200 kbps que a voz permite, **o próprio OpenH264 joga fora 16% dos quadros em
1080p** para não estourar — o encoder chega a avisar sozinho no console
(`Warning:[Rc] iContinualSkipFrames(3) is large`). Em 720p são 11%, em 540p 12%,
em 360p 2%.

Ou seja: quem escolhe 1080p num caminho doméstico não recebe 1080p30. Recebe
1080p a uns 25 quadros por segundo, com o encoder decidindo sozinho quais cinco
cair. A escolha é legítima e a máquina aguenta; o que ela não faz é entregar o
que o rótulo diz — que é precisamente por que o §5 obriga a interface a mostrar
o que está saindo ao lado do que foi pedido.

E há um limite por baixo. Descendo a resolução, o encoder passa a **não
conseguir gastar o orçamento**: 360p rende 416 kbps de 1200 disponíveis. Abaixo
de um certo tamanho não há mais detalhe para comprar com aqueles bits, e a
escolha deixa de trocar nitidez por qualquer coisa — só torra nitidez.

### 3 · Uma fatia, uma thread

Quatro fatias com quatro threads dão 2,4× de quadros por 2,5× de CPU: **nenhuma
eficiência**, só latência menor por quadro (0,84 ms contra 2,11 ms em 1080p). E
custam qualidade — os quadros pulados sobem de 16,2% para 23,9%, porque a
predição não atravessa fatia e cada fatia recomeça o contexto.

Numa máquina que entrega dezesseis vezes o necessário, latência menor por quadro
não compra nada, e a qualidade perdida é paga por quem assiste. **Uma fatia, uma
thread**, que é também a única forma compatível com o §2 (*«o encoder mora numa
thread própria»*): três threads a mais dentro do encoder seriam três threads que
o §2 não desenhou.

### 4 · O quadro-chave custa 65 KiB em 1080p, e o §3.3 tinha razão

Um IDR de 1080p sai com **65,4 KiB** e leva **8,4 ms** para sair — quatro vezes
o custo de um quadro comum. Em 720p são 39,7 KiB e 4,5 ms.

65 KiB são **446 ms do orçamento inteiro** de 1200 kbps. Despejá-los num tique
de 33 ms é pedir 16 Mbps por um instante, que é exatamente a rajada que o
`tela-no-transporte` mediu enchendo a fila do gargalo e empurrando a voz de
22 ms para 115 ms. **O «espalhe o quadro-chave» do §3.3 está confirmado com um
número**, e o «sob demanda em vez de periódico» também: forçar um a cada 2 s
tira 21% dos quadros por segundo (498 → 392) e sobe os pulados de 16,2% para
17,6%, tudo para um receptor que não precisava.

### 5 · Quando não dá conta, o encoder não faz nada — quem faz é o chamador

`encode()` é síncrono: entra um quadro, sai um quadro, e ele volta quando
terminou. **O encoder não enfileira e não descarta.** Ele não tem fila, não tem
thread interna de entrada e não sabe que existe um relógio. A única coisa que
ele faz por conta própria é o que o item 2 mostrou — pular quadro por **falta de
bits**, nunca por falta de tempo.

Então quem decide é quem captura, e as duas políticas possíveis foram postas
lado a lado, alimentando o encoder a uma cadência 1,6× acima do que ele
sustenta:

```text
politica      entregues  descartados  fila final    idade p50   idade pior     cresce
enfileira          3856            0        1165       958 ms      1870 ms      23.4%
descarta           2999         2023           0         3 ms         9 ms       0.1%
```

`idade` é quanto tempo se passou entre o quadro ter sido capturado e ter saído
do encoder. Com fila, ela **cresce linearmente e sem limite** — 23% de todo o
tempo decorrido vira atraso, e nada nesse caminho para de crescer: em oito
segundos são 1,9 s de atraso, em oitenta seriam dezenove. A fila final tem 1165
quadros, que em 1080p são 3,6 GB de I420 se alguém guardar os pixels.

Com descarte, a idade fica em **3 ms** e não anda. Entregam-se menos quadros — é
o que déficit de CPU significa — mas cada um que sai é recente.

A mesma corrida na prioridade de fundo dá a mesma forma com números maiores:
1,58 s de idade mediana e 3,0 s no pior caso enfileirando, contra 9 ms
descartando.

**A regra que o §1 já tinha escrito para a captura está confirmada, e ela vale
igual para a fila entre a captura e o encoder:** o quadro novo substitui o
velho, nunca entra atrás dele.

E há uma cauda que a prioridade de fundo traz de brinde: numa das duas corridas,
**um único quadro de 1080p levou 615 ms**, com p95 de 14,7 ms na mesma corrida.
Prioridade abaixo do normal não tem piso de latência — o sistema pode
simplesmente não escalonar a thread por meio segundo. É o preço certo a pagar
(a voz nunca cede à tela), mas a tela tem de estar preparada para engasgar sem
que nada esteja errado.

## O que este spike fecha na spec

A lista de opções do §5, que o próprio §5 deixou em aberto apontando para cá.
Está escrita lá. Em uma frase: **1080p, 720p e 540p; 30, 15 e 8 quadros por
segundo** — e 1080p entra porque a CPU não é o gargalo, com o aviso de que a
1200 kbps ele já perde 16% dos quadros sozinho.

## O que este spike **não** responde

- **Nada sobre máquinas que não sejam Apple Silicon.** Faltam as três que
  importam: um x86-64 de mesa, um notebook fino que estrangula por calor (a
  máquina que o §2 nomeia e que ninguém mediu), e o Windows. O
  `nucleo/30` é a coluna feita para receber essas medidas ao lado destas.
- **Nada sobre captura de verdade em cadência.** A textura é real, o movimento é
  sintetizado. Uma ScreenCaptureKit entregando 30 quadros por segundo tem custo
  próprio — conversão de espaço de cor, cópia de `IOSurface` — que não está aqui
  e que soma ao do encoder.
- **Nada sobre qualidade percebida.** `pulados` diz quantos quadros caíram, não
  se o texto continua legível. Isso quer olho humano, e o §5 já manda a
  interface mostrar o que está saindo.
- **Nada sobre decodificação.** Quem recebe também gasta CPU, e não foi medido.
- **Nada sobre a lista de tetos de banda** do §5. Este spike mediu com um único
  teto, o de 1200 kbps que o `tela-no-transporte` sustenta. Fixar uma lista de
  bandas exige medir o caminho, que é a pergunta 2 do §8 e não é esta.
- **Nada sobre duas transmissões na mesma máquina.** A pergunta 3 do §8 continua
  aberta, e agora com um dado a favor dela: cada transmissão de 1080p30 custa
  0,06 de núcleo, então a CPU não é o que a impede.
