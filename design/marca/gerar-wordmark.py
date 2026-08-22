#!/usr/bin/env python3
"""Redesenha os arquivos de marca que trazem letra, a partir da fonte embarcada.

    python3 design/marca/gerar-wordmark.py

Ferramenta de humano, como `gerar-icones.py`: roda quando a marca muda, e o que
o produto usa são os arquivos versionados que isto escreve.

# Por que a letra sai em contorno, e não em `<text>`

A razão velha morreu com o katakana: era não depender de uma Noto Sans JP
instalada. A razão nova é outra e continua obrigando o contorno.

Os SVGs da marca não são lidos dentro da página. `marca-muda.svg` é carregado
como favicon, `marca-simbolo.svg` como `<img>`, e os de ícone são rasterizados
pelo `qlmanage`. Nos três casos o SVG é um **documento isolado**: o `@font-face`
que `ui/fontes.css` declara não alcança lá dentro, e o `qlmanage` não tem folha
de estilo nenhuma. Um `<text font-family="Saira Condensed">` cairia no segundo
item da pilha — Arial Narrow, exatamente a falha silenciosa que `fontes.css`
descreve — ou na face de sistema do rasterizador.

Embutir a fonte em `data:` dentro de cada SVG resolveria e custa 20 KB por
arquivo, num favicon. Contorno custa o tamanho do desenho e não depende de nada.

# De onde vem o desenho da letra

`apps/seele-app/ui/fontes/saira-condensed-900.woff2` — a mesma face que o app
serve, então a marca e a interface são a mesma letra. Licença e procedência em
`ui/fontes/PROCEDENCIA.md`.

O tracking é aplicado aqui, em unidades de fonte (`0,06 em` = 60 de 1000), e não
por `letter-spacing`: em contorno não existe `letter-spacing`, existe a posição
de cada glifo. O mesmo vale para os `0,42 em` da tagline.

# A geometria do símbolo não sai daqui

`simbolo.svg` é a fonte de todas as formas e este arquivo só o copia. Editar o
símbolo é editar aquele arquivo.
"""

from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont

AQUI = Path(__file__).resolve().parent
RAIZ = AQUI.parent.parent
FONTE = RAIZ / "apps" / "seele-app" / "ui" / "fontes" / "saira-condensed-900.woff2"

LARANJA = "#F2521F"
NEGRO = "#050403"
OSSO = "#EAE3CF"
APAGADO = "#7A7061"

# `0,06 em` do wordmark e `0,42 em` da tagline, em unidades de uma fonte de
# 1000 por em.
TRACKING_NOME = 60
TRACKING_TAGLINE = 420

NOME = "SEELE"
TAGLINE = "DE MÁQUINA A MÁQUINA"
TAGLINE_L1 = "DE MÁQUINA"
TAGLINE_L2 = "A MÁQUINA"

_fonte = TTFont(FONTE)
_cmap = _fonte.getBestCmap()
_glifos = _fonte.getGlyphSet()
_hmtx = _fonte["hmtx"]
CAP = _fonte["OS/2"].sCapHeight


def _numero(valor: float) -> str:
    return ("%.4f" % valor).rstrip("0").rstrip(".") or "0"


def _tinta(nome: str) -> tuple[float, float]:
    limites = BoundsPen(_glifos)
    _glifos[nome].draw(limites)
    return limites.bounds[0], limites.bounds[2]


def medir(texto: str, tracking: int) -> tuple[list[tuple[float, str]], float, float]:
    """Glifos posicionados, largura de tinta e onde a tinta começa.

    Largura de **tinta**, não de avanço: uma marca se alinha pelo que se vê, e o
    avanço traz a lateral do último glifo e o tracking pendurado depois dele.
    """
    x = 0.0
    postos: list[tuple[float, str]] = []
    esquerda = direita = None
    for caractere in texto:
        glifo = _cmap[ord(caractere)]
        avanco, _ = _hmtx[glifo]
        caneta = SVGPathPen(_glifos, ntos=_numero)
        _glifos[glifo].draw(caneta)
        desenho = caneta.getCommands()
        if desenho:
            postos.append((x, desenho))
            menor, maior = _tinta(glifo)
            esquerda = x + menor if esquerda is None else min(esquerda, x + menor)
            direita = x + maior if direita is None else max(direita, x + maior)
        x += avanco + tracking
    return postos, direita - esquerda, esquerda


def largura(texto: str, tracking: int, altura_de_caixa: float) -> float:
    _, tinta, _ = medir(texto, tracking)
    return tinta * altura_de_caixa / CAP


