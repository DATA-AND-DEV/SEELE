// Abre a página do anfitrião num navegador real do Playwright e imprime o
// resultado em JSON.
//
//   node correr.cjs <chromium|webkit> <eco|adulterar|sobe|desce> [fp|-] [ip] [furar 0|1] [extra-query] [args-do-chrome]
//
// `fp` = `-` pede a impressão ao anfitrião (`GET /fp`).
//
// De onde vem o Playwright (nunca da pasta pessoal de alguém, como em
// apps/seele-app/bancada/playwright.cjs):
//   · SPIKE_PLAYWRIGHT_CHROMIUM / SPIKE_PLAYWRIGHT_WEBKIT, se postas;
//   · senão, `node_modules/` (playwright 1.61 = Chrome for Testing 149, chromium-1228)
//     e `pw162/node_modules/` (playwright 1.62 = WebKit 2336) DESTA pasta.
// Ver README.md para instalar sem baixar navegador.
const path = require("path");
const [, , navegador, modo, fpArg = "-", ip = "127.0.0.1", furar = "0", extra = "", chromeArgs = ""] = process.argv;
const HTTP = 18384, UDP = 18383;
let fp = fpArg;

const mod = navegador === "webkit"
  ? require(process.env.SPIKE_PLAYWRIGHT_WEBKIT || path.join(__dirname, "pw162/node_modules/playwright"))
  : require(process.env.SPIKE_PLAYWRIGHT_CHROMIUM || path.join(__dirname, "node_modules/playwright"));

(async () => {
  if (fp === "-") fp = await (await fetch(`http://127.0.0.1:${HTTP}/fp`)).text();
  const tipo = mod[navegador];
  const args = navegador === "chromium" ? chromeArgs.split(" ").filter(Boolean) : [];
  const browser = await tipo.launch({ headless: true, args });
  const page = await browser.newPage();
  const consola = [];
  page.on("console", (m) => consola.push(`${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => consola.push(`pageerror: ${e}`));
  const url = `http://127.0.0.1:${HTTP}/?fp=${fp}&porta=${UDP}&ip=${ip}&modo=${modo}&furar=${furar}${extra ? "&" + extra : ""}`;
  const t = Date.now();
  await page.goto(url);
  let r = null;
  try {
    await page.waitForFunction(() => window.__resultado !== null, null, { timeout: 180000, polling: 200 });
    r = await page.evaluate(() => window.__resultado);
  } catch (e) {
    r = { erro: "sem resultado: " + e.message, log: await page.evaluate(() => document.getElementById("saida").textContent) };
  }
  r.navegador = `${navegador} ${browser.version()}`;
  r.ms_total = Date.now() - t;
  r.consola = consola;
  console.log(JSON.stringify(r, null, 1));
  await browser.close();
})().catch((e) => { console.error(e); process.exit(1); });
