# A imagem que cabe: cinco estágios até 1080p60 numa casa

> Estado: desenho aprovado. Estágio 1 começando.
>
> Pergunta que o originou: «se eu quero transmitir em 1080p60 para 5-6 pessoas,
> fica impossível então? O motivo do Discord permitir várias pessoas
> transmitindo ao mesmo tempo, com qualidade boa e sem pixelização, é por quê?»

## A conta de hoje

1080p60 de jogo, com qualidade decente, custa uns 8 Mbps. A topologia atual é:
quem compartilha manda **uma** cópia ao servidor, e o servidor abre **um fluxo
por espectador** (`tela::bombear`, uma tarefa por sessão).

| | subida |
| --- | --- |
| quem compartilha | 8 Mbps |
| **quem hospeda**, 6 espectadores | **48 Mbps**, mais a voz de todo mundo |
| dois transmitindo ao mesmo tempo | **96 Mbps** |

Numa fibra simétrica de 300/300 cabe. Num plano 500/50 — que é o da maioria —
não cabe, e o produto não diz isso: ele entrega 540p e deixa a pessoa achando
que o programa é ruim.

E a conta piora antes de começar. O teto sai de uma estimativa que **começa em
2 Mbps chutados** (`CAMINHO_DO_SERVER_BPS`), dos quais 60% vão para o vídeo
(`FRACAO_DO_CAMINHO`, medida: com o vídeo solto a voz vai de 23 ms para 225 ms),
divididos pela contagem de espectadores. Com 3 pessoas assistindo são 400 kbps
para cada uma — abaixo dos 900 kbps que a escada pede para 720p, e a prioridade
`Movimento` desce mais um degrau. **540p, numa rede local de gigabit.**

## O que o Discord faz de diferente, e o que não é mágica

A razão principal é topologia e não código: quem transmite manda uma cópia para
um datacenter, e o datacenter replica. O custo de quem transmite é o mesmo para
1 ou 50 espectadores, e a réplica sai de uma máquina com 10 ou 100 Gbps.

Auto-hospedado significa que a banda é de alguém. **Esse divisor não some sem
deixar de ser o que este produto é** — e não é o que este desenho tenta tirar.

O que ele tira é tudo o mais, que não é pouco: nós codificamos a tela com o
perfil de câmera, mandamos uma qualidade só para todo mundo, começamos de um
chute de 2 Mbps, e replicamos em estrela quando dava para replicar em árvore.

## Os cinco estágios

Em ordem de rendimento por esforço, com uma dependência que decide a posição do
terceiro.

### 1. A imagem que já cabe no cano

*Dias. Risco baixo.*

- **O ponto de partida deixa de ser um chute.** O caminho já é conhecido —
  `Caminho::RedeLocal`, `Ipv6Direto`, `FuroDeNat` — e uma rede local não é uma
  subida residencial de 2 Mbps.
- **Os limiares da escada são remedidos.** 900 kbps para 720p e 1,5 Mbps para
  1080p foram escolhidos com qual conteúdo? São otimistas para movimento, e é
  por isso que `Movimento` desce um degrau e cai no piso.

Alvo: numa rede local, de 540p para 1080p, sem tocar em arquitetura nenhuma.

### 2. Mais imagem pelos mesmos bits

*Semanas. Risco baixo. Medida antes da decisão.*

A binding do OpenH264 fixa `iUsageType = CAMERA_VIDEO_REAL_TIME`. Existe
`SCREEN_CONTENT_REAL_TIME`, feito para texto, bordas duras e regiões paradas —
que é exatamente o que uma tela é.

**Um spike decide, e não a literatura**: mesma cena, mesmo bitrate, os dois
modos, como `spikes/tela-no-codec` já fez com a resolução. Se render os 30% que
se promete, são 15 Mbps de graça no cenário de 48.

E ele informa o estágio seguinte: se o OpenH264 no modo certo já ficar bom, o
hardware ganha tempo.

