// O núcleo de confiança no desktop (rascunho da API 6, §10; tarefa 9 da F3),
// com o HTML, o CSS e o tema reais; só a ponte nativa é simulada. No molde de
// `capa.cjs`.
//
// # O que ela mede
//
// 1. **O tema não alcança os controles do núcleo** (Steps 1 e 2): o MUDO
//    (desligado e ligado), o fone, o SAIR, o «no ar» e o veredito têm a mesma
//    cor, o mesmo fundo efetivo e a mesma borda com e sem o tema do servidor —
//    inclusive sob o tema hostil, de acento igual ao fundo, que a API 6
//    recusa pelos pares e que esta bancada aplica por baixo dela, como a do
//    celular (`tools/bancada-do-celular.py`), para ver que o núcleo aguenta
//    mesmo assim. O fundo efetivo é o primeiro opaco atrás da peça: um MUDO
//    transparente sobre a barra mostraria o fundo do tema.
// 2. **O guarda 11** (Step 3), um por item: a portaria, o MUDO, o fone, o «no
//    ar», o SAIR, o veredito, a moderação, o aceite, e a atribuição e a saída
//    de uma superfície aberta. (a) Nenhum mora dentro do contêiner de um ponto
//    que monta nós de MOD (`CONTEINERES_DOS_PONTOS`); os dois que só pintam
//    ficam de fora, com o motivo: `servidor.aparencia` é o tema, medido no 1,
//    e `pessoa.capa` é uma imagem atrás das placas (tarefa 6). (b) Com a capa
//    de quem usa e uma contribuição em `operador.barra` montadas, nada cobre
//    os que estão à vista: no centro de cada um, quem recebe o clique é ele.
//    Numa bancada, e não em texto no `frontend.rs`: os contêineres são
//    seletores sobre o DOM, e alguns só existem desenhados.
// 3. **O modal de MOD não cobre a barra do operador** (Step 4b; decisão da
//    orquestradora, 09/10: o recorte). A 1400 e a 900 de largura, com um
//    diálogo modal alto e largo aberto: o MUDO, o fone e o SAIR ficam
//    acordados e no furo do véu — a 900, com a gaveta fechada, a porta dela, e
//    aberta, o MUDO —; o resto da sessão continua inerte e coberto; o Tab fica
//    no diálogo, e o Esc o fecha; e o diálogo nunca cruza a barra, rolando por
//    dentro.
//
// Rodar:  node apps/seele-app/bancada/nucleo.cjs

const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

