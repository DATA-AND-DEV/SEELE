# Procedência das três faces

O ADR 0019 recusou npm para não carregar uma árvore de dependências que ninguém
auditou. Fonte que chega à mão obedece à mesma regra: cada arquivo aqui tem
origem nomeada, versão, licença lida e resumo conferível. Nada veio de CDN, e
nada veio de "o que o Google Fonts servia naquele dia".

As três são **SIL Open Font License 1.1**. Não foi presumido: o texto de licença
de cada pacote baixado está ao lado, e o registro `name` ID 13 de cada `.woff2`
publicado repete a mesma frase.

## Saira Condensed — 500, 700, 900

| | |
|---|---|
| Origem | <https://github.com/Omnibus-Type/Saira> |
| Commit | `1916f2a575479b626238d9842126e63aa208eebf` ("generated fonts 1.101") |
| Versão da face | `Version 1.101; ttfautohint (v1.8.3)` |
| Licença | SIL OFL 1.1 · `LICENCA-saira-condensed.txt` |
| Direitos | Copyright 2020 The Saira Project Authors, com Reserved Font Name "Saira" |

O repositório não publica *release* nem *tag*: o que existe de nomeado é o
commit de build, cuja mensagem é a própria versão (`1.101`), e é o que a tabela
`name` das faces confirma. Baixado por caminho fixo no commit, nunca por `master`:

```
https://raw.githubusercontent.com/Omnibus-Type/Saira/1916f2a575479b626238d9842126e63aa208eebf/Saira/fonts/ttf/SairaCondensed-{Medium,Bold,Black}.ttf
https://raw.githubusercontent.com/Omnibus-Type/Saira/1916f2a575479b626238d9842126e63aa208eebf/OFL.txt
```

SHA-256 do que foi baixado:

```
9aced533b166bdeb432764f9231a58bcaf4b4db14ba4c525d0af8f8663c08c75  SairaCondensed-Medium.ttf
647c3a2f6183d1d4908c0edf8fff4f5e0c4a1854f2303f4d93f1cc3dd2a1c0d3  SairaCondensed-Bold.ttf
15e16fff1dd71a1b7de193d5f017cccdac0b5c416b9e5ecc905bca3aae4a3f9e  SairaCondensed-Black.ttf
f2665d4718b452b3818a877191355ac884a6b9b419d35408fe7ee487e9e8f30f  OFL.txt
```

## IBM Plex Mono — 400, 500, 600

| | |
|---|---|
| Origem | <https://github.com/IBM/plex/releases/tag/%40ibm%2Fplex-mono%402.5.0> |
| Release | `@ibm/plex-mono@2.5.0`, ativo `ibm-plex-mono.zip` |
| Versão da face | `Version 2.005` |
| Licença | SIL OFL 1.1 · `LICENCA-ibm-plex-mono.txt` |
| Direitos | Copyright © 2017 IBM Corp., com Reserved Font Name "Plex" |

Sobre a troca de licença que se atribui ao Plex: **não foi confirmada nesta
família**. O `LICENSE.txt` do pacote 2.5.0 e o do 1.1.0 são a mesma OFL 1.1, com
o mesmo cabeçalho de 2017 — a versão anterior foi baixada e lida só para isso.
Quem for revisar o histórico completo do IBM/plex deve fazê-lo antes de repetir a
afirmação; aqui vale o que está no arquivo ao lado.

```
https://github.com/IBM/plex/releases/download/@ibm/plex-mono@2.5.0/ibm-plex-mono.zip
6d23f01257663d8cc49a0d64c22ced630b79e0e2a0ac08a0da86e9a38bbc481c  ibm-plex-mono.zip
```

As faces vieram de `ibm-plex-mono/fonts/complete/otf/IBMPlexMono-{Regular,Medium,SemiBold}.otf`
dentro desse arquivo.

## Noto Sans JP — 700, 900

| | |
|---|---|
| Origem | <https://github.com/notofonts/noto-cjk/releases/tag/Sans2.004> |
| Release | `Sans2.004`, ativo `16_NotoSansJP.zip` |
| Versão da face | `Version 2.004;hotconv 1.0.118;makeotfexe 2.5.65603` |
| Licença | SIL OFL 1.1 · `LICENCA-noto-sans-jp.txt` |
| Direitos | © 2014-2021 Adobe (os fontes do Noto CJK derivam do Source Han Sans) |

