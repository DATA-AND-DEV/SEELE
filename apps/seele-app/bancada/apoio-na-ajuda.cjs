// **O apoio, na ajuda.**
//
// A UI é a real; a ponte é um dublê do aplicativo. O que ela prende:
//
//  1. A ajuda mostra o botão de apoio, e ele cabe na caixa (visível, sem rolar).
//  2. Apertar pede ao aplicativo para abrir **a página de apoio do site**, em
//     `https`, pelo `abrir_no_navegador` — e não navega esta janela.
//  3. Se o sistema não abre o navegador, a ajuda **diz o endereço** em vez de
//     ficar calada; e a próxima abertura da ajuda não herda o aviso.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/apoio-na-ajuda.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (erro) => erros.push(String(erro)));
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.testCalls = [];
        window.falharAoAbrir = false;
        window.__TAURI__ = {
          core: {
            invoke: async (cmd, args) => {
              window.testCalls.push({ cmd, args });
              if (cmd === "abrir_no_navegador" && window.falharAoAbrir) throw new Error("sem navegador");
              return Object.hasOwn(table, cmd) ? table[cmd] : null;
            },
          },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      },
      respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    const inicio = page.url();
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());

    const aberturas = () =>
      page.evaluate(() => window.testCalls.filter((c) => c.cmd === "abrir_no_navegador").map((c) => c.args?.url));

    // 1. O botão está na ajuda, e cabe na caixa.
    await page.evaluate(() => abrirAjuda());
    const botao = page.locator("#ajuda-apoiar");
    assert.ok(await botao.isVisible(), "a ajuda não mostra o botão de apoio");
    const cabe = await page.evaluate(() => {
      const b = $("ajuda-apoiar").getBoundingClientRect();
      const c = document.querySelector(".ajuda-caixa").getBoundingClientRect();
      return b.top >= c.top && b.bottom <= c.bottom && b.left >= c.left && b.right <= c.right;
    });
    assert.ok(cabe, "o botão de apoio ficou fora da caixa da ajuda");
    assert.ok(await page.evaluate(() => $("ajuda-apoio-estado").hidden), "o aviso nasceu à mostra");

    // 2. Apertar abre a página de apoio do site, fora desta janela.
    await botao.click();
    let abertas = await aberturas();
    assert.equal(abertas.length, 1, `o aperto não pediu uma abertura: ${JSON.stringify(abertas)}`);
    const destino = new URL(abertas[0]);
    assert.equal(destino.protocol, "https:");
    assert.equal(destino.host, "get.seele.sh");
    assert.equal(destino.hash, "#apoie");
    assert.equal(page.url(), inicio, "o aperto navegou esta janela");
    assert.ok(await page.evaluate(() => $("ajuda-apoio-estado").hidden), "abriu, e a ajuda disse que não");

    // 3. Sem navegador, a ajuda diz o endereço.
    await page.evaluate(() => { window.falharAoAbrir = true; });
    await botao.click();
    await page.waitForFunction(() => !$("ajuda-apoio-estado").hidden);
    const dito = await page.evaluate(() => $("ajuda-apoio-estado").textContent);
    assert.match(dito, /get\.seele\.sh\/#apoie/, `o aviso não diz o endereço: ${dito}`);
    await page.evaluate(() => { fecharAjuda(); abrirAjuda(); });
    assert.ok(await page.evaluate(() => $("ajuda-apoio-estado").hidden), "a ajuda reabriu com o aviso da vez passada");

    assert.deepEqual(erros, [], `a página lançou: ${erros.join(" | ")}`);
    console.log("apoio na ajuda: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