### 3. Codificação por hardware

*Semanas. Superfície grande, costura pequena.*

**Subiu do quinto para o terceiro lugar por dois argumentos que a primeira
versão deste plano errou.**

O primeiro: eu disse que CPU não era a parede, citando a medição de que a
máquina entrega dezesseis vezes o necessário a 720p30. Aquela medição foi numa
máquina **ociosa**, e o caso de uso é compartilhar um **jogo** — com a máquina no
limite fazendo a coisa que importa mais. Uma thread de codificação tirada de um
jogo custa quadros no jogo. O codificador de hardware é um circuito à parte que
não disputa com ele, e é por isso que todo produto de transmissão de jogo usa
um.

O segundo: o estágio 4 são camadas temporais, e camadas são configuradas **no
codificador**. Construí-las sobre o OpenH264 e depois trocar é fazer o trabalho
duas vezes.

A costura já existe: `Codificador::codificar(&QuadroI420, chave) ->
Option<QuadroCodificado>` — quadro cru entra, Annex-B sai. O hardware é um
segundo implementador atrás dela, com recuo para o software quando não houver.

VideoToolbox no macOS; Media Foundation no Windows, que embrulha NVENC,
QuickSync e AMF atrás de uma API só.

### 4. Cada um recebe o que aguenta

*Semanas. Mudança de protocolo.*

Hoje o teto é **um número para a sala inteira** (`TetoDeVideo`), dividido pela
contagem. Uma pessoa no wifi de hotel arrasta todo mundo para 540p, porque
existe uma qualidade só.

Camadas temporais consertam isso sem recodificar: quem repassa descarta a camada
de cima para o espectador lento — 60 quadros para quem aguenta, 30 para quem
não. Precisa de um identificador de camada no cabeçalho de quadro, e de teto por
espectador em vez de um número dividido.

### 5. A réplica deixa de sair toda da mesma casa

*O trabalho grande. **ADR antes do código.***

Quem hospeda manda 2 cópias; cada uma repassa 2. A saída de cada nó fica
limitada independentemente do tamanho da sala: 48 Mbps viram ~11.

Três decisões que o ADR tem de tomar antes de existir código:

- **Confiança.** O ADR 0003 é TOFU por servidor. Um espectador repassando para
  outro é superfície nova, e o modelo de confiança do produto muda.
- **Metadados.** Quem repassa vê volume e ritmo do que passa, mesmo cifrado.
- **Conserto da árvore.** Quando um nó do meio sai, a subárvore reencaixa — e
  isso não pode virar uma tempestade de reconexões.

## O alvo

| | hoje | depois dos cinco |
| --- | --- | --- |
| 1080p60 para 6 | 48 Mbps, e na prática 540p | ~11 Mbps em 1080p |
| um espectador lento | derruba todo mundo | recebe menos, sozinho |
| dois transmitindo | 96 Mbps | ~22 Mbps |
| quem compartilha um jogo | uma thread disputando com o jogo | um circuito à parte |

Isto **não** alcança o Discord em números absolutos, e nenhum estágio aqui
promete isso: a réplica continua saindo da casa de alguém. O que ele alcança é
**caber numa fibra doméstica comum**, que é a diferença entre o recurso existir e
não existir.

## O que fica de fora, e por quê

- **Envio direto de quem compartilha aos espectadores.** Redistribui a dor em vez
  de reduzi-la — quem transmite passa a pagar pelo que transmite, o que é justo e
  não é menos. Só ganha sentido se a cascata mostrar que a árvore por si não
  resolve dois transmissores ao mesmo tempo.
- **Simulcast espacial** (codificar 1080p e 540p em paralelo). Custa dois
  codificadores no computador de quem já está jogando. As camadas temporais dão
  a maior parte do benefício por uma fração do custo, e a decisão de ir além
  delas se toma depois de elas existirem.
