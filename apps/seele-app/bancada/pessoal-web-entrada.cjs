// Exercita o ciclo da UI com serviço/mídia controlados. Permissão tardia não
// reativa microfone depois de sair; fechar página desmonta todos os recursos.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const engines = require('./playwright.cjs');
const ui = path.resolve(__dirname, '../ui');
(async () => {
  const server = http.createServer((req, res) => {
    if (req.url === '/') { res.setHeader('Content-Type', 'text/html'); res.end('<!doctype html><meta name="viewport" content="width=device-width"><button id="boot-conversa-pessoal" hidden>CONVERSAR POR LINK</button><span id="boot-perfil-nome">Pessoa</span><script src="/glifos.js"></script><script src="/pessoal-web.js"></script><script src="/pessoal-web-entrada.js"></script>'); return; }
    const nome = req.url.slice(1);
    if (!['glifos.js','pessoal-web.js','pessoal-web-entrada.js'].includes(nome)) { res.statusCode = 404; res.end(); return; }
    res.setHeader('Content-Type', 'text/javascript'); res.end(fs.readFileSync(path.join(ui, nome)));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try { for (const engine of ['chromium', 'webkit']) {
    const browser = await engines[engine]().launch({ headless: true });
    try {
      const page = await browser.newPage();
      await page.addInitScript(() => {
        window.PONTE_NO_LUGAR_DO_TAURI = true;
        window.prova = { fechouVoz: 0, entrouVoz: 0, fechouTela: 0, destruiu: 0, liberouFoco: 0 };
        window.invocarNaPonte = async cmd => { if (cmd === 'focar_voz') prova.liberouFoco++; };
        window.carregarOWasmDaPagina = async () => ({ bytes: new ArrayBuffer(0) });
        window.temCapturaParaCodificar = () => false;
        window.criarVozDoNavegador = () => ({
          ligar: () => new Promise(ok => window.permitirMicrofone = ok),
          desligar: async () => { prova.fechouVoz++; }, modo() {},
          entrar: () => { prova.entrouVoz++; }, contas: async () => ({ pagina: { falhas: [] } }),
        });
        window.criarCompartilhamentoPessoalWeb = () => ({
          fechar: () => { prova.fechouTela++; }, receberControle() {},
        });
        window.criarServicoDeChamadasWeb = async callbacks => {
          window.callbacksDaChamada = callbacks;
          return {
            wasm: {}, criarConvite: async () => { callbacks.aoEstado({ estado: 'recebendo' }); return { link: 'https://example.test/#chamada/teste' }; },
            abrirConvite: async () => {}, chamar: async () => {}, atender: async () => {},
            destruir: () => { prova.destruiu++; }, encerrar: () => callbacks.aoEstado({ estado: 'encerrada' }),
            revogarConvite: async () => {},
          };
        };
      });
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.locator('#boot-conversa-pessoal').click();
      await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).click();
      await page.evaluate(() => {
        callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-recebe' });
        callbacksDaChamada.aoPar({ transporteDaVoz: {}, tela: {}, abortar() {} }, { papel: 'quem-recebe' });
      });
      await page.waitForFunction(() => prova.liberouFoco === 1);
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => typeof permitirMicrofone === 'function');
      await page.getByRole('button', { name: 'SAIR DA CONVERSA' }).click();
      await page.evaluate(() => permitirMicrofone());
      await page.waitForFunction(() => prova.fechouVoz >= 2);
      assert.equal(await page.evaluate(() => prova.entrouVoz), 0, 'a permissão tardia reabriu voz após sair');
      assert.ok(await page.evaluate(() => prova.fechouTela >= 1 && prova.destruiu >= 1));
      assert.equal(await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).count(), 0);
      await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide')));
      console.log(`${engine}: foco cedido, permissão tardia, saída com operação pendente e pagehide PASS`);
    } finally { await browser.close(); }
  } } finally { server.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
