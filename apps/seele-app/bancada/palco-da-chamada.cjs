// O palco dos MODs sobre a chamada (rascunho da API 6, §5; tarefa 7, Step 5 da
// F3), com o HTML, o CSS e o registro reais; só a ponte nativa é simulada. No
// molde de `tema-do-servidor.cjs`.
//
// # O que ela mede
//
// `chamada.palco` é um ponto de perfil `superficie` — a gramática inteira, com
// toque e clique — sobre a grade da chamada:
//
// 1. **O palco monta, e começa escondido**: a substituição escolhida no
//    miolo, montada atrás da grade, que continua à vista — é nela que moram as
//    capas de quem fala (D-capa-1), e um MOD não a tira da vista sem a pessoa
//    pedir. VER O PALCO o traz à frente, e a grade continua no DOM, repintada.
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
// 8. **O palco escondido continua soando** (tarefa 10, Step 4; rascunho §11,
//    pergunta da MESA): a regra «fora da tela, a declaração não liga o som»
//    vale para o lugar desmontado, e não para o palco escondido pela vista
//    alternada. Um som com `repetir: true` no palco: VER A GRADE não manda
//    `pausada` e a fonte segue; sair da sala manda, e a fonte para. Com o
//    WebAudio de verdade, como `oSomQueSaiDaTelaPara`, em
//    `diagnostico-de-mods.cjs`.
// 9. **O tamanho** (rascunho §5 e §7.2): cada MOD do palco ouve o `lugar` com
//    a largura e a altura que tem ali — a substituição, o miolo; o acréscimo,
//    a faixa — e `regiao: 'chamada.palco'`. Escondido, 0 × 0. Mudar o tamanho
//    da janela diz o novo; redesenhar sem mudar não repete.
//
// Rodar:  node apps/seele-app/bancada/palco-da-chamada.cjs

const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

