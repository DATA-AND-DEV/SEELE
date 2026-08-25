# A marca

Normativo, como `specs/07-tema-evangelion.md`. A diferença é o alcance: o tema
governa a estética do produto — cores, densidade, o ar de terminal — e esta
folha governa a única imagem que o produto tem de si mesmo.

O **ADR 0033** tirou a camada de linguagem da interface: na tela se lê servidor,
sala de voz, canal de texto, pessoa. O **ADR 0034** completou o movimento na
imagem: saíram as duas citações diretas que restavam — o katakana `ゼーレ` e a
silhueta do connection de entrada — e entrou um símbolo desenhado do zero.

**Nada disso alcança a estética.** A palheta é a mesma, o ar de terminal é o
mesmo, o nome continua SEELE. O que mudou é o desenho que diz o nome.

---

## O símbolo

`design/marca/simbolo.svg` — a fonte de todas as formas.

**Dois nós e uma ligação.** O nó cheio é quem hospeda, o nó vazio é quem chega,
a diagonal é o enlace entre os dois. É a arquitetura do produto num glifo: um
par ponto a ponto, e nada entre eles além da linha que os une.

Geometria, numa grade de 96:

| Parte | Forma | Cor |
| --- | --- | --- |
| nó cheio | `rect x=12 y=12 w=24 h=24` | `#F2521F` preenchido |
| nó vazio | `rect x=62 y=62 w=20 h=20`, traço 4 | `#F2521F` no traço |
| enlace | `M34 34 L62 62`, traço 4, a 45° | `#EAE3CF` |

O nó vazio tem lado 20 e traço 4 **centrado**: a extensão externa dá 24, igual à
do cheio. As massas são iguais de propósito — quem hospeda e quem chega pesam o
mesmo, e o que os distingue é o furo, não o tamanho.

O enlace é desenhado **antes** dos nós. As pontas dele entram 2 unidades dentro
de cada nó e ficam escondidas embaixo; sem essa ordem a linha aparece pingando
para fora do nó cheio.

Regras de construção: grade de 96, traço e respiro em múltiplos de 4, área de
respiro de um nó cheio (24) em toda a volta.

---

## As formas

### Assinatura — símbolo e nome

`design/marca/assinatura.svg`

Símbolo mais o wordmark **SEELE** em Saira Condensed 900, tracking `0,06 em`,
em `#EAE3CF`. O respiro entre um e outro é um nó cheio. A caixa alta do wordmark
é metade do quadro do símbolo, e a linha de base põe o centro óptico da palavra
no centro do símbolo.

Onde o nome precisa ser dito com espaço para respirar: inicialização,
autenticação, cabeçalho de documentação, tela cheia de erro fatal.

**Mínimo: 16 px de altura de símbolo**, e aí o wordmark vem junto.

### Assinatura com tagline

`design/marca/assinatura-tagline.svg`

A mesma, com **DE MÁQUINA A MÁQUINA** por baixo, `letter-spacing 0,42 em`, em
`#7A7061`. A tagline é **opcional** e **só entra a partir de 48 px de altura de
símbolo**: abaixo disso a caixa alta dela não chega a 5 px e o tracking a
espalha até virar textura.

Ela alinha pela esquerda do símbolo, não pela do wordmark — por baixo do
conjunto inteiro ela é uma régua; por baixo só do wordmark seria uma terceira
coluna.

### Empilhada

`design/marca/empilhada.svg`

Símbolo em cima, wordmark embaixo, os dois centrados. Para suporte alto e
estreito, onde a assinatura deitada não cabe.

### Mono — uma cor

`design/marca/mono.svg`

Tudo em `#EAE3CF`. Para gravação, carimbo, bordado, e qualquer suporte que herde
a cor de fora. O laranja vira osso, e não o contrário. **O nó vazio continua
vazio**: sem cor, o furo é a única coisa que ainda separa quem chega de quem
hospeda.

### Forma muda — o favicon

`design/marca/muda.svg`

O símbolo sozinho, **redesenhado para 16 px**. Não é o símbolo encolhido, e a
diferença é aritmética: reduzido a 16 px o traço de 4 vale 0,67 px — meio pixel
cinza, que é como um ícone some numa aba. Aqui o traço é 6 (1 px exato) e o furo
do nó vazio é 12 (2 px). A extensão externa dos dois nós continua 24: é o furo
que paga o traço mais grosso, não o tamanho do nó.

É a forma de favicon e a da bandeja do sistema. **Mínimo: 16 px.**

### Ícone de app

`design/marca/icone-app-128.svg` e `-16`

O símbolo invertido em negro sobre a placa laranja. **Duas faixas**, e o corte
está em 48 px:

| Faixa | Arquivo | Traço | Serve |
| --- | --- | --- | --- |
| larga | `icone-app-128.svg` | 4 | 48 px e acima |
| miúda | `icone-app-16.svg` | 6 | abaixo de 48 px |

