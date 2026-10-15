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

**O que já se mediu, e não se tenta de novo.**

- **O `fetch` não lê arquivo em `file://`** (medido em 10/10). O Chromium e o
  WebKit recusam, com ou sem `--allow-file-access-from-files`; o
  `XMLHttpRequest` só passa no Chromium com a bandeira. A casca usa o `fetch`
  porque a página de verdade vem de https: um caso que dependa dele (o enlace da
  entrada lê o `marca-simbolo.svg`) pede `Caso(http=True)`, e a cópia é servida
  em 127.0.0.1 numa porta efêmera, que não disputa a 8384. Trocar o `fetch` da
  casca para caber na bancada seria provar outra coisa.
- **Visível é o que o `checkVisibility()` diz** (regra da bancada desde 10/10).
  Num `<details>` fechado o conteúdo some por `content-visibility`, e o
  `getBoundingClientRect` ainda o mede com altura: uma conferência só pela caixa
  dava como à vista a falha escrita num registro fechado, e a reversão passava.
  Toda conferência nova de «está à vista» usa o `checkVisibility()`, e não só a
  caixa.
- **O Ed25519 e o IndexedDB funcionam em `file://`** no Chromium da bancada
  (medido em 10/10, com a identidade de verdade da F4).

Uso:
    python3 tools/bancada-do-celular.py /tmp/bancada
    python3 tools/bancada-do-celular.py /tmp/bancada --so sessao-entrou
    python3 tools/bancada-do-celular.py /tmp/bancada --so g3

Precisa do Playwright com o Chromium e o WebKit (`python3 -m pip install
playwright` e `python3 -m playwright install chromium webkit`) e do Pillow. Sai
com 1 se algo reprovar.
"""

import argparse
import contextlib
import dataclasses
import functools
import http.server
import importlib.util
import json
import pathlib
import re
import shutil
import sys
import tempfile
import threading
from typing import Callable

from PIL import Image, ImageChops
from playwright.sync_api import Error as ErroDoPlaywright
from playwright.sync_api import Page, sync_playwright

RAIZ = pathlib.Path(__file__).resolve().parent.parent
UI = RAIZ / "apps" / "seele-app" / "ui"

_spec = importlib.util.spec_from_file_location("carga", RAIZ / "tools" / "carga-da-casca.py")
carga = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(carga)

# A marca, como a ponte da F4 a escreve na carga (spec, §9.2): a escolha
# guardada à mão primeiro (`localStorage["seele.casca"]`, num `try`), e o
# critério do aparelho depois, que aqui é a casca que o caso pede.
MARCA = ('<script>{{ let casca = "{}"; try {{ const escolhida = localStorage.getItem("seele.casca");'
         ' if (escolhida === "celular" || escolhida === "larga") casca = escolhida; }} catch {{}}'
         ' document.documentElement.dataset.casca = casca; }}</script>\n')


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
    # Se o ponto troca a âncora, em vez de entrar depois dela.
    troca: bool = False


# Vazia desde a nona integração: o P1 e o P5 a P8 entraram no produto, e o P9 e
# o P10 já entram nele direto. Fica para o próximo ponto que precisar esperar.
PONTOS_INJETADOS: list[Ponto] = []

# As áreas seguras de um iPhone em retrato (a ilha em cima, a barra de gestos
# embaixo) e de um Android comum, com a barra de navegação por gestos.
APARELHOS = {
    "390": {"largura": 390, "altura": 844, "topo": 59, "pe": 34},
    "360": {"largura": 360, "altura": 800, "topo": 24, "pe": 24},
    # Um tablet, ou a casca do celular escolhida à mão numa tela grande.
    "1280": {"largura": 1280, "altura": 800, "topo": 0, "pe": 0, "celular": False},
    # O iPhone deitado: a ilha vai para o lado, e a barra de gestos fica embaixo.
    "deitado": {"largura": 844, "altura": 390, "topo": 0, "pe": 21},
    # Um computador, com mouse: a casca larga (D-web-3).
    "1440": {"largura": 1440, "altura": 900, "topo": 0, "pe": 0, "celular": False, "toque": False},
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
                texto = texto.replace(ponto.ancora, ponto.injetado if ponto.troca else ponto.ancora + ponto.injetado)
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
    # O duble montado pela carga, com todo marcador preenchido e nenhum
    # sobrando: a bancada não repete as trocas (ver `carga.duble_montado`).
    pagina.write_text(texto[:corte] + carga.duble_montado() + texto[corte:])
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
            ctx = navegador.new_context(viewport={"width": 1440, "height": 900}, reduced_motion="reduce", locale="pt-BR")
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
([topo, pe, raiz]) => {
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
  // O que se mede: a página inteira, ou só a parte que a tarefa já desenhou
  // para o celular (as outras ainda são do desktop). A largura do documento é
  // medida sempre na página inteira.
  const todos = [...document.querySelectorAll(raiz)].flatMap((r) => [r, ...r.querySelectorAll("*")]).filter(vis);
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
    moldura do celular (o primeiro) não as liga, e `raiz` as restringe ao que já
    é do celular. `temas` repete o caso sob os temas de teste, com o núcleo
    comparado ao da foto sem tema.
    """

    levar: Callable[[Page], list[str]]
    medir: bool = True
    temas: tuple[str, ...] = ()
    # O seletor do que as medidas do aparelho leem. Os painéis que uma tarefa
    # ainda não adaptou ficam de fora, e entram quando a tarefa deles chegar.
    raiz: str = "body"
    # O texto do sistema (1 é o padrão; 1,5 é o tamanho grande de acessibilidade).
    escala: float = 1.0
    aparelhos: tuple[str, ...] = ("390", "360")
    # A casca do navegador: "celular", ou "larga" para o computador; vazia é o
    # desktop, com os arquivos das cascas carregados e inertes.
    marca: str = "celular"
    # Entrar na sessão antes do caso; um caso da entrada entra ele mesmo.
    entrar: bool = True
    # Servir a cópia por HTTP, e não por `file://`. A página de verdade vem de
    # uma origem https, e o `fetch` de um arquivo da `ui/` (o símbolo da marca,
    # para o enlace) não funciona em `file://`: medido em 10/10, o Chromium e o
    # WebKit recusam, com ou sem `--allow-file-access-from-files`.
    http: bool = False


@contextlib.contextmanager
def servida(pasta: pathlib.Path):
    """A cópia servida em 127.0.0.1, numa porta efêmera: nada disputa a 8384."""
    class Calado(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *_):
            pass
    servidor = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Calado, directory=str(pasta)))
    fio = threading.Thread(target=servidor.serve_forever, daemon=True)
    fio.start()
    try:
        yield f"http://127.0.0.1:{servidor.server_address[1]}"
    finally:
        servidor.shutdown()
        servidor.server_close()


# ------------------------------------------------------------- a entrada
#
# A identidade de verdade (`criarIdentidadeDoNavegador`, da F4), com os dois
# pontos que ela recebe na criação trocados pelo caso: `analisar` (no produto,
# o `.wasm`) lê o estado pedido no fragmento, e `conectar` (no produto, a
# sessão pela ponte, 2.6) anota cada entrada e responde o que estiver na fila.
# Nada mais é duble: os estados, os desfechos, o token e o aceite são os dela.
IDENTIDADE_DO_CASO = r"""() => {
  const agora = Math.floor(Date.now() / 1000);
  const link = (extra = {}) => ({ link: { alvo: "198.51.100.7:8383", impressao: "3cbcfb02".repeat(8), versao: "1.0",
    ate: agora + 7 * 86400, hashes: ["sha-256:caso"], convite: "7K4MNPQRSTVWXYZ23456", ...extra } });
  const LINKS = {
    pronto: link(), vencido: link({ ate: agora - 60 }),
    "servidor-0.15": { erro: "SemHashesDoNavegador" }, incompleto: { erro: "SemImpressao" }, "outra-origem": { erro: "OutraOrigem" },
  };
  window.__conexoes = [];
  window.__respostas = [];
  window.__identidade = criarIdentidadeDoNavegador({
    analisar: (texto) => LINKS[texto.split("#")[1]] ?? { erro: "Desconhecido" },
    conectar: async (alvo, apelido, credencial) => {
      window.__conexoes.push({ endereco: alvo.endereco, apelido, credencial });
      const resposta = window.__respostas.shift();
      // «connect»: o connect do duble, como o da ponte fará na 2.6, com a
      // resposta inteira do desktop (a sessão, o veredito).
      if (resposta === "connect") {
        return window.__TAURI__.core.invoke("connect", { server: alvo.endereco, nickname: apelido, audio: true, joinSecret: credencial });
      }
      if (resposta === "espera") await new Promise((soltar) => { window.__soltarAConexao = soltar; });
      else if (resposta) throw resposta;
    },
    instalado: () => window.__instalado ?? true,
    // O relógio da identidade: o caso o adianta para ver os hashes guardados
    // vencerem (o LINK VENCIDO dos servidores deste aparelho).
    agora: () => window.__agora ?? Date.now(),
  });
}"""

ENTRADA_LIDA = r"""() => {
  const corpo = document.querySelector(".cel-entrada-corpo");
  const impressao = corpo.querySelector(".cel-entrada-impressao");
  const b = impressao?.getBoundingClientRect();
  return {
    titulo: corpo.querySelector("#cel-entrada-titulo")?.textContent.replace(/\s+/g, " ").trim() ?? null,
    avisos: [...corpo.querySelectorAll(".cel-entrada-aviso-titulo")].map((e) => e.textContent),
    grupos: impressao ? [...impressao.children].map((e) => e.textContent) : [],
    impressaoCabe: !impressao || b.right <= innerWidth,
    vaza: document.documentElement.scrollWidth > innerWidth,
    acoes: [...document.querySelectorAll(".cel-entrada-pe button")].map((e) => [e.textContent, Math.round(e.getBoundingClientRect().height)]),
    campo: corpo.querySelector("#cel-entrada-apelido") ? { valor: corpo.querySelector("#cel-entrada-apelido").value,
      invalido: corpo.querySelector("#cel-entrada-apelido").getAttribute("aria-invalid") } : null,
    conexoes: window.__conexoes.length,
  };
}"""


INICIO_LIDO = """() => ({ titulo: document.querySelector("#cel-entrada-titulo")?.textContent,
    servidores: [...document.querySelectorAll(".cel-entrada-servidor")].map((b) => [b.textContent, Math.round(b.getBoundingClientRect().height)]),
    vazio: document.querySelector(".cel-entrada-corpo > .cel-entrada-nota")?.textContent ?? null,
    baixar: document.querySelector(".cel-entrada-fora a.cel-baixar")?.getAttribute("href"),
    colar: [...document.querySelectorAll(".cel-entrada-pe button")].map((b) => b.textContent),
    caixaDoDesktop: getComputedStyle(document.querySelector("#tela-boot > .boot-caixa")).display })"""


def usar_a_entrada(pg: Page, **opcoes) -> None:
    """A tela com a identidade do caso. Os servidores deste aparelho são os da
    identidade (`conhecidos()` e `entrarNoConhecido`, da F4), guardados no
    IndexedDB pelas entradas que deram certo, e não uma lista do caso."""
    pg.evaluate("""(o) => CELULAR.ajudante("entrada").usar(window.__identidade, { link: o.link ?? null })""", opcoes)
    pg.wait_for_timeout(300)


def tocar_acao(pg: Page, texto: str) -> bool:
    """Toca a ação do pé da entrada que diz `texto`."""
    return tocar(pg, f".cel-entrada-pe button:text-is('{texto}')")


def entrada(pg: Page) -> list[str]:
    """A entrada (Tarefa 10; spec §2.1), da tela de início aos estados da
    «ENTRAR EM», sobre a identidade de verdade da F4.

    A tela de início lista os servidores deste aparelho com ENTRAR, tem COLAR UM
    LINK e a frase do hospedar com o BAIXAR O APP DO SEELE apontando para a
    `PAGINA_DO_APP`. Em cada estado: o título «ENTRAR EM <endereço>?», sem nome
    de servidor; a impressão em dezesseis grupos de quatro, sem passar da tela;
    o aviso do estado e as ações dele. **Nada conecta antes do toque em
    ENTRAR.** Termina na «ENTRAR EM» pronta, para a foto."""
    falhas = []
    pg.evaluate(IDENTIDADE_DO_CASO)
    usar_a_entrada(pg)
    inicio = pg.evaluate(INICIO_LIDO)
    f = pg.evaluate("FRASES_DO_CELULAR.entrada")
    if inicio["titulo"] != f["inicio"]["titulo"]:
        falhas.append(f"a tela de início diz «{inicio['titulo']}»")
    if inicio["servidores"] or inicio["vazio"] != f["inicio"]["vazio"]:
        falhas.append(f"sem servidor guardado, a tela de início mostra {inicio['servidores']} e diz «{inicio['vazio']}»")
    if inicio["baixar"] != pg.evaluate("PAGINA_DO_APP"):
        falhas.append(f"o BAIXAR O APP DO SEELE aponta para {inicio['baixar']}")
    if inicio["colar"] != [f["inicio"]["colar"]]:
        falhas.append(f"o pé da tela de início tem {inicio['colar']}")
    if inicio["caixaDoDesktop"] != "none":
        falhas.append("a caixa de entrada do desktop continua à vista no celular")

    e = f["estados"]
    esperado = {
        "pronto": (None, [f["entrarEm"]["entrar"], f["entrarEm"]["agoraNao"]]),
        "numa-aba": (e["numa-aba"]["titulo"], [e["numa-aba"]["copiar"], f["entrarEm"]["agoraNao"]]),
        "vencido": ("ESTE LINK VENCEU EM", [f["entrarEm"]["servidores"]]),
        "servidor-0.15": (e["servidor-0.15"]["titulo"], [f["entrarEm"]["servidores"]]),
        "incompleto": (e["incompleto"]["titulo"], [f["entrarEm"]["servidores"]]),
        "outra-origem": (e["outra-origem"]["titulo"], [f["entrarEm"]["servidores"]]),
    }
    for estado, (aviso, acoes) in esperado.items():
        pg.evaluate(f"window.__instalado = {'false' if estado == 'numa-aba' else 'true'}")
        usar_a_entrada(pg, link=f"https://pagina.invalid/#{'pronto' if estado == 'numa-aba' else estado}")
        lido = pg.evaluate(ENTRADA_LIDA)
        com_endereco = estado in ("pronto", "numa-aba", "vencido")
        if com_endereco and lido["titulo"] != "ENTRAR EM 198.51.100.7:8383?":
            falhas.append(f"{estado}: o título diz «{lido['titulo']}», e não «ENTRAR EM 198.51.100.7:8383?»")
        if com_endereco and (len(lido["grupos"]) != 16 or any(len(g) != 4 for g in lido["grupos"])):
            falhas.append(f"{estado}: a impressão sai em {lido['grupos']}")
        if not lido["impressaoCabe"] or lido["vaza"]:
            falhas.append(f"{estado}: a impressão passa da tela, ou a página rola de lado")
        if aviso and not any(a.startswith(aviso) for a in lido["avisos"]):
            falhas.append(f"{estado}: o aviso diz {lido['avisos']}, e não «{aviso}…»")
        if [t for t, _ in lido["acoes"]] != acoes:
            falhas.append(f"{estado}: as ações são {lido['acoes']}, e não {acoes}")
        if any(alto < 44 for _, alto in lido["acoes"]):
            falhas.append(f"{estado}: uma ação tem menos de 44: {lido['acoes']}")
        if lido["conexoes"]:
            falhas.append(f"{estado}: a tela conectou antes do ENTRAR ({lido['conexoes']} entradas)")
    pg.evaluate("window.__instalado = true")
    falhas += conhecidos_de_verdade(pg, f)
    return falhas


def conhecidos_de_verdade(pg: Page, f: dict) -> list[str]:
    """SERVIDORES DESTE APARELHO pela identidade da F4: uma entrada que dá
    certo guarda o servidor; o nome chega pelo `lembrarONome` (no produto, o
    primeiro retrato); a lista mostra o nome, o endereço, a versão e o dia, com
    ENTRAR, que entra pela chave e **sem o convite**; e com o relógio da
    identidade oito dias adiante os hashes guardados vencem, a lista diz LINK
    VENCIDO, e o toque recebe o `link-vencido`."""
    falhas = []
    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    pg.fill("#cel-entrada-apelido", "rafa")
    tocar_acao(pg, f["entrarEm"]["entrar"])
    pg.wait_for_timeout(600)
    # Antes do primeiro retrato o servidor não tem nome: o endereço é o título,
    # uma vez só.
    usar_a_entrada(pg)
    sem_nome = pg.evaluate(INICIO_LIDO)["servidores"]
    if len(sem_nome) != 1 or sem_nome[0][0].count("198.51.100.7:8383") != 1:
        falhas.append(f"o servidor guardado, ainda sem nome, sai como {sem_nome}")
    pg.evaluate('window.__identidade.lembrarONome("3cbcfb02".repeat(8), "OFICINA")')
    usar_a_entrada(pg)
    inicio = pg.evaluate(INICIO_LIDO)
    versao = pg.evaluate("FRASES_DO_CELULAR.entrada.inicio.servidor('1.0')")
    servidores = inicio["servidores"]
    if (len(servidores) != 1 or not servidores[0][0].startswith("OFICINA") or "198.51.100.7:8383" not in servidores[0][0]
            or versao not in servidores[0][0] or not servidores[0][0].endswith(f["inicio"]["entrar"]) or servidores[0][1] < 44):
        falhas.append(f"o servidor guardado sai na tela de início como {servidores}")
    antes = pg.evaluate("window.__conexoes.length")
    tocar(pg, ".cel-entrada-servidor")
    pg.wait_for_timeout(600)
    pelo_conhecido = pg.evaluate("window.__conexoes.slice(%d)" % antes)
    if len(pelo_conhecido) != 1 or pelo_conhecido[0]["credencial"] is not None:
        falhas.append(f"o ENTRAR da lista não entrou pela chave, sem o convite: {pelo_conhecido}")
    # Oito dias depois, os hashes guardados (sete dias) venceram.
    pg.evaluate("window.__agora = Date.now() + 8 * 86400000")
    usar_a_entrada(pg)
    vencido = pg.evaluate(INICIO_LIDO)
    if len(vencido["servidores"]) != 1 or not vencido["servidores"][0][0].endswith(f["inicio"]["linkVencido"]):
        falhas.append(f"com os hashes guardados vencidos, a lista diz {vencido['servidores']}")
    tocar(pg, ".cel-entrada-servidor")
    pg.wait_for_timeout(600)
    aviso = pg.evaluate("[...document.querySelectorAll('.cel-entrada-aviso-titulo')].map((e) => e.textContent)")
    if aviso != [f["desfechos"]["link-vencido"][0]]:
        falhas.append(f"o toque num servidor de link vencido diz {aviso}")
    return falhas


