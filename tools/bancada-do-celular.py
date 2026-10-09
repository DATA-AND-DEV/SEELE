#!/usr/bin/env python3
"""A bancada do celular: a casca de verdade num navegador, com o que um celular reprova.

**Por que isto existe.** Os guardas de `apps/seele-app/tests/celular.rs` leem os
`celular-*` como texto: cada regra começa pela marca, cada script é inerte sem
ela, nenhum chama o `invoke`. Nenhum deles **desenha** a casca, e as duas
promessas que mais importam só se veem desenhadas:

- **o desktop não muda** (G3): com os arquivos do celular carregados e sem a
  marca, a sessão do desktop é igual pixel a pixel à da `ui/` sem eles. A
  comparação roda no Chromium (o motor do WebView2 do Windows) e no WebKit (o
  substituto do WKWebView do Mac e do WebKitGTK do Linux), cada motor contra
  ele mesmo;
- **o celular cabe no aparelho**: a 390 × 844 e a 360 × 800, nada rola de lado
  fora de um trilho marcado, todo controle tem 44 de alvo, nada fica na ilha nem
  na barra de gestos, nenhum texto passa da caixa, todo texto tem contraste, e o
  núcleo (MUDO, SURDO, SAIR…) tem a mesma cor com e sem o tema do servidor.

As medidas são as do protótipo (`spikes/casca-do-celular/fotos.py`), portadas
para a casca de verdade. O spec é
`docs/superpowers/specs/2026-10-09-0.16-celular-design.md`, §9.

**A cópia.** Como `tools/carga-da-casca.py`, a bancada copia a `ui/` para uma
pasta temporária e põe o duble do Tauri na cópia. Os pontos de extensão que
ainda não estão no produto (spec, §9.4) são injetados só na cópia, pela tabela
`PONTOS_INJETADOS`. Quando um deles entra no produto, a bancada reprova até a
injeção dele sair da tabela, no mesmo commit.

Uso:
    python3 tools/bancada-do-celular.py /tmp/bancada
    python3 tools/bancada-do-celular.py /tmp/bancada --so sessao-entrou
    python3 tools/bancada-do-celular.py /tmp/bancada --so g3

Precisa do Playwright com o Chromium e o WebKit (`python3 -m pip install
playwright` e `python3 -m playwright install chromium webkit`) e do Pillow. Sai
com 1 se algo reprovar.
"""

import argparse
import dataclasses
import importlib.util
import json
import pathlib
import re
import shutil
import sys
import tempfile
from typing import Callable

from PIL import Image, ImageChops
from playwright.sync_api import Page, sync_playwright

RAIZ = pathlib.Path(__file__).resolve().parent.parent
UI = RAIZ / "apps" / "seele-app" / "ui"

_spec = importlib.util.spec_from_file_location("carga", RAIZ / "tools" / "carga-da-casca.py")
carga = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(carga)

VIEWPORT = '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
MARCA = '<script>document.documentElement.dataset.casca = "{}";</script>\n'


@dataclasses.dataclass(frozen=True)
class Ponto:
    """Um ponto de extensão do spec (§9.4) que ainda não está no produto."""

    nome: str
    arquivo: str
    # O texto do produto depois do qual o ponto entra; `None` é o fim do arquivo.
    ancora: str | None
    injetado: str
    # O que, achado no arquivo do produto, quer dizer que o ponto já entrou.
    sinal: str


PONTOS_INJETADOS = [
    # Sem a meta de viewport, o celular desenha a página a 980 px e a encolhe
    # (medido na prova da Etapa A).
    Ponto("P1", "index.html", '<meta charset="utf-8">\n', VIEWPORT, 'name="viewport"'),
    Ponto(
        "P5",
        "base.js",
        None,
        "\nconst CASCA = Object.seal({ depoisDeDesenhar: [], "
        'pedirSenhaDaSala: async () => window.prompt("Senha da sala"), '
        "enterEnvia: true, avisoDeFone: () => AVISO_DE_FONE });\n",
        "const CASCA",
    ),
    # No produto, o P6 é a última linha de `desenhar`. Aqui ele entra no começo,
    # adiado para depois dela por um microtask: o efeito é o mesmo, e a âncora é
    # estável.
    Ponto(
        "P6",
        "tela-sessao.js",
        "function desenhar(snapshot) {\n  if (!snapshot) return;\n",
        "  queueMicrotask(() => CASCA.depoisDeDesenhar.forEach((f) => f(snapshot)));\n",
        "CASCA.depoisDeDesenhar",
    ),
]

