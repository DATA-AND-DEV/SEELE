#!/usr/bin/env python3
"""Gera os ícones do app a partir das duas formas da marca.

    python3 design/marca/gerar-icones.py

Ferramenta de humano, rodada quando a marca muda — **não** entra em build nem em
CI. O que o build usa são os arquivos que isto escreve, e eles estão versionados.

Só precisa de macOS: o `qlmanage` do sistema rasteriza SVG, e o `iconutil`
monta o `.icns`. Os SVGs de entrada já trazem os glifos em contorno, então não
há fonte para instalar nem para baixar.

# A regra de troca de forma

A folha de marca diz: abaixo de 32 px de largura do plug, trocar de forma — a
versão muda, sem o nome dentro. Não é preferência: `ゼーレ` a 27/162 da altura
vira três borrões antes de virar ilegível, e um ícone borrado num dock parece
software quebrado. Aqui isso é um `if`, e é a única decisão de design que este
arquivo toma sozinho.
"""

import re
import shutil
import struct
import subprocess
import zlib
import sys
import tempfile
from pathlib import Path

AQUI = Path(__file__).resolve().parent
RAIZ = AQUI.parent.parent
DESTINO = RAIZ / "apps" / "seele-app" / "icons"

# `docs/marca.md`: área livre ao redor de qualquer forma = metade da largura do
# plug. Com o plug de 88, isso dá 44 de cada lado e uma caixa de 176.
PLUG_L, PLUG_A = 88, 162
FOLGA = PLUG_L // 2
CAIXA = PLUG_L + 2 * FOLGA
NEGRO = "#050403"

# Abaixo disto o plug renderizado teria menos de 32 px de largura.
TROCA_DE_FORMA = 64


def corpo(nome: str) -> str:
    """O conteúdo de um dos SVGs da marca, sem o invólucro."""
    texto = (AQUI / nome).read_text(encoding="utf-8")
    return texto.split("<title>SEELE</title>", 1)[1].rsplit("</svg>", 1)[0]


REDUZIDA = corpo("reduzida.svg")
MUDA = corpo("muda.svg")


def cantos(png: bytes) -> dict[str, bytes]:
    """Os quatro pixels de canto de um PNG RGBA.

    Decodifica o mínimo: sem dependência para ler quatro pixels, e ler quatro
    pixels é o que separa "gerou um arquivo" de "gerou o ícone".
    """
    largura, altura = struct.unpack(">II", png[16:24])
    profundidade, tipo = png[24], png[25]
    if (profundidade, tipo) != (8, 6):
        raise SystemExit("esperava PNG RGBA de 8 bits")

    comprimido = b""
    passo = 8
    while passo < len(png):
        tamanho, = struct.unpack(">I", png[passo : passo + 4])
        if png[passo + 4 : passo + 8] == b"IDAT":
            comprimido += png[passo + 8 : passo + 8 + tamanho]
        passo += 12 + tamanho

    cru = zlib.decompress(comprimido)
    largura_linha = largura * 4
    anterior = bytearray(largura_linha)
    primeira = ultima = None
    passo = 0
    for indice in range(altura):
        filtro = cru[passo]
        passo += 1
        linha = bytearray(cru[passo : passo + largura_linha])
        passo += largura_linha
        for x in range(largura_linha):
            esquerda = linha[x - 4] if x >= 4 else 0
            acima = anterior[x]
            diagonal = anterior[x - 4] if x >= 4 else 0
            if filtro == 1:
                linha[x] = (linha[x] + esquerda) & 0xFF
            elif filtro == 2:
                linha[x] = (linha[x] + acima) & 0xFF
            elif filtro == 3:
                linha[x] = (linha[x] + (esquerda + acima) // 2) & 0xFF
            elif filtro == 4:
                p = esquerda + acima - diagonal
                pa, pb, pc = abs(p - esquerda), abs(p - acima), abs(p - diagonal)
                melhor = esquerda if (pa <= pb and pa <= pc) else (acima if pb <= pc else diagonal)
                linha[x] = (linha[x] + melhor) & 0xFF
        if indice == 0:
            primeira = bytes(linha)
        ultima = linha
        anterior = linha

    return {
        "superior esquerdo": primeira[:4],
        "superior direito": primeira[-4:],
        "inferior esquerdo": bytes(ultima[:4]),
        "inferior direito": bytes(ultima[-4:]),
    }


def quadrado_de(forma: str, lado: int) -> str:
    """A marca centrada num quadrado preto de `lado` píxeis.

    Quadrado sólido, não transparente. A folha de marca proíbe a marca sobre
    imagem, e um ícone com fundo transparente é exatamente uma marca sobre
    qualquer imagem que o sistema resolva pôr atrás.
    """
    forma = MUDA if forma == "muda" else REDUZIDA
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {CAIXA} {CAIXA}"'
        f' width="{lado}" height="{lado}">'
        f'<rect width="{CAIXA}" height="{CAIXA}" fill="{NEGRO}"/>'
        f'<g transform="translate({FOLGA} {(CAIXA - PLUG_A) / 2})">{forma}</g>'
        "</svg>"
    )