/** Um WAV de 440 Hz, feito aqui: nenhum binário no repositório (o de `diagnostico-de-mods.cjs`). */
function wavDeTeste(segundos) {
  const taxa = 8000;
  const amostras = Math.round(taxa * segundos);
  const bytes = Buffer.alloc(44 + amostras * 2);
  bytes.write('RIFF', 0, 'ascii');
  bytes.writeUInt32LE(36 + amostras * 2, 4);
  bytes.write('WAVE', 8, 'ascii');
  bytes.write('fmt ', 12, 'ascii');
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(taxa, 24);
  bytes.writeUInt32LE(taxa * 2, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36, 'ascii');
  bytes.writeUInt32LE(amostras * 2, 40);
  for (let i = 0; i < amostras; i += 1) {
    bytes.writeInt16LE(Math.round(Math.sin((2 * Math.PI * 440 * i) / taxa) * 8000), 44 + i * 2);
  }
  return [...bytes];
}

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
        core: {
          invoke: async (cmd, args) => {
            const resposta = testTable[cmd];
            return typeof resposta === 'function' ? resposta(args) : (resposta ?? null);
          },
        },
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
    assert.equal(agora.palco, false, `o palco tomou a vista sem a pessoa pedir: ${JSON.stringify(agora)}`);
    assert.equal(agora.grade, true, 'o palco que chegou tirou a grade da vista');
    assert.equal(agora.verOPalco, true, 'o palco chegou, e a barra da chamada não oferece VER O PALCO');
    assert.match(agora.miolo, /o mapa da mesa/, 'o palco escondido não está montado');
    await page.click('#palco-de-mods-ver-o-palco');
    agora = await estado();
    assert.equal(agora.palco, true, `VER O PALCO não trouxe o palco: ${JSON.stringify(agora)}`);
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
    // O palco que volta depois de sair começa escondido de novo.
    assert.equal((await estado()).palco, false, 'o palco que voltou tomou a vista sem a pessoa pedir');
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

    // 8. O palco escondido continua soando; sair da sala o cala.
    await page.evaluate((bytes) => {
      window.ativas = 0;
      window.paradas = 0;
      const parar = AudioBufferSourceNode.prototype.stop;
      AudioBufferSourceNode.prototype.stop = function (...args) {
        window.paradas += 1;
        return parar.apply(this, args);
      };
      const comecar = AudioBufferSourceNode.prototype.start;
      AudioBufferSourceNode.prototype.start = function (...args) {
        window.ativas += 1;
        this.addEventListener('ended', () => { window.ativas -= 1; });
        return comecar.apply(this, args);
      };
      testTable.midia_do_mod = { papel: 'som', bytes: bytes.length };
      testTable.som_do_mod = () => new Uint8Array(bytes).buffer;
    }, wavDeTeste(1));
    await registrar('mod/dados', {
      ponto: 'chamada.palco', modo: 'adicionar',
      conteudo: [{ forma: 'midia', chave: 'trilha', fonte: 'som/trilha.wav', tocando: true, repetir: true }],
    });
    await redesenhar();
    const daTrilha = () => page.evaluate(() => eventos
      .filter((e) => e.mod === 'mod/dados' && e.nome === 'midia' && e.chave === 'trilha')
      .map((e) => e.estado));
    await page.waitForFunction(() => eventos.some((e) => e.nome === 'midia' && e.chave === 'trilha' && e.estado === 'tocando'),
      null, { timeout: 10_000 }).catch(async () => {
      assert.fail(`a trilha do palco não tocou: ${JSON.stringify(await daTrilha())}`);
    });
    const ouvidosAntes = (await daTrilha()).length;
    await page.click('#palco-de-mods-ver-o-palco');
    await page.click('#palco-de-mods-ver-a-grade');
    await redesenhar();
    // Um segundo inteiro: a fonte, sem `repetir`, já teria terminado.
    await page.waitForTimeout(1200);
    const escondido = await page.evaluate(() => ({ ativas: window.ativas, paradas: window.paradas }));
    const depoisDeEsconder = (await daTrilha()).slice(ouvidosAntes);
    assert.ok(!depoisDeEsconder.includes('pausada'),
      `VER A GRADE pausou o som do palco: ${JSON.stringify(depoisDeEsconder)} — o palco escondido não é lugar desmontado`);
    assert.ok(escondido.ativas >= 1 && escondido.paradas === 0,
      `com o palco escondido, a fonte em laço parou: ${JSON.stringify(escondido)}`);
    await page.click('#palco-de-mods-ver-o-palco');
    // Sair da sala desmonta o palco, e o som para.
    await page.evaluate(() => {
      const r = testTable.snapshot;
      testTable.snapshot = { ...r, voice_rooms: r.voice_rooms.map((s) => ({ ...s, occupied_by_us: false })) };
      desenhar(testTable.snapshot);
    });
    await page.waitForFunction(() => eventos.some((e) => e.nome === 'midia' && e.chave === 'trilha' && e.estado === 'pausada'),
      null, { timeout: 10_000 }).catch(async () => {
      assert.fail(`saiu da sala, e o MOD não ouviu «pausada»: ${JSON.stringify(await daTrilha())}`);
    });
    // A fonte parada, contada no `stop()`: a pausa suspende o áudio da janela
    // logo depois, e o `ended` de uma fonte num contexto suspenso não chega.
    assert.equal(await page.evaluate(() => window.paradas), 1, 'saiu da sala, e a fonte da trilha não foi parada');

    // 9. O tamanho do palco, a cada MOD dele.
    await page.evaluate(async () => {
      const r = testTable.snapshot;
      testTable.snapshot = { ...r, voice_rooms: r.voice_rooms.map((s) => ({ ...s, occupied_by_us: s.id === 1 })) };
      eventos.length = 0;
      await abrirChamada();
    });
    await registrar('mod/dados', {
      ponto: 'chamada.palco', modo: 'adicionar', conteudo: [{ forma: 'texto', chave: 'd', dentro: 'os dados rolados' }],
    });
    await redesenhar();
    const lugares = (mod) => page.evaluate((mod) => eventos
      .filter((e) => e.mod === mod && e.nome === 'lugar' && e.regiao === 'chamada.palco')
      .map(({ largura, altura, onde, regioes }) => ({ largura, altura, onde, temRegioes: Array.isArray(regioes) })), mod);
    const ultimo = async (mod) => (await lugares(mod)).at(-1) ?? null;
    assert.deepEqual(await ultimo('mod/mesa'), { largura: 0, altura: 0, onde: 'computador', temRegioes: true },
      'o palco escondido não disse 0 × 0 a quem substitui');
    assert.deepEqual(await ultimo('mod/dados'), { largura: 0, altura: 0, onde: 'computador', temRegioes: true },
      'o palco escondido não disse 0 × 0 a quem acrescenta');
    await page.click('#palco-de-mods-ver-o-palco');
    const medidas = () => page.evaluate(() => ({
      miolo: [$('palco-de-mods-miolo').clientWidth, $('palco-de-mods-miolo').clientHeight],
      faixa: [$('palco-de-mods-acrescimos').clientWidth, $('palco-de-mods-acrescimos').clientHeight],
    }));
    const esperarOTamanho = async (mod, [largura, altura], oQue) => {
      await page.waitForFunction(([mod, largura, altura]) => eventos.some((e) => e.mod === mod && e.nome === 'lugar'
        && e.regiao === 'chamada.palco' && e.largura === largura && e.altura === altura), [mod, largura, altura], { timeout: 5000 })
        .catch(async () => assert.fail(`${oQue}: esperava ${largura} × ${altura}, e ouviu ${JSON.stringify(await lugares(mod))}`));
    };
    let medida = await medidas();
    assert.ok(medida.miolo[0] > 0 && medida.miolo[1] > 0 && medida.faixa[1] > 0, `o palco aberto não tem tamanho: ${JSON.stringify(medida)}`);
    await esperarOTamanho('mod/mesa', medida.miolo, 'aberto o palco, quem substitui não ouviu o tamanho do miolo');
    await esperarOTamanho('mod/dados', medida.faixa, 'aberto o palco, quem acrescenta não ouviu o tamanho da faixa');
    // Redesenhar sem mudar não repete.
    const quantos = (await lugares('mod/mesa')).length;
    await redesenhar();
    await redesenhar();
    await page.waitForTimeout(200);
    assert.equal((await lugares('mod/mesa')).length, quantos, 'redesenhar sem mudar o tamanho repetiu o `lugar`');
    // A janela muda de tamanho, e o MOD sabe.
    await page.setViewportSize({ width: 1100, height: 800 });
    await page.waitForTimeout(100);
    medida = await medidas();
    await esperarOTamanho('mod/mesa', medida.miolo, 'a janela mudou de tamanho, e quem substitui não ouviu');
    // Escondido de novo, 0 × 0.
    await page.click('#palco-de-mods-ver-a-grade');
    await esperarOTamanho('mod/mesa', [0, 0], 'o palco escondido não disse 0 × 0');

    assert.deepEqual(errors, []);
    console.log('palco da chamada: monta sobre a grade, que continua no DOM; o clique chega ao MOD; a saída é do produto e não desmonta; os acréscimos têm faixa própria; revogar devolve a grade; sair da sala desmonta, mesmo com a conversa à frente; e o palco escondido continua soando, até sair da sala; e cada MOD do palco ouve o tamanho dele.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
