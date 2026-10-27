// **A imagem volta depois de um erro do decodificador no meio do fluxo**
// (item 1 da revisão de comunicação e telas; Codex). Bancada SINTÉTICA: a
// casca real do desktop, a ponte de duble, os eventos de tela entregues pelo
// duble do `listen`, um `VideoDecoder` de mentira e o relógio do palco na mão
// da bancada. Não é reprodução nativa, e é OUTRA falha que a do desaparecimento
// depois de reconectar: esta é a recuperação de uma imagem que já estava no ar.
//
// # O que ela mede
//
// 1. A transmissão abre (um pedido de chave, o da abertura), a chave chega, o
//    decodificador arma e desenha.
// 2. O decodificador erra no meio, com deltas chegando sem parar: sai UM
//    pedido de quadro-chave — e não um por delta.
// 3. A chave chega: um decodificador novo arma, recebe a chave e os deltas
//    seguintes — a imagem volta.
// 4. Sem chave nenhuma por 30 s de deltas, os pedidos seguem o limite (1 s,
//    2 s, 4 s, 8 s, 16 s…): poucos, e não uma tempestade.
//
// Rodar:  node apps/seele-app/bancada/recuperar-a-chave.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.ouvintes = {};
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          return structuredClone(tabela[cmd] ?? null);
        } },
        event: { listen: async (nome, f) => { (ouvintes[nome] ??= []).push(f); return () => {}; } },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
      window.emitir = (carga) => { for (const f of ouvintes["seele://event"] ?? []) f({ payload: carga }); };
      window.instancias = [];
      window.VideoDecoder = class {
        constructor(cb) { this.cb = cb; this.entregues = []; instancias.push(this); }
        configure(c) { this.config = c; }
        decode(chunk) { this.entregues.push(chunk.type); }
        close() { this.fechado = true; }
        static async isConfigSupported() { return { supported: true }; }
      };
      window.EncodedVideoChunk = class { constructor(o) { Object.assign(this, o); } };
    }, respostas({}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);

    await page.evaluate(() => {
      window.agora = 0;
      relogioDoPalco = () => agora;
      window.CHAVE = btoa(String.fromCharCode(0, 0, 0, 1, 0x67, 0x64, 0x00, 0x1f, 0xac, 0, 0, 0, 1, 0x65, 0x88));
      window.DELTA = btoa(String.fromCharCode(0, 0, 0, 1, 0x41, 0x9a, 0x02));
      window.pedidos = () => chamadas.filter((c) => c.cmd === "pedir_quadro_chave").length;
      entrarNaGeracao(1);
    });

    // 1. Abertura, chave, desenho.
    await page.evaluate(() => {
      emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      emitir({ ScreenFrame: { screen: 5, key: true, data: CHAVE } });
    });
    await page.waitForFunction(() => instancias.length === 1 && instancias[0].entregues.length === 1);
    await page.evaluate(() => { for (let i = 0; i < 3; i += 1) emitir({ ScreenFrame: { screen: 5, key: false, data: DELTA } }); });
    assert.deepEqual(await page.evaluate(() => instancias[0].entregues), ["key", "delta", "delta", "delta"]);
    assert.equal(await page.evaluate(() => pedidos()), 1, "a abertura não pediu exatamente uma chave");

    // 2. O erro no meio, e deltas sem parar: um pedido só.
    await page.evaluate(() => {
      agora = 5000;
      instancias[0].cb.error(new Error("decodificação falhou"));
      for (let i = 0; i < 30; i += 1) {
        agora += 20; // 30 deltas em 600 ms
        emitir({ ScreenFrame: { screen: 5, key: false, data: DELTA } });
      }
    });
    assert.equal(await page.evaluate(() => pedidos()), 2,
      "o erro do decodificador não pediu UM quadro-chave (ou pediu um por delta)");
    assert.deepEqual(await page.evaluate(() => chamadas.filter((c) => c.cmd === "pedir_quadro_chave").at(-1).args), { tela: 5 });

    // 3. A chave chega: a imagem volta.
    await page.evaluate(() => {
      emitir({ ScreenFrame: { screen: 5, key: true, data: CHAVE } });
    });
    await page.waitForFunction(() => instancias.length === 2 && instancias[1].entregues.length === 1);
    await page.evaluate(() => { emitir({ ScreenFrame: { screen: 5, key: false, data: DELTA } }); });
    assert.deepEqual(await page.evaluate(() => ({ entregues: instancias[1].entregues, esteEOdePe: decodificador === instancias[1] })),
      { entregues: ["key", "delta"], esteEOdePe: true }, "a chave pedida não trouxe a imagem de volta");

    // 4. Erro, e 30 s de deltas sem chave nenhuma: os pedidos seguem o limite.
    const antes = await page.evaluate(() => pedidos());
    await page.evaluate(() => {
      agora = 100000;
      instancias[1].cb.error(new Error("de novo"));
      for (let i = 0; i < 300; i += 1) {
        agora += 100; // 300 deltas em 30 s
        emitir({ ScreenFrame: { screen: 5, key: false, data: DELTA } });
      }
    });
    const novos = (await page.evaluate(() => pedidos())) - antes;
    // 0 s, ~1 s, ~3 s, ~7 s, ~15 s, ~31 s: no máximo 6 em 30 s, e mais de 1.
    assert.ok(novos >= 2 && novos <= 6, `pedidos de chave em 30 s sem resposta: ${novos} (o limite deixa entre 2 e 6)`);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
    console.log(`recuperar a chave: o erro pede UMA chave, a chave traz a imagem de volta, e sem resposta são ${novos} pedidos em 30 s.`);
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error(e); process.exit(1); });
