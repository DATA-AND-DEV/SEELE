#!/usr/bin/env python3
"""A arquitetura híbrida sobre a casca de verdade: três cargas, três respostas.

**Descartável**, como o protótipo. O desenho do celular
(`docs/superpowers/specs/2026-10-09-0.16-celular-design.md`) aposta que a casca
do desktop pode ser reorganizada pelo CSS do celular, com a moldura nova em
`celular-*.js` acionando os controles que já existem, sem tocar no desktop. Esta
prova roda isso contra a `ui/` de hoje, por uma cópia (como
`tools/carga-da-casca.py`), e responde:

1. **O desktop não muda** com os arquivos do celular carregados: a sessão, sem a
   marca `data-casca`, é igual pixel a pixel à da `ui/` sem eles.
2. **A aba PESSOAS sai da lista do desktop**: `#lista-roster`, desenhada pelo
   `tela-sessao.js` de sempre, aparece na largura do celular.
3. **O MUDO da faixa de voz aciona o caminho de sempre**: o toque nele faz a
   casca mandar `set_muted` pelo mesmo `#botao-mudo`.

O gancho depois de `desenhar(snapshot)` é injetado **só na cópia**: ele é o
ponto de extensão que a casca pede à F5, e a prova o mostra funcionando antes de
ele existir.

Uso:
    python3 spikes/casca-do-celular/prova-sobre-a-casca/prova.py /tmp/prova
"""
import importlib.util
import pathlib
import shutil
import sys
import tempfile

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

RAIZ = pathlib.Path(__file__).resolve().parents[3]
AQUI = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("carga", RAIZ / "tools/carga-da-casca.py")
carga = importlib.util.module_from_spec(spec)
spec.loader.exec_module(carga)

GANCHO_ANTES = "function desenhar(snapshot) {\n  if (!snapshot) return;\n"
GANCHO_DEPOIS = (GANCHO_ANTES
                 + "  queueMicrotask(() => (globalThis.DEPOIS_DE_DESENHAR || []).forEach((f) => f(snapshot)));\n")
MARCA = '<script>document.documentElement.dataset.casca = "celular";</script>\n'
# O ponto de extensão da `index.html`: sem ele, o celular desenha a página a
# 980 px e a encolhe (medido nesta prova).
VIEWPORT = '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
SEM_VIEWPORT = "--sem-viewport" in sys.argv


def montar(destino: pathlib.Path, com_celular: bool, com_marca: bool) -> pathlib.Path:
    pasta = destino / "ui"
    shutil.copytree(RAIZ / "apps/seele-app/ui", pasta)
    pagina = pasta / "index.html"
    texto = pagina.read_text()
    if com_celular and not SEM_VIEWPORT:
        texto = texto.replace('<meta charset="utf-8">', '<meta charset="utf-8">\n' + VIEWPORT, 1)
    if com_celular:
        shutil.copy(AQUI / "celular-prova.css", pasta)
        shutil.copy(AQUI / "celular-prova.js", pasta)
        sessao = pasta / "tela-sessao.js"
        fonte = sessao.read_text()
        assert GANCHO_ANTES in fonte, "a forma de `desenhar` mudou; a prova precisa do começo dela"
        sessao.write_text(fonte.replace(GANCHO_ANTES, GANCHO_DEPOIS))
        texto = texto.replace('<link rel="stylesheet" href="acessibilidade.css">',
                              '<link rel="stylesheet" href="celular-prova.css">\n<link rel="stylesheet" href="acessibilidade.css">')
        texto = texto.replace("</body>", '<script src="celular-prova.js"></script>\n</body>')
    if com_marca:
        texto = texto.replace("<head>", "<head>\n" + MARCA, 1)
    corte = texto.index("<script src=")
    duble = (carga.DUBLE.replace("__APIS_ACEITAS__", carga.apis_aceitas())
             .replace("__VERSAO_DO_SERVIDOR__", carga.versao_do_servidor())
             .replace("__LINHA__", carga.linha_desta_janela()))
    pagina.write_text(texto[:corte] + duble + texto[corte:])
    return pagina


def entrar(pg):
    pg.wait_for_timeout(1200)
    pg.evaluate("conectar('127.0.0.1:8383', 'rafa', null)")
    pg.wait_for_timeout(1000)
    pg.evaluate("document.getElementById('auth-botao').click()")
    pg.wait_for_timeout(1500)
    pg.evaluate("document.getElementById('aviso-fone').hidden = true")


def main() -> int:
    saida = pathlib.Path(sys.argv[1])
    saida.mkdir(parents=True, exist_ok=True)
    falhas = []
    with tempfile.TemporaryDirectory() as t, sync_playwright() as p:
        nav = p.chromium.launch()
        fotos = {}
        for nome, com_celular in (("desktop-sem-celular", False), ("desktop-com-celular", True)):
            pagina = montar(pathlib.Path(t) / nome, com_celular, com_marca=False)
            ctx = nav.new_context(viewport={"width": 1440, "height": 900}, reduced_motion="reduce")
            pg = ctx.new_page()
            pg.goto(f"file://{pagina}")
            entrar(pg)
            fotos[nome] = saida / f"{nome}.png"
            pg.screenshot(path=fotos[nome], animations="disabled",
                          mask=[pg.locator("#barra-relogio"), pg.locator("#tel-uptime")])
            ctx.close()
        a = Image.open(fotos["desktop-sem-celular"]).convert("RGB")
        b = Image.open(fotos["desktop-com-celular"]).convert("RGB")
        caixa = ImageChops.difference(a, b).getbbox()
        print("1. o desktop com os arquivos do celular, sem a marca:",
              "IGUAL pixel a pixel" if caixa is None else f"DIFERENTE em {caixa}")
        if caixa is not None:
            falhas.append("o desktop mudou")

        pagina = montar(pathlib.Path(t) / "celular", True, com_marca=True)
        ctx = nav.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True)
        pg = ctx.new_page()
        erros = []
        pg.on("pageerror", lambda e: erros.append(str(e)[:200]))
        pg.goto(f"file://{pagina}")
        entrar(pg)
        pg.screenshot(path=saida / "celular-pessoas.png")
        linhas = pg.evaluate("document.querySelectorAll('#lista-roster .pessoa').length")
        largura = pg.evaluate("Math.round(document.getElementById('lista-roster').getBoundingClientRect().width)")
        print(f"2. a aba PESSOAS do celular: {linhas} pessoas da lista do desktop, {largura} px de largura")
        if linhas == 0 or largura > 390:
            falhas.append(f"a aba PESSOAS não cabe no aparelho: {largura} px, ou a lista do desktop não chegou")
        faixa = pg.evaluate("(() => { const v = document.getElementById('voz'); return v && !v.hidden ? v.textContent : null; })()")
        pg.evaluate("window.__SEELE_ARGS.set_muted = undefined")
        pg.locator("#voz-mudo").tap()
        pg.wait_for_timeout(300)
        mandou = pg.evaluate("window.__SEELE_ARGS.set_muted")
        print(f"3. a faixa de voz «{faixa}»; o toque no MUDO mandou set_muted {mandou}")
        if not mandou or mandou.get("on") is not True:
            falhas.append("o MUDO da faixa não chegou ao set_muted")
        if erros:
            falhas.append(f"erros: {erros}")
        ctx.close()
        nav.close()
    for f in falhas:
        print("REPROVA:", f)
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