Esta é a família em que a licença **de fato** mudou de versão maior: o Noto CJK
1.x saiu sob Apache 2.0 e a linha 2.x é OFL 1.1. Por isso a origem é a release
`Sans2.004` e não um espelho qualquer — e o `LICENSE` que veio dentro dela é o
que está ao lado.

```
https://github.com/notofonts/noto-cjk/releases/download/Sans2.004/16_NotoSansJP.zip
2bbdd2c20f30670b39ca735c96d75f1fdabdb348103e43b820cf17701fd22b18  16_NotoSansJP.zip
```

## Recorte

`fonttools` 4.63.0. Cada face foi recortada para o que o produto desenha, nunca
para "o alfabeto todo por via das dúvidas". Comando comum a todas:

```
python -m fontTools.subset <origem> --output-file=<destino> --flavor=woff2 \
  --name-IDs=0,1,2,3,4,5,6,13,14 --layout-features='*' --desubroutinize <recorte>
```

`--name-IDs` preserva direitos (0), licença (13) e URL da licença (14): um
recorte que apaga a licença de dentro do arquivo é um recorte que perde a
procedência que este documento existe para guardar. `--layout-features='*'`
mantém `tnum`, que `seele.css` usa em toda métrica.

### Saira Condensed

```
--unicodes='U+0020-007E,U+00A0-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2039-203A,U+2212'
```

Latino básico, Suplemento Latin-1 e a pontuação tipográfica. Nada de blocos,
setas ou traços de caixa: a face de cartela nunca desenha nenhum deles — e, mais
importante, **nunca desenha texto de terceiros**. Ela aparece em `.marca`,
`.boot-marca.fim` e nos números de cartela, todos texto do produto, em
português. Latin Extended-A ficaria 3 KB por peso mais pesado sem cobrir
nenhuma tela.

### IBM Plex Mono

```
--unicodes='U+0020-007E,U+00A0-00FF,U+0100-017F,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2039-203A,U+2212,U+2190-2193,U+2260,U+2264-2265,U+2500-257F,U+2580-259F,U+25A0-25CF'
```

Esta é a face que carrega o que **a pessoa digita**: corpo de mensagem, apelido,
endereço de Server, link de convite. Por isso o recorte é generoso onde o da
Saira é apertado — Latin Extended-A entra porque quem escreve não é o produto.
O resto saiu de leitura, não de palpite: `ui/index.html`, `ui/seele.css`,
`ui/seele.js` e `design/SEELE v2.dc.html` foram varridos caractere a
caractere, e o que apareceu fora do ASCII alfanumérico foi `·  —  –  …  •  →  ←
↑  ↓  ≥  ≤  −  ×  █  ▓  ▒  ░  ▁▂▃▄▅▆▇  ─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼ ═ ╞ ╡ ╪  ▸ ◂ ▶ ◀
▼ ⌘ ● ○ ≡`, mais os acentos do português. Os blocos e os traços de caixa vêm da
variante terminal-safe do comp, que é desenhada em mono.

**Nem tudo que se pediu existe na face.** A faixa `U+25A0-25CF` foi pedida
inteira e devolveu **um** glifo: `▸ ◂ ▶ ◀ ▼ ▴ ▾ ● ○` não estão no `cmap` do IBM
Plex Mono. `≡` e `⌘` também não. Isso é ausência **na face**, não recorte do
subconjunto: o `IBMPlexMono-Regular.otf` 2.005 de onde estas saíram tem 1049
entradas de `cmap` e exatamente um glifo em `U+25A0-25CF`. Prova pelo contrário:
`█ ▓ ▒ ░ ─ → · —` foram pedidos pela mesma linha, existem na face e viajam
embarcados.

Seis caracteres, em seis lugares, caem na monoespaçada do sistema — **hoje e
depois deste commit**:

