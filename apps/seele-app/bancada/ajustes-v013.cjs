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

/**
 * O link que a pessoa cola, com o convite dentro.
 *
 * A forma é a de um `seele://`; quem o lê aqui é o `analisar_convite` de
 * mentira, que devolve o alvo e o token como o Rust devolveria.
 */
const LINK_R = `seele://${ALVO_R}?token=${CONVITE_R}`;

/** O que o campo do diálogo de servidores diz agora. */
function campoDoLink(page) {
  return page.evaluate(() => document.getElementById('servidores-endereco').value);
}

/**
 * Uma batida nova, a partir da tela de entrada, **pelo campo**: o diálogo de
 * servidores, o link colado e Enter — o caminho de quem recebe um convite.
 */
async function baterComOConvite(page) {
  await page.evaluate(() => {
    window.testServidor = { aceitou: false, gasto: false };
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    return abrirServidores();
  });
  await page.fill('#servidores-endereco', LINK_R);
  await page.press('#servidores-endereco', 'Enter');
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
  await page.addInitScript(({ table, convite, mod, snapshot }) => {
    window.testTable = table; window.testCalls = []; window.testAtraso = {};
    window.testServidor = { aceitou: false, gasto: false };
    // O sim dos MODs gravado nesta máquina, por alvo, como o disco o guarda
    // (`seele_ffi::mods`): o `aceitar_mods` grava quando termina, e o
    // `esquecer_aceite_de_mods` apaga (lote W da F5).
    window.testAceites = {};
    // Um comando que falha, por nome, com a forma que o Rust devolve.
    window.testFalhas = {};
    // O console inteiro, com os argumentos serializados: um objeto passado ao
    // `console.warn` chega ao Playwright como «Object», e a credencial dentro
    // dele passaria sem ser vista.
    window.testConsole = [];
    for (const nivel of ['log', 'info', 'warn', 'error', 'debug']) {
      const original = console[nivel].bind(console);
      console[nivel] = (...args) => {
        try { window.testConsole.push(JSON.stringify(args)); } catch { window.testConsole.push(String(args)); }
        original(...args);
      };
    }
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        window.testCalls.push({ cmd, args });
        if (window.testAtraso[cmd]) await new Promise(ok => setTimeout(ok, window.testAtraso[cmd]));
        if (window.testFalhas[cmd]) throw window.testFalhas[cmd];
        if (cmd === 'aceitar_mods') { window.testServidor.aceitou = true; window.testAceites[args.alvo] = args.conjunto; }
        if (cmd === 'esquecer_aceite_de_mods') delete window.testAceites[args.alvo];
        // Um link com erro de digitação: o `analisar_convite` de verdade
        // responde com o nome da falha, e nenhuma tentativa começa.
        if (cmd === 'analisar_convite' && String(args?.link).includes('erro-de-digitacao')) throw 'TokenInvalido';
        // Um link de um servidor de outra linha: o Rust devolve o
        // `versoes::ParaEntrar`, e a entrada segue por `entrarPelaLinha`.
        if (cmd === 'analisar_convite' && String(args?.link).includes('outra-linha')) {
          return {
            alvo: '192.0.2.9:8384', token: 'convite-de-outra-linha',
            para_entrar: { Abrir: { servidor: '0.15', linha: '0.15', perde_conversas: false } },
          };
        }
        if (cmd === 'instalar_mod_do_catalogo') window.testTable.mods_instalados = [{ id: mod.id, hash: mod.hash }];
        if (cmd === 'connect') {
          const servidor = window.testServidor;
          const recusa = (error) => { throw { error, trail: [] }; };
          // A forma é a de `ConnectionError::Refused` da FFI, que a tela lê
          // como CREDENCIAL RECUSADA (`fraseDeErro`).
          // `testOutroConvite` é o segredo de uma segunda batida, para a cena em
          // que uma pergunta substitui outra (5b).
          const aceita = args.joinSecret === convite || (window.testOutroConvite && args.joinSecret === window.testOutroConvite);
          if (!aceita || servidor.gasto) recusa({ Refused: { reason: 'CredentialRejected' } });
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
      analisar_convite: { alvo: ALVO_R, token: CONVITE_R, para_entrar: null },
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

  // ---- 2. Ela é esquecida ao entrar — e o link colado também.
  assert.equal(await page.evaluate(() => aceitePendente?.credencial ?? null), null,
    'a credencial da tentativa continuou guardada depois de a pessoa entrar: um convite de uso único já gasto ficou na memória da janela');
  assert.equal(await campoDoLink(page), '',
    'o link colado, com o convite dentro, ficou no campo do diálogo de servidores depois de a pessoa entrar');

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
    assert.equal(await campoDoLink(page), '',
      `desistir por ${porta} deixou o link colado, com o convite dentro, no campo do diálogo de servidores`);
  }

  // ---- 4b. Um link que o `analisar_convite` recusa fica no campo.
  //
  // Ele não inicia tentativa nenhuma e nada sai com ele; mantê-lo deixa a
  // pessoa corrigir o erro de digitação em vez de colar tudo de novo. O que o
  // campo não guarda é o link que **seguiu** para uma tentativa: os casos de
  // entrar e de desistir, acima, continuam conferindo o campo vazio.
  const LINK_QUEBRADO = `seele://${ALVO_R}?token=erro-de-digitacao`;
  const conectsAntesDoQuebrado = (await conectsFeitos(page)).length;
  await page.evaluate(() => {
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    return abrirServidores();
  });
  await page.fill('#servidores-endereco', LINK_QUEBRADO);
  await page.press('#servidores-endereco', 'Enter');
  await page.waitForFunction(() => !document.getElementById('servidores-erro').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('o link com erro de digitação não foi recusado no diálogo, e este caso não mede nada'));
  assert.equal(await campoDoLink(page), LINK_QUEBRADO,
    'o link que o analisar_convite recusou sumiu do campo: ele não iniciou tentativa nenhuma, e a pessoa tem de colar tudo de novo em vez de corrigir o erro de digitação');
  assert.equal((await conectsFeitos(page)).length, conectsAntesDoQuebrado,
    'o link recusado pelo analisar_convite chegou a um connect');
  await page.evaluate(() => fecharServidores());

  // ---- 4c. Um link de outra linha segue para a tentativa dela, e o campo
  // fica vazio.
  //
  // É a outra saída que leva o link adiante: `entrarPelaLinha` abre o SEELE
  // daquela linha com o link (`abrir_versao`), e o convite dentro dele não
  // pode ficar escrito no campo desta janela, que continua aberta.
  const LINK_DE_OUTRA_LINHA = 'seele://192.0.2.9:8384?token=convite-de-outra-linha&v=outra-linha';
  const conectsAntesDaOutraLinha = (await conectsFeitos(page)).length;
  await page.evaluate(() => {
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    return abrirServidores();
  });
  await page.fill('#servidores-endereco', LINK_DE_OUTRA_LINHA);
  await page.press('#servidores-endereco', 'Enter');
  await page.waitForFunction(() => !document.getElementById('versoes-confirmar').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('o link de outra linha não chegou à pergunta de abrir a outra versão, e este caso não mede nada'));
  await page.click('#versoes-confirmar-sim');
  await page.waitForFunction(() => testCalls.some(c => c.cmd === 'abrir_versao'), null, { timeout: 3000 })
    .catch(() => assert.fail('o link de outra linha não chegou ao abrir_versao'));
  assert.equal(await page.evaluate(() => testCalls.filter(c => c.cmd === 'abrir_versao').at(-1).args.link), LINK_DE_OUTRA_LINHA,
    'o abrir_versao não recebeu o link colado, e este caso não mede o caminho de verdade');
  assert.equal(await campoDoLink(page), '',
    'o link de outra linha seguiu para o SEELE daquela linha e ficou escrito no campo do diálogo de servidores desta janela, com o convite dentro');
  assert.equal((await conectsFeitos(page)).length, conectsAntesDaOutraLinha,
    'o link de outra linha chegou a um connect desta janela');
  await page.evaluate(() => { $('versoes-confirmar').hidden = true; fecharServidores(); });

  // ---- 5. Desistir enquanto o MOD chega também esquece, e não entra.
  //
  // A obtenção leva tempo, e a pergunta continua aberta durante ela: RECUSAR,
  // Escape e clicar fora seguem à mão. Quem desiste nesse meio não pode ver a
  // janela entrar sozinha quando o pacote termina de chegar — «sair da
  // pergunta não entra em lugar nenhum» — e muito menos com a credencial que
  // desistir esqueceu.
  //
  // **E o sim não fica gravado** (lote W da F5, ADR 0045). Ele era gravado
  // antes da obtenção: a volta não saía, mas o `aceitar_mods` já tinha valido,
  // e na entrada seguinte neste servidor ninguém perguntava e o MOD rodava —
  // contra o «não» que a pessoa deu por último. O sim da cena 1 é apagado
  // antes, para esta medir só o desta pergunta.
  await page.evaluate((alvo) => {
    delete testAceites[alvo];
    testTable.mods_instalados = []; testAtraso.instalar_mod_do_catalogo = 600;
  }, ALVO_R);
  await baterComOConvite(page);
  const antesDoAceite = (await conectsFeitos(page)).length;
  const conferenciasAntes = await page.evaluate(() => testCalls.filter(c => c.cmd === 'mods_instalados').length);
  // A cópia que `aceitarOsMods` guarda da pergunta (`pergunta`) enquanto o
  // catálogo responde: a bancada guarda a mesma referência, para ver o que ela
  // ainda alcança depois de a pessoa desistir.
  await page.evaluate(() => { window.testPergunta = aceitePendente; });
  await page.click('#mods-aceitar');
  await page.waitForFunction(() => testCalls.some(c => c.cmd === 'instalar_mod_do_catalogo'), null, { timeout: 3000 });
  await page.click('#mods-recusar');
  // Ainda durante a obtenção: o catálogo não respondeu, e a cópia continua viva.
  assert.equal(await page.evaluate(() => window.testPergunta?.credencial ?? null), null,
    'desistir enquanto o catálogo responde soltou a pergunta, mas a cópia que `aceitarOsMods` guarda dela continua com a credencial — alcançável enquanto o catálogo trava');
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
  assert.equal(await page.evaluate((alvo) => testAceites[alvo] ?? null, ALVO_R), null,
    'a pessoa desistiu enquanto o MOD chegava, e o sim dos MODs ficou gravado: na próxima entrada neste servidor ' +
    'ninguém pergunta e o MOD roda, contra o «não» que ela deu por último (ADR 0045)');
  await page.evaluate((m) => { testAtraso = {}; testTable.mods_instalados = [{ id: m.id, hash: m.hash }]; }, MOD_R);

  // ---- 5c. Desistir enquanto o sim é gravado também o desfaz.
  //
  // Com o MOD já nesta máquina não há obtenção, e a única espera depois do
  // ACEITAR é a do `aceitar_mods` (aqui, 500 ms). Quem fecha a pergunta nesse
  // meio desistiu do mesmo jeito, e o sim que terminou de ser gravado depois
  // disso não pode ficar valendo.
  await page.evaluate((alvo) => { delete testAceites[alvo]; testAtraso.aceitar_mods = 500; }, ALVO_R);
  await baterComOConvite(page);
  const antesDaGravacao = (await conectsFeitos(page)).length;
  const gravacoesAntes = await page.evaluate(() => testCalls.filter(c => c.cmd === 'aceitar_mods').length);
  await page.click('#mods-aceitar');
  await page.waitForFunction(
    (antes) => testCalls.filter(c => c.cmd === 'aceitar_mods').length > antes,
    gravacoesAntes, { timeout: 3000 },
  ).catch(() => assert.fail('ACEITAR não chegou a gravar o sim, e desistir durante a gravação não foi medido'));
  await page.click('#mods-recusar');
  await page.waitForTimeout(900);
  assert.equal(await page.evaluate((alvo) => testAceites[alvo] ?? null, ALVO_R), null,
    'a pessoa desistiu enquanto o sim dos MODs era gravado, e ele ficou valendo: na próxima entrada neste servidor ' +
    'ninguém pergunta e o MOD roda, contra o «não» que ela deu por último (ADR 0045)');
  assert.equal((await conectsFeitos(page)).length, antesDaGravacao,
    'a pessoa fechou a pergunta enquanto o sim era gravado, e a janela entrou mesmo assim');
  await page.evaluate(() => { testAtraso = {}; });

  // ---- 5d. E se desfazer o sim falhar, a tela diz (rodada 1, M2).
  //
  // A sonda P2 da revisão: o mesmo caso da 5c, com o `esquecer_aceite_de_mods`
  // falhando. O sim fica gravado contra o «não», e a falha ia só ao console.
  // A tela tem de dizer que o sim ficou e onde desfazê-lo.
  await page.evaluate((alvo) => {
    delete testAceites[alvo];
    testAtraso.aceitar_mods = 500;
    testFalhas.esquecer_aceite_de_mods = { Recusado: { motivo: 'o-disco-recusou' } };
  }, ALVO_R);
  await baterComOConvite(page);
  const gravacoesAntesDaFalha = await page.evaluate(() => testCalls.filter(c => c.cmd === 'aceitar_mods').length);
  await page.click('#mods-aceitar');
  await page.waitForFunction(
    (antes) => testCalls.filter(c => c.cmd === 'aceitar_mods').length > antes,
    gravacoesAntesDaFalha, { timeout: 3000 },
  ).catch(() => assert.fail('ACEITAR não chegou a gravar o sim, e a falha de desfazê-lo não foi medida'));
  await page.click('#mods-recusar');
  await page.waitForTimeout(900);
  assert.equal(await page.evaluate((alvo) => testAceites[alvo] ?? null, ALVO_R), 'conjunto-do-lote-r',
    'o sim não ficou gravado, e a falha de desfazê-lo não foi medida');
  const disse = await page.evaluate(() => {
    const linha = document.getElementById('boot-erro');
    return linha.hidden ? null : linha.textContent;
  });
  assert.ok(disse && disse.includes('FICOU GRAVADO') && disse.includes(ALVO_R) && disse.includes('SERVIDORES A QUEM VOCÊ DISSE SIM'),
    'a pessoa recusou enquanto o sim dos MODs era gravado, desfazê-lo falhou, e a tela não disse que o sim ficou nem onde ' +
    `desfazê-lo — a entrada diz ${JSON.stringify(disse)}: na próxima entrada o MOD roda contra o «não» dela, e ela não sabe`);
  await page.evaluate((alvo) => { testAtraso = {}; testFalhas = {}; delete testAceites[alvo]; }, ALVO_R);

  // ---- 5b. Uma pergunta nova que substitui a antiga durante a obtenção.
  //
  // A revisão da rodada 1 mediu (`troca-r.cjs`): com uma batida nova enquanto
  // o MOD da antiga chega, `abrirAceiteDeMods` troca `aceitePendente`, e a
  // cópia que a obtenção antiga guarda (`pergunta`) ainda alcança a credencial
  // de A até a obtenção acabar.
  const CONVITE_B = `${CONVITE_R}-da-segunda-batida`;
  await page.evaluate(() => { testTable.mods_instalados = []; testAtraso.instalar_mod_do_catalogo = 800; });
  await baterComOConvite(page);
  await page.evaluate(() => { window.testPergunta = aceitePendente; });
  const obtencoesAntes = await page.evaluate(() => testCalls.filter(c => c.cmd === 'instalar_mod_do_catalogo').length);
  await page.click('#mods-aceitar');
  await page.waitForFunction(
    (antes) => testCalls.filter(c => c.cmd === 'instalar_mod_do_catalogo').length > antes,
    obtencoesAntes, { timeout: 3000 },
  );
  // A segunda batida, a outro servidor, com outro segredo, durante a obtenção.
  await page.evaluate((convite) => {
    window.testOutroConvite = convite;
    window.testServidor = { aceitou: false, gasto: false };
    irParaOServidor('192.0.2.8:8384', null, convite);
  }, CONVITE_B);
  await page.waitForFunction(() => aceitePendente && aceitePendente !== window.testPergunta, null, { timeout: 3000 })
    .catch(() => assert.fail('a segunda batida não abriu uma pergunta nova, e a troca não foi medida'));
  assert.equal(await credencialGuardada(page), CONVITE_B,
    'a pergunta nova não guardou a credencial da segunda batida, e esta cena não mede nada');
  assert.equal(await page.evaluate(() => window.testPergunta?.credencial ?? null), null,
    'uma pergunta nova substituiu a antiga, e a cópia que a obtenção antiga guarda ainda alcança a credencial da primeira batida');
  // A obtenção antiga termina sem voltar (a pergunta dela já não é a aberta), e
  // a nova é recusada: a página volta ao estado de antes.
  await page.waitForTimeout(1000);
  await page.click('#mods-recusar');
  await page.evaluate((m) => {
    testAtraso = {}; window.testOutroConvite = null;
    testTable.mods_instalados = [{ id: m.id, hash: m.hash }];
  }, MOD_R);

  // ---- 6. A credencial fica presa ao alvo da batida, e não ao último alvo da
  // janela.
  //
  // `hospedar()` reescreve o `ultimoAlvo`, e HOSPEDAR AQUI não fica apagado
  // enquanto um `connect` anda. A revisão mediu (`corrida-r.cjs`, rodada 1):
  // com o `ultimoAlvo` trocado no meio do `connect` de A, a pergunta guardava o
  // alvo novo, e a volta levava o convite de A a outro servidor. Aqui o
  // `connect` demora 400 ms, e a troca acontece aos 100.
  await page.evaluate(([alvo, convite]) => {
    window.testServidor = { aceitou: false, gasto: false };
    testAtraso.connect = 400;
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    irParaOServidor(alvo, null, convite);
    // O que `hospedar()` faz no meio: `ultimoAlvo = anfitriao.aqui`.
    setTimeout(() => { ultimoAlvo = '127.0.0.1:9999'; }, 100);
  }, [ALVO_R, CONVITE_R]);
  await page.waitForFunction(() => !document.getElementById('mods-aceite').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('a batida da corrida não chegou à pergunta dos MODs, e a corrida não foi medida'));
  await page.evaluate(() => { testAtraso = {}; });
  const antesDaCorrida = (await conectsFeitos(page)).length;
  await page.click('#mods-aceitar');
  await page.waitForFunction((n) => testCalls.filter(c => c.cmd === 'connect').length > n, antesDaCorrida, { timeout: 3000 });
  const voltaDaCorrida = (await conectsFeitos(page)).at(-1);
  assert.equal(voltaDaCorrida.server, ALVO_R,
    `com o último alvo da janela trocado no meio da batida, a volta que aceita os MODs foi a ${voltaDaCorrida.server}` +
    (voltaDaCorrida.joinSecret === CONVITE_R ? `, levando o convite de ${ALVO_R}` : '') +
    ': a pergunta amarrou a credencial ao alvo da janela, e não ao da batida');
  const simGravado = await page.evaluate(() => testCalls.filter(c => c.cmd === 'aceitar_mods').at(-1)?.args?.alvo);
  assert.equal(simGravado, ALVO_R,
    `o sim dos MODs foi gravado para ${simGravado}, e não para o servidor que perguntou`);

  // ---- 7. Depois de tudo isso, a credencial não está em lugar nenhum além do
  // `connect`: nem no armazenamento da página, nem no DOM, nem no console, nem
  // em outro comando — que é por onde ela chegaria a um arquivo.
  const vazou = await page.evaluate((convite) => {
    const onde = [];
    const guardado = (loja) => Object.keys(loja).map((k) => `${k}=${loja.getItem(k)}`).join('\n');
    if (guardado(localStorage).includes(convite)) onde.push('localStorage');
    if (guardado(sessionStorage).includes(convite)) onde.push('sessionStorage');
    if (document.documentElement.outerHTML.includes(convite)) onde.push('o DOM');
    if ([...document.querySelectorAll('input, textarea')].some((campo) => campo.value.includes(convite))) onde.push('um campo');
    if (testConsole.some((linha) => linha.includes(convite))) onde.push('o console');
    // O `connect` a leva de propósito, e o `analisar_convite` recebe o link de
    // onde ela sai: é a leitura dele, e não um caminho dela.
    for (const chamada of testCalls) {
      if (chamada.cmd === 'connect' || chamada.cmd === 'analisar_convite') continue;
      if (JSON.stringify(chamada.args ?? null).includes(convite)) onde.push(`o comando ${chamada.cmd}`);
    }
    return onde;
  }, CONVITE_R);
  assert.deepEqual(vazou, [],
    `a credencial da tentativa apareceu fora do connect: ${vazou.join(', ')}. Ela é um convite de uso único ou a senha do servidor, e só vive na memória da pergunta dos MODs`);

  assert.deepEqual(erros, [], 'a cena do lote R lançou erro na página');
  await page.close();
}

// --------------------------------------------------------------------------
// Lote W da F5: o apelido tomado e a tentativa seguinte
// --------------------------------------------------------------------------
//
// **O caso.** Um servidor fechado por convite, sem portaria, onde o apelido
// desta máquina já é de outra chave. A frase diz «Não é o convite nem a senha:
// escolha outro apelido e entre de novo», e o servidor guarda o convite
// intacto (`seele-server/src/session.rs`, «o convite ficou intacto»). A pessoa
// troca o apelido no perfil e tenta de novo.
//
// **O servidor de mentira confere na ordem do de verdade**: o segredo, depois
// o apelido, e só gasta o convite na entrada.

const CONVITE_W = `convite-do-lote-w-${process.pid}-${Date.now()}`;
const ALVO_W = '192.0.2.17:8384';
const OUTRO_ALVO_W = '192.0.2.18:8384';
/** O servidor que esta janela levanta ao hospedar. */
const AQUI_W = '127.0.0.1:8383';
/** Um convite novo ao mesmo servidor, num link novo, que quem hospeda mandou depois. */
const CONVITE_NOVO_W = `${CONVITE_W}-novo`;
const LINK_NOVO_W = `seele://${ALVO_W}?token=${CONVITE_NOVO_W}`;
const LINK_W = `seele://${ALVO_W}?token=${CONVITE_W}`;

/**
 * A credencial que o apelido tomado deixou para a tentativa seguinte, ou
 * `null`.
 *
 * `recusaDoApelido` (`tela-boot.js`) é o único lugar onde ela mora entre a
 * recusa e a tentativa seguinte. Lida pelo nome, como `credencialGuardada` lê
 * a pergunta dos MODs; antes do lote W o lugar não existia, e a resposta é
 * `null`.
 */
function credencialDoApelido(page) {
  return page.evaluate(() => (typeof recusaDoApelido === 'undefined' ? null : recusaDoApelido?.credencial ?? null));
}

/**
 * Uma tentativa pelo campo do diálogo de servidores — o link colado ou o
 * endereço digitado —, esperando a resposta dela: a linha vermelha da entrada,
 * a `#tela-auth` ou a pergunta dos MODs.
 */
async function tentarPeloCampo(page, escrito) {
  const antes = await page.evaluate(() => testCalls.filter(c => c.cmd === 'connect').length);
  await page.evaluate(() => {
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    return abrirServidores();
  });
  await page.fill('#servidores-endereco', escrito);
  await page.press('#servidores-endereco', 'Enter');
  await page.waitForFunction(
    (n) => testCalls.filter(c => c.cmd === 'connect').length > n
      && (!document.getElementById('boot-erro').hidden || !document.getElementById('tela-auth').hidden
        || !document.getElementById('mods-aceite').hidden),
    antes, { timeout: 3000 },
  ).catch(() => assert.fail(`a tentativa por «${escrito}» não chegou a uma resposta`));
  return page.evaluate(() => testCalls.filter(c => c.cmd === 'connect').at(-1).args);
}

/** Troca o apelido pelo perfil da entrada, como a pessoa faz. */
async function trocarOApelido(page, novo) {
  await page.click('#boot-perfil');
  await page.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 });
  await page.fill('#perfil-apelido', novo);
  await page.press('#perfil-apelido', 'Enter');
  await page.waitForFunction(() => document.getElementById('perfil').hidden, null, { timeout: 3000 });
  assert.equal(await page.evaluate(() => testTable.apelido_local), novo,
    'o perfil da entrada não gravou o apelido novo, e esta cena não mede a troca');
}

