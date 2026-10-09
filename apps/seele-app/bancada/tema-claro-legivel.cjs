// O texto da sessão sob um tema claro, com o contraste **computado** pelo
// navegador, e não o que a validação do tema calcula.
//
// # Por que esta bancada existe
//
// Medido pela F8 e conferido pela F3 em 09/10/2026: com um tema claro que a
// própria validação aceita (`oTextoAlcancaOFundo()` devolve `true`), o corpo
// das mensagens do desktop saía em `rgb(234,227,207)` sobre `rgb(242,236,224)`,
// 1,09:1, e o SAIR DA SALA da barra do operador a 1,04:1. O `body` resolve
// `color: var(--seele-osso)` com o valor da raiz, o tema redefine o token em
// `#tela-sessao`, abaixo dele, e quem não declara cor herda a já resolvida. A
// validação passava e a tela não acompanhava — e o defeito estava na 0.15.1
// publicada, igual.
//
// # O que ela mede
//
// Três lugares que herdavam a cor resolvida acima de `#tela-sessao`: o corpo de
// uma mensagem, o SAIR DA SALA e o nome de uma pessoa no roster. Cada um precisa
// de 4,5:1 contra o primeiro fundo opaco acima dele. As cores de estado (o
// fósforo da sincronia) ficam de fora: elas são derivadas do tema na tarefa 17e
// do plano da F3.
//
// Rodar:  node apps/seele-app/bancada/tema-claro-legivel.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/** Um tema claro completo, de seis cores, que a validação aceita. */
const TEMA_CLARO = {
  fundo: "#f2ece0",
  painel: "#e6dfd0",
  texto: "#12100e",
  apagado: "#5a5148",
  acento: "#8a4a00",
  borda: "#b8ad9c",
};

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (erro) => {
      throw erro;
    });
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.__TAURI__ = {
          core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
          event: { listen: async () => () => {} },
          window: {
            getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }),
          },
        };
      },
      respostas({
        snapshot: null,
        microfones: [],
        saidas: [],
        pacotes_no_cache: [],
        aceites_de_mods: [],
        limite_da_mensagem: 4096,
        previas_consentidas: [],
        som_da_tela: { volume: 1, calada: false },
        exclusao_do_som_da_captura: "Excluido",
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());
    await page.waitForSelector(".mensagens .corpo");

    const medido = await page.evaluate((tema) => {
      aplicarOTemaDoMod("seele/estilo", tema);
      const rgb = (s) => (s.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) =>
        [r, g, b]
          .map((c) => {
            const v = c / 255;
            return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
          })
          .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => {
        const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
        return (x + 0.05) / (y + 0.05);
      };
      const fundoDe = (el) => {
        for (let n = el; n && n !== document.documentElement; n = n.parentElement) {
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] > 0.5)) return c.slice(0, 3);
        }
        return rgb(getComputedStyle(document.body).backgroundColor).slice(0, 3);
      };
      const medir = (seletor) => {
        const el = document.querySelector(seletor);
        if (!el) return { seletor, achado: false };
        const cor = getComputedStyle(el).color;
        const fundo = fundoDe(el);
        return {
          seletor,
          achado: true,
          cor,
          fundo: `rgb(${fundo.join(",")})`,
          razao: Number(razao(rgb(cor).slice(0, 3), fundo).toFixed(2)),
        };
      };
      return {
        legivel: oTextoAlcancaOFundo(),
        lugares: [
          medir(".mensagens .corpo"),
          medir("#operador-sair"),
          medir(".roster .pessoa-nome"),
        ],
      };
    }, TEMA_CLARO);

    assert.equal(medido.legivel, true, "a validação recusou o tema claro de teste");
    for (const lugar of medido.lugares) {
      assert.ok(lugar.achado, `«${lugar.seletor}» não está na sessão desenhada`);
      assert.ok(
        lugar.razao >= 4.5,
        `«${lugar.seletor}» sai a ${lugar.razao}:1 sob o tema claro (${lugar.cor} sobre ` +
          `${lugar.fundo}): a cor foi resolvida acima de #tela-sessao, onde o tema não chega`,
      );
    }
    console.log(
      "tema claro legível:",
      medido.lugares.map((l) => `${l.seletor} ${l.razao}:1`).join(" · "),
    );
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro.message ?? erro);
  process.exit(1);
});
