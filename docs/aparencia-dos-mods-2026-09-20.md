# A aparência dos três MODs oficiais — o que mudou, e o que dá para ver

Data: 20/09/2026. Resposta à rejeição visual registrada em
[validacao-nativa-api4-c4fe3ea.md](validacao-nativa-api4-c4fe3ea.md),
seção «Critério visual reforçado pelo usuário».

> **Isto não declara paridade visual.** As comparações abaixo são do
> laboratório — renderer do produto num navegador, transporte e identidades
> simulados. Composição, espaçamento, contraste e quebra dá para ver ali;
> foco, teclado e custo, não. O roteiro nativo está no fim.

---

## 1. A causa que estava embaixo de todas as outras

**As classes de um MOD nunca desenharam nada neste produto.**

A CSP do `tauri.conf.json` é `style-src 'self'`, sem `'unsafe-inline'`.
`declararClasses` servia as regras num `<style>` criado por script: o elemento
entrava na árvore, recebia texto, e o navegador o descartava — `sheet` fica
`null` e o console escreve «Applying inline style violates the following
Content Security Policy directive».

Medido, e não deduzido: uma página com a mesma política, um `el.style
.setProperty` e um `<style>` criado por script. O primeiro aplica; o segundo
não.

O que isso custava era invisível de uma forma cara:

- as **consultas de contêiner** — que é como um editor empilha quando a janela
  aperta — nunca rodaram;
- os **estados** (`sobre`, `foco`, `ativo`) declarados por um MOD nunca
  pintaram;
- e o teste do laboratório conferia as classes **como texto**, então tudo
  passava.

Agora elas vão numa `CSSStyleSheet` construída, adotada pelo documento. Ela não
é estilo *inline*, não passa por `style-src`, e o conteúdo continua sendo
montado por `folhaDeClassesDeMod` de partes que o validador conferiu — a
fronteira nunca foi a CSP. O escopo também não muda: as regras começam por
`[data-escopo-de-mod=…]`.

Um guarda novo impede a volta: nenhum script do produto cria `<style>` enquanto
a CSP não disser `'unsafe-inline'`.

## 2. O contrato de layout

`direcao: 'linha'` escrevia só `flex-direction`, que não faz nada num elemento
que não é contêiner flex — e `caixa` não é. Os três pacotes declaravam campos e
prévia lado a lado, e os três empilhavam.

**`direcao` passou a escrever `display: flex` junto.** É o que a palavra quer
dizer, e vale mais do que documentar que ela só serve em `pilha`: uma API cuja
forma correta depende de saber qual primitiva já é flex é uma API que ensina
pelo erro.

Isso descobriu a segunda metade: com a linha funcionando, as duas colunas
passaram a **transbordar** em vez de dividir. `crescer: 1` dá `flex-grow`, e
sem um tamanho de partida cada coluna parte do próprio conteúdo. Entrou `base`
(`flex-basis`) no vocabulário, e o par que os três pacotes usam agora é
`crescer: 1, base: 0, larguraMinima: N` — divide enquanto couber, quebra
sozinho quando não couber, sem depender de consulta nenhuma.

## 3. A casca, e por que ela não é uma segunda linguagem visual

A versão de `ce976fd`/`0fba9a5` tinha CSS próprio: `<dialog>` de 920px,
cabeçalho grudado, `h1` de 28px em Saira Condensed, corpo com 24px de margem,
campos de largura total com 8px entre rótulo e caixa, ação principal sólida.

Nada disso exigia acesso ao documento. O que faltava era o **host** desenhar a
mesma coisa:

| O que era | O que é |
|---|---|
| Margem interna `16px 8px` — régua de espaço entre controles | `--mod-margem`, 20px, 16 quando a superfície aperta |
| Título no mono do corpo | Cartela: Saira Condensed, caixa alta, 26px no diálogo e 22 na página — a mesma de `.moderar-titulo` |
| Partes coladas no primeiro nível | O corpo é uma coluna com `--mod-respiro` |
| Rótulo a 2px do campo, 12px | 8px e 10px, como `.server-campo` e `.rotulo` |
| Campo com a largura padrão do navegador | Largura da coluna, 36px de altura |
| Ação principal com contorno laranja | Bloco sólido, como `.botao` do produto |
| Marca e interruptor empilhados sob o quadradinho | Uma linha: controle e texto lado a lado |
| Amostra de cor de 32×24, acima do hexadecimal | Um controle só: amostra de 48×36 e hexadecimal na mesma linha |

Quem escreve um MOD continua livre no corpo — `estilo` e `classes` existem para
isso. O que ele deixou de precisar é reconstruir a aparência do SEELE para
parecer do SEELE.

## 3b. A composição: o objeto em tamanho real

A volta anterior consertou a mecânica — as classes passaram a desenhar, a linha
passou a ser linha, a casca ganhou a régua do produto — e **a composição
continuou errada**. As três telas punham os controles como herói e o que se
está editando como uma caixinha num canto, com metade da coluna vazia ao lado.

O trabalho de cada uma é o objeto, e não o formulário:

| Tela | O que se edita | Onde ele estava |
|---|---|---|
| PERFIS | o cartão com que você aparece | caixa de 90px ao lado de um formulário de 700 |
| ESTILO | a aparência da conversa | caixa numa coluna estreita à direita |
| MESA | a mesa | abaixo do formulário de administração |

**Conceito:** o objeto abre a janela, com a largura inteira e no tamanho em que
vai existir; os controles vêm abaixo, servindo-o.

