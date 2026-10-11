# O `operador.barra` do desktop sob uma região clara (F8, 10/10)

Medido ao fazer o tema da região no celular (`2cfcdde4`), com o mesmo método
de contraste da `apps/seele-app/bancada/capa.cjs` da F3, no Chromium a
1400 × 900, na `versao/0.16` da décima segunda integração (`b0efa45d`).

A região é a paleta clara da bancada: fundo `#f2ece0`, texto `#12100e`,
apagado `#574e43`, acento `#a83a10`, borda `#b8ad9a`. Ela chega pelo
`tema_do_servidor` do quadro, e a medida é feita depois dos redesenhos.

| texto da `.operador` | sem tema | com a região |
|---|---|---|
| uma contribuição de MOD no `#operador-mods` | 15,6 | **1,09** |
| COMPARTILHAR | 15,6 | **1,09** |
| o rótulo OPERADOR | 5,52 | 6,93 |
| o nome | 5,83 | 4,53 |
| a inicial | 4,11 | 4,13 |
| a frase da voz | 4,11 | 4,11 |
| SAIR DA SALA (núcleo) | 15,6 | 15,6 |

A `.operador` toma o fundo da região, mas não declara `color`. O que está
dentro dela, e não declara a própria cor, herda a tinta já resolvida em
`#tela-sessao` com os tokens da sessão. A `.regiao-de-mod` declara
`color: var(--seele-osso)` e escapa; a `.contribuicao-de-mod` e o
COMPARTILHAR não. As outras regiões não foram medidas.

No celular, os contêineres da casca declaram a tinta e o fundo, e o caso
`regiao-do-operador` da `tools/bancada-do-celular.py` confere que tudo neles
fica acima de 4,5:1.

`desktop-regiao-clara.png` é o recorte da barra com a região. Para medir de
novo:

```
python3 docs/evidencias/f8-regiao-do-operador/medir.py /tmp/regiao
```
