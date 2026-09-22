# 0054 — Uma transmissão tem identidade

Status: **aceito**
Data: 2026-09-21
Sobre o commit `5340290`, pela revisão da v15 (`docs/review-v15-2026-09-21.md`, R16–R21).

> **Tudo o que é de compartilhamento passa a ser por `ScreenId`.** A sala cabe
> mais de uma transmissão desde que o servidor passou a guardá-las num mapa; o
> que faltou foi o resto do produto aprender isso.

## O defeito de origem, e ele é um só

O servidor já guardava várias transmissões por sala. O cliente continuou
escrevendo como se houvesse uma:

- `tela_de` pegava **a primeira** transmissão do mapa da sala, sem relacioná-la
  com a que o espectador escolheu nem com a própria. Autoria, controles e
  metadados podiam falar de uma tela enquanto a imagem era de outra;
- a fila de som da tela não sabia de quem ela era, então **qualquer**
  `TelaFechou` a limpava: alguém parando de transmitir calava o som de outra
  transmissão;
- a escolha de quem assiste vivia numa variável de JavaScript, e a janela não
  sobrevive a uma recarga;
- a caixa de compartilhar desabilitava o botão com «cabe uma por vez», que o
  servidor nunca disse.

Cinco sintomas, uma causa: **nada carregava a identidade da transmissão até
onde a decisão era tomada.**

## A decisão

### Três coisas separadas, e elas são três

O review nomeia o corte e ele é adotado por inteiro:

| O quê | Onde vive | Por quê |
|---|---|---|
| `minha_transmissao` | `Snapshot`, `Option<TelaEmCurso>` | é a única que carrega o teto pedido — o teto é escolha de quem transmite e não viaja no fio |
| `transmissoes` | `Snapshot`, `Vec<TransmissaoNaSala>` | o que há para escolher, cada uma com `assistida` e a contagem do servidor |
| `tela` | `Snapshot`, `Option<TelaEmCurso>` | a **assistida**, e só ela |

Transmitir e assistir a tela de outra pessoa ao mesmo tempo é o caso normal, não
o raro. Um campo só não tinha como responder às duas perguntas, e respondia à
errada.

### A escolha mora no Rust

`EscolhaDeTela` tem três estados, e são três de propósito:
`NinguemEscolheu` (o servidor decide, e ele liga todo mundo na primeira),
`Nenhuma` (recusou, e a recusa vale contra o religar) e `Esta(ScreenId)`.

Ela é gravada **antes** de o pedido sair, e é dela que saem `Snapshot::tela` e
`TransmissaoNaSala::assistida`. Uma transmissão que sai do ar devolve a escolha a
`NinguemEscolheu` e **não** a `Nenhuma`: herdar a recusa faria a próxima a
começar ser recusada por uma decisão que ninguém tomou sobre ela.

### A intenção tem geração

Na janela, `geracaoDaIntencao` anda a cada gesto, e toda operação assíncrona a
confere depois de cada `await`. É o conserto do R16: `pararDeVer` guardava a
recusa e `trocarDeTransmissao` pedia a nova **depois** de esperar o cancelamento
da anterior, sem revalidar — então um clique em «não ver» durante aquela espera
deixava sair um `assistir(B, true)` depois do clique.

### Parar de assistir sai do cinema

R19. `pararDeVer` fechava a imagem e não saía da tela cheia, e `botaoDeCinema` só
sai quando não há transmissão no palco — mas o desenho continuava passando
`true`, porque a outra pessoa continuava transmitindo. Sobrava navegação
escondida por CSS com nada na frente dela. A saída acontece **antes** do
primeiro `await`, para que nem durante a espera a navegação fique escondida.

### O som da transmissão tem volume e mudo próprios

R17. `ScreenId` segue até a mistura; a fila sabe de quem ela é e só é limpa pela
tela dela; e o `SSRC_DA_TELA`, que já existia, ganhou o ganho próprio que o
comentário ao lado dele previa. Volume e mudo são separados pela mesma razão que
o mudo do microfone é separado do ganho: voltar tem de devolver o volume
escolhido.