def entrada_desfechos(pg: Page) -> list[str]:
    """Os desfechos do ENTRAR (Tarefa 10), sobre a identidade de verdade.

    O apelido tomado devolve o campo vazio e marcado, e o convite gasto logo
    depois dele diz que o convite acabou, e não que o apelido está tomado. Sem
    rede, o ENTRAR continua. Os MODs a aceitar abrem o aceite do desktop, e o
    ACEITAR entra de novo **com o mesmo token**; o RECUSAR encerra a tentativa.
    Quem já entrou volta como conhecido, sem gastar o convite. Enquanto a
    conexão abre, o enlace se desenha, parado sob movimento reduzido."""
    falhas = []
    pg.evaluate(IDENTIDADE_DO_CASO)
    f = pg.evaluate("FRASES_DO_CELULAR.entrada")
    entrar = f["entrarEm"]["entrar"]

    def tentar(apelido: str, respostas: str) -> dict:
        pg.evaluate(f"window.__respostas.push(...{respostas})")
        pg.fill("#cel-entrada-apelido", apelido)
        tocar_acao(pg, entrar)
        pg.wait_for_timeout(500)
        return pg.evaluate(ENTRADA_LIDA)

    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    tomado = tentar("rafa", '[{ Refused: { reason: "NicknameTaken" } }]')
    if tomado["campo"] != {"valor": "", "invalido": "true"} or f["desfechos"]["apelido-tomado"][0] not in tomado["avisos"]:
        falhas.append(f"o apelido tomado não devolve o campo vazio e marcado: {tomado}")
    gasto = tentar("rafa2", '["CredentialRejected"]')
    if gasto["avisos"][:1] != [f["desfechos"]["convite-gasto"][0]] or [t for t, _ in gasto["acoes"]] != [f["entrarEm"]["servidores"]]:
        falhas.append(f"o convite gasto depois do apelido tomado diz {gasto['avisos']}, com {gasto['acoes']}")
    tokens = pg.evaluate("window.__conexoes.map((c) => c.credencial)")
    if tokens != ["7K4MNPQRSTVWXYZ23456", "7K4MNPQRSTVWXYZ23456"]:
        falhas.append(f"as duas entradas levaram {tokens}")

    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    sem_rede = tentar("rafa", '["Unreachable"]')
    if sem_rede["avisos"][:1] != [f["desfechos"]["sem-rede"][0]] or [t for t, _ in sem_rede["acoes"]][:1] != [entrar]:
        falhas.append(f"sem rede, a tela diz {sem_rede['avisos']}, com {sem_rede['acoes']}")

    # Os MODs: o aceite do desktop abre, e o ACEITAR entra de novo com o mesmo token.
    pg.evaluate("window.__conexoes.length = 0")
    mods = '[{ ModsNaoAceitos: { mods: [{ id: "seele/estilo", version: "4.0.0", hash: "ab".repeat(32), reach: ["servidor.aparencia"] }], conjunto: "c1" } }]'
    tentar("rafa", mods)
    if not pg.locator("#mods-aceite").is_visible():
        falhas.append("os MODs a aceitar não abriram o aceite")
    else:
        tocar(pg, "#mods-aceitar")
        pg.wait_for_timeout(500)
        depois = pg.evaluate("""() => ({ tokens: window.__conexoes.map((c) => c.credencial),
            aceite: JSON.parse(localStorage.getItem("seele.aceites-de-mods") ?? "{}")["3cbcfb02".repeat(8)] ?? null,
            aberto: !document.getElementById("mods-aceite").hidden })""")
        if depois != {"tokens": ["7K4MNPQRSTVWXYZ23456", "7K4MNPQRSTVWXYZ23456"], "aceite": "c1", "aberto": False}:
            falhas.append(f"o ACEITAR não entrou de novo com o mesmo token e o sim gravado: {depois}")
    # O RECUSAR encerra a tentativa e volta à tela de início.
    usar_a_entrada(pg, link="https://pagina.invalid/#vencido")
    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    tentar("rafa", mods)
    tocar(pg, "#mods-recusar")
    pg.wait_for_timeout(400)
    recusado = pg.evaluate("""() => ({ titulo: document.querySelector("#cel-entrada-titulo")?.textContent, tentativa: window.__identidade.ler() })""")
    if recusado != {"titulo": f["inicio"]["titulo"], "tentativa": None}:
        falhas.append(f"o RECUSAR não encerrou a tentativa: {recusado}")

    # Conectando: o enlace, parado sob movimento reduzido e desenhado sem ele.
    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    pg.evaluate('window.__respostas.push("espera")')
    pg.fill("#cel-entrada-apelido", "rafa")
    tocar_acao(pg, entrar)
    pg.wait_for_timeout(400)
    # Sem movimento reduzido, o enlace se desenha; com ele, fica parado e
    # desenhado inteiro (o traço no fim, e não no começo, que é invisível).
    enlace = "(() => { const p = document.querySelector('.cel-entrada-conectando svg path'); return p ? [getComputedStyle(p).animationName, getComputedStyle(p).strokeDashoffset] : null; })()"
    parado = pg.evaluate(enlace)
    pg.emulate_media(reduced_motion="no-preference")
    andando = pg.evaluate(enlace)
    pg.emulate_media(reduced_motion="reduce")
    if parado != ["none", "0px"] or not andando or andando[0] != "cel-enlace":
        falhas.append(f"o enlace da conexão está {andando!r} sem movimento reduzido e {parado!r} com ele")
    if not pg.evaluate("typeof window.__soltarAConexao === 'function'"):
        falhas.append("o ENTRAR não chegou à conexão que espera: a tela de conectar não foi medida")
        return falhas
    pg.evaluate("window.__soltarAConexao()")
    pg.wait_for_timeout(500)
    # Quem entrou volta como conhecido: o convite não é usado.
    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    conhecido = pg.evaluate(ENTRADA_LIDA)
    if f["estados"]["ja-conhecido"]["titulo"] not in conhecido["avisos"] or [t for t, _ in conhecido["acoes"]][:1] != [entrar]:
        falhas.append(f"quem já entrou não volta como conhecido: {conhecido['avisos']}, {conhecido['acoes']}")
    return falhas


# ------------------------------------------------- a entrada direta (D-celular-3)

CONFERIDA = '{ FirstContactVerified: { fingerprint: "3cbc fb02 3cbc fb02 3cbc fb02 3cbc fb02" } }'


def entrar_com_veredito(pg: Page, veredito: str, antes: str = "") -> None:
    """Entra pelo `conectar` do desktop com o veredito pedido, contando cada
    vez que o aperto aparece sem a marca da entrada direta."""
    pg.wait_for_function("typeof conectar === 'function'")
    pg.wait_for_timeout(600)
    pg.evaluate(f"""() => {{
      SEELE_RESPOSTAS.connect = {{ snapshot: SEELE_QUADRO, veredito: {veredito} }};
      {antes}
      window.__apertos = 0;
      const tela = document.getElementById('tela-auth');
      new MutationObserver(() => {{
        if (!tela.hidden && tela.dataset.modo === 'aperto' && !tela.hasAttribute('data-entrada-direta')) window.__apertos++;
      }}).observe(tela, {{ attributes: true, attributeFilter: ['hidden', 'data-modo', 'data-entrada-direta'] }});
    }}""")
    pg.evaluate("conectar('127.0.0.1:8383', 'rafa', null)")
    pg.wait_for_timeout(1500)


ESTADO_DA_ENTRADA = r"""() => {
  const tela = document.getElementById('tela-auth'), botao = document.getElementById('auth-botao');
  const linhas = [...document.querySelectorAll('#auth-registro .registro-linha')];
  // Num `<details>` fechado, o conteúdo some por `content-visibility`, e o
  // `getBoundingClientRect` ainda o mede: quem diz se ele se vê é o
  // `checkVisibility`, e o `<details>` aberto.
  const visivel = (e) => !!e && e.checkVisibility() && !e.closest('details:not([open])') && e.getBoundingClientRect().height > 0;
  return { sessao: !document.getElementById('tela-sessao').hidden, auth: !tela.hidden, modo: tela.dataset.modo ?? null,
           direta: tela.hasAttribute('data-entrada-direta'), apertos: window.__apertos,
           botao: [botao.textContent, botao.disabled, Math.round(botao.getBoundingClientRect().height)],
           registro: linhas.map((l) => l.querySelector('.registro-texto')?.textContent ?? ''),
           ultima: linhas.length ? [linhas.at(-1).querySelector('.registro-texto')?.textContent ?? '', visivel(linhas.at(-1))] : null,
           vaza: document.documentElement.scrollWidth > innerWidth };
}"""


def entrada_direta(pg: Page) -> list[str]:
    """D-celular-3: com a impressão conferida, o aperto da tela de entrada não
    aparece no celular, e a sessão abre sem um toque. Pula-se a tela, e não o
    passo: o registro diz a entrada direta, e o `inserirPlug` roda (P11)."""
    falhas = []
    entrar_com_veredito(pg, CONFERIDA)
    e = pg.evaluate(ESTADO_DA_ENTRADA)
    if not e["sessao"] or e["apertos"]:
        falhas.append(f"com a impressão conferida, o aperto apareceu ou a sessão não abriu sem um toque: {e}")
    if pg.evaluate("FRASES_DO_CELULAR.entrada.entradaDireta.registro") not in e["registro"]:
        falhas.append(f"o registro não diz a entrada direta: {e['registro']}")
    return falhas


def entrada_com_aperto(pg: Page) -> list[str]:
    """Um veredito diferente (o primeiro contato sem impressão conferida)
    continua indo ao aperto, agora numa coluna: o botão com 44, nada passando
    da tela, e o ENTRAR NO SERVIDOR entrando. Termina no aperto, para a foto."""
    falhas = []
    entrar_com_veredito(pg, '{ FirstContact: { fingerprint: "3cbc fb02 3cbc fb02" } }')
    e = pg.evaluate(ESTADO_DA_ENTRADA)
    if e["sessao"] or not e["auth"] or e["modo"] != "aperto" or e["direta"] or not e["apertos"]:
        falhas.append(f"um veredito diferente não foi ao aperto: {e}")
    if e["botao"][0] != "ENTRAR NO SERVIDOR" or e["botao"][1] or e["botao"][2] < 44:
        falhas.append(f"o botão do aperto está {e['botao']}")
    if e["vaza"]:
        falhas.append("o aperto rola de lado no celular")
    return falhas


def entrada_direta_falha(pg: Page) -> list[str]:
    """Na entrada direta, uma falha do `inserirPlug` tem tela e frase: o registro
    aberto mostra a frase do erro, e ENTRAR NO SERVIDOR volta para tentar de
    novo. O celular não fica parado sem dizer nada."""
    falhas = []
    entrar_com_veredito(pg, CONFERIDA, 'SEELE_RESPOSTAS.open_channel = { __recusa: "NotConnected" };')
    e = pg.evaluate(ESTADO_DA_ENTRADA)
    if e["sessao"] or not e["auth"]:
        falhas.append(f"com a entrada falhando, a tela não ficou na entrada: {e}")
    # A última linha do registro é a frase do erro, a mesma que o desktop
    # escreve, e ela está à vista: o veredito conferido também escreve no tom de
    # atenção, então o que se confere é a frase, e não o tom.
    frase = pg.evaluate("fraseDeErro('NotConnected')")
    if e["ultima"] != [frase, True]:
        falhas.append(f"a falha da entrada não está à vista: a última linha é {e['ultima']}, e não «{frase}»")
    if e["botao"][0] != "ENTRAR NO SERVIDOR" or e["botao"][1]:
        falhas.append(f"depois da falha, o botão está {e['botao']}")
    return falhas


def entrada_espera(pg: Page) -> list[str]:
    """A espera da portaria (`AdmissionPending`, ADR 0030) numa coluna, como a
    D-celular-3 pede: a frase da espera à vista, a frase de que ela para com a
    página fora da tela (spec §2.1), a saída com 44, e nada passando da tela.
    Termina na espera, para a foto e as medidas."""
    falhas = []
    entrar_com_veredito(pg, CONFERIDA, 'SEELE_RESPOSTAS.connect = { __recusa: { Refused: { reason: "AdmissionPending" } } };')
    e = pg.evaluate("""() => { const tela = document.getElementById('tela-auth'), saida = document.getElementById('auth-voltar');
        const frase = document.getElementById('auth-espera-frase'), pausa = document.querySelector('.cel-espera-pausa');
        return { auth: !tela.hidden, modo: tela.dataset.modo ?? null, frase: frase?.textContent.trim() ?? '',
                 pausa: !!pausa && pausa.getBoundingClientRect().height > 0 && pausa.textContent === FRASES_DO_CELULAR.entrada.esperaPausa,
                 fraseVisivel: !!frase && frase.getBoundingClientRect().height > 0,
                 saida: [saida.textContent.trim(), Math.round(saida.getBoundingClientRect().height)],
                 vaza: document.documentElement.scrollWidth > innerWidth }; }""")
    if not e["auth"] or e["modo"] != "espera" or not e["fraseVisivel"] or not e["frase"]:
        falhas.append(f"a espera da portaria não está numa tela própria, com a frase: {e}")
    if not e["pausa"]:
        falhas.append("a espera não diz que para com a página fora da tela")
    if e["saida"][1] < 44:
        falhas.append(f"a saída da espera tem {e['saida'][1]} de altura")
    if e["vaza"]:
        falhas.append("a espera rola de lado no celular")
    return falhas


def entrada_abre_a_sessao(pg: Page) -> list[str]:
    """A «ENTRAR EM» abre a sessão: o ENTRAR chama o `entrar` da identidade, o
    `conectar` dela chega ao `connect` (aqui o do duble, como o da ponte fará na
    2.6), e o `{ ok: true, resposta }` passa pelo `depoisDeConectar`, o mesmo do
    desktop. Com a impressão conferida, o aperto não aparece (D-celular-3), e a
    sessão abre sem outro toque."""
    falhas = []
    pg.evaluate(IDENTIDADE_DO_CASO)
    pg.evaluate(f"""() => {{
      SEELE_RESPOSTAS.connect = {{ snapshot: SEELE_QUADRO, veredito: {CONFERIDA} }};
      window.__respostas.push("connect");
      window.__apertos = 0;
      const tela = document.getElementById('tela-auth');
      new MutationObserver(() => {{
        if (!tela.hidden && tela.dataset.modo === 'aperto' && !tela.hasAttribute('data-entrada-direta')) window.__apertos++;
      }}).observe(tela, {{ attributes: true, attributeFilter: ['hidden', 'data-modo', 'data-entrada-direta'] }});
    }}""")
    usar_a_entrada(pg, link="https://pagina.invalid/#pronto")
    pg.fill("#cel-entrada-apelido", "rafa")
    tocar_acao(pg, pg.evaluate("FRASES_DO_CELULAR.entrada.entrarEm.entrar"))
    pg.wait_for_timeout(1500)
    e = pg.evaluate(ESTADO_DA_ENTRADA)
    if not e["sessao"] or e["apertos"]:
        falhas.append(f"o ENTRAR da «ENTRAR EM» não abriu a sessão, ou parou no aperto: {e}")
    if pg.evaluate("!document.getElementById('tela-boot').hidden"):
        falhas.append("depois do ENTRAR, a tela de entrada continua à vista")
    return falhas


def entrada_direta_na_larga(pg: Page) -> list[str]:
    """A casca larga não muda (D-celular-3): com a mesma impressão conferida, o
    aperto aparece como no desktop."""
    entrar_com_veredito(pg, CONFERIDA)
    e = pg.evaluate(ESTADO_DA_ENTRADA)
    return [] if (not e["sessao"] and e["auth"] and not e["direta"] and e["apertos"]) else [f"na larga, a impressão conferida pulou o aperto: {e}"]


# ------------------------------------------- as camadas sobre a sessão (Tarefa 11)

MUDO_ALCANCAVEL = """() => { const b = document.getElementById('voz-mudo').getBoundingClientRect();
    return !!document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2)?.closest('#voz-mudo'); }"""


def queda(pg: Page) -> list[str]:
    """A queda (spec §2.5; specs/07): uma faixa no alto, em largura cheia, e não
    uma caixa sobre a conversa. A sessão esmaece e continua legível: uma
    mensagem à vista embaixo da faixa recebe o toque. A faixa de voz diz «SEM
    SOM · RECONECTANDO», e o MUDO continua ao alcance. Quando o enlace volta,
    tudo volta. Termina na queda, para a foto."""
    falhas = []
    ir_para_aba(pg, "conversa", falhas)
    pg.evaluate("SEELE_QUADRO.link = { InternalBattery: { remaining_seconds: 287, attempts: 2 } }")
    esperar_quadro(pg)
    v = pg.evaluate("""() => {
        const caixa = document.querySelector('#bateria .bateria-caixa'); const b = caixa?.getBoundingClientRect();
        const legivel = [...document.querySelectorAll('#lista-mensagens li.mensagem')].some((li) => {
            const r = li.getBoundingClientRect(); if (!b || r.top < b.bottom || r.height === 0) return false;
            const c = document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(r.height / 2, 12)); return !!c && li.contains(c); });
        return { visivel: !!caixa && caixa.checkVisibility(), caixa: b ? [Math.round(b.top), Math.round(b.width), Math.round(b.height)] : null,
                 legivel, faixa: document.querySelector('.voz-celular .voz-sala-quem')?.textContent ?? '',
                 inteira: ((q, f) => q.scrollWidth <= q.clientWidth + 1 && q.getBoundingClientRect().bottom <= f.getBoundingClientRect().bottom + 1)(
                     document.querySelector('.voz-celular .voz-sala-quem'), document.querySelector('.voz-celular')),
                 vaza: document.documentElement.scrollWidth > innerWidth }; }""")
    largura, altura = pg.evaluate("[innerWidth, innerHeight]")
    # A altura conta sem a área segura do alto: aquela faixa é do sistema (a
    # ilha), e não da conversa.
    seguro = pg.evaluate("parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cel-seguro-topo')) || 0")
    if not v["visivel"] or v["caixa"][0] > 1 or v["caixa"][1] < largura - 1 or v["caixa"][2] - seguro > altura / 3:
        falhas.append(f"a queda não é uma faixa no alto, em largura cheia: {v['caixa']} numa tela de {largura}×{altura}, com {seguro} de área segura")
    if not v["legivel"]:
        falhas.append("com a queda, nenhuma mensagem fica à vista e ao toque embaixo da faixa")
    reconectando = pg.evaluate("FRASES_DO_CELULAR.voz.reconectando")
    if v["faixa"] != reconectando:
        falhas.append(f"com a queda, a faixa de voz diz «{v['faixa']}», e não «{reconectando}»")
    elif not v["inteira"]:
        falhas.append(f"com a queda, «{reconectando}» sai cortado na faixa de voz")
    if not pg.evaluate(MUDO_ALCANCAVEL):
        falhas.append("com a queda, o MUDO não recebe o toque")
    if v["vaza"]:
        falhas.append("com a queda, a página rola de lado")
    pg.evaluate("SEELE_QUADRO.link = 'Online'")
    esperar_quadro(pg)
    volta = pg.evaluate("[document.getElementById('bateria').hidden, document.querySelector('.voz-celular .voz-sala-quem')?.textContent ?? '']")
    if not volta[0] or volta[1] == reconectando:
        falhas.append(f"o enlace voltou, e a queda ficou: {volta}")
    pg.evaluate("SEELE_QUADRO.link = { InternalBattery: { remaining_seconds: 287, attempts: 2 } }")
    esperar_quadro(pg)
    return falhas


def alerta(pg: Page) -> list[str]:
    """O alerta (`#banner`; spec §2.5) em largura cheia, com o fechar de 44. A
    porta de moderar do alerta não aparece no celular: moderar é pelo SEELE no
    computador. Termina com o alerta aberto, para a foto."""
    falhas = []
    pg.evaluate("SEELE_QUADRO.notice = { severity: 'Alta', reason: 'ServerShuttingDown', operator_text: null }")
    esperar_quadro(pg)
    v = pg.evaluate("""() => {
        const caixa = document.querySelector('#banner .alerta-caixa'); const b = caixa?.getBoundingClientRect();
        const fechar = document.getElementById('banner-fechar').getBoundingClientRect();
        return { visivel: !!caixa && caixa.checkVisibility(), largura: b ? Math.round(b.width) : 0,
                 fechar: [Math.round(fechar.width), Math.round(fechar.height)],
                 porta: document.getElementById('alerta-ejetar').checkVisibility(),
                 vaza: document.documentElement.scrollWidth > innerWidth }; }""")
    if not v["visivel"] or v["largura"] < pg.evaluate("innerWidth") - 1:
        falhas.append(f"o alerta não está em largura cheia: {v['largura']} px")
    if min(v["fechar"]) < 44:
        falhas.append(f"o fechar do alerta tem {v['fechar']}")
    if v["porta"]:
        falhas.append("o alerta oferece moderar no celular")
    if v["vaza"]:
        falhas.append("com o alerta, a página rola de lado")
    return falhas


def confirmacao(pg: Page) -> list[str]:
    """A confirmação (spec §2.4) é uma folha de baixo, como as do celular: em
    largura cheia, presa ao pé, com a borda vermelha, o verbo inteiro no botão,
    o CANCELAR com 44 como saída, e o voltar do aparelho a fecha. Termina com
    ela aberta, para a foto."""
    falhas = []
    abrir = "abrirConfirmacao('APAGAR A MENSAGEM', 'A mensagem sai da conversa para todo mundo, e não volta.', 'APAGAR A MENSAGEM', () => {})"
    pg.evaluate(abrir)
    pg.wait_for_timeout(300)
    v = pg.evaluate("""() => {
        const caixa = document.querySelector('#moderar .moderar-caixa'); const b = caixa.getBoundingClientRect();
        const verbo = document.getElementById('moderar-confirmar'), cancelar = document.getElementById('moderar-cancelar');
        return { caixa: [Math.round(b.left), Math.round(b.bottom), Math.round(b.width)], borda: getComputedStyle(caixa).borderTopColor,
                 vermelho: getComputedStyle(document.documentElement).getPropertyValue('--seele-vermelho-alerta').trim(),
                 verbo: [verbo.textContent.trim(), Math.round(verbo.getBoundingClientRect().height)],
                 cancelar: cancelar.checkVisibility() ? Math.round(cancelar.getBoundingClientRect().height) : 0,
                 vaza: document.documentElement.scrollWidth > innerWidth }; }""")
    largura, altura = pg.evaluate("[innerWidth, innerHeight]")
    if v["caixa"][0] > 0 or v["caixa"][2] < largura - 1 or abs(v["caixa"][1] - altura) > 1:
        falhas.append(f"a confirmação não é uma folha presa ao pé, em largura cheia: {v['caixa']}")
    vermelho = pg.evaluate("(cor) => { const e = document.createElement('i'); e.style.color = cor; document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; }", v["vermelho"])
    if v["borda"] != vermelho:
        falhas.append(f"a borda da confirmação é {v['borda']}, e não o vermelho do alerta ({vermelho})")
    if v["verbo"][0] != "APAGAR A MENSAGEM" or v["verbo"][1] < 44:
        falhas.append(f"o botão da confirmação diz «{v['verbo'][0]}», com {v['verbo'][1]} de altura")
    if v["cancelar"] < 44:
        falhas.append(f"a saída da confirmação (CANCELAR) tem {v['cancelar']} de altura")
    if v["vaza"]:
        falhas.append("com a confirmação, a página rola de lado")
    pg.go_back()
    pg.wait_for_timeout(400)
    if not pg.evaluate("document.getElementById('moderar').hidden"):
        falhas.append("o voltar do aparelho não fechou a confirmação")
    if pg.evaluate("document.getElementById('tela-sessao').hidden"):
        falhas.append("o voltar saiu da sessão em vez de fechar a confirmação")
    pg.evaluate(abrir)
    pg.wait_for_timeout(300)
    return falhas


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
    if pg.evaluate("getComputedStyle(document.documentElement).overscrollBehaviorY") != "none":
        falhas.append("puxar a página para baixo a recarrega, e recarregar derruba a sessão")
    if estado["barra"] != "0px":
        falhas.append(f"a barra da janela mede {estado['barra']!r}, e no celular ela não existe")
    # O aviso de fone é o do navegador (P9; a frase e o porquê estão no
    # frases.js). A `entrar` o dispensa, e o texto continua escrito na faixa.
    fone = pg.evaluate("[document.getElementById('aviso-fone-texto').textContent, AVISO_DE_FONE_CELULAR]")
    if fone[0] != fone[1]:
        falhas.append(f"o aviso de fone do celular diz «{fone[0]}», e não a frase do navegador")
    return falhas


