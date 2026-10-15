// **As saídas de quem hospeda com um MOD de API antiga, como botões** (o corte
// da API 6; tarefa 13 do plano da F3, Step 5b; as decisões (i), (ii) e (iii)
// da orquestradora de 15/10). Com o HTML, o CSS e a `tela-boot.js` reais; só a
// ponte nativa é simulada.
//
// # O que ela mede
//
// 1. **As três saídas, na ordem do dono** (08/10), em cada MOD: ATUALIZAR,
//    HOSPEDAR PELO SERVIDOR 0.15 e DESLIGAR. ATUALIZAR **só** com a versão da
//    API 6 no catálogo (ii); sem ela, «ainda não há versão dele para a API 6».
// 2. **O pacote que não está nesta máquina** não oferece a 0.15 (a API dele não
//    é sabida, e nada diz que a 0.15 o roda), e oferece ATUALIZAR e DESLIGAR.
// 3. **ATUALIZAR** baixa a versão da API 6 pelo uso «obter», que não mexe em
//    servidor nenhum, passa o hash dela à saída 1 **do servidor que recusou**,
//    e hospeda de novo; **DESLIGAR** passa «desligar» e hospeda de novo.
// 4. **Uma recusa da saída é dita** com a frase dela, e não hospeda de novo.
// 5. **Com o catálogo fora do ar**, cada MOD diz a falha do catálogo, e não que
//    não há versão: seria afirmar o que não se sabe.
// 6. **HOSPEDAR PELO SERVIDOR 0.15** (iii) abre a tela das versões do servidor
//    com a 0.15 escolhida, e a frase diz que quem hospeda é a janela dela.
//
// Rodar:  node apps/seele-app/bancada/saidas-de-quem-hospeda.cjs

const assert = require('node:assert/strict');
const { servir, respostas } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

