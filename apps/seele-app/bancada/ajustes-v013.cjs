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
  // E a obtenção que terminou depois do «não» disse ao comando que é obtenção
  // (lote X da F5, ADR 0045): só a atualização pela gestão reaplica o MOD no
  // servidor que esta janela hospeda. Toda obtenção desta bancada é da pergunta.
  const usos = await page.evaluate(() =>
    testCalls.filter(c => c.cmd === 'instalar_mod_do_catalogo').map(c => c.args?.uso ?? null));
  assert.ok(usos.length > 0 && usos.every(uso => uso === 'obter'),
    'a obtenção da pergunta dos MODs, que segue até o fim mesmo depois de a pessoa desistir, não diz ao comando que é ' +
    `obtenção (usos: ${JSON.stringify(usos)}): no servidor que esta janela hospeda, a versão exigida troca pela deste (ADR 0045)`);
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

  // ---- 5c. A mesma corrida, com a tentativa velha voltando com a pergunta dos
  // MODs (re-revisão da rodada 1, sonda P6).
  //
  // A tentativa a este servidor volta `ModsNaoAceitos` depois que o `hospedar`
  // já entrou no servidor daqui. A pergunta abria por cima da sessão, com o
  // convite deste servidor guardado nela. Uma tentativa que já não é a mais
  // nova não abre a pergunta — e, desde o lote W2, também não escreve na linha
  // vermelha da entrada: a recusa dela vai ao `seele.log`. Ver
  // `aTentativaSubstituidaNaoFala`, abaixo.
  await page.evaluate(([alvo, convite]) => {
    testServidor.gasto = false; testServidor.tomados = []; testServidor.exigeMods = true;
    testAtrasoPorAlvo[alvo] = 600;
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    window.testCorrida = { antes: testCalls.filter(c => c.cmd === 'connect').length };
    irParaOServidor(alvo, null, convite);
    setTimeout(() => { window.testCorrida.hospedou = hospedar(); }, 100);
  }, [ALVO_W, CONVITE_W]);
  await page.waitForFunction(() => testCalls.filter(c => c.cmd === 'connect').length >= testCorrida.antes + 2, null, { timeout: 3000 })
    .catch(() => assert.fail('a corrida com a pergunta dos MODs não chegou aos dois connect, e não foi medida'));
  await page.evaluate(() => testCorrida.hospedou);
  await page.waitForTimeout(800);
  const naCorridaDosMods = await page.evaluate(() => testCalls.filter(c => c.cmd === 'connect').slice(testCorrida.antes).map(c => [c.args.server, c.args.joinSecret ?? null]));
  assert.deepEqual(naCorridaDosMods, [[ALVO_W, CONVITE_W], [AQUI_W, null]],
    `a corrida com a pergunta dos MODs não foi a esperada: ${JSON.stringify(naCorridaDosMods)}`);
  assert.equal(await page.evaluate(() => document.getElementById('tela-sessao').hidden && document.getElementById('tela-auth').hidden), false,
    'o hospedar do meio da corrida não entrou, e a corrida com a pergunta dos MODs não foi medida');
  assert.equal(await credencialGuardada(page), null,
    `uma tentativa a ${ALVO_W}, que voltou com a pergunta dos MODs depois que o hospedar já tinha entrado no servidor daqui, ` +
    'guardou o convite na pergunta: ele fica na memória da janela durante a sessão, e aceitar o mandaria a um servidor que a pessoa já deixou');
  assert.equal(await page.evaluate(() => document.getElementById('mods-aceite').hidden), true,
    'a pergunta dos MODs de uma tentativa que outra já substituiu abriu por cima da tela de quem já entrou em outro servidor');
  assert.equal(await credencialDoApelido(page), null,
    'a corrida com a pergunta dos MODs deixou a credencial no lugar do apelido tomado');
  await page.evaluate((alvo) => { delete testAtrasoPorAlvo[alvo]; testServidor.exigeMods = false; }, ALVO_W);

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

// --------------------------------------------------------------------------
// Lote W2 da F5: a tentativa que outra substituiu não fala
// --------------------------------------------------------------------------
//
// **O caso.** Uma tentativa de entrada que outra já substituiu — o `hospedar()`
// entrando no servidor daqui enquanto ela andava, ou a pessoa escolhendo outro
// servidor na lista — volta recusada. A mais nova está em curso, ou já entrou,
// e é ela que fala. A recusa da velha não vai à linha vermelha da entrada, nem
// à espera da portaria; vai ao `seele.log`, com o motivo.
//
// **O que acontecia.** A recusa pelos MODs da velha caía na linha vermelha
// terminando em «A tela para ler e aceitar esta lista ainda não existe neste
// app.», falsa desde que a tela existe; com a espera na frente, a mesma frase
// tomava o lugar da frase da espera; e a recusa da velha pela portaria abria a
// espera por cima da sessão em que a pessoa já estava.

/** Um servidor com portaria, onde a batida fica pendente. */
const PORTARIA_W2 = '192.0.2.19:8384';
/** O convite que a tentativa velha leva: ele não pode chegar ao registro. */
const CONVITE_W2 = `convite-do-lote-w2-${process.pid}-${Date.now()}`;