# As áreas seguras de um iPhone em retrato (a ilha em cima, a barra de gestos
# embaixo) e de um Android comum, com a barra de navegação por gestos.
APARELHOS = {
    "390": {"largura": 390, "altura": 844, "topo": 59, "pe": 34},
    "360": {"largura": 360, "altura": 800, "topo": 24, "pe": 24},
}

# Os temas de teste, aplicados pelo produto (`temaDosMods`), e não à mão. São os
# do protótipo: PAPEL e PROFUNDO são os do ESTILO, o claro inverte o fundo, e o
# hostil é o caso do §8 do spec, com o acento igual ao fundo. No hostil mede-se
# só o núcleo: a API 6 o recusa pelos pares, e a bancada o aplica por baixo dela
# para ver que o núcleo aguenta mesmo assim.
TEMAS = {
    "papel": {"fundo": "#12100e", "painel": "#1b1815", "texto": "#f2ece0", "apagado": "#a2988a",
              "acento": "#c9a227", "borda": "#4a423a", "densidade": "confortavel", "fonte": "sans",
              "arredondamento": 4, "brilho": False},
    "profundo": {"fundo": "#03060b", "painel": "#081019", "texto": "#dbe7f2", "apagado": "#7e94ab",
                 "acento": "#38bdf8", "borda": "#1f3347", "densidade": "compacta", "fonte": "mono",
                 "arredondamento": 8, "brilho": True},
    "hostil": {"fundo": "#050403", "painel": "#0a0806", "texto": "#eae3cf", "apagado": "#908574",
               "acento": "#050403", "borda": "#241f19", "densidade": "compacta", "fonte": "mono",
               "arredondamento": 0, "brilho": False},
    "claro": {"fundo": "#f2ece0", "painel": "#e6dfcf", "texto": "#12100e", "apagado": "#574e43",
              "acento": "#a83a10", "borda": "#b8ad9a", "densidade": "confortavel", "fonte": "mono",
              "arredondamento": 4, "brilho": False},
}


def montar(destino: pathlib.Path, *, celular: bool, marca: str | None) -> pathlib.Path:
    """Uma cópia da `ui/` com o duble, e com ou sem os arquivos das cascas do navegador.

    `celular=False` é o desktop de antes do celular: sem nenhum `celular-*` nem
    `larga-*`, sem as linhas que os carregam e sem os pontos injetados. É o
    termo de comparação do G3.
    """
    pasta = destino / "ui"
    shutil.copytree(UI, pasta)
    pagina = pasta / "index.html"
    if celular:
        for ponto in PONTOS_INJETADOS:
            arquivo = pasta / ponto.arquivo
            texto = arquivo.read_text()
            if ponto.sinal in texto:
                raise SystemExit(
                    f"o {ponto.nome} já está no produto ({ponto.arquivo}): tire a injeção dele de "
                    "PONTOS_INJETADOS, no mesmo commit"
                )
            if ponto.ancora is None:
                texto += ponto.injetado
            else:
                if texto.count(ponto.ancora) != 1:
                    raise SystemExit(f"a âncora do {ponto.nome} mudou em {ponto.arquivo}; a bancada precisa dela")
                texto = texto.replace(ponto.ancora, ponto.ancora + ponto.injetado)
            arquivo.write_text(texto)
    else:
        for arquivo in pasta.iterdir():
            if arquivo.name.startswith(("celular-", "larga-")):
                arquivo.unlink()
        texto = re.sub(r'<link rel="stylesheet" href="(?:celular|larga)-[^"]+">\n', "", pagina.read_text())
        texto = re.sub(r'<script src="(?:celular|larga)-[^"]+"></script>\n', "", texto)
        assert "celular-" not in texto and "larga-" not in texto, "sobrou uma casca do navegador na cópia do desktop"
        pagina.write_text(texto)

    texto = pagina.read_text()
    if marca:
        texto = texto.replace("<head>", "<head>\n" + MARCA.format(marca), 1)
    corte = texto.index("<script src=")
    duble = (
        carga.DUBLE.replace("__APIS_ACEITAS__", carga.apis_aceitas())
        .replace("__VERSAO_DO_SERVIDOR__", carga.versao_do_servidor())
        .replace("__LINHA__", carga.linha_desta_janela())
    )
    pagina.write_text(texto[:corte] + duble + texto[corte:])
    return pagina