# **Por que o último `tocar` não tocou**: o alvo ausente, escondido, ou o que o
# Playwright disse ao desistir (coberto por outro nó, instável, desabilitado).
# O `exigir_toque` o põe na reprovação. «Não há como tocar» sozinho não separa
# uma máquina lenta de um alvo coberto: a reprovação que a F7 viu em 11/10, com
# a máquina carregada, ficou sem causa por isso.
motivo_do_ultimo_toque = ""

PISTAS_DO_PLAYWRIGHT = ("intercepts pointer events", "not stable", "not visible", "not enabled",
                        "detached", "outside of the viewport")


def motivo_do_playwright(erro: Exception) -> str:
    """A linha do registro do Playwright que diz por que ele desistiu, ou a
    primeira do erro."""
    linhas = [linha.strip() for linha in str(erro).splitlines() if linha.strip()]
    pistas = [linha for linha in linhas if any(p in linha for p in PISTAS_DO_PLAYWRIGHT)]
    return (pistas[-1] if pistas else linhas[0] if linhas else "sem mensagem")[:200]


def tocar(pg: Page, seletor: str) -> bool:
    """Um toque no primeiro alvo que casa, se ele estiver à vista.

    Devolve `False` sem tocar quando o alvo não existe ou está escondido: um
    caso que toca num controle ausente diz o que faltou, em vez de esperar o
    tempo-limite do Playwright e estourar. O porquê fica em
    `motivo_do_ultimo_toque`.
    """
    global motivo_do_ultimo_toque
    alvo = pg.locator(seletor).first
    if alvo.count() == 0:
        motivo_do_ultimo_toque = "o alvo não existe"
        return False
    if not alvo.is_visible():
        motivo_do_ultimo_toque = "o alvo está escondido"
        return False
    try:
        # Um alvo coberto, ou que não recebe toque, é um alvo que o dedo não
        # alcança: o Playwright desiste em três segundos, e o caso diz isso.
        # Num computador (a casca larga) não há dedo, e o toque é o clique.
        if pg.evaluate("navigator.maxTouchPoints") > 0:
            alvo.tap(timeout=3000)
        else:
            alvo.click(timeout=3000)
    except ErroDoPlaywright as erro:
        motivo_do_ultimo_toque = motivo_do_playwright(erro)
        return False
    motivo_do_ultimo_toque = ""
    return True


def exigir_toque(pg: Page, seletor: str, falhas: list[str]) -> bool:
    """O `tocar`, e a reprovação quando o alvo não está ao alcance do dedo."""
    if tocar(pg, seletor):
        return True
    falhas.append(f"não há como tocar {seletor}: {motivo_do_ultimo_toque}")
    return False


def ir_para_aba(pg: Page, aba: str, falhas: list[str]) -> bool:
    """Vai à aba como uma pessoa iria: com a CHAMADA aberta, as abas estão
    escondidas, e primeiro se recolhe a CHAMADA pela faixa."""
    if pg.evaluate("document.getElementById('tela-sessao').hasAttribute('data-chamada')"):
        if not exigir_toque(pg, "#voz .voz-sala", falhas):
            return False
        pg.wait_for_timeout(200)
    return exigir_toque(pg, f"#cel-aba-{aba}", falhas)


def esperar_quadro(pg: Page) -> None:
    """A casca relê o `snapshot` a cada 500 ms; uma mudança no quadro do duble
    aparece no redesenho seguinte."""
    pg.wait_for_timeout(700)


def faixa_de_voz(pg: Page) -> list[str]:
    """A moldura do celular: as quatro abas e a faixa de voz (Tarefa 3).

    Termina no estado de partida (dentro da PONTE, na aba CONVERSA, com o áudio
    aberto), que é o que as medidas e a foto leem.
    """
    falhas = []
    # O celular nasce na VOZ (spec, §2.3); o duble vem com o modo do desktop.
    pg.evaluate("SEELE_QUADRO.voice_mode = 'VoiceActivated'")
    esperar_quadro(pg)
    faixa = pg.evaluate("""(() => {
        const v = document.getElementById('voz');
        if (!v) return null;
        return { escondida: v.hidden || !!v.closest('[hidden]'),
                 nome: v.querySelector('.voz-sala-nome')?.textContent ?? '',
                 conta: v.querySelector('.voz-sala-conta')?.textContent ?? '' };
    })()""")
    if faixa is None:
        return ["não há faixa de voz (#voz)"]
    if "PONTE" not in faixa["nome"] or faixa["conta"] != "3/8":
        falhas.append(f"a faixa diz «{faixa['nome']}» «{faixa['conta']}», e não PONTE 3/8")
    # Escondida, não há o que tocar: o resto do caso esperaria o tempo-limite do
    # Playwright e estouraria, em vez de dizer o que reprovou.
    if faixa["escondida"]:
        return falhas + ["dentro da PONTE, a faixa de voz está escondida"]

    pg.evaluate("window.__SEELE_ARGS.set_muted = undefined")
    exigir_toque(pg, "#voz-mudo", falhas)
    pg.wait_for_timeout(300)
    mandou = pg.evaluate("window.__SEELE_ARGS.set_muted ?? null")
    if not mandou or mandou.get("on") is not True:
        falhas.append(f"o toque no MUDO mandou set_muted {mandou}, e não {{on: true}}")

    paineis = {"servidor": ".painel-canais", "conversa": ".painel-linha", "pessoas": ".painel-pessoas"}
    for aba in ("servidor", "conversa", "pessoas", "ajustes"):
        ir_para_aba(pg, aba, falhas)
        estado = pg.evaluate("""(paineis) => {
            const s = document.getElementById('tela-sessao');
            const visivel = (sel) => { const e = s.querySelector('.paineis > ' + sel);
                return !!e && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().width > 0; };
            return { aba: s.dataset.aba,
                     atual: [...document.querySelectorAll('.cel-aba[aria-current="page"]')].map((b) => b.dataset.aba),
                     visiveis: Object.entries(paineis).filter(([, sel]) => visivel(sel)).map(([a]) => a) };
        }""", paineis)
        if estado["aba"] != aba or estado["atual"] != [aba]:
            falhas.append(f"o toque em {aba.upper()} deu data-aba={estado['aba']!r} e aria-current em {estado['atual']}")
        esperado = [aba] if aba in paineis else []
        if estado["visiveis"] != esperado:
            falhas.append(f"na aba {aba.upper()}, os painéis visíveis são {estado['visiveis']}, e não {esperado}")

    # A abertura do áudio (matriz, B-F08b e B-F08c): as palavras vêm do
    # `ABERTURA_DO_AUDIO` do produto, e não de uma cópia.
    for abertura in ("abrindo", "nao_abriu_no_prazo"):
        pg.evaluate(f"SEELE_QUADRO.abertura_do_audio = '{abertura}'")
        esperar_quadro(pg)
        visto = pg.evaluate("""(abertura) => {
            const quem = document.querySelector('#voz .voz-sala-quem')?.textContent ?? '';
            const aviso = document.getElementById('aviso-audio');
            const r = aviso.getBoundingClientRect(), v = document.getElementById('voz').getBoundingClientRect();
            return { quem, rodape: ABERTURA_DO_AUDIO[abertura].rodape, frase: ABERTURA_DO_AUDIO[abertura].frase,
                     aviso: aviso.hidden ? null : document.getElementById('aviso-audio-texto').textContent,
                     dentro: r.left >= 0 && r.right <= innerWidth + 0.5, colado: Math.abs(r.bottom - v.top) <= 1 };
        }""", abertura)
        if visto["rodape"] not in visto["quem"]:
            falhas.append(f"com o áudio em {abertura}, a faixa diz «{visto['quem']}», sem «{visto['rodape']}»")
        if visto["frase"] is not None:
            if visto["aviso"] != visto["frase"]:
                falhas.append(f"com o áudio em {abertura}, o #aviso-audio diz {visto['aviso']!r}")
            # Logo acima da faixa, colado nela, e não só em algum lugar acima:
            # no alto da tela o aviso também estaria «acima», longe da voz que
            # ele explica.
            elif not (visto["dentro"] and visto["colado"]):
                falhas.append(f"o #aviso-audio não fica logo acima da faixa, dentro da tela: {visto}")
    pg.evaluate("SEELE_QUADRO.abertura_do_audio = 'aberto'")

    pg.evaluate("window.__SEELE_CHAMADAS.length = 0")
    exigir_toque(pg, "#voz-sair", falhas)
    pg.wait_for_timeout(300)
    if "leave_voice_room" not in pg.evaluate("window.__SEELE_CHAMADAS"):
        falhas.append("o toque no SAIR não chamou leave_voice_room")

    pg.evaluate("SEELE_QUADRO.voice_rooms.forEach((s) => { s.occupied_by_us = false; })")
    esperar_quadro(pg)
    if not pg.evaluate("document.getElementById('voz').hidden"):
        falhas.append("fora de sala, a faixa de voz continua à vista")
    pg.evaluate("SEELE_QUADRO.voice_rooms[0].occupied_by_us = true")
    ir_para_aba(pg, "conversa", falhas)
    esperar_quadro(pg)
    return falhas


def segurar(pg: Page) -> list[str]:
    """SEGURAR PARA FALAR (Tarefa 4): fala só com o dedo no MUDO, e fecha sozinho.

    Cada jeito de o dedo deixar de segurar tem de terminar com o último
    `set_talking` em `false`: soltar, o sistema cancelar o toque, o dedo sair do
    controle, a página sumir e o enlace cair. Um microfone que fica aberto depois
    do dedo é o pior defeito que esta tela pode ter.
    """
    falhas = []
    pg.evaluate("SEELE_QUADRO.voice_mode = 'PushToTalk'")
    esperar_quadro(pg)
    modo = pg.evaluate("(() => { const m = document.getElementById('voz-mudo'); return [m.dataset.modo, m.textContent]; })()")
    if modo[0] != "segurar" or "SEGURE" not in modo[1]:
        return [f"no PushToTalk, o MUDO é {modo}, e não o SEGURE"]

    apertar = """() => document.getElementById('voz-mudo').dispatchEvent(new PointerEvent('pointerdown',
        { bubbles: true, cancelable: true, pointerId: 7, pointerType: 'touch', isPrimary: true }))"""
    jeitos = {
        "soltar": "() => document.getElementById('voz-mudo').dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 7, pointerType: 'touch' }))",
        "o sistema cancelar o toque": "() => document.getElementById('voz-mudo').dispatchEvent(new Event('touchcancel', { bubbles: true }))",
        "o dedo sair do controle": "() => document.getElementById('voz-mudo').dispatchEvent(new PointerEvent('pointerleave', { pointerId: 7, pointerType: 'touch' }))",
        "a página sumir": """() => { Object.defineProperty(document, 'hidden', { value: true, configurable: true });
            document.dispatchEvent(new Event('visibilitychange'));
            Object.defineProperty(document, 'hidden', { value: false, configurable: true }); }""",
        "o enlace cair": "() => { SEELE_QUADRO.link = { InternalBattery: { remaining_seconds: 300, attempts: 1 } }; }",
    }
    for jeito, soltar in jeitos.items():
        pg.evaluate("window.__SEELE_HISTORICO.length = 0")
        pg.evaluate(apertar)
        # Segura por um quadro inteiro antes de soltar: a fala tem de continuar
        # aberta enquanto o dedo está lá. Soltar antes do quadro escondia um
        # celular que soltava sozinho no primeiro redesenho.
        esperar_quadro(pg)
        no_ar = pg.evaluate("document.getElementById('voz-mudo').textContent")
        segurando = pg.evaluate("window.__SEELE_HISTORICO.filter(([c]) => c === 'set_talking').map(([, a]) => a.talking)")
        if segurando != [True]:
            falhas.append(f"{jeito}: com o dedo ainda no SEGURE, a fala foi {segurando}")
        pg.evaluate(soltar)
        if jeito == "o enlace cair":
            esperar_quadro(pg)
            pg.evaluate("SEELE_QUADRO.link = 'Online'")
            esperar_quadro(pg)
        else:
            pg.wait_for_timeout(150)
        falas = pg.evaluate("window.__SEELE_HISTORICO.filter(([c]) => c === 'set_talking').map(([, a]) => a.talking)")
        if falas[:1] != [True]:
            falhas.append(f"{jeito}: apertar não abriu a fala (set_talking {falas})")
        elif falas[-1] is not False:
            falhas.append(f"{jeito}: o microfone ficou aberto (set_talking {falas})")
        if jeito == "soltar" and "NO AR" not in no_ar:
            falhas.append(f"segurando, o controle diz «{no_ar}», e não NO AR")
        if pg.evaluate("window.__SEELE_HISTORICO.some(([c]) => c === 'set_muted')"):
            falhas.append(f"{jeito}: segurar alternou o MUDO")
    return falhas


def aba_servidor(pg: Page) -> list[str]:
    """A aba SERVIDOR (Tarefa 5): a lista de salas do desktop, em cartões.

    O quadro do duble ganha os três casos que pedem marca: OFICINA lotada,
    SILÊNCIO com senha e uma sala livre, PALCO, para o ENTRAR MUDO.
    """
    falhas = []
    pg.evaluate("""(() => {
        const [, oficina, silencio] = SEELE_QUADRO.voice_rooms;
        oficina.limit = oficina.people.length;
        silencio.password_required = true;
        SEELE_QUADRO.voice_rooms.push({ id: 4, name: "PALCO", limit: 4, password_required: false,
            occupied_by_us: false, channel: null, sync: null, people: [] });
    })()""")
    ir_para_aba(pg, "servidor", falhas)
    esperar_quadro(pg)

    visto = pg.evaluate("""(() => {
        const visivel = (e) => !!e && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
        const topo = document.querySelector('.cel-topo');
        const selo = topo?.querySelector('.cel-selo-seguro');
        const trilha = document.querySelector('.paineis > .trilha');
        const salas = {};
        for (const li of document.querySelectorAll('#lista-voice_rooms > li')) {
            const ir = li.querySelector('button[data-voice_room]');
            const nome = li.querySelector('.voice_room-nome');
            salas[nome.textContent] = {
                marcas: [...li.querySelectorAll('.cel-marca')].map((m) => m.textContent.trim()),
                entrarAtivo: !!ir && !ir.disabled,
                mudo: !!li.querySelector('.cel-entrar-mudo'),
                nomeEhBotao: nome.tagName === 'BUTTON',
            };
        }
        const fora = [...document.querySelectorAll('.painel-canais .cel-fora')].filter(visivel);
        const gestao = ['#nova-voice_room', '#nova-linha', '.voice_room-apagar', '.linha-controles']
            .filter((s) => [...document.querySelectorAll('.painel-canais ' + s)].some(visivel));
        return {
            topo: visivel(topo) ? topo.textContent : null,
            selo: selo ? [selo.textContent, selo.dataset.padrao] : null,
            trilha: visivel(trilha) && trilha.hasAttribute('data-rola-de-lado')
                && trilha.getBoundingClientRect().bottom <= document.querySelector('#lista-voice_rooms').getBoundingClientRect().top + 1,
            salas, fora: fora.map((p) => p.textContent), gestao,
        };
    })()""")
    if not visto["topo"] or "TÓQUIO-3" not in visto["topo"]:
        falhas.append(f"o topo da aba SERVIDOR não diz o servidor: {visto['topo']!r}")
    if not visto["selo"] or "CONEXÃO SEGURA" not in visto["selo"][0] or visto["selo"][1] != "Verified":
        falhas.append(f"o selo da conexão diz {visto['selo']}")
    if not visto["trilha"]:
        falhas.append("a trilha não é uma faixa deitada, marcada para rolar de lado, acima das salas")
    esperadas = {"PONTE": ["VOCÊ ESTÁ AQUI"], "OFICINA": ["LOTADA"], "SILÊNCIO": ["COM SENHA"], "PALCO": []}
    for nome, marcas in esperadas.items():
        sala = visto["salas"].get(nome)
        if sala is None:
            falhas.append(f"não há cartão para {nome}")
            continue
        if sala["marcas"] != marcas:
            falhas.append(f"{nome} tem as marcas {sala['marcas']}, e não {marcas}")
        if not sala["entrarAtivo"]:
            falhas.append(f"o ENTRAR de {nome} está desligado (quem recusa uma sala lotada é o servidor)")
        if sala["mudo"] != (nome != "PONTE"):
            falhas.append(f"o ENTRAR MUDO de {nome} {'falta' if nome != 'PONTE' else 'sobra'}")
        if sala["nomeEhBotao"]:
            falhas.append(f"o nome de {nome} é um botão (renomear é pelo SEELE no computador)")
    if len(visto["fora"]) != 1 or "SALAS E CANAIS" not in visto["fora"][0]:
        falhas.append(f"a frase de SALAS E CANAIS aparece assim: {visto['fora']}")
    if visto["gestao"]:
        falhas.append(f"a gestão de salas e canais continua à vista: {visto['gestao']}")

    def chamadas():
        return pg.evaluate("window.__SEELE_HISTORICO.map(([c, a]) => [c, a])")

    # ENTRAR MUDO: o MUDO antes da entrada, e a CHAMADA abre na aba dela.
    pg.evaluate("window.__SEELE_HISTORICO.length = 0")
    if not tocar(pg, "#lista-voice_rooms > li:has(button[data-voice_room='4']) .cel-entrar-mudo"):
        falhas.append("não há ENTRAR MUDO à vista em PALCO")
    else:
        pg.wait_for_timeout(600)
        feitas = [c for c, _ in chamadas() if c in ("set_muted", "enter_voice_room")]
        args = dict(chamadas())
        if feitas[:2] != ["set_muted", "enter_voice_room"] or (args.get("set_muted") or {}).get("on") is not True:
            falhas.append(f"o ENTRAR MUDO mandou {feitas} ({args.get('set_muted')}), e não set_muted {{on: true}} antes da entrada")
        depois = pg.evaluate("[document.getElementById('tela-sessao').dataset.aba, document.getElementById('vista-chamada').hidden]")
        if depois != ["conversa", False]:
            falhas.append(f"entrar não abriu a CHAMADA na aba dela: aba {depois[0]}, vista-chamada escondida {depois[1]}")

    # A sala com senha: a folha, a senha que ela manda, e o AGORA NÃO que cancela.
    ir_para_aba(pg, "servidor", falhas)
    for resposta in ("entrar", "agora-nao"):
        pg.evaluate("window.__SEELE_HISTORICO.length = 0")
        if not tocar(pg, "#lista-voice_rooms > li:has(button[data-voice_room='3']) button[data-voice_room]"):
            falhas.append("não há ENTRAR à vista em SILÊNCIO")
            break
        pg.wait_for_timeout(200)
        folha = pg.locator("#cel-folha-senha")
        if not folha.is_visible():
            falhas.append("a sala com senha não abriu a folha SENHA DA SALA")
            break
        if resposta == "entrar":
            folha.locator("input").fill("abc")
            exigir_toque(pg, "#cel-folha-senha button[data-resposta='entrar']", falhas)
        else:
            exigir_toque(pg, "#cel-folha-senha button[data-resposta='agora-nao']", falhas)
        pg.wait_for_timeout(400)
        entradas = [a for c, a in chamadas() if c == "enter_voice_room"]
        if resposta == "entrar" and entradas != [{"voiceRoom": 3, "senha": "abc"}]:
            falhas.append(f"o ENTRAR da folha mandou {entradas}")
        if resposta == "agora-nao" and entradas:
            falhas.append(f"o AGORA NÃO da folha ainda tentou entrar: {entradas}")
        if folha.is_visible():
            falhas.append(f"a folha continua aberta depois de {resposta}")
        ir_para_aba(pg, "servidor", falhas)

    # O selo mora no topo de toda aba: o tema e a aba não o tiram de vista.
    ir_para_aba(pg, "conversa", falhas)
    if not pg.evaluate("(() => { const s = document.querySelector('.cel-topo .cel-selo-seguro'); return !!s && s.getBoundingClientRect().height > 0; })()"):
        falhas.append("na aba CONVERSA, o selo da conexão some")
    ir_para_aba(pg, "servidor", falhas)
    esperar_quadro(pg)
    return falhas


