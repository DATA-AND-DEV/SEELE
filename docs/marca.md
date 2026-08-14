# A marca

Normativo, como `specs/07-tema-evangelion.md`. A diferença é o alcance: o tema
governa o vocabulário do produto — Dogma, Cage, Piloto — e esta folha governa a
única imagem que o produto tem de si mesmo.

Três formas. A escolha entre elas é sempre a mesma pergunta: **quanto espaço
existe e o que a tela precisa dizer.** Nunca gosto.

---

## As três formas

### Marca reduzida solta — o plug com o nome

`design/marca/reduzida.svg`

O contorno do Entry Plug com `ゼーレ` na faixa laranja. Onde não cabe texto e a
marca precisa ser reconhecida de relance **dentro da interface**, sobre o negro
do produto:

- Avatar do Dogma na lista
- Indicador de plug inserido
- Cabeçalho de painel onde o nome já foi dito uma vez

**Tamanho mínimo: 32 px de largura.** Abaixo disso a forma muda.

### Ícone de app — o plug sem o nome

`design/marca/icone-app-16.svg`, `-32`, `-64`, `-128`

O mesmo plug com a **cinta vazia**, e quatro arquivos em vez de um. Não é a
marca reduzida solta encolhida, e a diferença não é gosto: é aritmética. A cinta
tem 38 de 162 de altura e o nome ocupa a largura toda dela — a 128 px cada
katakana teria seis pixels de largura, e três borrões numa cinta não são um
nome. Então no ícone o nome não entra em tamanho nenhum. Ele fica na
inicialização, na assinatura e na cartela, que é onde há espaço para lê-lo.

Cada faixa é um desenho próprio, com traço mais grosso conforme o orçamento de
pixel encolhe e placas de profundidade que vão sumindo até sobrar a silhueta:

| Faixa | Bloco | Arquivo | Traço |
| --- | --- | --- | --- |
| 4 placas | 128 e acima | `icone-app-128.svg` | 7 |
| 2 placas | 64 a 127 | `icone-app-64.svg` | 9 |
| 1 placa | 32 a 63 | `icone-app-32.svg` | 12 |
| muda | abaixo de 32 | `icone-app-16.svg` | 16 |

Cada tamanho gerado sai do arquivo da sua faixa, **nunca** da redução do maior:
reduzir o de 128 até 16 devolve um traço de meio pixel, que é como um ícone fica
cinza no dock. `design/marca/gerar-icones.py` faz a escolha por tabela e
`apps/seele-app/tests/marca.rs` confere as quatro faixas.

### Forma muda

`design/marca/muda.svg`

O mesmo plug, traço mais grosso, uma barra no lugar do nome. A silhueta continua
legível a 16 px. É a forma de favicon, a da bandeja do sistema, e a construção
que os ícones transparentes usam em toda faixa (regra 6).

### Assinatura — o logotipo

`design/marca/assinatura.svg`

`ゼーレ` com o traço `ー` em laranja, e a legenda latina `SEELE` abaixo, opcional.
Onde o nome precisa ser dito com espaço para respirar: inicialização,
autenticação, cabeçalho de documentação, tela cheia de erro fatal.

O `ー` é o único elemento colorido. Ele ocupa o lugar da régua laranja do
sistema — **nunca** se põe uma segunda régua ao lado do logotipo.

**Mínimo: 28 px de altura.**

### Forma institucional — a cartela

Katakana no campo sólido, latim no campo vazio, descritor em duas linhas fixas.
Onde o nome precisa vir acompanhado do que a coisa é: rodapé, changelog,
README, compartilhamento social. **Mínimo: 180 px de largura.** A assimetria dos
dois campos é a marca; a moldura não é.

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
2. **Uma forma por tela.** Se o plug está no canto, o logotipo não aparece ali.
   Repetir duas formas é o erro mais comum e o que mais barateia a marca.
3. **O `ー` é o único glifo laranja da assinatura.**
4. **Vermelho (`#FF1A1A`) é reservado a falha.** A marca nunca usa vermelho —
   ela não pode significar erro.
5. **Área livre** ao redor de qualquer forma: metade da largura do plug.
6. Sem sombra, gradiente, contorno extra, ou marca sobre imagem.
   **Segunda exceção nomeada: os ícones transparentes de Windows e de Linux.**
   A regra existe contra a marca sobre fotografia, onde o fundo tem detalhe e
   compete com ela. Barra de tarefas não é fotografia: é uma superfície lisa de
   cor desconhecida. Vale dizer isso em vez de deixar a tensão sem explicação —
   um fundo transparente é, literalmente, a marca sobre o que o sistema puser
   atrás. É por causa disso que o alvo transparente usa a **construção solta,
   tudo laranja e nada preto**, a mesma de `muda.svg`. **E o que sustenta a
   exceção é a convenção, não uma razão de contraste** — porque a razão não
   sustenta. Medido pela fórmula de luminância relativa da sRGB, `#F2521F` fica
   em **5,71:1** sobre `--seele-negro-painel` (`#0A0806`), que é o número que
   `design/seele-tokens.css` e o ADR 0014 já registram para este par, e em
   **5,86:1** sobre `--seele-negro-absoluto` (`#050403`). Do lado claro não há
   número bom: sobre **branco puro** dá **3,50:1**, e isso é o **teto** — nenhuma
   superfície clara faz melhor —; sobre o próprio `--seele-osso` (`#EAE3CF`) cai
   para **2,73:1**, abaixo do critério de 3:1 para elemento não textual. Numa
   barra de tarefas clara a marca solta fica pouco acima do mínimo, e sobre um
   creme fica abaixo dele. O que decide a favor da construção solta é a
   alternativa: pegar a arte de ícone e só tirar a placa faz o contorno preto e
   a cinta preta sumirem numa barra escura, e sobra um anel laranja oco, que não
   é um plug. Uma forma reconhecível com contraste apertado no claro é melhor
   que uma forma que deixa de ser a marca no escuro — mas é uma troca, e está
   escrita aqui como troca.
