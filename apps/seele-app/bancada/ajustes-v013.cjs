// Navegador com HTML/CSS reais; a ponte nativa é simulada.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium: abrirChromium } = require('./playwright.cjs');
const chromium = abrirChromium();
(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(table => {
      window.testTable = table; window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => { window.testCalls.push({cmd,args}); return window.testTable[cmd] ?? null; } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ microfones: [], saidas: [], snapshot: null, fontes_de_tela: [{ id: 1, nome: 'Monitor', monitor: true, largura: 1920, altura: 1080 }], permissao_de_tela: 'Concedida', pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(() => abrirServer('tela-boot'));
    assert.equal(await page.locator('#secao-mods').isVisible(), false);
    await page.evaluate(r => { window.testTable.snapshot = r; fecharServer(); $('tela-boot').hidden = true; $('tela-sessao').hidden = false; desenhar(r); }, retrato());
    await page.evaluate(() => abrirServer('tela-sessao'));
    assert.equal(await page.locator('#secao-mods').isVisible(), true);
    await page.evaluate(() => abrirSecao('secao-mods'));
    await page.screenshot({ path: '/tmp/seele-mods-v013.png' });
    await page.evaluate(() => { fecharServer(); window.testTable.snapshot = null; return abrirServer('tela-boot'); });
    assert.equal(await page.locator('#painel-mods').isVisible(), false, 'o painel lembrado vazou na tela inicial');
    await page.evaluate(r => { fecharServer(); window.testTable.snapshot = r; }, retrato());
    await page.evaluate(() => abrirCompartilhar());
    // **A caixa nasce em 720p.** Com o piso, o padrão deixou de ser preferência
    // e virou suposição sobre a casa de quem hospeda: a 1080p ele suporia
    // 10,4 Mbps de todo mundo que nunca abrisse esta caixa.
    assert.equal(
      await page.evaluate(() => document.getElementById('compartilhar-resolucao').value),
      '720',
      'a caixa deixou de nascer em 720p, e o padrão voltou a supor 10,4 Mbps de quem nunca a abriu',
    );
    // Os quadros são escolha, e não mais o 60 escrito na função.
    await page.selectOption('#compartilhar-quadros', '30');
    assert.equal(
      await page.evaluate(() => limitesEscolhidos().quadros_maximos), 30,
      'o seletor de quadros virou enfeite: a pessoa mexe e a ponte manda outra coisa',
    );
    await page.selectOption('#compartilhar-quadros', '60');
    assert.equal(await page.evaluate(() => limitesEscolhidos().quadros_maximos), 60);
    await page.selectOption('#compartilhar-resolucao', '720');
    assert.equal(await page.evaluate(() => limitesEscolhidos().altura_maxima), 720);
    await page.screenshot({ path: '/tmp/seele-compartilhar-v013.png' });
    await page.evaluate(() => { window.testTable.snapshot.tela = { e_minha: true }; });
    await page.selectOption('#compartilhar-resolucao', '1080');
    await page.waitForFunction(() => testCalls.some(c => c.cmd === 'ajustar_limites_da_tela' && c.args.limites.altura_maxima === 1080));
    // O mesmo para os quadros: a troca atravessa com a transmissão no ar.
    await page.selectOption('#compartilhar-quadros', '30');
    await page.waitForFunction(() => testCalls.some(c => c.cmd === 'ajustar_limites_da_tela' && c.args.limites.quadros_maximos === 30));
    await page.evaluate(() => {
      fecharCompartilhar();
      const dono = { falar() {}, instancia: { registrar: () => ({ esquecer() {} }) } };
      const mod = { id: 'teste/estilo' };
      contribuicoesDosMods.registrar(mod, dono.instancia, { ponto: 'servidor.navegacao', rotulo: 'Tema oculto', listarNaBarra: false });
      contribuicoesDosMods.registrar({ id: 'teste/mesa' }, dono.instancia, { ponto: 'servidor.navegacao', rotulo: 'Mesa' });
      redesenharAsEntradasDeMod();
      window.testSurfaces = new SuperficiesDoMod('teste/mesa', dono, {
        paginas: $('palco-de-paginas'), paineis: $('palco-de-paineis'), camadas: $('palco-de-camadas'), avisos: $('avisos-de-mod'),
      });
      testSurfaces.criar({ id: 'mesa', tipo: 'pagina', titulo: 'Mesa', imersiva: true });
      testSurfaces.de('mesa').montar([{ forma: 'titulo', dentro: 'CAMPANHA' }, { forma: 'texto', dentro: 'Tabuleiro da sessão' }]);
    });
    assert.equal(await page.locator('#lista-mods-navegacao li').count(), 1);
    const box = await page.locator('#palco-de-paginas').boundingBox();
    assert.equal(box.width, 1280); assert.equal(box.x, 0); assert.equal(box.y, 32);
    await page.screenshot({ path: '/tmp/seele-mesa-v013.png' });
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('#palco-de-paginas').isVisible(), false);
    await page.evaluate(() => {
      for (let i = 0; i < 20; i++) testSurfaces.criar({ id: 'aviso-' + i, tipo: 'aviso', titulo: 'Imagem salva' });
    });
    assert.equal(await page.locator('.aviso-de-mod').count(), 4);
    assert.equal(await page.locator('.aviso-de-mod').first().innerText(), 'Imagem salva');
    await page.waitForTimeout(3700);
    assert.equal(await page.locator('.aviso-de-mod').count(), 0);
    assert.equal(await page.evaluate(() => testSurfaces.porChave.size), 1, 'avisos expirados retidos');
    assert.deepEqual(errors, []);
    console.log('v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