def aba_conversa(pg: Page) -> list[str]:
    """A aba CONVERSA (Tarefa 6): a conversa do desktop no tamanho do polegar.

    Termina com uma mensagem própria recusada no fim da lista, que é o que a foto
    e as medidas leem: a placa de alerta dela é do núcleo.
    """
    falhas = []
    tocar(pg, "#cel-aba-conversa")

    def feitas(cmd):
        return pg.evaluate(f"window.__SEELE_HISTORICO.filter(([c]) => c === '{cmd}').map(([, a]) => a)")

    # O Return quebra a fileira, e só o ENVIAR envia: no celular não há Shift.
    pg.evaluate("window.__SEELE_HISTORICO.length = 0")
    campo = pg.locator("#campo-mensagem")
    campo.fill("primeira")
    campo.press("Enter")
    campo.press_sequentially("segunda")
    if feitas("send_message"):
        falhas.append("o Return no campo enviou a mensagem")
    if campo.input_value() != "primeira\nsegunda":
        falhas.append(f"o Return não quebrou a fileira: o campo tem {campo.input_value()!r}")
    if not tocar(pg, "#botao-enviar"):
        falhas.append("não há ENVIAR à vista")
    pg.wait_for_timeout(300)
    if [a.get("body") for a in feitas("send_message")] != ["primeira\nsegunda"]:
        falhas.append(f"o ENVIAR mandou {feitas('send_message')}")
    placeholder = pg.evaluate("document.getElementById('campo-mensagem').placeholder")
    if "Enter" in placeholder:
        falhas.append(f"o campo ainda promete que o Enter envia: {placeholder!r}")

    # A folha da mensagem: COPIAR em todas, APAGAR só nas próprias, e a frase de
    # REMOVER nas dos outros só para quem tem a permissão.
    pg.evaluate("navigator.clipboard.writeText = async (texto) => { window.__copiado = texto; }")
    propria = "#lista-mensagens > li.mensagem:has(.corpo:text-is('UDP. tá escrito no README, duas linhas depois do comando'))"
    alheia = "#lista-mensagens > li.mensagem:has(.mensagem-autor:text-is('juli'))"
    for nome, seletor, pode_remover in (("própria", propria, True), ("alheia", alheia, True), ("alheia sem permissão", alheia, False)):
        pg.evaluate(f"SEELE_QUADRO.may_remove_message = {'true' if pode_remover else 'false'}")
        esperar_quadro(pg)
        if not tocar(pg, f"{seletor} .corpo"):
            falhas.append(f"não há mensagem {nome} à vista")
            continue
        folha = pg.locator("#cel-folha-mensagem")
        if not folha.is_visible():
            falhas.append(f"o toque na mensagem {nome} não abriu a folha dela")
            continue
        tem = pg.evaluate("""(() => { const f = document.getElementById('cel-folha-mensagem');
            return { copiar: !!f.querySelector('[data-acao="copiar"]'), apagar: !!f.querySelector('[data-acao="apagar"]'),
                     remover: /REMOVER/.test(f.querySelector('.cel-fora')?.textContent ?? '') }; })()""")
        esperado = {"copiar": True, "apagar": nome == "própria", "remover": nome == "alheia"}
        if tem != esperado:
            falhas.append(f"a folha da mensagem {nome} tem {tem}, e não {esperado}")
        if nome == "própria":
            tocar(pg, "#cel-folha-mensagem [data-acao='copiar']")
            copiado = pg.evaluate("window.__copiado ?? null")
            if copiado != "UDP. tá escrito no README, duas linhas depois do comando":
                falhas.append(f"COPIAR O TEXTO copiou {copiado!r}")
            tocar(pg, f"{seletor} .corpo")
            tocar(pg, "#cel-folha-mensagem [data-acao='apagar']")
            pg.wait_for_timeout(200)
            if not pg.evaluate("!document.getElementById('moderar').hidden"):
                falhas.append("APAGAR A MINHA MENSAGEM não abriu a confirmação do desktop")
            pg.evaluate("document.getElementById('moderar').hidden = true")
        else:
            pg.keyboard.press("Escape")
        if folha.is_visible():
            falhas.append(f"a folha da mensagem {nome} não fechou")

    # O + abre a folha ANEXAR; as duas escolhas pedem o arquivo pelo caminho do
    # desktop, e quem abre o seletor do navegador é a ponte da F4.
    pg.evaluate("window.__SEELE_HISTORICO.length = 0")
    if not tocar(pg, ".cel-anexar"):
        falhas.append("não há + à vista no compositor")
    elif not pg.locator("#cel-folha-anexar").is_visible():
        falhas.append("o + não abriu a folha ANEXAR")
    else:
        escolhas = pg.evaluate("[...document.querySelectorAll('#cel-folha-anexar [data-escolha]')].map((b) => b.dataset.escolha)")
        if escolhas != ["midia", "arquivo"]:
            falhas.append(f"a folha ANEXAR oferece {escolhas}, e não FOTO OU VÍDEO e ARQUIVO")
        tocar(pg, "#cel-folha-anexar [data-escolha='arquivo']")
        pg.wait_for_timeout(200)
        if not feitas("escolher_arquivo"):
            falhas.append("ARQUIVO não pediu o arquivo pelo caminho do desktop (escolher_arquivo)")

    # A mensagem que falhou: a placa de alerta, com as duas saídas do desktop.
    pg.evaluate("""(() => {
        SEELE_RESPOSTAS.messages.push({ id: 6, channel: 1, author: 1, author_nickname: "rafa", at_seconds: 1756000700,
            body: "alguém tem o link do mapa?", own: true, edited: false, attachment: null,
            estado: { Recusada: "PermissionDenied" }, client_message_id: "c-1" });
        SEELE_QUADRO.messages_revision += 1;
    })()""")
    esperar_quadro(pg)
    placa = pg.evaluate("""(() => {
        const p = document.querySelector('#lista-mensagens [data-envio="falhou"]');
        if (!p) return null;
        const r = p.getBoundingClientRect();
        return { acoes: [...p.querySelectorAll('button[data-acao]')].map((b) => [b.dataset.acao, Math.round(b.getBoundingClientRect().height)]),
                 fundo: getComputedStyle(p).backgroundColor,
                 alerta: getComputedStyle(document.documentElement).getPropertyValue('--seele-vermelho-fraco').trim(),
                 dentro: r.left >= 0 && r.right <= innerWidth + 0.5 };
    })()""")
    if placa is None:
        falhas.append("a mensagem recusada não mostra a placa de alerta")
    else:
        if [a for a, _ in placa["acoes"]] != ["reenviar", "descartar"] or any(h < 44 for _, h in placa["acoes"]):
            falhas.append(f"a placa da mensagem recusada tem {placa['acoes']}, e não TENTAR DE NOVO e DESISTIR de 44")
        if not placa["dentro"]:
            falhas.append("a placa da mensagem recusada passa da tela")
    pg.locator("#lista-mensagens").evaluate("(l) => { l.scrollTop = l.scrollHeight; }")
    pg.wait_for_timeout(200)
    return falhas


def aba_pessoas(pg: Page) -> list[str]:
    """A aba PESSOAS (Tarefa 7): a lista do desktop, e a folha de cada pessoa."""
    falhas = []
    tocar(pg, "#cel-aba-pessoas")
    pg.wait_for_timeout(200)
    linhas = pg.evaluate("""(() => [...document.querySelectorAll('#lista-roster li.pessoa')].map((li) => {
        const visivel = (e) => !!e && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
        return { nome: li.querySelector('.pessoa-nome')?.textContent ?? '', altura: Math.round(li.getBoundingClientRect().height),
                 numero: visivel(li.querySelector('.pessoa-sync-valor')), barra: visivel(li.querySelector('.barra')),
                 palavra: visivel(li.querySelector('.pastilha')), volume: visivel(li.querySelector('input.volume')) };
    }))()""")
    if len(linhas) < 4:
        falhas.append(f"a aba PESSOAS mostra {len(linhas)} pessoas, e não 4")
    for l in linhas:
        if l["altura"] < 56:
            falhas.append(f"a linha de {l['nome']} tem {l['altura']} de altura, e não 56")
        if not (l["numero"] and l["barra"] and l["palavra"]):
            falhas.append(f"o sinal de {l['nome']} não tem número, barra e palavra: {l}")
        if l["volume"]:
            falhas.append(f"o volume de {l['nome']} aparece na linha (ele mora na folha da pessoa)")

    def abrir(nome):
        if not tocar(pg, f"#lista-roster li.pessoa:has(.pessoa-nome:text-is('{nome}'))"):
            return None
        pg.wait_for_timeout(150)
        if not pg.locator("#cel-folha-pessoa").is_visible():
            return None
        return pg.evaluate("""(() => { const f = document.getElementById('cel-folha-pessoa');
            return { volume: !!f.querySelector('input[type=range]'), rotulo: f.querySelector('.cel-folha-volume')?.textContent ?? '',
                     moderar: /MODERAR/.test(f.querySelector('.cel-fora')?.textContent ?? ''),
                     moderacaoDoDesktop: !document.getElementById('moderar').hidden,
                     foco: f.contains(document.activeElement) }; })()""")

    folha = abrir("juli")
    if folha is None:
        falhas.append("o toque em juli não abriu a folha da pessoa")
    else:
        # Uma folha é um diálogo modal: o foco entra nela, e o leitor da tela lê
        # a folha, e não a página de trás (Tarefa 15, na árvore do WebKit).
        if not folha["foco"]:
            falhas.append("a folha de juli abriu e o foco ficou na página de trás")
        if not folha["volume"] or "100%" not in folha["rotulo"]:
            falhas.append(f"a folha de juli não mostra o volume com o número: {folha}")
        if not folha["moderar"]:
            falhas.append("com may_kick, a folha de juli não diz onde se modera")
        if folha["moderacaoDoDesktop"]:
            falhas.append("o toque abriu também a moderação do desktop")
        pg.evaluate("window.__SEELE_HISTORICO.length = 0")
        pg.evaluate("""(() => { const v = document.querySelector('#cel-folha-pessoa input[type=range]');
            v.value = '150'; v.dispatchEvent(new Event('input', { bubbles: true })); })()""")
        pg.wait_for_timeout(200)
        do_desktop = pg.evaluate("document.querySelector(\"#lista-roster input.volume[data-pessoa='juli']\")?.value ?? null")
        volumes = pg.evaluate("window.__SEELE_HISTORICO.filter(([c]) => c === 'set_volume').map(([, a]) => a)")
        rotulo = pg.evaluate("document.querySelector('#cel-folha-pessoa .cel-folha-volume')?.textContent ?? ''")
        if do_desktop != "150" or {"nickname": "juli", "percent": 150} not in volumes:
            falhas.append(f"o volume da folha não chegou ao desktop: input {do_desktop}, set_volume {volumes}")
        if "150%" not in rotulo:
            falhas.append(f"o número do volume não acompanhou: {rotulo!r}")
        pg.keyboard.press("Escape")

    propria = abrir("rafa (você)")
    if propria is None:
        falhas.append("o toque em rafa não abriu a folha")
    elif propria["volume"] or propria["moderar"]:
        falhas.append(f"a folha de quem usa o aparelho tem volume ou moderar: {propria}")
    elif not propria["foco"]:
        falhas.append("a folha de rafa abriu e o foco ficou na página de trás")
    pg.keyboard.press("Escape")

    pg.evaluate("SEELE_QUADRO.may_kick = SEELE_QUADRO.may_ban = SEELE_QUADRO.may_move_person = false")
    esperar_quadro(pg)
    sem = abrir("vitor")
    if sem is None:
        falhas.append("o toque em vitor não abriu a folha")
    elif sem["moderar"]:
        falhas.append("sem nenhuma permissão de moderar, a folha ainda fala em moderar")
    pg.keyboard.press("Escape")
    pg.wait_for_timeout(150)
    return falhas


def chamada(pg: Page) -> list[str]:
    """A CHAMADA (Tarefa 8): por cima da sessão, com a faixa no mesmo lugar.

    O duble ganha duas transmissões na sala, e a de juli assistida. Termina
    com a CHAMADA aberta, que é o que a foto e as medidas leem.
    """
    falhas = []
    pg.evaluate("""(() => {
        SEELE_QUADRO.transmissoes = [
            { tela: 1, de: 2, e_minha: false, assistida: true, espectadores: 2 },
            { tela: 2, de: 3, e_minha: false, assistida: false, espectadores: 1 }];
        SEELE_QUADRO.tela = { tela: 1, de: 2, e_minha: false, espectadores: 2, parada: null, pedido: null };
    })()""")
    esperar_quadro(pg)
    caixa = "(s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null; const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; }"
    mudo_antes = pg.evaluate(caixa, "#voz-mudo")
    abas_antes = pg.evaluate(caixa, ".cel-abas")

    if not tocar(pg, "#voz .voz-sala"):
        return ["não há como abrir a CHAMADA pela faixa"]
    esperar_quadro(pg)
    visto = pg.evaluate("""(caixa) => {
        const c = new Function('s', 'return (' + caixa + ')(s)');
        return { aberta: document.getElementById('tela-sessao').hasAttribute('data-chamada'),
                 vista: c('#vista-chamada'), topo: c('.cel-topo'), voz: c('#voz'), mudo: c('#voz-mudo'),
                 abas: c('.cel-abas'), telemetria: c('.cel-telemetria'),
                 palavras: [...document.querySelectorAll('.cel-telemetria .rotulo')].map((r) => r.textContent),
                 sala: document.querySelector('#voz .voz-sala')?.textContent ?? '' };
    }""", caixa)
    if not visto["aberta"] or not visto["vista"]:
        falhas.append("o toque na faixa não abriu a CHAMADA")
    else:
        v, t, z = visto["vista"], visto["topo"], visto["voz"]
        largura = pg.evaluate("innerWidth")
        if v[2] < largura - 1 or abs(v[1] - (t[1] + t[3])) > 1 or abs(v[1] + v[3] - z[1]) > 1:
            falhas.append(f"a CHAMADA não cobre a sessão entre o topo e a faixa: {v}, topo {t}, faixa {z}")
    if visto["mudo"] != mudo_antes:
        falhas.append(f"o MUDO mudou de lugar com a CHAMADA: {mudo_antes} → {visto['mudo']}")
    if visto["abas"] is not None or visto["telemetria"] is None:
        falhas.append(f"a telemetria não tomou o lugar das abas: abas {visto['abas']}, telemetria {visto['telemetria']}")
    elif abas_antes and (abs(visto["telemetria"][1] - abas_antes[1]) > 1 or abs(visto["telemetria"][3] - abas_antes[3]) > 1):
        falhas.append(f"a telemetria não ocupa o lugar exato das abas: {visto['telemetria']} contra {abas_antes}")
    if visto["palavras"] != ["ATRASO", "JITTER", "PERDA", "CODEC"]:
        falhas.append(f"a telemetria da CHAMADA diz {visto['palavras']}")
    if "RECOLHER" not in visto["sala"]:
        falhas.append(f"na CHAMADA, a faixa diz «{visto['sala']}», e não RECOLHER")

    palco = pg.evaluate("""(() => {
        const alto = (e) => e && !e.closest('[hidden]') ? Math.round(e.getBoundingClientRect().height) : null;
        return { escolha: [...document.querySelectorAll('#palco-escolha button')].map((b) => [b.textContent.trim(), alto(b)]),
                 controles: ['#palco-mudo', '#palco-volume', '#palco-parar-de-assistir'].map((s) => [s, alto(document.querySelector(s))]),
                 colunas: getComputedStyle(document.getElementById('chamada-grade')).gridTemplateColumns.split(' ').length };
    })()""")
    if len(palco["escolha"]) != 3 or "NÃO VER" not in palco["escolha"][-1][0]:
        falhas.append(f"com duas transmissões, a escolha mostra {palco['escolha']}")
    # Cada botão diz de quem é a tela, pelo nome dos presentes.
    rotulos = " ".join(texto for texto, _ in palco["escolha"])
    if "juli" not in rotulos or "vitor" not in rotulos or "—" in rotulos:
        falhas.append(f"a escolha não diz de quem é cada tela: {rotulos!r}")
    for texto, alto in palco["escolha"]:
        if alto is not None and alto < 44:
            falhas.append(f"«{texto}» na escolha tem {alto} de altura")
    for seletor, alto in palco["controles"]:
        if alto is None or alto < 44:
            falhas.append(f"{seletor} no palco tem {alto} de altura, e não 44")
    if palco["colunas"] != 2:
        falhas.append(f"a grade da CHAMADA tem {palco['colunas']} colunas, e não 2")

    if not tocar(pg, "#chamada-grade > li[data-pessoa='juli']"):
        falhas.append("não há cartão de juli à vista na grade")
    elif not pg.locator("#cel-folha-pessoa").is_visible():
        falhas.append("o toque no cartão de juli não abriu a folha dela")
    pg.keyboard.press("Escape")

    tocar(pg, "#voz .voz-sala")
    esperar_quadro(pg)
    fechada = pg.evaluate("[document.getElementById('vista-chamada').hidden, document.getElementById('tela-sessao').hasAttribute('data-chamada')]")
    if fechada != [True, False]:
        falhas.append(f"RECOLHER não fechou a CHAMADA: vista escondida {fechada[0]}, marca {fechada[1]}")
    tocar(pg, "#voz .voz-sala")
    esperar_quadro(pg)
    return falhas


def capa_na_chamada(pg: Page) -> list[str]:
    """A capa de uma pessoa no cartão dela na CHAMADA do celular (D-capa-1:
    animada também aqui, até o mesmo teto do desktop). O cartão é o do
    desktop (`pintarCartao` e o `vestirCapa` da F3); a casca só o põe em duas
    colunas. Confere que a capa veste o cartão inteiro e está entre as que
    animam, e que todo texto fica acima de 4,5:1 com o mesmo contraste contra
    uma capa branca, uma preta e uma da cor da tinta: nenhum texto encosta na
    capa, todos ficam sobre as placas do desktop.

    O contraste não é o de sem capa, como a F3 exige no desktop: sem capa, o
    cartão do celular é do painel, falando ou calado; com capa, a placa de
    quem fala é a laranja do desktop (`--fundo-da-placa`). O nome cai de 5,71
    a 5,12 e SINAL de 5,52 a 4,94 (medido em 10/10), iguais aos do desktop
    com capa."""
    falhas = []
    juli = pg.evaluate("String(SEELE_QUADRO.voice_rooms.flatMap((s) => s.people).find((p) => p.nickname === 'juli')?.id)")
    if not tocar(pg, "#voz .voz-sala"):
        return ["não há como abrir a CHAMADA pela faixa"]
    esperar_quadro(pg)
    cartao = f"#chamada-grade .chamada-cartao[data-pessoa-da-capa='{juli}']"
    pg.evaluate("entrarNaGeracao(1)")
    pg.evaluate(MOD_DE_PE)
    pg.evaluate("""(alvo) => { const c = document.createElement('canvas'); c.width = 96; c.height = 32;
        const x = c.getContext('2d'); x.fillStyle = '#32e6a2'; x.fillRect(0, 0, 96, 32);
        SEELE_RESPOSTAS.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: c.toDataURL('image/jpeg') };
        return __pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'pessoa.capa', modo: 'substituir', alvo,
            conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:juli' } } } } }); }""", juli)
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    vestido = pg.evaluate("""(s) => { const p = document.querySelector(s); const c = p?.querySelector(':scope > .capa-camada');
        const b = c?.getBoundingClientRect();
        return { vestido: p?.dataset.comCapa === 'sim', animada: p?.dataset.capaAnimada === 'sim',
                 imagem: c ? getComputedStyle(c).backgroundImage.startsWith('url(') : false,
                 cobre: !!b && b.width > 0 && Math.abs(b.width - p.clientWidth) < 1 && Math.abs(b.height - p.clientHeight) < 1 }; }""", cartao)
    if vestido != {"vestido": True, "animada": True, "imagem": True, "cobre": True}:
        falhas.append(f"a capa de juli não veste o cartão dela na CHAMADA, ou não está entre as que animam: {vestido}")
        return falhas
    medidas = {nome: pg.evaluate(CONTRASTE_EM, [cartao, cor]) or {} for nome, cor in CAPAS_DE_PROVA}
    for nome, medida in medidas.items():
        baixos = {k: v for k, v in medida.items() if v < 4.5}
        if baixos:
            falhas.append(f"sobre uma capa {nome}, texto do cartão abaixo de 4,5:1: {baixos}")
    if len({tuple(sorted(m.items())) for m in medidas.values()}) != 1:
        falhas.append(f"um texto do cartão encosta na capa: o contraste muda com ela {medidas}")
    return falhas