def escrever(texto, tracking, altura_de_caixa, x_da_tinta, linha_de_base, cor, recuo="  "):
    """O texto como um `<g>` de contornos, ancorado pela borda esquerda da tinta.

    A escala inverte o Y porque a fonte cresce para cima e o SVG para baixo.
    """
    postos, _, esquerda = medir(texto, tracking)
    k = altura_de_caixa / CAP
    tx = x_da_tinta - esquerda * k
    linhas = [
        f'{recuo}<g fill="{cor}" transform="translate({_numero(tx)} '
        f'{_numero(linha_de_base)}) scale({k:.8g} -{k:.8g})">'
    ]
    for x, desenho in postos:
        deslocamento = f' transform="translate({_numero(x)} 0)"' if x else ""
        linhas.append(f'{recuo}  <path{deslocamento} d="{desenho}"/>')
    linhas.append(f"{recuo}</g>")
    return "\n".join(linhas)


def simbolo(cor_dos_nos: str, cor_do_enlace: str, recuo: str = "  ") -> str:
    """As três formas de `simbolo.svg`, na cor que o suporte pede.

    Ordem: o enlace primeiro, os nós por cima. É o que esconde as pontas da
    diagonal dentro dos nós — sem isso a linha aparece pingando para fora do nó
    cheio, e a folha manda que ela não os toque.
    """
    return "\n".join(
        [
            f'{recuo}<path d="M34 34L62 62" stroke="{cor_do_enlace}" stroke-width="4"/>',
            f'{recuo}<rect x="12" y="12" width="24" height="24" fill="{cor_dos_nos}"/>',
            f'{recuo}<rect x="62" y="62" width="20" height="20" fill="none"'
            f' stroke="{cor_dos_nos}" stroke-width="4"/>',
        ]
    )


# ---- as medidas das composições -------------------------------------------
#
# Grade de 96, como o símbolo. O respiro de um nó cheio (24) é o que separa o
# símbolo do wordmark e o que sobra em volta do conjunto; o símbolo já traz 12
# de respiro dentro do próprio quadro, então a margem externa é esses 12.
CAIXA_NOME = 48  # altura de caixa alta do wordmark, metade do quadro do símbolo
RESPIRO = 24
MARGEM = 12
CAIXA_TAGLINE = 10


def assinatura(com_tagline: bool) -> str:
    nome_x = 84 + RESPIRO  # a borda do nó vazio mais o respiro de um nó cheio
    nome_largura = largura(NOME, TRACKING_NOME, CAIXA_NOME)
    larg = round(nome_x + nome_largura + MARGEM)
    alt = 96
    corpo = [simbolo(LARANJA, OSSO), escrever(NOME, TRACKING_NOME, CAIXA_NOME, nome_x, 72, OSSO)]
    if com_tagline:
        base = 96 + MARGEM + CAIXA_TAGLINE
        alt = round(base + MARGEM)
        corpo.append(escrever(TAGLINE, TRACKING_TAGLINE, CAIXA_TAGLINE, MARGEM, base, APAGADO))
    return montar(
        larg,
        alt,
        "SEELE",
        corpo,
        comentario(
            "assinatura" + (" com tagline" if com_tagline else ""),
            [
                "Símbolo mais o wordmark SEELE em Saira Condensed 900, tracking 0,06 em,",
                "separados pelo respiro de um nó cheio. A caixa alta do wordmark é 48 —",
                "metade do quadro do símbolo — e a linha de base cai em 72, que põe o",
                "centro óptico da palavra no centro do símbolo.",
                "",
                "O respiro à direita fecha em 11,8 e não em 12: a largura do wordmark é",
                "de tipo, não de grade, e é melhor arredondar a caixa que a letra.",
            ]
            + (
                [
                    "",
                    "A tagline é opcional e só entra a partir de 48 px de símbolo — abaixo",
                    "disso a caixa alta dela não chega a 5 px e o tracking de 0,42 em a",
                    "espalha até virar textura. Alinha pela esquerda do símbolo, não pela",
                    "do wordmark: por baixo do conjunto inteiro ela é uma régua, e por",
                    "baixo só do wordmark seria uma terceira coluna.",
                ]
                if com_tagline
                else []
            ),
        ),
    )


