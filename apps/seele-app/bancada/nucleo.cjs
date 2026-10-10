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

    assert.deepEqual(errors, []);
    console.log('núcleo: o MUDO, o fone, o SAIR, o «no ar» e o veredito têm as cores do produto sob o papel, o claro e o hostil.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