O traço de 4 vale `lado / 24` px, e a partir de 48 px isso passa de 2 px; abaixo
disso entra o desenho da faixa miúda, que vale 2 px a 32 e 1 px a 16. Cada
tamanho gerado sai do arquivo da sua faixa, **nunca** da redução do maior.
`design/marca/gerar-icones.py` faz a escolha por tabela e
`apps/seele-app/tests/marca.rs` confere a conta nas duas faixas.

**Eram quatro faixas quando a marca era o connection**, e ter duas agora não é
afrouxamento: o connection tinha contorno de octógono, cinta e placas de profundidade,
e a razão entre eles quebrava em quatro pontos diferentes. O símbolo tem um
valor de traço só, então há um limiar só.

**A placa laranja vai em todo sistema** — fora dela o símbolo teria de ser
entregue sobre fundo desconhecido, e a regra 6 proíbe isso. O que muda é o
enquadramento, e só ele: no macOS a placa é recortada na superelipse da regra 1,
com a folga da grade da Apple; em Windows e Linux ela é o quadro inteiro, de
canto reto e opaca, sangrada até a borda.

### Forma institucional — a cartela

`design/marca/cartela.svg`

Campo cheio à esquerda com o símbolo invertido, campo vazio à direita com o nome
e a tagline em duas linhas. Onde o nome precisa vir acompanhado do que a coisa
é: rodapé de página, changelog, README, compartilhamento social. **Fora da
interface do produto** — dentro das telas quem aparece é o símbolo. **Mínimo:
180 px de largura.** A assimetria dos dois campos é a marca; a moldura não é.

---

## As regras

1. **`border-radius` nunca.** Nem no ícone de app, nem em máscara do sistema.
   **Exceção nomeada: o `.icns` do macOS e os PNGs que o alimentam.** Ali a
   placa laranja é uma superelipse de 824 num quadro de 1024 — a grade de ícone
   da Apple, `|x/a|^n + |y/b|^n = 1` com `n = 5`, desenhada como caminho fechado
   em `design/marca/gerar-icones.py`. O motivo é que o macOS **não** recorta
   ícone de aplicativo: entrega o quadro como está, e um quadrado sangrado fica
   visivelmente maior e mais duro que todo vizinho no Dock — o único ícone da
   fila que parece não pertencer a ela. Isto é uma exceção decidida, não uma
   deriva: quem encontrar a curva no gerador e quiser endireitá-la está
   revogando esta linha, não corrigindo um deslize. Fora do `.icns` — Windows,
   Linux, favicon, bandeja, interface — continua canto reto.
2. **Uma forma por tela.** Se o símbolo está no canto, a assinatura não aparece
   ali. Repetir duas formas é o erro mais comum e o que mais barateia a marca.
3. **Nunca preencher o nó vazio, nunca inverter a diagonal, nunca rotacionar.**
   Preencher o nó vazio apaga a diferença entre quem hospeda e quem chega;
   inverter a diagonal troca quem chamou quem; rotacionar tira o par do eixo
   que a grade de 45° dá a ele.
4. **Vermelho (`#FF1A1A`) é reservado a falha.** A marca nunca usa vermelho —
   ela não pode significar erro. Vale inclusive na queda: ali a diagonal some e
   os dois nós ficam como estão, sem trocar de cor.
5. **Área livre** ao redor de qualquer forma: um nó cheio, 24 na grade de 96.
6. Sem sombra, gradiente, contorno extra, raio, ou marca sobre imagem. **Nunca
   sobre laranja** a não ser invertida em negro, que é o caso da placa e do
   campo cheio da cartela.
7. **Fonte: Saira Condensed 900. Sem substituição** — e por isso os arquivos
   trazem a letra em contorno, ver abaixo.
8. Abaixo do mínimo, **trocar de forma** — nunca reduzir mais.

Duas cores: laranja sobre negro, ou negro sobre laranja, com o osso no enlace e
no nome.

### As cores são as dos tokens congelados

| Papel | Valor | Token |
| --- | --- | --- |
| Nós | `#F2521F` | `--seele-laranja-nerv` |
| Fundo e contra-cor | `#050403` | `--seele-negro-absoluto` |
| Enlace e wordmark | `#EAE3CF` | `--seele-osso` |
| Tagline | `#7A7061` | `--seele-osso-apagado` |

Não são valores novos: são os mesmos de `apps/seele-app/ui/tokens.css`,
congelados em M0.12. Um teste confere que a marca não introduz nenhuma cor fora
dessa lista.

**As seis placas de profundidade saíram da marca com o connection.** Elas eram cor
plana deslocada por trás de um contorno de octógono, para dar volume sem sombra;
o símbolo de dois nós não tem contorno para deslocar. Os tokens continuam em
`tokens.css` — tirá-los de lá é decisão de quem cuida da folha de tokens, não
desta folha.