def entrar(pg: Page) -> None:
    """Da carga à sessão, pelo link, como o celular entra: sem HOSPEDAR AQUI."""
    pg.wait_for_function("typeof conectar === 'function'")
    pg.wait_for_timeout(600)
    pg.evaluate("conectar('127.0.0.1:8383', 'rafa', null)")
    pg.wait_for_function("!document.getElementById('auth-botao').hidden || !document.getElementById('tela-sessao').hidden")
    pg.evaluate("(() => { const b = document.getElementById('auth-botao'); if (b && !b.hidden) b.click(); })()")
    pg.wait_for_function("!document.getElementById('tela-sessao').hidden")
    pg.wait_for_timeout(1200)
    # O aviso de fone é um aviso de uma vez, e a foto é da sessão.
    pg.evaluate("document.getElementById('aviso-fone').hidden = true")


def g3(p, saida: pathlib.Path, temporaria: pathlib.Path) -> list[str]:
    """O desktop igual pixel a pixel com os arquivos das cascas do navegador."""
    falhas = []
    for motor in (p.chromium, p.webkit):
        navegador = motor.launch()
        fotos = {}
        for nome, celular in (("desktop-sem-celular", False), ("desktop-com-celular", True)):
            pagina = montar(temporaria / motor.name / nome, celular=celular, marca=None)
            ctx = navegador.new_context(viewport={"width": 1440, "height": 900}, reduced_motion="reduce")
            pg = ctx.new_page()
            erros = []
            pg.on("pageerror", lambda e: erros.append(str(e)[:200]))
            pg.goto(f"file://{pagina}")
            entrar(pg)
            fotos[nome] = saida / f"g3-{nome}-{motor.name}.png"
            pg.screenshot(path=fotos[nome], animations="disabled",
                          mask=[pg.locator("#barra-relogio"), pg.locator("#tel-uptime")])
            if celular and pg.evaluate("typeof CELULAR !== 'undefined'"):
                falhas.append(f"G3 ({motor.name}): sem a marca, um script do celular rodou e deixou CELULAR")
            if erros:
                falhas.append(f"G3 ({motor.name}, {nome}): erros {erros}")
            ctx.close()
        navegador.close()
        a = Image.open(fotos["desktop-sem-celular"]).convert("RGB")
        b = Image.open(fotos["desktop-com-celular"]).convert("RGB")
        caixa = ImageChops.difference(a, b).getbbox()
        print(f"G3 ({motor.name}): o desktop com os arquivos do celular, sem a marca:",
              "IGUAL pixel a pixel" if caixa is None else f"DIFERENTE em {caixa}")
        if caixa is not None:
            falhas.append(f"G3 ({motor.name}): o desktop mudou em {caixa}")
    return falhas


# --------------------------------------------------------------- as medidas