async function aTentativaSubstituidaNaoFala(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    // Por alvo: quanto o `connect` leva, e a recusa com que ele volta. Sem
    // recusa, entra. Lidos quando a chamada chega, e não quando ela volta: uma
    // tentativa lenta responde como o servidor dela respondia quando ela saiu.
    window.testAtrasoPorAlvo = {};
    window.testRecusaPorAlvo = {};
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        const chamada = { cmd, args };
        window.testCalls.push(chamada);
        if (cmd === 'connect') {
          const atraso = window.testAtrasoPorAlvo[args.server];
          const recusa = window.testRecusaPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          chamada.respondida = true;
          if (recusa) throw { error: recusa, trail: [] };
          return { snapshot, veredito: null, geracao: window.testCalls.length };
        }
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    table: respostas({
      apelido_local: 'Bia', snapshot: null, microfones: [], saidas: [],
      hospedar: {
        aqui: AQUI_W, convite: `seele://${AQUI_W}?fp=ab`, alcance: 'SoRedeLocal',
        porta_recusada: null, encontro_recusado: null,
      },
    }),
    snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  const PELOS_MODS = { ModsNaoAceitos: { mods: [MOD_R], conjunto: 'conjunto-do-lote-w2' } };
  const NA_PORTARIA = { Refused: { reason: 'AdmissionPending' } };
  const SEM_CREDENCIAL = { Refused: { reason: 'CredentialRejected' } };

  /**
   * A tentativa velha, com o convite, e aos 100 ms a que a substitui: o
   * `hospedar()` ou outra escolha na lista de servidores. Espera 900 ms — a
   * velha já voltou, e a nova, se é lenta, ainda anda.
   */
  async function correr(velha, nova) {
    await page.evaluate(([velha, nova, convite]) => {
      window.testAtrasoPorAlvo = {};
      window.testRecusaPorAlvo = {};
      for (const t of [velha, nova]) {
        if (t.atraso) window.testAtrasoPorAlvo[t.alvo] = t.atraso;
        if (t.recusa) window.testRecusaPorAlvo[t.alvo] = t.recusa;
      }
      for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
      $('tela-boot').hidden = false;
      window.testCorrida = { antes: testCalls.length };
      irParaOServidor(velha.alvo, null, convite);
      setTimeout(() => {
        if (nova.via === 'hospedar') hospedar();
        else irParaOServidor(nova.alvo, null, null);
      }, 100);
    }, [velha, nova, CONVITE_W2]);
    await page.waitForTimeout(900);
    const feitos = await page.evaluate(() => testCalls.slice(testCorrida.antes).filter(c => c.cmd === 'connect').map(c => c.args.server));
    assert.deepEqual(feitos, [velha.alvo, nova.alvo],
      `a corrida não foi a esperada (a velha a ${velha.alvo}, e a nova a ${nova.alvo}): ${JSON.stringify(feitos)}`);
  }

  /** As linhas que a recusa da velha deixou no `seele.log`, pelo nível. */
  const noRegistro = (alvo, motivo) => page.evaluate(([alvo, motivo]) => testCalls.slice(testCorrida.antes)
    .filter(c => c.cmd === 'registrar_da_janela' && String(c.args?.oQue).includes(alvo) && String(c.args?.oQue).includes(motivo))
    .map(c => c.args.nivel), [alvo, motivo]);
  const dizONoRegistro = async (alvo, motivo) => assert.deepEqual(await noRegistro(alvo, motivo), ['aviso'],
    `a recusa por ${motivo} da tentativa a ${alvo}, que outra já tinha substituído, não chegou ao seele.log como aviso: ` +
    'a tela não a mostra, e sem a linha ela some sem rastro — o produto sabe e não conta');

  // ---- 1. A recusa pelos MODs de uma tentativa que o hospedar substituiu
  // (a reprodução do achado).
  await correr({ alvo: ALVO_W, atraso: 600, recusa: PELOS_MODS }, { via: 'hospedar', alvo: AQUI_W });
  assert.equal(await page.evaluate(() => document.getElementById('tela-sessao').hidden && document.getElementById('tela-auth').hidden), false,
    'o hospedar do meio da corrida não entrou, e a corrida não foi medida');
  // A pessoa desconecta e volta à entrada: é ali que ela lê.
  await page.evaluate(() => ejetar());
  assert.equal(await page.isVisible('#tela-boot'), true, 'desconectar não voltou à entrada, e esta cena não mede o que se lê nela');
  const naEntrada = await page.evaluate(() => ($('boot-erro').hidden ? null : $('boot-erro').textContent));
  assert.equal(naEntrada, null,
    `a recusa pelos MODs de uma tentativa a ${ALVO_W}, que o hospedar já tinha substituído, foi escrita na linha vermelha ` +
    `da entrada, e quem desconecta lê ali, sobre um servidor que deixou: «${naEntrada}»`);
  await dizONoRegistro(ALVO_W, 'ModsNaoAceitos');

  // ---- 2. Duas tentativas pela lista: a velha volta enquanto a nova anda, e
  // é a nova que fala.
  await correr({ alvo: ALVO_W, atraso: 600, recusa: PELOS_MODS }, { via: 'lista', alvo: OUTRO_ALVO_W, atraso: 1400, recusa: SEM_CREDENCIAL });
  const enquantoANovaAnda = await page.evaluate(() => ($('boot-erro').hidden ? null : $('boot-erro').textContent));
  assert.equal(enquantoANovaAnda, null,
    `com a tentativa a ${OUTRO_ALVO_W} ainda andando, a recusa pelos MODs da anterior, a ${ALVO_W}, apareceu na linha ` +
    `vermelha da entrada: «${enquantoANovaAnda}»`);
  await dizONoRegistro(ALVO_W, 'ModsNaoAceitos');
  // **E o CONECTAR continua apagado** (lote W2, rodada da revisão). A nova o
  // apagou ao começar; o `finally` da velha, que terminou primeiro, o
  // reacendia, e um clique ali abria uma terceira entrada por baixo da nova.
  const estado = await page.evaluate((alvo) => ({
    velhaVoltou: testCalls.slice(testCorrida.antes).some(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida),
    apagado: $('botao-conectar').disabled,
  }), ALVO_W);
  assert.equal(estado.velhaVoltou, true, 'a tentativa velha ainda não tinha voltado, e o botão não foi medido depois dela');
  assert.equal(estado.apagado, true,
    `com a tentativa a ${OUTRO_ALVO_W} ainda andando, a anterior, a ${ALVO_W}, terminou e reacendeu o CONECTAR que a ` +
    'nova tinha apagado: um clique ali abre uma terceira entrada por baixo da que está em curso');
  await page.waitForFunction((alvo) => testCalls.slice(testCorrida.antes).some(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida),
    OUTRO_ALVO_W, { timeout: 3000 })
    .catch(() => assert.fail(`a tentativa a ${OUTRO_ALVO_W} não voltou, e esta cena não mede quem fala`));
  await page.waitForFunction(() => !document.getElementById('boot-erro').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail(`a tentativa a ${OUTRO_ALVO_W} foi recusada e a linha vermelha não disse nada`));
  const daNova = await page.evaluate(([recusa]) => ({
    escrita: $('boot-erro').textContent,
    titulo: fraseDeErro(recusa).split('\n')[0],
  }), [SEM_CREDENCIAL]);
  assert.ok(daNova.escrita.startsWith(daNova.titulo),
    `a linha vermelha não diz a recusa da tentativa mais nova («${daNova.titulo}»), e sim «${daNova.escrita}»`);
  assert.equal(await page.evaluate(() => $('botao-conectar').disabled), false,
    'a tentativa mais nova terminou e o CONECTAR ficou apagado: quem devolve o botão é ela');

  // ---- 3. A nova parou na espera da portaria, e a velha volta recusada pelos
  // MODs: a espera continua dizendo a espera.
  await correr({ alvo: ALVO_W, atraso: 600, recusa: PELOS_MODS }, { via: 'lista', alvo: PORTARIA_W2, recusa: NA_PORTARIA });
  const naEspera = await page.evaluate(([recusa]) => ({
    aberta: !$('tela-auth').hidden && $('tela-auth').dataset.modo === 'espera',
    frase: $('auth-espera-frase').textContent,
    esperada: fraseDeErro(recusa),
    botao: $('auth-botao').textContent,
  }), [NA_PORTARIA]);
  assert.equal(naEspera.aberta, true, 'a tentativa à portaria não parou na espera, e esta cena não mede nada');
  assert.equal(naEspera.frase, naEspera.esperada,
    `com a espera da portaria na frente, a recusa pelos MODs da tentativa anterior, a ${ALVO_W}, tomou o lugar da frase ` +
    `da espera: «${naEspera.frase}»`);
  assert.equal(naEspera.botao, 'AGUARDANDO PERMISSÃO',
    `a recusa da tentativa anterior trocou o estado da espera para «${naEspera.botao}»`);
  await dizONoRegistro(ALVO_W, 'ModsNaoAceitos');
  await page.evaluate(() => voltarParaAEntrada());

  // ---- 4. Não só pelos MODs: a velha recusada pela portaria depois que o
  // hospedar entrou não abre a espera por cima da sessão.
  await correr({ alvo: PORTARIA_W2, atraso: 600, recusa: NA_PORTARIA }, { via: 'hospedar', alvo: AQUI_W });
  const naFrente = await page.evaluate(() => (document.getElementById('tela-auth').hidden ? 'a sessão' : `a tela de entrada no modo ${$('tela-auth').dataset.modo}`));
  if (naFrente === 'a tela de entrada no modo espera') await page.evaluate(() => voltarParaAEntrada());
  assert.notEqual(naFrente, 'a tela de entrada no modo espera',
    `a tentativa a ${PORTARIA_W2}, pendente na portaria depois que o hospedar já tinha entrado no servidor daqui, abriu ` +
    'a espera por cima da sessão, e a espera bate de novo no servidor desta máquina');
  await dizONoRegistro(PORTARIA_W2, 'AdmissionPending');
  await page.evaluate(() => ejetar());

  // ---- 5. A frase dos MODs diz o que esta versão faz, e o que ela manda
  // fazer funciona.
  const frase = await page.evaluate(([recusa]) => fraseDeErro(recusa), [PELOS_MODS]);
  assert.ok(frase.includes(`${MOD_R.id} ${MOD_R.version}`), `a frase dos MODs deixou de mostrar a lista: «${frase}»`);
  assert.ok(!/não existe/.test(frase),
    `a frase dos MODs ainda diz que a tela de aceitar não existe, e ela existe: «${frase}»`);
  assert.ok(/entre de novo/i.test(frase),
    `a frase dos MODs não diz o que a pessoa pode fazer com esta versão — entrar de novo, que abre a pergunta: «${frase}»`);
  await page.evaluate(([alvo, recusa]) => {
    window.testAtrasoPorAlvo = {};
    window.testRecusaPorAlvo = { [alvo]: recusa };
    for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
    $('tela-boot').hidden = false;
    irParaOServidor(alvo, null, null);
  }, [ALVO_W, PELOS_MODS]);
  await page.waitForFunction(() => !document.getElementById('mods-aceite').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('entrar de novo no servidor não abriu a pergunta dos MODs, e a frase promete o que esta versão não faz'));
  await page.evaluate(() => fecharAceiteDeMods());

  // ---- 6. O convite da tentativa velha não chega ao registro.
  const noLog = await page.evaluate((convite) => testCalls
    .filter(c => c.cmd === 'registrar_da_janela' && JSON.stringify(c.args ?? null).includes(convite)).length, CONVITE_W2);
  assert.equal(noLog, 0, 'o convite da tentativa velha foi escrito no seele.log junto com a recusa dela');

  assert.deepEqual(erros, [], 'a cena do lote W2 lançou erro na página');
  await page.close();
}