E **três níveis de tipo, só três**: a cartela do título (Saira 26/22, do
produto), o título de grupo (Saira 13), o rótulo de campo (mono 10 apagado).
Antes tudo era o terceiro nível — cada campo com um rótulo do mesmo peso — e a
tela lia como uma lista de coisas iguais. Isso é o contrário da hierarquia de
comando que `specs/07` chama de regra de ouro.

O que saiu, porque era repetição e não informação: o rótulo «PRÉVIA» sobre a
prévia (a linha «Assim você aparece» já diz); a amostra da conversa repetida
dentro das abas do ESTILO; o nome da campanha escrito no corpo da MESA além da
cartela. E o que entrou onde faltava direção: a mesa vazia deixou de avisar
«nenhuma cena» e passou a dizer o próximo passo a quem mestra.

## 4. Os três

### PERFIS

O cartão voltou a ser um cartão: faixa sempre presente — imagem quando há,
gradiente da cor escolhida quando não há —, retrato subindo sobre ela com um
anel da cor, nome em corpo maior, pronome e status como pastilhas na mesma
linha. O editor abre num diálogo de 820px com **campos e prévia lado a lado**,
e empilha quando a largura não dá.

Quem não tem perfil mantém a identidade nativa **e** ganha um caminho visível
até o editor: um botão com rótulo, e não um retângulo com `aria-label`.

### ESTILO

Voltou a ser um **diálogo de 980px**, como em `0fba9a5`. A escolha de página
tinha custado o que o reteste mediu: o editor dividia a coluna da conversa e as
amostras ficavam num corredor. Escolher tema é tarefa curta e focada.

Dentro: abas CORES/FORMA/CONJUNTOS, controles à esquerda e a amostra da
conversa à direita, prévia local separada da publicação por um interruptor que
diz o que cada estado significa, e rodapé fixo com RESTAURAR, DESCARTAR e
PUBLICAR. A montagem continua dentro do teto da ponte, **inclusive ao trocar de
aba**: só a aba aberta atravessa.

### MESA

Continua sendo o espaço de jogo numa **página** — ela é atividade longa, e é o
caso para o qual a página existe. Criação e edição em diálogos. As cinco
atividades continuam em abas, e o formulário de rolagem deixou de ocupar a
largura da mesa.

## 5. O que dá para ver, lado a lado

Os retratos saem de `ferramentas/retratos.cjs`, nos três repositórios:

```sh
node ferramentas/retratos.cjs . abrir /tmp/perfis '[]'
ESTREITO=1 node ferramentas/retratos.cjs . abrir /tmp/perfis-estreito '[]'
```

Conferido nesta rodada, em 1240px e em 460px:

| | Antes | Agora |
|---|---|---|
| Editor do PERFIS | Coluna única; prévia fora da tela à direita | Duas colunas que dividem; empilha em 460px |
| Cartão do PERFIS | Linha com retrato de 32px e uma pastilha | Faixa, retrato de 52px com anel, nome em 15px, pastilhas |
| ESTILO | Página na coluna da conversa | Diálogo de 980px com abas e amostra ao lado |
| Cor | Amostra pequena acima do hexadecimal, em três andares | Amostra e hexadecimal na mesma linha |
| Prévia do ESTILO | Quadradinho com o rótulo **embaixo** dele | Interruptor e texto na mesma linha |
| Dados da MESA | Botão ROLAR por cima do campo | Campo e botão lado a lado, em 420px |
| Prévia do PERFIS | Caixa de 90px numa coluna 60% vazia | Cartão em tamanho real no topo, com a biografia |
| Amostra do ESTILO | Caixa numa coluna estreita, repetida em duas abas | Uma só, largura inteira, no topo |
| Cores do ESTILO | Seis campos numa coluna; a sexta exigia rolar | Seis em duas colunas, no mesmo olhar |
| Título da MESA | Escrito duas vezes, em duas tipografias | Uma vez, na cartela da janela |
| Mesa vazia | «Nenhuma cena em cima da mesa.» | O próximo passo, para quem mestra e para quem joga |

## 6. O diagnóstico UTF-8

`TextEncoder` não existe no QuickJS, e o recuo para `texto.length` media
unidades UTF-16. A contagem passou a ser feita pela tabela do UTF-8, com pares
substitutos. Medido no motor real, pelo teste
`o_preludio_conta_utf8_sem_textencoder`: sete mil «é» dão **14.066 bytes** — o
mesmo número do reteste — e a recusa diz `mensagem-grande` com o tamanho e o
teto, em vez de `fila-cheia`. O teto do Rust continua onde estava.

## 7. O que continua pendente no aplicativo

Nada aqui foi visto na janela do SEELE. O roteiro curto, com os três pacotes
instalados e um cliente:

1. abrir **Perfis** pela navegação; conferir a grade do diretório e a paginação;
2. abrir o editor pelo cartão; conferir campos e prévia lado a lado, e que
   digitar no nome atualiza a prévia;
3. estreitar a janela até o editor empilhar, e conferir que nada some;
4. abrir **Aparência do servidor**; trocar de aba; escolher um conjunto; ligar
   e desligar a prévia; publicar; restaurar;
5. abrir a **Mesa**; criar uma cena; rolar dados; conferir que o formulário não
   domina o tabuleiro;
6. numa pessoa **sem perfil**, conferir que o nome nativo continua e que o
   botão do PERFIS tem rótulo visível;
7. na gestão, escolher «usar apresentação do SEELE» e conferir que **nada** do
   MOD sobra naquele ponto;
8. Tab e Shift+Tab dentro de cada modal, Escape, e o foco voltando ao que abriu;
9. sair do servidor e conferir que nada de MOD sobra na tela.

Os passos 1–9 são de um cliente. Continuam exigindo dois: sincronização entre
participantes, autorização de outra pessoa e qualidade de voz.
