// O palco dos MODs sobre a chamada (rascunho da API 6, §5; tarefa 7, Step 5 da
// F3), com o HTML, o CSS e o registro reais; só a ponte nativa é simulada. No
// molde de `tema-do-servidor.cjs`.
//
// # O que ela mede
//
// `chamada.palco` é um ponto de perfil `superficie` — a gramática inteira, com
// toque e clique — sobre a grade da chamada:
//
// 1. **O palco monta**: a substituição escolhida no miolo, e a grade continua
//    no DOM, repintada, fora da vista enquanto o palco está à frente.
// 2. **O clique chega ao MOD**: é superfície, e não ponto de cartão.
// 3. **A saída é do produto**: VER A GRADE traz a grade e esconde o palco sem
//    desmontá-lo; VER O PALCO o devolve.
// 4. **Os acréscimos** (`adicionar`) moram numa faixa do palco, abaixo do
//    miolo, e aparecem sem substituição nenhuma.
// 5. **Revogar** devolve a grade quando não sobra nada.
// 6. **Sair da sala desmonta o palco**, mesmo com a conversa à frente: a
//    chamada só se redesenha com a vista dela à frente, e o desenho da sessão
//    é o que vê a sala mudar.
// 7. O retrato diz que a casca tem `chamada.palco`.
//
// Rodar:  node apps/seele-app/bancada/palco-da-chamada.cjs

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
      window.__TAURI__ = {
        core: { invoke: async (cmd) => testTable[cmd] ?? null },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(async r => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      await abrirChamada();
      entrarNaGeracao(1);
      window.eventos = [];
      for (const id of ['mod/mesa', 'mod/dados']) {
        const inst = new InstanciaDeMod(id, 'h', geracaoDaSessao, {
          pedirEncerramento() {}, async encerrou() { return true; },
          entregar(m) { if (m.tipo === 'evento') eventos.push({ mod: id, ...JSON.parse(JSON.stringify(m)) }); },
        });
        inst.estado = ESTADOS_DE_MOD.ativa;
        inst.api = 6;
        modsCarregados.set(id, inst);
      }
    }, retrato());

    /** Redesenha a sessão e a chamada com o retrato da bancada. */
    const redesenhar = () => page.evaluate(async () => {
      desenhar(testTable.snapshot);
      if (!$('vista-chamada').hidden) await atualizarChamada();
    });
    const registrar = (mod, pedido) => page.evaluate(([mod, pedido]) => {
      const inst = modsCarregados.get(mod);
      return contribuicoesDosMods.registrar({ id: mod }, inst, pedido).handle;
    }, [mod, pedido]);
    const revogar = (mod, handle) => page.evaluate(([mod, handle]) => {
      contribuicoesDosMods.revogar(handle, { id: mod, instancia: modsCarregados.get(mod) });
    }, [mod, handle]);
    const estado = () => page.evaluate(() => {
      const palco = $('palco-de-mods');
      const grade = $('chamada-grade');
      return {
        palco: Boolean(palco) && !palco.hidden && getComputedStyle(palco).display !== 'none',
        grade: getComputedStyle(grade).display !== 'none' && grade.getClientRects().length > 0,
        cartoes: grade.isConnected ? grade.querySelectorAll('.chamada-cartao').length : -1,
        miolo: $('palco-de-mods-miolo')?.textContent ?? null,
        acrescimos: $('palco-de-mods-acrescimos')?.textContent ?? null,
        quem: $('palco-de-mods-quem')?.textContent ?? null,
        verOPalco: Boolean($('palco-de-mods-ver-o-palco')) && !$('palco-de-mods-ver-o-palco').hidden,
      };
    });

    // 7. O retrato diz que a casca tem o palco.
    assert.ok(await page.evaluate(() => cascaDoRetrato().regioes.includes('chamada.palco')),
      'o retrato não diz que esta casca tem `chamada.palco`');

    // 1. O palco monta, e a grade continua no DOM.
    assert.equal((await estado()).palco, false, 'sem contribuição, o palco apareceu');
    const mesa = await registrar('mod/mesa', {
      ponto: 'chamada.palco', modo: 'substituir',
      conteudo: [
        { forma: 'texto', chave: 'mapa', dentro: 'o mapa da mesa' },
        { forma: 'botao', chave: 'rolar', dentro: 'ROLAR' },
        { forma: 'tela', chave: 'tabuleiro', largura: 64, altura: 64 },
      ],
    });
    await redesenhar();
    let agora = await estado();
    assert.equal(agora.palco, true, `o palco não apareceu com a substituição: ${JSON.stringify(agora)}`);
    assert.match(agora.miolo, /o mapa da mesa/, 'o miolo do palco não tem o desenho do MOD');
    assert.match(agora.quem, /mod\/mesa/, 'a barra do palco não diz de quem ele é');
    assert.equal(agora.grade, false, 'com o palco à frente, a grade ficou à vista ao lado dele');
    assert.equal(agora.cartoes, 2, 'a grade saiu do DOM, ou parou de ser desenhada, com o palco à frente');
    assert.ok(await page.evaluate(() => Boolean($('palco-de-mods-miolo').querySelector('canvas'))),
      'a `tela` não montou: o palco é superfície, e aceita a gramática inteira');

    // 2. O clique chega ao MOD.
    await page.evaluate(() => [...$('palco-de-mods-miolo').querySelectorAll('button')]
      .find((b) => b.textContent === 'ROLAR').click());
    assert.ok(await page.evaluate(() => eventos.some((e) => e.mod === 'mod/mesa' && e.nome === 'botao' && e.chave === 'rolar')),
      'o clique no palco não chegou ao MOD');

    // 3. A saída é do produto, e esconder não desmonta.
    const no = await page.evaluateHandle(() => $('palco-de-mods-miolo').firstElementChild);
    await page.click('#palco-de-mods-ver-a-grade');
    agora = await estado();
    assert.equal(agora.palco, false, 'VER A GRADE não escondeu o palco');
    assert.equal(agora.grade, true, 'VER A GRADE não trouxe a grade');
    assert.equal(agora.verOPalco, true, 'com o palco escondido, não há como voltar a ele');
    assert.ok(await no.evaluate((n) => n.isConnected), 'VER A GRADE desmontou o palco: o som dele pararia');
    await redesenhar();
    assert.equal((await estado()).palco, false, 'o redesenho seguinte desfez a escolha de ver a grade');
    await page.click('#palco-de-mods-ver-o-palco');
    agora = await estado();
    assert.equal(agora.palco, true, 'VER O PALCO não devolveu o palco');
    assert.equal(agora.verOPalco, false, 'com o palco à frente, VER O PALCO continuou');

    // 4. Os acréscimos.
    const dados = await registrar('mod/dados', {
      ponto: 'chamada.palco', modo: 'adicionar', conteudo: [{ forma: 'texto', chave: 'd', dentro: 'os dados rolados' }],
    });
    await redesenhar();
    agora = await estado();
    assert.match(agora.acrescimos, /os dados rolados/, 'o acréscimo não apareceu na faixa do palco');
    assert.doesNotMatch(agora.miolo, /os dados rolados/, 'o acréscimo entrou no miolo de quem substitui');

    // 5. Revogar.
    await revogar('mod/mesa', mesa);
    await redesenhar();
    agora = await estado();
    assert.equal(agora.palco, true, 'sem a substituição, o acréscimo sumiu junto com o palco');
    assert.equal(agora.miolo, '', 'revogada, a substituição ficou no miolo');
    await revogar('mod/dados', dados);
    await redesenhar();
    agora = await estado();
    assert.equal(agora.palco, false, 'sem nada no palco, ele ficou de pé');
    assert.equal(agora.grade, true, 'sem nada no palco, a grade não voltou');
    assert.equal(agora.verOPalco, false, 'sem palco, VER O PALCO apareceu');

    // 6. Sair da sala desmonta, com a conversa à frente.
    await registrar('mod/mesa', {
      ponto: 'chamada.palco', modo: 'substituir', conteudo: [{ forma: 'texto', chave: 'mapa', dentro: 'o mapa da mesa' }],
    });
    await redesenhar();
    const montado = await page.evaluateHandle(() => $('palco-de-mods-miolo').firstElementChild);
    assert.ok(await montado.evaluate((n) => Boolean(n)), 'o palco não remontou');
    await page.evaluate(() => fecharChamada());
    await page.evaluate(() => {
      const r = testTable.snapshot;
      testTable.snapshot = { ...r, voice_rooms: r.voice_rooms.map((s) => ({ ...s, occupied_by_us: false })) };
      desenhar(testTable.snapshot);
    });
    assert.equal(await montado.evaluate((n) => n.isConnected), false,
      'saiu da sala com a conversa à frente, e o palco continuou montado: o som dele seguiria');
    // E de volta à sala, ele volta.
    await page.evaluate(async () => {
      const r = testTable.snapshot;
      testTable.snapshot = { ...r, voice_rooms: r.voice_rooms.map((s) => ({ ...s, occupied_by_us: s.id === 1 })) };
      await abrirChamada();
    });
    assert.match((await estado()).miolo, /o mapa da mesa/, 'de volta à sala, o palco não voltou');

    assert.deepEqual(errors, []);
    console.log('palco da chamada: monta sobre a grade, que continua no DOM; o clique chega ao MOD; a saída é do produto e não desmonta; os acréscimos têm faixa própria; revogar devolve a grade; sair da sala desmonta, mesmo com a conversa à frente.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
