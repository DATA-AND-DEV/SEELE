// Rode cargo test -p xtask pacote_web::tests antes: ele gera as duas cópias
// com o versionador REAL do pacote. Sem page.route: o cache HTTP fica ativo.
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const pw = require('./playwright.cjs');
(async () => {
  const browser = await (process.argv.includes('--webkit') ? pw.webkit() : pw.chromium()).launch();
  try {
    for (const modo of ['sem-correcao', 'cache-antigo', 'cache-versionado']) {
      let versao = 'antiga';
      const recebidos = [];
      const servidor = http.createServer((req, res) => {
        const url = new URL(req.url, 'http://localhost');
        const nome = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
        if (!['index.html', 'app.js', 'base.css', 'worker.js', 'motor.wasm'].includes(nome)) { res.writeHead(404); res.end(); return; }
        recebidos.push(`${versao}:${req.url}`);
        let texto = fs.readFileSync(path.resolve(__dirname, '../../../target/pacote-web-testes', `cache-${versao}`, nome), 'utf8');
        if (modo === 'sem-correcao' || (versao === 'antiga' && modo === 'cache-antigo')) texto = texto.replace(/\?v=[a-f0-9]{64}/g, '');
        res.writeHead(200, { 'Content-Type': nome.endsWith('.html') ? 'text/html' : nome.endsWith('.css') ? 'text/css' : nome.endsWith('.js') ? 'text/javascript' : 'application/wasm', 'Cache-Control': nome.endsWith('.html') ? 'no-cache' : 'max-age=14400' });
        res.end(texto);
      });
      await new Promise(r => servidor.listen(0, '127.0.0.1', r));
      const contexto = await browser.newContext();
      try {
        const page = await contexto.newPage();
        const url = `http://127.0.0.1:${servidor.address().port}/`;
        await page.goto(url);
        await page.waitForFunction(() => window.trabalhador === 'antiga');
        assert.equal(await page.evaluate(() => window.recurso), 'antiga');
        await page.evaluate(async () => {
          const banco = await new Promise((resolve, reject) => {
            const pedido = indexedDB.open('identidade-cache-teste', 1);
            pedido.onupgradeneeded = () => pedido.result.createObjectStore('chaves');
            pedido.onsuccess = () => resolve(pedido.result); pedido.onerror = () => reject(pedido.error);
          });
          await new Promise(resolve => { const tx = banco.transaction('chaves', 'readwrite'); tx.objectStore('chaves').put('mesma-identidade', 'pessoa'); tx.oncomplete = resolve; });
          banco.close();
        });
        versao = 'nova';
        // Navegação normal: nenhum reload que force revalidar os subrecursos.
        await page.goto('about:blank');
        await page.goto(url);
        const esperada = modo === 'sem-correcao' ? 'antiga' : 'nova';
        await page.waitForFunction(v => window.trabalhador === v, esperada);
        assert.equal(await page.evaluate(() => window.versao), esperada);
        const wasm = await page.evaluate(() => window.recurso);
        // WebKit pode revalidar fetch() enquanto reutiliza JS/CSS frescos:
        // justamente uma mistura de versões. A correção exige todos novos.
        if (modo !== 'sem-correcao') assert.equal(wasm, esperada);
        assert.equal(await page.evaluate(() => getComputedStyle(document.body).getPropertyValue('--versao').trim()), esperada);
        assert.equal(await page.evaluate(async () => {
          const db = await new Promise(resolve => { const p = indexedDB.open('identidade-cache-teste', 1); p.onsuccess = () => resolve(p.result); });
          const valor = await new Promise(resolve => { const p = db.transaction('chaves').objectStore('chaves').get('pessoa'); p.onsuccess = () => resolve(p.result); }); db.close(); return valor;
        }), 'mesma-identidade');
        for (const nome of ['app.js', 'base.css']) {
          assert.equal(recebidos.some(r => r.startsWith(`nova:/${nome}`)), modo !== 'sem-correcao', `${modo}: cache não foi exercitado em ${nome}`);
        }
        if (modo !== 'sem-correcao') assert(recebidos.some(r => r.startsWith('nova:/motor.wasm?v=')));
        console.log(`PASS ${modo}: JS/CSS/Worker=${esperada}, WASM=${wasm}, identidade preservada`);
      } finally { await contexto.close(); await new Promise(r => servidor.close(r)); }
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