// --------------------------------------------------------------------------
// Lote W2 da F5, rodada 2 da revisão: o CONECTAR sempre tem dono
// --------------------------------------------------------------------------
//
// **O caso** (I1 da revisão). A tentativa velha fica suspensa **antes** de
// apagar o CONECTAR — na pergunta do nome, a primeira entrada sem nome desta
// execução, ou esperando o `apelido_local` —, e a mais nova começa e termina
// nesse meio-tempo pelo HOSPEDAR AQUI, que continua alcançável por Tab por
// baixo da caixa. Depois a velha seguia: apagava o CONECTAR, chamava o
// `connect`, e o `finally` dela, que desde o `6af0bedd` só devolve o botão à
// mais nova, não o devolvia. Quem saía da sessão achava o CONECTAR apagado e a
// lista de servidores sem abrir.
//
// **O que vale agora.** Só a tentativa que ainda é a mais nova mexe no botão:
// a que outra já substituiu desiste antes de apagá-lo (e antes de perguntar o
// nome e de chamar o `connect`), e conta isso ao `seele.log`; o `finally` da
// pergunta do nome também só devolve o botão à mais nova.

async function oConectarTemDono(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    // Por alvo, como na cena acima; e por comando, para segurar o
    // `apelido_local` de quem pergunta.
    window.testAtrasoPorAlvo = {};
    window.testRecusaPorAlvo = {};
    window.testAtrasoPorComando = {};
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        const chamada = { cmd, args };
        window.testCalls.push(chamada);
        const espera = window.testAtrasoPorComando[cmd];
        if (espera) await new Promise(ok => setTimeout(ok, espera));
        if (cmd === 'connect') {
          const atraso = window.testAtrasoPorAlvo[args.server];
          const recusa = window.testRecusaPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          chamada.respondida = true;
          // O `AlreadyConnected` do `main.rs` chega cru, como o `?? falha` de
          // `conectar` espera.
          if (recusa) throw (typeof recusa === 'string' ? recusa : { error: recusa, trail: [] });
          return { snapshot, veredito: null, geracao: window.testCalls.length };
        }
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    // Sem nome nesta máquina: a primeira entrada pergunta.
    table: respostas({
      apelido_local: '', snapshot: null, microfones: [], saidas: [],
      hospedar: {
        aqui: AQUI_W, convite: `seele://${AQUI_W}?fp=ab`, alcance: 'SoRedeLocal',
        porta_recusada: null, encontro_recusado: null,
      },
    }),
    snapshot: retrato(),
  });

  /**
   * Uma janela nova, na entrada, com a velha a caminho de `alvo`. Com
   * `atrasoDoApelido`, o `apelido_local` leva esse tempo para todo mundo; com
   * `hospedarAos`, a nova começa pelo `hospedar()` depois desse tempo.
   */
  async function abrirComAVelha(alvo, { atrasoDoApelido = 0, hospedarAos = null } = {}) {
    await page.goto(`http://127.0.0.1:${porta}`);
    await page.waitForTimeout(500);
    await page.evaluate(([alvo, atrasoDoApelido, hospedarAos]) => {
      // Quando a velha chegar ao `connect`, a sessão da nova já está de pé.
      window.testRecusaPorAlvo[alvo] = 'AlreadyConnected';
      if (atrasoDoApelido) window.testAtrasoPorComando.apelido_local = atrasoDoApelido;
      for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
      $('tela-boot').hidden = false;
      window.testAntes = testCalls.length;
      irParaOServidor(alvo, null, null);
      if (hospedarAos !== null) setTimeout(() => { window.testHospedou = hospedar(); }, hospedarAos);
    }, [alvo, atrasoDoApelido, hospedarAos]);
  }
  const estado = () => page.evaluate(() => ({
    pergunta: !document.getElementById('perfil').hidden,
    dentro: !document.getElementById('tela-sessao').hidden || !document.getElementById('tela-auth').hidden,
    conectar: document.getElementById('botao-conectar').disabled ? 'apagado' : 'aceso',
    connects: testCalls.slice(testAntes).filter(c => c.cmd === 'connect').map(c => c.args.server),
    registro: testCalls.slice(testAntes).filter(c => c.cmd === 'registrar_da_janela').map(c => [c.args.nivel, c.args.oQue]),
  }));
  /** A pessoa sai da sessão e aperta CONECTAR: a lista de servidores abre. */
  async function aEntradaAindaFunciona(caso) {
    // O diálogo do link, que o `hospedar` abre, fechado como a pessoa o fecha.
    await page.evaluate(() => { if (!document.getElementById('porta').hidden) fecharPorta(); });
    await page.evaluate(() => ejetar());
    assert.equal(await page.isVisible('#tela-boot'), true, `${caso}: desconectar não voltou à entrada, e o CONECTAR não foi medido`);
    const depois = await estado();
    assert.equal(depois.conectar, 'aceso',
      `${caso}: de volta à entrada, o CONECTAR ficou apagado — a tentativa velha o apagou depois que a mais nova já o ` +
      'tinha devolvido, e ninguém o devolve: a lista de servidores não abre, e a pessoa só sai dali hospedando de novo ou fechando o app');
    await page.click('#botao-conectar', { timeout: 1500 });
    await page.waitForFunction(() => !document.getElementById('servidores').hidden, null, { timeout: 3000 })
      .catch(() => assert.fail(`${caso}: o CONECTAR aceso não abriu a lista de servidores`));
    await page.evaluate(() => fecharServidores());
  }
  const desistiuNoRegistro = async (caso, alvo) => {
    const linhas = (await estado()).registro.filter(([, frase]) => frase.includes(alvo));
    assert.deepEqual(linhas.map(([nivel]) => nivel), ['aviso'],
      `${caso}: a tentativa a ${alvo}, que outra já tinha substituído, desistiu sem deixar uma linha (e uma só) no ` +
      `seele.log como aviso: ${JSON.stringify(linhas)}`);
  };

  // ---- 1. A sequência do revisor: a velha na pergunta do nome, a nova entra
  // pelo HOSPEDAR AQUI, a caixa fecha, a pessoa sai da sessão.
  await abrirComAVelha(ALVO_W);
  await page.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('a primeira entrada sem nome não perguntou o nome, e este caso não mede nada'));
  await page.evaluate(() => { window.testHospedou = hospedar(); });
  await page.waitForFunction(() => !document.getElementById('tela-sessao').hidden || !document.getElementById('tela-auth').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('o hospedar não entrou com a pergunta do nome aberta, e este caso não mede nada'));
  await page.evaluate(() => testHospedou);
  await page.evaluate(() => fecharPerfil());
  await page.waitForTimeout(400);
  const depoisDaCaixa = await estado();
  assert.equal(depoisDaCaixa.conectar, 'aceso',
    'a pergunta do nome fechou depois que o hospedar já tinha entrado, e a tentativa velha apagou o CONECTAR que a mais nova ' +
    'tinha devolvido: o `finally` dela não o devolve, e ninguém mais o devolve');
  assert.deepEqual(depoisDaCaixa.connects, [AQUI_W],
    `a tentativa velha seguiu até o connect depois que outra a tinha substituído (${JSON.stringify(depoisDaCaixa.connects)}): ` +
    'com a nova dentro, ele volta AlreadyConnected; com a nova ainda andando, ele revogaria a geração dela');
  await desistiuNoRegistro('a velha na pergunta do nome', ALVO_W);
  await aEntradaAindaFunciona('a velha na pergunta do nome');

  // ---- 2. A velha esperando o `apelido_local` quando a nova começa: ela não
  // abre a pergunta do nome por cima da nova.
  //
  // As duas esperam 500 ms pelo `apelido_local`. A velha volta primeiro e,
  // sem a conferência, perguntava o nome — e a nova, achando a marca posta,
  // entrava com a caixa da velha aberta por cima da sessão.
  await abrirComAVelha(OUTRO_ALVO_W, { atrasoDoApelido: 500, hospedarAos: 100 });
  await page.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('ninguém perguntou o nome, e este caso não mede nada'));
  await page.waitForTimeout(200);
  const comACaixa = await estado();
  assert.equal(comACaixa.dentro, false,
    'a pergunta do nome está aberta por cima da sessão: foi a tentativa velha que a abriu, depois que a mais nova começou, e a nova entrou por baixo dela');
  await page.evaluate(() => { window.testAtrasoPorComando = {}; fecharPerfil(); });
  await page.waitForFunction(() => !document.getElementById('tela-sessao').hidden || !document.getElementById('tela-auth').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('a mais nova não entrou depois de responder ao nome'));
  await page.evaluate(() => testHospedou);
  await page.waitForTimeout(200);
  assert.deepEqual((await estado()).connects, [AQUI_W],
    `a tentativa velha seguiu até o connect depois que outra a tinha substituído: ${JSON.stringify((await estado()).connects)}`);
  await desistiuNoRegistro('a velha esperando o apelido', OUTRO_ALVO_W);
  await aEntradaAindaFunciona('a velha esperando o apelido');

  // ---- 3. A velha na pergunta do nome, e a nova ainda andando quando a caixa
  // fecha: o `finally` da pergunta não devolve o CONECTAR à velha.
  await abrirComAVelha(ALVO_W);
  await page.waitForFunction(() => !document.getElementById('perfil').hidden, null, { timeout: 3000 })
    .catch(() => assert.fail('a primeira entrada sem nome não perguntou o nome, e este caso não mede nada'));
  await page.evaluate((aqui) => { window.testAtrasoPorAlvo[aqui] = 1200; window.testHospedou = hospedar(); }, AQUI_W);
  await page.waitForFunction((aqui) => testCalls.some(c => c.cmd === 'connect' && c.args.server === aqui), AQUI_W, { timeout: 3000 })
    .catch(() => assert.fail('o hospedar não chegou ao connect, e este caso não mede nada'));
  await page.evaluate(() => fecharPerfil());
  await page.waitForTimeout(300);
  assert.equal((await estado()).conectar, 'apagado',
    'a pergunta do nome da tentativa velha fechou com a mais nova ainda andando, e o `finally` dela reacendeu o CONECTAR: ' +
    'um clique ali abre uma terceira entrada por baixo da que está em curso');
  await page.evaluate(() => testHospedou);
  await page.waitForTimeout(200);
  assert.equal((await estado()).conectar, 'aceso', 'a mais nova terminou e o CONECTAR ficou apagado: quem devolve o botão é ela');
  await desistiuNoRegistro('a velha com a nova andando', ALVO_W);
  await page.evaluate((aqui) => { delete window.testAtrasoPorAlvo[aqui]; }, AQUI_W);

  assert.deepEqual(erros, [], 'a cena do CONECTAR do lote W2 lançou erro na página');
  await page.close();
}

