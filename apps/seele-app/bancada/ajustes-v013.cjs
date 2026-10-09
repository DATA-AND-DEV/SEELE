// Navegador com HTML/CSS reais; a ponte nativa é simulada.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium: abrirChromium } = require('./playwright.cjs');
const chromium = abrirChromium();

/**
 * Espera um elemento dizer `trecho`, e reprova com o que ele diz se não disser.
 *
 * Três segundos: a faixa de contexto da configuração é redesenhada por
 * `atualizarServer`, que roda a cada 500 ms, e lê o quadro da sessão, que
 * também anda a cada 500 ms. Um segundo cobre os dois; o resto é folga de
 * máquina lenta.
 */
async function esperarQueDiga(page, id, trecho, defeito) {
  try {
    await page.waitForFunction(
      ([id, trecho]) => (document.getElementById(id)?.textContent ?? '').includes(trecho),
      [id, trecho],
      { timeout: 3000 },
    );
  } catch {
    const agora = await page.evaluate(id => document.getElementById(id)?.textContent, id);
    assert.fail(`${defeito} — #${id} diz «${agora}»`);
  }
}

/**
 * A tinta de cada linha do título do perfil, pela fonte de verdade.
 *
 * A área de conteúdo do `Range` não serve: esta face tem 38px de área para 24px
 * de letra, e as áreas se sobrepõem com qualquer altura de linha abaixo de 1,58.
 * O defeito que se vê é a tinta de uma linha encostar na da outra. A linha de
 * base sai do topo da área mais o ascendente da fonte; a altura da tinta, de
 * `measureText` do texto daquela linha.
 */
function tintasDoTitulo() {
  const h2 = document.getElementById('perfil-titulo');
  const no = h2.firstChild;
  const linhas = [];
  for (let i = 0; i < no.textContent.length; i++) {
    const faixa = document.createRange();
    faixa.setStart(no, i);
    faixa.setEnd(no, i + 1);
    const caixa = faixa.getClientRects()[0];
    if (!caixa) continue;
    let linha = linhas.find(l => Math.abs(l.topo - caixa.top) < 1);
    if (!linha) linhas.push(linha = { topo: caixa.top, texto: '' });
    linha.texto += no.textContent[i];
  }
  const estilo = getComputedStyle(h2);
  const ctx = document.createElement('canvas').getContext('2d');
  ctx.font = `${estilo.fontWeight} ${estilo.fontSize} ${estilo.fontFamily}`;
  const tintas = linhas.map(l => {
    const m = ctx.measureText(l.texto.trim());
    const base = l.topo + m.fontBoundingBoxAscent;
    return { texto: l.texto.trim(), topo: base - m.actualBoundingBoxAscent, fim: base + m.actualBoundingBoxDescent };
  });
  const folga = tintas.length > 1 ? Math.round((tintas[1].topo - tintas[0].fim) * 10) / 10 : null;
  return { tintas, folga, lineHeight: estilo.lineHeight };
}

// --------------------------------------------------------------------------
// Lote R da F5: a volta que aceita os MODs leva a credencial da tentativa
// --------------------------------------------------------------------------
//
// **O caso.** Um servidor fechado por convite, com a portaria desligada e um
// MOD exigido. A primeira batida para na pergunta dos MODs; a pessoa aceita, e a
// volta saía sem o convite — o servidor respondia CREDENCIAL RECUSADA. O lado do
// servidor a F1 consertou em `d0b98b9c`: recusar os MODs deixa o convite
// intacto. Esta cena prova o lado do app.
//
// **O servidor de mentira responde como o de verdade, na mesma ordem.** O
// segredo é conferido antes de tudo (`Politica::avaliar`, sem memória com a
// portaria desligada), depois vêm os MODs, e o convite é gasto só quando a
// pessoa entra. A volta sem o convite recebe `CredentialRejected` aqui pelo
// mesmo motivo que recebe lá.
//
// **A credencial é gerada aqui** e não é de servidor nenhum. Ela é procurada no
// fim em todo lugar onde não pode estar: no armazenamento da página, no DOM, no
// console e em qualquer comando que não seja o `connect`.

