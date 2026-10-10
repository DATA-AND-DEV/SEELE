"""Compara as fotos de duas rodadas do app, tela por tela.

Uso: python3 comparar.py [antes] [depois]   (os nomes das rodadas; o padrão é
antes e depois).

Mostra quanto difere e onde: as caixas das regiões diferentes (componentes
ligados numa grade de 16 px), em pixels da foto (retina, 2×). O relógio, o
UPTIME e o cursor de um campo são as diferenças esperadas; o resto é achado.
Grava, para cada tela com diferença, a foto das duas lado a lado com as caixas.
"""
import pathlib, sys
from PIL import Image, ImageChops, ImageDraw

AQUI = pathlib.Path(__file__).resolve().parent
GRADE = 16


def regioes(dif):
    w, h = dif.size
    px = dif.load()
    cel = set()
    for y in range(0, h, 2):
        for x in range(0, w, 2):
            if px[x, y] > 8:
                cel.add((x // GRADE, y // GRADE))
    caixas, vistos = [], set()
    for c in cel:
        if c in vistos:
            continue
        pilha, xs, ys = [c], [], []
        vistos.add(c)
        while pilha:
            cx, cy = pilha.pop()
            xs.append(cx); ys.append(cy)
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    v = (cx + dx, cy + dy)
                    if v in cel and v not in vistos:
                        vistos.add(v); pilha.append(v)
        caixas.append((min(xs) * GRADE, min(ys) * GRADE, (max(xs) + 1) * GRADE, (max(ys) + 1) * GRADE))
    return sorted(caixas, key=lambda c: (c[1], c[0]))


uma, outra = (sys.argv[1:3] + ["antes", "depois"][len(sys.argv[1:3]):])[:2]
for antes in sorted((AQUI / f"fotos-{uma}").glob("[0-9]-*.png")):
    depois = AQUI / f"fotos-{outra}" / antes.name
    if not depois.exists():
        print(f"{antes.stem}: sem a foto de depois")
        continue
    a, d = Image.open(antes).convert("RGB"), Image.open(depois).convert("RGB")
    if a.size != d.size:
        print(f"{antes.stem}: tamanhos diferentes {a.size} {d.size}")
        continue
    dif = ImageChops.difference(a, d).convert("L")
    n = sum(dif.point(lambda v: 255 if v > 8 else 0).histogram()[255:])
    caixas = regioes(dif)
    print(f"{antes.stem}: {100 * n / (a.size[0] * a.size[1]):.3f} % dos pixels diferem, em {len(caixas)} região(ões): {caixas}")
    if caixas:
        lado = Image.new("RGB", (a.size[0] * 2 + 20, a.size[1]), (255, 0, 0))
        for i, img in enumerate((a, d)):
            c = img.copy()
            desenho = ImageDraw.Draw(c)
            for caixa in caixas:
                desenho.rectangle(caixa, outline=(0, 255, 255), width=4)
            lado.paste(c, (i * (a.size[0] + 20), 0))
        lado.resize((lado.width // 3, lado.height // 3)).save(AQUI / f"dif-{outra}-{antes.stem}.png")
