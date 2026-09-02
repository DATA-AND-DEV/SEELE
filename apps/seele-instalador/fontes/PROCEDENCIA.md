# Procedência das faces do instalador

As mesmas três famílias do produto, das mesmas origens fixas, recortadas do mesmo
jeito — e **em `.ttf`/`.otf` e não em `.woff2`**, que é a única diferença.

## Por que outro formato, e não os arquivos que já estão na árvore

O `apps/seele-app/ui/fontes/` serve a casca HTML, e ali `woff2` é o formato
certo: comprime melhor e é o que o navegador lê.

O instalador não tem navegador. Ele desenha em GDI, e a única forma de usar uma
face sem instalá-la no sistema de quem instala é `AddFontMemResourceEx`, que lê
`.ttf` e `.otf` e **não lê `woff2`** — o `woff2` é um invólucro com compressão
Brotli que só o navegador desempacota.

A alternativa seria cair na condensada que o Windows tiver à mão, que é o que a
alternativa do `--seele-display` já declara (`Arial Narrow`). Foi recusada: a
primeira tela do produto é onde a identidade menos deveria ser aproximada, e
carregar da memória não deixa rastro nenhum na máquina de quem instala — nada é
instalado, nada sobra depois.

## Origem

Idênticas às do produto, com as URLs fixas por commit e as somas conferidas
contra `apps/seele-app/ui/fontes/PROCEDENCIA.md` **antes** do recorte. Bateram as
três:

| origem baixada | SHA-256 conferido |
|---|---|
| `SairaCondensed-Medium.ttf` | `9aced533b166bdeb432764f9231a58bcaf4b4db14ba4c525d0af8f8663c08c75` |
| `SairaCondensed-Bold.ttf` | `647c3a2f6183d1d4908c0edf8fff4f5e0c4a1854f2303f4d93f1cc3dd2a1c0d3` |
| `ibm-plex-mono.zip` | `6d23f01257663d8cc49a0d64c22ced630b79e0e2a0ac08a0da86e9a38bbc481c` |

A Saira veio de
`https://raw.githubusercontent.com/Omnibus-Type/Saira/1916f2a575479b626238d9842126e63aa208eebf/Saira/fonts/ttf/`;
o Plex, da release `@ibm/plex-mono@2.5.0`, de onde saiu
`ibm-plex-mono/fonts/complete/otf/IBMPlexMono-Regular.otf`.

Ambas são **SIL Open Font License 1.1**, e o texto de cada uma está ao lado.

## Recorte

`fonttools` 4.63.0 — a mesma versão que recortou as do produto. O comando é o
mesmo, sem o `--flavor=woff2`, que é justamente o que muda o formato de saída:

```
python3 -m fontTools.subset <origem> --output-file=<destino> \
  --name-IDs=0,1,2,3,4,5,6,13,14 --layout-features='*' --desubroutinize \
  --unicodes='U+0020-007E,U+00A0-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2039-203A,U+2212'
```

A faixa é a da Saira do produto, e vale para as três aqui: **o instalador nunca
desenha texto de terceiros.** Tudo o que ele escreve é texto do produto, em
português — títulos, rótulos, o caminho da pasta e o nome dos arquivos que
copia. Não há apelido, mensagem nem nada digitado por quem instala.

`--name-IDs` preserva direitos (0), licença (13) e URL da licença (14): um
recorte que apaga a licença de dentro do arquivo perde a procedência que este
documento existe para guardar.

## O que está aqui

| arquivo | bytes | onde desenha |
|---|---|---|
| `saira-condensed-700.ttf` | 49 504 | cartela: `SEELE`, títulos de passo |
| `saira-condensed-500.ttf` | 49 004 | rótulos em caixa alta |
| `ibm-plex-mono-400.otf` | 31 220 | todo o resto: prosa, caminho, medidas |

```
05879572d4addf393a124d781462ca7ec27763461072fce6ba6e6da6eb967a59  saira-condensed-700.ttf
0de07ed5e8e232956fbfa34eea24a9c8281c3698e9379ce185f1832c66b48ec4  saira-condensed-500.ttf
b03ada708beb7a85941559f52743df21c2a0cc8d55721453d06592cd0e118c09  ibm-plex-mono-400.otf
```

## Nome reservado

Vale aqui o que vale no produto: recortar é produzir uma Modified Version, o
nome interno foi mantido, e nenhuma face foi redesenhada — só teve glifos
removidos. Ver a seção equivalente em `apps/seele-app/ui/fontes/PROCEDENCIA.md`.
