# O áudio da tela compartilhada

> Estado: desenho aprovado, implementação começando.
> Pedido: «a transmissão também não carrega o áudio, algo que deveria ter em
> transmissão de jogos».

## O problema

Compartilhar a tela mostra a imagem e mais nada. Quem transmite um jogo, um
vídeo ou uma chamada de outro programa está mostrando metade do que quer
mostrar, e a metade que falta é a que diz o que está acontecendo.

Hoje a voz da sala e a tela viajam por caminhos separados: a voz num fluxo de
mídia por sala, a tela num fluxo QUIC por espectador. O áudio da tela é uma
terceira coisa e não uma das duas — ele nasce da máquina de quem compartilha,
não do microfone, e precisa chegar junto da imagem a que pertence.

## O que já existe, e é quase tudo

Este desenho é barato porque quatro das cinco peças estão prontas:

- **Captura, no Windows.** O cpal liga o modo *loopback* do WASAPI sozinho
  quando se abre uma **saída** como entrada. Não há código de plataforma a
  escrever: é o mesmo `build_input_stream` da voz, apontado para outro
  dispositivo.
- **Captura, no macOS.** A captura de tela já usa ScreenCaptureKit, e o áudio do
  sistema é um parâmetro do mesmo `SCStream` — hoje escrito
  `with_captures_audio(false)`. Ele vem no mesmo retorno de amostras que a
  imagem, pela API que já está montada.
- **Codificação.** O Opus já está na árvore, e é o mesmo codec da voz.
- **Enquadramento.** O fluxo de tela leva **um byte de tipo** e quatro de
  tamanho na frente de cada quadro. Os tipos `0` e `1` são o quadro comum e o
  quadro-chave; `2` fica livre, e o servidor já recusa qualquer outro — então
  acrescentar um tipo é uma mudança que o enquadramento foi desenhado para
  receber.

O que falta é o fio entre elas, e a mistura do outro lado.

## O desenho

### Por dentro do fluxo da tela, e não ao lado dele

O áudio da tela vai **no mesmo fluxo QUIC** da imagem, como um terceiro tipo de
quadro. Três razões, em ordem de peso:

1. **Sincronia por construção.** Dois fluxos separados chegam em ordens
   diferentes e precisariam de carimbo e de fila de alinhamento. No mesmo fluxo,
   a ordem de chegada é a ordem de saída.
2. **Uma porta, um teto.** O orçamento de banda da tela (`FRACAO_DO_CAMINHO`) já
   mede o fluxo inteiro. Um segundo fluxo precisaria de um segundo teto e de uma
   segunda decisão sobre o que cede.
3. **Nada de novo no servidor.** Ele repassa o fluxo por pedaço, sem remontar
   quadro. Um tipo novo atravessa sem que ele precise entendê-lo — só a
   validação do byte de tipo muda, e a porta de entrada continua sendo o começo
   de um quadro-chave **de imagem**.

### O que cede quando aperta

O áudio **não cede**. Ele custa 32 kbps contra os 1200 kbps do vídeo — menos de
3% do orçamento — e é a metade da transmissão que continua útil quando a imagem
engasga. Um jogo a 8 quadros com som é acompanhável; a 30 quadros mudo, não.

Isso inverte a regra do vídeo de propósito, e é a mesma inversão que fez
`Movimento` virar o padrão: a `specs/07` foi escrita para texto.

### Silêncio não viaja

Quadros de áudio só saem quando há som. O caminho de quem compartilha uma janela
parada com um documento aberto não paga nada, e o Opus já distingue silêncio.

### Do outro lado

O áudio da tela entra na mesma mistura de saída que a voz da sala, num canal
próprio para que o volume por pessoa continue valendo e o **isolamento total**
continue significando o que diz: quem se isola não ouve nem a voz nem a tela.

## O que este desenho não faz

- **Não captura o áudio de uma janela só.** As duas plataformas entregam o áudio
  da máquina inteira; separar por processo é outro assunto e nenhuma das duas o
  oferece de graça.
- **Não pede permissão nova no Windows.** O loopback do WASAPI não precisa. No
  macOS a permissão de gravação de tela já cobre o áudio do mesmo `SCStream`.
- **Não dá controle de volume do áudio da tela separado da voz**, na primeira
  versão. Se a mistura ficar desequilibrada em campo, o controle nasce daí e não
  de uma suposição.

## Ordem de implementação

Cada passo tem prova própria, e nenhum depende do seguinte para ser verificado:

1. **Captura de loopback em `seele-audio`**, com o teste de campo que já existe
   para a captura de tela: abrir, tomar amostras, provar que não são silêncio.
2. **O tipo de quadro `2`** no enquadramento dos dois lados, com o teste de que
   o servidor o repassa e não o confunde com porta de entrada.
3. **Codificação e envio**, atrás da mesma decisão de teto.
4. **Recepção e mistura**, com o isolamento total valendo.
5. **A casca**: nada a escolher. O áudio vai junto porque é parte do que se
   está mostrando.
