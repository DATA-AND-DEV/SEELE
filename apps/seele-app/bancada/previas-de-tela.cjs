// **As prévias na caixa de compartilhar.**
//
// A UI é a real; a ponte é um dublê do aplicativo. O que ela prende:
//
//  1. Abrir a caixa pede as prévias uma vez por lista (`previas_de_tela`), e a
//     lista aparece sem esperar por elas.
//  2. Uma prévia que chega (`seele://previa-de-tela`) vai para o cartão da
//     fonte dela, e só dela: o quadro do cartão passa a mostrar a imagem.
//  3. Uma prévia com o `id` de um cartão e o **nome de outro** não é posta: o
//     `id` é a posição na lista, e uma relista o troca de dono.
//  4. O que não é uma imagem em `data:` não entra no estilo.
//  5. Redesenhar a lista mantém as prévias que já chegaram; fechar a caixa as
//     esquece, e a próxima abertura pede de novo.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/previas-de-tela.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

// Um PNG de um ponto, vermelho.
const IMAGEM =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

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
        window.ouvintes = new Map();
        window.__TAURI__ = {
          core: {
            invoke: async (cmd, args) => {
              window.testCalls.push({ cmd, args });
              return Object.hasOwn(table, cmd) ? table[cmd] : null;
            },
          },
          event: {
            listen: async (nome, ouvinte) => {
              if (!window.ouvintes.has(nome)) window.ouvintes.set(nome, []);
              window.ouvintes.get(nome).push(ouvinte);
              return () => {};
            },
          },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
        window.emitir = (nome, payload) => {
          for (const ouvinte of window.ouvintes.get(nome) ?? []) ouvinte({ payload });
        };
      },
      respostas({
        snapshot: null,
        microfones: [],
        saidas: [],
        pacotes_no_cache: [],
        aceites_de_mods: [],
        exclusao_do_som_da_captura: "Excluido",
        som_da_tela: { volume: 1, calada: false },
        limite_da_mensagem: 4096,
        previas_consentidas: [],
        permissao_de_tela: "Concedida",
        previas_de_tela: null,
        fontes_de_tela: [
          { id: 0, nome: "Monitor 1", monitor: true, largura: 1920, altura: 1080 },
          { id: 1, nome: "Editor", monitor: false, largura: 1200, altura: 800 },
          { id: 2, nome: "Navegador", monitor: false, largura: 1300, altura: 900 },
        ],
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());

    const cartoes = () =>
      page.evaluate(() =>
        [...document.querySelectorAll("#compartilhar-fontes .compartilhar-fonte")].map((botao) => ({
          nome: botao.dataset.nomeDaFonte,
          comPrevia: botao.dataset.comPrevia === "sim",
          fundo: getComputedStyle(botao, "::before").backgroundImage.slice(0, 20),
        })),
      );
    const pedidos = () => page.evaluate(() => window.testCalls.filter((c) => c.cmd === "previas_de_tela").length);

    // 1. A lista aparece, e as prévias são pedidas uma vez.
    await page.evaluate(() => abrirCompartilhar());
    await page.waitForFunction(() => document.querySelectorAll("#compartilhar-fontes .compartilhar-fonte").length === 3);
    let vistos = await cartoes();
    assert.deepEqual(vistos.map((c) => c.nome).sort(), ["Editor", "Monitor 1", "Navegador"]);
    assert.ok(vistos.every((c) => !c.comPrevia && c.fundo === "none"), `um cartão nasceu com prévia: ${JSON.stringify(vistos)}`);
    assert.equal(await pedidos(), 1, "as prévias não foram pedidas uma vez ao abrir");

    // 2. A prévia que chega vai para o cartão da fonte dela, e só dela.
    await page.evaluate((url) => window.emitir("seele://previa-de-tela", { id: 1, nome: "Editor", url }), IMAGEM);
    vistos = await cartoes();
    const editor = vistos.find((c) => c.nome === "Editor");
    assert.ok(editor.comPrevia && editor.fundo.startsWith('url("data:image/'), `a prévia não foi para o cartão: ${JSON.stringify(vistos)}`);
    assert.ok(vistos.filter((c) => c.nome !== "Editor").every((c) => !c.comPrevia), `a prévia foi para outro cartão: ${JSON.stringify(vistos)}`);

    if (process.env.SEELE_PREVIEW_DIR) {
      await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "previas-de-tela.png") });
    }

    // 3. O `id` de um cartão com o nome de outro não é posto.
    await page.evaluate((url) => window.emitir("seele://previa-de-tela", { id: 2, nome: "Outra janela", url }), IMAGEM);
    // 4. O que não é imagem em `data:` não entra no estilo.
    await page.evaluate(() => window.emitir("seele://previa-de-tela", { id: 2, nome: "Navegador", url: "https://exemplo.org/x.png" }));
    await page.evaluate(() => window.emitir("seele://previa-de-tela", { id: 0, nome: "Monitor 1", url: "javascript:alert(1)" }));
    vistos = await cartoes();
    assert.ok(
      vistos.filter((c) => c.nome !== "Editor").every((c) => !c.comPrevia && c.fundo === "none"),
      `uma prévia de outra lista, ou que não é imagem, foi posta: ${JSON.stringify(vistos)}`,
    );

    // 5. Redesenhar mantém o que chegou, sem pedir de novo; fechar esquece.
    await page.evaluate(() => desenharCompartilhar());
    await page.waitForTimeout(150);
    vistos = await cartoes();
    assert.ok(vistos.find((c) => c.nome === "Editor").comPrevia, "redesenhar a lista perdeu a prévia");
    assert.equal(await pedidos(), 1, "a mesma lista pediu as prévias de novo");
    await page.evaluate(() => fecharCompartilhar());
    await page.evaluate(() => abrirCompartilhar());
    await page.waitForFunction(() => document.querySelectorAll("#compartilhar-fontes .compartilhar-fonte").length === 3);
    vistos = await cartoes();
    assert.ok(vistos.every((c) => !c.comPrevia), `a caixa reaberta guardou a prévia da anterior: ${JSON.stringify(vistos)}`);
    assert.equal(await pedidos(), 2, "a caixa reaberta não pediu as prévias de novo");

    assert.deepEqual(erros, []);
    console.log("previas-de-tela: pedidas uma vez por lista, postas só no cartão da fonte, recusadas fora de data:image, mantidas ao redesenhar e esquecidas ao fechar.");
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
