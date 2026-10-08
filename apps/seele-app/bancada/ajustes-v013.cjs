// Navegador com HTML/CSS reais; a ponte nativa é simulada.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium: abrirChromium } = require('./playwright.cjs');
const chromium = abrirChromium();

/**
 * Espera um elemento dizer `trecho`, e reprova com o que ele diz se não disser.
 *
 * Três segundos: a faixa de contexto da configuração é redesenhada por
 * `atualizarServer`, que roda a cada 500 ms, e lê o quadro da sessão, que
 * também anda a cada 500 ms. Um segundo cobre os dois; o resto é folga de
 * máquina lenta.
 */
async function esperarQueDiga(page, id, trecho, defeito) {
  try {
    await page.waitForFunction(
      ([id, trecho]) => (document.getElementById(id)?.textContent ?? '').includes(trecho),
      [id, trecho],
      { timeout: 3000 },
    );
  } catch {
    const agora = await page.evaluate(id => document.getElementById(id)?.textContent, id);
    assert.fail(`${defeito} — #${id} diz «${agora}»`);
  }
}

/**
 * A tinta de cada linha do título do perfil, pela fonte de verdade.
 *
 * A área de conteúdo do `Range` não serve: esta face tem 38px de área para 24px
 * de letra, e as áreas se sobrepõem com qualquer altura de linha abaixo de 1,58.
 * O defeito que se vê é a tinta de uma linha encostar na da outra. A linha de
 * base sai do topo da área mais o ascendente da fonte; a altura da tinta, de
 * `measureText` do texto daquela linha.
 */
