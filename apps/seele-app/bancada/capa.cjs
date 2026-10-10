// A capa de uma pessoa (`pessoa.capa`; rascunho da API 6, §4; tarefa 6 da F3),
// com o HTML, o CSS, o registro e o carregador reais; só a ponte nativa é
// simulada. No molde de `avatares-do-mod.cjs`.
//
// # O que ela mede
//
// 1. Registrada para uma pessoa, a capa pinta o cartão dela na chamada: o
//    cartão ganha `data-pessoa-da-capa`, e a imagem é a do MOD.
// 2. **O halo de quem fala e a pastilha ficam acima da capa**: no centro da
//    pastilha, o elemento que recebe o clique é ela, e não a capa.
// 3. **A capa recusada** deixa o cartão com o fundo liso, e o MOD recebe o
//    `midia` com o ponto e o alvo (rascunho §7.1).
// 4. **Com movimento reduzido**, o cartão usa o quadro estático, e não a
//    imagem que se move.
//
// Rodar:  node apps/seele-app/bancada/capa.cjs

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
          if (cmd === 'ler_imagem_mod' && window.recusarCapa) return { papel: 'som', bytes: 64, uri: 'data:audio/wav;base64,' };
          return testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      window.foto = cor => {
        const canvas = document.createElement('canvas');
        canvas.width = 96;
        canvas.height = 32;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = cor; ctx.fillRect(0, 0, 96, 32);
        // JPEG: o quadro estático sai em PNG, e as duas não se confundem.
        return canvas.toDataURL('image/jpeg');
      };
      window.daCapa = foto('#32e6a2');
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      desenharChamada(r);
      // A vista da chamada na frente: o cartão precisa de caixa para ser medido.
      $('vista-conversa').hidden = true;
      $('vista-chamada').hidden = false;
      entrarNaGeracao(1);
      window.entregues = [];
      window.instanciaTeste = new InstanciaDeMod('teste/perfis', 'hash', geracaoDaSessao, {
        entregar: async m => { entregues.push(m); },
        encerrar: async () => {},
      });
      instanciaTeste.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set('teste/perfis', instanciaTeste);
      window.registrarCapa = (alvo, path = 'volume:capa') => contribuicoesDosMods.registrar(
        { id: 'teste/perfis' }, instanciaTeste,
        { ponto: 'pessoa.capa', modo: 'substituir', alvo,
          conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path } } } },
      ).handle;
    }, retrato());
    // Lia (id 2) fala no retrato da bancada? Quem falar, o cartão dela é o que
    // mostra o halo; a capa vai para ela.
    const alvo = await page.evaluate(() => {
      const pessoas = testTable.snapshot.voice_rooms.flatMap(s => s.people);
      return String((pessoas.find(p => !p.is_self) ?? pessoas[0]).id);
    });
    const cartao = `.chamada-cartao[data-pessoa-da-capa="${alvo}"]`;

    // 1. A capa pinta o cartão.
    const registro = await page.evaluate(alvo => {
      try {
        window.handleCapa = registrarCapa(alvo);
        desenharChamada(testTable.snapshot);
        return '';
      } catch (erro) { return erro.message; }
    }, alvo);
    assert.equal(registro, '', `pessoa.capa foi recusada no registro: ${registro}`);
    await page.waitForFunction(seletor => {
      const el = document.querySelector(seletor);
      const camada = el?.querySelector(':scope > .capa-camada');
      return camada && getComputedStyle(camada).backgroundImage.includes(window.daCapa);
    }, cartao, { timeout: 5000 });

    // 2. A pastilha e o nome ficam acima da capa: no centro de cada um, o
    //    elemento que recebe o clique é ele (ou um filho dele).
    const porCima = await page.evaluate(seletor => {
      const el = document.querySelector(seletor);
      return ['.chamada-pastilha:not([hidden])', '.chamada-cartao-nome'].map(alvoInterno => {
        const no = el.querySelector(alvoInterno);
        if (!no) return `${alvoInterno}: ausente`;
        const caixa = no.getBoundingClientRect();
        const noCentro = document.elementFromPoint(caixa.left + caixa.width / 2, caixa.top + caixa.height / 2);
        return no.contains(noCentro) ? '' : `${alvoInterno}: coberto por ${noCentro?.className}`;
      }).filter(Boolean);
    }, cartao);
    assert.deepEqual(porCima, [], 'a capa ficou por cima do que o produto escreve no cartão');

    // 3. Com movimento reduzido, o quadro estático.
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.evaluate(() => redesenharCapas());
    const comMovimentoReduzido = await page.evaluate(seletor => {
      const camada = document.querySelector(seletor)?.querySelector(':scope > .capa-camada');
      const fundo = camada ? getComputedStyle(camada).backgroundImage : '';
      return { temImagem: fundo.startsWith('url('), eAOriginal: fundo.includes(window.daCapa) };
    }, cartao);
    assert.ok(comMovimentoReduzido.temImagem && !comMovimentoReduzido.eAOriginal,
      `com movimento reduzido, o cartão não usou o quadro estático: ${JSON.stringify(comMovimentoReduzido)}`);
    await page.emulateMedia({ reducedMotion: 'no-preference' });

    // 4. A capa recusada: fundo liso, e o MOD sabe, com o ponto e o alvo.
    await page.evaluate(alvo => {
      contribuicoesDosMods.revogar(handleCapa);
      window.recusarCapa = true;
      entregues.length = 0;
      window.handleCapa = registrarCapa(alvo, 'volume:recusada');
      desenharChamada(testTable.snapshot);
    }, alvo);
    await page.waitForFunction(() => entregues.some(m => m.nome === 'midia'), null, { timeout: 5000 });
    const recusada = await page.evaluate(seletor => {
      const el = document.querySelector(seletor);
      const camada = el?.querySelector(':scope > .capa-camada');
      return {
        fundo: camada ? getComputedStyle(camada).backgroundImage : 'none',
        comCapa: el?.dataset.comCapa ?? null,
        aviso: entregues.find(m => m.nome === 'midia') ?? null,
      };
    }, cartao);
    assert.ok(!recusada.fundo.startsWith('url(') && !recusada.comCapa,
      `a capa recusada deixou imagem no cartão: ${JSON.stringify(recusada)}`);
    assert.equal(recusada.aviso?.ponto, 'pessoa.capa', `o midia não disse o ponto: ${JSON.stringify(recusada.aviso)}`);
    assert.equal(String(recusada.aviso?.alvo), alvo, `o midia não disse o alvo: ${JSON.stringify(recusada.aviso)}`);

    assert.deepEqual(errors, []);
    console.log('capa: o cartão pintado, a pastilha e o nome por cima, o quadro estático com movimento reduzido, e a recusa dita com o ponto e o alvo.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