/** O convite desta cena: um texto qualquer, que só esta bancada conhece. */
const CONVITE_R = `convite-da-bancada-${process.pid}-${Date.now()}`;
const ALVO_R = '192.0.2.7:8384';
const MOD_R = {
  id: 'fulano/exemplo', version: '1.0.0', hash: 'ab'.repeat(32),
  repo: 'https://github.com/seele/exemplo', reach: ['ler'], no_servidor: false,
};

/** Os `connect` que a página mandou até agora, com os argumentos. */
function conectsFeitos(page) {
  return page.evaluate(() => testCalls.filter(c => c.cmd === 'connect').map(c => c.args));
}

/**
 * A credencial que a pergunta dos MODs guarda agora, ou `null`.
 *
 * `aceitePendente` é o único lugar onde ela mora entre a batida e a volta
 * (`camada-mods.js`). Lida pelo nome porque «esquecida» quer dizer isto: o
 * lugar que a guardava não a guarda mais.
 */
function credencialGuardada(page) {
  return page.evaluate(() => aceitePendente?.credencial ?? null);
}

/** Um ponto do véu da pergunta que não é a caixa: onde clicar fora. */
function pontoDoVeu(page) {
  return page.evaluate(() => {
    const veu = document.getElementById('mods-aceite');
    const caixa = veu.getBoundingClientRect();
    for (const [x, y] of [[caixa.left + 4, caixa.bottom - 4], [caixa.right - 4, caixa.bottom - 4], [caixa.left + 4, caixa.top + 40]]) {
      if (document.elementFromPoint(x, y) === veu) return { x, y };
    }
    return null;
  });
}

/** Uma batida nova, a partir da tela de entrada, com o convite colado. */
async function baterComOConvite(page) {
  await page.evaluate(([alvo, convite]) => {
    window.testServidor = { aceitou: false, gasto: false };
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    // O caminho do diálogo de servidores depois de `analisar_convite`: o alvo e
    // o token do link, na mesma chamada.
    irParaOServidor(alvo, null, convite);
  }, [ALVO_R, CONVITE_R]);
  try {
    await page.waitForFunction(() => !document.getElementById('mods-aceite').hidden, null, { timeout: 3000 });
  } catch {
    const naEntrada = await page.evaluate(() => document.getElementById('boot-erro').textContent);
    assert.fail(`a batida com o convite não chegou à pergunta dos MODs — a entrada diz «${naEntrada}»`);
  }
}

