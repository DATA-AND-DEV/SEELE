// O codificador de vídeo em uso, no rodapé da sessão do desktop, com o HTML, o
// CSS e o `tela-sessao.js` reais; só a ponte nativa é simulada. No molde de
// `nucleo.cjs`.
//
// # Por que existe
//
// O motor escolhe entre o codificador do sistema (VideoToolbox no Mac, o do
// Media Foundation no Windows) e a queda para o OpenH264, em software, e só o
// `seele.log` ficava sabendo qual. Quem transmitia e via a CPU subir não tinha
// na tela a única informação que explica. O nome agora sobe pelo retrato
// (`minha_transmissao.codificador`, provado em Rust em
// `seele-ffi/src/testes/o_codificador_no_retrato.rs`) e o rodapé o desenha na
// célula VÍDEO, ao lado de ATRASO, JITTER e DISTÚRBIO.
//
// # O que ela mede, desenhado
//
// 1. Com uma transmissão própria e o nome no retrato, a célula VÍDEO está **à
//    vista** (pela caixa, e não só pelo atributo) e diz o nome, sem cortar.
// 2. O nome que muda no retrato (a troca de degrau que cai para software) muda
//    na célula.
// 3. O nome é texto: um nome com marcação não vira nó (no Windows, quem dá o
//    nome é o sistema).
// 4. Sem nome (a câmera, o navegador, antes do primeiro fluxo) e sem
//    transmissão, a célula some, e as vizinhas continuam.
// 5. O retrato repetido não muta o rodapé: ele chega duas vezes por segundo.
//
// Rodar:  node apps/seele-app/bancada/codificador-no-rodape.cjs

const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

const minha = (codificador) => ({ tela: 9, de: 1, e_minha: true, espectadores: 1, parada: null, pedido: null, codificador });

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

    // Desenha um retrato pelo `desenhar` do produto e lê a célula como a
    // pessoa a vê: a caixa, o texto, e se o texto cabe.
    const desenhado = (r) => page.evaluate((r) => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      const celula = document.getElementById('tel-video-celula');
      const valor = document.getElementById('tel-video');
      const caixa = celula.getBoundingClientRect();
      const rodape = celula.closest('.telemetria').getBoundingClientRect();
      const aVista = (e) => e.checkVisibility() && e.getBoundingClientRect().width > 0;
      return {
        aVista: aVista(celula),
        rotulo: celula.querySelector('.rotulo').textContent,
        texto: valor.textContent,
        filhos: valor.childElementCount,
        cabe: valor.scrollWidth <= valor.clientWidth + 1 || getComputedStyle(valor).display === 'inline',
        noRodape: caixa.left >= rodape.left - 1 && caixa.right <= rodape.right + 1,
        vizinhas: ['tel-rtt', 'tel-jit', 'tel-loss', 'tel-codec'].filter((id) => !aVista(document.getElementById(id))),
      };
    }, r);

    // 1. O nome do software, à vista.
    let visto = await desenhado(retrato({ minha_transmissao: minha('OpenH264 (software)') }));
    assert.equal(visto.aVista, true, 'com a transmissão no ar e o codificador no retrato, a célula VÍDEO do rodapé não está à vista');
    assert.equal(visto.rotulo, 'VÍDEO');
    assert.equal(visto.texto, 'OpenH264 (software)', 'o rodapé não diz o codificador que o retrato traz');
    assert.equal(visto.cabe && visto.noRodape, true, `o nome do codificador não cabe no rodapé: ${JSON.stringify(visto)}`);
    assert.deepEqual(visto.vizinhas, [], 'a célula VÍDEO tirou uma vizinha do rodapé');

    // 2. A troca de degrau que muda o codificador.
    visto = await desenhado(retrato({ minha_transmissao: minha('VideoToolbox') }));
    assert.equal(visto.texto, 'VideoToolbox', 'o codificador mudou no retrato e o rodapé ficou com o de antes');

    // 3. O nome que o sistema dá é texto.
    visto = await desenhado(retrato({ minha_transmissao: minha('<b id="injetado">NVIDIA</b> H.264 Encoder MFT') }));
    assert.equal(visto.filhos, 0, 'o nome do codificador virou marcação dentro do rodapé');
    assert.equal(visto.texto, '<b id="injetado">NVIDIA</b> H.264 Encoder MFT');
    assert.equal(visto.noRodape, true, `um nome comprido do sistema sai do rodapé: ${JSON.stringify(visto)}`);

    // 4. Sem nome, e sem transmissão: a célula some.
    for (const [caso, r] of [
      ['a transmissão sem nome (a câmera, antes do primeiro fluxo)', retrato({ minha_transmissao: minha(null) })],
      ['o retrato sem o campo (o navegador)', retrato({ minha_transmissao: { tela: 9, de: 1, e_minha: true, espectadores: 1, parada: null, pedido: null } })],
      ['o nome vazio', retrato({ minha_transmissao: minha('  ') })],
      ['sem transmissão própria', retrato({ minha_transmissao: null })],
    ]) {
      visto = await desenhado(r);
      assert.equal(visto.aVista, false, `${caso}: a célula VÍDEO ficou à vista, dizendo «${visto.texto}»`);
      assert.deepEqual(visto.vizinhas, [], `${caso}: uma vizinha da célula VÍDEO sumiu junto`);
    }

    // 5. O mesmo retrato de novo não muta a célula.
    await desenhado(retrato({ minha_transmissao: minha('VideoToolbox') }));
    const mutacoes = await page.evaluate(async (r) => {
      const vistas = [];
      const olho = new MutationObserver((lista) => vistas.push(...lista.map((m) => m.type)));
      olho.observe(document.getElementById('tel-video-celula'), { attributes: true, childList: true, characterData: true, subtree: true });
      desenhar(r);
      desenhar(r);
      await new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      olho.disconnect();
      return vistas;
    }, retrato({ minha_transmissao: minha('VideoToolbox') }));
    assert.deepEqual(mutacoes, [], 'o retrato repetido mutou a célula VÍDEO do rodapé');

    assert.deepEqual(errors, [], `erros de script: ${errors}`);
    console.log('PASS o rodapé mostra o codificador de vídeo em uso, o troca com o retrato, o trata como texto e some sem ele');
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error(e); process.exitCode = 1; });
