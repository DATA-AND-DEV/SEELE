# Procedência das quatro faces

O ADR 0019 recusou npm para não carregar uma árvore de dependências que ninguém
auditou. Fonte que chega à mão obedece à mesma regra: cada arquivo aqui tem
origem nomeada, versão, licença lida e resumo conferível. Nada veio de CDN, e
nada veio de "o que o Google Fonts servia naquele dia".

A 0.19 trocou Saira Condensed e IBM Plex Mono (registradas nas versões
anteriores deste arquivo, no histórico do git) por Inter, Inter Tight e
JetBrains Mono. Noto Sans JP ficou: `base.css` ainda tem `.kanji` e `tokens.css`
ainda declara `--seele-jp`.

As quatro são **SIL Open Font License 1.1**. Não foi presumido: o texto de
licença de cada pacote baixado está ao lado, e o registro `name` ID 13 de cada
`.woff2` publicado repete a mesma frase.

## Inter — 400, 500, 600

| | |
|---|---|
| Origem | <https://github.com/rsms/inter/releases/tag/v4.1> |
| Release | `v4.1`, ativo `Inter-4.1.zip` |
| Versão da face | `Version 4.001;git-9221beed3` |
| Licença | SIL OFL 1.1 · `LICENCA-inter.txt` (o `LICENSE.txt` do pacote) |
| Direitos | Copyright 2016 The Inter Project Authors (<https://github.com/rsms/inter>) |

```
https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip
9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e  Inter-4.1.zip
```

As faces vieram dos `.woff2` estáticos do próprio pacote, `web/Inter-{Regular,Medium,SemiBold}.woff2`
(texto, não a variante `Display`):

```
e06f6b1bc553aaea4e4668023ed0ab0a147129c3107f511bc7d03d361b0ae085  web/Inter-Regular.woff2
0ff3e94614e1493eb556314fd247ae6c4a85a7783b4cc86be539940cf83f2a48  web/Inter-Medium.woff2
5cb7103e4e605989afebc03d989c79201e54b21b5183db33981f70db9178a301  web/Inter-SemiBold.woff2
262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a  LICENSE.txt
```

## Inter Tight — 600, 700

| | |
|---|---|
| Origem | <https://github.com/google/fonts/tree/6c1dbe27dcf7d817d4b62cec033ba77fa8e2a5ec/ofl/intertight> |
| Commit | `6c1dbe27dcf7d817d4b62cec033ba77fa8e2a5ec` do repositório `google/fonts` (`ofl/intertight`) |
| Fonte declarada | `METADATA.pb` do mesmo commit aponta <https://github.com/rsms/inter-gf-tight> no commit `c194f94c60b569b47876811321f5ef1f0c2614a2` |
| Versão da face | `Version 3.004` |
| Licença | SIL OFL 1.1 · `LICENCA-inter-tight.txt` |
| Direitos | Copyright 2022 The Inter Project Authors (<https://github.com/rsms/inter-tight>) |

O repositório de origem (`rsms/inter-gf-tight`) tem um único commit e nenhuma
*release*; o que há de nomeado é a versão `3.004` que o Google Fonts incorporou.
Por isso o download é por caminho fixo no commit do `google/fonts`, nunca por
`main`. O Google Fonts publica só a fonte **variável**; não existe `.woff2`
estático de Inter Tight a baixar, então os dois pesos foram instanciados (ver
Recorte).

```
https://raw.githubusercontent.com/google/fonts/6c1dbe27dcf7d817d4b62cec033ba77fa8e2a5ec/ofl/intertight/InterTight%5Bwght%5D.ttf
https://raw.githubusercontent.com/google/fonts/6c1dbe27dcf7d817d4b62cec033ba77fa8e2a5ec/ofl/intertight/OFL.txt
```

```
b81b73dcb64df3c230cabade7df6c5773bf863233f24c9ee51087519f1f88b6f  InterTight[wght].ttf
50240ab035cf1b6b3307940235481d515c4b6de3ab1fa843dbe59e7892cb9d58  OFL.txt
```

## JetBrains Mono — 400, 500, 700

| | |
|---|---|
| Origem | <https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304> |
| Release | `v2.304`, ativo `JetBrainsMono-2.304.zip` |
| Versão da face | `Version 2.304; ttfautohint (v1.8.4.7-5d5b)` |
| Licença | SIL OFL 1.1 · `LICENCA-jetbrains-mono.txt` (o `OFL.txt` do pacote) |
| Direitos | Copyright 2020 The JetBrains Mono Project Authors (<https://github.com/JetBrains/JetBrainsMono>) |

```
https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf  JetBrainsMono-2.304.zip
```

As faces vieram dos `.woff2` estáticos do pacote,
`fonts/webfonts/JetBrainsMono-{Regular,Medium,Bold}.woff2`:

```
a9cb1cd82332b23a47e3a1239d25d13c86d16c4220695e34b243effa999f45f2  fonts/webfonts/JetBrainsMono-Regular.woff2
086c48dfbea9ddaff1320f7e09399b8e2924e88ce67453721255db3bdbb5a353  fonts/webfonts/JetBrainsMono-Medium.woff2
c503cc5ec5f8b2c7666b7ecda1adf44bd45f2e6579b2eba0fc292150416588a2  fonts/webfonts/JetBrainsMono-Bold.woff2
30f0c136e3c88e422d0791acd97238870f9054a9729bc34cf2ff0d4ed8cac4ad  OFL.txt
```

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

`fonttools` 4.63.0 (com `brotli`). Cada face foi recortada para o que o produto
desenha, nunca para "o alfabeto todo por via das dúvidas". Comando comum a todas
as faces novas:

```
python -m fontTools.subset <origem> --output-file=<destino> --flavor=woff2 \
  --name-IDs=0,1,2,3,4,5,6,13,14 --layout-features='*' --desubroutinize \
  --unicodes='U+0020-007E,U+00A0-00FF,U+0100-017F,U+0394,U+03B1,U+2000-206F,U+2190-21FF,U+2212,U+2248,U+2260,U+2264-2265,U+2318,U+2500-257F,U+2580-259F,U+25A0-25FF'
```

`--name-IDs` preserva direitos (0), licença (13) e URL da licença (14): um
recorte que apaga a licença de dentro do arquivo perde a procedência que este
documento existe para guardar. `--layout-features='*'` mantém `tnum` em Inter e
Inter Tight (que `seele.css` usa em toda métrica); a JetBrains Mono não tem
`tnum` porque é monoespaçada, e todo algarismo já é tabular.

A faixa é a mesma para as três: Latino básico, Latin-1, Latin Extended-A
(quem escreve não é o produto: apelido, mensagem e endereço são texto de
terceiros), Pontuação Geral inteira (`U+2000-206F`), setas, traços de caixa,
blocos e formas geométricas. O resto saiu de leitura: `ui/*.html`, `ui/*.js` e
`ui/*.css` foram varridos caractere a caractere e o que apareceu fora do ASCII
foi `§ « ° ± µ · º » × ÷ Δ α – — ‖ • … ‹ › ← ↑ → ↓ ↻ − ≈ ≥ ⋯ ⌕ ⌘ ⌨ ⏻ ─ ┌ ┐ └ ┘ ├ ┤ ┬ █
░ ▒ ▤ ▶ ▸ ▼ ◂ ○ ◍ ● ◧ ⚙ ⚿ ⤓` mais os acentos do português e o kanji. O kanji é da
Noto Sans JP, abaixo.

**Nem tudo que a interface desenha existe nas faces** (conferido no `cmap` dos
arquivos baixados, não no recorte). Caem na fonte do sistema:

| face | caracteres ausentes da própria face |
|---|---|
| Inter e Inter Tight | `⋯ ⌕ ⌨ ⏻ ─ ┌ ┐ └ ┘ ├ ┤ ┬ █ ░ ▒ ▤ ▸ ◂ ◍ ◧ ⚙ ⚿` (Inter Tight também `⤓`) |
| JetBrains Mono | `↻ ⌕ ▤ ◍ ⚙ ⚿ ⤓` |

Os traços de caixa e blocos existem na JetBrains Mono, que é onde o comp os
desenha (terminal-safe). `⌕ ▤ ◍ ⚙ ⚿ ⤓` não existem em nenhuma das três: qual
glifo a interface desenha é decisão de desenho, não de empacotamento.

### Inter

Sem instanciação: os três `.woff2` estáticos foram só recortados. O nome interno
de 500 e 600 é `Inter Medium` / `Inter SemiBold` (como no pacote), irrelevante
para o CSS, que usa o rótulo do `@font-face`.

### Inter Tight

`InterTight[wght].ttf` (eixo `wght` 100–900) foi instanciado e depois recortado:

```
python -m fontTools.varLib.instancer "InterTight[wght].ttf" wght=600 -o InterTight-600.ttf
python -m fontTools.varLib.instancer "InterTight[wght].ttf" wght=700 -o InterTight-700.ttf
```

```
105d8676b06529c93acbb794fdf144a4e36cf14c8290964b7c585a02c28874a8  InterTight-600.ttf
acef01bb3f69abef5b2ffcb022e43726427d6e954090c78f215ca66186e01a77  InterTight-700.ttf
```

O instanciador fixa `OS/2.usWeightClass` (600, 700) mas **não reescreve o nome**:
os dois arquivos ainda dizem `Inter Tight` / `Regular` na tabela `name`. É só
rótulo interno; o peso real é o do `@font-face`.

### JetBrains Mono

Sem instanciação: três `.woff2` estáticos recortados.

### Noto Sans JP

```
--text='ゼレー三了京内動同告実市新期末本東査検橙源率端第終行警起部電青 '
```

Trinta e um glifos, que é o japonês inteiro do produto. Não é estimativa: o
repositório foi varrido inteiro por faixa Unicode (Hiragana, Katakana, CJK
Unificado). O varrimento devolveu **trinta e cinco**; quatro ficaram de fora, e
os dois casos são diferentes:

- `定` e `診` só existem em nota de trabalho (`.superpowers/`), não em produto.
- `発` e `令` formam `発令`, que `specs/07-estetica.md:67` lista como
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
| `inter-400.woff2` | 34 044 |
| `inter-500.woff2` | 34 748 |
| `inter-600.woff2` | 34 684 |
| `inter-tight-600.woff2` | 30 572 |
| `inter-tight-700.woff2` | 30 796 |
| `jetbrains-mono-400.woff2` | 44 736 |
| `jetbrains-mono-500.woff2` | 45 608 |
| `jetbrains-mono-700.woff2` | 46 124 |
| `noto-sans-jp-700.woff2` | 7 600 |
| `noto-sans-jp-900.woff2` | 7 656 |
| **soma das faces** | **316 568** |
| licenças (4 arquivos) | 17 463 |

SHA-256 do que é servido:

```
219546bbf0f86b0b617b8de97f957951f01384875bc782bb949df5f0ab0e79b5  inter-400.woff2
d005f6d5f53e50e6eac71dbda968bfabd9fdf0e4f959b2d2a903d1b111e4ac12  inter-500.woff2
81bfcbab8bad92d3fe1f8df97e8a1453bac743f0835f6b1f8c9295c6b9e63b34  inter-600.woff2
cc4cc66612cc0cea97c43078d56b3faa3038d64eb78aeef56b6e2847b29a9d55  inter-tight-600.woff2
3d0297ab5a8c83cedeadea6e1be4f62cdd249225a79ec98d6b98c7f5924ab73e  inter-tight-700.woff2
014fb3f5868e907b6f29066ac761edce0e9a374d82c79a11572d30b5fb2ddba9  jetbrains-mono-400.woff2
4bcc75d5d0f0622921d78e4f8b0cbd7e3a3f7f0c54f3badd5525c0e6174d5dc2  jetbrains-mono-500.woff2
641880b0fce87c6f335aa4380fed9af040789c2c46eec2fffab42f863e40857e  jetbrains-mono-700.woff2
4733c5c0a2f39e53ba99838bcbea21ccded48b5e267840e920341c007253e0d4  noto-sans-jp-700.woff2
b906a1dfda03d82d8b9ca8c37b9701aa264d035b49dbb890aa5ef4c5746a996b  noto-sans-jp-900.woff2
```

## Nome reservado

As licenças de Inter, Inter Tight e JetBrains Mono não declaram Reserved Font Name na linha de direitos (conferido); a da Noto Sans JP é a de antes. Recortar e instanciar é tecnicamente
produzir uma Modified Version, e o nome interno foi mantido. O que importa para o
CSS é outro nome: o `font-family` de um `@font-face` é rótulo local. Nenhuma face
foi redesenhada, só teve glifos removidos (e, na Inter Tight, o eixo fixado).
