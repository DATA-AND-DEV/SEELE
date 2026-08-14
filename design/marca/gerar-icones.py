#!/usr/bin/env python3
"""Gera os ícones do app a partir dos desenhos de ícone da marca.

    python3 design/marca/gerar-icones.py

Ferramenta de humano, rodada quando a marca muda — **não** entra em build nem em
CI. O que o build usa são os arquivos que isto escreve, e eles estão versionados.

Só precisa de macOS: o `qlmanage` do sistema rasteriza SVG, e o `iconutil`
monta o `.icns`. Os SVGs de entrada já trazem os glifos em contorno, então não
há fonte para instalar nem para baixar.

# Um desenho por faixa, e não a redução do maior

`design/marca/icone-app-16.svg`, `-32`, `-64` e `-128` são quatro desenhos, não
quatro tamanhos do mesmo desenho: o traço engrossa conforme o orçamento de pixel
encolhe, e as placas de profundidade vão sumindo até sobrar a silhueta. Cada
tamanho pedido sai do arquivo da sua faixa. Uma redução do maior devolveria um
traço de meio pixel a 16, que é como um ícone fica cinza no dock.

A faixa de 48 sai do desenho de 32: a folha de marca fecha a faixa de uma placa
em 64, e 48 cai dentro dela.

# Duas famílias, por causa do que cada sistema faz com o quadro

- **macOS** (`icon.icns`): placa laranja recortada em superelipse, transparente
  fora dela. O macOS não recorta ícone de aplicativo — entrega o quadro como
  está — e um quadrado sangrado fica visivelmente maior e mais duro que todo
  vizinho no Dock. É a exceção nomeada da regra 1 em `docs/marca.md`.
- **Windows e Linux** (`icon.ico` e os PNGs): fundo transparente e a construção
  solta, tudo laranja e nada preto. Não é a arte de placa com a placa retirada:
  sem a placa atrás, o contorno preto e a cinta preta somem numa barra de
  tarefas escura e sobra um anel laranja oco, que não é um plug. É a exceção
  nomeada da regra 6.

A regra que separa as duas é mecânica: no desenho de ícone, o que está pintado
de negro é a marca, e o que está nas cores de placa é profundidade que só
significa alguma coisa sobre a placa. A família transparente fica com o negro,
repintado de laranja, e descarta o resto.

# Por que se rasteriza duas vezes

O `qlmanage` achata transparência contra **branco**: pedindo um SVG de fundo
vazio ele devolve um PNG opaco. Então rasteriza-se a mesma arte sobre branco e
sobre preto e extrai-se o alfa dos dois: com `Cb = a·C` e `Cw = a·C + (1−a)·255`,
sai `a = 1 − (Cw − Cb)/255` e `C = Cb/a`. Medido: um retângulo a 50% devolveu
`a = 0,502` e `C = #F1521F`. A composição do renderizador é em espaço de
dispositivo, que é o que torna a conta exata.

# Por que se rasteriza a 1024 e reduz

Medido, não escolhido: pedindo 200 o `qlmanage` devolve um PNG de 200×200 com a
arte ocupando pouco mais da metade e o resto **branco** — as dimensões conferem
e o conteúdo não, que é o tipo de falha que passa despercebida até alguém olhar
o dock. A 1024 ele preenche. Então rasteriza-se uma vez em cima e reduz-se.

A redução é média de área feita aqui, e não mais o `sips`. Medido também: o
filtro do `sips` toca o alfa perto da borda da placa, e a 16 px um pixel que
está inteiro dentro da placa voltava com alfa 240. Um filtro de lóbulo negativo
também inventa cor nas bordas, e a marca é cor plana. Média de área devolve
sempre uma combinação das cores que já estavam lá, e um bloco inteiramente
opaco volta opaco — que é o que a conferência exige.
"""

import math
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

NEGRO = "#050403"
LARANJA = "#F2521F"

# Onde o `qlmanage` desenha a arte inteira em vez de encostá-la num canto.
GRANDE = 1024