// Os temas de teste da bancada do celular (F8): PAPEL e o claro passam pelos
// pares; o hostil, não, e é aplicado por baixo da validação.
const TEMAS = {
  papel: { fundo: '#12100e', painel: '#1b1815', texto: '#f2ece0', apagado: '#a2988a', acento: '#c9a227', borda: '#4a423a' },
  claro: { fundo: '#f2ece0', painel: '#e6dfcf', texto: '#12100e', apagado: '#574e43', acento: '#a83a10', borda: '#b8ad9a' },
  hostil: { fundo: '#050403', painel: '#0a0806', texto: '#eae3cf', apagado: '#908574', acento: '#050403', borda: '#241f19' },
};

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
    await page.evaluate(r => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      // O veredito de um primeiro contato, para a faixa estar à vista.
      mostrarVeredito({ FirstContact: { fingerprint: 'SHA256:aa:bb:cc' } });
    }, retrato());

    // As peças do núcleo, e a cor de cada uma: o texto, a borda e o fundo
    // efetivo. O MUDO é medido desligado e ligado.
    const medir = () => page.evaluate(() => {
      const fundoEfetivo = (el) => {
        for (let n = el; n; n = n.parentElement) {
          const c = getComputedStyle(n).backgroundColor;
          if (!/rgba\(0, 0, 0, 0\)|transparent/.test(c)) return c;
        }
        return 'nenhum';
      };
      const peca = (el) => {
        if (!el || el.closest('[hidden]') || !el.getClientRects().length) return null;
        const s = getComputedStyle(el);
        return { cor: s.color, borda: s.borderTopColor, fundo: fundoEfetivo(el) };
      };
      const mudo = document.getElementById('botao-mudo');
      const saida = {
        mudo: peca(mudo),
        fone: peca(document.getElementById('botao-surdo')),
        sair: peca(document.getElementById('operador-sair')),
        noAr: peca(document.getElementById('voz-estado')),
        veredito: peca(document.getElementById('veredito')),
      };
      const antes = mudo.dataset.ativo;
      mudo.dataset.ativo = 'sim';
      saida.mudoLigado = peca(mudo);
      if (antes === undefined) delete mudo.dataset.ativo; else mudo.dataset.ativo = antes;
      return saida;
    });

    const semTema = await medir();
    for (const [peca, medida] of Object.entries(semTema)) {
      assert.ok(medida, `a peça do núcleo «${peca}» não está à vista, e o caso não a mede`);
    }
    const diferencas = [];
    for (const [nome, valores] of Object.entries(TEMAS)) {
      const comTema = await page.evaluate((valores) => {
        temaDosMods.set('bancada', valores);
        escreverOTemaDaSessao();
      }, valores).then(medir);
      for (const [peca, medida] of Object.entries(semTema)) {
        for (const [parte, cor] of Object.entries(medida)) {
          if (comTema[peca]?.[parte] !== cor) diferencas.push(`${nome}: ${peca}.${parte} ${cor} → ${comTema[peca]?.[parte]}`);
        }
      }
      await page.evaluate(() => { temaDosMods.delete('bancada'); escreverOTemaDaSessao(); });
    }
    assert.deepEqual(diferencas, [], 'o tema alcançou um controle do núcleo');

    // 2. O guarda 11.
    const guarda = await page.evaluate(async () => {
      // A capa de quem usa e uma contribuição alta na barra do operador, e uma
      // superfície aberta, por um MOD de API 6 de pé.
      entrarNaGeracao(1);
      const mod = { id: 'teste/nucleo', hash: 'h', api: 6 };
      const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, { pedirEncerramento() {}, async encerrou() { return true; }, entregar() {} });
      inst.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set(mod.id, inst);
      const tela = document.createElement('canvas');
      tela.width = 64; tela.height = 32;
      tela.getContext('2d').fillRect(0, 0, 64, 32);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: tela.toDataURL() };
      contribuicoesDosMods.registrar(mod, inst, { ponto: 'pessoa.capa', modo: 'substituir', alvo: String(testTable.snapshot.me),
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:capa' } } } });
      const linha = { forma: 'texto', chave: 't', dentro: 'uma linha do MOD na barra, longa o bastante para empurrar o que vier depois' };
      const primeira = contribuicoesDosMods.registrar(mod, inst, { ponto: 'operador.barra', modo: 'adicionar', conteudo: [linha] });
      await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 1, descricao: { id: 's', tipo: 'dialogo', modal: false, titulo: 'Superfície' } });
      desenhar(testTable.snapshot);
      await new Promise((ok) => setTimeout(ok, 300));
      // **O desenho do MOD mira cada item** (o molde de `regioes-do-produto.cjs`):
      // `estilo.mover` vai a ±512 px, e um nó vai do canto da região até o
      // centro de cada item à vista.
      const canto = document.getElementById('operador-mods').getBoundingClientRect();
      const limite = (n) => Math.max(-512, Math.min(512, Math.round(n)));
      const mirados = ['#botao-mudo', '#botao-surdo', '#voz-estado', '#operador-sair', '#veredito'].map((seletor, i) => {
        const centro = document.querySelector(seletor).getBoundingClientRect();
        return { forma: 'texto', chave: `mirado${i}`, dentro: '█'.repeat(6), estilo: { mover: {
          x: limite(centro.left + centro.width / 2 - canto.left - 8), y: limite(centro.top + centro.height / 2 - canto.top - 8) } } };
      });
      contribuicoesDosMods.revogar(primeira.handle, { id: mod.id, instancia: inst });
      contribuicoesDosMods.registrar(mod, inst, { ponto: 'operador.barra', modo: 'adicionar', conteudo: [...mirados, linha] });
      desenhar(testTable.snapshot);
      await new Promise((ok) => setTimeout(ok, 300));

      // Os que só pintam: o tema e a capa (ver o cabeçalho).
      const SO_PINTAM = new Set(['servidor.aparencia', 'pessoa.capa']);
      const conteineres = Object.entries(CONTEINERES_DOS_PONTOS).filter(([ponto]) => !SO_PINTAM.has(ponto));
      const ITENS = {
        portaria: '#portaria-batendo', mudo: '#botao-mudo', fone: '#botao-surdo', noAr: '#voz-estado',
        sair: '#operador-sair', veredito: '#veredito', moderacao: '#moderar', aceite: '#mods-aceite',
        atribuicao: '[data-superficie="s"] .superficie-de-mod-origem', saidaDaSuperficie: '[data-superficie="s"] .superficie-de-mod-sair',
      };
      const dentro = [];
      const cobertos = [];
      const ausentes = [];
      for (const [item, seletor] of Object.entries(ITENS)) {
        const el = document.querySelector(seletor);
        if (!el) { ausentes.push(item); continue; }
        for (const [ponto, conteiner] of conteineres) {
          if (el.matches(conteiner) || el.closest(conteiner)) dentro.push(`${item} dentro de ${ponto} (${conteiner})`);
        }
        if (el.closest('[hidden]') || !el.getClientRects().length) continue;
        const r = el.getBoundingClientRect();
        const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        if (!alvo || !(alvo === el || el.contains(alvo))) cobertos.push(`${item} coberto por <${alvo?.tagName?.toLowerCase()}${alvo?.id ? `#${alvo.id}` : ''}${alvo?.className ? `.${String(alvo.className).split(' ')[0]}` : ''}>`);
      }
      return {
        dentro, cobertos, ausentes,
        comCapa: Boolean(document.querySelector('#tela-sessao .operador[data-com-capa]')),
        barraComMod: Boolean(document.querySelector('#operador-mods')?.textContent.includes('uma linha do MOD')),
      };
    });
    assert.ok(guarda.comCapa && guarda.barraComMod, `o guarda não mede o que diz: a capa ou a barra do MOD não montou: ${JSON.stringify(guarda)}`);
    assert.deepEqual(guarda.ausentes, [], 'um item do núcleo sumiu do DOM, e o guarda não o mede');
    assert.deepEqual(guarda.dentro, [], 'um item do núcleo mora dentro do contêiner de um ponto de contribuição');
    assert.deepEqual(guarda.cobertos, [], 'uma contribuição cobre um item do núcleo');

    // 3. O modal não cobre a barra.
    const abrirModal = () => page.evaluate(async () => {
      const mod = { id: 'teste/nucleo', hash: 'h', api: 6 };
      const inst = modsCarregados.get(mod.id);
      superficiesDoMod(mod, inst).soltarTudo();
      await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 2, descricao: { id: 'alto', tipo: 'dialogo', titulo: 'Alto e largo', tamanho: { largura: 1600 } } });
      await atenderOMod(mod, inst, { tipo: 'superficie-montar', n: 3, superficie: 'alto', arvore: [
        ...Array.from({ length: 80 }, (_, i) => ({ forma: 'texto', chave: `l${i}`, dentro: `linha ${i} do diálogo alto` })),
        { forma: 'campo', chave: 'a', rotulo: 'A' }, { forma: 'campo', chave: 'b', rotulo: 'B' },
      ] });
      await new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
    });
    const medirOModal = () => page.evaluate(() => {
      const alcancado = (el) => {
        if (!el || !el.getClientRects().length) return 'invisivel';
        if (el.closest('[inert]')) return 'inerte';
        const r = el.getBoundingClientRect();
        const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return alvo && (alvo === el || el.contains(alvo)) ? 'sim' : `coberto por ${alvo?.className ?? alvo?.tagName}`;
      };
      const cruza = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
      const caixa = document.querySelector('[data-superficie="alto"].dialogo-de-mod, [data-superficie="alto"] .dialogo-de-mod, .dialogo-de-mod[data-superficie="alto"]');
      const barra = document.querySelector('#tela-sessao .operador');
      const porta = document.querySelector('#tela-sessao .linha-canais');
      const corpo = caixa?.querySelector('.dialogo-de-mod-corpo');
      return {
        mudo: alcancado(document.getElementById('botao-mudo')),
        fone: alcancado(document.getElementById('botao-surdo')),
        sair: alcancado(document.getElementById('operador-sair')),
        porta: alcancado(porta),
        resto: alcancado(document.querySelector('#tela-sessao textarea')),
        // **Coberto, e não só inerte**: um nó inerte não entra no teste de
        // clique, então quem recebe o clique no centro dele precisa ser o
        // modal — o véu ou a caixa —, e não o que estiver por baixo.
        restoCoberto: ['#tela-sessao textarea', '#lista-linhas'].map((seletor) => {
          const el = document.querySelector(seletor);
          if (!el || !el.getClientRects().length) return null;
          const r = el.getBoundingClientRect();
          return Boolean(document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(r.height / 2, 20))?.closest('.camada-de-mod'));
        }).filter((v) => v !== null),
        cruzaBarra: Boolean(caixa && barra?.getClientRects().length && cruza(caixa.getBoundingClientRect(), barra.getBoundingClientRect())),
        cruzaPorta: Boolean(caixa && porta?.getClientRects().length && cruza(caixa.getBoundingClientRect(), porta.getBoundingClientRect())),
        rola: Boolean(corpo && corpo.scrollHeight > corpo.clientHeight),
        dentroDaJanela: Boolean(caixa && caixa.getBoundingClientRect().bottom <= innerHeight && caixa.getBoundingClientRect().right <= innerWidth),
      };
    });

    await abrirModal();
    const largo = await medirOModal();
    assert.deepEqual({ mudo: largo.mudo, fone: largo.fone, sair: largo.sair }, { mudo: 'sim', fone: 'sim', sair: 'sim' },
      `a 1400, com o modal aberto, a barra do operador não está alcançável: ${JSON.stringify(largo)}`);
    assert.equal(largo.resto, 'inerte', `a 1400, o resto da sessão escapou do modal: ${JSON.stringify(largo)}`);
    assert.ok(largo.restoCoberto.length >= 2 && largo.restoCoberto.every(Boolean), `a 1400, o resto da sessão não ficou sob o modal: ${JSON.stringify(largo)}`);
    assert.deepEqual({ cruzaBarra: largo.cruzaBarra, rola: largo.rola, dentroDaJanela: largo.dentroDaJanela }, { cruzaBarra: false, rola: true, dentroDaJanela: true },
      `a 1400, o diálogo alto cruza a barra, não rola por dentro, ou sai da janela: ${JSON.stringify(largo)}`);
    // O Tab fica no diálogo, e o Esc o fecha.
    const dentroDoAlto = () => page.evaluate(() => Boolean(document.activeElement?.closest('[data-superficie="alto"]')));
    const tabs = [];
    for (let i = 0; i < 6; i += 1) {
      await page.keyboard.press('Tab');
      tabs.push(await dentroDoAlto());
    }
    assert.deepEqual(tabs, [true, true, true, true, true, true], 'um Tab levou o foco para fora do diálogo modal');
    await page.keyboard.press('Escape');
    assert.equal(await page.evaluate(() => Boolean(document.querySelector('[data-superficie="alto"]')?.isConnected)), false, 'o Esc não fechou o diálogo modal');

    // A 900, a coluna é gaveta: fechada, a porta dela fica alcançável; aberta,
    // o MUDO.
    await page.setViewportSize({ width: 900, height: 700 });
    await abrirModal();
    const estreito = await medirOModal();
    assert.equal(estreito.porta, 'sim', `a 900, com o modal aberto, a porta da gaveta não está alcançável: ${JSON.stringify(estreito)}`);
    assert.equal(estreito.resto, 'inerte', `a 900, o resto da sessão escapou do modal: ${JSON.stringify(estreito)}`);
    assert.ok(estreito.restoCoberto.length >= 1 && estreito.restoCoberto.every(Boolean), `a 900, o resto da sessão não ficou sob o modal: ${JSON.stringify(estreito)}`);
    assert.deepEqual({ cruzaPorta: estreito.cruzaPorta, rola: estreito.rola, dentroDaJanela: estreito.dentroDaJanela }, { cruzaPorta: false, rola: true, dentroDaJanela: true },
      `a 900, o diálogo cruza a porta da gaveta, não rola, ou sai da janela: ${JSON.stringify(estreito)}`);
    await page.click('#tela-sessao .linha-canais');
    await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const gavetaAberta = await medirOModal();
    assert.deepEqual({ mudo: gavetaAberta.mudo, sair: gavetaAberta.sair }, { mudo: 'sim', sair: 'sim' },
      `a 900, com a gaveta aberta e o modal aberto, o MUDO ou o SAIR não está alcançável: ${JSON.stringify(gavetaAberta)}`);
    assert.equal(gavetaAberta.cruzaBarra, false, `a 900, o diálogo cruza a barra da gaveta aberta: ${JSON.stringify(gavetaAberta)}`);
    assert.ok(gavetaAberta.restoCoberto.length >= 2 && gavetaAberta.restoCoberto.every(Boolean), `a 900, a gaveta aberta escapou do modal fora da barra: ${JSON.stringify(gavetaAberta)}`);
    await page.setViewportSize({ width: 1400, height: 900 });

    assert.deepEqual(errors, []);
    console.log('núcleo: o MUDO, o fone, o SAIR, o «no ar» e o veredito têm as cores do produto sob o papel, o claro e o hostil; e o guarda 11 vale para os dez itens, com a capa e a barra do MOD montadas; e o modal de MOD não cobre a barra, a 1400 e a 900, e continua modal no resto.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
