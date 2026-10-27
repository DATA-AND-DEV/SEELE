// Pacote real, cabeçalhos reais, sem microfone nem servidor de sessão.
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const pw = require('./playwright.cjs');
(async () => {
  const pasta = path.resolve(__dirname, '../../../target/pacote-web');
  const regras = new Map(); let regra;
  for (const linha of fs.readFileSync(path.join(pasta, '_headers'), 'utf8').split('\n')) {
    if (linha.startsWith('/')) { regra = {}; regras.set(linha.trim(), regra); }
    else if (regra && /^\s+[^:]+:/.test(linha)) { const i = linha.indexOf(':'); regra[linha.slice(0, i).trim()] = linha.slice(i + 1).trim(); }
  }
  const pedidos = [], falhas = [];
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://localhost');
    const nome = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
    const arquivo = path.resolve(pasta, nome);
    if (!arquivo.startsWith(pasta + '/') || !fs.existsSync(arquivo)) { falhas.push(req.url); res.writeHead(404); res.end(); return; }
    pedidos.push(req.url);
    const tipos = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
    res.writeHead(200, { ...regras.get('/*'), ...regras.get(url.pathname), 'Content-Type': tipos[path.extname(nome)] || 'application/octet-stream' });
    res.end(fs.readFileSync(arquivo));
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const browser = await (process.argv.includes('--webkit') ? pw.webkit() : pw.chromium()).launch();
  try {
    const page = await browser.newPage(); const erros = [];
    page.on('pageerror', e => erros.push(e.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.evaluate(async () => {
      await carregarOWasmDaPagina();
      await prepararOsExecutoresDoNavegador();
      const ctx = new AudioContext();
      try { await ctx.audioWorklet.addModule(CONFIGURACAO_DA_VOZ.worklet); } finally { await ctx.close(); }
      await document.fonts.ready;
    });
    assert.deepEqual(falhas, [], 'recurso 404');
    assert.deepEqual(erros, [], 'erro de página');
    for (const nome of ['celular-entrada.js', 'seele_web.wasm', 'seele_motor_web.wasm', 'executor-de-mod.html', 'voz-worklet.js']) assert(pedidos.some(p => p.startsWith(`/${nome}?v=`)), `${nome} sem versão ou não carregou`);
    assert(pedidos.some(p => p.startsWith('/fontes/') && p.includes('?v=')), 'fonte não versionada');
    console.log('PASS pacote real + CSP: UI, dois WASM, iframe MOD, worklet e fontes carregaram versionados; zero 404/pageerror');
  } finally { await browser.close(); await new Promise(r => server.close(r)); }
})().catch(e => { console.error(e); process.exitCode = 1; });
