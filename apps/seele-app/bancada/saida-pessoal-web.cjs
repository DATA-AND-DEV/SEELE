// APIs de dispositivos simuladas: não solicita acesso a microfone/saída reais.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { servir, respostas } = require('./telas.cjs');
const motores = require('./playwright.cjs');
const codigo = fs.readFileSync(path.join(__dirname, '../ui/voz-web.js'), 'utf8');
const contexto = vm.createContext({ console, navigator: {}, window: {}, DOMException });
vm.runInContext(codigo, contexto);
vm.runInContext(fs.readFileSync(path.join(__dirname, '../ui/pessoal-web.js'), 'utf8'), contexto);
async function controlador() {
  const memoria = new Map(), aplicados = [], estados = [];
  let lista = [{ kind: 'audiooutput', deviceId: 'fone-real', label: 'Fone' }], chamadas = 0, escolher;
  const armazenamento = { getItem: k => memoria.get(k), setItem: (k, v) => memoria.set(k, v), removeItem: k => memoria.delete(k) };
  const midia = { enumerateDevices: async () => lista, selectAudioOutput: () => { chamadas++; return escolher(); } };
  const criar = (suportada = true) => contexto.criarSelecaoDaSaidaPessoal({ suportada, midia, armazenamento,
    aplicar: async id => aplicados.push(id), aoEstado: e => estados.push(e) });
  const c = criar();
  await c.atualizar(); assert.equal(chamadas, 0); assert.equal(estados.at(-1).disponivel, true);
  await c.selecionar('fone-real'); assert.equal(aplicados.at(-1), 'fone-real');
  const nova = criar(); await nova.restaurar(); assert.equal(aplicados.at(-1), 'fone-real'); assert.equal(chamadas, 0);
  escolher = async () => { throw new DOMException('Cancelado', 'NotAllowedError'); };
  await nova.selecionar(); assert.equal(estados.at(-1).id, 'fone-real');
  let resolver; escolher = () => new Promise(r => resolver = r);
  const pendente = nova.selecionar(); nova.invalidar(); const antes = aplicados.length;
  resolver({ deviceId: 'outro-real', label: 'Outro' }); await pendente;
  assert.equal(aplicados.length, antes, 'seletor tardio não muda conversa encerrada');
  await c.selecionar('fone-real'); lista = []; await c.atualizar();
  assert.equal(aplicados.at(-1), ''); assert.equal(estados.at(-1).id, ''); assert.equal(memoria.size, 0);
  const semApi = criar(false); await semApi.atualizar(); await semApi.selecionar();
  assert.equal(estados.at(-1).disponivel, false); assert.equal(chamadas, 2);
  const semSeletor = contexto.criarSelecaoDaSaidaPessoal({ suportada: true, midia: { enumerateDevices: async () => [] }, armazenamento,
    aplicar: async () => assert.fail('Sem dispositivos não deve aplicar'), aoEstado: e => estados.push(e) });
  await semSeletor.atualizar(); assert.equal(estados.at(-1).disponivel, false);
  let recusar = false;
  const enumerada = contexto.criarSelecaoDaSaidaPessoal({ suportada: true,
    midia: { enumerateDevices: async () => [{ kind: 'audiooutput', deviceId: 'caixa-real', label: 'Caixa' }] }, armazenamento,
    aplicar: async id => { if (recusar) throw new DOMException('Recusado', 'NotAllowedError'); aplicados.push(id); }, aoEstado: e => estados.push(e) });
  await enumerada.atualizar(); assert.equal(estados.at(-1).nativa, false); assert.equal(estados.at(-1).disponivel, true);
  await enumerada.selecionar('caixa-real'); recusar = true;
  await assert.rejects(enumerada.selecionar(''), { name: 'NotAllowedError' });
  assert.equal(estados.at(-1).id, 'caixa-real', 'falha não anuncia uma rota que não abriu');
  await assert.rejects(c.selecionar('inventado'), /Saída não disponível/);
  console.log('PASS controlador: seleção, cancelamento, preferência, remoção, geração e API ausente');
}
async function elementoAtivo() {
  const elementos = [], aplicados = [];
  let contextos = 0, capturas = 0, completar;
  const evento = { addEventListener() {}, removeEventListener() {} };
  Object.assign(contexto, { performance, window: evento,
    navigator: { mediaDevices: { getUserMedia() { capturas++; throw new Error('Não autorizado no teste'); } } },
    document: { ...evento, body: { append() {} }, createElement() {
      const el = { play: async () => {}, pause() {}, remove() {}, setSinkId(id) {
        aplicados.push([this, id]); return completar ? new Promise(r => completar.resolver = r) : Promise.resolve();
      } }; elementos.push(el); return el;
    } },
    AudioContext: class {
      constructor() { contextos++; this.audioWorklet = { addModule: async () => {} }; }
      createMediaStreamDestination() { return { stream: { getTracks: () => [] } }; }
      async resume() {} async close() {}
    },
    AudioWorkletNode: class { constructor() { this.port = { postMessage() {} }; } connect() {} disconnect() {} },
  });
  const voz = contexto.criarVozDoNavegador({ wasm: {}, bytesDoWasm: new ArrayBuffer(0) });
  await voz.ligar({ somenteOuvir: true }); await voz.trocarSaida('fone-real');
  assert.equal(aplicados[0][0], elementos[0]); assert.equal(aplicados[0][1], 'fone-real');
  assert.equal(contextos, 1); assert.equal(capturas, 0, 'trocar saída não abre microfone');
  completar = {};
  const tardia = voz.trocarSaida('outro-real'); await voz.desligar();
  completar.resolver(); await assert.rejects(tardia, { name: 'AbortError' });
  await assert.rejects(voz.trocarSaida('fone-real'), /indisponível/);
  console.log('PASS adaptador: saída no elemento ativo, sem novo contexto/microfone, encerramento tardio');
}
async function vista() {
  const server = servir(); await new Promise(r => server.listen(0, '127.0.0.1', r));
  try {
    for (const motor of ['chromium', 'webkit']) {
      const browser = await motores[motor]().launch({ headless: true });
      try {
        const page = await browser.newPage();
        await page.addInitScript(tabela => { window.__TAURI__ = { core: { invoke: async cmd => tabela[cmd] ?? null },
          event: { listen: async () => () => {} }, window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } }; },
        respostas({ amigos: [], contatos_da_agenda: [], conhecidos: [], microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.evaluate(() => {
          document.documentElement.dataset.casca = "celular";
          window.escolhas = [];
          const acao = () => {};
          window.vistaSaida = criarVistaDaConversaPessoal({ acoes: { voltar: acao, criar: acao, chamar: acao, copiar: acao, atender: acao,
            recusar: acao, encerrar: acao, enviarTexto: acao, microfone: acao, compartilhar: acao, podeCompartilhar: () => true,
            saida: id => escolhas.push(id === undefined ? 'seletor-nativo' : id) } });
          vistaSaida.elemento.id = "saida-pessoal-bancada"; document.body.append(vistaSaida.elemento); vistaSaida.abrir(); vistaSaida.estado({ estado: 'dentro', apelido: 'Par' });
        });
        const raiz = page.locator('#saida-pessoal-bancada');
        assert.equal(await raiz.locator('.pessoal-web-saida').isVisible(), false);
        await page.evaluate(() => vistaSaida.saida({ disponivel: true, nativa: true, aparelhos: [{ id: 'fone-real', nome: '<Fone>' }], id: '', nome: 'Sistema' }));
        for (const [width, height] of [[320, 568], [390, 844], [568, 320]]) {
          await page.setViewportSize({ width, height });
          const botoes = await raiz.locator('.pessoal-web-rodape button:visible').evaluateAll(bs => bs.map(b => {
            const r = b.getBoundingClientRect(); return { w: r.width, h: r.height, x: r.x, fim: r.right, baixo: r.bottom };
          }));
          assert.ok(botoes.every(b => b.w >= 44 && b.h >= 44 && b.x >= 0 && b.fim <= width + 1 && b.baixo <= height + 1), JSON.stringify(botoes));
          assert.equal(await raiz.evaluate(e => e.scrollWidth > innerWidth + 1), false, JSON.stringify(await raiz.evaluate(e => ({ width: innerWidth, scroll: e.scrollWidth, rect: e.getBoundingClientRect().toJSON() }))));
        }
        await raiz.getByRole('button', { name: 'Saída de áudio: Sistema', exact: true }).click();
        await raiz.getByRole('combobox', { name: 'Saída de áudio', exact: true }).selectOption('fone-real');
        await raiz.getByRole('button', { name: 'ESCOLHER DISPOSITIVO', exact: true }).click();
        assert.deepEqual(await page.evaluate(() => escolhas), ['fone-real', 'seletor-nativo']);
        await raiz.getByRole('combobox').press('Escape');
        assert.equal(await raiz.locator('.pessoal-web-painel-saida').isVisible(), false);
        await page.evaluate(() => vistaSaida.estado({ estado: 'encerrada' }));
        assert.equal(await raiz.locator('.pessoal-web-saida').isVisible(), false);
        console.log(`PASS ${motor}: saída acessível, seleção explícita, mobile/landscape, API ausente`);
      } finally { await browser.close(); }
    }
  } finally { await new Promise(r => server.close(r)); }
}
(async () => { await controlador(); await elementoAtivo(); await vista(); })().catch(e => { console.error(e); process.exitCode = 1; });
