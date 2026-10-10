// O tema do servidor na casca (rascunho da API 6, §9; tarefa 17d da F3), com
// o HTML, o CSS e o tema reais; só a ponte nativa é simulada. No molde de
// `capa.cjs`.
//
// # O que ela mede
//
// O núcleo guarda o `TemaMudou`, e o retrato o leva em `tema_do_servidor`
// (testes em `seele-core` e `seele-ffi`). Aqui, a outra metade:
//
// 1. **O tema chega e é aplicado antes de desenhar a sessão**: os tokens dele
//    em `#tela-sessao`, com as mesmas tabelas e os mesmos pisos do tema de um
//    MOD.
// 2. **`None` o tira por inteiro**: nenhum token do tema fica.
// 3. **Um tema que a janela recusa não fica** — nem o de antes —, e o motivo
//    vai ao `seele.log`.
// 4. **Sair da sessão o tira**, como o tema de um MOD.
//
// Rodar:  node apps/seele-app/bancada/tema-do-servidor.cjs

const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

/** Um tema claro que os pares aceitam (o de `tema-claro-legivel.cjs`). */
const CLARO = { fundo: '#f2ece0', painel: '#e6dfd0', texto: '#12100e', apagado: '#5a5148', acento: '#8a4a00', borda: '#b8ad9c' };

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
        core: { invoke: async (cmd, args) => { testCalls.push({ cmd, args }); return testTable[cmd] ?? null; } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
    }, retrato());

    // Os tokens do tema em `#tela-sessao`, pelo nome da API.
    const tokens = () => page.evaluate(() => Object.fromEntries(Object.entries(TEMA_DA_API)
      .map(([nome, token]) => [nome, $('tela-sessao').style.getPropertyValue(token).trim()])
      .filter(([, valor]) => valor)));
    const desenharCom = (tema) => page.evaluate((tema) => {
      testTable.snapshot = { ...testTable.snapshot, tema_do_servidor: tema };
      desenhar(testTable.snapshot);
    }, tema);
    const comoVeio = { mod_id: 'seele/estilo', valores: JSON.stringify(CLARO) };

    // 1. O tema chega e é aplicado.
    assert.deepEqual(await tokens(), {}, 'a sessão começou com tokens de tema');
    await desenharCom(comoVeio);
    const aplicados = await tokens();
    for (const [nome, cor] of Object.entries(CLARO)) {
      assert.equal(aplicados[nome], cor, `o tema do servidor não pôs «${nome}» na sessão: ${JSON.stringify(aplicados)}`);
    }

    // 2. `None` o tira por inteiro.
    await desenharCom(null);
    assert.deepEqual(await tokens(), {}, 'o None do servidor deixou tokens do tema na sessão');

    // 3. Um tema que a janela recusa — o acento igual ao fundo — não fica, nem
    //    o de antes, e o motivo vai ao seele.log.
    await desenharCom(comoVeio);
    const antesDoLog = await page.evaluate(() => testCalls.length);
    await desenharCom({ mod_id: 'seele/estilo', valores: JSON.stringify({ ...CLARO, acento: CLARO.fundo }) });
    assert.deepEqual(await tokens(), {}, 'um tema recusado deixou tokens na sessão, dele ou do de antes');
    const linha = await page.evaluate((de) => testCalls.slice(de).find((c) => c.cmd === 'registrar_da_janela')?.args, antesDoLog);
    assert.ok(linha && linha.modId === 'seele/estilo' && /tema/.test(linha.oQue), `o tema recusado não foi ao seele.log: ${JSON.stringify(linha)}`);

    // 4. Sair da sessão o tira.
    await desenharCom(comoVeio);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'o tema não voltou depois da recusa');
    await page.evaluate(() => encerrarOAmbienteDosMods());
    assert.deepEqual(await tokens(), {}, 'o tema do servidor ficou depois de sair da sessão');
    // E a sessão seguinte, com o mesmo tema, o tem de novo: o «em vigor» saiu
    // junto, e não é confundido com o que já estava aplicado.
    await desenharCom(comoVeio);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'a sessão seguinte, com o mesmo tema, ficou sem ele');

    assert.deepEqual(errors, []);
    console.log('tema do servidor: chega pelo retrato e é aplicado, o None o tira, o recusado não fica e vai ao seele.log, e sai com a sessão.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
