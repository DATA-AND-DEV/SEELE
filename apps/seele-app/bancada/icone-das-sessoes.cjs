// UI de produto, ponte fixture: ícone na trilha E3 e isolamento das respostas.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas, retrato } = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try { for (const engine of ['chromium', 'webkit']) {
    const browser = await engines[engine]().launch({ headless: true });
    try {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
      await page.addInitScript(table => {
        window.table = table; window.calls = []; window.delayIcon = false;
        window.__TAURI__ = { core: { invoke: async (cmd, args) => {
          calls.push({ cmd, args });
          if (cmd === 'snapshot' && window.delaySnapshot) { delaySnapshot = false; return new Promise(ok => window.finishSnapshot = ok); }
          if (cmd === 'icone_do_server' && delayIcon) { delayIcon = false; return new Promise(ok => window.finishIcon = ok); }
          return table[cmd] ?? null;
        } }, event: { listen: async () => () => {} }, window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } };
      }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [], amigos: [], contatos_da_agenda: [] }));
      if (process.argv.includes('--mutante')) await page.route('**/camada-sessoes.js', route => route.fulfill({ contentType: 'text/javascript', body: fs.readFileSync(path.join(__dirname, '../ui/camada-sessoes.js'), 'utf8').replace('rotulo, iconeDaSessao(s.carimbo)', 'rotulo, null') }));
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.evaluate(async r => {
        const canvas = document.createElement('canvas'); canvas.width = canvas.height = 56;
        const ctx = canvas.getContext('2d');
        window.icons = ['#398cd7', '#ef6044'].map(color => { ctx.fillStyle = color; ctx.fillRect(0, 0, 56, 56); ctx.fillStyle = 'white'; ctx.font = 'bold 32px sans-serif'; ctx.fillText('S', 17, 40); return Array.from(Uint8Array.from(atob(canvas.toDataURL().split(',')[1]), c => c.charCodeAt(0))); });
        window.r = r; table.snapshot = r; table.icone_do_server = icons[0];
        table.sessoes = [1, 2].map(id => ({ carimbo: { sessao: id, geracao: 1 }, nome: `Servidor ${id}`, endereco: `fixture-${id}`, na_vista: id === 1, enlace: 'online' }));
        porNaVista(table.sessoes[0].carimbo); await recarregarSessoes();
        $('tela-boot').hidden = true; $('tela-sessao').hidden = false;
        desenhar(r); await sincronizarIcone(r);
      }, retrato());
      const icon = page.locator('#trilha-sessoes [data-sessao="1"] img');
      await icon.waitFor({ timeout: 3000 });
      assert.ok(await icon.isVisible());
      const first = await icon.getAttribute('src');
      await page.evaluate(async () => { table.icone_do_server = icons[1]; r.icon_revision++; await sincronizarIcone(r); });
      assert.notEqual(await icon.getAttribute('src'), first);
      if (process.env.SEELE_PREVIEW_DIR) { fs.mkdirSync(process.env.SEELE_PREVIEW_DIR, { recursive: true }); await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `${engine}-icone-servidor.png`) }); }
      const count = await page.evaluate(() => calls.filter(c => c.cmd === 'icone_do_server').length);
      await page.evaluate(async () => { for (let i = 0; i < 10; i++) await sincronizarIcone(r); });
      assert.equal(await page.evaluate(() => calls.filter(c => c.cmd === 'icone_do_server').length), count);
      // Mesmo número de revisão, outra sessão: nunca herda a imagem de A.
      await page.evaluate(async () => { porNaVista(table.sessoes[1].carimbo); table.icone_do_server = null; await sincronizarIcone(r); });
      assert.equal(await page.locator('#trilha-sessoes [data-sessao="2"] img').count(), 0);
      assert.equal(await icon.count(), 1);
      assert.equal(await page.locator('#server-icone-previa').getAttribute('src'), null);
      await page.evaluate(async () => { porNaVista(table.sessoes[0].carimbo); await sincronizarIcone(r); });
      assert.equal(await page.locator('#server-icone-previa').getAttribute('src'), await icon.getAttribute('src'));
      // Snapshot antigo recusado pelo invoke não pode limpar a prévia atual.
      await page.evaluate(async () => {
        delaySnapshot = true; window.pendingSnapshot = seguirOServidor(); await Promise.resolve();
        porNaVista(table.sessoes[1].carimbo); table.icone_do_server = icons[0];
        await sincronizarIcone({icon_revision: 4});
        window.beforeLateSnapshot = $('server-icone-previa').src;
        finishSnapshot(r); await pendingSnapshot;
      });
      assert.equal(await page.locator('#server-icone-previa').getAttribute('src'), await page.evaluate(() => beforeLateSnapshot));

      // Reconexão da mesma sessão reinicia revisão e exige outra leitura.
      await page.evaluate(async () => { table.sessoes[0].carimbo.geracao = 2; porNaVista(table.sessoes[0].carimbo); table.icone_do_server = icons[0]; r.icon_revision = 0; await recarregarSessoes(); await sincronizarIcone(r); });
      assert.equal(await icon.getAttribute('src'), first);
      // A resposta pendente de A não apaga nem substitui a imagem nova de B.
      await page.evaluate(async () => { delayIcon = true; r.icon_revision++; window.pendingIcon = sincronizarIcone(r); await Promise.resolve(); porNaVista(table.sessoes[1].carimbo); table.icone_do_server = icons[1]; await sincronizarIcone({ icon_revision: 2 }); finishIcon(icons[0]); await pendingIcon; });
      assert.notEqual(await page.locator('#trilha-sessoes [data-sessao="2"] img').getAttribute('src'), first);
      await page.evaluate(async () => { table.icone_do_server = null; await sincronizarIcone({ icon_revision: 3 }); });
      assert.equal(await page.locator('#trilha-sessoes [data-sessao="2"] img').count(), 0);
      console.log(`${engine}: ícone E3, troca, revisão/cache, reconexão, remoção e resposta atrasada PASS`);
    } finally { await browser.close(); }
  } } finally { server.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
