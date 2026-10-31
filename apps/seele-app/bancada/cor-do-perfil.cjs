// A cor do perfil no lugar da capa (0.19, pedido do dono: «tem que ter a cor do
// banner escolhido como fundo, caso não tenha imagem»), com o HTML, o CSS e o
// desenho reais; só a ponte nativa é simulada. No molde de `capa.cjs`.
//
// # O que ela mede
//
// 1. Quem tem perfil oficial **sem capa** ganha a camada de cor
//    (`data-capa-de-cor`) na linha de pessoa, na barra do operador e no cartão
//    da chamada; quem tem capa, não; quem não tem perfil, nada.
// 2. **O texto continua legível**: para cada uma das oito amostras do editor,
//    todo texto do produto sobre a camada tem contraste de 4,5:1 ou mais contra
//    o fundo efetivo no **pior ponto** do degradê (o canto da cor mais forte),
//    debaixo do véu mais fraco da superfície — ou, quando o texto já tinha
//    menos que isso sem cor nenhuma, pelo menos o que tinha.
//
// Rodar:  node apps/seele-app/bancada/cor-do-perfil.cjs   (SEELE_PREVIEW_DIR grava as telas)

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas, retrato } = require('./telas.cjs');
const engines = require('./playwright.cjs');

const AMOSTRAS = ['#f2521f', '#f47732', '#ffd166', '#6bffb6', '#4fd1c5', '#2f8fe0', '#b48cff', '#eae3cf'];

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await engines[process.env.SEELE_BROWSER || 'chromium']().launch({ headless: true });
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
      r.perfis_revision = 1;
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
    }, retrato());

    /** Desenha com a lista de perfis dada (`semPerfil` fica de fora). */
    const desenharCom = (cor, semPerfil) => page.evaluate(async ([cor, semPerfil]) => {
      const r = testTable.snapshot;
      const ids = new Set([r.me, ...r.presentes.map(p => p.id), ...r.voice_rooms.flatMap(s => s.people.map(p => p.id))]);
      testTable.perfis = [...ids].filter(id => id !== semPerfil).map(id => ({ pessoa: id, revisao: 1, nome_exibido: `Pessoa ${id}`,
        pronomes: 'ela/dela', status: 'no ar', bio: '', cor, efeito: 'Nenhum', foto: null, capa: null }));
      r.perfis_revision += 1;
      desenhar(r);
      await new Promise(ok => setTimeout(ok, 80));
      desenhar(r);
      desenharChamada(r);
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
    }, [cor, semPerfil]);

    // O contraste de cada texto do produto dentro de um quadro com a camada.
    const medir = (seletor, comCor) => page.evaluate(([seletor, comCor]) => {
      const rgba = (texto) => {
        const saida = [];
        for (const m of texto.matchAll(/(rgba?|color)\(([^)]*)\)/g)) {
          const n = (m[2].match(/[\d.]+/g) || []).map(Number);
          if (m[1] === 'color') saida.push([n[0] * 255, n[1] * 255, n[2] * 255, n[3] ?? 1]);
          else saida.push([n[0], n[1], n[2], n[3] ?? 1]);
        }
        return saida;
      };
      const lum = ([r, g, b]) => [r, g, b]
        .map((c) => { const v = c / 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; })
        .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
      const sobre = (de, atras) => atras.map((v, i) => de[3] * de[i] + (1 - de[3]) * v);
      const saida = {};
      for (const quadro of document.querySelectorAll(seletor)) {
        const camada = quadro.querySelector(':scope > .capa-camada');
        // O pior ponto da camada de cor: a cor mais forte do degradê (a mais
        // clara), sob o véu mais fraco da superfície.
        let base = rgba(getComputedStyle(quadro).backgroundColor)[0] ?? [13, 12, 10, 1];
        if (comCor && camada) {
          const pontos = rgba(getComputedStyle(camada).backgroundImage);
          base = pontos.sort((a, b) => lum(b) - lum(a))[0];
          const veus = [...rgba(getComputedStyle(camada, '::after').backgroundImage), ...rgba(getComputedStyle(camada, '::after').backgroundColor)]
            .filter(v => v[3] > 0).sort((a, b) => a[3] - b[3]);
          if (veus[0]) base = sobre(veus[0], base);
        }
        const fundo = (el) => {
          if (!el || el === quadro) return base;
          const c = rgba(getComputedStyle(el).backgroundColor)[0];
          if (!c || c[3] === 0) return fundo(el.parentElement);
          return sobre(c, fundo(el.parentElement));
        };
        const textos = [...quadro.querySelectorAll('*')].filter((e) => e.getClientRects().length
          && getComputedStyle(e).visibility !== 'hidden'
          && [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim()));
        for (const e of textos) {
          const cor = rgba(getComputedStyle(e).color)[0];
          const chave = `${quadro.className.split(' ')[0]}${quadro.dataset.pessoaId ? '#' + quadro.dataset.pessoaId : ''} ${e.className || e.tagName}`;
          saida[chave] = Math.min(saida[chave] ?? 99, Number(razao(cor, fundo(e)).toFixed(2)));
        }
      }
      return saida;
    }, [seletor, comCor]);

    const QUADROS = '#lista-roster li.pessoa, #tela-sessao .operador, .chamada-cartao';
    // Sem perfil nenhum: o contraste de base de cada texto.
    await desenharCom('#f2521f', 'todos');
    await page.evaluate(() => { testTable.perfis = []; testTable.snapshot.perfis_revision += 1; desenhar(testTable.snapshot); });
    await page.waitForTimeout(150);
    const semCor = await medir(QUADROS, false);

    const pioras = [];
    for (const cor of AMOSTRAS) {
      await desenharCom(cor, null);
      const marcados = await page.evaluate(() => ({
        linhas: document.querySelectorAll('#lista-roster li.pessoa[data-capa-de-cor]').length,
        operador: Boolean(document.querySelector('#tela-sessao .operador[data-capa-de-cor]')),
        cartoes: document.querySelectorAll('.chamada-cartao[data-capa-de-cor]').length,
        cor: (() => { const li = document.querySelector('#lista-roster li.pessoa[data-capa-de-cor]'); return li ? getComputedStyle(li).getPropertyValue('--cor-do-perfil').trim() : null; })(),
      }));
      assert.ok(marcados.linhas > 0 && marcados.operador && marcados.cartoes > 0, `${cor}: a cor não entrou em todas as superfícies: ${JSON.stringify(marcados)}`);
      assert.equal(marcados.cor, cor, 'a cor não chegou pela propriedade');
      const comCor = await medir('[data-capa-de-cor]', true);
      for (const [texto, razao] of Object.entries(comCor)) {
        const antes = semCor[texto] ?? 4.5;
        if (razao < Math.min(4.5, antes) - 0.01) pioras.push(`${cor} ${texto}: ${razao} (sem cor, ${antes})`);
      }
      if (process.env.SEELE_PREVIEW_DIR && (cor === '#4fd1c5' || cor === '#eae3cf')) {
        fs.mkdirSync(process.env.SEELE_PREVIEW_DIR, { recursive: true });
        await page.locator('#lista-roster').screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `cor-linhas-${cor.slice(1)}.png`) });
      }
    }
    assert.deepEqual(pioras, [], 'um texto do produto perdeu contraste sobre a cor do perfil');

    // Com capa, a imagem e nenhuma cor; sem perfil, nada.
    await desenharCom('#4fd1c5', null);
    const outro = await page.evaluate(() => {
      const li = document.querySelector('#lista-roster li.pessoa[data-capa-de-cor]');
      const id = li.dataset.pessoaId;
      const c = document.createElement('canvas'); c.width = 8; c.height = 8;
      window.capaContribuida = (p) => (String(p) === id ? { url: c.toDataURL(), estatico: c.toDataURL() } : null);
      desenhar(testTable.snapshot);
      return id;
    });
    await page.waitForTimeout(100);
    const comCapa = await page.evaluate((id) => { const li = document.querySelector(`#lista-roster li.pessoa[data-pessoa-id="${id}"]`); return { cor: li.dataset.capaDeCor ?? null, capa: li.dataset.comCapa ?? null }; }, outro);
    assert.deepEqual(comCapa, { cor: null, capa: 'sim' }, 'com capa, a cor ficou');
    await page.evaluate(() => { window.capaContribuida = () => null; });
    await desenharCom('#4fd1c5', 'todos');
    await page.evaluate(() => { testTable.perfis = []; testTable.snapshot.perfis_revision += 1; desenhar(testTable.snapshot); });
    await page.waitForTimeout(150);
    assert.equal(await page.locator('[data-capa-de-cor]').count(), 0, 'sem perfil, uma superfície ganhou cor');
    assert.deepEqual(errors, []);
    console.log(`cor-do-perfil: a cor entra sem capa (linha, operador, chamada) nas ${AMOSTRAS.length} amostras, com o texto legível; com capa, não; sem perfil, nada.`);
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error(e); process.exitCode = 1; });