// --------------------------------------------------------------------------
// Lote W2 da F5, rodada 2 da revisão: a batida da espera só cala a si mesma
// --------------------------------------------------------------------------
//
// **O caso** (M1 da revisão, que já existia antes do W2). A espera da portaria
// bate de novo sozinha; quem sai dela pela porta com uma batida no ar deixa
// essa batida calada quando ela volta (`levarParaAEspera`). A bandeira era um
// sim, e calava **qualquer** falha que voltasse com a batida no ar: quem saía
// e apertava HOSPEDAR AQUI tinha a recusa da entrada nova calada, sem tela e
// sem registro. Agora a bandeira guarda o número da tentativa da batida: a
// falha de outra tentativa fala na linha vermelha, e a da batida, calada na
// tela, vai ao `seele.log`.

async function aBatidaDaEsperaSoCalaASiMesma(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    window.testAtrasoPorAlvo = {};
    window.testRecusaPorAlvo = {};
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        const chamada = { cmd, args };
        window.testCalls.push(chamada);
        if (cmd === 'connect') {
          const atraso = window.testAtrasoPorAlvo[args.server];
          const recusa = window.testRecusaPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          chamada.respondida = true;
          if (recusa) throw { error: recusa, trail: [] };
          return { snapshot, veredito: null, geracao: window.testCalls.length };
        }
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    table: respostas({
      apelido_local: 'Bia', snapshot: null, microfones: [], saidas: [],
      hospedar: {
        aqui: AQUI_W, convite: `seele://${AQUI_W}?fp=ab`, alcance: 'SoRedeLocal',
        porta_recusada: null, encontro_recusado: null,
      },
    }),
    snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  const NA_PORTARIA = { Refused: { reason: 'AdmissionPending' } };
  const LOTADO = { Refused: { reason: 'ServerFull' } };

  /** A espera aberta por uma batida pendente na portaria. */
  async function abrirAEspera() {
    await page.evaluate(([alvo, recusa]) => {
      window.testAtrasoPorAlvo = {};
      window.testRecusaPorAlvo = { [alvo]: recusa };
      for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
      $('tela-boot').hidden = false;
      irParaOServidor(alvo, null, null);
    }, [PORTARIA_W2, NA_PORTARIA]);
    await page.waitForFunction(() => !document.getElementById('tela-auth').hidden && $('tela-auth').dataset.modo === 'espera', null, { timeout: 3000 })
      .catch(() => assert.fail('a batida pendente não abriu a espera, e este caso não mede nada'));
  }
  const respondida = (alvo) => page.waitForFunction(
    (alvo) => testCalls.slice(testAntes).some(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida),
    alvo, { timeout: 3000 },
  ).catch(() => assert.fail(`a tentativa a ${alvo} não voltou, e este caso não mede nada`));
  const noRegistro = (trecho) => page.evaluate((trecho) => testCalls.slice(testAntes)
    .filter(c => c.cmd === 'registrar_da_janela' && String(c.args?.oQue).includes(trecho))
    .map(c => [c.args.nivel, c.args.oQue]), trecho);

  // ---- 1. A sonda do revisor: a batida no ar, a pessoa sai da espera e
  // aperta HOSPEDAR AQUI, e o servidor daqui recusa a entrada (lotado) antes
  // de a batida voltar. Quem fala é a entrada nova.
  await abrirAEspera();
  await page.evaluate(([alvo, aqui, lotado]) => {
    window.testAtrasoPorAlvo[alvo] = 1500;
    window.testRecusaPorAlvo[aqui] = lotado;
    window.testAntes = testCalls.length;
    window.testBatida = baterDeNovo();
    setTimeout(() => voltarParaAEntrada(), 100);
    setTimeout(() => { window.testHospedou = hospedar(); }, 200);
  }, [PORTARIA_W2, AQUI_W, LOTADO]);
  await respondida(AQUI_W);
  await page.waitForTimeout(100);
  const daNova = await page.evaluate(([lotado]) => ({
    escrita: $('boot-erro').hidden ? null : $('boot-erro').textContent,
    titulo: fraseDeErro(lotado).split('\n')[0],
  }), [LOTADO]);
  assert.ok(daNova.escrita !== null && daNova.escrita.startsWith(daNova.titulo),
    `a pessoa saiu da espera com uma batida no ar e apertou HOSPEDAR AQUI; a entrada nova foi recusada («${daNova.titulo}») ` +
    `e a linha vermelha ${daNova.escrita === null ? 'ficou calada' : `disse «${daNova.escrita}»`}: a batida no ar calou a falha de outra tentativa, sem tela e sem registro`);
  // A batida volta depois, já substituída: calada na tela (W2), no registro.
  await respondida(PORTARIA_W2);
  await page.evaluate(() => testBatida);
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => document.getElementById('tela-auth').hidden), true,
    'a batida que voltou depois de a pessoa sair da espera e entrar por outro caminho reabriu a espera');
  assert.ok((await page.evaluate(() => $('boot-erro').textContent)).startsWith(daNova.titulo),
    'a batida que voltou depois tirou da linha vermelha a recusa da entrada nova');
  assert.deepEqual((await noRegistro(PORTARIA_W2)).map(([nivel]) => nivel), ['aviso'],
    `a batida que voltou depois de outra tentativa começar não deixou uma linha (e uma só) no seele.log: ${JSON.stringify(await noRegistro(PORTARIA_W2))}`);
  await page.evaluate(() => { if (!document.getElementById('porta').hidden) fecharPorta(); return ejetar(); });

  // ---- 2. Só a batida: a pessoa sai da espera e não faz mais nada. A batida
  // volta calada na tela — a espera foi largada de propósito —, e no registro.
  await abrirAEspera();
  await page.evaluate((alvo) => {
    window.testAtrasoPorAlvo[alvo] = 600;
    window.testAntes = testCalls.length;
    window.testBatida = baterDeNovo();
    setTimeout(() => voltarParaAEntrada(), 100);
  }, PORTARIA_W2);
  await respondida(PORTARIA_W2);
  await page.evaluate(() => testBatida);
  await page.waitForTimeout(100);
  const calada = await page.evaluate(() => ({
    espera: !document.getElementById('tela-auth').hidden,
    linha: $('boot-erro').hidden ? null : $('boot-erro').textContent,
  }));
  assert.deepEqual(calada, { espera: false, linha: null },
    `a batida que voltou depois de a pessoa sair da espera falou na tela: ${JSON.stringify(calada)}`);
  const daBatida = await noRegistro(PORTARIA_W2);
  assert.deepEqual(daBatida.map(([nivel]) => nivel), ['aviso'],
    `a batida que voltou depois de a pessoa sair da espera foi calada na tela sem deixar uma linha (e uma só) no seele.log: ${JSON.stringify(daBatida)}`);
  assert.ok(daBatida[0][1].includes('AdmissionPending'),
    `a linha da batida calada não diz o motivo: ${JSON.stringify(daBatida)}`);

  // ---- 3. Duas batidas sobrepostas (rodada 3 da revisão, R2-M1). O relógio
  // da espera é religado pelo `visibilitychange` com uma batida no ar, e uma
  // segunda sai antes de a primeira voltar. A primeira, voltando, não pode
  // zerar a marca da segunda: com a marca zerada, a segunda volta pendente
  // depois que a pessoa saiu e reabre a espera por cima da entrada que ela
  // escolheu — o que a marca existe para evitar. É o `finally` de
  // `baterDeNovo`, que só zera a própria marca.
  await abrirAEspera();
  await page.evaluate((alvo) => {
    pararDeBater();
    window.testAntes = testCalls.length;
    window.testAtrasoPorAlvo[alvo] = 600;
    window.testB1 = baterDeNovo();
    setTimeout(() => { window.testAtrasoPorAlvo[alvo] = 1200; window.testB2 = baterDeNovo(); }, 300);
    setTimeout(() => voltarParaAEntrada(), 800);
  }, PORTARIA_W2);
  await page.waitForFunction(
    (alvo) => testCalls.slice(testAntes).filter(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida).length >= 2,
    PORTARIA_W2, { timeout: 4000 },
  ).catch(() => assert.fail('as duas batidas não voltaram, e este caso não mede nada'));
  await page.evaluate(() => Promise.all([testB1, testB2]));
  await page.waitForTimeout(100);
  const depoisDasDuas = await page.evaluate(() => ({
    espera: !document.getElementById('tela-auth').hidden,
    linha: $('boot-erro').hidden ? null : $('boot-erro').textContent,
  }));
  if (depoisDasDuas.espera) await page.evaluate(() => voltarParaAEntrada());
  assert.deepEqual(depoisDasDuas, { espera: false, linha: null },
    'com duas batidas sobrepostas, a primeira voltou e zerou a marca da segunda; a segunda voltou depois de a pessoa sair ' +
    `da espera e falou na tela por cima da entrada que ela escolheu: ${JSON.stringify(depoisDasDuas)}`);
  const dasDuas = (await noRegistro(PORTARIA_W2)).map(([nivel, frase]) => [nivel, frase.includes('batida da espera') ? 'batida' : 'substituída']);
  assert.deepEqual(dasDuas, [['aviso', 'substituída'], ['aviso', 'batida']],
    'cada uma das duas batidas sobrepostas devia deixar a sua linha no seele.log — a primeira como tentativa substituída, ' +
    `a segunda como a batida largada: ${JSON.stringify(await noRegistro(PORTARIA_W2))}`);

  assert.deepEqual(erros, [], 'a cena da batida da espera do lote W2 lançou erro na página');
  await page.close();
}