# A grade de ícone da Apple: num quadro de 1024, o quadrado arredondado tem 824
# e sobra 100 de folga de cada lado. A curva é a superelipse
# |x/a|^n + |y/b|^n = 1 com n = 5 — não é um `border-radius`, e é justamente por
# não ser que o ícone assenta no Dock ao lado dos do sistema.
PLACA = 824
FOLGA = (GRANDE - PLACA) // 2
EXPOENTE = 5
AMOSTRAS = 240

# Cada tamanho pedido e o desenho de que ele sai.
FAIXA = {16: 16, 32: 32, 48: 32, 64: 64, 128: 128, 256: 128, 512: 128, 1024: 128}

ATRIBUTO = re.compile(r'([\w:-]+)="([^"]*)"')
ELEMENTO = re.compile(r"<(?:polygon|rect)\b[^>]*?/?>")


def bloco(faixa: int) -> tuple[str, str]:
    """As duas construções da faixa, em coordenadas do bloco quadrado.

    Devolve (com placa, solta). O arquivo de origem põe a marca num `<svg>`
    interno com `x`, `y`, `width`, `height` e `viewBox` próprios; aqui isso vira
    um `transform` equivalente. É de propósito: `<svg>` aninhado depende de
    `overflow="visible"` para não cortar traço grosso, e um `transform` não
    depende de nada.
    """
    texto = (AQUI / f"icone-app-{faixa}.svg").read_text(encoding="utf-8")
    _, _, interno = texto.split("<svg", 2)
    atributos, _, resto = interno.partition(">")
    filhos = resto.split("</svg>", 1)[0]

    a = dict(ATRIBUTO.findall(atributos))
    x, y = float(a["x"]), float(a["y"])
    largura, altura = float(a["width"]), float(a["height"])
    cx, cy, cl, ca = (float(v) for v in a["viewBox"].split())

    # `preserveAspectRatio` padrão: xMidYMid meet.
    escala = min(largura / cl, altura / ca)
    tx = x + (largura - cl * escala) / 2 - cx * escala
    ty = y + (altura - ca * escala) / 2 - cy * escala
    posto = f'<g transform="translate({tx:.6g} {ty:.6g}) scale({escala:.6g})">%s</g>'

    return posto % filhos, posto % soltar(filhos)


def soltar(filhos: str) -> str:
    """A mesma marca sem placa nenhuma: tudo laranja, nada preto.

    Fica só o que estava pintado de negro — o contorno do plug e a cinta —,
    repintado de laranja. As placas de profundidade saem inteiras: elas são cor
    plana deslocada sobre a placa, e sem a placa atrás não deslocam nada.
    """
    soltos = []
    for elemento in ELEMENTO.findall(filhos):
        if NEGRO not in elemento:
            continue
        # A ordem importa: o preenchimento laranja do contorno existe só para
        # vazar a placa de trás, e vira `none` antes de o negro virar laranja.
        solto = elemento.rstrip().rstrip(">").rstrip("/")
        solto = solto.replace(f'fill="{LARANJA}"', 'fill="none"')
        solto = solto.replace(f'fill="{NEGRO}"', f'fill="{LARANJA}"')
        solto = solto.replace(f'stroke="{NEGRO}"', f'stroke="{LARANJA}"')
        soltos.append(solto + "/>")
    return "".join(soltos)


def superelipse() -> str:
    """A placa do macOS como caminho fechado, amostrada ponto a ponto.

    Caminho, e não um `rect` com `clip-path`: o recorte teria de sobreviver ao
    `qlmanage`, e não há motivo para apostar nisso quando a curva se escreve.
    """
    raio = PLACA / 2
    centro = GRANDE / 2
    pontos = []
    for indice in range(AMOSTRAS):
        angulo = 2 * math.pi * indice / AMOSTRAS
        cosseno, seno = math.cos(angulo), math.sin(angulo)
        pontos.append(
            "%.3f,%.3f"
            % (
                centro + math.copysign(raio * abs(cosseno) ** (2 / EXPOENTE), cosseno),
                centro + math.copysign(raio * abs(seno) ** (2 / EXPOENTE), seno),
            )
        )
    return "M" + "L".join(pontos) + "Z"


