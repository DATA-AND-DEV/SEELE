#!/usr/bin/env python3
"""Fotografa cada estado do protótipo e mede o que um celular reprova.

**Descartável**, como o protótipo. Ele existe para o spec do celular ter as
fotos e para cada estado do desenho passar pelas mesmas cinco medidas que a
revisão de 05/10/2026 (`arquivo/mobile-ios:docs/revisao-celular-2026-10-05.md`)
achou faltando:

- **página rolando de lado**: algo passa da largura sem estar num trilho que
  rola de lado;
- **alvos de toque** abaixo de 44 × 44 (o `range` e o texto corrido de fora);
- **as áreas seguras**: controle ou texto dentro da ilha (em cima) ou da barra de
  gestos (embaixo). O Chromium não tem área segura — `env(safe-area-inset-*)`
  vale zero nele —, e o protótipo passa as quatro por variáveis
  (`--cel-seguro-*`), então aqui elas recebem as medidas de um iPhone;
- **o núcleo sob tema**: o MUDO, o SURDO e o SAIR têm, com qualquer tema, a
  mesma cor computada que sem tema, texto e fundo a pelo menos 4,5:1, e a borda
  a pelo menos 3:1 contra o fundo da faixa (condição da orquestradora, 09/10);
- **erros** de script.

Uso:
    python3 spikes/casca-do-celular/fotos.py /tmp/fotos-do-celular
    python3 spikes/casca-do-celular/fotos.py /tmp/fotos --so 11-conversa

Sai com 1 se alguma medida reprovar.
"""
import argparse
import functools
import http.server
import json
import pathlib

import sys
import threading

from playwright.sync_api import sync_playwright

RAIZ = pathlib.Path(__file__).resolve().parents[2]
CAMINHO = "/spikes/casca-do-celular/"

# As áreas seguras de um iPhone em retrato: a ilha em cima, a barra de gestos
# embaixo. O 360 × 800 é um Android comum, com a barra de navegação por gestos.
TAMANHOS = {
    "390": {"largura": 390, "altura": 844, "topo": 59, "pe": 34},
    "360": {"largura": 360, "altura": 800, "topo": 24, "pe": 24},
}

ESTADOS = {
    "01-inicio": "tela=inicio",
    "02-entrar": "tela=entrar&convite=pronto",
    "03-entrar-numa-aba": "tela=entrar&convite=numa-aba",
    "04-entrar-vencido": "tela=entrar&convite=vencido",
    "05-entrar-servidor-0.15": "tela=entrar&convite=servidor-0.15",
    "06-entrar-apelido-tomado": "tela=entrar&convite=apelido-tomado",
    "07-conectando": "tela=conectando",
    "08-aceite": "tela=aceite",
    "09-servidor": "tela=sessao&aba=servidor&sala=1",
    "10-servidor-fora-de-sala": "tela=sessao&aba=servidor",
    "11-conversa": "tela=sessao&aba=conversa&sala=1",
    "12-conversa-teclado": "tela=sessao&aba=conversa&sala=1&teclado=1",
    "13-pessoas": "tela=sessao&aba=pessoas&sala=1",
    "14-ajustes": "tela=sessao&aba=ajustes&sala=1",
    "15-ajustes-audio": "tela=sessao&aba=ajustes&sala=1&sub=audio&modo=segurar",
    "16-no-computador": "tela=sessao&aba=ajustes&sala=1&sub=computador",
    "17-chamada": "tela=sessao&aba=conversa&sala=1&chamada=1",
    "18-folha-da-pessoa": "tela=sessao&aba=pessoas&sala=1&folha=pessoa:3",
    "19-folha-da-mensagem": "tela=sessao&aba=conversa&sala=1&folha=mensagem:2",
    "20-aparencia-do-servidor": "tela=sessao&aba=ajustes&sala=1&mod=dialogo",
    "21-previa-do-tema": "tela=sessao&aba=conversa&sala=1&previa=1&tema=profundo",
    "22-queda": "tela=sessao&aba=conversa&sala=1&queda=1",
    "23-pagina-de-mod": "tela=sessao&aba=conversa&sala=1&mod=pagina",
    "24-aviso-de-mod": "tela=sessao&aba=servidor&sala=1&mod=aviso",
    "25-fim": "tela=fim",
    "26-segurar-para-falar": "tela=sessao&aba=pessoas&sala=1&modo=segurar",
    "27-mudo": "tela=sessao&aba=conversa&sala=1&mudo=1",
    "28-este-servidor": "tela=sessao&aba=ajustes&sala=1&sub=servidor",
    "29-conexao": "tela=sessao&aba=ajustes&sub=conexao",
    "30-sala-lotada": "tela=sessao&aba=servidor",
    "31-tela-nao-abre": "tela=sessao&aba=conversa&sala=1&chamada=1&palco=nao-abre",
}