async function oApelidoTomadoNaoPerdeATentativa(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, convite, novo, alvo, aqui, mod, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    window.testServidor = { gasto: false, tomados: ['Bia'], exigeMods: false };
    // Um `connect` lento por alvo, para as corridas: o do servidor daqui, que
    // o `hospedar` levanta, não espera.
    window.testAtrasoPorAlvo = {};
    // O console inteiro, com os argumentos serializados, como na cena do R.
    window.testConsole = [];
    for (const nivel of ['log', 'info', 'warn', 'error', 'debug']) {
      const original = console[nivel].bind(console);
      console[nivel] = (...args) => {
        try { window.testConsole.push(JSON.stringify(args)); } catch { window.testConsole.push(String(args)); }
        original(...args);
      };
    }
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        window.testCalls.push({ cmd, args });
        if (cmd === 'escolher_apelido_local') window.testTable.apelido_local = args.apelido;
        // O link novo: o `analisar_convite` devolve o token dele.
        if (cmd === 'analisar_convite' && String(args?.link).includes(novo)) return { alvo, token: novo, para_entrar: null };
        if (cmd === 'connect') {
          const servidor = window.testServidor;
          const recusa = (error) => { throw { error, trail: [] }; };
          const atraso = window.testAtrasoPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          // O servidor que esta janela acabou de levantar: entra.
          if (args.server === aqui) return { snapshot, veredito: null, geracao: window.testCalls.length };
          // Outro servidor, também fechado, para o qual esta máquina não tem
          // convite nenhum.
          if (args.server !== alvo) recusa({ Refused: { reason: 'CredentialRejected' } });
          if (![convite, novo].includes(args.joinSecret) || servidor.gasto) recusa({ Refused: { reason: 'CredentialRejected' } });
          if (servidor.tomados.includes(args.nickname)) recusa({ Refused: { reason: 'NicknameTaken' } });
          if (servidor.exigeMods) recusa({ ModsNaoAceitos: { mods: [mod], conjunto: 'conjunto-do-lote-w' } });
          servidor.gasto = true;
          return {
            snapshot,
            veredito: { FirstContactVerified: { fingerprint: 'ef'.repeat(32) } },
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
      analisar_convite: { alvo: ALVO_W, token: CONVITE_W, para_entrar: null },
      // A forma de `Anfitriao` no Rust, como a do duble de `carga-da-casca.py`.
      hospedar: {
        aqui: AQUI_W, convite: `seele://${AQUI_W}?fp=ab`, alcance: 'SoRedeLocal',
        porta_recusada: null, encontro_recusado: null,
      },
    }),
    convite: CONVITE_W, novo: CONVITE_NOVO_W, alvo: ALVO_W, aqui: AQUI_W, mod: MOD_R, snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  // ---- 1. O link colado, com o apelido desta máquina já tomado.
  const primeira = await tentarPeloCampo(page, LINK_W);
  assert.equal(primeira.joinSecret, CONVITE_W, 'a primeira tentativa não levou o convite do link, e esta cena não mede nada');
  assert.ok((await page.evaluate(() => $('boot-erro').textContent)).includes('«Bia» JÁ É DE OUTRA PESSOA'),
    `a primeira tentativa não foi recusada pelo apelido — a entrada diz «${await page.evaluate(() => $('boot-erro').textContent)}»`);

  // ---- 2. A troca de apelido, pelo perfil da entrada.
  await trocarOApelido(page, 'Bia2');

  // ---- 3. A tentativa seguinte, ao mesmo servidor, sem colar o link de novo.
  //
  // O link já não está em lugar nenhum da página — o campo é zerado quando ele
  // segue para uma tentativa (lote R) —, e a pessoa digita o endereço. O
  // convite que o apelido tomado não gastou vai junto (I-4, lote W).
  const segunda = await tentarPeloCampo(page, ALVO_W);
  assert.equal(segunda.nickname, 'Bia2',
    `a pessoa trocou o apelido no perfil e a tentativa seguinte mandou «${segunda.nickname}»: o apelido recusado ficou ` +
    'lembrado, o perfil não foi lido, e a frase que manda escolher outro apelido e entrar de novo não tem como ser cumprida');
  assert.equal(segunda.joinSecret ?? null, CONVITE_W,
    `a tentativa seguinte ao mesmo servidor saiu sem o convite (joinSecret ${JSON.stringify(segunda.joinSecret ?? null)}): ` +
    'o servidor o guardou intacto e a frase diz que não é o convite, e a tela acaba dizendo CREDENCIAL RECUSADA sobre um convite válido');
  await page.waitForFunction(() => !document.getElementById('tela-auth').hidden, null, { timeout: 3000 })
    .catch(async () => assert.fail(`com o apelido novo e o convite, a tentativa seguinte não entrou — a entrada diz «${await page.evaluate(() => $('boot-erro').textContent)}»`));
  assert.equal(await credencialDoApelido(page), null,
    'a credencial que o apelido tomado deixou continuou guardada depois de a pessoa entrar: um convite de uso único já gasto ficou na memória da janela');

  // ---- 3b. Uma tentativa ao mesmo servidor com credencial própria leva a
  // dela, e não a guardada (rodada 1, M6).
  //
  // Depois da recusa, quem hospeda manda um link novo, e a pessoa o cola: o
  // convite que vale é o do link, e não o que a recusa anterior deixou. É o
  // `??=` de `conectar` — com `=`, a guardada passava por cima da do link.
  await page.evaluate(() => { testServidor.gasto = false; testServidor.tomados = [testTable.apelido_local]; });
  await tentarPeloCampo(page, LINK_W);
  assert.equal(await credencialDoApelido(page), CONVITE_W,
    'recusada pelo apelido, a tentativa não deixou a credencial, e este caso não mede nada');
  const comLinkNovo = await tentarPeloCampo(page, LINK_NOVO_W);
  assert.equal(comLinkNovo.server, ALVO_W, 'o link novo não foi ao mesmo servidor, e este caso não mede nada');
  assert.equal(comLinkNovo.joinSecret ?? null, CONVITE_NOVO_W,
    `uma tentativa ao mesmo servidor com um link novo levou ${JSON.stringify(comLinkNovo.joinSecret ?? null)}, e não o convite ` +
    'do link: a credencial que a recusa anterior deixou passou por cima da que a pessoa acabou de colar');

  // ---- 4. Uma tentativa a outro servidor não a leva, e a esquece.
  await page.evaluate(() => { testServidor.gasto = false; testServidor.tomados = ['Bia2']; });
  await tentarPeloCampo(page, LINK_W);
  assert.equal(await credencialDoApelido(page), CONVITE_W,
    'recusada pelo apelido, a tentativa não deixou a credencial para a seguinte, e este caso não mede nada');
  const outro = await tentarPeloCampo(page, OUTRO_ALVO_W);
  assert.equal(outro.server, OUTRO_ALVO_W, 'a tentativa ao outro servidor não foi a ele, e este caso não mede nada');
  assert.equal(outro.joinSecret ?? null, null,
    `uma tentativa a ${OUTRO_ALVO_W} levou o convite de ${ALVO_W}: a credencial que o apelido tomado deixou é daquele servidor, e foi mandada a quem não a emitiu`);
  const deVolta = await tentarPeloCampo(page, ALVO_W);
  assert.equal(deVolta.joinSecret ?? null, null,
    'depois de uma tentativa a outro servidor, a volta a este levou o convite: trocar de alvo devia tê-lo esquecido');

  // ---- 5. A tentativa seguinte para na pergunta dos MODs, e a pessoa desiste.
  //
  // A credencial **muda de lugar**: a tentativa a tira do lugar do apelido
  // tomado ao começar (parte 6 do guarda) e, parando na pergunta dos MODs, a
  // entrega à pergunta (lote R); RECUSAR E VOLTAR a esquece de lá. Cada
  // asserção mede o passo onde ele acontece (rodada 1, M6): a do lugar do
  // apelido logo depois da tentativa — o RECUSAR não mexe nele, e conferi-lo
  // depois do RECUSAR atribuía a ele o que a tentativa fez —, e a da pergunta
  // depois do RECUSAR.
  await tentarPeloCampo(page, LINK_W);
  assert.equal(await credencialDoApelido(page), CONVITE_W,
    'recusada pelo apelido outra vez, a tentativa não deixou a credencial, e a desistência não foi medida');
  await trocarOApelido(page, 'Bia3');
  await page.evaluate(() => { testServidor.exigeMods = true; });
  const comMods = await tentarPeloCampo(page, ALVO_W);
  assert.equal(comMods.joinSecret ?? null, CONVITE_W, 'a tentativa seguinte não levou o convite, e a desistência não foi medida');
  assert.equal(await page.evaluate(() => document.getElementById('mods-aceite').hidden), false,
    'a tentativa seguinte não parou na pergunta dos MODs, e a desistência não foi medida');
  assert.equal(await credencialGuardada(page), CONVITE_W,
    'a pergunta dos MODs não recebeu a credencial que a tentativa levou: aceitar voltaria sem ela');
  assert.equal(await credencialDoApelido(page), null,
    'a tentativa que parou na pergunta dos MODs deixou a credencial também no lugar do apelido tomado: ela passa à ' +
    'pergunta, e não fica em dois lugares — desistir da pergunta não a esqueceria do outro');
  await page.click('#mods-recusar');
  await page.waitForFunction(() => document.getElementById('mods-aceite').hidden, null, { timeout: 3000 });
  assert.equal(await credencialGuardada(page), null, 'desistir da pergunta dos MODs deixou a credencial guardada nela');
  await page.evaluate(() => { testServidor.exigeMods = false; });
  const depoisDeDesistir = await tentarPeloCampo(page, ALVO_W);
  assert.equal(depoisDeDesistir.joinSecret ?? null, null,
    'depois de a pessoa desistir da pergunta dos MODs, a tentativa seguinte ainda levou o convite: ele voltou a um ' +
    'dos dois lugares, que desistir devia ter deixado vazios');

  // ---- 5b. Uma tentativa mais velha, recusada pelo apelido depois que outra
  // entrou, não deixa a credencial (rodada 1, M1).
  //
  // A sonda P1 da revisão: uma tentativa a este servidor, com o convite, leva
  // 600 ms e volta com `NicknameTaken`; aos 100 ms, `hospedar()` entra no
  // servidor daqui. A recusa chega com a janela já dentro de outro servidor,
  // e guardava a credencial para «a tentativa seguinte» — que já tinha
  // acontecido.
  await page.evaluate(([alvo, convite]) => {
    testServidor.gasto = false; testServidor.tomados = [testTable.apelido_local];
    testAtrasoPorAlvo[alvo] = 600;
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    window.testCorrida = { antes: testCalls.filter(c => c.cmd === 'connect').length };
    irParaOServidor(alvo, null, convite);
    setTimeout(() => { window.testCorrida.hospedou = hospedar(); }, 100);
  }, [ALVO_W, CONVITE_W]);
  await page.waitForFunction(() => testCalls.filter(c => c.cmd === 'connect').length >= testCorrida.antes + 2, null, { timeout: 3000 })
    .catch(() => assert.fail('a corrida não chegou aos dois connect, e não foi medida'));
  await page.evaluate(() => testCorrida.hospedou);
  await page.waitForTimeout(800);
  const naCorrida = await page.evaluate(() => testCalls.filter(c => c.cmd === 'connect').slice(testCorrida.antes).map(c => [c.args.server, c.args.joinSecret ?? null]));
  assert.deepEqual(naCorrida, [[ALVO_W, CONVITE_W], [AQUI_W, null]],
    `a corrida não foi a esperada (o convite a ${ALVO_W}, e depois o servidor daqui sem nada): ${JSON.stringify(naCorrida)}`);
  assert.equal(await page.evaluate(() => document.getElementById('tela-sessao').hidden && document.getElementById('tela-auth').hidden), false,
    'o hospedar do meio da corrida não entrou, e a corrida não foi medida');
  assert.equal(await credencialDoApelido(page), null,
    `uma tentativa a ${ALVO_W}, recusada pelo apelido depois que o hospedar já tinha entrado no servidor daqui, deixou a ` +
    'credencial guardada para uma «tentativa seguinte» que já tinha acontecido: o convite fica na memória da janela durante a sessão inteira');
  await page.evaluate((alvo) => { delete testAtrasoPorAlvo[alvo]; }, ALVO_W);

  // ---- 6. A credencial não está em lugar nenhum além do `connect`.
  const vazou = await page.evaluate((convite) => {
    const onde = [];
    const guardado = (loja) => Object.keys(loja).map((k) => `${k}=${loja.getItem(k)}`).join('\n');
    if (guardado(localStorage).includes(convite)) onde.push('localStorage');
    if (guardado(sessionStorage).includes(convite)) onde.push('sessionStorage');
    if (document.documentElement.outerHTML.includes(convite)) onde.push('o DOM');
    if ([...document.querySelectorAll('input, textarea')].some((campo) => campo.value.includes(convite))) onde.push('um campo');
    if (testConsole.some((linha) => linha.includes(convite))) onde.push('o console');
    for (const chamada of testCalls) {
      if (chamada.cmd === 'connect' || chamada.cmd === 'analisar_convite') continue;
      if (JSON.stringify(chamada.args ?? null).includes(convite)) onde.push(`o comando ${chamada.cmd}`);
    }
    return onde;
  }, CONVITE_W);
  assert.deepEqual(vazou, [],
    `a credencial que o apelido tomado deixou apareceu fora do connect: ${vazou.join(', ')}. Ela é um convite de uso único ou a senha do servidor, e só vive na memória da janela`);

  assert.deepEqual(erros, [], 'a cena do lote W lançou erro na página');
  await page.close();
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
    await oApelidoTomadoNaoPerdeATentativa(browser, server.address().port);

    console.log('v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium');
    console.log(`lote J: a faixa de contexto acompanha a tecla e o modo, a entrada por link pergunta o nome antes do connect, e o título da pergunta não se sobrepõe (folga de ${titulo.folga}px entre as tintas)`);
    console.log('lote R: a volta que aceita os MODs leva a credencial da tentativa e entra, e o RECONECTAR de uma sessão que entrou vai sem ela');
    console.log('lote R: desistir da pergunta (RECUSAR, Escape, clicar fora, ou no meio da obtenção) esquece a credencial e não entra');
    console.log('lote R: com o último alvo trocado no meio da batida, a volta e o sim vão ao servidor que perguntou');
    console.log('lote R: a credencial não aparece no armazenamento, no DOM, no console nem em comando nenhum além do connect');
    console.log('lote W: desistir no meio da obtenção ou da gravação não deixa o sim dos MODs valendo');
    console.log('lote W: depois do apelido tomado, a tentativa seguinte ao mesmo servidor leva o apelido novo e o convite, e entra; a outro servidor, não leva nada');
    console.log('lote W: entrar, trocar de alvo e desistir da pergunta dos MODs esquecem a credencial, e ela não aparece fora do connect');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
