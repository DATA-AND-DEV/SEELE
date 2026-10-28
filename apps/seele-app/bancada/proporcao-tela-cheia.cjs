// UI do produto e Fullscreen API real. Captura/IPC ficam simulados;
// o teste H264 de compartilhar-pessoal-web cobre o transporte separadamente.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
  const server = servir(); await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try {
    for (const engine of ['chromium', 'webkit']) {
      const browser = await engines[engine]().launch({ headless: true });
      try {
        const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
        const errors = []; page.on('pageerror', e => errors.push(e.message));
        await page.addInitScript(table => {
          window.testTable = table;
          window.__TAURI__ = {
            core: { invoke: async (cmd, args) => cmd === 'tela_cheia' ? trocarTelaCheiaDoNavegador(args.ligada) : testTable[cmd] ?? null },
            event: { listen: async () => () => {} },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.evaluate(async r => {
          testTable.snapshot = r; $('tela-boot').hidden = true; $('tela-sessao').hidden = false;
          desenhar(r); await abrirChamada();
          const c = $('palco-imagem'); c.hidden = false; c.width = 360; c.height = 640;
          c.getContext('2d').fillRect(0, 0, 360, 640); botaoDeCinema(true);
        }, retrato());
        const bounds = selector => page.locator(selector).evaluate(e => {
          const r = e.getBoundingClientRect(), s = getComputedStyle(e);
          return { x: r.x, y: r.y, w: r.width, h: r.height, vw: innerWidth, vh: innerHeight, fit: s.objectFit };
        });
        const fills = b => {
          assert.ok(Math.abs(b.x) < 1 && Math.abs(b.y) < 1 && Math.abs(b.w - b.vw) < 2 && Math.abs(b.h - b.vh) < 2, JSON.stringify(b));
          assert.equal(b.fit, 'contain');
        };
        await page.locator('#palco-cheia').click();
        await page.waitForFunction(() => document.body.dataset.cinemaModo);
        if (engine === 'chromium') assert(await page.evaluate(() => !!document.fullscreenElement), 'API real entrou');
        fills(await bounds('#palco-imagem'));
        await page.locator('#palco-sair-desktop').click();
        await page.waitForFunction(() => !document.body.dataset.cinema);
        assert.equal(await page.locator('#palco-imagem').isVisible(), true);
        await page.locator('#palco-cheia').click();
        await page.waitForFunction(() => document.body.dataset.cinemaModo);
        await page.keyboard.press('Escape');
        await page.waitForFunction(() => !document.body.dataset.cinema);
        await page.evaluate(() => {
          window.holder = document.createElement('div'); document.body.append(holder);
          holder.style.position = 'fixed'; holder.style.inset = '0'; holder.style.zIndex = '100';
          window.estadoDaTela = null;
          window.janelaNativa = [];
          window.share = criarCompartilhamentoPessoalWeb({ servico: { wasm: {} }, transporte: { ouvir: () => () => {} }, palco: holder, enviarControle() {}, telaCheiaNativa: ligada => janelaNativa.push(ligada), aoEstado: tipo => { estadoDaTela = tipo; } });
          holder.hidden = false; holder.id = 'teste-pessoal';
          const c = holder.querySelector('canvas'); c.width = 360; c.height = 640;
        });
        await page.locator('#teste-pessoal > button').click();
        await page.waitForFunction(() => document.fullscreenElement === holder || document.webkitFullscreenElement === holder || estadoDaTela === 'erro' || holder.querySelector('video')?.readyState >= 1);
        const native = await page.evaluate(() => document.fullscreenElement === holder || document.webkitFullscreenElement === holder);
        if (engine === 'chromium') assert(native, 'pessoal entra por API real');
        if (native) {
          fills(await bounds('#teste-pessoal canvas'));
          const button = await bounds('#teste-pessoal > button');
          assert(button.x >= 0 && button.y >= 0 && button.x + button.w <= button.vw && button.y + button.h <= button.vh);
          await page.locator('#teste-pessoal > button').click();
          await page.waitForFunction(() => !document.fullscreenElement && !document.webkitFullscreenElement);
          await page.locator('#teste-pessoal > button').click();
          await page.waitForFunction(() => document.fullscreenElement || document.webkitFullscreenElement);
          await page.waitForFunction(() => janelaNativa.length === 3);
          await page.evaluate(() => document.exitFullscreen?.() ?? document.webkitExitFullscreen?.());
          await page.waitForFunction(() => document.querySelector('#teste-pessoal > button').textContent === 'TELA CHEIA');
          await page.waitForFunction(() => janelaNativa.length === 4);
          assert.deepEqual(await page.evaluate(() => janelaNativa), [true, false, true, false], 'desktop acompanha entrada e saída da Fullscreen API');
        }
        // Um evento de outro palco não deve roubar o foco do chat.
        assert(await page.evaluate(() => {
          const campo = document.createElement('input'); holder.append(campo); campo.focus();
          document.dispatchEvent(new Event('fullscreenchange'));
          const preservou = document.activeElement === campo; campo.remove(); return preservou;
        }));
        // Alternativa iPhone: apresentação controlada, stream de canvas real.
        await page.evaluate(() => {
          holder.requestFullscreen = undefined; holder.webkitRequestFullscreen = undefined;
          HTMLVideoElement.prototype.webkitEnterFullscreen = function () {
            this.dataset.noPlayer = 'sim'; this.dispatchEvent(new Event('webkitbeginfullscreen'));
          };
          HTMLVideoElement.prototype.webkitExitFullscreen = function () {
            delete this.dataset.noPlayer; this.dispatchEvent(new Event('webkitendfullscreen'));
          };
          Object.defineProperty(HTMLVideoElement.prototype, 'webkitDisplayingFullscreen', { configurable: true, get() { return this.dataset.noPlayer === 'sim'; } });
          const c = holder.querySelector('canvas'), ctx = c.getContext('2d');
          window.pintura = setInterval(() => ctx.fillRect(0, 0, c.width, c.height), 30);
        });
        await page.locator('#teste-pessoal > button').click();
        await page.waitForFunction(() => document.querySelector('#teste-pessoal video')?.readyState >= 1);
        await page.locator('#teste-pessoal > button').click();
        assert(await page.evaluate(() => {
          const v = holder.querySelector('video'); window.faixasCinema = v.srcObject.getTracks();
          return v.webkitDisplayingFullscreen && v.style.opacity === '1' && v.style.objectFit === 'contain' && v.style.width === '100%';
        }), 'player nativo não herda transparência/1px do preparo');
        await page.evaluate(() => { holder.querySelector('video').webkitExitFullscreen(); clearInterval(pintura); });
        assert(await page.evaluate(() => !holder.querySelector('video') && faixasCinema.every(t => t.readyState === 'ended')));
        await page.evaluate(() => { share.fechar(); holder.remove(); });
        assert.deepEqual(errors, []);
        console.log(`PASS ${engine}: fullscreen pessoal real=${native}; proporção contain, palco ocupa viewport, saída visível, foco e alternativa iPhone controlada`);
      } finally { await browser.close(); }
    }
  } finally { server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