MEDIR = r"""
([topo, pe]) => {
  const W = innerWidth, H = innerHeight;
  const vis = (e) => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && !e.closest("[hidden]"); };
  const nome = (e) => (e.id ? "#" + e.id : e.tagName.toLowerCase() + (typeof e.className === "string" && e.className ? "." + e.className.trim().split(/\s+/)[0] : ""))
    + " «" + (e.getAttribute("aria-label") || e.textContent || e.placeholder || "").trim().replace(/\s+/g, " ").slice(0, 28) + "»";
  // Só um trilho marcado rola de lado: um painel com `overflow-y: auto` tem
  // `overflow-x: auto` computado também, e contar isso como trilho escondia
  // todo transbordo dentro de um painel que rola.
  const rolaX = (e) => !!e.parentElement?.closest("[data-rola-de-lado]");
  const rolaY = (e) => { for (let p = e.parentElement; p; p = p.parentElement) { const s = getComputedStyle(p);
      if (/(auto|scroll)/.test(s.overflowY) && p.scrollHeight > p.clientHeight) return true; } return false; };
  const todos = [...document.querySelectorAll("body *")].filter(vis);
  const vaza = todos.filter((e) => { const r = e.getBoundingClientRect(); return (r.right > W + 1 || r.left < -1) && !rolaX(e); })
    .filter((e) => ![...e.children].some((c) => vis(c) && c.getBoundingClientRect().right > W + 1));
  const tocavel = (e) => { const r = e.getBoundingClientRect(); const x = Math.min(Math.max(r.left + r.width / 2, 1), W - 1), y = r.top + r.height / 2;
      if (y < 0 || y > H) return false; const alvo = document.elementFromPoint(x, y); return alvo && (e === alvo || e.contains(alvo)); };
  const controles = todos.filter((e) => e.matches("button, a[href], input:not([type=hidden]), select, textarea, summary, [role=button]")).filter(tocavel);
  const pequenos = controles.filter((e) => { const r = e.getBoundingClientRect(); return (r.width < 44 || r.height < 44) && !e.matches("input[type=range]"); });
  const temTexto = (e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
  const naIlha = todos.filter((e) => (controles.includes(e) || temTexto(e)) && e.getBoundingClientRect().top < topo - 1 && e.getBoundingClientRect().bottom > 0 && !rolaY(e))
    .map((e) => nome(e) + " y=" + Math.round(e.getBoundingClientRect().top));
  const naBarra = todos.filter((e) => (controles.includes(e) || temTexto(e)) && e.getBoundingClientRect().bottom > H - pe + 1 && e.getBoundingClientRect().top < H && !rolaY(e))
    .map((e) => nome(e) + " fim=" + Math.round(e.getBoundingClientRect().bottom));
  const transborda = todos.filter((e) => temTexto(e) && !rolaX(e) && getComputedStyle(e).overflowX === "visible"
      && e.scrollWidth > e.clientWidth + 1 && e.clientWidth > 0)
    .map((e) => nome(e) + ` ${e.scrollWidth}>${e.clientWidth}`);
  // O contraste de todo texto visível contra o fundo opaco de verdade atrás
  // dele: 4,5:1, ou 3:1 para texto grande (WCAG 1.4.3). `color(srgb …)` vem em
  // 0–1 (o que um `color-mix` devolve), e `rgb(…)` em 0–255.
  const rgba = (c) => { const n = (c.match(/[\d.]+/g) || []).map(Number);
    return c.startsWith("color(srgb") ? n.map((v, i) => (i < 3 ? v * 255 : v)) : n; };
  const luzDe = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const fundoDe = (e) => { for (let p = e; p; p = p.parentElement) { const s = getComputedStyle(p);
      if (p.classList.contains("cel-capa") || s.backgroundImage !== "none" && !p.classList.contains("varredura")) return null;
      const c = rgba(s.backgroundColor); if (c.length >= 3 && (c.length < 4 || c[3] >= 0.99)) return c; } return [5, 4, 3]; };
  const baixo = [];
  for (const e of todos) {
    if (!temTexto(e) || e.closest("[aria-hidden='true']")) continue;
    const s = getComputedStyle(e); const cor = rgba(s.color); if (cor.length >= 4 && cor[3] < 0.99) continue;
    if (parseFloat(s.opacity) < 1) continue;
    const fundo = fundoDe(e); if (!fundo) continue;
    const [a, b] = [luzDe(cor), luzDe(fundo)].sort((x, y) => y - x); const razao = (a + 0.05) / (b + 0.05);
    const px = parseFloat(s.fontSize), grande = px >= 24 || (px >= 18.66 && Number(s.fontWeight) >= 700);
    if (razao < (grande ? 3 : 4.5)) baixo.push(nome(e) + ` ${razao.toFixed(2)}:1`);
  }
  return {
    contraste: [...new Set(baixo)].slice(0, 8),
    transborda: transborda.slice(0, 6),
    vaza: vaza.slice(0, 6).map((e) => nome(e) + " r=" + Math.round(e.getBoundingClientRect().right)),
    pequenos: pequenos.slice(0, 8).map((e) => { const r = e.getBoundingClientRect(); return nome(e) + ` ${Math.round(r.width)}x${Math.round(r.height)}`; }),
    naIlha: naIlha.slice(0, 6), naBarra: naBarra.slice(0, 6),
    larguraDoDocumento: document.documentElement.scrollWidth,
  };
}
"""