// --------------------------------------------------------------------------
// Lote E54 da F5: a entrada velha que chega tarde não toma a tela
// --------------------------------------------------------------------------
//
// **O caso** (pendência 58, item 54; R2-M2 da revisão do W2). O W2 calou a
// tentativa velha que volta **recusada**; esta é a que volta **entrando**. A
// revisão mediu: uma velha a 192.0.2.17 leva 1,2 s; aos 100 ms o `hospedar()`
// entra no servidor daqui; aos 1,9 s a velha volta, reabre `#tela-auth` em
// `aperto` com o endereço dela por cima da sessão, e a geração da sessão volta
// à dela. No Rust era pior: a conexão dela tomava o slot da nova (medido em
// `a_entrada_velha_que_chega_tarde`, no `main.rs`).
//
// **O que vale agora.** O `connect` do Rust recusa a velha quando a nova já
// chegou a ele (a recusa cai no ramo do W2). Quando a nova ainda não chegou —
// está no `conferir_esta_versao` —, a velha entra no Rust, e a janela não a
// mostra: registra, e pede ao Rust que solte a sessão dela pela geração dela
// (`soltar_a_entrada`).

/** O convite que a tentativa velha leva: ele não pode chegar ao registro. */
const CONVITE_E54 = `convite-do-lote-e54-${process.pid}-${Date.now()}`;