7. **Fontes: Noto Sans JP 900 e Saira Condensed 900. Sem substituição.**
8. Abaixo do mínimo, **trocar de forma** — nunca reduzir mais.

Duas cores: laranja sobre preto, ou preto sobre laranja. As placas de
profundidade não são uma terceira: são a mesma dupla escalonada em cor plana
deslocada, que é como esta marca dá volume sem sombra e sem gradiente.

### As cores são as dos tokens congelados

| Papel | Valor | Token |
| --- | --- | --- |
| Marca | `#F2521F` | `--seele-laranja-nerv` |
| Fundo e contra-cor | `#050403` | `--seele-negro-absoluto` |
| Texto do logotipo | `#EAE3CF` | `--seele-osso` |
| Descritor | `#7A7061` | `--seele-osso-apagado` |
| Borda 1px | `#241F19` | `--seele-linha` |
| Placa 1 sobre negro | `#A83A10` | `--seele-placa-negro-1` |
| Placa 2 sobre negro | `#7A2A0B` | `--seele-placa-negro-2` |
| Placa 3 sobre negro | `#4A1806` | `--seele-placa-negro-3` |
| Placa 1 sobre laranja | `#FFA070` | `--seele-placa-laranja-1` |
| Placa 2 sobre laranja | `#C4400F` | `--seele-placa-laranja-2` |
| Placa 3 sobre laranja | `#8E2A08` | `--seele-placa-laranja-3` |

Não são valores novos: são os mesmos de `apps/seele-app/ui/tokens.css`,
congelados em M0.12. Um teste confere que a marca não introduz nenhuma cor fora
dessa lista.

As seis placas entraram com o ícone de app e são da marca desenhada, não da
interface: nenhuma superfície do produto se pinta com elas, e por isso elas não
têm índice ANSI — no terminal a marca é a forma latina.

---

## No terminal

`ゼーレ` ocupa célula dupla e nem todo emulador mede igual. Uma marca que às
vezes desalinha o quadro não é uma marca.

**Dentro da TUI, a forma latina.** O katakana fica para o app gráfico, o site e
o impresso.

```text
assinatura   7 células      ──SEELE──
reduzida     7 × 3          ┌─────┐
                            [inv] SEELE
                            └─────┘
favicon      3 células      [S]        (S em vídeo inverso)
```

256 cores ANSI: laranja `202`, osso `230`, apagado `243`.
16 cores: `yellow-bold`, `white`, `white-dim`.

Isto **não** revoga os kanji do tema. `同期率` na barra e `警告` no alerta
continuam como `specs/07` manda: são vocabulário da interface, não a marca.

---

## Por que os glifos estão em contorno

Os SVGs de `design/marca/` não contêm texto: os três katakana estão convertidos
em `<path>`. Foi uma decisão, não uma conveniência de exportação.

O app não embarca fonte nenhuma — `withGlobalTauri`, sem bundler, sem npm
(ADR 0019). Se `ゼーレ` fosse texto, a marca seria Hiragino no macOS, Yu Gothic
no Windows, e uma caixa vazia numa máquina sem face japonesa. A regra 7 proíbe
exatamente isso. Contorno é o que torna a regra 7 verdadeira em vez de pedida.

O preço é que a marca não se edita num editor de texto. É o preço certo: uma
marca não deveria ser editável num editor de texto.

`apps/seele-app/tests/marca.rs` reprova qualquer SVG da marca que volte a ter
`<text>` ou `font-family`.

---

## Como regerar

```sh
python3 design/marca/gerar-icones.py
```

Só no macOS: usa o `qlmanage` e o `iconutil` do sistema. **Não roda em build nem
em CI** — o que o build usa são os arquivos versionados. Rode quando a marca
mudar, e comite o resultado.

Se algum dia for preciso regerar os contornos a partir da fonte, a origem é
Noto Sans JP Black (SIL OFL), glifos `ゼ`, `ー`, `レ`, upem 1000.

---

## Direitos

`specs/07-tema-evangelion.md` já trata disso e vale aqui inteiro: o vocabulário
e a estética são referência, não material do anime. Nenhum frame, nenhum
logotipo da NERV, nenhum asset de terceiro entra no produto ou no repositório. A
marca desta folha é desenho original — um octógono, uma faixa, e três katakana
de uma fonte livre.