# Onde o `qlmanage` desenha a arte inteira em vez de encostá-la num canto.
#
# Medido, não escolhido: pedindo 128 ele devolve um PNG de 128×128 com a arte
# ocupando pouco mais da metade e o resto **branco** — as dimensões conferem e o
# conteúdo não, que é o tipo de falha que passa despercebida até alguém olhar o
# dock. A 1024 ele preenche. Então rasteriza-se uma vez em cima e reduz-se.
GRANDE = 1024


def rasterizar_grande(forma: str, trabalho: Path) -> Path:
    """Rasteriza uma das formas a `GRANDE` píxeis."""
    fonte = trabalho / f"{forma}.svg"
    fonte.write_text(quadrado_de(forma, GRANDE), encoding="utf-8")
    saida = trabalho / f"raster-{forma}"
    saida.mkdir(exist_ok=True)

    subprocess.run(
        ["qlmanage", "-t", "-s", str(GRANDE), "-o", str(saida), str(fonte)],
        check=True,
        capture_output=True,
    )
    pngs = list(saida.glob("*.png"))
    if not pngs:
        raise SystemExit(f"o qlmanage não produziu nada para «{forma}»")
    conferir(pngs[0], GRANDE)
    return pngs[0]


def reduzir(origem: Path, lado: int, trabalho: Path) -> bytes:
    """Reduz para `lado` píxeis com o `sips`."""
    alvo = trabalho / f"{origem.stem}-{lado}.png"
    shutil.copyfile(origem, alvo)
    subprocess.run(
        ["sips", "-z", str(lado), str(lado), str(alvo)],
        check=True,
        capture_output=True,
    )
    conferir(alvo, lado)
    return alvo.read_bytes()


def conferir(caminho: Path, lado: int) -> None:
    """As dimensões e o fundo.

    O fundo entra na conferência porque foi assim que a falha do `qlmanage`
    apareceu: tamanho certo, arte no canto, branco no resto.
    """
    bytes_ = caminho.read_bytes()
    largura, altura = struct.unpack(">II", bytes_[16:24])
    if (largura, altura) != (lado, lado):
        raise SystemExit(f"esperava {lado}×{lado} e veio {largura}×{altura}")

    esperado = tuple(int(NEGRO[i : i + 2], 16) for i in (1, 3, 5))
    for canto, pixel in cantos(bytes_).items():
        if tuple(pixel[:3]) != esperado:
            raise SystemExit(
                f"{caminho.name}: o canto {canto} é #{bytes(pixel[:3]).hex()}, "
                f"não o negro da marca. A arte não preencheu o quadro."
            )


def cartela() -> None:
    """A forma institucional, em PNG, para o README.

    Markdown no GitHub descarta estilo embutido, então a cartela só chega
    inteira como imagem. É a forma que a folha de marca pede para documento.

    Sai de um quadrado pelo mesmo motivo dos ícones — é o que o `qlmanage`
    desenha inteiro — e depois é recortada na faixa útil.
    """
    svg = (AQUI / "cartela.svg").read_text(encoding="utf-8")
    medida = re.search(r'viewBox="0 0 ([0-9.]+) ([0-9.]+)"', svg)
    if not medida:
        raise SystemExit("cartela.svg sem viewBox")
    largura, altura = float(medida.group(1)), float(medida.group(2))
    dentro = svg.split("<title>SEELE</title>", 1)[1].rsplit("</svg>", 1)[0]

    margem, escala = 20, 4
    lado = largura + margem * 2
    pixeis = int(lado * escala)

    with tempfile.TemporaryDirectory() as tmp:
        trabalho = Path(tmp)
        fonte = trabalho / "cartela.svg"
        fonte.write_text(
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {lado} {lado}"'
            f' width="{pixeis}" height="{pixeis}">'
            f'<rect width="{lado}" height="{lado}" fill="{NEGRO}"/>'
            f'<g transform="translate({margem} {(lado - altura) / 2:.3f})">{dentro}</g></svg>',
            encoding="utf-8",
        )
        subprocess.run(
            ["qlmanage", "-t", "-s", str(pixeis), "-o", str(trabalho), str(fonte)],
            check=True,
            capture_output=True,
        )
        bruto = next(p for p in trabalho.glob("*.png") if p != fonte)

        destino = RAIZ / "docs" / "imagens" / "marca-cartela.png"
        destino.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "sips",
                "-c",
                str(int((altura + margem * 2) * escala)),
                str(pixeis),
                str(bruto),
                "--out",
                str(destino),
            ],
            check=True,
            capture_output=True,
        )


