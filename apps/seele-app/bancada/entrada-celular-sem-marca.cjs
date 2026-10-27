// UI móvel e identidade reais; só o transporte e o pedido decorativo são controlados.
// node entrada-celular-sem-marca.cjs [--webkit] [--teclado] [--playwright=...]
// --reverter recoloca o await da marca; --reverter-teclado deixa só a sessão
// acompanhar o viewport. Ambas as reversões devem reprovar. O teclado daqui
// controla visualViewport; não substitui a prova Safari com teclado do sistema.
const assert = require('node:assert/strict');
const { servir } = require('./telas.cjs');
const pw = require('./playwright.cjs');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const servidor = servir();
  await new Promise(ok => servidor.listen(0, '127.0.0.1', ok));
  const browser = await (process.argv.includes('--webkit') ? pw.webkit() : pw.chromium()).launch();
  try {
    const page = await browser.newPage({ viewport: { width: 440, height: 956 }, isMobile: true, hasTouch: true });
    if (process.argv.includes('--reverter')) {
      await page.route('**/celular-entrada.js', route => {
        const fonte = fs.readFileSync(path.join(__dirname, '../ui/celular-entrada.js'), 'utf8')
          .replace('const simbolo = h("img", { class: "cel-entrada-simbolo-parado", src: "marca-simbolo.svg", alt: "" });', 'const simbolo = await simboloDaMarca();');
        return route.fulfill({ contentType: 'text/javascript', body: fonte });
      });
    }
    if (process.argv.includes('--reverter-teclado')) {
      await page.route('**/celular-casca.js', route => {
        const fonte = fs.readFileSync(path.join(__dirname, '../ui/celular-casca.js'), 'utf8')
          .replace('[sessao, document.getElementById("tela-boot")]', '[sessao]');
        return route.fulfill({ contentType: 'text/javascript', body: fonte });
      });
    }
    await page.addInitScript(() => {
      localStorage.setItem('seele.casca', 'celular');
      const original = window.fetch;
      window.fetch = (...args) => String(args[0]).endsWith('marca-simbolo.svg') ? new Promise(() => {}) : original(...args);
    });
    await page.goto(`http://127.0.0.1:${servidor.address().port}`);
    await page.waitForFunction(() => typeof CELULAR !== 'undefined' && CELULAR.ajudante('entrada'));
    await page.evaluate(async () => {
      window.passos = [];
      Object.defineProperty(navigator, 'storage', { value: { persist: async () => { passos.push('persist'); return false; } }, configurable: true });
      const gerar = crypto.subtle.generateKey.bind(crypto.subtle);
      crypto.subtle.generateKey = (...args) => { passos.push('chave'); return gerar(...args); };
      const identidade = criarIdentidadeDoNavegador({
        analisar: () => ({ link: { alvo: '192.0.2.1:8383', impressao: 'ab'.repeat(32), versao: '2.0' } }),
        conectar: async () => { passos.push('transporte'); throw { Refused: { reason: 'AdmissionPending' } }; },
      });
      await CELULAR.ajudante('entrada').usar(identidade, { link: 'link da bancada' });
    });
    await page.locator('#cel-entrada-apelido').fill('bancada mobile');
    if (process.argv.includes('--teclado')) {
      const medir = async (altura, topo) => page.evaluate(({ altura, topo }) => {
        Object.defineProperty(visualViewport, 'height', { value: altura, configurable: true });
        Object.defineProperty(visualViewport, 'offsetTop', { value: topo, configurable: true });
        // Safari reduz innerHeight também; o monitor deve comparar clientHeight.
        Object.defineProperty(window, 'innerHeight', { value: altura, configurable: true });
        visualViewport.dispatchEvent(new Event('resize'));
        visualViewport.dispatchEvent(new Event('scroll'));
        const caixa = document.querySelector('.cel-entrada-pe').getBoundingClientRect();
        return { top: caixa.top, bottom: caixa.bottom, teclado: document.getElementById('tela-boot').hasAttribute('data-teclado') };
      }, { altura, topo });
      for (const topo of [0, 80]) {
        const caixa = await medir(460, topo);
        assert.equal(caixa.teclado, true, 'entrada não acompanhou o teclado');
        assert.ok(caixa.top >= topo && caixa.bottom <= 460 + topo + 1, `ações sob o teclado: ${JSON.stringify(caixa)}`);
      }
      const fechada = await medir(956, 0);
      assert.equal(fechada.teclado, false);
      await medir(460, 0);
      // O mesmo gesto acessível do teclado e um clique atrasado não duplicam entrada.
      await page.evaluate(() => {
        const campo = document.getElementById('cel-entrada-apelido');
        const botao = [...document.querySelectorAll('.cel-entrada-pe button')].find(b => b.textContent === 'ENTRAR');
        campo.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true }));
        campo.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', repeat: true, bubbles: true }));
        if (passos.length) throw new Error('composição ou repetição abriu tentativa');
        campo.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        botao.click();
        campo.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      });
    } else {
      await page.locator('.cel-entrada-pe button').filter({ hasText: /^ENTRAR$/ }).click();
    }
    await page.waitForFunction(() => passos.includes('transporte'), { }, { timeout: 3000 });
    await page.getByText('QUEM HOSPEDA AINDA NÃO DECIDIU', { exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => passos), ['persist', 'chave', 'transporte']);
    assert.equal(await page.locator('.cel-entrada-conectando').count(), 0);
    console.log('OK: ' + (process.argv.includes('--teclado') ? 'ações acima do teclado; Enter único; ' : '') + 'marca pendente não bloqueia persistência, chave, transporte e admissão na UI móvel');
  } finally { await browser.close(); servidor.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
