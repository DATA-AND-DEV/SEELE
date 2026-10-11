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
    // A transmissão desta pessoa no ar, no retrato que o produto lê: a troca
    // durante a transmissão pergunta por `minha_transmissao` (ADR 0054), e não
    // mais por `tela.e_minha`. Com o retrato velho, a bancada esperava 30 s
    // por uma chamada que o produto, com razão, não fazia. Os campos são os de
    // `TelaEmCurso` (`crates/seele-ffi/src/types.rs`); sem `pedido`, para a
    // caixa não restaurar limites de outro pedido.
    await page.evaluate(() => {
      window.testTable.snapshot.minha_transmissao = { tela: 1, de: 1, e_minha: true, espectadores: 0, parada: null, pedido: null };
    });
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
    await aRecusaSubstituidaChegaCalada(browser, server.address().port);
    console.log('E54: a recusa da tentativa substituída chega calada, e a recusa comum ainda pinta a linha');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });

// --------------------------------------------------------------------------
// Lote E54 da 0.16, portado para a 0.15.2: a recusa da tentativa substituída
// chega calada
// --------------------------------------------------------------------------
//
// **O caso.** Uma tentativa de entrada lenta (a velha, a 192.0.2.17) e, aos
// 100 ms, outra (a nova, aqui). O `connect` do Rust larga a velha que termina
// depois de a nova ter começado, e a devolve recusada com a marca
// `substituida` (`largar_a_substituida`, no `main.rs`). A janela desta linha
// não tem o contador de tentativas da 0.16: sem conferir a marca, a recusa da
// velha pintava a linha vermelha da entrada com a frase de uma tentativa que a
// pessoa tinha deixado. O guarda de forma é
// `a_recusa_da_tentativa_substituida_chega_calada`, em `tests/frontend.rs`.

const ALVO_E54 = '192.0.2.17:8384';
const AQUI_E54 = '127.0.0.1:8383';
/** O convite que a tentativa velha leva: ele não pode chegar ao registro. */
const CONVITE_E54 = `convite-do-lote-e54-${process.pid}-${Date.now()}`;

async function aRecusaSubstituidaChegaCalada(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    window.testAtrasoPorAlvo = {};
    window.testRecusaPorAlvo = {};
    window.testGeracao = 0;
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        const chamada = { cmd, args };
        window.testCalls.push(chamada);
        if (cmd === 'connect') {
          // A geração do Rust: cada `connect` revoga a anterior ao começar, e
          // o que termina com a dele já revogada volta como
          // `largar_a_substituida` o devolve.
          const geracao = ++window.testGeracao;
          chamada.geracao = geracao;
          const atraso = window.testAtrasoPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          chamada.respondida = true;
          const recusa = window.testRecusaPorAlvo[args.server];
          if (recusa) throw { error: recusa, trail: [] };
          if (geracao !== window.testGeracao) {
            throw { error: 'NotConnected', trail: [], substituida: true };
          }
          return { snapshot, veredito: null, geracao };
        }
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    table: respostas({ apelido_local: 'Bia', snapshot: null, microfones: [], saidas: [] }),
    snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  /** O que a pessoa vê, e a geração em que a janela está. */
  const aTela = () => page.evaluate(() => ({
    entrada: !document.getElementById('tela-boot').hidden,
    sessao: !document.getElementById('tela-sessao').hidden,
    autenticacao: document.getElementById('tela-auth').hidden
      ? null : `${$('tela-auth').dataset.modo} ${$('auth-endereco').textContent}`,
    linha: $('boot-erro').hidden ? null : $('boot-erro').textContent,
    geracao: geracaoDaSessao,
  }));
  const respondida = (alvo) => page.waitForFunction(
    (alvo) => testCalls.slice(testAntes).some(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida),
    alvo, { timeout: 4000 },
  ).catch(() => assert.fail(`a tentativa a ${alvo} não voltou, e este caso não mede nada`));
  /** As linhas que a janela mandou ao `seele.log` desde o começo do caso. */
  const noRegistro = () => page.evaluate(() => testCalls.slice(testAntes)
    .filter(c => c.cmd === 'registrar_da_janela')
    .map(c => [c.args.nivel, c.args.onde, c.args.oQue]));
  /** Começa um caso na tela de entrada. */
  const comecar = (atrasos, recusas) => page.evaluate(([atrasos, recusas]) => {
    window.testAtrasoPorAlvo = atrasos;
    window.testRecusaPorAlvo = recusas;
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    window.testAntes = testCalls.length;
  }, [atrasos, recusas]);

  // ---- 1. A velha leva 1,2 s; aos 100 ms a nova entra aqui. A velha volta
  // recusada como substituída, e a tela da nova não muda.
  await comecar({ [ALVO_E54]: 1200 }, {});
  await page.evaluate(([velha, nova, convite]) => {
    conectar(velha, 'Bia', convite);
    setTimeout(() => conectar(nova, 'Bia', null), 100);
  }, [ALVO_E54, AQUI_E54, CONVITE_E54]);
  await respondida(AQUI_E54);
  await page.waitForTimeout(300);
  const comANova = await aTela();
  assert.equal(comANova.entrada, false, 'a nova não entrou, e a corrida não foi medida');
  await respondida(ALVO_E54);
  await page.waitForTimeout(300);
  const depoisDaVelha = await aTela();
  assert.deepEqual(depoisDaVelha, comANova,
    `a recusa da tentativa a ${ALVO_E54}, substituída pela entrada aqui, mudou a tela da sessão: ` +
    `antes ${JSON.stringify(comANova)}, depois ${JSON.stringify(depoisDaVelha)}`);
  const linhas = await noRegistro();
  const daVelha = linhas.filter(([, , oQue]) => String(oQue).includes(ALVO_E54));
  assert.equal(daVelha.length, 1,
    `a recusa da tentativa substituída não deixou uma linha (e uma só) no seele.log: ${JSON.stringify(linhas)}`);
  assert.deepEqual(daVelha[0].slice(0, 2), ['aviso', 'entrada'],
    `a linha da tentativa substituída não foi como aviso da entrada: ${JSON.stringify(daVelha[0])}`);
  assert.ok(!JSON.stringify(linhas).includes(CONVITE_E54), 'o convite da tentativa substituída foi ao seele.log');
  // A marca sai quando a geração caiu: por outra entrada, ou pelo fim da
  // própria sessão. A linha diz as duas, e não promete «a nova» que pode não
  // existir (M1 da revisão do porte).
  assert.ok(String(daVelha[0][2]).includes('outra entrada começou, ou a sessão acabou')
    && !String(daVelha[0][2]).includes('segue com a nova'),
    `a linha da tentativa recusada atribui a queda da geração a uma causa só: ${daVelha[0][2]}`);

  // ---- 2. Uma recusa sem a marca ainda pinta a linha vermelha, como antes.
  await comecar({}, { [ALVO_E54]: 'Unreachable' });
  await page.evaluate((velha) => { conectar(velha, 'Bia', null); }, ALVO_E54);
  await respondida(ALVO_E54);
  await page.waitForTimeout(300);
  const recusada = await aTela();
  assert.ok(recusada.linha, `uma recusa sem a marca deixou de pintar a linha vermelha: ${JSON.stringify(recusada)}`);

  assert.deepEqual(erros, [], `a página lançou: ${erros.join(' · ')}`);
  await page.close();
}