def escrever_ico(caminho: Path, imagens: dict[int, bytes]) -> None:
    """Um `.ico` com PNGs dentro, que é o formato que o Windows lê desde o Vista.

    Escrito à mão porque a alternativa seria uma dependência inteira para
    concatenar seis arquivos atrás de um cabeçalho de seis campos.
    """
    lados = sorted(imagens)
    cabecalho = struct.pack("<HHH", 0, 1, len(lados))
    deslocamento = len(cabecalho) + 16 * len(lados)

    entradas, corpos = b"", b""
    for lado in lados:
        dados = imagens[lado]
        # 256 se escreve como 0 no campo de um byte. É o formato, não um erro.
        entradas += struct.pack(
            "<BBBBHHII",
            0 if lado >= 256 else lado,
            0 if lado >= 256 else lado,
            0,
            0,
            1,
            32,
            len(dados),
            deslocamento,
        )
        corpos += dados
        deslocamento += len(dados)

    caminho.write_bytes(cabecalho + entradas + corpos)


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit(
            "isto só roda no macOS: usa o qlmanage e o iconutil do sistema.\n"
            "Os ícones já estão versionados; só rode se a marca mudar."
        )

    DESTINO.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        trabalho = Path(tmp)

        grandes = {
            forma: rasterizar_grande(forma, trabalho) for forma in ("reduzida", "muda")
        }

        # O que o Tauri pede, mais o que o macOS e o Windows pedem por dentro.
        precisos = [16, 32, 48, 64, 128, 256, 512, 1024]
        imagens = {
            lado: reduzir(
                grandes["muda" if lado < TROCA_DE_FORMA else "reduzida"], lado, trabalho
            )
            for lado in precisos
        }

        (DESTINO / "32x32.png").write_bytes(imagens[32])
        (DESTINO / "128x128.png").write_bytes(imagens[128])
        (DESTINO / "128x128@2x.png").write_bytes(imagens[256])
        (DESTINO / "icon.png").write_bytes(imagens[512])

        escrever_ico(DESTINO / "icon.ico", {l: imagens[l] for l in (16, 32, 48, 64, 128, 256)})

        # `.icns` via iconutil, que quer um diretório com nomes exatos.
        conjunto = trabalho / "SEELE.iconset"
        conjunto.mkdir()
        for base in (16, 32, 128, 256, 512):
            (conjunto / f"icon_{base}x{base}.png").write_bytes(imagens[base])
            (conjunto / f"icon_{base}x{base}@2x.png").write_bytes(imagens[base * 2])
        subprocess.run(
            ["iconutil", "-c", "icns", str(conjunto), "-o", str(DESTINO / "icon.icns")],
            check=True,
        )

    # O app carrega a marca de dentro da própria pasta `ui/` — a CSP dele só
    # aceita `'self'`, e um caminho para fora de `ui/` não é servido. As cópias
    # são conferidas por teste contra estes originais, para não divergirem.
    for origem, nome in (("assinatura.svg", "marca-assinatura.svg"), ("muda.svg", "marca-muda.svg")):
        shutil.copyfile(AQUI / origem, RAIZ / "apps" / "seele-app" / "ui" / nome)

    cartela()

    for arquivo in sorted(DESTINO.iterdir()):
        print(f"  {arquivo.name:20} {arquivo.stat().st_size:>8} bytes")
    print("  ui/marca-assinatura.svg, ui/marca-muda.svg")


if __name__ == "__main__":
    main()
