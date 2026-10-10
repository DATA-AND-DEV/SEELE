"""As duas telas da medida do teto do quadro (idr-no-teto.md), em I420 cru.

    python3 telas.py <pasta>     # escreve codigo.yuv e foto.yuv, 1920×1680

Sem captura da tela de quem roda: o código é renderizado com a Menlo de 13 px,
e a foto é sintética (luz suave, contornos e grão de σ 10).
"""
import sys, random, math, pathlib
from PIL import Image, ImageDraw, ImageFont

d = pathlib.Path(sys.argv[1]); random.seed(7)
L, A = 1920, 1080 + 600

def i420(img, nome):
    img = img.convert("YCbCr"); y, cb, cr = img.split()
    cb = cb.resize((L // 2, A // 2)); cr = cr.resize((L // 2, A // 2))
    (d / nome).write_bytes(y.tobytes() + cb.tobytes() + cr.tobytes())

fonte = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 13)
img = Image.new("RGB", (L, A), (255, 255, 255)); g = ImageDraw.Draw(img)
tokens = ["fn", "let", "mut", "self", "match", "Some(x)", "=>", "return", "if", "else", "for", "in", "impl", "pub", "struct", "&mut", "Vec<u8>", "Ok(())", "Err(e)", "{", "}", "(", ")", ";", ".iter()", ".map(|v|", "as", "usize", "String::new()", "// comentário de código", "assert_eq!", "0x1f", "123", "\"texto\""]
cores = [(0, 0, 0), (170, 13, 145), (28, 0, 207), (196, 26, 22), (0, 116, 0), (63, 110, 116)]
for coluna in (0, 960):
    for linha in range(0, A, 18):
        if random.random() < 0.15: continue
        x = coluna + 8 + 4 * 8 * random.randint(0, 4)
        while x < coluna + 940:
            t = random.choice(tokens); g.text((x, linha), t, font=fonte, fill=random.choice(cores)); x += 8 * (len(t) + 1)
            if random.random() < 0.08: break
i420(img, "codigo.yuv")

img = Image.new("RGB", (L, A)); px = img.load()
for yy in range(A):
    for xx in range(L):
        b = 128 + 60 * math.sin(xx / 97.0) * math.cos(yy / 71.0) + 40 * math.sin((xx + yy) / 23.0)
        r = b + 30 * math.sin(xx / 13.0); gg = b + 20 * math.cos(yy / 17.0); bl = b - 25
        n = random.gauss(0, 10)
        px[xx, yy] = (max(0, min(255, int(r + n))), max(0, min(255, int(gg + n))), max(0, min(255, int(bl + n))))
i420(img, "foto.yuv")
