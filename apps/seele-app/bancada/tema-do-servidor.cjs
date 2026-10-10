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
// 5. **A escolha de quem vê e o evento `tema`** (tarefa 17f; rascunho §9, B2):
//    «o SEELE desenha» para a aparência tira o tema daquele servidor nesta
//    máquina, e o MOD que o declarou recebe `tema` com `estado: 'nativo'`;
//    voltar ao automático o devolve, com `aplicado`; um tema recusado manda
//    `recusado` com os pares (`frente`, `fundo`, `razao`, `piso`).
// 6. **A janela que sobe depois do tema também sabe**: o `TemaMudou` chega
//    logo depois do `Session`, antes de a janela do MOD estar de pé; o
//    desfecho fica guardado, e o `montarOMod` de verdade o entrega quando o
//    código do MOD termina de rodar.
// 7. **A prévia** (tarefa 17g; rascunho §9, B4): na 6, `SeeleUI.tema` sem
//    `previa` é recusado pelo nome; com `{ previa: true, superficie }`, ela
//    vale só nesta máquina, por cima do tema publicado, com o selo «prévia não
//    publicada», e o evento `tema` volta com `previa: true`; `null` a tira sem
//    fechar a superfície, e fechar a superfície a tira; uma prévia sem
//    contraste é recusada com os pares, e não fica.
// 8. **A herança e a citação** (tarefa 17h; rascunho §9, B8): a `folha` e o
//    `dialogo` pintam com o tema em vigor, e um estilo que cita um token —
//    `cor: { tema: 'acento' }` — tem a cor do tema, e muda quando ele muda; um
//    nome que não é do tema é recusado como qualquer cor que não serve.
// 9. **A linha da aparência em QUEM DESENHA O QUE** (tarefa 17f): na API 6 o
//    tema é do servidor e a janela não registra o ponto, então a linha não sai
//    do registro; ela diz quem pinta, oferece «o SEELE desenha» e a volta, e a
//    troca vale **na hora**, sem outro retrato. O recusado é dito, e o
//    DIAGNÓSTICO chama o tema do servidor pelo nome, e não de tema da API 3.
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

    // 5. A escolha de quem vê e o evento `tema`, à janela do MOD que o
    //    declarou.
    await page.evaluate(() => {
      entrarNaGeracao(1);
      window.temas = [];
      const inst = new InstanciaDeMod('seele/estilo', 'h', geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; },
        entregar(m) { if (m.tipo === 'evento' && m.nome === 'tema') temas.push(JSON.parse(JSON.stringify(m))); },
      });
      inst.estado = ESTADOS_DE_MOD.ativa;
      inst.api = 6;
      modsCarregados.set('seele/estilo', inst);
    });
    const ultimoTema = () => page.evaluate(() => temas.at(-1) ?? null);
    await desenharCom(null);
    await desenharCom(comoVeio);
    assert.deepEqual(await ultimoTema(), { tipo: 'evento', nome: 'tema', estado: 'aplicado', previa: false }, 'o MOD não soube que o tema foi aplicado');
    await page.evaluate(() => escolherApresentacao('servidor.aparencia', NATIVO));
    await desenharCom(comoVeio);
    assert.deepEqual(await tokens(), {}, '«o SEELE desenha» não tirou o tema do servidor desta máquina');
    assert.deepEqual(await ultimoTema(), { tipo: 'evento', nome: 'tema', estado: 'nativo', previa: false }, 'o MOD não soube que quem vê escolheu o SEELE');
    await page.evaluate(() => escolherApresentacao('servidor.aparencia', ''));
    await desenharCom(comoVeio);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'voltar ao automático não devolveu o tema');
    assert.equal((await ultimoTema())?.estado, 'aplicado', 'o MOD não soube que o tema voltou');
    await desenharCom({ mod_id: 'seele/estilo', valores: JSON.stringify({ ...CLARO, acento: CLARO.fundo }) });
    const recusado = await ultimoTema();
    assert.equal(recusado?.estado, 'recusado', `o MOD não soube que o tema foi recusado: ${JSON.stringify(recusado)}`);
    assert.ok(recusado.pares?.length && recusado.pares.every((p) => p.frente && p.fundo && p.razao > 0 && p.piso > 0)
      && recusado.pares.some((p) => p.frente === 'acento' && p.fundo === 'fundo'),
    `a recusa não trouxe os pares: ${JSON.stringify(recusado)}`);
    await desenharCom(comoVeio);

    // 6. A janela que sobe depois: sem instância, o tema chega; depois o MOD
    //    sobe pelo `montarOMod` de verdade, com um executor falso, e recebe o
    //    desfecho guardado.
    const tardio = await page.evaluate(async (valores) => {
      modsCarregados.delete('seele/estilo');
      testTable.snapshot = { ...testTable.snapshot, tema_do_servidor: null };
      desenhar(testTable.snapshot);
      testTable.snapshot = { ...testTable.snapshot, tema_do_servidor: { mod_id: 'seele/estilo', valores } };
      desenhar(testTable.snapshot);
      const recebidos = [];
      const executorDeAntes = window.executorDoMod;
      window.executorDoMod = () => ({
        nome: 'falso',
        async iniciar() {},
        entregar(m) { recebidos.push(JSON.parse(JSON.stringify(m))); },
        pedirEncerramento() {},
        async encerrou() { return true; },
      });
      testTable.codigo_do_mod = 'seele.postar("{}")';
      modsCarregados.set('seele/estilo', { geracao: geracaoDaSessao, admite: () => true, encerrar: async () => {} });
      try {
        await montarOMod({ id: 'seele/estilo', hash: 'h', api: 6 });
      } finally {
        window.executorDoMod = executorDeAntes;
      }
      return recebidos.filter((m) => m.tipo === 'evento' && m.nome === 'tema');
    }, JSON.stringify(CLARO));
    assert.deepEqual(tardio, [{ tipo: 'evento', nome: 'tema', estado: 'aplicado', previa: false }],
      `a janela do MOD que subiu depois do tema não soube dele: ${JSON.stringify(tardio)}`);

    // 7. A prévia.
    const PAPEL = { fundo: '#12100e', painel: '#1b1815', texto: '#f2ece0', apagado: '#a2988a', acento: '#c9a227', borda: '#4a423a' };
    await page.evaluate(async () => {
      window.eventosDoTema = [];
      window.respostasDoMod = [];
      const inst = new InstanciaDeMod('seele/estilo', 'h', geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; },
        entregar(m) {
          const copia = JSON.parse(JSON.stringify(m));
          if (copia.tipo === 'evento' && copia.nome === 'tema') eventosDoTema.push(copia);
          if (copia.tipo === 'resposta') respostasDoMod.push(copia);
        },
      });
      inst.estado = ESTADOS_DE_MOD.ativa;
      inst.api = 6;
      modsCarregados.set('seele/estilo', inst);
      let n = 100;
      window.pedirComoOMod = async (m) => {
        const numero = ++n;
        await atenderOMod({ id: 'seele/estilo', hash: 'h', api: 6 }, modsCarregados.get('seele/estilo'), { ...m, n: numero });
        return respostasDoMod.find((r) => r.n === numero) ?? null;
      };
      await pedirComoOMod({ tipo: 'superficie-criar', descricao: { id: 'editor', tipo: 'dialogo', modal: false, titulo: 'Editor de tema' } });
    });
    const selo = () => page.evaluate(() => {
      const el = document.getElementById('selo-da-previa');
      return Boolean(el && !el.hidden && el.getClientRects().length);
    });
    const ultimoEvento = () => page.evaluate(() => eventosDoTema.at(-1) ?? null);
    const pedir = (m) => page.evaluate((m) => pedirComoOMod(m), m);

    const semPrevia = await pedir({ tipo: 'tema', valores: PAPEL });
    assert.equal(semPrevia?.erro, 'sem-previa', `na 6, o tema sem prévia não foi recusado pelo nome: ${JSON.stringify(semPrevia)}`);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'o tema sem prévia mexeu no tema publicado');

    const comPrevia = await pedir({ tipo: 'tema', valores: PAPEL, previa: true, superficie: 'editor' });
    assert.equal(comPrevia?.ok, true, `a prévia foi recusada: ${JSON.stringify(comPrevia)}`);
    assert.equal((await tokens()).fundo, PAPEL.fundo, 'a prévia não pintou a sessão por cima do tema publicado');
    assert.equal(await selo(), true, 'a prévia não mostrou o selo «prévia não publicada»');
    assert.deepEqual(await ultimoEvento(), { tipo: 'evento', nome: 'tema', estado: 'aplicado', previa: true }, 'o MOD não soube que a prévia foi aplicada');

    const semSuperficie = await pedir({ tipo: 'tema', valores: PAPEL, previa: true, superficie: 'nao-existe' });
    assert.equal(semSuperficie?.erro, 'superficie-desconhecida', `a prévia de uma superfície que não existe não foi recusada pelo nome: ${JSON.stringify(semSuperficie)}`);

    const tirada = await pedir({ tipo: 'tema', valores: null, previa: true });
    assert.equal(tirada?.ok, true, `tirar a prévia foi recusado: ${JSON.stringify(tirada)}`);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'tirar a prévia não devolveu o tema publicado');
    assert.equal(await selo(), false, 'o selo ficou depois de tirar a prévia');
    assert.equal(await page.evaluate(() => Boolean(document.querySelector('[data-superficie="editor"]')?.isConnected)), true, 'tirar a prévia fechou a superfície');

    await pedir({ tipo: 'tema', valores: PAPEL, previa: true, superficie: 'editor' });
    await pedir({ tipo: 'superficie-fechar', superficie: 'editor' });
    assert.equal((await tokens()).fundo, CLARO.fundo, 'fechar a superfície não tirou a prévia');
    assert.equal(await selo(), false, 'o selo ficou depois de fechar a superfície da prévia');

    await pedir({ tipo: 'superficie-mostrar', superficie: 'editor' });
    const semContraste = await pedir({ tipo: 'tema', valores: { ...PAPEL, acento: PAPEL.fundo }, previa: true, superficie: 'editor' });
    assert.equal(semContraste?.erro, 'contraste', `a prévia sem contraste não foi recusada pelo nome: ${JSON.stringify(semContraste)}`);
    assert.ok(semContraste.pares?.some((p) => p.frente === 'acento' && p.fundo === 'fundo'), `a recusa da prévia não trouxe os pares: ${JSON.stringify(semContraste)}`);
    const eventoRecusado = await ultimoEvento();
    assert.ok(eventoRecusado?.estado === 'recusado' && eventoRecusado.previa === true && eventoRecusado.pares?.length,
      `o evento da prévia recusada não veio com os pares: ${JSON.stringify(eventoRecusado)}`);
    assert.equal((await tokens()).fundo, CLARO.fundo, 'a prévia recusada pintou a sessão');
    assert.equal(await selo(), false, 'a prévia recusada mostrou o selo');

    // 8. A herança e a citação.
    const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
    await page.evaluate(async () => {
      await pedirComoOMod({ tipo: 'superficie-criar', descricao: { id: 'citacao', tipo: 'dialogo', modal: false, titulo: 'Citação' } });
      await pedirComoOMod({ tipo: 'superficie-montar', superficie: 'citacao', arvore: [
        { forma: 'texto', chave: 'citado', dentro: 'no acento do tema', estilo: { cor: { tema: 'acento' }, fundo: { tema: 'painel' } } },
        { forma: 'texto', chave: 'sem-tema', dentro: 'um nome que não é do tema', estilo: { cor: { tema: 'nao-existe' } } },
      ] });
      await pedirComoOMod({ tipo: 'superficie-criar', descricao: { id: 'lateral', tipo: 'folha', titulo: 'Folha' } });
    });
    const cores = () => page.evaluate(() => {
      const no = (chave) => document.querySelector(`[data-superficie="citacao"] [data-chave-do-mod="${chave}"]`);
      const caixa = (seletor) => getComputedStyle(document.querySelector(seletor)).backgroundColor;
      return {
        citado: getComputedStyle(no('citado')).color,
        citadoFundo: getComputedStyle(no('citado')).backgroundColor,
        semTema: no('sem-tema')?.style.color ?? null,
        dialogo: caixa('.dialogo-de-mod[data-superficie="citacao"]'),
        folha: caixa('.folha-de-mod[data-superficie="lateral"]'),
      };
    });
    await desenharCom(comoVeio);
    const comClaro = await cores();
    assert.deepEqual(
      { citado: comClaro.citado, citadoFundo: comClaro.citadoFundo, dialogo: comClaro.dialogo, folha: comClaro.folha },
      { citado: rgb(CLARO.acento), citadoFundo: rgb(CLARO.painel), dialogo: rgb(CLARO.painel), folha: rgb(CLARO.painel) },
      `sob o tema claro, a citação ou a herança não tem a cor do tema: ${JSON.stringify(comClaro)}`,
    );
    assert.equal(comClaro.semTema, '', `um nome que não é do tema pintou o nó: ${JSON.stringify(comClaro)}`);
    await desenharCom({ mod_id: 'seele/estilo', valores: JSON.stringify(PAPEL) });
    const comPapel = await cores();
    assert.deepEqual(
      { citado: comPapel.citado, dialogo: comPapel.dialogo, folha: comPapel.folha },
      { citado: rgb(PAPEL.acento), dialogo: rgb(PAPEL.painel), folha: rgb(PAPEL.painel) },
      `trocado o tema, a citação ou a herança não acompanhou: ${JSON.stringify(comPapel)}`,
    );
    await desenharCom(comoVeio);

    // 9. QUEM DESENHA O QUE tem a linha da aparência com o tema do servidor, a
    //    troca reaplica na hora, e o DIAGNÓSTICO o chama pelo nome.
    const aparencia = () => page.evaluate(() => {
      desenharApresentacoes();
      const item = $('lista-apresentacoes').querySelector('li[data-ponto="servidor.aparencia"]');
      return {
        secao: !$('mods-apresentacao').hidden,
        texto: item?.querySelector('.mods-linha-texto')?.textContent ?? null,
        botoes: item ? [...item.querySelectorAll('button')].map((b) => b.textContent) : [],
      };
    });
    const apertar = (rotulo) => page.evaluate((rotulo) => {
      [...$('lista-apresentacoes').querySelectorAll('li[data-ponto="servidor.aparencia"] button')]
        .find((b) => b.textContent === rotulo).click();
    }, rotulo);
    // A instância de pé desde a seção 7 guarda os eventos em `eventosDoTema`.
    const diagnostico = () => page.evaluate(() =>
      frasesDeQuemPinta(quemPintaCadaPonto().find((l) => l.ponto === 'servidor.aparencia')));
    await desenharCom(comoVeio);
    let linhaDaAparencia = await aparencia();
    assert.ok(linhaDaAparencia.secao && linhaDaAparencia.texto,
      `QUEM DESENHA O QUE não tem a linha da aparência com o tema do servidor: ${JSON.stringify(linhaDaAparencia)}`);
    assert.match(linhaDaAparencia.texto, /pintada pelo tema deste servidor, de seele\/estilo/);
    assert.deepEqual(linhaDaAparencia.botoes, ['USAR APRESENTAÇÃO DO SEELE']);
    const frasesComOTema = await diagnostico();
    assert.ok(frasesComOTema.some((f) => /tema deste servidor, de seele\/estilo/.test(f)),
      `o DIAGNÓSTICO não diz que o tema do servidor pinta a aparência: ${JSON.stringify(frasesComOTema)}`);
    assert.ok(!frasesComOTema.some((f) => /API 3/.test(f)),
      `o DIAGNÓSTICO chama o tema do servidor de tema da API 3: ${JSON.stringify(frasesComOTema)}`);
    assert.ok(!frasesComOTema.some((f) => /tema pedido por/.test(f)),
      `o DIAGNÓSTICO conta o tema do servidor de novo como pedido do caminho antigo: ${JSON.stringify(frasesComOTema)}`);
    // A troca pelo botão vale **na hora**, sem outro retrato.
    await apertar('USAR APRESENTAÇÃO DO SEELE');
    assert.deepEqual(await tokens(), {}, '«o SEELE desenha» só tirou o tema do servidor no retrato seguinte');
    assert.equal((await ultimoEvento())?.estado, 'nativo', 'o MOD não soube, na hora, que quem vê escolheu o SEELE');
    linhaDaAparencia = await aparencia();
    assert.match(linhaDaAparencia.texto, /o SEELE desenha, por escolha desta máquina/);
    assert.deepEqual(linhaDaAparencia.botoes, ['USAR O TEMA DO SERVIDOR']);
    await apertar('USAR O TEMA DO SERVIDOR');
    assert.equal((await tokens()).fundo, CLARO.fundo, 'voltar ao tema do servidor só valeu no retrato seguinte');
    assert.equal((await ultimoEvento())?.estado, 'aplicado', 'o MOD não soube, na hora, que o tema voltou');
    // O recusado é dito, e não dado como aplicado.
    await desenharCom({ mod_id: 'seele/estilo', valores: JSON.stringify({ ...CLARO, acento: CLARO.fundo }) });
    linhaDaAparencia = await aparencia();
    assert.match(linhaDaAparencia.texto, /tema deste servidor, de seele\/estilo, foi recusado nesta máquina/,
      `um tema recusado aparece como se pintasse: ${linhaDaAparencia.texto}`);
    // Sem tema do servidor, sem a linha.
    await desenharCom(null);
    assert.equal((await aparencia()).texto, null, 'sem tema do servidor, a linha da aparência ficou');
    await desenharCom(comoVeio);

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
    console.log('tema do servidor: chega pelo retrato e é aplicado, o None o tira, o recusado não fica e vai ao seele.log, e sai com a sessão; «o SEELE desenha» o tira desta máquina, e o MOD sabe de cada desfecho; e a prévia vale só nesta máquina, com o selo, e sai com a superfície; e a folha, o diálogo e a citação seguem o tema; e QUEM DESENHA O QUE tem a linha da aparência, que troca na hora.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