def palco_de_mods(pg: Page) -> list[str]:
    """O palco dos MODs na CHAMADA (`chamada.palco`, c7c64814 da F3): a vista
    alternativa à grade, que começa atrás. Um MOD o contribui pelo roteador;
    VER O PALCO aparece na barra da chamada, com 44 de alvo, e o traz à
    frente. O palco toma a CHAMADA do que vem antes dele (a barra, e o palco
    da transmissão, que é o aviso de quem compartilha e fica como no desktop)
    até a faixa, sem a grade, com
    o que o MOD desenhou à vista e recebendo o toque, e o MOD ouve o `lugar`
    com o tamanho do miolo. VER A GRADE, também com 44, devolve a grade sem
    desmontar o palco. Termina com o palco à frente, para a foto."""
    falhas = []
    # O `lugar` só vai a um MOD de uma geração de pé, e o duble não entra em
    # nenhuma (a mesma entrada da `bancada/palco-da-chamada.cjs` da F3).
    pg.evaluate("entrarNaGeracao(1)")
    pg.evaluate(MOD_DE_PE)
    pg.evaluate("""__pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'chamada.palco', modo: 'substituir',
        conteudo: [{ forma: 'texto', chave: 't', dentro: 'SONDA DO PALCO' }, { forma: 'botao', chave: 'b', dentro: 'APERTE' }] } })
        .then(() => (window.__entregues ?? []).filter((m) => m.tipo === 'resposta').at(-1))""")
    if not tocar(pg, "#voz .voz-sala"):
        return ["não há como abrir a CHAMADA pela faixa"]
    esperar_quadro(pg)
    alvo = """(s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null;
        const b = e.getBoundingClientRect(); const c = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
        return { altura: Math.round(b.height), noAlto: !!c && (e.contains(c) || c.contains(e)) }; }"""
    ver = pg.evaluate(alvo, "#palco-de-mods-ver-o-palco")
    if not ver or not ver["noAlto"] or ver["altura"] < 44:
        falhas.append(f"com um palco de MOD na sala, VER O PALCO não está ao alcance: {ver}")
        return falhas
    pg.evaluate("window.__entregues.length = 0")
    tocar(pg, "#palco-de-mods-ver-o-palco")
    pg.wait_for_timeout(400)
    visto = pg.evaluate("""() => {
        const r = (s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null;
            const b = e.getBoundingClientRect(); return [Math.round(b.top), Math.round(b.bottom), Math.round(b.width), Math.round(b.height)]; };
        const sonda = [...document.querySelectorAll('#palco-de-mods-miolo *')].find((e) => e.children.length === 0 && e.textContent.trim() === 'SONDA DO PALCO');
        const sb = sonda?.getBoundingClientRect();
        const noCentro = sb && document.elementFromPoint(sb.left + sb.width / 2, sb.top + sb.height / 2);
        const lugar = (window.__entregues ?? []).filter((m) => m.nome === 'lugar' && m.regiao === 'chamada.palco').at(-1);
        const miolo = document.getElementById('palco-de-mods-miolo');
        const antes = document.getElementById('palco-de-mods').previousElementSibling;
        let anterior = antes;
        while (anterior && (anterior.hidden || getComputedStyle(anterior).display === 'none')) anterior = anterior.previousElementSibling;
        return { palco: r('#palco-de-mods'), anterior: anterior ? [anterior.id || anterior.className, Math.round(anterior.getBoundingClientRect().bottom)] : null,
                 grade: r('#chamada-grade'), voz: r('#voz'),
                 vista: r('#vista-chamada'), miolo: [miolo.clientWidth, miolo.clientHeight],
                 sonda: !!noCentro && (sonda.contains(noCentro) || noCentro.contains(sonda)),
                 lugar: lugar ? [lugar.largura, lugar.altura] : null,
                 quem: document.getElementById('palco-de-mods-quem').textContent,
                 vaza: document.documentElement.scrollWidth > innerWidth }; }""")
    if not visto["palco"]:
        falhas.append("VER O PALCO não trouxe o palco à frente")
        return falhas
    if visto["grade"]:
        falhas.append(f"com o palco à frente, a grade continua à vista: {visto['grade']}")
    p, a, z, v = visto["palco"], visto["anterior"], visto["voz"], visto["vista"]
    if a and abs(p[0] - a[1]) > 1:
        falhas.append(f"o palco não começa logo abaixo de {a[0]}: palco {p}, {a[0]} termina em {a[1]}")
    if abs(p[1] - v[1]) > 1 or p[1] > z[0] + 1:
        falhas.append(f"o palco não vai até a faixa, ou passa dela: palco {p}, vista {v}, faixa {z}")
    if visto["miolo"][1] < 200:
        falhas.append(f"o miolo do palco tem {visto['miolo'][1]} de altura")
    if not visto["sonda"]:
        falhas.append("o que o MOD desenhou no palco não está à vista ou não recebe o toque")
    if visto["lugar"] != visto["miolo"]:
        falhas.append(f"o MOD ouviu o lugar {visto['lugar']}, e o miolo tem {visto['miolo']}")
    if "teste/bancada" not in visto["quem"]:
        falhas.append(f"a barra do palco não diz de quem ele é: «{visto['quem']}»")
    if visto["vaza"]:
        falhas.append("com o palco à frente, a página rola de lado")
    grade = pg.evaluate(alvo, "#palco-de-mods-ver-a-grade")
    if not grade or not grade["noAlto"] or grade["altura"] < 44:
        falhas.append(f"VER A GRADE não está ao alcance: {grade}")
    else:
        tocar(pg, "#palco-de-mods-ver-a-grade")
        pg.wait_for_timeout(400)
        depois = pg.evaluate("""() => ({ palco: !document.getElementById('palco-de-mods').hidden,
            montado: document.getElementById('palco-de-mods-miolo').childElementCount > 0,
            grade: getComputedStyle(document.getElementById('chamada-grade')).display !== 'none',
            volta: !document.getElementById('palco-de-mods-ver-o-palco').hidden })""")
        if depois != {"palco": False, "montado": True, "grade": True, "volta": True}:
            falhas.append(f"VER A GRADE não devolveu a grade com o palco montado e a volta à vista: {depois}")
        tocar(pg, "#palco-de-mods-ver-o-palco")
        pg.wait_for_timeout(300)
    return falhas


def ajustes_medida(pg: Page) -> list[str]:
    """A medida da Tarefa 9: as seções de `#tela-server` viram páginas da aba
    AJUSTES só com o CSS e o celular-ajustes.js, sem mudar o tela-server.js?"""
    falhas = []
    if not ir_para_aba(pg, "ajustes", falhas):
        return falhas
    pg.wait_for_timeout(500)
    caixa = """(s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null;
        const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.bottom)]; }"""
    camada, topo, faixa = (pg.evaluate(caixa, s) for s in ("#tela-server", ".cel-topo", "#voz"))
    largura = pg.evaluate("innerWidth")
    if camada is None:
        return falhas + ["tocar AJUSTES não abriu a tela de configurações"]
    if camada[2] < largura - 1 or abs(camada[1] - topo[3]) > 1 or abs(camada[3] - faixa[1]) > 1:
        falhas.append(f"a tela de AJUSTES não fica entre o topo e a faixa: {camada}, topo {topo}, faixa {faixa}")
    if pg.evaluate("getComputedStyle(document.getElementById('tela-server'), '::before').display") != "none":
        falhas.append("a tela de AJUSTES ainda tem o véu de camada")
    lista = pg.evaluate(caixa, "#tela-server .server-coluna"), pg.evaluate(caixa, "#tela-server .server-corpo")
    if lista[0] is None or lista[1] is not None:
        falhas.append(f"a primeira página não é a lista de seções: coluna {lista[0]}, corpo {lista[1]}")
    if not exigir_toque(pg, "#secao-audio", falhas):
        return falhas
    pg.wait_for_timeout(300)
    secao = pg.evaluate("""(() => ({
        coluna: getComputedStyle(document.querySelector('#tela-server .server-coluna')).display,
        corpo: getComputedStyle(document.querySelector('#tela-server .server-corpo')).display,
        paineis: [...document.querySelectorAll('#tela-server .server-painel > [id^="painel-"]')].filter((p) => !p.hidden).map((p) => p.id),
        voltar: !!document.querySelector('#tela-server .cel-ajustes-voltar')?.offsetParent }))()""")
    if secao["coluna"] != "none" or secao["corpo"] == "none" or secao["paineis"] != ["painel-audio"] or not secao["voltar"]:
        falhas.append(f"a seção ÁUDIO não abriu sozinha, numa coluna, com «‹ AJUSTES»: {secao}")
    # A faixa de contexto diz como o microfone abre (CASCA.fraseDoModo): no
    # celular, o botão que se segura, e não a tecla que ele não tem. Achado na
    # foto do Safari: «microfone abre enquanto segura ESPAÇO».
    pg.wait_for_timeout(600)
    contexto = pg.evaluate("document.getElementById('server-contexto').textContent")
    if "SEGURE" not in contexto or "ESPAÇO" in contexto:
        falhas.append(f"a faixa de contexto de ÁUDIO diz «{contexto}», e não o SEGURE")
    exigir_toque(pg, "#tela-server .cel-ajustes-voltar", falhas)
    if pg.evaluate("document.getElementById('tela-server').getAttribute('data-cel-pagina')") != "lista":
        falhas.append("«‹ AJUSTES» não voltou à lista")
    if not exigir_toque(pg, "#voz-mudo", falhas):
        falhas.append("com AJUSTES aberto, o MUDO não está ao alcance")
    ir_para_aba(pg, "conversa", falhas)
    pg.wait_for_timeout(300)
    if not pg.evaluate("document.getElementById('tela-server').hidden"):
        falhas.append("sair da aba AJUSTES não fechou a tela de configurações")
    ir_para_aba(pg, "ajustes", falhas)
    pg.wait_for_timeout(500)
    return falhas


ORDEM_DE_AJUSTES = ["ÁUDIO", "APARÊNCIA", "ESTE SERVIDOR", "MODS DESTE SERVIDOR", "CONEXÃO", "IDENTIDADE",
                    "O QUE O CELULAR NÃO FAZ", "COMO ESTA PÁGINA ABRE", "SOBRE", "SAIR DO SERVIDOR"]


def aba_ajustes(pg: Page) -> list[str]:
    """A aba AJUSTES (Tarefa 9): SEU PERFIL, as seções na ordem do spec, CONEXÃO,
    o que o celular não faz, e as ações que o desktop já tem."""
    falhas = []
    if not ir_para_aba(pg, "ajustes", falhas):
        return falhas
    pg.wait_for_timeout(500)
    lista = pg.evaluate("""(() => {
        const visivel = (e) => !!e && !e.closest('[hidden]') && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
        const coluna = document.querySelector('#tela-server .server-coluna');
        const perfil = coluna.querySelector('.cel-perfil');
        const itens = [...coluna.querySelectorAll('button')].filter(visivel)
            .filter((b) => !b.closest('.cel-perfil')).map((b) => (b.querySelector('.server-secao-nome') ?? b).textContent.trim());
        const primeiro = [...coluna.querySelectorAll('*')].find(visivel);
        return { perfil: visivel(perfil) ? perfil.textContent : null, perfilNoAlto: !!perfil && perfil.contains(primeiro) || primeiro === perfil, itens };
    })()""")
    if not lista["perfil"] or "rafa" not in lista["perfil"] or not lista["perfilNoAlto"]:
        falhas.append(f"SEU PERFIL não está no alto de AJUSTES: {lista['perfil']!r}")
    if lista["itens"] != ORDEM_DE_AJUSTES:
        falhas.append(f"AJUSTES mostra {lista['itens']}, e não {ORDEM_DE_AJUSTES}")

    def abrir(id_):
        ok = exigir_toque(pg, f"#tela-server #{id_}", falhas)
        pg.wait_for_timeout(250)
        return ok

    def voltar():
        exigir_toque(pg, "#tela-server .cel-ajustes-voltar", falhas)

    if abrir("cel-secao-conexao"):
        medidas = pg.evaluate("[...document.querySelectorAll('#cel-painel-conexao .cel-medida')].map((m) => [m.querySelector('.rotulo').textContent, m.querySelector('.cel-medida-valor').textContent.trim()])")
        rotulos = [r for r, _ in medidas]
        if rotulos != ["ATRASO", "JITTER", "PERDA", "CODEC", "CAMINHO", "UPTIME", "SINAL DA SALA"] or any(not v for _, v in medidas):
            falhas.append(f"CONEXÃO mostra {medidas}")
        voltar()

    if abrir("cel-secao-fora"):
        fora = pg.evaluate("""(() => [...document.querySelectorAll('#cel-painel-fora .cel-fora-grupo')].map((g) => ({
            grupo: g.querySelector('.rotulo').textContent, itens: g.querySelectorAll('li').length,
            link: g.querySelector('a.cel-baixar')?.href ?? null })))()""")
        if [g["grupo"] for g in fora] != ["SÓ NO APP DO SEELE", "NO COMPUTADOR"] or [g["itens"] for g in fora] != [8, 6]:
            falhas.append(f"O QUE O CELULAR NÃO FAZ mostra {fora}")
        elif fora[0]["link"] != "https://seele.app.br/#baixar":
            falhas.append(f"HOSPEDAR UM SERVIDOR não leva a quem baixa o app: {fora[0]['link']}")
        voltar()

    # Tocar de novo na aba AJUSTES volta à raiz.
    if abrir("secao-aparencia"):
        exigir_toque(pg, "#cel-aba-ajustes", falhas)
        if pg.evaluate("document.getElementById('tela-server').getAttribute('data-cel-pagina')") != "lista":
            falhas.append("tocar de novo na aba AJUSTES não voltou à raiz")

    if abrir("secao-audio"):
        modos = pg.evaluate("[...document.querySelectorAll('#painel-audio .server-modo')].filter((b) => getComputedStyle(b).display !== 'none').map((b) => [b.dataset.modo, b.querySelector('.server-modo-nome').textContent])")
        if modos != [["PushToTalk", "SEGURAR PARA FALAR"], ["VoiceActivated", "VOZ"]]:
            falhas.append(f"o modo de voz em ÁUDIO mostra {modos}, e não SEGURAR PARA FALAR e VOZ")
        pg.evaluate("window.__SEELE_HISTORICO.length = 0")
        exigir_toque(pg, "#server-modo-tecla", falhas)
        pg.wait_for_timeout(300)
        modo = pg.evaluate("window.__SEELE_HISTORICO.filter(([c]) => c === 'set_voice_mode').map(([, a]) => a)")
        if {"mode": "PushToTalk"} not in modo:
            falhas.append(f"SEGURAR PARA FALAR mandou {modo}, e não set_voice_mode PushToTalk")
        voltar()

    if exigir_toque(pg, "#tela-server .cel-perfil-editar", falhas):
        pg.wait_for_timeout(300)
        if not pg.evaluate("!document.getElementById('perfil').hidden"):
            falhas.append("EDITAR PERFIL não abriu o perfil")
        pg.evaluate("fecharPerfil()")
        pg.wait_for_timeout(200)

    pg.evaluate("window.__SEELE_HISTORICO.length = 0")
    if exigir_toque(pg, "#tela-server .cel-sair-do-servidor", falhas):
        pg.wait_for_timeout(500)
        if not pg.evaluate("window.__SEELE_HISTORICO.some(([c]) => c === 'disconnect')"):
            falhas.append("SAIR DO SERVIDOR não saiu do servidor")
    return falhas


# O que não cresce com o texto do sistema de propósito, e por quê. Todo o resto
# cresce: o caso `texto-maior` reprova o texto à vista que ficar no tamanho. Os
# mesmos de `NO_TAMANHO_DO_DESKTOP`, em `tests/celular.rs`, cujo gêmeo no
# `celular-texto.css` repete o tamanho, mais os tetos escritos à mão.
TEXTO_COM_TETO = {
    ".cel-abas": "as abas têm teto de 13 px, como a barra de abas do iOS (spec, §5)",
    ".cel-telemetria": "a telemetria da CHAMADA tem a altura das abas e teto como elas: quatro medidas em 390",
    ".trilha-item, .trilha-mais, .mensagem-avatar, .chamada-avatar":
        "as iniciais numa caixa de tamanho fixo são marcas, e não texto de ler",
    ".barra, .sync-marca, .chamada-cartao-barra, .chamada-cartao-marca":
        "a barra do sinal é desenho; o número e a palavra ao lado crescem",
}

TAMANHOS_DO_TEXTO = """(seletorDoTeto) => {
    const raiz = document.documentElement;
    const visivel = (e) => { for (let n = e; n; n = n.parentElement) { const s = getComputedStyle(n);
        if (n.hidden || s.display === 'none' || s.visibility === 'hidden') return false; } return e.getBoundingClientRect().width > 0; };
    const nos = [...document.querySelectorAll('#tela-sessao *, #tela-server *')].filter((e) =>
        ([...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())
         || (/^(INPUT|TEXTAREA)$/.test(e.tagName) && e.type !== 'range'))
        && !e.closest(seletorDoTeto) && visivel(e));
    const antes = raiz.style.getPropertyValue('--cel-escala');
    // A leitura depois de um recálculo inteiro (o `offsetHeight` da raiz força
    // estilo e layout), e não só o do nó: medido em 10/10, uma rodada a cada
    // dez da bancada inteira, nós que já viam `--cel-escala` 2 devolviam o
    // tamanho de antes numa leitura logo depois da troca.
    const ler = () => { void raiz.offsetHeight; return nos.map((e) => parseFloat(getComputedStyle(e).fontSize)); };
    raiz.style.setProperty('--cel-escala', '1');
    const a1 = ler();
    raiz.style.setProperty('--cel-escala', '2');
    const a2 = ler();
    // Para quando um nó não cresce, o porquê vem junto, inclusive o tamanho
    // relido depois: se ele mudar, era a leitura, e não a página.
    const primeiro = nos.find((e, i) => a2[i] < a1[i] * 1.3);
    const porque = primeiro ? {
        relido: parseFloat(getComputedStyle(primeiro).fontSize),
        escalaNoNo: getComputedStyle(primeiro).getPropertyValue('--cel-escala').trim(),
        escalaNaRaiz: raiz.style.getPropertyValue('--cel-escala'),
        // Se a folha está carregada, e não só se o `<link>` está ligado: um
        // `<link>` religado diz «ligado» enquanto a folha ainda recarrega.
        folha: ((l) => !l ? 'ausente' : !l.sheet ? 'sem folha (recarregando)' : l.sheet.disabled ? 'desligada' : 'carregada')(document.querySelector('link[href="celular-texto.css"]')),
        adotadas: document.adoptedStyleSheets.length,
    } : null;
    if (antes) raiz.style.setProperty('--cel-escala', antes); else raiz.style.removeProperty('--cel-escala');
    const parados = {};
    nos.forEach((e, i) => {
        if (a2[i] >= a1[i] * 1.3) return;
        const nome = e.id ? '#' + e.id : e.tagName.toLowerCase() + [...e.classList].map((c) => '.' + c).join('');
        parados[nome] = `${a1[i]} → ${a2[i]} px, «${(e.textContent || e.placeholder || '').trim().slice(0, 20)}»`;
    });
    return { nos: nos.length, parados, porque };
}"""


# A 1, o celular-texto.css não muda nada: com ele ligado e desligado, todo nó da
# página tem o mesmo tamanho de letra. O arquivo só declara tamanho de letra,
# então o mesmo tamanho em todo nó é o mesmo pixel.
A_UM_IGUAL = """() => {
    const folha = document.querySelector('link[href="celular-texto.css"]');
    if (!folha?.sheet) return ['a página não carrega o celular-texto.css'];
    const nos = [...document.querySelectorAll('*')];
    const ligado = nos.map((e) => getComputedStyle(e).fontSize);
    // Pela folha do CSSOM, e não pelo `<link>`. Desligar o `<link>` descarrega
    // a folha, e religá-lo a recarrega depois, fora desta leitura (medido em
    // 10/10, no Chromium: o `sheet` fica `null` e o texto no tamanho do
    // desktop). Com a máquina ocupada, a medida seguinte a pegava ainda sem
    // regras, e o texto-maior reprovava «não cresce» por defeito da bancada.
    // A folha do CSSOM desliga e religa na hora, nos dois motores.
    folha.sheet.disabled = true;
    const desligado = nos.map((e) => getComputedStyle(e).fontSize);
    folha.sheet.disabled = false;
    const nome = (e) => e.id ? '#' + e.id : e.tagName.toLowerCase() + [...e.classList].map((c) => '.' + c).join('');
    return nos.flatMap((e, i) => ligado[i] === desligado[i] ? [] : [`${nome(e)}: ${desligado[i]} sem ele, ${ligado[i]} com ele`]);
}"""


def texto_maior(pg: Page) -> list[str]:
    """O texto do sistema (Tarefa 15): a 2, todo texto à vista na sessão cresce,
    em cada aba, em cada seção de AJUSTES, na CHAMADA e nas folhas. O núcleo
    cresce até 1,4× e passa; o que tem teto está em `TEXTO_COM_TETO`.

    Achado no Safari do Simulador com o maior tamanho de acessibilidade: o
    campo de mensagem, o ENVIAR, os nomes das seções e a lista de PESSOAS
    ficavam no tamanho do desktop, ao lado de mensagens no dobro."""
    falhas = []
    teto = ", ".join(TEXTO_COM_TETO)

    def conferir(estado):
        pg.wait_for_timeout(200)
        for diferenca in pg.evaluate(A_UM_IGUAL)[:8]:
            falhas.append(f"{estado}: a 1, o celular-texto.css muda {diferenca}")
        visto = pg.evaluate(TAMANHOS_DO_TEXTO, teto)
        if visto["nos"] < 3:
            falhas.append(f"{estado}: só {visto['nos']} textos à vista, e o caso não mediu nada")
        for nome, medida in sorted(visto["parados"].items()):
            falhas.append(f"{estado}: {nome} não cresce ({medida})")
        if visto["parados"]:
            falhas.append(f"{estado}: o que o primeiro nó parado via: {visto['porque']}")
        # E cabe: a 2, nada rola de lado nem passa da caixa. Os casos a 150 %
        # não viam a cabeça de PESSOAS cortando o «VOZ» (medido no Safari).
        pg.evaluate("document.documentElement.style.setProperty('--cel-escala', '2')")
        pg.wait_for_timeout(100)
        m = pg.evaluate(MEDIR, [0, 0, "#tela-sessao, #tela-server"])
        pg.evaluate("document.documentElement.style.removeProperty('--cel-escala')")
        if m["vaza"] or m["larguraDoDocumento"] > pg.evaluate("innerWidth"):
            falhas.append(f"{estado}: a 2, rola de lado: {m['vaza']} (documento com {m['larguraDoDocumento']} px)")
        if m["transborda"]:
            falhas.append(f"{estado}: a 2, texto passa da caixa: {m['transborda']}")

    for aba in ("servidor", "conversa", "pessoas"):
        if ir_para_aba(pg, aba, falhas):
            conferir(f"a aba {aba.upper()}")
    if tocar(pg, "#lista-roster li.pessoa"):
        conferir("a folha da pessoa")
        pg.keyboard.press("Escape")
    if ir_para_aba(pg, "conversa", falhas) and tocar(pg, "#lista-mensagens li.mensagem .corpo"):
        conferir("a folha da mensagem")
        pg.keyboard.press("Escape")
    if ir_para_aba(pg, "ajustes", falhas):
        conferir("a lista de AJUSTES")
        for secao in ("secao-audio", "secao-aparencia", "secao-servidor", "secao-mods", "cel-secao-conexao",
                      "secao-identidade", "cel-secao-fora", "cel-secao-casca"):
            if not pg.locator(f"#{secao}").is_visible():
                continue
            if exigir_toque(pg, f"#{secao}", falhas):
                conferir(f"a seção #{secao}")
                exigir_toque(pg, "#tela-server .cel-ajustes-voltar", falhas)
    ir_para_aba(pg, "servidor", falhas)
    if tocar(pg, "#voz .voz-sala"):
        esperar_quadro(pg)
        conferir("a CHAMADA")
    return falhas