# Os estados que levam também os dois temas do ESTILO e o hostil.
COM_TEMA = ["09-servidor", "11-conversa", "13-pessoas", "14-ajustes", "17-chamada", "27-mudo", "28-este-servidor"]
TEMAS = ["papel", "profundo", "hostil", "claro"]
# O texto dinâmico a 150 %, em cinco estados.
COM_TEXTO_GRANDE = ["11-conversa", "13-pessoas", "17-chamada", "14-ajustes", "26-segurar-para-falar"]

MEDIR = r"""
([topo, pe]) => {
  const W = innerWidth, H = innerHeight;
  const vis = (e) => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && !e.closest("[hidden]"); };
  const nome = (e) => (e.id ? "#" + e.id : e.tagName.toLowerCase() + (typeof e.className === "string" && e.className ? "." + e.className.trim().split(/\s+/)[0] : ""))
    + " «" + (e.getAttribute("aria-label") || e.textContent || e.placeholder || "").trim().replace(/\s+/g, " ").slice(0, 28) + "»";
  // **Só um trilho marcado rola de lado.** Um painel com `overflow-y: auto` tem
  // `overflow-x: auto` computado também, e contar isso como trilho escondia
  // todo transbordo dentro de um painel que rola — foi assim que a impressão
  // cortada da tela ENTRAR EM passou numa versão anterior desta medida.
  const rolaX = (e) => !!e.parentElement?.closest("[data-rola-de-lado]");
  const rolaY = (e) => { for (let p = e.parentElement; p; p = p.parentElement) { const s = getComputedStyle(p);
      if (/(auto|scroll)/.test(s.overflowY) && p.scrollHeight > p.clientHeight) return true; } return false; };
  const app = document.getElementById("app").getBoundingClientRect();
  // O teclado falso faz o papel do sistema: ele é que ocupa a barra de gestos.
  const todos = [...document.querySelectorAll("#app *")].filter(vis).filter((e) => !e.closest(".cel-teclado-falso"));
  const vaza = todos.filter((e) => { const r = e.getBoundingClientRect(); return (r.right > app.right + 1 || r.left < app.left - 1) && !rolaX(e); })
    .filter((e) => ![...e.children].some((c) => vis(c) && c.getBoundingClientRect().right > app.right + 1));
  const tocavel = (e) => { const r = e.getBoundingClientRect(); const x = Math.min(Math.max(r.left + r.width / 2, 1), W - 1), y = r.top + r.height / 2;
      if (y < 0 || y > H) return false; const alvo = document.elementFromPoint(x, y); return alvo && (e === alvo || e.contains(alvo)); };
  const controles = todos.filter((e) => e.matches("button, a[href], input:not([type=hidden]), select, textarea, summary, [role=button]")).filter(tocavel);
  const pequenos = controles.filter((e) => { const r = e.getBoundingClientRect(); return (r.width < 44 || r.height < 44) && !e.matches("input[type=range]"); });
  const temTexto = (e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
  const naIlha = todos.filter((e) => (controles.includes(e) || temTexto(e)) && e.getBoundingClientRect().top < topo - 1 && e.getBoundingClientRect().bottom > 0 && !rolaY(e))
    .map((e) => nome(e) + " y=" + Math.round(e.getBoundingClientRect().top));
  const naBarra = todos.filter((e) => (controles.includes(e) || temTexto(e)) && e.getBoundingClientRect().bottom > H - pe + 1 && e.getBoundingClientRect().top < H && !rolaY(e))
    .map((e) => nome(e) + " fim=" + Math.round(e.getBoundingClientRect().bottom));
  // **Texto que passa da própria caixa**, mesmo dentro da tela: duas abas
  // vizinhas com o texto a 150 % se sobrepunham sem nada passar da largura da
  // tela, e a medida de «rola de lado» não via.
  const transborda = todos.filter((e) => temTexto(e) && !rolaX(e) && getComputedStyle(e).overflowX === "visible"
      && e.scrollWidth > e.clientWidth + 1 && e.clientWidth > 0)
    .map((e) => nome(e) + ` ${e.scrollWidth}>${e.clientWidth}`);
  // **O contraste de todo texto visível** contra o fundo opaco de verdade
  // atrás dele: 4,5:1, ou 3:1 para texto grande (WCAG 1.4.3). É a medida que vê
  // o produto escrevendo uma cor fixa sobre uma superfície que o tema pintou.
  // `rgb(…)` em 0–255, ou `color(srgb …)` em 0–1 (o que um `color-mix` devolve):
  // ler o segundo como o primeiro dava um fundo quase preto onde ele é claro.
  const rgba = (c) => { const n = (c.match(/[\d.]+/g) || []).map(Number);
    return c.startsWith("color(srgb") ? n.map((v, i) => (i < 3 ? v * 255 : v)) : n; };
  const luzDe = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const fundoDe = (e) => { for (let p = e; p; p = p.parentElement) { const s = getComputedStyle(p);
      if (p.classList.contains("cel-capa") || s.backgroundImage !== "none" && !p.classList.contains("varredura")) return null;
      const c = rgba(s.backgroundColor); if (c.length >= 3 && (c.length < 4 || c[3] >= 0.99)) return c; } return [5, 4, 3]; };
  const baixo = [];
  for (const e of todos) {
    if (!temTexto(e) || e.closest("[aria-hidden='true'], .cel-teclado-falso") || !tocavel(e) && !temTexto(e)) continue;
    const s = getComputedStyle(e); const cor = rgba(s.color); if (cor.length >= 4 && cor[3] < 0.99) continue;
    if (parseFloat(s.opacity) < 1 || e.closest(".cel-painel") && e.closest("[data-queda]")) continue;
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

NUCLEO = r"""
() => {
  const rgb = (c) => (c.match(/[\d.]+/g) || []).slice(0, 3).map(Number);
  const luz = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const razao = (a, b) => { const [x, y] = [luz(rgb(a)), luz(rgb(b))].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
  const out = {};
  for (const id of ["voz-mudo", "voz-surdo", "voz-sair", "selo-seguro", "selo-seguro-chamada", "cel-aba-ajustes", "linha-mods"]) {
    const el = document.getElementById(id);
    if (!el) continue;
    const s = getComputedStyle(el);
    // A borda contra o que está em volta do controle: o fundo opaco mais
    // próximo acima dele (a placa do núcleo, se ela existir).
    let em = el.parentElement;
    while (em && /rgba\(0, 0, 0, 0\)|transparent/.test(getComputedStyle(em).backgroundColor)) em = em.parentElement;
    const emVolta = em ? getComputedStyle(em).backgroundColor : "rgb(0, 0, 0)";
    out[id] = { cor: s.color, fundo: s.backgroundColor, borda: s.borderTopColor, emVolta,
                texto: Number(razao(s.color, s.backgroundColor).toFixed(2)),
                bordaContraFaixa: Number(razao(s.borderTopColor, emVolta).toFixed(2)) };
  }
  return Object.keys(out).length ? out : null;
}
"""


class Calado(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


class Servidor(http.server.ThreadingHTTPServer):
    # A fila padrão do Python é 5, e uma página com doze arquivos estoura com
    # `ERR_CONNECTION_RESET` quando duas cargas se encostam.
    request_queue_size = 128
    daemon_threads = True


def servir():
    # Um fio por conexão: o Chromium abre conexões de antemão e as deixa
    # paradas, e um servidor de um fio só fica preso na primeira delas.
    manipulador = functools.partial(Calado, directory=str(RAIZ))
    servidor = Servidor(("127.0.0.1", 0), manipulador)
    threading.Thread(target=servidor.serve_forever, daemon=True).start()
    return servidor


def main() -> int:
    argumentos = argparse.ArgumentParser(description=__doc__)
    argumentos.add_argument("saida", type=pathlib.Path)
    argumentos.add_argument("--so", default=None)
    opcoes = argumentos.parse_args()
    opcoes.saida.mkdir(parents=True, exist_ok=True)

    casos = []
    for nome, consulta in ESTADOS.items():
        if opcoes.so and nome != opcoes.so:
            continue
        for tamanho in TAMANHOS:
            casos.append((f"{nome}-{tamanho}", consulta, tamanho))
        if nome in COM_TEMA:
            for tema in TEMAS:
                casos.append((f"{nome}-{tema}-390", f"{consulta}&tema={tema}", "390"))
        if nome in COM_TEXTO_GRANDE:
            casos.append((f"{nome}-texto150-390", f"{consulta}&texto=1.5", "390"))
    # A casca do celular numa tela grande: um tablet, ou a escolha à mão. O
    # computador recebe a casca larga (D-web-3), que a prova sobre a casca
    # fotografa.
    if not opcoes.so:
        casos.append(("11-conversa-larga", ESTADOS["11-conversa"], "larga"))

    servidor = servir()
    base = f"http://127.0.0.1:{servidor.server_address[1]}{CAMINHO}"
    relatorio, falhas, nucleo_nativo = {}, [], {}
    with sync_playwright() as p:
        navegador = p.chromium.launch()
        for nome, consulta, tamanho in casos:
            medida = TAMANHOS.get(tamanho, {"largura": 1280, "altura": 800, "topo": 0, "pe": 0})
            pagina = navegador.new_page(viewport={"width": medida["largura"], "height": medida["altura"]},
                                        device_scale_factor=1, is_mobile=tamanho != "larga", has_touch=tamanho != "larga")
            erros = []
            pagina.on("pageerror", lambda e: erros.append(str(e)[:200]))
            for tentativa in range(2):
                pagina.goto(base + "?" + consulta)
                try:
                    pagina.wait_for_function("document.documentElement.dataset.pronto === 'sim'", timeout=10000)
                    break
                except Exception:
                    if tentativa:
                        raise
            pagina.evaluate(
                f"""document.documentElement.style.setProperty('--cel-seguro-topo', '{medida['topo']}px');
                    document.documentElement.style.setProperty('--cel-seguro-pe', '{medida['pe']}px');""")
            pagina.wait_for_timeout(250)
            pagina.screenshot(path=opcoes.saida / f"{nome}.png")
            m = pagina.evaluate(MEDIR, [medida["topo"], medida["pe"]])
            m["erros"] = erros
            m["nucleo"] = pagina.evaluate(NUCLEO)
            relatorio[nome] = m
            pagina.close()

            motivos = []
            if m["vaza"] or m["larguraDoDocumento"] > medida["largura"]:
                motivos.append(f"rola de lado: {m['vaza']}")
            # O hostil é o caso do §8: a API 6 o recusa pelos pares, e o
            # celular só fala a 6. Nele, a medida é só a do núcleo.
            if m["contraste"] and "-hostil-" not in nome:
                motivos.append(f"texto abaixo do contraste: {m['contraste']}")
            if m["transborda"]:
                motivos.append(f"texto passa da caixa: {m['transborda']}")
            if m["pequenos"]:
                motivos.append(f"alvos abaixo de 44: {m['pequenos']}")
            if m["naIlha"]:
                motivos.append(f"na ilha: {m['naIlha']}")
            if m["naBarra"]:
                motivos.append(f"na barra de gestos: {m['naBarra']}")
            if erros:
                motivos.append(f"erros: {erros}")
            for id_, n in (m["nucleo"] or {}).items():
                if n["texto"] < 4.5:
                    motivos.append(f"{id_}: texto {n['texto']}:1")
                # A borda contra a vizinhança é critério de controle (WCAG
                # 1.4.11); o selo da conexão segura é rótulo, e conta só o texto.
                if id_.startswith("voz-") and n["bordaContraFaixa"] < 3:
                    motivos.append(f"{id_}: borda {n['bordaContraFaixa']}:1 contra o que está em volta")
            if motivos:
                falhas.append((nome, motivos))

        # O núcleo com tema tem de ser o núcleo sem tema, cor por cor.
        for nome, m in relatorio.items():
            if not m["nucleo"]:
                continue
            base_nome = nome
            for tema in TEMAS:
                base_nome = base_nome.replace(f"-{tema}", "")
            nativo = relatorio.get(base_nome, {}).get("nucleo")
            if nativo and nome != base_nome:
                for id_, n in m["nucleo"].items():
                    if id_ not in nativo:
                        continue
                    chaves = ("cor", "fundo", "borda", "emVolta") if id_.startswith("voz-") else ("cor", "fundo", "borda")
                    a = {k: n[k] for k in chaves}
                    b = {k: nativo[id_][k] for k in chaves}
                    if a != b:
                        falhas.append((nome, [f"{id_} mudou com o tema: {a} contra {b}"]))
        navegador.close()
    servidor.shutdown()

    (opcoes.saida / "relatorio.json").write_text(json.dumps(relatorio, ensure_ascii=False, indent=2))
    print(f"{len(casos)} fotos em {opcoes.saida}")
    for nome, motivos in falhas:
        print(f"REPROVA {nome}")
        for motivo in motivos:
            print(f"    {motivo}")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
