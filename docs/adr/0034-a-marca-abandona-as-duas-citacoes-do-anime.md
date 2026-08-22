# ADR 0034 — A marca abandona as duas citações do anime

**Estado:** aceito
**Data:** 2026-08-22

O [ADR 0033](0033-o-vocabulario-sai-da-interface-a-estetica-fica.md) tirou a
camada de **linguagem** da interface e deixou escrito que a estética fica. Ele
parou na porta da marca de propósito: a folha da época dizia que nada daquilo
alcançava a imagem, porque `Entry Plug` ali era o nome de uma forma desenhada e
não um rótulo de tela.

Este ADR desfaz aquela ressalva. **As duas citações diretas que restavam saem da
imagem, e a estética continua inteira.**

## Contexto

Sobraram exatamente duas, e as duas eram citação literal, não referência:

- **`design/marca/assinatura.svg`** era `ゼーレ` — o nome da organização, na
  grafia da abertura da série. Estava em **contorno**, convertido em `<path>`, e
  é por isso que nenhuma busca por texto no repositório o encontrou: ele não era
  caractere, era desenho de caractere. Passou por três revisões de vocabulário
  sem aparecer em nenhuma.
- **`design/marca/muda.svg`** e `reduzida.svg` eram a silhueta do **plug de
  entrada**, o octógono com a cinta. A forma inteira é do objeto do anime.

O argumento de que uma forma desenhada não é vocabulário era verdadeiro e
insuficiente. O 0033 tirou o termo `Cage` da tela porque ele cobrava aprendizado
sem devolver capacidade; a mesma conta vale para a imagem, com uma diferença que
piora o caso: um termo estranho na tela a pessoa lê uma vez e traduz, e a marca
ela vê em toda aba, todo dock e todo compartilhamento. E o que a marca velha
dizia sobre o produto era nada — nem o plug nem o katakana falam de rede, de par
a par, ou de conversa.

A postura de direitos (`specs/07`, e o README dos ADRs) recomendava repositório
privado até M4 justamente por causa deste tipo de material. Com as duas citações
fora, essa recomendação deixa de pesar sobre a marca.

## Decisão

A marca passa a ser **dois nós e uma ligação**: um quadrado cheio, um quadrado
vazio de massa igual, e uma diagonal a 45° entre os dois.

**O que ela diz, e é a razão de ter sido escolhida entre as explorações:** o nó
cheio é quem hospeda, o nó vazio é quem chega, a diagonal é o enlace. É a
arquitetura do produto num glifo — um par ponto a ponto, sem serviço no meio. A
marca velha não dizia nada sobre o produto; esta não diz outra coisa.

O nome **SEELE** fica, agora escrito em Saira Condensed 900 com tracking
`0,06 em` — a mesma face que o app já embarca e serve. A tagline **DE MÁQUINA A
MÁQUINA** é opcional e só entra a partir de 48 px de símbolo.

`docs/marca.md` é a folha, e foi reescrita inteira.

### O que **não** muda

Isto importa tanto quanto o que muda, e está aqui para que ninguém leia este ADR
como licença para redesenhar o produto:

- **a estética de terminal inteira** — densidade, réguas, caixa alta, o ar de
  console. A `specs/07` continua valendo no que o 0033 deixou de pé;
- **a palheta** — `#F2521F`, `#050403`, `#EAE3CF`, `#7A7061`, os mesmos valores
  congelados em M0.12 e registrados no [ADR 0014](0014-palheta-v2-canonica.md).
  Nenhuma cor nova entrou com a marca nova;
- **o nome SEELE**, na tela, no binário e no repositório;
- **o vermelho reservado** a alerta e queda;
- **a superelipse do `.icns`**, que continua sendo a exceção nomeada da regra 1
  da folha, pelo mesmo motivo de Dock de sempre.

### O comportamento

Três estados, e um deles é o único movimento que a marca faz:

- **conectando** — a diagonal se desenha do nó cheio ao vazio. É a única
  animação permitida;
- **conexão segura** — o símbolo em repouso;
- **queda** — a diagonal some, os dois nós ficam, e nada troca de cor.

**O vermelho nunca toca a marca**, inclusive na queda. Uma marca que fica
vermelha quando a rede cai passou a significar erro, e não dá para desdizer isso
depois: o alerta é da interface em volta.

## Alternativas

- **Manter o katakana e trocar só o plug.** Metade do problema, e a metade
  errada: o katakana é que é o nome próprio da organização do anime.
- **Estilizar o plug até não ser mais reconhecível.** Ou ainda se reconhece o
  objeto, e não resolve nada, ou não se reconhece mais, e aí é um desenho novo
  com um nome de arquivo mentiroso.
- **Uma marca puramente tipográfica, só o wordmark.** Barata e sem um símbolo
  que caiba em 16 px. Favicon, bandeja e dock precisam de forma, e a alternativa
  seria a inicial dentro de um quadrado — que é o que todo produto faz.
- **Um traço de letra a menos no wordmark que o comp já tinha.** Não foi
  considerado seriamente: o nome não estava em discussão.

## Consequências

**Fica mais fácil.** A marca passa a ser a mesma nos dois clientes: `■—□` no
terminal é o mesmo desenho do SVG, enquanto `ゼーレ` ocupava célula dupla, media
diferente em cada emulador, e obrigava a TUI a usar uma forma latina que o app
gráfico não usava. A folha perdeu uma seção inteira por causa disso.

O ícone de app caiu de quatro faixas de desenho para duas. O plug tinha contorno
de octógono, cinta e placas de profundidade, e a razão entre eles quebrava em
quatro pontos de tamanho diferentes; o símbolo tem um valor de traço só, então há
um limiar só, em 48 px.

**Fica mais difícil.** Toda a arte foi refeita, e quem tiver uma captura de tela,
um comp, ou um pacote assinado de antes vai encontrar a marca velha lá. As seis
placas de profundidade da palheta deixaram de ter uso na marca — os tokens
continuam em `tokens.css`, agora sem consumidor.

`apps/seele-app/tests/marca.rs` guardava nove decisões desta folha e várias eram
da marca velha. Nenhuma foi apagada: as que sobreviveram ficaram com a razão
nova escrita no comentário, e as que caíram foram reescritas para cobrar a
propriedade que passou a valer, com o que elas cobravam antes dito por extenso
no próprio teste. Um teste que se conserta afrouxando não vale o arquivo em que
está.