# O núcleo (spec, §6): a cor computada de cada peça, o contraste do texto e o da
# borda contra o fundo opaco mais próximo em volta (WCAG 1.4.11).
NUCLEO = r"""
() => {
  const rgb = (c) => (c.match(/[\d.]+/g) || []).slice(0, 3).map(Number);
  const luz = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const razao = (a, b) => { const [x, y] = [luz(rgb(a)), luz(rgb(b))].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
  const out = {};
  for (const id of ["voz-mudo", "voz-surdo", "voz-sair", "cel-selo-seguro", "cel-aba-ajustes", "cel-linha-mods"]) {
    const el = document.getElementById(id);
    if (!el || el.closest("[hidden]")) continue;
    const s = getComputedStyle(el);
    let em = el.parentElement;
    while (em && /rgba\(0, 0, 0, 0\)|transparent/.test(getComputedStyle(em).backgroundColor)) em = em.parentElement;
    const emVolta = em ? getComputedStyle(em).backgroundColor : "rgb(0, 0, 0)";
    out[id] = { cor: s.color, fundo: s.backgroundColor, borda: s.borderTopColor, emVolta,
                texto: Number(razao(s.color, s.backgroundColor).toFixed(2)),
                bordaContraFaixa: Number(razao(s.borderTopColor, emVolta).toFixed(2)) };
  }
  return out;
}
"""


def reprovacoes(m: dict, aparelho: dict, *, so_nucleo: bool) -> list[str]:
    """O que as medidas de uma foto reprovam."""
    motivos = []
    if not so_nucleo:
        if m["vaza"] or m["larguraDoDocumento"] > aparelho["largura"]:
            motivos.append(f"rola de lado: {m['vaza']} (documento com {m['larguraDoDocumento']} px)")
        if m["contraste"]:
            motivos.append(f"texto abaixo do contraste: {m['contraste']}")
        if m["transborda"]:
            motivos.append(f"texto passa da caixa: {m['transborda']}")
        if m["pequenos"]:
            motivos.append(f"alvos abaixo de 44: {m['pequenos']}")
        if m["naIlha"]:
            motivos.append(f"na ilha: {m['naIlha']}")
        if m["naBarra"]:
            motivos.append(f"na barra de gestos: {m['naBarra']}")
    for id_, n in m["nucleo"].items():
        if n["texto"] < 4.5:
            motivos.append(f"{id_}: texto {n['texto']}:1")
        if id_.startswith("voz-") and n["bordaContraFaixa"] < 3:
            motivos.append(f"{id_}: borda {n['bordaContraFaixa']}:1 contra o que está em volta")
    return motivos


def aplicar_tema(pg: Page, nome: str) -> None:
    """O tema de teste, pelo mesmo caminho que um MOD de tema usa no produto."""
    pg.evaluate(
        "(valores) => { temaDosMods.set('bancada', valores); escreverOTemaDaSessao(); }",
        TEMAS[nome],
    )
    escrito = pg.evaluate("document.getElementById('tela-sessao').style.getPropertyValue('--seele-negro-absoluto')")
    if escrito.strip().lower() != TEMAS[nome]["fundo"]:
        raise SystemExit(f"o tema {nome} não chegou a #tela-sessao: a bancada perdeu o caminho do tema")


# ---------------------------------------------------------------- os casos

@dataclasses.dataclass(frozen=True)
class Caso:
    """Um estado da casca do celular, e o que se confere nele.

    `levar` leva a página, já na sessão, ao estado, e devolve o que reprovou
    nele. `medir` liga as medidas do aparelho; um caso que ainda não tem a
    moldura do celular (o primeiro) não as liga. `temas` repete o caso sob os
    temas de teste, com o núcleo comparado ao da foto sem tema.
    """

    levar: Callable[[Page], list[str]]
    medir: bool = True
    temas: tuple[str, ...] = ()


def sessao_entrou(pg: Page) -> list[str]:
    """A sessão abriu com a marca, e a raiz do celular rodou."""
    falhas = []
    estado = pg.evaluate("""(() => ({
        marca: document.documentElement.dataset.casca,
        sessao: !document.getElementById('tela-sessao').hidden,
        celular: typeof CELULAR === 'object' && typeof CELULAR.h === 'function',
        barra: getComputedStyle(document.documentElement).getPropertyValue('--seele-barra-janela').trim(),
    }))()""")
    if estado["marca"] != "celular":
        falhas.append(f"a marca é {estado['marca']!r}")
    if not estado["sessao"]:
        falhas.append("a sessão não ficou visível")
    if not estado["celular"]:
        falhas.append("celular-casca.js não deixou CELULAR.h: a raiz do celular não rodou com a marca")
    if estado["barra"] != "0px":
        falhas.append(f"a barra da janela mede {estado['barra']!r}, e no celular ela não existe")
    return falhas