function tintasDoTitulo() {
  const h2 = document.getElementById('perfil-titulo');
  const no = h2.firstChild;
  const linhas = [];
  for (let i = 0; i < no.textContent.length; i++) {
    const faixa = document.createRange();
    faixa.setStart(no, i);
    faixa.setEnd(no, i + 1);
    const caixa = faixa.getClientRects()[0];
    if (!caixa) continue;
    let linha = linhas.find(l => Math.abs(l.topo - caixa.top) < 1);
    if (!linha) linhas.push(linha = { topo: caixa.top, texto: '' });
    linha.texto += no.textContent[i];
  }
  const estilo = getComputedStyle(h2);
  const ctx = document.createElement('canvas').getContext('2d');
  ctx.font = `${estilo.fontWeight} ${estilo.fontSize} ${estilo.fontFamily}`;
  const tintas = linhas.map(l => {
    const m = ctx.measureText(l.texto.trim());
    const base = l.topo + m.fontBoundingBoxAscent;
    return { texto: l.texto.trim(), topo: base - m.actualBoundingBoxAscent, fim: base + m.actualBoundingBoxDescent };
  });
  const folga = tintas.length > 1 ? Math.round((tintas[1].topo - tintas[0].fim) * 10) / 10 : null;
  return { tintas, folga, lineHeight: estilo.lineHeight };
}

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
    // **A faixa de contexto acompanha o que muda com a configuração aberta**
    // (lote J da F5). Ela diz a sala, o modo do microfone e, no modo tecla, a
    // tecla — e era escrita só ao abrir: trocada a tecla para F, o botão dizia
    // F e a faixa ao lado continuava em ESPAÇO; trocado o modo, a faixa dizia o
    // de antes.
    await esperarQueDiga(page, 'server-contexto', 'microfone abre enquanto segura ESPAÇO',
      'a faixa de contexto não diz a tecla de falar padrão no modo tecla');
    await page.evaluate(() => { window.testTable.tecla_de_falar = 'KeyF'; pararDeOuvirTecla(); });
    await esperarQueDiga(page, 'server-contexto', 'microfone abre enquanto segura F',
      'trocada a tecla de falar para F, a faixa de contexto continuou dizendo a tecla de antes');
    // O modo: a ponte aplica e o retrato da sessão passa a dizer o novo.
    await page.evaluate(() => { window.testTable.snapshot.voice_mode = 'VoiceActivated'; return escolherModo('VoiceActivated'); });
    await esperarQueDiga(page, 'server-contexto', 'microfone abre por voz',
      'trocado o modo do microfone para VOZ, a faixa de contexto continuou dizendo o modo de antes');
    await page.evaluate(() => { window.testTable.snapshot.voice_mode = 'PushToTalk'; window.testTable.tecla_de_falar = null; pararDeOuvirTecla(); });
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

    // **A entrada por link, sem nome nesta máquina** (lote J da F5). O launcher
    // pede `entrar`, e a primeira entrada sem nome pergunta antes do `connect`.
    // A ponte daqui resolve em microtarefa, que é o que expunha a ordem de
    // carga: com a abertura chamada no topo de `tela-boot.js`, a entrada
    // chegava à pergunta antes de `camada-nomear.js` existir — `perguntarONome
    // is not defined`, e a pergunta nunca abria.
    const link = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const errosDoLink = []; link.on('pageerror', e => errosDoLink.push(e.message));
    await link.addInitScript(table => {
      window.testTable = table; window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          window.testCalls.push({ cmd, args });
          // O que se grava fica gravado: a pergunta relê o nome depois de fechar.
          if (cmd === 'escolher_apelido_local') window.testTable.apelido_local = args.apelido;
          return window.testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({
      apelido_local: '', snapshot: null, microfones: [], saidas: [],
      abertura: { entrar: 'seele://192.0.2.1:8384', hospedar: false, nome_publico: null },
      analisar_convite: { alvo: '192.0.2.1:8384', token: null },
    }));
    await link.goto(`http://127.0.0.1:${server.address().port}`);
    try {
      await link.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 });
    } catch {
      const naEntrada = await link.evaluate(() => document.getElementById('boot-erro').textContent);
      assert.fail(`aberta por um link sem nome nesta máquina, a janela não perguntou o nome — a entrada diz «${naEntrada}»`);
    }
    assert.equal(await link.evaluate(() => $('perfil-titulo').textContent), 'COMO OS OUTROS VÃO TE CHAMAR?');
    assert.equal(await link.evaluate(() => testCalls.filter(c => c.cmd === 'connect').length), 0,
      'a entrada por link conectou antes de a pergunta do nome ser respondida');

    // O título da pergunta quebra em duas linhas na caixa de 480px. Com a
    // altura de linha herdada de `base.css` (16px para letra de 24px), a tinta
    // da segunda encostava na da primeira (folga de −0,9px).
    await link.evaluate(() => document.fonts.ready);
    const titulo = await link.evaluate(tintasDoTitulo);
    assert.equal(titulo.tintas.length, 2, `o título da pergunta deixou de quebrar em duas linhas, e esta medida não mede mais nada: ${JSON.stringify(titulo)}`);
    assert.ok(titulo.folga > 0,
      `as duas linhas do título da pergunta se sobrepõem: folga de ${titulo.folga}px entre as tintas, com line-height ${titulo.lineHeight}`);

    // Responder pelo teclado: o foco está no campo, e Enter salva.
    await link.keyboard.type('Bia');
    await link.keyboard.press('Enter');
    await link.waitForFunction(() => testCalls.some(c => c.cmd === 'connect'), null, { timeout: 3000 });
    const ordem = await link.evaluate(() => testCalls.map(c => c.cmd).filter(c => c === 'escolher_apelido_local' || c === 'connect'));
    assert.deepEqual(ordem, ['escolher_apelido_local', 'connect'], 'o nome não foi gravado antes do connect');
    assert.equal(await link.evaluate(() => testCalls.find(c => c.cmd === 'connect').args.nickname), 'Bia',
      'a entrada por link não levou o nome que acabou de ser respondido');
    assert.deepEqual(errosDoLink, []);
    await link.close();

    console.log('v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium');
    console.log(`lote J: a faixa de contexto acompanha a tecla e o modo, a entrada por link pergunta o nome antes do connect, e o título da pergunta não se sobrepõe (folga de ${titulo.folga}px entre as tintas)`);
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
