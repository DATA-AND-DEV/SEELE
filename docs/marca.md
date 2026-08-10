# A marca

Normativo, como `specs/07-tema-evangelion.md`. A diferença é o alcance: o tema
governa o vocabulário do produto — Dogma, Cage, Piloto — e esta folha governa a
única imagem que o produto tem de si mesmo.

Três formas. A escolha entre elas é sempre a mesma pergunta: **quanto espaço
existe e o que a tela precisa dizer.** Nunca gosto.

---

## As três formas

### Marca reduzida — o plug

`design/marca/reduzida.svg`

O contorno do Entry Plug com `ゼーレ` na faixa laranja. Onde não cabe texto e a
marca precisa ser reconhecida de relance.

- Ícone de aplicativo e de dock
- Favicon e aba do navegador
- Avatar do Dogma na lista
- Indicador de plug inserido

**Tamanho mínimo: 32 px de largura.** Abaixo disso a forma muda, e é a única
troca automática do sistema — está implementada como um `if` em
`design/marca/gerar-icones.py` e conferida em `apps/seele-app/tests/marca.rs`.

### Forma muda

`design/marca/muda.svg`

O mesmo plug, traço mais grosso, uma barra no lugar do nome. A silhueta continua
legível a 16 px. É a forma de favicon e a de qualquer ícone abaixo de 32 px.

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
2. **Uma forma por tela.** Se o plug está no canto, o logotipo não aparece ali.
   Repetir duas formas é o erro mais comum e o que mais barateia a marca.
3. **O `ー` é o único glifo laranja da assinatura.**
4. **Vermelho (`#FF1A1A`) é reservado a falha.** A marca nunca usa vermelho —
   ela não pode significar erro.
5. **Área livre** ao redor de qualquer forma: metade da largura do plug.
6. Sem sombra, gradiente, contorno extra, ou marca sobre imagem.
7. **Fontes: Noto Sans JP 900 e Saira Condensed 900. Sem substituição.**
8. Abaixo do mínimo, **trocar de forma** — nunca reduzir mais.

Duas cores: laranja sobre preto, ou preto sobre laranja.

### As cores são as dos tokens congelados

| Papel | Valor | Token |
| --- | --- | --- |
| Marca | `#F2521F` | `--seele-laranja-nerv` |
| Fundo e contra-cor | `#050403` | `--seele-negro-absoluto` |
| Texto do logotipo | `#EAE3CF` | `--seele-osso` |
| Descritor | `#7A7061` | `--seele-osso-apagado` |
| Borda 1px | `#241F19` | `--seele-linha` |

Não são valores novos: são os mesmos de `apps/seele-app/ui/tokens.css`,
congelados em M0.12. Um teste confere que a marca não introduz nenhuma cor fora
dessa lista.

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