def empilhada() -> str:
    nome_largura = largura(NOME, TRACKING_NOME, CAIXA_NOME)
    larg = round(nome_largura + 2 * MARGEM)
    base = 84 + RESPIRO + CAIXA_NOME
    alt = round(base + MARGEM)
    simbolo_x = (larg - 96) / 2
    return montar(
        larg,
        alt,
        "SEELE",
        [
            f'  <g transform="translate({_numero(simbolo_x)} 0)">',
            simbolo(LARANJA, OSSO, recuo="    "),
            "  </g>",
            escrever(NOME, TRACKING_NOME, CAIXA_NOME, (larg - nome_largura) / 2, base, OSSO),
        ],
        comentario(
            "empilhada",
            [
                "Símbolo em cima, wordmark embaixo, os dois centrados na mesma largura.",
                "Para suporte alto e estreito, onde a assinatura deitada não cabe.",
                "",
                "O respiro entre os dois conta da borda do símbolo (84), não da borda do",
                "quadro: o quadro já tem 12 de folga dentro dele, e medir pelo quadro",
                "somaria duas folgas e afastaria o nome do desenho.",
            ],
        ),
    )


def mono() -> str:
    nome_x = 84 + RESPIRO
    nome_largura = largura(NOME, TRACKING_NOME, CAIXA_NOME)
    larg = round(nome_x + nome_largura + MARGEM)
    return montar(
        larg,
        96,
        "SEELE",
        [simbolo(OSSO, OSSO), escrever(NOME, TRACKING_NOME, CAIXA_NOME, nome_x, 72, OSSO)],
        comentario(
            "assinatura em uma cor",
            [
                "Para onde não há duas: gravação, carimbo, bordado, fax, e qualquer",
                "suporte que herde a cor de fora.",
                "",
                "O laranja vira osso, e não o contrário. O nó vazio continua vazio: é o",
                "furo que separa quem chega de quem hospeda, e sem cor ele é a única",
                "coisa que ainda os separa.",
            ],
        ),
    )


def cartela() -> str:
    """A forma institucional: campo cheio à esquerda, nome e tagline à direita."""
    larg, alt = 264, 96
    campo = 96  # a largura do campo laranja, um quadro de símbolo
    escala = 92 / 96
    texto_x = campo + RESPIRO
    return montar(
        larg,
        alt,
        "SEELE",
        [
            f'  <rect x="1" y="1" width="{larg - 2}" height="{alt - 2}" fill="none"'
            f' stroke="{LARANJA}" stroke-width="2"/>',
            f'  <rect x="2" y="2" width="{campo - 2}" height="{alt - 4}" fill="{LARANJA}"/>',
            f'  <g transform="translate(3 2) scale({escala:.8g})">',
            simbolo(NEGRO, NEGRO, recuo="    "),
            "  </g>",
            escrever(NOME, TRACKING_NOME, 36, texto_x, 49, OSSO),
            escrever(TAGLINE_L1, TRACKING_TAGLINE, 8, texto_x, 69, APAGADO),
            escrever(TAGLINE_L2, TRACKING_TAGLINE, 8, texto_x, 83, APAGADO),
        ],
        comentario(
            "forma institucional (cartela)",
            [
                "Fora da interface: README, changelog, rodapé, compartilhamento. Dentro",
                "do produto quem aparece é o símbolo.",
                "",
                "O campo cheio à esquerda inverte o símbolo — negro sobre laranja — e o",
                "campo vazio à direita traz o nome. A assimetria dos dois campos é a",
                "marca; a moldura não é.",
                "",
                "A tagline vem em duas linhas porque em uma só, com 0,42 em de tracking,",
                "ela passaria da largura do nome e mandaria na composição.",
                "",
                "Mínimo: 180 px de largura.",
            ],
        ),
    )


def comentario(titulo: str, linhas: list[str]) -> str:
    corpo = "\n".join(("     " + linha).rstrip() for linha in linhas)
    return (
        f"<!-- SEELE · {titulo}. Gerado por `design/marca/gerar-wordmark.py` a partir\n"
        f"     de `simbolo.svg` e da Saira Condensed 900 embarcada. NÃO editar à mão.\n"
        f"\n{corpo} -->"
    )


def montar(largura_: float, altura: float, titulo: str, corpo: list[str], cabecalho: str) -> str:
    dentro = "\n".join(corpo)
    return (
        f"{cabecalho}\n"
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {_numero(largura_)}'
        f' {_numero(altura)}" width="{_numero(largura_)}" height="{_numero(altura)}"'
        f' role="img" aria-label="SEELE">\n'
        f"  <title>{titulo}</title>\n"
        f"{dentro}\n"
        f"</svg>\n"
    )


def main() -> None:
    saida = {
        "assinatura.svg": assinatura(False),
        "assinatura-tagline.svg": assinatura(True),
        "empilhada.svg": empilhada(),
        "mono.svg": mono(),
        "cartela.svg": cartela(),
    }
    for nome, conteudo in saida.items():
        (AQUI / nome).write_text(conteudo, encoding="utf-8")
        print(f"  {nome:24} {len(conteudo):>6} bytes")


if __name__ == "__main__":
    main()