async function aVoltaDoAceiteLevaACredencial(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  const consoles = []; page.on('console', m => consoles.push(m.text()));
  await page.addInitScript(({ table, convite, mod, snapshot }) => {
    window.testTable = table; window.testCalls = []; window.testAtraso = {};
    window.testServidor = { aceitou: false, gasto: false };
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        window.testCalls.push({ cmd, args });
        if (window.testAtraso[cmd]) await new Promise(ok => setTimeout(ok, window.testAtraso[cmd]));
        if (cmd === 'aceitar_mods') window.testServidor.aceitou = true;
        if (cmd === 'instalar_mod_do_catalogo') window.testTable.mods_instalados = [{ id: mod.id, hash: mod.hash }];
        if (cmd === 'connect') {
          const servidor = window.testServidor;
          const recusa = (error) => { throw { error, trail: [] }; };
          if (args.joinSecret !== convite || servidor.gasto) recusa('CredentialRejected');
          if (!servidor.aceitou) recusa({ ModsNaoAceitos: { mods: [mod], conjunto: 'conjunto-do-lote-r' } });
          servidor.gasto = true;
          return {
            snapshot,
            veredito: { FirstContactVerified: { fingerprint: 'cd'.repeat(32) } },
            geracao: window.testCalls.length,
          };
        }
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    table: respostas({
      apelido_local: 'Bia', snapshot: null, microfones: [], saidas: [],
      mods_instalados: [{ id: MOD_R.id, hash: MOD_R.hash }],
    }),
    convite: CONVITE_R, mod: MOD_R, snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  // ---- 1. A volta que aceita os MODs leva a credencial, e entra.
  await baterComOConvite(page);
  assert.equal((await conectsFeitos(page))[0]?.joinSecret, CONVITE_R,
    'a primeira batida não levou o convite do link, e esta cena não mede nada');
  await page.click('#mods-aceitar');
  await page.waitForFunction(() => testCalls.filter(c => c.cmd === 'connect').length >= 2, null, { timeout: 3000 });
  const volta = (await conectsFeitos(page))[1];
  assert.equal(volta.joinSecret, CONVITE_R,
    `a volta que aceita os MODs saiu sem a credencial da tentativa (joinSecret ${JSON.stringify(volta.joinSecret)}): ` +
    'num servidor fechado por convite ela recebe CREDENCIAL RECUSADA, e o convite que o servidor guardou intacto não serve para nada');
  await page.waitForFunction(() => !document.getElementById('tela-auth').hidden, null, { timeout: 3000 })
    .catch(async () => assert.fail(`a volta com a credencial não entrou — a entrada diz «${await page.evaluate(() => $('boot-erro').textContent)}»`));

  // ---- 2. Ela é esquecida ao entrar.
  assert.equal(await page.evaluate(() => aceitePendente?.credencial ?? null), null,
    'a credencial da tentativa continuou guardada depois de a pessoa entrar: um convite de uso único já gasto ficou na memória da janela');

  // ---- 3. A reconexão depois de uma sessão que entrou vai sem ela.
  await page.evaluate(() => mostrarFim('Kicked'));
  await page.click('#botao-voltar');
  await page.waitForFunction(() => testCalls.filter(c => c.cmd === 'connect').length >= 3, null, { timeout: 3000 });
  const reconexao = (await conectsFeitos(page))[2];
  assert.equal(reconexao.joinSecret ?? null, null,
    'o RECONECTAR de uma sessão que entrou levou o convite: reenviá-lo é gastar de novo o que já foi gasto');

  // ---- 4. Desistir esquece, por qualquer das três portas da pergunta.
  const desistencias = [
    ['RECUSAR E VOLTAR', () => page.click('#mods-recusar')],
    ['Escape', () => page.keyboard.press('Escape')],
    ['clicar fora', async () => {
      const ponto = await pontoDoVeu(page);
      assert.ok(ponto, 'não achei um ponto do véu da pergunta fora da caixa, e clicar fora não foi medido');
      await page.mouse.click(ponto.x, ponto.y);
    }],
  ];
  for (const [porta, desistir] of desistencias) {
    await baterComOConvite(page);
    assert.equal(await credencialGuardada(page), CONVITE_R,
      `com a pergunta aberta, a credencial não está guardada nela, e desistir por ${porta} não mede nada`);
    await desistir();
    await page.waitForFunction(() => document.getElementById('mods-aceite').hidden, null, { timeout: 3000 })
      .catch(() => assert.fail(`desistir por ${porta} não fechou a pergunta dos MODs`));
    assert.equal(await credencialGuardada(page), null,
      `desistir por ${porta} fechou a pergunta e deixou a credencial da tentativa na memória da janela`);
  }

  // ---- 5. Desistir enquanto o MOD chega também esquece, e não entra.
  //
  // A obtenção leva tempo, e a pergunta continua aberta durante ela: RECUSAR,
  // Escape e clicar fora seguem à mão. Quem desiste nesse meio não pode ver a
  // janela entrar sozinha quando o pacote termina de chegar — «sair da
  // pergunta não entra em lugar nenhum» — e muito menos com a credencial que
  // desistir esqueceu.
  await page.evaluate(() => { testTable.mods_instalados = []; testAtraso.instalar_mod_do_catalogo = 600; });
  await baterComOConvite(page);
  const antesDoAceite = (await conectsFeitos(page)).length;
  const conferenciasAntes = await page.evaluate(() => testCalls.filter(c => c.cmd === 'mods_instalados').length);
  await page.click('#mods-aceitar');
  await page.waitForFunction(() => testCalls.some(c => c.cmd === 'instalar_mod_do_catalogo'), null, { timeout: 3000 });
  await page.click('#mods-recusar');
  // A obtenção termina e confere o que chegou (a segunda leitura da lista); o
  // que viria depois dela é a volta.
  await page.waitForFunction(
    (antes) => testCalls.filter(c => c.cmd === 'mods_instalados').length >= antes + 2,
    conferenciasAntes, { timeout: 3000 },
  );
  await page.waitForTimeout(400);
  const depoisDoAceite = await conectsFeitos(page);
  assert.equal(depoisDoAceite.length, antesDoAceite,
    'a pessoa fechou a pergunta enquanto o MOD chegava, e a janela entrou mesmo assim' +
    (depoisDoAceite.at(-1)?.joinSecret === CONVITE_R ? ', com a credencial que desistir devia ter esquecido' : ''));
  assert.equal(await credencialGuardada(page), null,
    'desistir enquanto o MOD chegava deixou a credencial da tentativa na memória da janela');
  await page.evaluate((m) => { testAtraso = {}; testTable.mods_instalados = [{ id: m.id, hash: m.hash }]; }, MOD_R);

  assert.deepEqual(erros, [], 'a cena do lote R lançou erro na página');
  await page.close();
  return { consoles };
}

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(table => {
      window.testTable = table; window.testCalls = []; window.testAtraso = {};
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          window.testCalls.push({cmd,args});
          // Uma ponte lenta, por comando, para as cenas que medem o que aparece
          // antes de a resposta chegar.
          if (window.testAtraso[cmd]) await new Promise(ok => setTimeout(ok, window.testAtraso[cmd]));
          return window.testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ microfones: [], saidas: [], snapshot: null, fontes_de_tela: [{ id: 1, nome: 'Monitor', monitor: true, largura: 1920, altura: 1080 }], permissao_de_tela: 'Concedida', pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(() => abrirServer('tela-boot'));
    assert.equal(await page.locator('#secao-mods').isVisible(), false);
    await page.evaluate(r => { window.testTable.snapshot = r; fecharServer(); $('tela-boot').hidden = true; $('tela-sessao').hidden = false; desenhar(r); }, retrato());
    await page.evaluate(() => abrirServer('tela-sessao'));
    assert.equal(await page.locator('#secao-mods').isVisible(), true);
    // **A faixa de contexto acompanha o que muda com a configuração aberta**
    // (lote J da F5). Ela diz a sala, o modo do microfone e, no modo tecla, a
    // tecla — e era escrita só ao abrir: trocada a tecla para F, o botão dizia
    // F e a faixa ao lado continuava em ESPAÇO; trocado o modo, a faixa dizia o
    // de antes.
    await esperarQueDiga(page, 'server-contexto', 'microfone abre enquanto segura ESPAÇO',
      'a faixa de contexto não diz a tecla de falar padrão no modo tecla');
    await page.evaluate(() => { window.testTable.tecla_de_falar = 'KeyF'; pararDeOuvirTecla(); });
    await esperarQueDiga(page, 'server-contexto', 'microfone abre enquanto segura F',
      'trocada a tecla de falar para F, a faixa de contexto continuou dizendo a tecla de antes');
    // O modo: a ponte aplica e o retrato da sessão passa a dizer o novo.
    await page.evaluate(() => { window.testTable.snapshot.voice_mode = 'VoiceActivated'; return escolherModo('VoiceActivated'); });
    await esperarQueDiga(page, 'server-contexto', 'microfone abre por voz',
      'trocado o modo do microfone para VOZ, a faixa de contexto continuou dizendo o modo de antes');
    await page.evaluate(() => { window.testTable.snapshot.voice_mode = 'PushToTalk'; window.testTable.tecla_de_falar = null; pararDeOuvirTecla(); });
    await esperarQueDiga(page, 'server-contexto', 'microfone abre enquanto segura ESPAÇO',
      'de volta ao modo tecla com ESPAÇO, a faixa de contexto não voltou a dizê-lo');
    // **E certa no primeiro quadro** (rodada 3 do lote J). A configuração
    // aparece no `hidden = false` de `abrirServer`; se a faixa só é escrita
    // depois da lista de aparelhos e de uma ida à ponte, os primeiros quadros
    // mostram a faixa da abertura anterior. Com a ponte lenta — lista de
    // microfones em 400 ms, retrato em 150 ms —, reabrir com o microfone já mudo
    // tem de dizer «microfone mudo» no primeiro quadro, e não «segura ESPAÇO».
    await page.evaluate(() => fecharServer());
    await page.evaluate(() => {
      window.testTable.snapshot.muted = true;
      desenhar(window.testTable.snapshot);
      window.testAtraso = { microfones: 400, snapshot: 150 };
    });
    const noPrimeiroQuadro = await page.evaluate(() => new Promise(ok => {
      window.testAbertura = abrirServer('tela-sessao');
      requestAnimationFrame(() => ok(document.getElementById('server-contexto').textContent));
    }));
    assert.ok(noPrimeiroQuadro.includes('microfone mudo'),
      `reaberta com o microfone já mudo, a configuração mostrou no primeiro quadro a faixa da abertura anterior: «${noPrimeiroQuadro}»`);
    await page.evaluate(() => window.testAbertura);
    await page.evaluate(() => { window.testAtraso = {}; window.testTable.snapshot.muted = false; desenhar(window.testTable.snapshot); });
    await page.evaluate(() => abrirSecao('secao-mods'));
    await page.screenshot({ path: '/tmp/seele-mods-v013.png' });
    await page.evaluate(() => { fecharServer(); window.testTable.snapshot = null; return abrirServer('tela-boot'); });
    assert.equal(await page.locator('#painel-mods').isVisible(), false, 'o painel lembrado vazou na tela inicial');
    await page.evaluate(r => { fecharServer(); window.testTable.snapshot = r; }, retrato());
    await page.evaluate(() => abrirCompartilhar());
    // **A caixa nasce em 720p.** Com o piso, o padrão deixou de ser preferência
    // e virou suposição sobre a casa de quem hospeda: a 1080p ele suporia
    // 10,4 Mbps de todo mundo que nunca abrisse esta caixa.
    assert.equal(
      await page.evaluate(() => document.getElementById('compartilhar-resolucao').value),
      '720',
      'a caixa deixou de nascer em 720p, e o padrão voltou a supor 10,4 Mbps de quem nunca a abriu',
    );
    // Os quadros são escolha, e não mais o 60 escrito na função.
    await page.selectOption('#compartilhar-quadros', '30');
    assert.equal(
      await page.evaluate(() => limitesEscolhidos().quadros_maximos), 30,
      'o seletor de quadros virou enfeite: a pessoa mexe e a ponte manda outra coisa',
    );
    await page.selectOption('#compartilhar-quadros', '60');
    assert.equal(await page.evaluate(() => limitesEscolhidos().quadros_maximos), 60);
    await page.selectOption('#compartilhar-resolucao', '720');
    assert.equal(await page.evaluate(() => limitesEscolhidos().altura_maxima), 720);
    await page.screenshot({ path: '/tmp/seele-compartilhar-v013.png' });
    // A transmissão desta pessoa no ar, no retrato que o produto lê: a troca
    // durante a transmissão pergunta por `minha_transmissao` (ADR 0054), e não
    // mais por `tela.e_minha`. Com o retrato velho, a bancada esperava 30 s
    // por uma chamada que o produto, com razão, não fazia. Os campos são os de
    // `TelaEmCurso` (`crates/seele-ffi/src/types.rs`); sem `pedido`, para a
    // caixa não restaurar limites de outro pedido.
    await page.evaluate(() => {
      window.testTable.snapshot.minha_transmissao = { tela: 1, de: 1, e_minha: true, espectadores: 0, parada: null, pedido: null };
    });
    await page.selectOption('#compartilhar-resolucao', '1080');
    await page.waitForFunction(() => testCalls.some(c => c.cmd === 'ajustar_limites_da_tela' && c.args.limites.altura_maxima === 1080));
    // O mesmo para os quadros: a troca atravessa com a transmissão no ar.
    await page.selectOption('#compartilhar-quadros', '30');
    await page.waitForFunction(() => testCalls.some(c => c.cmd === 'ajustar_limites_da_tela' && c.args.limites.quadros_maximos === 30));
    await page.evaluate(() => {
      fecharCompartilhar();
      const dono = { falar() {}, instancia: { registrar: () => ({ esquecer() {} }) } };
      const mod = { id: 'teste/estilo' };
      contribuicoesDosMods.registrar(mod, dono.instancia, { ponto: 'servidor.navegacao', rotulo: 'Tema oculto', listarNaBarra: false });
      contribuicoesDosMods.registrar({ id: 'teste/mesa' }, dono.instancia, { ponto: 'servidor.navegacao', rotulo: 'Mesa' });
      redesenharAsEntradasDeMod();
      window.testSurfaces = new SuperficiesDoMod('teste/mesa', dono, {
        paginas: $('palco-de-paginas'), paineis: $('palco-de-paineis'), camadas: $('palco-de-camadas'), avisos: $('avisos-de-mod'),
      });
      testSurfaces.criar({ id: 'mesa', tipo: 'pagina', titulo: 'Mesa', imersiva: true });
      testSurfaces.de('mesa').montar([{ forma: 'titulo', dentro: 'CAMPANHA' }, { forma: 'texto', dentro: 'Tabuleiro da sessão' }]);
    });
    assert.equal(await page.locator('#lista-mods-navegacao li').count(), 1);
    const box = await page.locator('#palco-de-paginas').boundingBox();
    assert.equal(box.width, 1280); assert.equal(box.x, 0); assert.equal(box.y, 32);
    await page.screenshot({ path: '/tmp/seele-mesa-v013.png' });
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('#palco-de-paginas').isVisible(), false);
    await page.evaluate(() => {
      for (let i = 0; i < 20; i++) testSurfaces.criar({ id: 'aviso-' + i, tipo: 'aviso', titulo: 'Imagem salva' });
    });
    assert.equal(await page.locator('.aviso-de-mod').count(), 4);
    assert.equal(await page.locator('.aviso-de-mod').first().innerText(), 'Imagem salva');
    await page.waitForTimeout(3700);
    assert.equal(await page.locator('.aviso-de-mod').count(), 0);
    assert.equal(await page.evaluate(() => testSurfaces.porChave.size), 1, 'avisos expirados retidos');
    assert.deepEqual(errors, []);

    // **A entrada por link, sem nome nesta máquina** (lote J da F5). O launcher
    // pede `entrar`, e a primeira entrada sem nome pergunta antes do `connect`.
    // A ponte daqui resolve em microtarefa, que é o que expunha a ordem de
    // carga: com a abertura chamada no topo de `tela-boot.js`, a entrada
    // chegava à pergunta antes de `camada-nomear.js` existir — `perguntarONome
    // is not defined`, e a pergunta nunca abria.
    const link = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const errosDoLink = []; link.on('pageerror', e => errosDoLink.push(e.message));
    await link.addInitScript(table => {
      window.testTable = table; window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          window.testCalls.push({ cmd, args });
          // O que se grava fica gravado: a pergunta relê o nome depois de fechar.
          if (cmd === 'escolher_apelido_local') window.testTable.apelido_local = args.apelido;
          return window.testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({
      apelido_local: '', snapshot: null, microfones: [], saidas: [],
      abertura: { entrar: 'seele://192.0.2.1:8384', hospedar: false, nome_publico: null },
      analisar_convite: { alvo: '192.0.2.1:8384', token: null },
    }));
    await link.goto(`http://127.0.0.1:${server.address().port}`);
    try {
      await link.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 });
    } catch {
      const naEntrada = await link.evaluate(() => document.getElementById('boot-erro').textContent);
      assert.fail(`aberta por um link sem nome nesta máquina, a janela não perguntou o nome — a entrada diz «${naEntrada}»`);
    }
    assert.equal(await link.evaluate(() => $('perfil-titulo').textContent), 'COMO OS OUTROS VÃO TE CHAMAR?',
      'a caixa que abriu pela entrada por link não é a pergunta do nome: quem chega sem nome lê outro título, e não sabe que o que se pede é como os outros vão chamá-lo');
    assert.equal(await link.evaluate(() => testCalls.filter(c => c.cmd === 'connect').length), 0,
      'a entrada por link conectou antes de a pergunta do nome ser respondida');

    // O título da pergunta quebra em duas linhas na caixa de 480px. Com a
    // altura de linha herdada de `base.css` (16px para letra de 24px), a tinta
    // da segunda encostava na da primeira (folga de −0,9px).
    await link.evaluate(() => document.fonts.ready);
    // **A quebra forçada, em qualquer plataforma.** Na caixa de 480px o título
    // quebra por 15px (364px numa linha, 349px de espaço, medido no macOS), e o
    // job `bancadas` roda em Linux, onde os avanços dos glifos podem diferir. Só
    // nesta cena, o título ganha largura máxima de 70% da própria largura numa
    // linha, medida aqui mesmo: menos que a linha inteira, então quebra; e o que
    // sobra depois da primeira linha cabe em 70%, então quebra uma vez só. A
    // conta vale para qualquer fonte, porque as duas larguras saem da mesma.
    await link.evaluate(() => {
      const h2 = document.getElementById('perfil-titulo');
      h2.style.whiteSpace = 'nowrap';
      const faixa = document.createRange();
      faixa.selectNodeContents(h2);
      const numaLinha = faixa.getBoundingClientRect().width;
      h2.style.whiteSpace = '';
      h2.style.maxWidth = `${Math.floor(numaLinha * 0.7)}px`;
    });
    const titulo = await link.evaluate(tintasDoTitulo);
    assert.equal(titulo.tintas.length, 2, `o título da pergunta deixou de quebrar em duas linhas, e esta medida não mede mais nada: ${JSON.stringify(titulo)}`);
    assert.ok(titulo.folga > 0,
      `as duas linhas do título da pergunta se sobrepõem: folga de ${titulo.folga}px entre as tintas, com line-height ${titulo.lineHeight}`);

    // Responder pelo teclado: o foco está no campo, e Enter salva.
    await link.keyboard.type('Bia');
    await link.keyboard.press('Enter');
    await link.waitForFunction(() => testCalls.some(c => c.cmd === 'connect'), null, { timeout: 3000 });
    const ordem = await link.evaluate(() => testCalls.map(c => c.cmd).filter(c => c === 'escolher_apelido_local' || c === 'connect'));
    assert.deepEqual(ordem, ['escolher_apelido_local', 'connect'], 'o nome não foi gravado antes do connect');
    assert.equal(await link.evaluate(() => testCalls.find(c => c.cmd === 'connect').args.nickname), 'Bia',
      'a entrada por link não levou o nome que acabou de ser respondido');
    assert.deepEqual(errosDoLink, [],
      'a entrada por link lançou erro na página: o que está nesta lista quebrou sem ninguém ver, e a janela de verdade não diz nada a quem a usa');
    await link.close();

    await aVoltaDoAceiteLevaACredencial(browser, server.address().port);

    console.log('v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium');
    console.log(`lote J: a faixa de contexto acompanha a tecla e o modo, a entrada por link pergunta o nome antes do connect, e o título da pergunta não se sobrepõe (folga de ${titulo.folga}px entre as tintas)`);
    console.log('lote R: a volta que aceita os MODs leva a credencial da tentativa e entra, e o RECONECTAR de uma sessão que entrou vai sem ela');
    console.log('lote R: desistir da pergunta (RECUSAR, Escape, clicar fora, ou no meio da obtenção) esquece a credencial e não entra');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