const versao = (versao, api, hash) => ({ versao, api, hash, nivel: 'oficial', notas: [], alcanca: [], arquivos: [] });
const CATALOGO = {
  esquema: 1,
  gerado_em: 0,
  mods: [
    { id: 'seele/estilo', titulo: 'Estilo', resumo: '', repo: '', nivel: 'oficial',
      versoes: [versao('3.1.0', 4, 'h31'), versao('4.0.0', 6, 'h40')] },
    // Sem versão para a API 6 ainda.
    { id: 'seele/mesa', titulo: 'Mesa', resumo: '', repo: '', nivel: 'oficial',
      versoes: [versao('3.1.0', 4, 'm31')] },
    { id: 'x/sumido', titulo: 'Sumido', resumo: '', repo: '', nivel: 'comunidade',
      versoes: [versao('1.0.0', 4, 's10'), versao('2.0.0', 6, 's20')] },
  ],
};
const LINHAS = [
  { linha: '1', servidor: '1.0', apis: [6], app: null, esta: true, instalada: true, tem_pacote: true, perde_conversas: false },
  { linha: '0.15', servidor: '0.15', apis: [5, 4, 3], app: '0.15.2', esta: false, instalada: true, tem_pacote: true, perde_conversas: false },
];
const RECUSA = { ModsDeApiAntiga: { mods: [['seele/estilo', 4], ['seele/mesa', 4], ['x/sumido', null]] } };

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
    const erros = [];
    page.on('pageerror', e => erros.push(e.message));
    // Os avisos do console só entram na mensagem de uma espera que falhou: um
    // desenho que lança dentro de uma promessa sai como `console.warn`, e não
    // como erro de página.
    const avisos = [];
    page.on('console', m => { if (m.type() === 'warning' || m.type() === 'error') avisos.push(m.text()); });
    await page.addInitScript(table => {
      window.testTable = table;
      window.testCalls = [];
      // Uma resposta pode ser uma função, posta depois pela bancada: é como uma
      // recusa chega — `invoke` rejeita com o que ela lançar.
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          testCalls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? null)) });
          const r = testTable[cmd];
          return typeof r === 'function' ? r(args) : (r ?? null);
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({
      servidores_guardados: [{ id: 'srv-1', nome: 'Casa', versao: '0.15.1', caminho: null, ultimo_uso: 0 }],
      // **A lista das APIs aceitas ainda não chegou**, que é quando o
      // `ultimaQueEsteBuildEntende` cai na última versão do catálogo, de
      // qualquer API: quem hospeda logo ao abrir o app pode receber a recusa
      // antes dela. O ATUALIZAR não depende dela (`versaoParaEsteApp`).
      apis_de_mod_aceitas: [],
    }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    await page.evaluate(({ catalogo, linhas, recusa }) => {
      testTable.catalogo_de_mods = catalogo;
      testTable.linhas_para_hospedar = linhas;
      testTable.instalar_mod_do_catalogo = false;
      window.recusaDeHospedar = recusa;
      testTable.hospedar = () => { throw window.recusaDeHospedar; };
      window.chamadas = cmd => testCalls.filter(c => c.cmd === cmd);
      // Cada MOD com o texto e as saídas, na ordem em que estão na tela.
      window.saidas = () => [...document.querySelectorAll('#boot-saidas [data-mod]')].map(li => ({
        mod: li.dataset.mod,
        texto: li.textContent,
        botoes: [...li.querySelectorAll('button')].map(b => ({ saida: b.dataset.saida, rotulo: b.textContent, visivel: b.getClientRects().length > 0 })),
      }));
      window.hospedarComRecusa = async () => {
        servidorEscolhido = 'srv-1';
        await hospedar();
      };
    }, { catalogo: CATALOGO, linhas: LINHAS, recusa: RECUSA });

    // **Uma espera que diz o que faltou.** Um tempo esgotado sozinho reprova
    // sem dizer por quê; este diz a promessa, o que a tela tem e os erros.
    const esperar = async (condicao, promessa, arg = null) => {
      try {
        await page.waitForFunction(condicao, arg, { timeout: 5000 });
      } catch (falha) {
        if (falha?.name !== 'TimeoutError') throw falha;
        const tela = await page.evaluate(() => ({ saidas: $('boot-saidas').textContent, erro: $('boot-erro').textContent }));
        assert.fail(`${promessa}: ${JSON.stringify(tela)}; erros na página: ${erros.join(' | ') || 'nenhum'}; ` +
          `avisos: ${avisos.slice(-3).join(' | ') || 'nenhum'}`);
      }
    };
    const desenhadas = async () => {
      await page.evaluate(() => hospedarComRecusa());
      await esperar(() => document.querySelectorAll('#boot-saidas [data-mod]').length === 3
        && !document.querySelector('#boot-saidas [data-esperando]'), 'as saídas dos três MODs não foram desenhadas');
      return page.evaluate(() => saidas());
    };

    // 1 e 2. As saídas de cada MOD, na ordem do dono.
    let vistas = await desenhadas();
    assert.equal(await page.evaluate(() => $('boot-saidas').hidden), false, 'as saídas não aparecem');
    assert.match(await page.evaluate(() => $('boot-erro').textContent), /ESTE SERVIDOR TEM MOD QUE ESTE APP NÃO RODA/,
      'a frase da recusa sumiu de cima das saídas');
    const de = id => vistas.find(v => v.mod === id);
    assert.deepEqual(de('seele/estilo').botoes.map(b => b.saida), ['atualizar', 'servidor-0.15', 'desligar'],
      `o estilo não tem as três saídas na ordem do dono: ${JSON.stringify(de('seele/estilo'))}`);
    assert.ok(de('seele/estilo').botoes.every(b => b.visivel), 'uma saída do estilo não aparece');
    assert.match(de('seele/estilo').botoes[0].rotulo, /ATUALIZAR PARA 4\.0\.0/, 'ATUALIZAR não diz a versão da API 6');
    assert.match(de('seele/estilo').botoes[1].rotulo, /SERVIDOR 0\.15/, 'a saída da 0.15 não diz o servidor');
    assert.deepEqual(de('seele/mesa').botoes.map(b => b.saida), ['servidor-0.15', 'desligar'],
      `sem versão da API 6 no catálogo, a mesa oferece ATUALIZAR: ${JSON.stringify(de('seele/mesa'))}`);
    assert.match(de('seele/mesa').texto, /ainda não há versão dele para a API 6/, 'a mesa não diz que não há versão para a 6');
    assert.deepEqual(de('x/sumido').botoes.map(b => b.saida), ['atualizar', 'desligar'],
      `o pacote que falta oferece a 0.15, ou não oferece ATUALIZAR: ${JSON.stringify(de('x/sumido'))}`);
    assert.doesNotMatch(de('x/sumido').texto, /API 0/, 'o pacote que falta escreveu «API 0»');

    // 3. ATUALIZAR: baixa pelo «obter», a saída 1 no servidor que recusou, hospeda de novo.
    await page.evaluate(() => { testCalls.length = 0; testTable.resolver_mod_de_api_antiga = null; });
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="atualizar"]');
    await esperar(() => chamadas('hospedar').length === 1, 'ATUALIZAR não hospedou de novo');
    const baixou = await page.evaluate(() => chamadas('instalar_mod_do_catalogo'));
    assert.deepEqual(baixou.map(c => c.args), [{ id: 'seele/estilo', versao: '4.0.0', uso: 'obter' }],
      `ATUALIZAR não baixou a versão da 6 pelo «obter»: ${JSON.stringify(baixou)}`);
    const resolveu = await page.evaluate(() => chamadas('resolver_mod_de_api_antiga'));
    assert.deepEqual(resolveu.map(c => c.args), [{ servidor: 'srv-1', id: 'seele/estilo', saida: { atualizar: { hash: 'h40' } } }],
      `ATUALIZAR não passou o hash da 4.0.0 à saída 1 do servidor que recusou: ${JSON.stringify(resolveu)}`);
    assert.equal((await page.evaluate(() => chamadas('hospedar')))[0].args.servidor, 'srv-1',
      'depois de ATUALIZAR, hospedou outro servidor');
    const ordem = await page.evaluate(() => testCalls.map(c => c.cmd).filter(c => ['instalar_mod_do_catalogo', 'resolver_mod_de_api_antiga', 'hospedar'].includes(c)));
    assert.deepEqual(ordem, ['instalar_mod_do_catalogo', 'resolver_mod_de_api_antiga', 'hospedar'], `a ordem da saída 1: ${ordem}`);

    // DESLIGAR.
    vistas = await desenhadas();
    await page.evaluate(() => { testCalls.length = 0; });
    await page.click('#boot-saidas [data-mod="seele/mesa"] [data-saida="desligar"]');
    await esperar(() => chamadas('hospedar').length === 1, 'DESLIGAR não hospedou de novo');
    assert.deepEqual((await page.evaluate(() => chamadas('resolver_mod_de_api_antiga'))).map(c => c.args),
      [{ servidor: 'srv-1', id: 'seele/mesa', saida: 'desligar' }], 'DESLIGAR não passou «desligar» ao servidor que recusou');
    assert.equal((await page.evaluate(() => chamadas('instalar_mod_do_catalogo'))).length, 0, 'DESLIGAR baixou alguma coisa');

    // 4. Uma recusa da saída é dita, e não hospeda de novo.
    vistas = await desenhadas();
    await page.evaluate(() => {
      testCalls.length = 0;
      testTable.resolver_mod_de_api_antiga = () => { throw 'ServidorNoAr'; };
    });
    await page.click('#boot-saidas [data-mod="seele/mesa"] [data-saida="desligar"]');
    await esperar(() => /PARE A HOSPEDAGEM/.test($('boot-saidas').textContent),
      'a recusa da saída não é dita debaixo das saídas (ou elas sumiram)');
    assert.equal((await page.evaluate(() => chamadas('hospedar'))).length, 0, 'com a saída recusada, hospedou de novo');
    assert.equal(await page.evaluate(() => document.querySelectorAll('#boot-saidas [data-mod]').length), 3,
      'a recusa da saída levou as saídas embora');
    await page.evaluate(() => { testTable.resolver_mod_de_api_antiga = null; });

    // 5. O catálogo fora do ar: a falha dele, e não «não há versão».
    await page.evaluate(() => { testTable.catalogo_de_mods = () => { throw { NaoRespondeu: 'tempo esgotado' }; }; });
    vistas = await desenhadas();
    const semCatalogo = await page.evaluate(() => fraseDeErro({ NaoRespondeu: 'tempo esgotado' }));
    for (const v of vistas) {
      assert.ok(!v.botoes.some(b => b.saida === 'atualizar'), `sem catálogo, ${v.mod} oferece ATUALIZAR`);
      assert.doesNotMatch(v.texto, /ainda não há versão/, `sem catálogo, ${v.mod} afirma que não há versão`);
    }
    const textoDasSaidas = await page.evaluate(() => $('boot-saidas').textContent);
    assert.ok(textoDasSaidas.includes(semCatalogo.split('\n')[0]), `sem catálogo, a falha dele não é dita: ${textoDasSaidas}`);
    await page.evaluate(c => { testTable.catalogo_de_mods = c; }, CATALOGO);

    // 6. HOSPEDAR PELO SERVIDOR 0.15: a tela das versões, com a 0.15 escolhida.
    vistas = await desenhadas();
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="servidor-0.15"]');
    await esperar(() => !$('tela-preparar').hidden && !$('preparar-versao-aviso').hidden,
      'a saída da 0.15 não abriu a tela das versões com uma escolhida e a frase dela');
    const preparar = await page.evaluate(() => ({
      escolhida: [...document.querySelectorAll('#preparar-lista-versoes li')]
        .filter(li => li.querySelector('button')?.textContent === 'ESCOLHIDA')
        .map(li => li.textContent),
      aviso: $('preparar-versao-aviso').textContent,
      confirmar: $('preparar-confirmar').textContent,
    }));
    assert.equal(preparar.escolhida.length, 1, `não há uma versão escolhida: ${JSON.stringify(preparar)}`);
    assert.match(preparar.escolhida[0], /SERVIDOR 0\.15/, `a escolhida não é a 0.15: ${JSON.stringify(preparar)}`);
    assert.match(preparar.aviso, /seele\/estilo/, `a frase não diz qual MOD leva à 0.15: ${preparar.aviso}`);
    assert.match(preparar.aviso, /Quem hospeda é outra janela do SEELE/, `a frase não diz que hospedar é feito lá: ${preparar.aviso}`);
    assert.match(preparar.confirmar, /SERVIDOR 0\.15 PARA HOSPEDAR/, `o botão de confirmar não é o da 0.15: ${preparar.confirmar}`);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(' | ')}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log('saídas de quem hospeda: as três na ordem do dono, ATUALIZAR só com a versão da 6, a saída no servidor que recusou e hospedar de novo, a recusa dita, o catálogo fora do ar dito, e a 0.15 pela tela das versões.');
})().catch(e => { console.error(e); process.exit(1); });