| caractere | onde | o que desenha |
|---|---|---|
| `▸` | `ui/index.html:234` | o prompt de composição |
| `◂` | `ui/index.html:228` | busca: ocorrência anterior |
| `▸` | `ui/index.html:230` | busca: próxima ocorrência |
| `▼` / `▶` | `ui/seele.js:299` | VoiceRoom aberto / fechado |
| `●` / `○` | `ui/seele.js:311` | pessoa falando / calado |
| `⌘` | `ui/seele.js:956` | a dica de cópia do convite |

Embarcar a face não conserta nada disto; só trocar o caractere, ou desenhar o
triângulo, conserta. E qual glifo a interface desenha é decisão de desenho — o
comp usa estes —, então fica com quem desenha, não com quem empacota a fonte.

### Noto Sans JP

```
--text='ゼレー三了京内動同告実市新期末本東査検橙源率端第終行警起部電青 '
```

Trinta e um glifos, que é o japonês inteiro do produto. Não é estimativa: o
repositório foi varrido inteiro por faixa Unicode (Hiragana, Katakana, CJK
Unificado). O varrimento devolveu **trinta e cinco**; quatro ficaram de fora, e
os dois casos são diferentes:

- `定` e `診` só existem em nota de trabalho (`.superpowers/`), não em produto.
- `発` e `令` formam `発令`, que `specs/07-tema-evangelion.md:67` lista como
  **fragmento aprovado** ao lado de `警告`, `同期率` e `第3新東京市` — mas que
  nenhuma tela usa. Se algum dia entrar numa tela, cai na Hiragino/Yu Gothic em
  silêncio, que é exatamente a falha que embarcar as faces existe para acabar.
  Dois glifos custariam algo perto de 400 bytes por peso.

7,6 KB por peso para 31 glifos é caro por glifo e barato no total: é o preço de
contorno CJK, e 15 KB pelos dois pesos não chega perto de justificar servir a
família inteira (4,7 MB por peso) nem de deixar o acento para a face que a
máquina tiver à mão.

## Tamanho

| arquivo | bytes |
|---|---|
| `saira-condensed-500.woff2` | 20 476 |
| `saira-condensed-700.woff2` | 20 600 |
| `saira-condensed-900.woff2` | 20 280 |
| `ibm-plex-mono-400.woff2` | 22 512 |
| `ibm-plex-mono-500.woff2` | 23 436 |
| `ibm-plex-mono-600.woff2` | 23 464 |
| `noto-sans-jp-700.woff2` | 7 600 |
| `noto-sans-jp-900.woff2` | 7 656 |
| **soma das faces** | **146 024** |
| licenças (3 arquivos) | 13 143 |

SHA-256 do que é servido:

```
07ae5e4d1fb0d82501d11b4aaff256d870f9f6f815455c62442927de04300f31  saira-condensed-500.woff2
336e94df8c3b99481a3d0dd9e09f46dfd11872bb5ed0c6de84fc82aa0f514a58  saira-condensed-700.woff2
6d32b0c617d96e279887ddce0ba72a7ce1324fec372c7b0cd08a4934df6f4c32  saira-condensed-900.woff2
0d5717a9356bdd3a80def42e110bb9848ce1cb03e06334ba89044025f399f091  ibm-plex-mono-400.woff2
e297ea941fa09127c597742254f496700bc17bb23794bbc2617f48e7faa0f3c8  ibm-plex-mono-500.woff2
393dca5c0ef0d22a64995f7e4e5caae0df630473993c51af147c51b5cc6015fb  ibm-plex-mono-600.woff2
4733c5c0a2f39e53ba99838bcbea21ccded48b5e267840e920341c007253e0d4  noto-sans-jp-700.woff2
b906a1dfda03d82d8b9ca8c37b9701aa264d035b49dbb890aa5ef4c5746a996b  noto-sans-jp-900.woff2
```

## Nome reservado

As três licenças trazem Reserved Font Name, e recortar é tecnicamente produzir
uma Modified Version. O nome interno foi mantido, que é o que o próprio Google
Fonts faz ao servir estas mesmas famílias recortadas por faixa. O que importa
para o CSS é outro nome: o `font-family` de um `@font-face` é rótulo local, e
`fontes.css` poderia chamá-las de qualquer coisa. Se algum dia isso incomodar,
o conserto é de uma linha por regra — e nenhuma face foi redesenhada, só teve
glifos removidos.
