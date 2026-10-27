// UI real + canvas animado da fixture Xcode existente, ponte fullscreen real.
// --url=http://127.0.0.1:8733/cinema?fullscreen=nativo
const assert = require('node:assert/strict');
const pw = require('./playwright.cjs');
(async () => {
  const webkit = process.argv.includes('--webkit');
  const browser = await (webkit ? pw.webkit() : pw.chromium()).launch();
  try {
    const page = await browser.newPage({ viewport: { width: 440, height: 956 }, isMobile: true, hasTouch: true });
    const erros = []; page.on('pageerror', e => erros.push(e.message));
    await page.goto(process.argv.find(a => a.startsWith('--url='))?.slice(6) || 'http://127.0.0.1:8733/cinema?fullscreen=nativo');
    await page.waitForFunction(() => window.__FIXTURE?.pronto, { timeout: 30000 });
    const quadro = await page.evaluate(() => __FIXTURE.quadros);
    await page.locator('#palco-cheia').click();
    await page.waitForFunction(() => document.body.dataset.cinemaModo);
    const nativa = await page.evaluate(() => Boolean(document.fullscreenElement || document.webkitFullscreenElement));
    const modo = await page.evaluate(() => document.body.dataset.cinemaModo);
    assert.equal(modo === 'nativa', nativa, 'CSS não pode fingir fullscreen');
    if (!webkit) assert(nativa, 'Chromium deve entrar em fullscreen real pelo gesto');
    await page.waitForFunction(q => __FIXTURE.quadros > q + 3, quadro);
    // Saída pelo navegador também deve devolver o layout sem encerrar imagem.
    if (nativa) await page.evaluate(() => document.exitFullscreen?.() ?? document.webkitExitFullscreen?.());
    else await page.locator('#palco-sair-da-cheia').click();
    await page.waitForFunction(() => !document.body.dataset.cinema);
    assert.equal(await page.locator('#palco-imagem').isVisible(), true);
    assert.equal(await page.locator('#palco-cheia').innerText(), 'TELA CHEIA');
    console.log(`PASS API real: ${nativa ? 'fullscreen nativo' : 'fallback explícito'}, saída e quadros preservados`);

    // Sem API genérica: testa canvas.captureStream/vídeo reais. Somente as
    // APIs de apresentação são controladas para simular iPhone no desktop.
    await page.evaluate(() => {
      document.documentElement.requestFullscreen = undefined;
      document.documentElement.webkitRequestFullscreen = undefined;
      Object.defineProperty(HTMLVideoElement.prototype, 'webkitSupportsFullscreen', { configurable: true, get: () => true });
      Object.defineProperty(HTMLVideoElement.prototype, 'webkitDisplayingFullscreen', { configurable: true, get() { return this.dataset.nativa === 'sim'; } });
      HTMLVideoElement.prototype.webkitEnterFullscreen = function () { this.dataset.nativa = 'sim'; this.dispatchEvent(new Event('webkitbeginfullscreen')); };
      HTMLVideoElement.prototype.webkitExitFullscreen = function () { delete this.dataset.nativa; this.dispatchEvent(new Event('webkitendfullscreen')); };
    });
    await page.locator('#palco-cheia').click();
    await page.locator('#palco-abrir-nativa').waitFor();
    await page.evaluate(() => { window.videoDoTeste = videoDaTelaCheia; window.tracksDoTeste = videoDaTelaCheia.srcObject.getTracks(); });
    await page.waitForFunction(() => videoDoTeste.currentTime > 0);
    await page.locator('#palco-abrir-nativa').click();
    await page.waitForFunction(() => document.body.dataset.cinemaModo === 'nativa');
    await page.evaluate(() => videoDoTeste.webkitExitFullscreen());
    await page.waitForFunction(() => !document.body.dataset.cinema);
    assert(await page.evaluate(() => tracksDoTeste.every(t => t.readyState === 'ended') && videoDoTeste.srcObject === null && !videoDoTeste.isConnected));
    assert.equal(await page.locator('#palco-imagem').isVisible(), true);
    console.log('PASS apresentação iPhone controlada: stream de canvas real avança, segundo gesto entra, saída libera tracks/vídeo');

    await page.evaluate(() => { HTMLVideoElement.prototype.webkitEnterFullscreen = function () { throw Error('recusado'); }; });
    await page.locator('#palco-cheia').click();
    await page.locator('#palco-abrir-nativa').waitFor();
    await page.locator('#palco-abrir-nativa').click();
    await page.waitForFunction(() => document.body.dataset.cinemaModo === 'pagina');
    assert(await page.locator('#palco-dica-girar').innerText().then(t => t.includes('tela cheia indisponível')));
    assert(await page.evaluate(() => videoDaTelaCheia === null));
    await page.locator('#palco-sair-da-cheia').click();
    assert.deepEqual(erros, []);
    console.log('PASS API recusada: fallback identificado, auxiliar liberado e saída disponível');

    await page.evaluate(() => {
      document.documentElement.requestFullscreen = () => Promise.reject(Error('sem suporte genérico'));
      HTMLVideoElement.prototype.webkitEnterFullscreen = function () { this.dataset.nativa = 'sim'; this.dispatchEvent(new Event('webkitbeginfullscreen')); };
    });
    await page.locator('#palco-cheia').click();
    await page.locator('#palco-abrir-nativa').waitFor();
    await page.evaluate(() => { window.tracksDaTroca = videoDaTelaCheia.srcObject.getTracks(); botaoDeCinema(false); });
    await page.waitForFunction(() => !document.body.dataset.cinema);
    assert(await page.evaluate(() => videoDaTelaCheia === null && tracksDaTroca.every(t => t.readyState === 'ended')));
    assert.equal(await page.locator('#palco-abrir-nativa').count(), 0);
    console.log('PASS genérica recusada prepara alternativa; fim da transmissão cancela preparo e libera cópia');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