def arte(faixa: int, com_placa: bool) -> str:
    """O conteúdo do SVG de `GRANDE` píxeis, sem fundo."""
    com, solta = bloco(faixa)
    if com_placa:
        # A marca escala com a placa de 824, não com o quadro de 1024: escalada
        # pelo quadro ela transborda a placa.
        return (
            f'<path d="{superelipse()}" fill="{LARANJA}"/>'
            f'<g transform="translate({FOLGA} {FOLGA}) scale({PLACA / faixa:.6g})">{com}</g>'
        )
    # Sem placa não há de que se afastar, e o alvo transparente ganha o quadro
    # inteiro — que é a proporção que a barra de tarefas espera.
    return f'<g transform="scale({GRANDE / faixa:.6g})">{solta}</g>'


def pixeis(png: bytes) -> tuple[int, list[bytes]]:
    """Um PNG RGBA de 8 bits em linhas cruas.

    Decodifica o mínimo: sem dependência para ler alguns pixels, e ler alguns
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
    linhas = []
    passo = 0
    for _ in range(altura):
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
        linhas.append(bytes(linha))
        anterior = linha

    return largura, linhas


def escrever_png(linhas: list[bytes], largura: int) -> bytes:
    """PNG RGBA sem filtro. Escrito à mão pelo motivo de sempre: é pouco."""

    def parte(tipo: bytes, dados: bytes) -> bytes:
        return (
            struct.pack(">I", len(dados))
            + tipo
            + dados
            + struct.pack(">I", zlib.crc32(tipo + dados) & 0xFFFFFFFF)
        )

    cabecalho = struct.pack(">IIBBBBB", largura, len(linhas), 8, 6, 0, 0, 0)
    cru = b"".join(b"\x00" + bytes(linha) for linha in linhas)
    return (
        b"\x89PNG\r\n\x1a\n"
        + parte(b"IHDR", cabecalho)
        + parte(b"IDAT", zlib.compress(cru, 9))
        + parte(b"IEND", b"")
    )


def destacar(brancas: list[bytes], pretas: list[bytes]) -> list[bytes]:
    """Recupera alfa e cor das duas rasterizações. Ver o cabeçalho do arquivo."""
    saida = []
    for branca, preta in zip(brancas, pretas):
        linha = bytearray(len(preta))
        for i in range(0, len(preta), 4):
            diferenca = (
                (branca[i] - preta[i])
                + (branca[i + 1] - preta[i + 1])
                + (branca[i + 2] - preta[i + 2])
            ) // 3
            alfa = max(0, min(255, 255 - diferenca))
            if alfa:
                for canal in range(3):
                    linha[i + canal] = min(255, round(preta[i + canal] * 255 / alfa))
                linha[i + 3] = alfa
        saida.append(bytes(linha))
    return saida


def reduzir(linhas: list[bytes], largura: int, lado: int) -> list[bytes]:
    """Média de área de `largura` para `lado`, com o alfa pré-multiplicado.

    Pré-multiplicado porque a média direta de cor sobre pixel transparente
    puxaria a borda para o que estivesse guardado no canal de cor de um pixel
    invisível — que é como uma marca ganha uma franja escura.
    """
    limites = [largura * i // lado for i in range(lado + 1)]
    saida = []
    for j in range(lado):
        linha = bytearray(lado * 4)
        faixa_de_linhas = linhas[limites[j] : limites[j + 1]]
        for i in range(lado):
            inicio, fim = limites[i] * 4, limites[i + 1] * 4
            soma_r = soma_g = soma_b = soma_a = 0
            for origem in faixa_de_linhas:
                for p in range(inicio, fim, 4):
                    alfa = origem[p + 3]
                    soma_a += alfa
                    soma_r += origem[p] * alfa
                    soma_g += origem[p + 1] * alfa
                    soma_b += origem[p + 2] * alfa
            if soma_a:
                quantos = (limites[j + 1] - limites[j]) * (limites[i + 1] - limites[i])
                linha[i * 4] = min(255, round(soma_r / soma_a))
                linha[i * 4 + 1] = min(255, round(soma_g / soma_a))
                linha[i * 4 + 2] = min(255, round(soma_b / soma_a))
                linha[i * 4 + 3] = min(255, round(soma_a / quantos))
        saida.append(bytes(linha))
    return saida


def rasterizar(faixa: int, com_placa: bool, trabalho: Path) -> list[bytes]:
    """Rasteriza a arte a `GRANDE` píxeis, com alfa de verdade."""
    nome = f"{'placa' if com_placa else 'solta'}-{faixa}"
    conteudo = arte(faixa, com_placa)
    camadas = {}
    for fundo in ("#FFFFFF", "#000000"):
        fonte = trabalho / f"{nome}-{fundo[1:]}.svg"
        fonte.write_text(
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {GRANDE} {GRANDE}"'
            f' width="{GRANDE}" height="{GRANDE}">'
            f'<rect width="{GRANDE}" height="{GRANDE}" fill="{fundo}"/>{conteudo}</svg>',
            encoding="utf-8",
        )
        saida = trabalho / f"raster-{nome}-{fundo[1:]}"
        saida.mkdir()
        subprocess.run(
            ["qlmanage", "-t", "-s", str(GRANDE), "-o", str(saida), str(fonte)],
            check=True,
            capture_output=True,
        )
        pngs = list(saida.glob("*.png"))
        if not pngs:
            raise SystemExit(f"o qlmanage não produziu nada para «{nome}»")
        largura, linhas = pixeis(pngs[0].read_bytes())
        if (largura, len(linhas)) != (GRANDE, GRANDE):
            raise SystemExit(f"esperava {GRANDE}² e veio {largura}×{len(linhas)}")
        camadas[fundo] = linhas

    return destacar(camadas["#FFFFFF"], camadas["#000000"])


def conferir(png: bytes, nome: str, lado: int, com_placa: bool) -> None:
    """As dimensões e alguns pixels escolhidos.

    O fundo entra na conferência porque foi assim que a falha do `qlmanage`
    apareceu: tamanho certo, arte no canto, branco no resto. Com fundo
    transparente os quatro cantos deixaram de bastar — canto vazio é o que a
    falha e o acerto têm em comum —, então a conferência de canto ganhou um
    par de sondas dentro do quadro, e elas diferem por família:

    - **Com placa**: os cantos vazios, e a 15% e a 85% da largura, na meia
      altura, o laranja exato da placa. As duas sondas prendem a extensão da
      placa dos dois lados, então uma arte encolhida no canto reprova. A sonda
      do meio cai sobre a cinta e só se exige que seja escura: a 16 px a cinta
      tem 1,7 pixel de largura e nenhum pixel dela é negro puro.
    - **Sem placa**: os cantos vazios e o pixel do meio no laranja exato,
      opaco. Ali não há placa para mascarar nada — se a arte não desenhou, ou
      desenhou encolhida, o meio vem transparente.
    """
    largura, linhas = pixeis(png)
    if (largura, len(linhas)) != (lado, lado):
        raise SystemExit(f"esperava {lado}×{lado} e veio {largura}×{len(linhas)}")

    def px(x: int, y: int) -> tuple[int, ...]:
        return tuple(linhas[y][x * 4 : x * 4 + 4])

    def em(fx: float, fy: float) -> tuple[int, ...]:
        return px(int(fx * lado), int(fy * lado))

    laranja = tuple(int(LARANJA[i : i + 2], 16) for i in (1, 3, 5))
    fim = lado - 1
    cantos = {
        "superior esquerdo": px(0, 0),
        "superior direito": px(fim, 0),
        "inferior esquerdo": px(0, fim),
        "inferior direito": px(fim, fim),
    }
    for canto, pixel in cantos.items():
        if pixel[3] != 0:
            raise SystemExit(
                f"{nome}: o canto {canto} tem alfa {pixel[3]} em vez de 0. "
                "O fundo do ícone não é transparente."
            )

    meio = em(0.5, 0.5)
    if com_placa:
        for lugar, fx in (("esquerda", 0.15), ("direita", 0.85)):
            pixel = em(fx, 0.5)
            if pixel[:3] != laranja or pixel[3] != 255:
                raise SystemExit(
                    f"{nome}: a {lugar} da placa é "
                    f"#{bytes(pixel[:3]).hex()} com alfa {pixel[3]}, não o laranja "
                    "opaco da marca. A placa não preencheu o quadro."
                )
        if sum(meio[:3]) > sum(laranja) // 2:
            raise SystemExit(
                f"{nome}: o meio é #{bytes(meio[:3]).hex()}, claro demais "
                "para ser a cinta. A marca não desenhou sobre a placa."
            )
    elif meio[:3] != laranja or meio[3] != 255:
        raise SystemExit(
            f"{nome}: o meio é #{bytes(meio[:3]).hex()} com alfa {meio[3]}, "
            "não o laranja opaco da marca. A marca não preencheu o quadro."
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
    pixeis_lado = int(lado * escala)

    with tempfile.TemporaryDirectory() as tmp:
        trabalho = Path(tmp)
        fonte = trabalho / "cartela.svg"
        fonte.write_text(
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {lado} {lado}"'
            f' width="{pixeis_lado}" height="{pixeis_lado}">'
            f'<rect width="{lado}" height="{lado}" fill="{NEGRO}"/>'
            f'<g transform="translate({margem} {(lado - altura) / 2:.3f})">{dentro}</g></svg>',
            encoding="utf-8",
        )
        subprocess.run(
            ["qlmanage", "-t", "-s", str(pixeis_lado), "-o", str(trabalho), str(fonte)],
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
                str(pixeis_lado),
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


def familia(com_placa: bool, lados: list[int], trabalho: Path) -> dict[int, bytes]:
    """Um PNG por tamanho pedido, cada um saído do desenho da sua faixa."""
    familia_ = "placa" if com_placa else "solta"
    grandes = {}
    for faixa in sorted({FAIXA[lado] for lado in lados}):
        linhas = rasterizar(faixa, com_placa, trabalho)
        conferir(escrever_png(linhas, GRANDE), f"{familia_} {faixa} a {GRANDE}", GRANDE, com_placa)
        grandes[faixa] = linhas

    imagens = {}
    for lado in lados:
        linhas = grandes[FAIXA[lado]]
        png = escrever_png(linhas if lado == GRANDE else reduzir(linhas, GRANDE, lado), lado)
        conferir(png, f"{familia_} {lado}", lado, com_placa)
        imagens[lado] = png
    return imagens


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit(
            "isto só roda no macOS: usa o qlmanage e o iconutil do sistema.\n"
            "Os ícones já estão versionados; só rode se a marca mudar."
        )

    DESTINO.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        trabalho = Path(tmp)

        # O que o macOS pede por dentro do `.icns`, e o que o Tauri, o Windows e
        # o Linux pedem em PNG e em `.ico`.
        placa = familia(True, [16, 32, 64, 128, 256, 512, 1024], trabalho)
        solta = familia(False, [16, 32, 48, 64, 128, 256, 512], trabalho)

        (DESTINO / "32x32.png").write_bytes(solta[32])
        (DESTINO / "128x128.png").write_bytes(solta[128])
        (DESTINO / "128x128@2x.png").write_bytes(solta[256])
        (DESTINO / "icon.png").write_bytes(solta[512])

        escrever_ico(DESTINO / "icon.ico", {l: solta[l] for l in (16, 32, 48, 64, 128, 256)})

        # `.icns` via iconutil, que quer um diretório com nomes exatos.
        conjunto = trabalho / "SEELE.iconset"
        conjunto.mkdir()
        for base in (16, 32, 128, 256, 512):
            (conjunto / f"icon_{base}x{base}.png").write_bytes(placa[base])
            (conjunto / f"icon_{base}x{base}@2x.png").write_bytes(placa[base * 2])
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
