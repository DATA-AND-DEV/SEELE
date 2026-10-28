// Página, analisador WASM e identidade reais; transporte controlado.
// Navega fragmentos no documento vivo. Não mede a rede do servidor.
// node link-na-aba-aberta.cjs [--webkit] [--playwright=...] [--reverter]
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const pw = require('./playwright.cjs');
const {origemDoProduto} = require('./origem-do-produto.cjs');
(async () => {
  const browser = await (process.argv.includes('--webkit') ? pw.webkit() : pw.chromium()).launch();
  try {
    for (const celular of [true, false]) {
      const page = await browser.newPage({ viewport: celular ? { width: 440, height: 956 } : { width: 1440, height: 900 }, isMobile: celular, hasTouch: celular });
      page.setDefaultTimeout(5000);
      page.on('pageerror', e => console.error('pagina:', e.message));
      await page.route(`${origemDoProduto}/**`, route => {
        const nome = new URL(route.request().url()).pathname.slice(1) || 'index.html';
        const arquivo = nome === 'seele_web.wasm' ? path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/seele_web.wasm') : path.resolve(__dirname, '../ui', nome);
        if (!fs.existsSync(arquivo)) return route.fulfill({ status: 404, body: '' });
        let body = fs.readFileSync(arquivo);
        if (process.argv.includes('--reverter') && nome === 'identidade-web.js') body = body.toString().replace('window.addEventListener("hashchange", async () => {', 'window.addEventListener("hashchange-desativado", async () => {');
        const tipos = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };
        return route.fulfill({ contentType: tipos[path.extname(nome)] || 'application/octet-stream', body });
      });
      await page.addInitScript(celular => {
        localStorage.setItem('seele.casca', celular ? 'celular' : 'larga');
        window.documentoDaBancada = 'mesmo documento';
      }, celular);
      const link = n => `${origemDoProduto}/#192.0.2.${n}:8383?fp=${String(n).repeat(64)}&v=2.0&convite=${'ab'.repeat(32)}`;
      await page.goto(link(1));
      await page.waitForFunction(() => typeof identidadeDaPagina === 'function');
      await page.evaluate(async () => { await identidadeDaPagina(); window.entradas = []; const original = invocarNaPonte; invocarNaPonte = async (comando, args) => { if (comando !== 'connect') return original(comando, args); entradas.push({ endereco: args.alvo.endereco, apelido: args.nickname, token: args.joinSecret }); throw { Refused: { reason: 'AdmissionPending' } }; }; });
      const consentimento = async n => {
        if (celular) await page.locator('#cel-entrada-titulo').filter({ hasText: `192.0.2.${n}:8383` }).waitFor();
        else {
          await page.locator('#servidores').waitFor({ state: 'visible' });
          await page.waitForFunction(n => document.getElementById('servidores-endereco').value === `192.0.2.${n}:8383`, n);
        }
        assert.equal(await page.evaluate(() => location.hash), '');
        assert.equal(await page.evaluate(() => entradas.length), 0);
        assert.equal(await page.evaluate(() => document.body.textContent.includes('ab'.repeat(32))), false);
      };
      await consentimento(1);
      await page.evaluate(() => { window.marcadorDoDocumento = 17; });
      await page.goto(link(2));
      await consentimento(2);
      assert.equal(await page.evaluate(() => marcadorDoDocumento), 17, 'recarregou o documento');
      // Outra sessão visível: receber o convite só abre a UI, sem mudar geração.
      await page.evaluate(() => {
        document.getElementById('tela-boot').hidden = true;
        document.getElementById('tela-sessao').hidden = false;
        window.geracaoAntes = epocaDaVista;
        window.vistaAntes = vistaDaJanela.sessao;
        linkColadoDaPonte = { texto: 'convite antigo', alvo: '192.0.2.3:8383', impressao: '3'.repeat(64) };
      });
      await page.goto(link(3));
      await consentimento(3);
      assert.equal(await page.evaluate(() => linkColadoDaPonte), null);
      assert.equal(await page.evaluate(() => epocaDaVista === geracaoAntes && vistaDaJanela.sessao === vistaAntes), true);
      if (celular) {
        await page.getByRole('button', { name: 'AGORA NÃO', exact: true }).click();
        assert.equal(await page.locator('#tela-sessao').isVisible(), true);
        await page.goto(link(3));
        await consentimento(3);
        await page.locator('#cel-entrada-apelido').fill('bancada link');
        await page.getByRole('button', { name: 'ENTRAR', exact: true }).click();
        await page.getByText('QUEM HOSPEDA AINDA NÃO DECIDIU', { exact: true }).waitFor();
        assert.equal(await page.evaluate(() => entradas[0].endereco), '192.0.2.3:8383');
        assert.equal(await page.evaluate(() => entradas[0].token), 'AB'.repeat(32));
        await page.evaluate(() => { entradas.length = 0; });
        await page.goto(link(3));
        await page.waitForFunction(async () => (await identidadeDaPagina()).ler()?.estado === 'ja-conhecido');
        await consentimento(3);
      } else {
        await page.evaluate(() => {
          document.getElementById('tela-sessao').hidden = true;
          document.getElementById('tela-boot').hidden = false;
          localStorage.setItem('seele.apelido', 'bancada desktop');
        });
        await page.locator('#servidores-forma button[type=submit]').click();
        await page.waitForFunction(() => entradas.length > 0);
        assert.equal(await page.evaluate(() => entradas[0].endereco), '192.0.2.3:8383');
        assert.equal(await page.evaluate(() => entradas[0].token), 'AB'.repeat(32));
      }
      console.log(`OK: ${celular ? 'celular' : 'desktop'} carga e hashchange oferecem consentimento, limpam fragmento e preservam vista/geração`);
      await page.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
