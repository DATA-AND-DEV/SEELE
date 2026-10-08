#!/usr/bin/env python3
"""Monta em `origem/` a página do spike para uma origem estática (medições 4 a 6
do ADR 0057): os três arquivos da página, o manifest e os ícones. O dono sobe a
pasta inteira para a origem provisória, na raiz ou num subcaminho.

    python3 empacotar_origem.py

A página lê o servidor do fragmento do link que o binário imprime com
`--origem`. Nada aqui escolhe o domínio (D-web-2). Descartável.
"""
import json, pathlib, shutil
from PIL import Image, ImageDraw

AQUI = pathlib.Path(__file__).resolve().parent
SAIDA = AQUI / "origem"
shutil.rmtree(SAIDA, ignore_errors=True)
SAIDA.mkdir()
for nome in ("index.html", "pagina.js", "processadores.js"):
    shutil.copy(AQUI / "pagina" / nome, SAIDA / nome)
(SAIDA / "manifest.webmanifest").write_text(json.dumps({
    "name": "SEELE · medição de voz",
    "short_name": "SEELE voz",
    # Sem fragmento: o ADR 0057 quer o `start_url` limpo. Que URL o iOS grava
    # no ícone é justamente uma das perguntas da medição 4.
    "start_url": "./",
    "display": "standalone",
    "background_color": "#141413",
    "theme_color": "#141413",
    "icons": [{"src": f"icone-{n}.png", "sizes": f"{n}x{n}", "type": "image/png"} for n in (192, 512)],
}, ensure_ascii=False, indent=1))
for n in (180, 192, 512):
    img = Image.new("RGB", (n, n), "#141413")
    d = ImageDraw.Draw(img)
    m = n // 5
    d.ellipse((m, m, n - m, n - m), outline="#ecebe6", width=max(2, n // 24))
    img.save(SAIDA / f"icone-{n}.png")
print(f"pronto: {SAIDA} ({', '.join(sorted(p.name for p in SAIDA.iterdir()))})")