O par em que a marca vive é laranja sobre o negro do produto, e ele tem número:
medido pela fórmula de luminância relativa da sRGB, `#F2521F` fica em **5,71:1**
sobre `--seele-negro-painel` (`#0A0806`) e em **5,86:1** sobre
`--seele-negro-absoluto` (`#050403`). São os mesmos valores que
`design/seele-tokens.css` e o ADR 0014 registram para este par. Fora do produto
a marca não é entregue sobre fundo desconhecido: no ícone ela vem sempre sobre a
própria placa.

---

## O comportamento

Três estados, e um deles é o único movimento que a marca faz:

- **conectando** — o enlace se desenha do nó cheio ao vazio. É a **única**
  animação permitida na marca;
- **conexão segura** — o símbolo em repouso, inteiro;
- **queda** — o enlace some, os dois nós ficam. Nada troca de cor.

A queda não pinta a marca de vermelho, e é a regra 4 dizendo a mesma coisa por
outro lado: o alerta é da interface em volta, não da marca. Uma marca que fica
vermelha quando a rede cai passou a significar erro, e não dá para desdizer isso
depois.

---

## No terminal

A TUI não desenha SVG. A marca ali é o próprio símbolo em três células:

```text
assinatura   ■—□ SEELE
símbolo      ■—□
queda        ■ □
favicon      ■          (uma célula, em vídeo inverso)
```

O desenho não muda de alfabeto entre o gráfico e o terminal, que era o preço da
marca velha: `ゼーレ` ocupava célula dupla, nem todo emulador media igual, e a
TUI tinha de usar uma forma latina que o app gráfico não usava. **Agora é o
mesmo desenho nos dois** — dois blocos e um traço são caracteres que toda fonte
de terminal tem na mesma largura.

256 cores ANSI: laranja `202`, osso `230`, apagado `243`.
16 cores: `yellow-bold`, `white`, `white-dim`.

---

## Por que a letra está em contorno

Os SVGs de `design/marca/` não contêm `<text>`: o wordmark e a tagline estão
convertidos em `<path>`. Foi uma decisão, não uma conveniência de exportação — e
a razão **mudou** com o ADR 0034 sem mudar a decisão.

A razão velha era o katakana: sem contorno, `ゼーレ` viraria a face japonesa que
o sistema tivesse. Não há mais japonês.

A razão nova vale para o alfabeto latino do mesmo jeito. Os SVGs da marca nunca
são lidos dentro da página: `marca-muda.svg` é favicon, `marca-simbolo.svg` entra
por `<img>`, e os de ícone são rasterizados pelo `qlmanage`. Nos três casos o SVG
é um **documento isolado** — o `@font-face` que `ui/fontes.css` declara não
alcança lá dentro, e o `qlmanage` não tem folha de estilo nenhuma. Um
`font-family: "Saira Condensed"` cairia no segundo item da pilha, Arial Narrow,
que é exatamente a falha silenciosa que `fontes.css` descreve.

Embutir a fonte em `data:` dentro de cada arquivo resolveria e custa 20 KB por
arquivo, num favicon. Contorno custa o tamanho do desenho e não depende de nada.

O preço é que a marca não se edita num editor de texto. É o preço certo: uma
marca não deveria ser editável num editor de texto.

`apps/seele-app/tests/marca.rs` reprova qualquer SVG da marca que volte a ter
`<text>` ou `font-family`.

---

## Como regerar

```sh
python3 design/marca/gerar-wordmark.py   # as formas com letra
python3 design/marca/gerar-icones.py     # os ícones, e as cópias para ui/
```

O primeiro lê `design/marca/simbolo.svg` e a fonte embarcada em
`apps/seele-app/ui/fontes/saira-condensed-900.woff2` — a mesma face que o app
serve, então a marca e a interface são a mesma letra. Licença e procedência em
`ui/fontes/PROCEDENCIA.md`.

O segundo só roda no macOS: usa o `qlmanage` e o `iconutil` do sistema. **Não
roda em build nem em CI** — o que o build usa são os arquivos versionados. Rode
quando a marca mudar, e comite o resultado.

O tracking sai aplicado em unidades de fonte, e não por `letter-spacing`: em
contorno não existe `letter-spacing`, existe a posição de cada glifo.

---

## Direitos

`specs/07-tema-evangelion.md` já trata disso e vale aqui inteiro: a estética é
referência, não material do anime. O ADR 0033 tirou o vocabulário da tela e o
ADR 0034 tirou as duas últimas citações da imagem — o katakana e o connection. Nenhum
frame, nenhum logotipo da NERV, nenhum asset de terceiro entra no produto ou no
repositório. A marca desta folha é desenho original: dois quadrados e uma
diagonal, mais o nome numa fonte livre.