def retrato_da_casca(onde: str) -> Callable[[Page], list[str]]:
    """O retrato que um MOD recebe diz onde a casca está (API 6, `lugar.onde`):
    `celular` com a casca do celular de pé, e `computador` na casca larga e no
    desktop. O `cascaDoRetrato` do base.js pergunta se o `CELULAR` existe, que só
    o celular-casca.js cria, e só com a marca dele; este caso prova que ele
    chega lá, chamando a função da página.

    A pedido da orquestradora, depois da oitava integração: o retrato lia a
    marca dentro de um arquivo do desktop, e nada provava a troca."""

    def levar(pg: Page) -> list[str]:
        visto = pg.evaluate("""(() => {
            if (typeof cascaDoRetrato !== 'function') return { erro: 'a página não tem o cascaDoRetrato' };
            const retrato = cascaDoRetrato();
            return { onde: retrato.onde, regioes: Array.isArray(retrato.regioes),
                     celular: typeof CELULAR === 'object' && CELULAR !== null,
                     marca: document.documentElement.getAttribute('data-casca') };
        })()""")
        if "erro" in visto:
            return [visto["erro"]]
        falhas = []
        if visto["onde"] != onde:
            falhas.append(f"com a marca {visto['marca']!r}, o retrato diz onde={visto['onde']!r}, e não {onde!r}"
                          f" (CELULAR {'existe' if visto['celular'] else 'não existe'})")
        if not visto["regioes"]:
            falhas.append("o retrato não traz a lista de regiões")
        return falhas

    return levar


# Um MOD da API 6 de pé, pelo caminho de verdade: a instância ativa em
# `modsCarregados`, e cada pedido pelo `atenderOMod`, o roteador que um MOD usa.
# O mesmo da `apps/seele-app/bancada/camadas.cjs` da F3.
MOD_DE_PE = """() => {
  const mod = { id: 'teste/bancada', hash: 'h6', api: 6 };
  const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, {
    pedirEncerramento() {}, async encerrou() { return true; },
    entregar(m) { (window.__entregues ??= []).push(JSON.parse(JSON.stringify(m))); } });
  inst.estado = ESTADOS_DE_MOD.ativa;
  // Como o carregador faz (`base.js`): sem a API na instância, tudo o que
  // pergunta pelas capacidades dela (o `lugar` do palco, os eventos da tela)
  // a via sem nenhuma.
  inst.api = Number(mod.api) || 0;
  modsCarregados.set(mod.id, inst);
  let n = 0;
  window.__pedirAoProduto = (m) => atenderOMod(mod, inst, { ...m, n: ++n });
}"""

def modal_deixa_a_faixa(pg: Page, nome: str, seletor: str) -> list[str]:
    """Com um modal de MOD aberto (o diálogo, a página imersiva), a faixa e as
    abas continuam acordadas: a pilha de inércia da F3 (682f39ba) pergunta ao
    `CELULAR.acordadosNoModal()`. O SEGURE fala, o SOM troca, AJUSTES abre, e o
    SAIR recebe o toque; o Tab fica dentro da superfície. O SAIR não é
    apertado: sair da sala tiraria a faixa das superfícies seguintes."""
    falhas = []
    estado = pg.evaluate("""() => { const centro = (id) => { const b = document.getElementById(id).getBoundingClientRect(); return document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2); };
        return { faixa: !document.querySelector('.voz-celular').closest('[inert]'), abas: !document.querySelector('.cel-abas').closest('[inert]'),
                 conversa: !!document.getElementById('lista-mensagens').closest('[inert]'),
                 sair: !!centro('voz-sair')?.closest('#voz-sair') }; }""")
    if not estado["faixa"] or not estado["abas"]:
        falhas.append(f"com {nome} na tela, a faixa ou as abas ficam inertes: {estado}")
    if not estado["conversa"]:
        falhas.append(f"com {nome} na tela, a conversa por trás não fica inerte")
    if not estado["sair"]:
        falhas.append(f"com {nome} na tela, o SAIR da faixa não recebe o toque")
    pg.evaluate("window.__SEELE_HISTORICO.length = 0")
    centro = pg.evaluate("(() => { const b = document.getElementById('voz-mudo').getBoundingClientRect(); return [b.left + b.width / 2, b.top + b.height / 2]; })()")
    pg.mouse.move(*centro)
    pg.mouse.down()
    pg.wait_for_timeout(250)
    pg.mouse.up()
    pg.wait_for_timeout(150)
    tocar(pg, "#voz-surdo")
    pg.wait_for_timeout(150)
    tocar(pg, "#voz-surdo")
    pg.wait_for_timeout(150)
    feitos = pg.evaluate("window.__SEELE_HISTORICO.map(([c]) => c)")
    if "set_talking" not in feitos:
        falhas.append(f"com {nome} na tela, segurar o SEGURE não fala: {feitos}")
    if feitos.count("set_total_isolation") < 2:
        falhas.append(f"com {nome} na tela, o SOM da faixa não troca: {feitos}")
    # O Tab, a partir de dentro: seis vezes, e o foco não sai da superfície.
    pg.evaluate("(s) => [...document.querySelectorAll(s)].at(-1)?.querySelector('button, [tabindex]')?.focus()", seletor)
    fora = []
    for _ in range(6):
        pg.keyboard.press("Tab")
        if not pg.evaluate("(s) => !![...document.querySelectorAll(s)].at(-1)?.contains(document.activeElement)", seletor):
            fora.append(pg.evaluate("document.activeElement?.id || document.activeElement?.className || document.activeElement?.tagName"))
    if fora:
        falhas.append(f"com {nome} na tela, o Tab sai da superfície: {fora}")
    tocar(pg, "#cel-aba-ajustes")
    pg.wait_for_timeout(300)
    if pg.evaluate("document.getElementById('tela-sessao').dataset.aba") != "ajustes":
        falhas.append(f"com {nome} na tela, AJUSTES não abre")
    ir_para_aba(pg, "conversa", falhas)
    return falhas


SUPERFICIES_DE_MOD = [
    ("a página", {"id": "p", "tipo": "pagina", "titulo": "PÁGINA DE TESTE"}, ".pagina-de-mod"),
    ("a página imersiva", {"id": "pi", "tipo": "pagina", "titulo": "IMERSIVA", "imersiva": True}, ".pagina-de-mod"),
    ("o diálogo", {"id": "d", "tipo": "dialogo", "titulo": "DIÁLOGO DE TESTE"}, ".dialogo-de-mod"),
    ("o aviso", {"id": "a", "tipo": "aviso", "titulo": "AVISO DE TESTE"}, "#avisos-de-mod > *"),
    ("a folha", {"id": "f", "tipo": "folha", "titulo": "FOLHA DE TESTE"}, "#palco-de-paineis > .folha-de-mod"),
]


def superficies_de_mod(pg: Page) -> list[str]:
    """Nenhuma superfície de MOD cobre a faixa de voz nem as abas (spec, §7; F3,
    tarefa 7, passo 5d). Achado no protótipo: o diálogo do ESTILO em tela cheia
    cobria o MUDO e AJUSTES, o único caminho para desligar um MOD no celular.

    Cada superfície, aberta pelo roteador de verdade, termina acima da faixa e
    das abas, e a saída dela tem 44. Com a página e o aviso, o MUDO e AJUSTES
    recebem o toque. Com o diálogo modal e a página imersiva, o MUDO não está
    coberto, mas ainda está inerte: deixá-lo acordado é a pilha de inércia da
    F3 perguntando o `CELULAR.acordadosNoModal()`, e o caso passa a conferir o
    toque quando ela entrar."""
    falhas = []
    ir_para_aba(pg, "conversa", falhas)
    pg.evaluate(MOD_DE_PE)
    medir = """(seletor) => {
        const caixa = (e) => { const b = e.getBoundingClientRect(); return [Math.round(b.left), Math.round(b.top), Math.round(b.right), Math.round(b.bottom)]; };
        const sup = [...document.querySelectorAll(seletor)].filter((e) => getComputedStyle(e).display !== 'none').at(-1);
        const pe = [...document.querySelectorAll('#tela-sessao > .voz-celular, #tela-sessao > .cel-abas')].map((e) => e.getBoundingClientRect().top);
        const centro = (id) => { const b = document.getElementById(id).getBoundingClientRect(); return document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2); };
        const sair = sup?.querySelector('.superficie-de-mod-sair');
        return { superficie: sup ? caixa(sup) : null, pe: Math.min(...pe),
                 mudo: centro('voz-mudo')?.closest('#voz-mudo') !== null, ajustes: centro('cel-aba-ajustes')?.closest('#cel-aba-ajustes') !== null,
                 sair: sair ? Math.round(sair.getBoundingClientRect().height) : null };
    }"""
    for nome, descricao, seletor in SUPERFICIES_DE_MOD:
        resposta = pg.evaluate("(d) => __pedirAoProduto({ tipo: 'superficie-criar', descricao: d }).then(() => (window.__entregues ?? []).filter((m) => m.tipo === 'resposta').at(-1))", descricao)
        pg.wait_for_timeout(300)
        if not resposta or not resposta.get("ok"):
            falhas.append(f"o produto não abriu {nome}: {resposta}")
            continue
        visto = pg.evaluate(medir, seletor)
        if visto["superficie"] is None:
            falhas.append(f"{nome} abriu e não aparece")
        elif visto["superficie"][3] > visto["pe"] or visto["superficie"][1] < 0:
            falhas.append(f"{nome} cobre a faixa ou as abas: {visto['superficie']}, e o pé começa em {visto['pe']}")
        if visto["sair"] is not None and visto["sair"] < 44:
            falhas.append(f"a saída de {nome} tem {visto['sair']} de altura, e não 44")
        if visto["superficie"] is not None and descricao["tipo"] == "folha" and visto["superficie"][2] - visto["superficie"][0] < pg.evaluate("innerWidth") - 1:
            falhas.append(f"a folha não ocupa a largura toda: {visto['superficie']}")
        if descricao["tipo"] in ("aviso", "folha") or not descricao.get("imersiva") and descricao["tipo"] == "pagina":
            if not visto["mudo"] or not visto["ajustes"]:
                falhas.append(f"com {nome} aberta, o MUDO ou AJUSTES não recebe o toque: {visto}")
        else:
            falhas += modal_deixa_a_faixa(pg, nome, seletor)
        pg.evaluate("(id) => __pedirAoProduto({ tipo: 'superficie-fechar', superficie: id })", descricao["id"])
        pg.wait_for_timeout(200)
    # Uma pilha de folhas: só a de cima aparece, e ela continua acima do pé.
    for id_ in ("f1", "f2"):
        pg.evaluate("(id) => __pedirAoProduto({ tipo: 'superficie-criar', descricao: { id, tipo: 'folha', titulo: 'FOLHA ' + id } })", id_)
    pg.wait_for_timeout(300)
    pilha = pg.evaluate("""() => [...document.querySelectorAll('#palco-de-paineis > .folha-de-mod')]
        .filter((f) => f.getClientRects().length > 0 && getComputedStyle(f).display !== 'none').map((f) => f.dataset.superficie)""")
    if pilha != ["f2"]:
        falhas.append(f"numa pilha de duas folhas, aparecem {pilha}, e não só a de cima")
    return falhas


def operador_barra(pg: Page) -> list[str]:
    """A região `operador.barra` no alto de AJUSTES, abaixo de SEU PERFIL (F3,
    tarefa 5, passo 8b; os critérios são dela). Tudo pelo roteador, como um MOD:
    1. uma contribuição marcada aparece dentro de `#operador-mods`, e ela mora
       em AJUSTES, abaixo de SEU PERFIL;
    2. 16 linhas longas (dentro dos 64 nós de um cartão) deixam a região no teto,
       rolando por dentro;
    3. com elas, e com nós deslocados (`estilo.mover`, até ±512 px) do canto da
       região até o centro de MODS DESTE SERVIDOR, a entrada da gestão fica
       inteira na janela e recebe o toque."""
    falhas = []
    if not ir_para_aba(pg, "ajustes", falhas):
        return falhas
    pg.wait_for_timeout(400)
    pg.evaluate(MOD_DE_PE)
    contribuir = """(conteudo) => __pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'operador.barra', modo: 'adicionar', conteudo } })
        .then(() => (window.__entregues ?? []).filter((m) => m.tipo === 'resposta').at(-1))"""
    revogar = "(handle) => __pedirAoProduto({ tipo: 'revogar-contribuicao', handle })"
    caixa = """(s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null;
        const b = e.getBoundingClientRect(); return { left: b.left, top: b.top, right: b.right, bottom: b.bottom, width: b.width, height: b.height }; }"""

    marcada = pg.evaluate(contribuir, [{"forma": "texto", "chave": "marca", "dentro": "MARCA DA BANCADA"}])
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    visto = pg.evaluate("""(caixa) => { const c = new Function('s', 'return (' + caixa + ')(s)');
        const o = document.getElementById('operador-mods');
        return { dentro: o.textContent.includes('MARCA DA BANCADA'), naAjustes: !!o.closest('#tela-server'), regiao: c('#operador-mods'), perfil: c('.cel-perfil') }; }""", caixa)
    if not visto["dentro"] or not visto["naAjustes"]:
        falhas.append(f"a contribuição marcada não aparece em #operador-mods, dentro de AJUSTES: {visto}")
    elif visto["regiao"] is None or visto["perfil"] is None or visto["regiao"]["top"] < visto["perfil"]["bottom"] - 1:
        falhas.append(f"a região não fica abaixo de SEU PERFIL: {visto['regiao']} contra {visto['perfil']}")
    pg.evaluate(revogar, marcada["valor"]["handle"])

    linhas = "Array.from({ length: 16 }, (_, i) => ({ forma: 'texto', chave: `l${i}`, dentro: `linha ${i} `.repeat(24) }))"
    grande = pg.evaluate(f"(contribuir) => (new Function('c', 'return (' + contribuir + ')(c)'))({linhas})", contribuir)
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    canto, alvo = pg.evaluate(caixa, "#operador-mods"), pg.evaluate(caixa, "#secao-mods")
    if canto is None or alvo is None:
        return falhas + [f"sem a região ou sem a entrada da gestão à vista: {canto} {alvo}"]
    limite = lambda n: max(-512, min(512, round(n)))
    mirado = {"forma": "texto", "chave": "mirado", "dentro": "█" * 6,
              "estilo": {"mover": {"x": limite(alvo["left"] + alvo["width"] / 2 - canto["left"] - 8),
                                   "y": limite(alvo["top"] + alvo["height"] / 2 - canto["top"] - 8)}}}
    pg.evaluate(revogar, grande["valor"]["handle"])
    pg.evaluate(f"(contribuir) => (new Function('c', 'return (' + contribuir + ')(c)'))([{__import__('json').dumps(mirado)}, ...{linhas}])", contribuir)
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    visto = pg.evaluate("""() => { const o = document.getElementById('operador-mods'); const g = document.getElementById('secao-mods'); const b = g.getBoundingClientRect();
        const noCentro = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
        return { altura: Math.round(o.getBoundingClientRect().height), rola: o.scrollHeight > o.clientHeight,
                 gestao: [Math.round(b.left), Math.round(b.top), Math.round(b.right), Math.round(b.bottom)],
                 inteira: b.top >= 0 && b.left >= 0 && b.bottom <= innerHeight && b.right <= innerWidth,
                 acima: b.bottom <= document.querySelector('.voz-celular').getBoundingClientRect().top,
                 toque: !!noCentro && g.contains(noCentro) }; }""")
    if visto["altura"] != 96 or not visto["rola"]:
        falhas.append(f"o conteúdo grande não deixou a região no teto de 96, rolando por dentro: {visto}")
    if not visto["inteira"] or not visto["acima"]:
        falhas.append(f"com o conteúdo grande, a entrada da gestão de MODs saiu da vista: {visto['gestao']}")
    elif not visto["toque"]:
        falhas.append("um nó do MOD, deslocado até o centro da gestão de MODs, a cobre")
    return falhas


# As regiões que a casca do celular declara (CELULAR.regioes): a aba onde moram,
# e o que a contribuição de cada uma leva além do ponto e do modo — o alvo de
# quem tem alvo (o canal aberto, uma mensagem), e o rótulo da navegação, que é
# uma entrada na lista de canais e não um cartão. A conversa só redesenha as
# mensagens a cada revisão, e a bancada a redesenha como a F3 faz.
REGIOES_DO_CELULAR = [
    ("redes", "servidor", "", False),
    ("servidor.navegacao", "servidor", "rotulo: 'SONDA SERVIDOR.NAVEGACAO', ", False),
    ("operador.barra", "ajustes", "", False),
    ("canal.cabecalho", "conversa", "alvo: String(linhaAberta), ", False),
    ("conversa.mensagem", "conversa", "alvo: String(mensagens.at(-1).id), ", True),
    ("compositor", "conversa", "", False),
    ("pessoas.lista", "pessoas", "", False),
    # O palco começa atrás da grade, e é o caso `palco-de-mods` que o traz à
    # frente e confere a casa dele; aqui ele entra na lista declarada.
    ("chamada.palco", None, "", False),
]


def regioes_do_celular(pg: Page) -> list[str]:
    """As regiões de MOD do celular (rascunho da API 6, §11; decidido em 09/10):
    toda região que a casca declara em `CELULAR.regioes` tem caixa à vista na
    aba dela, e uma que ela não declara (`status.barra`) recebe `regiao-ausente`
    e sai das `regioes` do retrato, em vez de desenhar num nó escondido.

    É o que impede a lista de envelhecer calada: uma região declarada que a
    casca esconde, ou uma escondida que ela declara, reprova aqui."""
    falhas = []
    declaradas = pg.evaluate("CELULAR.regioes instanceof Set ? [...CELULAR.regioes].sort() : null")
    if declaradas != sorted(r for r, *_ in REGIOES_DO_CELULAR):
        falhas.append(f"o CELULAR.regioes declara {declaradas}, e não as {len(REGIOES_DO_CELULAR)} do celular")
    # Cada uma, como um MOD a usaria: uma contribuição pelo roteador, e o texto
    # dela à vista na aba, recebendo o toque.
    pg.evaluate(MOD_DE_PE)
    for regiao, aba, extra, redesenhar in REGIOES_DO_CELULAR:
        if aba is None:
            continue
        if not ir_para_aba(pg, aba, falhas):
            continue
        pg.wait_for_timeout(250)
        marca = f"SONDA {regiao}".upper()
        pedido = f"{{ ponto: '{regiao}', modo: 'adicionar', {extra}conteudo: [{{ forma: 'texto', chave: 's', dentro: '{marca}' }}] }}"
        resposta = pg.evaluate(f"__pedirAoProduto({{ tipo: 'contribuir', pedido: {pedido} }}).then(() => (window.__entregues ?? []).filter((m) => m.tipo === 'resposta').at(-1))")
        esperar_quadro(pg)
        if redesenhar:
            pg.evaluate("desenharMensagens()")
        pg.wait_for_timeout(300)
        visto = pg.evaluate("""(marca) => {
            const d = [...document.querySelectorAll('#tela-sessao *, #tela-server *')].find((e) => e.children.length === 0 && e.textContent.trim() === marca);
            if (!d) return 'o texto da contribuição não está na página';
            d.scrollIntoView({ block: 'center' });
            for (let n = d; n; n = n.parentElement) { const st = getComputedStyle(n); if (st.display === 'none' || st.visibility === 'hidden' || n.hidden) return 'escondido por ' + (n.id || n.className); }
            const b = d.getBoundingClientRect();
            if (b.height === 0 || b.top < 0 || b.bottom > innerHeight) return `fora da janela, em ${Math.round(b.top)}`;
            const noCentro = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
            return d.contains(noCentro) || noCentro?.contains(d) ? 'à vista' : 'coberto por ' + (noCentro?.id || noCentro?.className);
        }""", marca)
        if visto != "à vista":
            falhas.append(f"a região {regiao} não tem casa na aba {aba.upper()}: {visto} (resposta {resposta})")
    # A que a casca não tem: o MOD fica sabendo, e o retrato não a lista.
    pg.evaluate("__pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'status.barra', modo: 'adicionar', conteudo: [{ forma: 'texto', chave: 'x', dentro: 'MEDIDA' }] } })")
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    avisado = pg.evaluate("(window.__entregues ?? []).some((m) => m.motivo === 'regiao-ausente' && m.ponto === 'status.barra')")
    if not avisado:
        falhas.append("uma contribuição no status.barra não recebeu regiao-ausente")
    regioes = pg.evaluate("cascaDoRetrato().regioes")
    if "status.barra" in regioes or any(r not in regioes for r, *_ in REGIOES_DO_CELULAR):
        falhas.append(f"o retrato diz as regiões {regioes}")
    return falhas