Isolamento total continua calando a mistura inteira, porque é isso que ele diz
que faz.

### Múltiplos transmissores são liberados na interface

R18. «Cabe uma por vez» era da janela e não do servidor — ele guarda várias por
sala e a admissão depende de capacidade de banda. O que ele de fato recusa é a
segunda abertura da **mesma** pessoa, e é essa recusa que continua de pé, como
`TROCAR`. A recusa de capacidade continua sendo dele, com a explicação dela.

### O painel de métricas sai

R20. `altura`, `quadros`, `kbps` e `medida` eram quatro campos que nada media: os
três primeiros saíam zerados e o quarto saía `false`, sempre. A tela reservava
três caixas e escrevia «ainda não há medida desta transmissão» nas três, para
sempre, numa área rolável sobre o vídeo.

Eles saem da ponte. No lugar fica uma barra compacta com o que se sabe e o que se
controla: quem transmite, quantas pessoas recebem, o som desta transmissão e
parar de assistir. Quando houver medida de verdade ela volta — ligada a um
`ScreenId`, dizendo o que mediu, e distinguindo envio de recepção.

**A contagem também trocou de fonte.** Era o roster desta máquina menos quem
compartilha, e incluía quem escolheu não assistir; passa a ser o `ScreenViewers`
do servidor, que conta assinaturas e é o mesmo N pelo qual ele divide o teto.

### O áudio capturado

R21. Três controles, e eles são três:

1. **«Ouvir transmissão»**, de quem assiste — o volume e o mudo acima;
2. **«Incluir áudio»**, de quem envia — `LimitesDeTela::com_som`. Desmarcado, o
   codificador de som nem nasce e a captura de som nem é lida;
3. **exclusão do áudio do SEELE na captura**, padrão onde o sistema a suporta.

No macOS é `excludesCurrentProcessAudio`, do macOS 13, e o código a pede sempre.
No Windows o caminho é o loopback da saída inteira e **não há exclusão**: quem a
faria é a API de loopback por processo da WASAPI, que o `cpal` não expõe.

Por isso a terceira não é uma promessa: é um relatório.
`seele_video::exclusao_medida_no_sistema` responde uma de três coisas, e a caixa
de compartilhar escreve qual é antes de alguém apertar. Onde não há exclusão, ela
diz isso e oferece as duas saídas que existem — compartilhar uma janela em vez do
monitor, ou transmitir sem áudio.

**O que este ADR não afirma:** que o eco está resolvido. A validação é acústica e
entre duas máquinas, e continua pendente. O que está feito é o caminho: a
propriedade é pedida onde existe, a ausência dela é relatada onde não existe, e a
pessoa tem duas saídas em vez de um sintoma.

## O que isto custa

- **Um `ScreenId` a mais em cada assinatura da ponte**, e uma leitura de
  `tela_escolhida` por `snapshot`. Um cadeado sem contenção, quatro vezes por
  segundo.
- **A comparação do §5 fica sem a metade esquerda até haver medida.** Ela já
  estava sem — a diferença é que agora a tela não reserva espaço para ela.
- **Quem estava acostumado com «cabe uma por vez»** vai encontrar a recusa de
  capacidade em vez de um botão apagado. É mais lento de descobrir e é verdadeiro.

## Provas

- `seele-core`: a fila de som só é limpa pela tela dela, e a troca de tela limpa
  a fila da anterior.
- `seele-ffi`: a escolha sobrevive à janela, e uma transmissão que sai do ar a
  devolve a `NinguemEscolheu` em vez de `Nenhuma`.
- `seele-video`: a resposta de exclusão é uma das três, e nunca mais otimista do
  que o código.
- `frontend`: a janela não afirma «cabe uma por vez», parar de assistir sai do
  cinema, e cada operação assíncrona revalida a geração.

**O que não está provado aqui e continua pendente:** a sessão nativa entre duas
máquinas — dois transmissores, um terceiro alternando, com vídeo tocando e
alguém falando. O review pede Windows↔Windows e Windows↔macOS, e nenhuma das
duas aconteceu nesta rodada.
