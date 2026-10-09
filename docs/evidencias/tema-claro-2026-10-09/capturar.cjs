// Captura a sessão do desktop, sem tema e com o tema claro completo, numa raiz:
//   PLAYWRIGHT=... node capturar.cjs <raiz> <prefixo>
const path = require("node:path");
const raiz = path.resolve(process.argv[2]);
const prefixo = process.argv[3];
const { servir, respostas, retrato } = require(path.join(raiz, "apps/seele-app/bancada/telas.cjs"));
const chromium = require(process.env.PLAYWRIGHT).chromium;
(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const largura of [1280, 900]) {
      const page = await browser.newPage({ viewport: { width: largura, height: 860 } });
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
        // O relógio da barra e o uptime mudam a cada segundo: parados, para a
        // comparação medir o desenho e não o tempo.
        document.querySelectorAll("#barra-relogio, #tel-uptime").forEach((n) => { n.textContent = "00:00:00"; });
      }, retrato());
      await page.waitForSelector(".mensagens .corpo");
      await page.waitForTimeout(800);
      await page.evaluate(() => document.querySelectorAll("#barra-relogio, #tel-uptime").forEach((n) => { n.textContent = "00:00:00"; }));
      await page.screenshot({ path: `${prefixo}-sem-tema-${largura}.png`, animations: "disabled", caret: "hide" });
      await page.close();
    }
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error(e); process.exit(1); });
