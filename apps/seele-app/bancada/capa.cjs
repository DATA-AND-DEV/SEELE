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

    // **A legibilidade, medida** (rascunho §4; tarefa 6, Step 4): todo texto do
    // produto sobre a capa mantém pelo menos o contraste que tem sem ela,
    // conferido contra uma capa branca e uma preta, com a pessoa calada e
    // falando. O fundo efetivo de um texto é a primeira cor opaca entre ele e o
    // cartão; sem nenhuma, é a capa.
    const falar = (sim) => page.evaluate(([alvo, sim]) => {
      const r = testTable.snapshot;
      for (const p of r.voice_rooms.flatMap(s => s.people)) if (String(p.id) === alvo) p.speaking = sim;
      desenharChamada(r);
    }, [alvo, sim]);
    const medirEm = (seletorDoQuadro, cor) => page.evaluate(([seletor, corDaCapa]) => {
      const quadro = document.querySelector(seletor);
      const rgb = (texto) => (texto.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) => [r, g, b]
        .map((c) => { const v = c / 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; })
        .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
      const fundo = (el) => {
        for (let n = el; n; n = n.parentElement) {
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] >= 0.99)) return c.slice(0, 3);
          if (n === quadro && corDaCapa) return corDaCapa;
        }
        return [0, 0, 0];
      };
      const textos = [...quadro.querySelectorAll('*')]
        .filter((e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim()) && e.getClientRects().length);
      return Object.fromEntries(textos.map((e, i) => [
        `${e.className}#${i}`,
        Number(razao(rgb(getComputedStyle(e).color).slice(0, 3), fundo(e)).toFixed(2)),
      ]));
    }, [seletorDoQuadro, cor]);
    const medir = (cor) => medirEm(`.chamada-cartao[data-pessoa-da-capa="${alvo}"]`, cor);
    const identidadeSemCapa = await medirEm('#operador-quem', null);
    await falar(false);
    const semCapa = { calado: await medir(null) };
    await falar(true);
    semCapa.falando = await medir(null);
    await falar(false);

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

    // A legibilidade, com a capa branca e com a preta, calado e falando.
    const pioras = [];
    for (const [nomeDaCapa, cor] of [['branca', [255, 255, 255]], ['preta', [0, 0, 0]]]) {
      for (const sim of [false, true]) {
        await falar(sim);
        const com = await medir(cor);
        const sem = semCapa[sim ? 'falando' : 'calado'];
        for (const [texto, antes] of Object.entries(sem)) {
          if ((com[texto] ?? 0) < antes - 0.01) pioras.push(`${texto} (${sim ? 'falando' : 'calado'}, capa ${nomeDaCapa}): ${antes} → ${com[texto]}`);
        }
      }
    }
    await falar(false);
    assert.deepEqual(pioras, [], 'um texto do produto perdeu contraste sobre a capa');

    // E a capa de quem usa veste a identidade, e não os controles: MUDO, fone e
    // SAIR nunca ficam sobre ela (núcleo, §10).
    const nucleo = await page.evaluate(() => {
      contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.capa', modo: 'substituir', alvo: String(testTable.snapshot.me),
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:eu' } } },
      });
      desenhar(testTable.snapshot);
      return new Promise(ok => setTimeout(() => ok({
        vestida: Boolean(document.querySelector('#operador-quem[data-com-capa]')),
        sobACapa: ['#botao-mudo', '#botao-surdo', '#operador-sair'].filter(s => document.querySelector(s)?.closest('[data-com-capa]')),
      }), 500));
    });
    assert.ok(nucleo.vestida, 'a capa de quem usa não vestiu a identidade na barra do operador');
    for (const [nomeDaCapa, cor] of [['branca', [255, 255, 255]], ['preta', [0, 0, 0]]]) {
      const com = await medirEm('#operador-quem', cor);
      for (const [texto, antes] of Object.entries(identidadeSemCapa)) {
        if ((com[texto] ?? 0) < antes - 0.01) pioras.push(`operador ${texto} (capa ${nomeDaCapa}): ${antes} → ${com[texto]}`);
      }
    }
    assert.deepEqual(pioras, [], 'um texto da identidade de quem usa perdeu contraste sobre a capa');
    assert.deepEqual(nucleo.sobACapa, [], 'um controle do núcleo ficou sobre a capa');

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

    // 6. **O uso e o bolso** (rascunho §4 e §12; tarefa 6, Step 5b): a capa pede
    //    ao Rust o uso `"capa"`, que a reduz ao lugar dela, e o bolso da capa
    //    conta os bytes **decodificados** que ele devolve — o custo na janela é o
    //    quadro, e não o arquivo. O avatar não nomeia uso.
    const bolso = await page.evaluate(async (alvo) => {
      window.recusarCapa = false;
      const esperar = () => new Promise(ok => setTimeout(ok, 400));
      const ultimoPedido = () => testCalls.filter(c => c.cmd === 'ler_imagem_mod').at(-1)?.args ?? {};
      contribuicoesDosMods.revogar(handleCapa);
      entregues.length = 0;
      // Um arquivo grande que decodifica pouco: aceita.
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 20 * 1024 * 1024, uri: daCapa, decodificados: 1024 * 1024 };
      window.handleCapa = registrarCapa(alvo, 'volume:leve');
      desenharChamada(testTable.snapshot);
      await esperar();
      const pediuOUso = ultimoPedido().uso;
      const leveAceita = Boolean(document.querySelector(`[data-pessoa-da-capa="${alvo}"][data-com-capa]`));
      // Um arquivo pequeno que decodifica 200 MiB: recusada, e dita.
      contribuicoesDosMods.revogar(handleCapa);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa, decodificados: 200 * 1024 * 1024 };
      window.handleCapa = registrarCapa(alvo, 'volume:pesada');
      desenharChamada(testTable.snapshot);
      await esperar();
      const pesadaRecusada = !document.querySelector(`[data-pessoa-da-capa="${alvo}"][data-com-capa]`)
        && entregues.some(m => m.nome === 'midia' && m.ponto === 'pessoa.capa');
      // O avatar não nomeia uso.
      const avatar = contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.avatar', modo: 'substituir', alvo,
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:avatar' } } },
      }).handle;
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      desenhar(testTable.snapshot);
      await esperar();
      const avatarSemUso = ultimoPedido().uso;
      // Revogados os dois: a seção seguinte cita um avatar que a pessoa não tem.
      contribuicoesDosMods.revogar(avatar);
      contribuicoesDosMods.revogar(handleCapa);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      return { pediuOUso, leveAceita, pesadaRecusada, avatarSemUso: avatarSemUso ?? null };
    }, alvo);
    assert.equal(bolso.pediuOUso, 'capa', `a capa não pediu o uso ao Rust: ${JSON.stringify(bolso)}`);
    assert.ok(bolso.leveAceita, `a capa de arquivo grande e pouco decodificada foi recusada: ${JSON.stringify(bolso)}`);
    assert.ok(bolso.pesadaRecusada, `a capa de 200 MiB decodificados não foi recusada e dita: ${JSON.stringify(bolso)}`);
    assert.equal(bolso.avatarSemUso, null, `o avatar nomeou um uso: ${JSON.stringify(bolso)}`);

    // 5. **A citação `daPessoa`** (rascunho §4; tarefa 6, Step 4b; o bloqueio A
    //    do PERFIS): um cartão que cita a capa da pessoa usa a imagem já
    //    carregada — nenhum pedido de mídia novo, e os bytes contam uma vez, na
    //    contribuição da capa. Sem contribuição para a imagem citada, nada: o
    //    retrato fica na inicial, sem erro.
    const citacao = await page.evaluate(async (alvo) => {
      window.recusarCapa = false;
      contribuicoesDosMods.revogar(handleCapa);
      window.handleCapa = registrarCapa(alvo, 'volume:citada');
      desenharChamada(testTable.snapshot);
      await new Promise(ok => setTimeout(ok, 400));
      const pedidosAntes = testCalls.filter(c => c.cmd === 'ler_imagem_mod').length;
      const ditos = [];
      const registrarDeAntes = registrarNoAnfitriao;
      registrarNoAnfitriao = (...argumentos) => { ditos.push(String(argumentos[1])); return registrarDeAntes(...argumentos); };
      const cartao = contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.cartao', modo: 'substituir', alvo,
        conteudo: [{
          forma: 'caixa', chave: 'faixa', fundoDeMidia: { daPessoa: 'capa' },
          dentro: [{ forma: 'texto', chave: 't', dentro: 'cartão' }, { forma: 'retrato', chave: 'r', daPessoa: 'avatar' }],
        }],
      }).handle;
      desenhar(testTable.snapshot);
      await new Promise(ok => setTimeout(ok, 400));
      registrarNoAnfitriao = registrarDeAntes;
      const linha = document.querySelector('li.pessoa-apresentada[data-apresentada-por="teste/perfis"]');
      const comFundo = [...(linha?.querySelectorAll('.pessoa-miolo *') ?? [])].some(e => e.style.backgroundImage.includes(window.daCapa));
      const retrato = linha?.querySelector('.pessoa-miolo [data-estado-do-retrato]');
      const montagem = contribuicoesDosMods.porHandle.get(cartao)?.montadas?.get(alvo);
      const bytesNoCartao = montagem
        ? Object.entries(montagem.renderer).filter(([k, v]) => /^bytes/.test(k) && typeof v === 'number').reduce((a, [, v]) => a + v, 0)
        : null;
      contribuicoesDosMods.revogar(cartao);
      return {
        comFundo,
        pedidosNovos: testCalls.filter(c => c.cmd === 'ler_imagem_mod').length - pedidosAntes,
        bytesNoCartao,
        retrato: retrato?.dataset.estadoDoRetrato ?? null,
        recusas: ditos.filter(d => /retrato|fundo/.test(d)),
      };
    }, alvo);
    assert.ok(citacao.comFundo, `o cartão que cita a capa não a mostrou: ${JSON.stringify(citacao)}`);
    assert.equal(citacao.pedidosNovos, 0, `citar a capa abriu ${citacao.pedidosNovos} pedido(s) de mídia novo(s)`);
    assert.equal(citacao.bytesNoCartao, 0, `a capa citada contou ${citacao.bytesNoCartao} bytes de novo no cartão`);
    assert.equal(citacao.retrato, 'inicial', `o retrato que cita um avatar que não existe saiu ${citacao.retrato}`);
    assert.deepEqual(citacao.recusas, [], 'citar uma imagem que a pessoa não tem virou recusa');

    assert.deepEqual(errors, []);
    console.log('capa: o cartão pintado, o texto legível e por cima, o núcleo fora dela, o quadro estático com movimento reduzido, a recusa dita com o ponto e o alvo, e a citação daPessoa sem pedido novo.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
