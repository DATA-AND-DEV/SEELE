#!/usr/bin/env python3
"""Desenha a marca do SEELE num `.bmp`, para a lombada do instalador do Windows.

# Por que gerada, e não exportada à mão

O NSIS só sabe desenhar `.bmp`, e um `.bmp` é opaco: ele não tem canal alfa. Uma
marca exportada de um `.png` com transparência chega com o fundo que o exportador
achou — quase sempre branco ou preto puro — e fica com uma auréola clara em volta
de cada traço, sobre um painel que é `#0A0806`. A auréola só aparece na tela de
quem instala, que é onde ninguém está olhando quando o pacote sai.

Gerando aqui, o fundo é o mesmo token do painel e não há o que compor.

E gerada **do mesmo desenho** que a marca do produto: os números abaixo são os do
`marca-simbolo.svg` e do `docs/marca.md` — dois quadrados e uma ligação, o cheio
é quem hospeda, o vazio é quem chega, a diagonal é o enlace.

# Por que sem biblioteca

`Pillow` não está na árvore e um instalador não é motivo para pô-la lá. O que este
arquivo precisa — três formas, uma suavização e um cabeçalho de 54 bytes — cabe em
Python puro, e assim ele roda em qualquer máquina que já roda o resto do
empacotamento.

# Uso

    python3 empacotar/marca-do-instalador.py apps/seele-app/marca-instalador.bmp
"""

import pathlib
import sys

# ---- o desenho, em coordenadas do `viewBox` de 96×96 do SVG ----
LADO_SVG = 96.0
DIAGONAL = ((34.0, 34.0), (62.0, 62.0))
DIAGONAL_ESPESSURA = 5.0
QUADRADO_CHEIO = (12.0, 12.0, 24.0)          # x, y, lado
QUADRADO_VAZIO = (62.0, 62.0, 20.0)          # x, y, lado
QUADRADO_VAZIO_ESPESSURA = 5.0

# ---- as cores, de `apps/seele-app/ui/tokens.css` ----
FUNDO = (0x0A, 0x08, 0x06)      # --seele-negro-painel
OSSO = (0xEA, 0xE3, 0xCF)       # --seele-osso
LARANJA = (0xF2, 0x52, 0x1F)    # --seele-laranja-nerv

# Quatro amostras por eixo, dezesseis por pixel. O bastante para a diagonal não
# sair serrilhada num traço de 5/96 do lado, e barato: o desenho todo é uma
# imagem de 52 pixels.
AMOSTRAS = 4


def na_diagonal(x: float, y: float) -> bool:
    """A ligação entre os dois nós, com a espessura do traço."""
    (x1, y1), (x2, y2) = DIAGONAL
    dx, dy = x2 - x1, y2 - y1
    comprimento = (dx * dx + dy * dy) ** 0.5
    # Projeção do ponto no segmento, presa às pontas — traço com ponta reta, que
    # é o `stroke-linecap` padrão do SVG.
    t = ((x - x1) * dx + (y - y1) * dy) / (comprimento * comprimento)
    t = max(0.0, min(1.0, t))
    px, py = x1 + t * dx, y1 + t * dy
    return ((x - px) ** 2 + (y - py) ** 2) ** 0.5 <= DIAGONAL_ESPESSURA / 2.0


def no_quadrado_cheio(x: float, y: float) -> bool:
    qx, qy, lado = QUADRADO_CHEIO
    return qx <= x < qx + lado and qy <= y < qy + lado


def no_quadrado_vazio(x: float, y: float) -> bool:
    """A moldura do quadrado vazio.

    O traço do SVG é centrado na linha, então ele sai metade para fora e metade
    para dentro — é por isso que o `fora` cresce e o `dentro` encolhe pela mesma
    metade. Desenhar a moldura só para dentro engordaria o vazio e deixaria os
    dois quadrados de tamanhos diferentes na tela.
    """
    qx, qy, lado = QUADRADO_VAZIO
    meio = QUADRADO_VAZIO_ESPESSURA / 2.0
    dentro_de = lambda ex: (qx - ex <= x <= qx + lado + ex) and (qy - ex <= y <= qy + lado + ex)
    return dentro_de(meio) and not dentro_de(-meio)


def cor_em(x: float, y: float) -> tuple[int, int, int] | None:
    """A cor de um ponto, na ordem de pintura do SVG: a ligação, depois os nós."""
    if no_quadrado_cheio(x, y) or no_quadrado_vazio(x, y):
        return LARANJA
    if na_diagonal(x, y):
        return OSSO
    return None


def desenhar(lado: int) -> list[list[tuple[int, int, int]]]:
    escala = LADO_SVG / lado
    passo = escala / AMOSTRAS
    linhas = []
    for py in range(lado):
        linha = []
        for px in range(lado):
            somas = [0, 0, 0]
            for sy in range(AMOSTRAS):
                for sx in range(AMOSTRAS):
                    x = (px * AMOSTRAS + sx) * passo + passo / 2
                    y = (py * AMOSTRAS + sy) * passo + passo / 2
                    cor = cor_em(x, y) or FUNDO
                    for canal in range(3):
                        somas[canal] += cor[canal]
            total = AMOSTRAS * AMOSTRAS
            linha.append(tuple(soma // total for soma in somas))
        linhas.append(linha)
    return linhas


def escrever_bmp(destino: pathlib.Path, pixels: list[list[tuple[int, int, int]]]) -> None:
    """Um BMP de 24 bits, sem compressão.

    De baixo para cima e com cada linha alinhada em 4 bytes, que é o formato que
    o NSIS lê. As duas coisas são exigência do formato, não escolha: uma imagem
    de cabeça para baixo e uma linha desalinhada são os dois jeitos clássicos de
    um BMP escrito à mão sair torto.
    """
    altura, largura = len(pixels), len(pixels[0])
    sobra = (4 - (largura * 3) % 4) % 4
    corpo = bytearray()
    for linha in reversed(pixels):
        for vermelho, verde, azul in linha:
            corpo += bytes((azul, verde, vermelho))  # BMP guarda BGR
        corpo += bytes(sobra)

    inicio_dos_dados = 54
    cabecalho = bytearray(b"BM")
    cabecalho += (inicio_dos_dados + len(corpo)).to_bytes(4, "little")
    cabecalho += (0).to_bytes(4, "little")
    cabecalho += inicio_dos_dados.to_bytes(4, "little")
    cabecalho += (40).to_bytes(4, "little")          # tamanho do BITMAPINFOHEADER
    cabecalho += largura.to_bytes(4, "little", signed=True)
    cabecalho += altura.to_bytes(4, "little", signed=True)
    cabecalho += (1).to_bytes(2, "little")           # planos
    cabecalho += (24).to_bytes(2, "little")          # bits por pixel
    cabecalho += (0).to_bytes(4, "little")           # sem compressão
    cabecalho += len(corpo).to_bytes(4, "little")
    cabecalho += (2835).to_bytes(4, "little")        # 72 dpi, em pixels por metro
    cabecalho += (2835).to_bytes(4, "little")
    cabecalho += (0).to_bytes(4, "little")           # cores na paleta
    cabecalho += (0).to_bytes(4, "little")           # cores importantes

    destino.write_bytes(bytes(cabecalho) + bytes(corpo))


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    destino = pathlib.Path(sys.argv[1])
    # 52 pixels: o tamanho que o desenho do instalador dá à marca da lombada.
    escrever_bmp(destino, desenhar(52))
    print(f"{destino}: {destino.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