# O contraste de cada texto de um quadro contra o fundo dele: a primeira cor
# opaca entre o texto e o quadro. Com uma capa, chegar ao quadro é chegar à
# capa (ela é a primeira camada dele, por cima do fundo próprio), e 'tinta' é
# a capa da cor do próprio texto, a pior que existe para ele. O método da
# `apps/seele-app/bancada/capa.cjs` da F3, com que ela mede a barra do desktop.
CONTRASTE_EM = r"""([seletor, capa]) => {
  const quadro = document.querySelector(seletor);
  if (!quadro) return null;
  const rgb = (t) => (t.match(/[\d.]+/g) || []).map(Number);
  const luz = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const razao = (a, b) => { const [x, y] = [luz(a), luz(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
  const fundo = (el, tinta) => {
    for (let n = el; n; n = n.parentElement) {
      if (n === quadro && capa) return capa === "tinta" ? tinta : capa;
      const c = rgb(getComputedStyle(n).backgroundColor);
      if (c.length >= 3 && (c.length < 4 || c[3] >= 0.99)) return c.slice(0, 3);
    }
    return [0, 0, 0];
  };
  const out = {};
  for (const e of quadro.querySelectorAll("*")) {
    if (!e.getClientRects().length || ![...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())) continue;
    const tinta = rgb(getComputedStyle(e).color).slice(0, 3);
    out[`${e.className || e.tagName} «${e.textContent.trim().slice(0, 24)}»`] = Number(razao(tinta, fundo(e, tinta)).toFixed(2));
  }
  return out;
}"""

# Uma região clara sobre o produto escuro: o pior caso para um contêiner que
# recebe a tinta da região sem pintar o painel dela atrás.
REGIAO_CLARA = {"fundo": "#f2ece0", "texto": "#12100e", "apagado": "#574e43", "acento": "#a83a10", "borda": "#b8ad9a"}


def regiao_do_operador(pg: Page) -> list[str]:
    """O tema por região no `operador.barra` (17i da F3, item 3; f09e7540). No
    celular a região mora em AJUSTES, fora da sessão, e a casca declara os
    contêineres dela em `CELULAR.conteineresDasRegioes`: a placa de SEU
    PERFIL, que é a barra do operador do desktop (o retrato e o apelido), e a
    própria região. Com o tema publicado do servidor pondo uma região clara
    (no quadro, como o servidor o manda, e de pé depois de dois redesenhos),
    o texto de uma contribuição e o apelido tomam a tinta da região, cada
    contêiner pinta o painel dela atrás, e todo texto dos dois fica acima de
    4,5:1. O tema que sai limpa os dois; o rótulo SEU PERFIL e EDITAR PERFIL,
    fora dos contêineres, não mudam."""
    falhas = []
    if not ir_para_aba(pg, "ajustes", falhas):
        return falhas
    pg.evaluate(MOD_DE_PE)
    pg.evaluate("__pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'operador.barra', modo: 'adicionar', conteudo: [{ forma: 'texto', chave: 's', dentro: 'SONDA DA REGIÃO' }] } })")
    esperar_quadro(pg)
    ler = """() => {
        const sonda = [...document.querySelectorAll('#operador-mods *')].find((e) => e.children.length === 0 && e.textContent.trim() === 'SONDA DA REGIÃO');
        const cor = (el, p) => (el ? getComputedStyle(el)[p] : null);
        const regiao = document.getElementById('operador-mods'), placa = document.querySelector('.cel-perfil-placa');
        return { sonda: cor(sonda, 'color'), fundoDaRegiao: cor(regiao, 'backgroundColor'),
                 apelido: cor(document.querySelector('.cel-perfil-apelido'), 'color'), fundoDaPlaca: cor(placa, 'backgroundColor'),
                 rotulo: cor(document.querySelector('.cel-perfil > .rotulo'), 'color'),
                 escritoNaRegiao: regiao.style.getPropertyValue('--seele-negro-painel').trim(),
                 escritoNaPlaca: placa.style.getPropertyValue('--seele-negro-painel').trim() }; }"""
    antes = pg.evaluate(ler)
    aplicar = "(cores) => { SEELE_QUADRO.tema_do_servidor = { mod_id: 'seele/estilo', valores: JSON.stringify({ regioes: { 'operador.barra': cores } }) }; }"
    pg.evaluate(aplicar, REGIAO_CLARA)
    esperar_quadro(pg)
    esperar_quadro(pg)
    com = pg.evaluate(ler)
    tinta = "rgb(18, 16, 14)"
    if com["sonda"] != tinta or com["apelido"] != tinta:
        falhas.append(f"com a região clara, a contribuição e o apelido não tomam a tinta dela: {com}")
    if not com["escritoNaRegiao"] or not com["escritoNaPlaca"]:
        falhas.append(f"a região não chegou aos contêineres da casca: {com}")
    if com["fundoDaRegiao"] == antes["fundoDaRegiao"] or com["fundoDaPlaca"] == antes["fundoDaPlaca"]:
        falhas.append(f"um contêiner não pinta o painel da região atrás: {antes['fundoDaRegiao']} → {com['fundoDaRegiao']}, placa {antes['fundoDaPlaca']} → {com['fundoDaPlaca']}")
    if com["rotulo"] != antes["rotulo"]:
        falhas.append(f"o rótulo SEU PERFIL, fora dos contêineres, mudou com a região: {antes['rotulo']} → {com['rotulo']}")
    for seletor in (".cel-perfil-placa", "#operador-mods"):
        baixos = {k: v for k, v in (pg.evaluate(CONTRASTE_EM, [seletor, None]) or {}).items() if v < 4.5}
        if baixos:
            falhas.append(f"com a região clara, texto abaixo de 4,5:1 em {seletor}: {baixos}")
    pg.evaluate("SEELE_QUADRO.tema_do_servidor = null")
    esperar_quadro(pg)
    depois = pg.evaluate(ler)
    if depois != antes:
        falhas.append(f"o tema saiu, e a região ficou: {antes} → {depois}")
    # A foto, com a região.
    pg.evaluate(aplicar, REGIAO_CLARA)
    esperar_quadro(pg)
    return falhas


CAPAS_DE_PROVA = (("branca", [255, 255, 255]), ("preta", [0, 0, 0]), ("da cor da tinta", "tinta"))


def capa_do_operador(pg: Page) -> list[str]:
    """A capa de quem usa (`pessoa.capa`; D-capa-1) na placa de SEU PERFIL,
    que é a barra do operador do desktop. A casca a veste com o `vestirCapa`
    da F3 a cada quadro; a camada cobre a placa inteira, e o retrato e o
    apelido ficam sobre placas opacas do painel. Medido como a F3 mede a barra
    (`bancada/capa.cjs`): cada texto mantém o contraste que tinha sem capa,
    contra uma capa branca, uma preta e uma da cor da própria tinta."""
    falhas = []
    if not ir_para_aba(pg, "ajustes", falhas):
        return falhas
    sem = pg.evaluate(CONTRASTE_EM, [".cel-perfil-placa", None])
    # A mídia de um MOD só vem numa geração de pé, e o duble não entra em
    # nenhuma: a mesma entrada que a `bancada/capa.cjs` da F3 faz.
    pg.evaluate("entrarNaGeracao(1)")
    pg.evaluate(MOD_DE_PE)
    pg.evaluate("""() => {
        const c = document.createElement('canvas'); c.width = 96; c.height = 32;
        const x = c.getContext('2d'); x.fillStyle = '#32e6a2'; x.fillRect(0, 0, 96, 32);
        SEELE_RESPOSTAS.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: c.toDataURL('image/jpeg') };
        return __pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'pessoa.capa', modo: 'substituir', alvo: String(SEELE_QUADRO.me),
            conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:eu' } } } } }); }""")
    esperar_quadro(pg)
    pg.wait_for_timeout(300)
    vestida = pg.evaluate("""() => { const p = document.querySelector('.cel-perfil-placa'); const c = p?.querySelector(':scope > .capa-camada');
        const b = c?.getBoundingClientRect();
        return { vestida: p?.dataset.comCapa === 'sim', imagem: c ? getComputedStyle(c).backgroundImage.startsWith('url(') : false,
                 cobre: !!b && Math.abs(b.width - p.clientWidth) < 1 && Math.abs(b.height - p.clientHeight) < 1,
                 midia: (window.__entregues ?? []).filter((m) => m.nome === 'midia') }; }""")
    if not (vestida["vestida"] and vestida["imagem"] and vestida["cobre"]):
        falhas.append(f"a capa de quem usa não veste a placa de SEU PERFIL inteira: {vestida}")
        return falhas
    for nome, cor in CAPAS_DE_PROVA:
        com = pg.evaluate(CONTRASTE_EM, [".cel-perfil-placa", cor]) or {}
        pioras = {k: f"{v} → {com.get(k)}" for k, v in sem.items() if (com.get(k) or 0) < v - 0.01}
        if pioras:
            falhas.append(f"sobre uma capa {nome}, um texto de SEU PERFIL perde contraste: {pioras}")
    return falhas


def teclado(pg: Page) -> list[str]:
    """O teclado do sistema (Tarefa 12): o compositor colado a ele, as abas fora,
    e a faixa, com o MUDO, logo abaixo do topo."""
    falhas = []
    ir_para_aba(pg, "conversa", falhas)
    pg.locator("#campo-mensagem").focus()
    # Como o Safari do iOS 27 faz (medido no Simulador, 09/10, a 402 × 874): o
    # que se vê encolhe, o `innerHeight` encolhe junto (só o `clientHeight` da
    # raiz fica), e a parte visível desce pela página. Um teclado falso que não
    # encolhia o `innerHeight` passava, e o teclado de verdade não ligava nada.
    altura, desceu = 480, 200
    pg.evaluate("""([altura, desceu]) => {
        Object.defineProperty(window.visualViewport, 'height', { get: () => altura, configurable: true });
        Object.defineProperty(window.visualViewport, 'offsetTop', { get: () => desceu, configurable: true });
        window.__innerHeightOriginal = Object.getOwnPropertyDescriptor(window, 'innerHeight');
        Object.defineProperty(window, 'innerHeight', { get: () => altura, configurable: true });
        window.visualViewport.dispatchEvent(new Event('resize'));
    }""", [altura, desceu])
    pg.wait_for_timeout(200)
    visto = pg.evaluate("""(() => {
        const r = (s) => { const e = document.querySelector(s); if (!e || getComputedStyle(e).display === 'none') return null; const b = e.getBoundingClientRect(); return [Math.round(b.top), Math.round(b.bottom)]; };
        return { marca: document.getElementById('tela-sessao').hasAttribute('data-teclado'),
                 abas: r('.cel-abas'), mudo: r('#voz-mudo'), compositor: r('#form-mensagem'), topo: r('.cel-topo') };
    })()""")
    if not visto["marca"]:
        return falhas + ["com o teclado aberto, a sessão não sabe dele (data-teclado)"]
    if visto["abas"] is not None:
        falhas.append("com o teclado aberto, as abas continuam à vista")
    if visto["topo"] is None or abs(visto["topo"][0] - desceu) > 1:
        falhas.append(f"com o teclado aberto, o topo não acompanha o que se vê: {visto['topo']} contra {desceu}")
    if visto["mudo"] is None or visto["topo"] is None or visto["mudo"][0] > visto["topo"][1] + 80:
        falhas.append(f"com o teclado aberto, o MUDO não está no alto: mudo {visto['mudo']}, topo {visto['topo']}")
    if visto["compositor"] is None or abs(visto["compositor"][1] - (desceu + altura)) > 1:
        falhas.append(f"o compositor não está colado ao teclado: {visto['compositor']} contra {desceu + altura}")
    pg.locator("#campo-mensagem").blur()
    pg.evaluate("""(() => {
        delete window.visualViewport.height; delete window.visualViewport.offsetTop;
        Object.defineProperty(window, 'innerHeight', window.__innerHeightOriginal);
        window.visualViewport.dispatchEvent(new Event('resize'));
    })()""")
    pg.wait_for_timeout(200)
    if pg.evaluate("document.getElementById('tela-sessao').hasAttribute('data-teclado')"):
        falhas.append("com o teclado fechado, a sessão continua no modo do teclado")
    return falhas


def tela_larga(pg: Page) -> list[str]:
    """Um tablet, ou a escolha à mão, numa tela grande (Tarefa 12): uma coluna de
    até 480 px no meio, com as abas e AJUSTES presos a ela."""
    falhas = []
    largura = pg.evaluate("innerWidth")
    for alvo in ("#tela-sessao", ".cel-abas"):
        caixa = pg.evaluate("(s) => { const r = document.querySelector(s).getBoundingClientRect(); return [Math.round(r.left), Math.round(r.width)]; }", alvo)
        if caixa[1] > 481 or abs(caixa[0] - (largura - caixa[1]) / 2) > 1:
            falhas.append(f"{alvo} não é a coluna de até 480 no meio: {caixa}")
    ir_para_aba(pg, "ajustes", falhas)
    pg.wait_for_timeout(500)
    camada = pg.evaluate("(() => { const r = document.getElementById('tela-server').getBoundingClientRect(); return [Math.round(r.left), Math.round(r.width)]; })()")
    if camada[1] > 481 or abs(camada[0] - (largura - camada[1]) / 2) > 1:
        falhas.append(f"AJUSTES não fica na coluna: {camada}")
    return falhas


def voltar(pg: Page) -> list[str]:
    """O voltar do aparelho (Tarefa 12): cada camada empurra um estado, e o
    `history.back()` fecha uma por vez."""
    falhas = []

    def costas():
        # Sem estado empilhado, o voltar sai da página, e a sessão cai junto.
        try:
            url = pg.url
            pg.evaluate("history.back()")
            pg.wait_for_timeout(400)
            return pg.url == url
        except ErroDoPlaywright:
            return False

    estado = lambda: pg.evaluate("""[!!document.getElementById('cel-folha-pessoa'),
        !document.getElementById('vista-chamada').hidden,
        document.getElementById('tela-sessao').dataset.aba]""")
    exigir_toque(pg, "#voz .voz-sala", falhas)
    esperar_quadro(pg)
    exigir_toque(pg, "#chamada-grade > li[data-pessoa='juli']", falhas)
    pg.wait_for_timeout(200)
    if estado()[:2] != [True, True]:
        return falhas + [f"a CHAMADA com a folha de juli não abriu: {estado()}"]
    if not costas():
        return falhas + ["o voltar com a folha aberta saiu da página: a sessão caiu"]
    if estado()[:2] != [False, True]:
        falhas.append(f"o primeiro voltar não fechou só a folha: {estado()}")
    if not costas():
        return falhas + ["o voltar com a CHAMADA aberta saiu da página: a sessão caiu"]
    if estado()[:2] != [False, False]:
        falhas.append(f"o segundo voltar não fechou a CHAMADA: {estado()}")

    ir_para_aba(pg, "ajustes", falhas)
    pg.wait_for_timeout(500)
    exigir_toque(pg, "#tela-server #secao-audio", falhas)
    pg.wait_for_timeout(200)
    if not costas():
        return falhas + ["o voltar numa seção de AJUSTES saiu da página: a sessão caiu"]
    pagina = pg.evaluate("document.getElementById('tela-server').getAttribute('data-cel-pagina')")
    if pagina != "lista":
        falhas.append(f"o voltar numa seção de AJUSTES não voltou à lista: {pagina}")
    if pg.evaluate("document.getElementById('tela-server').hidden"):
        falhas.append("o voltar numa seção de AJUSTES fechou AJUSTES inteiro")
    return falhas


def deitado(pg: Page) -> list[str]:
    """O aparelho deitado na CHAMADA (Tarefa 12, vindo da 8): o palco toma o
    espaço acima da faixa, e a faixa continua à mão."""
    falhas = []
    exigir_toque(pg, "#voz .voz-sala", falhas)
    esperar_quadro(pg)
    # A imagem de uma tela pede um servidor de verdade: o palco é posto no
    # estado de quem transmite, que é o que o desktop escreve quando ela chega.
    pg.evaluate("document.getElementById('palco').dataset.transmitindo = 'sim'")
    pg.wait_for_timeout(200)
    visto = pg.evaluate("""(() => {
        const r = (s) => { const e = document.querySelector(s); if (!e || e.closest('[hidden]') || getComputedStyle(e).display === 'none') return null; const b = e.getBoundingClientRect(); return [Math.round(b.top), Math.round(b.bottom), Math.round(b.width)]; };
        return { palco: r('#palco'), voz: r('#voz'), mudo: r('#voz-mudo'), topo: r('.cel-topo') };
    })()""")
    if visto["voz"] is None or visto["mudo"] is None:
        return falhas + ["deitado, com a tela no palco, a faixa de voz sumiu"]
    if visto["palco"] is None or visto["palco"][0] > 1 or abs(visto["palco"][1] - visto["voz"][0]) > 1:
        falhas.append(f"deitado, o palco não toma o espaço acima da faixa: palco {visto['palco']}, faixa {visto['voz']}")
    if visto["topo"] is not None:
        falhas.append("deitado, com a tela no palco, o topo ainda come altura")
    return falhas


def casca_larga(pg: Page) -> list[str]:
    """A casca larga (Tarefa 17, D-web-3): o desktop no navegador de um
    computador. O que é do Tauri sai, e o que só o app faz vira frase no lugar
    do controle. O caso começa na entrada, antes de entrar."""
    falhas = []
    visivel = "(s) => { const e = document.querySelector(s); return !!e && !e.closest('[hidden]') && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0; }"
    pg.wait_for_function("typeof conectar === 'function'")
    pg.wait_for_timeout(600)

    entrada = pg.evaluate("""(visivel) => {
        const v = new Function('s', 'return (' + visivel + ')(s)');
        const link = document.querySelector('.larga-hospedar a');
        return { barra: v('#barra-janela'), hospedar: v('#botao-hospedar'), nomePublico: v('#boot-nome-publico'),
                 versoes: v('#boot-versoes'), frase: document.querySelector('.larga-hospedar')?.textContent ?? null,
                 link: link?.getAttribute('href') ?? null,
                 proposito: document.querySelector('#tela-boot .boot-proposito').textContent,
                 topo: Math.round(document.getElementById('tela-boot').getBoundingClientRect().top) };
    }""", visivel)
    if entrada["barra"]:
        falhas.append("a barra da janela continua à vista: no navegador, quem desenha a janela é ele")
    if entrada["hospedar"] or entrada["nomePublico"] or entrada["versoes"]:
        falhas.append(f"a entrada ainda oferece o que é de quem hospeda: {entrada}")
    if not entrada["frase"] or "app do SEELE" not in entrada["frase"] or entrada["link"] != "https://seele.app.br/#baixar":
        falhas.append(f"no lugar do HOSPEDAR AQUI não há a frase do app e o link: {entrada['frase']!r} {entrada['link']!r}")
    if "pela" in entrada["proposito"] and "no ar a partir desta máquina" in entrada["proposito"] or "link" not in entrada["proposito"]:
        falhas.append(f"o propósito ainda fala em pôr um servidor no ar: {entrada['proposito']!r}")
    if entrada["topo"] != 0:
        falhas.append(f"a entrada não começa no alto da página: {entrada['topo']}")
    antes = pg.evaluate("(() => { const e = new Event('beforeunload', { cancelable: true }); window.dispatchEvent(e); return e.defaultPrevented; })()")
    if antes:
        falhas.append("sem sessão, sair da página pergunta antes (não há o que perder)")

    entrar(pg)
    # A larga ainda diz a frase do desktop no aviso de fone: o P9 só a troca no
    # celular, e a larga espera a medida do eco da F4 (a 3.3 dela).
    if pg.evaluate("document.getElementById('aviso-fone-texto').textContent !== AVISO_DE_FONE"):
        falhas.append("o aviso de fone da larga mudou de frase sem a F4 confirmar o eco do navegador")
    sessao = pg.evaluate("""(() => { const r = document.getElementById('tela-sessao').getBoundingClientRect();
        const e = new Event('beforeunload', { cancelable: true }); window.dispatchEvent(e);
        return { topo: Math.round(r.top), altura: Math.round(r.height), pergunta: e.defaultPrevented }; })()""")
    if sessao["topo"] != 0 or sessao["altura"] != pg.evaluate("innerHeight"):
        falhas.append(f"a sessão não ocupa a página inteira: {sessao}")
    if not sessao["pergunta"]:
        falhas.append("com a sessão aberta, sair da página não pergunta antes: recarregar derruba a sessão")

    # CONFIGURAÇÕES: o que é do app vira frase no lugar.
    exigir_toque(pg, "#botao-server-sessao", falhas)
    pg.wait_for_timeout(600)

    def secao(id_, painel):
        if not exigir_toque(pg, f"#{id_}", falhas):
            return None
        pg.wait_for_timeout(300)
        return pg.evaluate("""([painel, visivel]) => {
            const v = new Function('s', 'return (' + visivel + ')(s)');
            const p = document.getElementById(painel);
            return { frase: p.querySelector('.larga-frase')?.textContent ?? '',
                     blocos: [...p.querySelectorAll('.server-bloco')].filter((b) => getComputedStyle(b).display !== 'none').length,
                     procurar: v('#atualizacao-procurar') };
        }""", [painel, visivel])

    atualizacao = secao("secao-atualizacao", "painel-atualizacao")
    if atualizacao and ("origem" not in atualizacao["frase"] or atualizacao["procurar"] or atualizacao["blocos"]):
        falhas.append(f"ATUALIZAÇÃO não diz que a página vem da origem, ou ainda procura e instala: {atualizacao}")
    versoes = secao("secao-versoes", "painel-versoes")
    if versoes and ("app do SEELE" not in versoes["frase"] or versoes["blocos"]):
        falhas.append(f"VERSÕES não diz que guardar versões é do app: {versoes}")
    # No computador há tecla, e a faixa de contexto continua a nomeá-la (o padrão
    # do CASCA.fraseDoModo é a frase do desktop).
    pg.wait_for_timeout(600)
    contexto = pg.evaluate("document.getElementById('server-contexto').textContent")
    if "ESPAÇO" not in contexto:
        falhas.append(f"a faixa de contexto da larga não nomeia a tecla de falar: «{contexto}»")
    atalhos = secao("secao-atalhos", "painel-atalhos")
    if atalhos and "foco" not in atalhos["frase"]:
        falhas.append(f"a tecla de falar não diz que só vale com a página em foco: {atalhos}")
    # O teste do microfone fora da sala existe no PC (decisão da orquestradora,
    # 09/10): a D-52 é do áudio da sessão, e o teste a pessoa abre de propósito.
    # A larga deixa o bloco do desktop, sem frase por cima; quem atende os
    # comandos pelo getUserMedia é a ponte da F4.
    audio = secao("secao-audio", "painel-audio")
    teste = pg.evaluate(visivel, "#server-testar-microfone")
    if audio is None or audio["frase"] or not teste:
        falhas.append(f"ÁUDIO na larga esconde o teste do microfone, ou põe frase no lugar dele: {audio}, botão {teste}")
    if exigir_toque(pg, "#larga-secao-fora", falhas):
        pg.wait_for_timeout(300)
        fora = pg.evaluate("""(() => { const p = document.getElementById('larga-painel-fora');
            return { grupos: [...p.querySelectorAll('.rotulo')].map((r) => r.textContent), itens: p.querySelectorAll('li').length,
                     link: p.querySelector('a')?.getAttribute('href') ?? null }; })()""")
        if fora["grupos"] != ["SÓ NO APP DO SEELE"] or fora["itens"] != 8 or fora["link"] != "https://seele.app.br/#baixar":
            falhas.append(f"O QUE O NAVEGADOR NÃO FAZ mostra {fora}")
    return falhas


