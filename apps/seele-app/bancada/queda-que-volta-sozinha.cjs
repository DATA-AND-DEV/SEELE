// **«Você ficou para trás; reconectando…»** — a queda recuperável dita na
// casca, sem tela de fim (o `CaiuComMotivo` do ffi do B, carimbado pela sessão
// e pela geração que caiu; na web, o mesmo evento emitido pela ponte). Bancada
// SINTÉTICA: o HTML e os scripts reais; no desktop a ponte é um duble, na web a
// `ponte-web.js` é a real com a sessão do navegador de mentira. Não é
// reprodução nativa.
//
// # Desktop
// 1. Com a sessão 1 na vista, o `CaiuComMotivo {FellBehind}` da sessão 1 mostra
//    a linha discreta, com a frase do «ficou para trás», e nenhuma tela de fim.
// 2. O retrato da MESMA geração (a bateria) não a apaga; o da geração nova,
//    Online, apaga.
// 3. O `CaiuComMotivo` de OUTRA sessão não aparece na vista.
// 4. Sair da sessão apaga a linha.
// # Web
// 5. A ponte real: o `Disconnecting {FellBehind}` dobrado pela sessão do
//    navegador vira `CaiuComMotivo` carimbado, e a linha aparece; uma vez por
//    geração do enlace.
//
// Rodar:  node apps/seele-app/bancada/queda-que-volta-sozinha.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    // ------------------------------------------------------------- desktop
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.ouvintes = new Map();
      window.__TAURI__ = {
        core: { invoke: async (cmd) => structuredClone(tabela[cmd] ?? null) },
        event: { listen: async (nome, fn) => { if (!ouvintes.has(nome)) ouvintes.set(nome, []); ouvintes.get(nome).push(fn); return () => {}; } },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
      window.emitirDaSessao = (sessao, geracao, evento) => {
        for (const fn of ouvintes.get("seele://event") ?? []) fn({ event: "seele://event", id: 0, payload: { sessao, geracao, vista: true, evento } });
      };
    }, { ...respostas({ microfones: [], saidas: [], meu_servidor: null, sessoes: null }), snapshot: retrato() });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);
    const linha = () => page.evaluate(() => ({
      visivel: !$("sessao-ficou-para-tras").hidden, texto: $("sessao-ficou-para-tras").textContent, fim: !$("tela-fim").hidden,
    }));

    await page.evaluate((r) => {
      porNaVista({ sessao: 1, geracao: 1 });
      sessoesConhecidas.add(2);
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      desenhar({ ...r, carimbo: { sessao: 1, geracao: 1 } });
    }, retrato());
    assert.equal((await linha()).visivel, false, "a linha aparece sem queda nenhuma");

    // 1.
    await page.evaluate(() => emitirDaSessao(1, 1, { CaiuComMotivo: { reason: "FellBehind" } }));
    let l = await linha();
    assert.equal(l.visivel, true, "o CaiuComMotivo da sessão na vista não mostrou a linha");
    assert.match(l.texto, /Você ficou para trás do servidor.*Reconectando/, `a frase: ${l.texto}`);
    assert.equal(l.fim, false, "a queda que volta sozinha abriu a tela de fim");

    // 2. Um retrato atrasado da MESMA geração, ainda Online, não apaga; a bateria
    //    (mesma geração) também não; a geração nova Online apaga.
    await page.evaluate((r) => desenhar({ ...r, carimbo: { sessao: 1, geracao: 1 }, link: "Online" }), retrato());
    assert.equal((await linha()).visivel, true, "um retrato atrasado da geração que caiu apagou a linha");
    await page.evaluate((r) => desenhar({ ...r, carimbo: { sessao: 1, geracao: 1 }, link: { InternalBattery: { remaining_seconds: 200, attempts: 0 } } }), retrato());
    assert.equal((await linha()).visivel, true, "a linha sumiu antes de a conexão voltar");
    await page.evaluate((r) => { porNaVista({ sessao: 1, geracao: 2 }); desenhar({ ...r, carimbo: { sessao: 1, geracao: 2 }, link: "Online" }); }, retrato());
    assert.equal((await linha()).visivel, false, "a geração nova no ar não apagou a linha");

    // 3. A queda de outra sessão não aparece na vista.
    await page.evaluate(() => emitirDaSessao(2, 1, { CaiuComMotivo: { reason: "FellBehind" } }));
    assert.equal((await linha()).visivel, false, "a queda de outra sessão apareceu na vista");

    // 4. Sair apaga.
    await page.evaluate(() => emitirDaSessao(1, 2, { CaiuComMotivo: { reason: "FellBehind" } }));
    assert.equal((await linha()).visivel, true);
    await page.evaluate(() => { vistaDaJanela.sessao = null; esquecerOEstadoDaSessao(); });
    assert.equal((await linha()).visivel, false, "sair da sessão deixou a linha à vista");
    assert.deepEqual(erros, [], `erros na página (desktop): ${erros.join(" | ")}`);
    await page.close();

    // ----------------------------------------------------------------- web
    const web = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const errosWeb = [];
    web.on("pageerror", (e) => errosWeb.push(e.message));
    await web.addInitScript(() => { try { localStorage.setItem("seele.casca", "larga"); } catch {} });
    await web.goto(`http://127.0.0.1:${server.address().port}/`);
    await web.waitForFunction(() => typeof PONTE_NO_LUGAR_DO_TAURI !== "undefined" && PONTE_NO_LUGAR_DO_TAURI);
    await web.evaluate(async (r) => {
      // A sessão do navegador de mentira, no lugar da do `sessao-web.js`.
      instanciarOWasmDaSessao = async () => ({});
      criarATransmissaoDoNavegador = () => ({ aoDobrar() {}, fechar() {} });
      let voltas = 0;
      abrirASessaoDoNavegador = async () => {
        const s = {
          acabou: false, voltando: false,
          get geracaoDoEnlace() { return voltas + 1; },
          voltar() { voltas += 1; },
          retrato: () => ({ sala: { ...r, voice_rooms: [], transmissoes: [], tela: null }, contados: {}, telemetria: { rtt_ms: 1, jitter_ms: 0, loss_fraction: 0, signal: 90, sync_band: "Nominal" } }),
          enlace: () => "Online",
          atender: (comando) => ({ resposta: comando === "sala_da_voz" ? { sala: null } : null }),
          encerrar: async () => {},
        };
        window.sessaoFalsa = s;
        return s;
      };
      const entrada = await invoke("connect", { alvo: { chave: {}, impressao: "T", endereco: "t.exemplo:8383", conhecido: true }, nickname: "Alex" });
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      depoisDeConectar(entrada, "t.exemplo:8383");
    }, retrato());
    assert.equal(await web.evaluate(() => vistaDaJanela.sessao), 1, "a sessão do navegador não entrou na vista");

    // 5. A despedida dobrada vira o evento carimbado, e a linha aparece; uma vez por geração.
    await web.evaluate(() => {
      window.emitidos = [];
      const original = emitirNaPonte;
      emitirNaPonte = (nome, carga) => { if (carga?.evento?.CaiuComMotivo) emitidos.push(carga); return original(nome, carga); };
      sessaoFalsa.aoDobrar({ Disconnecting: { reason: "FellBehind" } });
      sessaoFalsa.aoDobrar({ Disconnecting: { reason: "FellBehind" } });
    });
    await web.waitForTimeout(50);
    const naWeb = await web.evaluate(() => ({
      emitidos, visivel: !$("sessao-ficou-para-tras").hidden, texto: $("sessao-ficou-para-tras").textContent,
    }));
    assert.deepEqual(naWeb.emitidos.map((e) => [e.sessao, e.geracao, e.evento.CaiuComMotivo.reason]), [[1, 1, "FellBehind"]],
      `a ponte não emitiu UM CaiuComMotivo carimbado: ${JSON.stringify(naWeb.emitidos)}`);
    assert.equal(naWeb.visivel, true, "na web, a linha da queda não apareceu");
    assert.match(naWeb.texto, /ficou para trás/);
    // Uma despedida que acaba (expulsão) não é queda que volta.
    await web.evaluate(() => { sessaoFalsa.voltar(); emitidos.length = 0; sessaoFalsa.aoDobrar({ Disconnecting: { reason: "Kicked" } }); });
    assert.deepEqual(await web.evaluate(() => emitidos.length), 0, "a expulsão virou queda que volta sozinha");
    assert.deepEqual(errosWeb.filter((e) => !/seele_web\.wasm|404/.test(e)), [], `erros na página (web): ${errosWeb.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("queda que volta sozinha: a linha discreta aparece com o CaiuComMotivo da sessão na vista, sem tela de fim, some na geração nova, não aparece pela de outra sessão, e na web a ponte emite o mesmo evento carimbado.");
})().catch((e) => { console.error(e); process.exit(1); });