async function aEntradaVelhaNaoTomaATela(browser, porta) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; page.on('pageerror', e => erros.push(e.message));
  await page.addInitScript(({ table, snapshot }) => {
    window.testTable = table; window.testCalls = [];
    window.testAtrasoPorAlvo = {};
    window.testAtrasoPorComando = {};
    // **O slot do Rust, quando a cena o modela**: o `connect` volta
    // `AlreadyConnected` com ele cheio (o começo do `connect` no `main.rs`), e
    // `soltar_a_entrada` o esvazia só pela geração que está nele.
    window.testModeloDoSlot = false;
    window.testSlot = null;
    window.testGeracao = 0;
    window.__TAURI__ = {
      core: { invoke: async (cmd, args) => {
        const chamada = { cmd, args };
        window.testCalls.push(chamada);
        if (cmd === 'connect') {
          if (window.testModeloDoSlot && window.testSlot) {
            chamada.respondida = true;
            throw { error: 'AlreadyConnected', trail: [] };
          }
          const geracao = ++window.testGeracao;
          chamada.geracao = geracao;
          const atraso = window.testAtrasoPorAlvo[args.server];
          if (atraso) await new Promise(ok => setTimeout(ok, atraso));
          chamada.respondida = true;
          if (window.testModeloDoSlot) window.testSlot = { server: args.server, geracao };
          return { snapshot, veredito: null, geracao };
        }
        if (cmd === 'soltar_a_entrada') {
          chamada.soltou = window.testSlot !== null && window.testSlot.geracao === args.geracao;
          if (chamada.soltou) window.testSlot = null;
          return chamada.soltou;
        }
        if (cmd === 'desmontar_a_sessao' || cmd === 'disconnect') window.testSlot = null;
        const atraso = window.testAtrasoPorComando[cmd];
        if (atraso) await new Promise(ok => setTimeout(ok, atraso));
        return window.testTable[cmd] ?? null;
      } },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, {
    table: respostas({
      apelido_local: 'Bia', snapshot: null, microfones: [], saidas: [],
      hospedar: {
        aqui: AQUI_W, convite: `seele://${AQUI_W}?fp=ab`, alcance: 'SoRedeLocal',
        porta_recusada: null, encontro_recusado: null,
      },
    }),
    snapshot: retrato(),
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  await page.waitForTimeout(500);

  /** O que a pessoa vê, e a geração em que a janela está. */
  const aTela = () => page.evaluate(() => ({
    entrada: !document.getElementById('tela-boot').hidden,
    sessao: !document.getElementById('tela-sessao').hidden,
    autenticacao: document.getElementById('tela-auth').hidden
      ? null : `${$('tela-auth').dataset.modo} ${$('auth-endereco').textContent}`,
    linha: $('boot-erro').hidden ? null : $('boot-erro').textContent,
    geracao: geracaoDaSessao,
  }));
  /** As chamadas a `cmd` desde o começo do caso. */
  const desde = (cmd) => page.evaluate((cmd) => testCalls.slice(testAntes).filter(c => c.cmd === cmd), cmd);
  /** A geração que o `connect` deu à tentativa a `alvo`, neste caso. */
  const geracaoDe = (alvo) => page.evaluate((alvo) => testCalls.slice(testAntes)
    .find(c => c.cmd === 'connect' && c.args.server === alvo)?.geracao ?? null, alvo);
  const respondida = (alvo) => page.waitForFunction(
    (alvo) => testCalls.slice(testAntes).some(c => c.cmd === 'connect' && c.args.server === alvo && c.respondida),
    alvo, { timeout: 4000 },
  ).catch(() => assert.fail(`a tentativa a ${alvo} não voltou, e este caso não mede nada`));
  /** A velha começa, e aos 100 ms o `hospedar()` a substitui. */
  async function correr(atrasoDaVelha, depoisDoHospedar) {
    await page.evaluate(([alvo, convite, atraso, conferir]) => {
      window.testAtrasoPorAlvo = { [alvo]: atraso };
      window.testAtrasoPorComando = {};
      for (const secao of document.querySelectorAll('section.tela')) secao.hidden = true;
      $('tela-boot').hidden = false;
      window.testAntes = testCalls.length;
      irParaOServidor(alvo, null, convite);
      setTimeout(() => {
        if (conferir) window.testAtrasoPorComando.conferir_esta_versao = conferir;
        hospedar();
      }, 100);
    }, [ALVO_W, CONVITE_E54, atrasoDaVelha, depoisDoHospedar]);
  }
  /** As linhas que a entrada largada deixou no `seele.log`, pelo nível. */
  const noRegistro = () => page.evaluate((alvo) => testCalls.slice(testAntes)
    .filter(c => c.cmd === 'registrar_da_janela' && String(c.args?.oQue).includes(alvo))
    .map(c => [c.args.nivel, c.args.oQue]), ALVO_W);
  const sair = () => page.evaluate(() => { if (!document.getElementById('porta').hidden) fecharPorta(); return ejetar(); });

  // ---- 1. A sequência que a revisão do W2 mediu: a velha leva 1,2 s e
  // entra; aos 100 ms o `hospedar()` entra no servidor daqui. A janela, sozinha:
  // a velha volta entrando, e não toca na tela.
  await correr(1200, 0);
  await respondida(AQUI_W);
  await page.waitForTimeout(600);
  const comANova = await aTela();
  assert.equal(comANova.entrada, false, 'o hospedar do meio da corrida não entrou, e a corrida não foi medida');
  await respondida(ALVO_W);
  await page.waitForTimeout(600);
  const depoisDaVelha = await aTela();
  assert.deepEqual(depoisDaVelha, comANova,
    `a tentativa a ${ALVO_W}, que entrou 1,2 s depois de o hospedar ter entrado no servidor daqui, tomou a tela da ` +
    `sessão: antes ${JSON.stringify(comANova)}, depois ${JSON.stringify(depoisDaVelha)}`);
  const daVelha = await geracaoDe(ALVO_W);
  const soltar1 = await desde('soltar_a_entrada');
  assert.deepEqual(soltar1.map(c => c.args.geracao), [daVelha],
    `a janela não pediu ao Rust que soltasse a sessão da tentativa largada (geração ${daVelha}), e só ela: ` +
    `${JSON.stringify(soltar1.map(c => c.args))} — sem isso a conexão dela fica no slot sem tela`);
  const linhas1 = await noRegistro();
  assert.deepEqual(linhas1.map(([nivel]) => nivel), ['aviso'],
    `a entrada largada não deixou uma linha (e uma só) no seele.log como aviso: ${JSON.stringify(linhas1)}`);
  await sair();

  // ---- 2. A outra ordem, com o slot do Rust modelado: a velha entra no Rust
  // antes de a nova chegar ao `connect` (a nova espera 600 ms no
  // `conferir_esta_versao`). A janela larga a velha e o slot fica livre: a nova
  // entra, em vez de receber `AlreadyConnected`.
  await page.evaluate(() => { window.testModeloDoSlot = true; window.testSlot = null; });
  await correr(300, 600);
  await respondida(ALVO_W);
  await respondida(AQUI_W);
  await page.waitForTimeout(300);
  const naOutraOrdem = await aTela();
  const slot = await page.evaluate(() => window.testSlot);
  assert.deepEqual({ linha: naOutraOrdem.linha, entrada: naOutraOrdem.entrada, slot: slot?.server ?? null },
    { linha: null, entrada: false, slot: AQUI_W },
    `a tentativa a ${ALVO_W} entrou no Rust antes de o hospedar chegar ao connect, e a entrada nova não ficou com a ` +
    `sessão: ${JSON.stringify({ tela: naOutraOrdem, slot })}`);
  assert.equal(naOutraOrdem.autenticacao === null || !naOutraOrdem.autenticacao.includes(ALVO_W), true,
    `a autenticação mostra o servidor que a pessoa deixou: «${naOutraOrdem.autenticacao}»`);
  const soltar2 = await desde('soltar_a_entrada');
  assert.deepEqual(soltar2.map(c => [c.args.geracao, c.soltou]), [[await geracaoDe(ALVO_W), true]],
    `a sessão da tentativa largada não foi solta pela geração dela: ${JSON.stringify(soltar2.map(c => [c.args, c.soltou]))}`);
  await sair();
  await page.evaluate(() => { window.testModeloDoSlot = false; window.testSlot = null; });

  // ---- O convite da tentativa velha não chega a comando nenhum além do
  // `connect`: nem ao registro, nem ao pedido de soltar.
  const vazou = await page.evaluate((convite) => testCalls
    .filter(c => c.cmd !== 'connect' && JSON.stringify(c.args ?? null).includes(convite)).map(c => c.cmd), CONVITE_E54);
  assert.deepEqual(vazou, [], `o convite da tentativa velha foi a comandos além do connect: ${JSON.stringify(vazou)}`);

  assert.deepEqual(erros, [], 'a cena do lote E54 lançou erro na página');
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
    await aTentativaSubstituidaNaoFala(browser, server.address().port);
    await oConectarTemDono(browser, server.address().port);
    await aBatidaDaEsperaSoCalaASiMesma(browser, server.address().port);
    await aEntradaVelhaNaoTomaATela(browser, server.address().port);

    console.log('v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium');
    console.log(`lote J: a faixa de contexto acompanha a tecla e o modo, a entrada por link pergunta o nome antes do connect, e o título da pergunta não se sobrepõe (folga de ${titulo.folga}px entre as tintas)`);
    console.log('lote R: a volta que aceita os MODs leva a credencial da tentativa e entra, e o RECONECTAR de uma sessão que entrou vai sem ela');
    console.log('lote R: desistir da pergunta (RECUSAR, Escape, clicar fora, ou no meio da obtenção) esquece a credencial e não entra');
    console.log('lote R: com o último alvo trocado no meio da batida, a volta e o sim vão ao servidor que perguntou');
    console.log('lote R: a credencial não aparece no armazenamento, no DOM, no console nem em comando nenhum além do connect');
    console.log('lote W: desistir no meio da obtenção ou da gravação não deixa o sim dos MODs valendo');
    console.log('lote W: depois do apelido tomado, a tentativa seguinte ao mesmo servidor leva o apelido novo e o convite, e entra; a outro servidor, não leva nada');
    console.log('lote W: entrar, trocar de alvo e desistir da pergunta dos MODs esquecem a credencial, e ela não aparece fora do connect');
    console.log('lote W2: a recusa de uma tentativa que outra substituiu não fala na entrada nem na espera, e vai ao seele.log');
    console.log('lote W2: a frase dos MODs manda entrar de novo, e entrar de novo abre a pergunta');
    console.log('lote W2: a tentativa substituída não apaga o CONECTAR, não pergunta o nome nem chama o connect, e conta ao seele.log que desistiu');
    console.log('lote W2: a batida da espera só cala a si mesma, e conta ao seele.log; a falha de outra tentativa fala na linha vermelha');
    console.log('lote E54: a tentativa que outra substituiu e entra depois não toca na tela, vai ao seele.log, e a sessão dela é solta pela geração dela');
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