def larga_tela_cheia(pg: Page) -> list[str]:
    """A tela cheia da larga sai junto com a do navegador (achado da F4). No
    navegador o Esc sai da tela cheia sem a página receber a tecla, e o cinema
    do desktop ficava ligado: a imagem esticada numa janela que não está mais
    em tela cheia, e o botão dizendo SAIR DA TELA CHEIA.

    O cinema entra pelo `trocarCinema` do desktop, a tela cheia de verdade por um
    clique (o navegador só a dá com gesto), e a saída pelo `exitFullscreen`, que
    dispara o mesmo `fullscreenchange` que o Esc do navegador."""
    falhas = []
    pg.evaluate("""() => { const b = document.createElement('button'); b.id = 'bancada-tela-cheia'; b.textContent = 'tela cheia';
        b.style.cssText = 'position:fixed;left:0;top:0;z-index:9999;width:60px;height:40px';
        b.addEventListener('click', () => document.documentElement.requestFullscreen()); document.body.append(b); }""")
    # A tela cheia antes do cinema: o cinema esconde tudo o que fica fora do
    # caminho da imagem, e o botão da bancada junto.
    pg.click("#bancada-tela-cheia")
    pg.wait_for_timeout(300)
    pg.evaluate("trocarCinema(true)")
    pg.wait_for_timeout(200)
    entrou = pg.evaluate("[!!document.fullscreenElement, document.body.dataset.cinema ?? null]")
    if entrou != [True, "sim"]:
        return falhas + [f"a bancada não pôs a página em tela cheia com o cinema ligado: {entrou}"]
    pg.evaluate("document.exitFullscreen()")
    pg.wait_for_timeout(400)
    saiu = pg.evaluate("[!!document.fullscreenElement, document.body.dataset.cinema ?? null, document.getElementById('palco-cheia').textContent.trim()]")
    if saiu[0]:
        falhas.append("o navegador não saiu da tela cheia")
    if saiu[1] is not None or saiu[2] != "TELA CHEIA":
        falhas.append(f"o navegador saiu da tela cheia e o cinema ficou ligado: data-cinema {saiu[1]!r}, o botão diz «{saiu[2]}»")
    return falhas


def microfone_negado(pg: Page) -> list[str]:
    """Com o microfone negado pelo navegador, a seção ÁUDIO diz isso e onde se
    libera (matriz, achado 3; a ponte da F4 devolve a `NegadaPeloNavegador`).
    Sem a frase, o bloco ficava escondido justamente nesse caso. O botão dos
    ajustes do sistema não aparece: a página não os abre."""
    falhas = []
    pg.evaluate("window.__SEELE_PERMISSAO_DO_MICROFONE = 'NegadaPeloNavegador'")
    marca = pg.evaluate("document.documentElement.dataset.casca")
    if marca == "celular":
        ir_para_aba(pg, "ajustes", falhas)
    else:
        exigir_toque(pg, "#botao-server-sessao", falhas)
    pg.wait_for_timeout(500)
    if not exigir_toque(pg, "#secao-audio", falhas):
        return falhas
    pg.wait_for_timeout(800)
    visto = pg.evaluate("""(() => { const v = (id) => { const e = document.getElementById(id); return !!e && !e.closest('[hidden]') && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0; };
        return { bloco: v('boot-microfone'), diz: document.getElementById('boot-microfone-diz').textContent, botao: v('boot-microfone-ajustes') }; })()""")
    if not visto["bloco"] or "NAVEGADOR" not in visto["diz"]:
        falhas.append(f"com o microfone negado pelo navegador, ÁUDIO não diz nada: {visto}")
    if visto["botao"]:
        falhas.append("o botão dos ajustes do sistema aparece, e a página não os abre")
    return falhas


# As peças da sala pessoal da F7 (a53c1349, 086082d0): a faixa do toque, a
# linha da sala na entrada e na sessão, e o SEU CÓDIGO no perfil.
PECAS_DA_SALA_PESSOAL = ("toque", "boot-sala-estado", "sala-estado", "perfil-sala")


def sala_pessoal(pg: Page) -> list[str]:
    """O navegador não tem sala pessoal: as duas cascas dele escondem toda peça
    com `data-sala-pessoal` (o contrato que a F7 escreveu no `index.html`), e o
    SÓ NO APP DO SEELE diz que receber ligações pelo código é do app.

    As peças nascem `hidden`, e no navegador a ponte responde neutro: «nenhuma
    à vista» passaria sem regra nenhuma. O caso as mostra como o desktop as
    mostraria (tira o `hidden` e põe texto) e exige que a casca as esconda."""
    falhas = []
    achadas = pg.evaluate("[...document.querySelectorAll('[data-sala-pessoal]')].map((e) => e.id)")
    faltam = [i for i in PECAS_DA_SALA_PESSOAL if i not in achadas]
    if faltam:
        falhas.append(f"as peças da sala pessoal mudaram: faltam {faltam}, e há {achadas}")
    marca = pg.evaluate("document.documentElement.dataset.casca")
    if marca == "celular":
        ir_para_aba(pg, "ajustes", falhas)
    vistas = pg.evaluate("""() => [...document.querySelectorAll('[data-sala-pessoal]')].map((e) => {
        e.hidden = false;
        if (!e.textContent.trim()) e.textContent = 'a sala pessoal diria algo aqui';
        for (let n = e.parentElement; n; n = n.parentElement) n.hidden = false;
        const b = e.getBoundingClientRect();
        return getComputedStyle(e).display !== 'none' && b.width > 0 && b.height > 0 ? e.id : null;
    }).filter(Boolean)""")
    if vistas:
        falhas.append(f"a casca do navegador deixa a sala pessoal à vista: {vistas}")
    diz = pg.evaluate("[...document.querySelectorAll('.cel-fora-grupo, .larga-fora-grupo')].some((g) => g.textContent.includes('RECEBER LIGAÇÕES PELO SEU CÓDIGO'))")
    if not diz:
        falhas.append("o SÓ NO APP DO SEELE não diz que receber ligações pelo código é do app")
    return falhas


def escolha_da_casca(pg: Page) -> list[str]:
    """A escolha da casca à mão (Tarefa 18): ela grava a preferência, diz que
    vale na próxima abertura, e não troca nada ao vivo."""
    falhas = []
    marca = pg.evaluate("document.documentElement.dataset.casca")
    outra, botao = ("larga", "#cel-secao-casca") if marca == "celular" else ("celular", "#larga-secao-casca")
    if marca == "celular":
        ir_para_aba(pg, "ajustes", falhas)
    else:
        exigir_toque(pg, "#botao-server-sessao", falhas)
    pg.wait_for_timeout(500)
    if not exigir_toque(pg, botao, falhas):
        return falhas
    pg.wait_for_timeout(200)
    if not exigir_toque(pg, f"[data-casca-escolhida='{outra}']", falhas):
        return falhas
    pg.wait_for_timeout(200)
    visto = pg.evaluate("""(() => ({ marca: document.documentElement.dataset.casca,
        guardada: localStorage.getItem('seele.casca'),
        frase: document.querySelector('.casca-estado')?.textContent ?? '' }))()""")
    if visto["guardada"] != outra:
        falhas.append(f"a escolha guardou {visto['guardada']!r}, e não {outra!r}")
    if "próxima vez" not in visto["frase"] or "sai da sessão" not in visto["frase"]:
        falhas.append(f"a escolha não diz quando vale: {visto['frase']!r}")
    if visto["marca"] != marca:
        falhas.append(f"a casca trocou ao vivo, no meio da sessão: {marca} → {visto['marca']}")
    pg.reload()
    pg.wait_for_function("typeof conectar === 'function'")
    depois = pg.evaluate("document.documentElement.dataset.casca")
    if depois != outra:
        falhas.append(f"aberta de novo, a página veio como {depois}, e não como a escolhida ({outra})")
    return falhas


def escolha_sem_armazenamento(pg: Page) -> list[str]:
    """Sem armazenamento (navegação privada, cota cheia), a escolha diz que não
    pôde guardar, e nada muda."""
    falhas = []
    # Só a chave da escolha é recusada: o desktop também guarda coisas no
    # navegador, e recusar tudo quebraria ele, e não só a escolha.
    pg.evaluate("""(() => { const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function (chave, valor) {
            if (chave === 'seele.casca') throw new DOMException('cheio', 'QuotaExceededError');
            return original.call(this, chave, valor);
        }; })()""")
    ir_para_aba(pg, "ajustes", falhas)
    pg.wait_for_timeout(500)
    exigir_toque(pg, "#cel-secao-casca", falhas)
    pg.wait_for_timeout(200)
    exigir_toque(pg, "[data-casca-escolhida='larga']", falhas)
    pg.wait_for_timeout(200)
    frase = pg.evaluate("document.querySelector('.casca-estado')?.textContent ?? ''")
    if "não deixou guardar" not in frase:
        falhas.append(f"sem armazenamento, a escolha não diz que não guardou: {frase!r}")
    return falhas


CASOS: dict[str, Caso] = {
    "sessao-entrou": Caso(levar=sessao_entrou, medir=False),
    "faixa-de-voz": Caso(levar=faixa_de_voz, temas=("papel", "profundo", "claro", "hostil"),
                         raiz="#voz, .cel-abas, #aviso-fone, #aviso-audio"),
    "segurar": Caso(levar=segurar, raiz="#voz, .cel-abas"),
    # O claro voltou com a Tarefa 13: o azul da sala ocupada e o fósforo de quem
    # fala são cores de estado, derivadas do tema pela `tonsDoTema` da F3 (17e).
    # Sem ela, o claro reprovava o «VOCÊ ESTÁ AQUI» a 2,6:1.
    "aba-servidor": Caso(levar=aba_servidor, temas=("papel", "claro", "hostil"),
                         raiz=".cel-topo, .paineis > .trilha, .painel-canais, #voz, .cel-abas"),
    "aba-conversa": Caso(levar=aba_conversa, temas=("papel", "claro", "hostil"),
                         raiz=".cel-topo, #vista-conversa, #voz, .cel-abas"),
    "aba-pessoas": Caso(levar=aba_pessoas, temas=("papel", "claro", "hostil"),
                        raiz=".cel-topo, .painel-pessoas, #voz, .cel-abas"),
    "chamada": Caso(levar=chamada, temas=("papel", "claro", "hostil"),
                    raiz=".cel-topo, #vista-chamada, #voz, .cel-telemetria"),
    "ajustes-medida": Caso(levar=ajustes_medida, raiz=".cel-topo, #tela-server .server-coluna, #voz, .cel-abas"),
    # Termina fora do servidor (SAIR DO SERVIDOR): as medidas do aparelho
    # ficam com o ajustes-medida, que termina na lista.
    "aba-ajustes": Caso(levar=aba_ajustes, medir=False),
    # O aparelho (Tarefa 12). O texto do sistema a 150 % passa pelas abas já
    # adaptadas, com as mesmas medidas.
    "faixa-de-voz-150": Caso(levar=faixa_de_voz, escala=1.5, raiz="#voz, .cel-abas, #aviso-fone, #aviso-audio"),
    "aba-servidor-150": Caso(levar=aba_servidor, escala=1.5, raiz=".cel-topo, .paineis > .trilha, .painel-canais, #voz, .cel-abas"),
    "aba-conversa-150": Caso(levar=aba_conversa, escala=1.5, raiz=".cel-topo, #vista-conversa, #voz, .cel-abas"),
    "aba-pessoas-150": Caso(levar=aba_pessoas, escala=1.5, raiz=".cel-topo, .painel-pessoas, #voz, .cel-abas"),
    "chamada-150": Caso(levar=chamada, escala=1.5, raiz=".cel-topo, #vista-chamada, #voz, .cel-telemetria"),
    "ajustes-150": Caso(levar=ajustes_medida, escala=1.5, raiz=".cel-topo, #tela-server .server-coluna, #voz, .cel-abas"),
    # O retrato dos MODs diz onde a casca está: celular só com a marca do
    # celular; na larga e no desktop (os arquivos das cascas carregados, sem
    # marca), computador.
    "retrato-no-celular": Caso(levar=retrato_da_casca("celular"), aparelhos=("390",), medir=False),
    "retrato-na-larga": Caso(levar=retrato_da_casca("computador"), marca="larga", aparelhos=("1440",), medir=False),
    "retrato-no-desktop": Caso(levar=retrato_da_casca("computador"), marca="", aparelhos=("1440",), medir=False),
    # As superfícies de MOD acima da faixa e das abas (Tarefa 11; F3, 5d).
    "superficies-de-mod": Caso(levar=superficies_de_mod, raiz=".cel-topo, #voz, .cel-abas, #avisos-de-mod"),
    # As regiões de MOD que o celular declara, e a que ele não tem.
    "regioes-do-celular": Caso(levar=regioes_do_celular, aparelhos=("390",), medir=False),
    # A região operador.barra no alto de AJUSTES (F3, passo 8b).
    "operador-barra": Caso(levar=operador_barra, medir=False),
    # O tema de região do operador.barra nos contêineres que a casca declara.
    "regiao-do-operador": Caso(levar=regiao_do_operador, medir=False),
    # A capa de quem usa em SEU PERFIL (D-capa-1).
    "capa-do-operador": Caso(levar=capa_do_operador, medir=False),
    # O texto do sistema no maior tamanho (Tarefa 15): tudo à vista cresce.
    "texto-maior": Caso(levar=texto_maior, medir=False),
    "teclado": Caso(levar=teclado, raiz=".cel-topo, #voz, #form-mensagem"),
    "tela-larga": Caso(levar=tela_larga, aparelhos=("1280",), medir=False),
    "voltar": Caso(levar=voltar, medir=False),
    "deitado": Caso(levar=deitado, aparelhos=("deitado",), raiz="#palco, #voz"),
    # A casca larga (Tarefa 17): o computador, com a marca larga.
    "casca-larga": Caso(levar=casca_larga, marca="larga", aparelhos=("1440",), entrar=False, medir=False),
    # As camadas sobre a sessão e a confirmação (Tarefa 11).
    "queda": Caso(levar=queda, medir=False),
    "alerta": Caso(levar=alerta, medir=False),
    "confirmacao": Caso(levar=confirmacao, raiz="#moderar"),
    # A entrada do celular (Tarefa 10), sobre a identidade da F4.
    "entrada": Caso(levar=entrada, entrar=False, raiz="#tela-boot"),
    "entrada-desfechos": Caso(levar=entrada_desfechos, entrar=False, medir=False, http=True),
    # D-celular-3: a impressão conferida dispensa o aperto, só no celular.
    "entrada-direta": Caso(levar=entrada_direta, entrar=False, medir=False),
    "entrada-com-aperto": Caso(levar=entrada_com_aperto, entrar=False, raiz="#tela-auth"),
    "entrada-direta-falha": Caso(levar=entrada_direta_falha, entrar=False, raiz="#tela-auth"),
    "entrada-espera": Caso(levar=entrada_espera, entrar=False, raiz="#tela-auth"),
    "entrada-abre-a-sessao": Caso(levar=entrada_abre_a_sessao, entrar=False, medir=False),
    "entrada-direta-larga": Caso(levar=entrada_direta_na_larga, marca="larga", aparelhos=("1440",), entrar=False, medir=False),
    # A escolha da casca à mão (Tarefa 18), nas duas cascas, e sem armazenamento.
    # A tela cheia da larga sai junto com a do navegador (achado da F4).
    "larga-tela-cheia": Caso(levar=larga_tela_cheia, marca="larga", aparelhos=("1440",), medir=False),
    # O microfone negado pelo navegador, nas duas cascas (achado 3).
    "microfone-negado": Caso(levar=microfone_negado, aparelhos=("390",), medir=False),
    "microfone-negado-larga": Caso(levar=microfone_negado, marca="larga", aparelhos=("1440",), medir=False),
    # A capa de uma pessoa no cartão da CHAMADA (D-capa-1).
    "capa-na-chamada": Caso(levar=capa_na_chamada, medir=False),
    # O palco dos MODs na CHAMADA (chamada.palco, c7c64814 da F3).
    "palco-de-mods": Caso(levar=palco_de_mods, raiz="#vista-chamada"),
    # A sala pessoal da F7, que o navegador não tem, nas duas cascas.
    "sala-pessoal": Caso(levar=sala_pessoal, aparelhos=("390",), medir=False),
    "sala-pessoal-larga": Caso(levar=sala_pessoal, marca="larga", aparelhos=("1440",), medir=False),
    "escolha-no-celular": Caso(levar=escolha_da_casca, aparelhos=("390",), medir=False),
    "escolha-no-computador": Caso(levar=escolha_da_casca, marca="larga", aparelhos=("1440",), medir=False),
    "escolha-sem-armazenamento": Caso(levar=escolha_sem_armazenamento, aparelhos=("390",), medir=False),
}


def rodar_caso(navegador, temporaria: pathlib.Path, saida: pathlib.Path, nome: str, caso: Caso) -> list[str]:
    falhas = []
    for tamanho in caso.aparelhos:
        aparelho = APARELHOS[tamanho]
        nucleo_nativo = None
        for tema in (None, *caso.temas):
            rotulo = f"{nome}-{tamanho}" + (f"-{tema}" if tema else "")
            pagina = montar(temporaria / rotulo, celular=True, marca=caso.marca)
            ctx = navegador.new_context(
                viewport={"width": aparelho["largura"], "height": aparelho["altura"]},
                device_scale_factor=1, is_mobile=aparelho.get("celular", True), has_touch=aparelho.get("toque", True),
                reduced_motion="reduce", locale="pt-BR",
            )
            pg = ctx.new_page()
            # Cinco segundos, e não os trinta do Playwright: nesta casca nada
            # demora tanto, e um caso que espera um nó que não vem diz isso logo.
            pg.set_default_timeout(5000)
            erros = []
            pg.on("pageerror", lambda e: erros.append(str(e)[:200]))
            servindo = contextlib.ExitStack()
            if caso.http:
                origem = servindo.enter_context(servida(pagina.parent))
                pg.goto(f"{origem}/{pagina.name}")
            else:
                pg.goto(f"file://{pagina}")
            if caso.entrar:
                entrar(pg)
            pg.evaluate(
                f"""document.documentElement.style.setProperty('--cel-seguro-topo', '{aparelho['topo']}px');
                    document.documentElement.style.setProperty('--cel-seguro-pe', '{aparelho['pe']}px');"""
            )
            if caso.escala != 1.0:
                pg.evaluate(f"document.documentElement.style.setProperty('--cel-escala', '{caso.escala}')")
            if tema:
                aplicar_tema(pg, tema)
            # Um caso que estoura reprova com o motivo, e não derruba a bancada.
            # O mais comum é a página ter saído (um voltar que passou da
            # sessão): sem ela não há o que medir nem fotografar.
            try:
                motivos = caso.levar(pg)
                estourou = False
            except ErroDoPlaywright as erro:
                estourou = True
                try:
                    saiu = pg.evaluate("typeof window.__SEELE_HISTORICO") == "undefined"
                except ErroDoPlaywright:
                    saiu = True
                motivos = (["a página saiu da sessão no meio do caso: um voltar foi além dela"] if saiu
                           else [f"o caso estourou: {str(erro).splitlines()[0][:200]}"])
            if not estourou:
                pg.wait_for_timeout(250)
                pg.screenshot(path=saida / f"{rotulo}.png", animations="disabled")
            if caso.medir and not estourou:
                m = pg.evaluate(MEDIR, [aparelho["topo"], aparelho["pe"], caso.raiz])
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
            servindo.close()
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
