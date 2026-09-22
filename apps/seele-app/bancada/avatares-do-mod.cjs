// HTML, CSS, registro e carregador reais; apenas a ponte nativa é simulada.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(table => {
      window.testTable = table;
      window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          testCalls.push({ cmd, args });
          if (cmd === 'ler_imagem_mod' && window.adiarAvatar) {
            return new Promise(resolve => { window.concluirAvatar = resolve; });
          }
          return testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      // Três PNGs diferentes, produzidos localmente pelo navegador.
      window.foto = cor => {
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = 48;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = cor; ctx.fillRect(0, 0, 48, 48);
        return canvas.toDataURL();
      };
      window.nativa = foto('#f2521f');
      window.doMod = foto('#32e6a2');
      testTable.imagem_da_pessoa = nativa;
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: doMod };
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      desenharChamada(r);
      entrarNaGeracao(1);
      window.instanciaTeste = new InstanciaDeMod('teste/perfis', 'hash', geracaoDaSessao, { entregar: async () => {}, encerrar: async () => {} });
      instanciaTeste.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set('teste/perfis', instanciaTeste);
      window.registrarAvatar = (path = 'volume:foto') => contribuicoesDosMods.registrar(
        { id: 'teste/perfis' }, instanciaTeste,
        { ponto: 'pessoa.avatar', modo: 'substituir', alvo: '1',
          conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path } } } },
      ).handle;
      window.handleAvatar = registrarAvatar();
    }, retrato());
    const todos = '[data-pessoa-do-avatar="1"]';
    await page.waitForFunction(() => {
      const avatares = [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')];
      return avatares.length >= 3 && avatares.every(e => e.style.backgroundImage.includes(doMod));
    });
    for (const seletor of ['#operador-inicial', '.chamada-avatar', '.mensagem-avatar']) {
      assert.ok(await page.locator(`${seletor}[data-pessoa-do-avatar="1"]`).count(), seletor);
    }
    assert.equal(await page.evaluate(() => testCalls.filter(c => c.cmd === 'ler_imagem_mod').length), 1,
      'as três superfícies devem compartilhar uma carga');

    await page.evaluate(() => escolherApresentacao('pessoa.cartao', NATIVO));
    assert.ok(await page.locator(todos).evaluateAll(es => es.every(e => e.style.backgroundImage.includes(window.nativa))));
    await page.evaluate(() => escolherApresentacao('pessoa.cartao', ''));
    assert.ok(await page.locator(todos).evaluateAll(es => es.every(e => e.style.backgroundImage.includes(window.doMod))));

    await page.evaluate(() => {
      contribuicoesDosMods.revogar(handleAvatar);
      window.doMod = foto('#729bff');
      testTable.ler_imagem_mod.uri = doMod;
      window.handleAvatar = registrarAvatar('volume:outra');
    });
    await page.waitForFunction(() => [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')]
      .every(e => e.style.backgroundImage.includes(doMod)));

    await page.evaluate(() => {
      $('vista-conversa').hidden = true;
      $('vista-chamada').hidden = false;
    });
    await page.screenshot({ path: '/tmp/seele-avatares-do-mod.png' });
    await page.evaluate(() => contribuicoesDosMods.revogar(handleAvatar));
    await page.waitForFunction(() => [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')]
      .every(e => e.style.backgroundImage.includes(nativa)));

    // Um carregamento que termina depois da revogação não pode pintar a sessão.
    await page.evaluate(() => { window.adiarAvatar = true; window.handleAvatar = registrarAvatar(); });
    await page.waitForFunction(() => typeof concluirAvatar === 'function');
    await page.evaluate(() => {
      contribuicoesDosMods.revogar(handleAvatar);
      concluirAvatar(testTable.ler_imagem_mod);
    });
    await page.waitForFunction(() => [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')]
      .every(e => e.style.backgroundImage.includes(nativa)));

    // Descarregar o MOD solta o cache e a contribuição pelo ciclo real da instância.
    await page.evaluate(() => {
      window.adiarAvatar = false;
      window.handleAvatar = registrarAvatar();
    });
    await page.waitForFunction(() => [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')]
      .every(e => e.style.backgroundImage.includes(doMod)));
    await page.evaluate(() => instanciaTeste.encerrar());
    await page.waitForFunction(() => [...document.querySelectorAll('[data-pessoa-do-avatar="1"]')]
      .every(e => e.style.backgroundImage.includes(nativa)));
    await page.evaluate(() => {
      retratos.clear();
      redesenharAvatares();
    });
    assert.ok(await page.locator(todos).evaluateAll(es => es.every(e => !e.style.backgroundImage && e.textContent.trim())));
    assert.equal(await page.evaluate(() => instanciaTeste.recursos.length), 0);
    assert.deepEqual(errors, []);
    console.log('avatares: chamada, operador e mensagens; cache, troca, preferência, remoção e carga tardia conferidos.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