CASOS: dict[str, Caso] = {
    "sessao-entrou": Caso(levar=sessao_entrou, medir=False),
}


def rodar_caso(navegador, temporaria: pathlib.Path, saida: pathlib.Path, nome: str, caso: Caso) -> list[str]:
    falhas = []
    for tamanho, aparelho in APARELHOS.items():
        nucleo_nativo = None
        for tema in (None, *caso.temas):
            rotulo = f"{nome}-{tamanho}" + (f"-{tema}" if tema else "")
            pagina = montar(temporaria / rotulo, celular=True, marca="celular")
            ctx = navegador.new_context(
                viewport={"width": aparelho["largura"], "height": aparelho["altura"]},
                device_scale_factor=1, is_mobile=True, has_touch=True, reduced_motion="reduce",
            )
            pg = ctx.new_page()
            erros = []
            pg.on("pageerror", lambda e: erros.append(str(e)[:200]))
            pg.goto(f"file://{pagina}")
            entrar(pg)
            pg.evaluate(
                f"""document.documentElement.style.setProperty('--cel-seguro-topo', '{aparelho['topo']}px');
                    document.documentElement.style.setProperty('--cel-seguro-pe', '{aparelho['pe']}px');"""
            )
            if tema:
                aplicar_tema(pg, tema)
            motivos = caso.levar(pg)
            pg.wait_for_timeout(250)
            pg.screenshot(path=saida / f"{rotulo}.png", animations="disabled")
            if caso.medir:
                m = pg.evaluate(MEDIR, [aparelho["topo"], aparelho["pe"]])
                m["nucleo"] = pg.evaluate(NUCLEO)
                (saida / f"{rotulo}.json").write_text(json.dumps(m, ensure_ascii=False, indent=2))
                motivos += reprovacoes(m, aparelho, so_nucleo=tema == "hostil")
                if tema is None:
                    nucleo_nativo = m["nucleo"]
                else:
                    for id_, n in m["nucleo"].items():
                        nativo = (nucleo_nativo or {}).get(id_)
                        chaves = ("cor", "fundo", "borda", "emVolta") if id_.startswith("voz-") else ("cor", "fundo", "borda")
                        if nativo and {k: n[k] for k in chaves} != {k: nativo[k] for k in chaves}:
                            motivos.append(f"{id_} mudou com o tema: {n} contra {nativo}")
            if erros:
                motivos.append(f"erros de script: {erros}")
            ctx.close()
            print(f"{rotulo}: " + ("passa" if not motivos else "REPROVA"))
            falhas += [f"{rotulo}: {motivo}" for motivo in motivos]
    return falhas


def main() -> int:
    argumentos = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    argumentos.add_argument("saida", type=pathlib.Path)
    argumentos.add_argument("--so", default=None, help="g3, ou o nome de um caso de CASOS")
    opcoes = argumentos.parse_args()
    if opcoes.so and opcoes.so != "g3" and opcoes.so not in CASOS:
        argumentos.error(f"não há caso {opcoes.so!r}; os casos são: g3, {', '.join(CASOS)}")
    opcoes.saida.mkdir(parents=True, exist_ok=True)

    falhas = []
    with tempfile.TemporaryDirectory() as t, sync_playwright() as p:
        temporaria = pathlib.Path(t)
        if opcoes.so in (None, "g3"):
            falhas += g3(p, opcoes.saida, temporaria / "g3")
        casos = {k: v for k, v in CASOS.items() if opcoes.so in (None, k)}
        if casos:
            navegador = p.chromium.launch()
            for nome, caso in casos.items():
                falhas += rodar_caso(navegador, temporaria / "casos", opcoes.saida, nome, caso)
            navegador.close()

    for falha in falhas:
        print("REPROVA", falha)
    if not falhas:
        print(f"\na bancada passa; as fotos estão em {opcoes.saida}")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
