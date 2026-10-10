// Mede o contraste computado do texto da sessão sob um tema claro, numa raiz de
// checkout qualquer:  PLAYWRIGHT=... node medir.cjs <raiz>
const path = require("node:path");
const raiz = path.resolve(process.argv[2]);
const { servir, respostas, retrato } = require(path.join(raiz, "apps/seele-app/bancada/telas.cjs"));
const chromium = require(process.env.PLAYWRIGHT).chromium;
const TEMA = process.env.TEMA ? JSON.parse(process.env.TEMA) : { texto: "#12100e", fundo: "#f2ece0" };

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  const erros = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (e) => erros.push(String(e)));
    await page.addInitScript((table) => {
      window.testTable = table;
      window.__TAURI__ = {
        core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [],
      limite_da_mensagem: 4096, previas_consentidas: [], som_da_tela: { volume: 1, calada: false },
      exclusao_do_som_da_captura: "Excluido" }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());
    await page.waitForTimeout(600);
    const resultado = await page.evaluate((tema) => {
      const aceito = (() => { try { aplicarOTemaDoMod("seele/estilo", tema); return true; } catch (e) { return String(e.message || e); } })();
      const legivel = typeof oTextoAlcancaOFundo === "function" ? oTextoAlcancaOFundo() : "?";
      const rgb = (s) => (s.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) => [r, g, b].map((c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; })
        .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
      const fundoDe = (el) => {
        for (let n = el; n && n !== document.documentElement; n = n.parentElement) {
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] > 0.5)) return c.slice(0, 3);
        }
        return rgb(getComputedStyle(document.body).backgroundColor).slice(0, 3);
      };
      const nome = (el) => el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (el.className && typeof el.className === "string" ? "." + el.className.trim().split(/\s+/).join(".") : "");
      const caminho = (el) => { const p = []; for (let n = el; n && n.id !== "tela-sessao"; n = n.parentElement) p.unshift(nome(n)); return p.slice(-3).join(" > "); };
      const vistos = [];
      for (const el of $("tela-sessao").querySelectorAll("*")) {
        const texto = [...el.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent.trim()).join("");
        if (!texto) continue;
        const r = el.getBoundingClientRect();
        const est = getComputedStyle(el);
        if (r.width === 0 || r.height === 0 || est.visibility === "hidden" || est.display === "none") continue;
        const cor = rgb(est.color).slice(0, 3);
        const fundo = fundoDe(el);
        vistos.push({ onde: caminho(el), texto: texto.slice(0, 24), cor: est.color, fundo: `rgb(${fundo.join(",")})`, razao: Number(razao(cor, fundo).toFixed(2)) });
      }
      vistos.sort((a, b) => a.razao - b.razao);
      const corpo = vistos.find((v) => v.onde.includes("span.corpo"));
      return { aceito, legivel, corpo, piores: vistos.slice(0, 12), total: vistos.length,
        abaixoDe3: vistos.filter((v) => v.razao < 3).length };
    }, TEMA);
    console.log(JSON.stringify({ raiz: path.basename(raiz), erros: erros.slice(0, 3), ...resultado }, null, 1));
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error(e); process.exit(1); });
