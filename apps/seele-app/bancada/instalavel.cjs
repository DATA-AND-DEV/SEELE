// **A página instalável** (o celular da 1.0 é web, sem loja).
//
// A UI é a real; a ponte é um dublê do aplicativo. O que ela prende:
//
//  1. A página aponta um manifesto que se lê, com nome, começo, janela própria
//     e ícones que **existem** no tamanho que declaram; e o ícone do iOS.
//  2. No iOS a página aponta o outro manifesto, o que abre na aba do Safari: a
//     janela própria de lá perde o microfone ao sair da frente.
//  3. Os ajustes do celular têm «PÔR NA TELA INICIAL». Sem o convite do
//     navegador ela diz o caminho (o do iOS, no iOS); com o convite, o botão o
//     abre, e o que a pessoa escolheu fica escrito.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/instalavel.cjs

const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

const IPHONE =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

/** A largura e a altura de um PNG, do cabeçalho dele. */
function medidasDoPng(bytes) {
  assert.equal(bytes.subarray(1, 4).toString("latin1"), "PNG", "o ícone não é um PNG");
  return `${bytes.readUInt32BE(16)}x${bytes.readUInt32BE(20)}`;
}

async function abrir(browser, base, opcoes = {}) {
  const contexto = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, ...opcoes });
  const page = await contexto.newPage();
  const erros = [];
  page.on("pageerror", (erro) => erros.push(String(erro)));
  await page.addInitScript(
    (table) => {
      window.testTable = table;
      window.__TAURI__ = {
        core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    },
    respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }),
  );
  // A casca do celular, como a ponte a marcaria num aparelho de toque.
  await page.route("**/", (rota) =>
    rota.fulfill({
      contentType: "text/html",
      body: fs.readFileSync(path.join(__dirname, "../ui/index.html"), "utf8").replace('<html lang="pt-BR">', '<html lang="pt-BR" data-casca="celular">'),
    }),
  );
  await page.goto(base);
  await page.evaluate((r) => {
    window.testTable.snapshot = r;
    $("tela-boot").hidden = true;
    $("tela-sessao").hidden = false;
    desenhar(r);
  }, retrato());
  return { contexto, page, erros };
}

const secao = (page) =>
  page.evaluate(() => {
    const painel = document.getElementById("cel-painel-instalar");
    return {
      existe: document.getElementById("cel-secao-instalar") !== null && painel !== null,
      botaoEscondido: painel?.querySelector("button")?.hidden ?? null,
      dito: painel?.querySelector('[role="status"]')?.textContent ?? null,
    };
  });

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const base = `http://127.0.0.1:${server.address().port}/`;
  const browser = await chromium.launch({ headless: true });
  try {
    // ---- 1. o manifesto e os ícones
    const android = await abrir(browser, base);
    const apontado = await android.page.evaluate(() => document.querySelector('link[rel="manifest"]')?.href ?? null);
    assert.ok(apontado?.endsWith("/seele.webmanifest"), `a página não aponta o manifesto: ${apontado}`);
    const manifesto = await (await fetch(apontado)).json();
    assert.equal(manifesto.name, "SEELE");
    assert.equal(manifesto.display, "standalone", "fora do iOS a página instalada abre em janela própria");
    assert.equal(manifesto.start_url, "/");
    assert.equal(manifesto.scope, "/");
    const tamanhos = manifesto.icons.map((icone) => icone.sizes).sort();
    assert.deepEqual(tamanhos, ["192x192", "512x512"], "o Chrome pede um ícone de 192 e um de 512 para instalar");
    for (const icone of manifesto.icons) {
      const bytes = Buffer.from(await (await fetch(new URL(icone.src, apontado))).arrayBuffer());
      assert.equal(medidasDoPng(bytes), icone.sizes, `o ícone ${icone.src} não tem o tamanho que declara`);
    }
    const doIos = await android.page.evaluate(() => document.querySelector('link[rel="apple-touch-icon"]')?.href ?? null);
    assert.ok(doIos, "a página não aponta o ícone do iOS");
    assert.equal(medidasDoPng(Buffer.from(await (await fetch(doIos)).arrayBuffer())), "180x180");

    // ---- 3a. a seção, sem o convite do navegador: diz o caminho
    let vista = await secao(android.page);
    assert.ok(vista.existe, "os ajustes do celular não têm a seção de instalar");
    assert.equal(vista.botaoEscondido, true, "sem convite do navegador, o botão de instalar apareceu");
    assert.match(vista.dito, /menu do navegador/, `sem convite, a seção não diz o caminho: ${vista.dito}`);

    // ---- 3b. com o convite: o botão o abre, e a escolha fica escrita
    const comConvite = async (desfecho) => {
      await android.page.evaluate((desfecho) => {
        window.abriuOConvite = 0;
        const evento = new Event("beforeinstallprompt", { cancelable: true });
        evento.prompt = async () => { window.abriuOConvite += 1; };
        evento.userChoice = Promise.resolve({ outcome: desfecho });
        window.dispatchEvent(evento);
        window.conviteFoiSegurado = evento.defaultPrevented;
      }, desfecho);
      const antes = await secao(android.page);
      assert.equal(antes.botaoEscondido, false, "com o convite do navegador, o botão de instalar não apareceu");
      assert.ok(await android.page.evaluate(() => window.conviteFoiSegurado), "a página deixou a caixa do navegador abrir sozinha");
      await android.page.evaluate(() => document.querySelector("#cel-painel-instalar button").click());
      await android.page.waitForFunction(() => document.querySelector('#cel-painel-instalar [role="status"]').textContent !== "");
      assert.equal(await android.page.evaluate(() => window.abriuOConvite), 1, "o botão não abriu o convite do navegador");
      return secao(android.page);
    };
    vista = await comConvite("dismissed");
    assert.match(vista.dito, /Nada foi instalado/, `desistir não ficou escrito: ${vista.dito}`);
    vista = await comConvite("accepted");
    assert.match(vista.dito, /está na tela inicial/, `instalar não ficou escrito: ${vista.dito}`);
    assert.deepEqual(android.erros, [], `a página lançou: ${android.erros.join(" | ")}`);
    await android.contexto.close();

    // ---- 2. no iOS: o manifesto da aba, e o caminho do Safari
    const ios = await abrir(browser, base, { userAgent: IPHONE });
    const noIos = await ios.page.evaluate(() => document.querySelector('link[rel="manifest"]')?.href ?? null);
    assert.ok(noIos?.endsWith("/seele-na-aba.webmanifest"), `no iOS a página não trocou de manifesto: ${noIos}`);
    const daAba = await (await fetch(noIos)).json();
    assert.equal(daAba.display, "browser", "no iOS o ícone tem de abrir a aba do Safari");
    assert.deepEqual({ ...daAba, display: "standalone" }, manifesto, "os dois manifestos só podem diferir na janela");
    vista = await secao(ios.page);
    assert.match(vista.dito, /Adicionar à Tela de Início/, `no iOS a seção não diz o caminho do Safari: ${vista.dito}`);
    assert.equal(vista.botaoEscondido, true);
    assert.deepEqual(ios.erros, [], `a página lançou no iOS: ${ios.erros.join(" | ")}`);
    await ios.contexto.close();

    console.log("instalavel: manifesto, ícones, a aba no iOS e a seção dos ajustes do celular: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
