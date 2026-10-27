// **As saídas de quem hospeda com um MOD de API antiga, na tela ANTES DE
// HOSPEDAR** (o corte da API 6, tarefa 13 do plano da F3, Step 5b; redesenhada
// no E2 da 0.17). Com o HTML, o CSS e os scripts reais; só a ponte nativa é
// simulada.
//
// # O que ela mede
//
// 1. **A recusa leva à tela própria**, e não à parede vermelha da entrada: o
//    `#boot-erro` fica escondido e a tela ANTES DE HOSPEDAR aparece com os três
//    blocos (os dados, quem consegue entrar, os MODs).
// 2. **As saídas de cada MOD, na ordem do dono** (08/10): ATUALIZAR antes de
//    DESLIGAR. ATUALIZAR **só** com a versão da API 6 no catálogo; sem ela, «ainda
//    não há versão dele para a API 6». O pacote que não está nesta máquina não
//    tem API sabida, e oferece ATUALIZAR e DESLIGAR.
// 3. **Hospedar pelo Servidor 0.15 é UMA escolha do servidor**, no bloco QUEM
//    CONSEGUE ENTRAR, e não um botão repetido em cada MOD.
// 4. **ATUALIZAR** baixa pelo uso «obter» e ESTAGIA a escolha no servidor que
//    recusou (P1 do Codex no app nativo: aplicá-la na hora convertia o banco
//    antes da cópia; o `resolver_mod_de_api_antiga` agora só confere e guarda,
//    `src/escolhas_de_mods.rs`): nada hospeda, a linha diz «A APLICAR AO
//    HOSPEDAR», e o resumo nunca diz «nada foi mudado».
// 5. **DESLIGAR** os outros acende o HOSPEDAR, que diz APLICAR AS ESCOLHAS E
//    HOSPEDAR; reaberta, a tela mostra as escolhas que o backend guardou.
// 6. **Uma recusa da saída é dita**, e não hospeda.
// 7. **Com o catálogo fora do ar**, nenhuma linha oferece ATUALIZAR nem afirma que
//    não há versão, e a falha do catálogo é dita.
// 8. **HOSPEDAR PELO SERVIDOR 0.15** abre a tela das versões com a 0.15
//    escolhida, e a frase diz qual MOD leva a ela e que quem hospeda é a janela
//    dela.
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
  { linha: '2', servidor: '2.0', apis: [6], app: null, esta: true, instalada: true, tem_pacote: true, perde_conversas: false },
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
    const avisos = [];
    page.on('console', m => { if (m.type() === 'warning' || m.type() === 'error') avisos.push(m.text()); });
    await page.addInitScript(table => {
      window.testTable = table;
      window.testCalls = [];
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
      window.saidas = () => [...document.querySelectorAll('#boot-saidas [data-mod]')].map(li => ({
        mod: li.dataset.mod,
        texto: li.textContent,
        resolvido: li.dataset.resolvido ?? null,
        botoes: [...li.querySelectorAll('button')].map(b => ({ saida: b.dataset.saida, rotulo: b.textContent, visivel: b.getClientRects().length > 0 })),
      }));
      window.hospedarComRecusa = async () => {
        servidorEscolhido = 'srv-1';
        await hospedar();
      };
    }, { catalogo: CATALOGO, linhas: LINHAS, recusa: RECUSA });

    const esperar = async (condicao, promessa, arg = null) => {
      try {
        await page.waitForFunction(condicao, arg, { timeout: 5000 });
      } catch (falha) {
        if (falha?.name !== 'TimeoutError') throw falha;
        const tela = await page.evaluate(() => ({ saidas: $('boot-saidas').textContent, erro: $('boot-erro').textContent, antes: $('tela-antes-de-hospedar').hidden }));
        assert.fail(`${promessa}: ${JSON.stringify(tela)}; erros na página: ${erros.join(' | ') || 'nenhum'}; ` +
          `avisos: ${avisos.slice(-3).join(' | ') || 'nenhum'}`);
      }
    };
    const desenhadas = async () => {
      await page.evaluate(() => hospedarComRecusa());
      await esperar(() => !$('tela-antes-de-hospedar').hidden
        && document.querySelectorAll('#boot-saidas [data-mod]').length === 3
        && !document.querySelector('#boot-saidas [data-esperando]'), 'as saídas dos três MODs não foram desenhadas na tela ANTES DE HOSPEDAR');
      return page.evaluate(() => saidas());
    };

    // 1. A tela própria, e não a entrada.
    let vistas = await desenhadas();
    const tela = await page.evaluate(() => ({
      boot: $('tela-boot').hidden,
      erro: $('boot-erro').hidden,
      dados: $('antes-dados').dataset.estado,
      versao: $('antes-versao').dataset.estado,
      mods: $('antes-mods').dataset.estado,
      hospedar: $('antes-hospedar').getAttribute('aria-disabled') === 'true',
    }));
    assert.equal(tela.boot, true, 'a entrada continua à vista atrás da recusa');
    assert.equal(tela.erro, true, 'a parede vermelha da entrada voltou');
    assert.equal(tela.mods, 'resolva', `o bloco dos MODs não diz RESOLVA: ${JSON.stringify(tela)}`);
    assert.ok(tela.dados && tela.versao, `os blocos dos dados e da versão não têm estado: ${JSON.stringify(tela)}`);
    assert.equal(tela.hospedar, true, 'HOSPEDAR aceso com MOD pendente');

    // 1b. **Sem número de esquema no texto principal** (revisão de UI): ele
    //     mora nos DETALHES TÉCNICOS, fechados por padrão. Com a pré-conferência
    //     respondendo, os números estão lá, e não nos blocos.
    await page.evaluate(() => {
      // A forma do backend (`src/antes_de_hospedar.rs`); sem o esquema desta
      // versão, que é da build e não desta bancada.
      testTable.antes_de_hospedar = {
        servidor: 'srv-1', banco: 'seele.db', esquema: { Gravado: 13 }, linha_dos_dados: '0.15',
        verificado: true, motivo: null, acoes: [], converte: true, futuro: false, vai_copiar: true,
        copias: [{ tipo: 'linha-legada', linha: '0.15', origem: 'seele.db', esquema: null, data: 1761300000,
          caminho: 'lancador/dados/servidor-0.15', do_estado_atual: false }], mods_de_api_antiga: [],
      };
    });
    await desenhadas();
    const esquema = await page.evaluate(() => ({
      principal: [...document.querySelectorAll('#tela-antes-de-hospedar .antes-passo')].map(p => p.textContent).join(' '),
      aberto: $('antes-detalhes').open,
      detalhes: $('antes-detalhes-lista').textContent,
      copia: $('antes-dados-copia').textContent,
    }));
    assert.doesNotMatch(esquema.principal, /esquema/i, `o texto principal fala em esquema: ${esquema.principal}`);
    assert.equal(esquema.aberto, false, 'os detalhes técnicos nascem abertos');
    assert.match(esquema.detalhes, /esquema gravado\s*13/, `os detalhes não guardam o esquema gravado: ${esquema.detalhes}`);
    // A cópia do disco é histórico, com a linha e a data; o caminho, nos detalhes.
    assert.match(esquema.copia, /Cópia anterior para o Servidor 0\.15, de [\d/]+\. É histórico/,
      `a cópia não diz a linha e a data como histórico: ${esquema.copia}`);
    assert.match(esquema.detalhes, /lancador\/dados\/servidor-0\.15/, `os detalhes não dizem onde a cópia fica: ${esquema.detalhes}`);
    await page.evaluate(() => { testTable.antes_de_hospedar = null; });
    vistas = await desenhadas();

    // 2. As saídas de cada MOD, na ordem do dono.
    const de = id => vistas.find(v => v.mod === id);
    assert.deepEqual(de('seele/estilo').botoes.map(b => b.saida), ['atualizar', 'desligar'],
      `o estilo não tem ATUALIZAR e DESLIGAR, nesta ordem: ${JSON.stringify(de('seele/estilo'))}`);
    assert.ok(de('seele/estilo').botoes.every(b => b.visivel), 'uma saída do estilo não aparece');
    assert.match(de('seele/estilo').botoes[0].rotulo, /^ATUALIZAR PARA 4\.0\.0$/, 'ATUALIZAR não diz a versão da API 6, ou promete hospedar');
    assert.deepEqual(de('seele/mesa').botoes.map(b => b.saida), ['desligar'],
      `sem versão da API 6 no catálogo, a mesa oferece ATUALIZAR: ${JSON.stringify(de('seele/mesa'))}`);
    assert.match(de('seele/mesa').texto, /ainda não há versão dele para a API 6/, 'a mesa não diz que não há versão para a 6');
    assert.deepEqual(de('x/sumido').botoes.map(b => b.saida), ['atualizar', 'desligar'],
      `o pacote que falta não oferece ATUALIZAR e DESLIGAR: ${JSON.stringify(de('x/sumido'))}`);
    assert.doesNotMatch(de('x/sumido').texto, /API 0/, 'o pacote que falta escreveu «API 0»');

    // 3. A versão do servidor é uma escolha, fora dos MODs.
    const da015 = await page.evaluate(() => ({
      nosMods: document.querySelectorAll('#boot-saidas [data-saida^="servidor-"]').length,
      naVersao: document.querySelectorAll('#antes-versao-lista [data-saida="servidor-0.15"]').length,
      frase: $('antes-versao-frase').textContent,
    }));
    assert.equal(da015.nosMods, 0, 'hospedar por outra versão voltou para dentro de cada MOD');
    assert.equal(da015.naVersao, 1, `o bloco QUEM CONSEGUE ENTRAR não oferece o Servidor 0.15 uma vez: ${JSON.stringify(da015)}`);
    assert.match(da015.frase, /seele\/estilo, seele\/mesa só rodam no Servidor 0\.15/, `a frase não diz quais MODs só rodam no 0.15: ${da015.frase}`);

    // 4. ATUALIZAR: baixa pelo «obter» e GUARDA a escolha (P1 do Codex): nada
    //    vai ao banco parado, nada hospeda, e a linha diz «a aplicar», e não «feito».
    await page.evaluate(() => { testCalls.length = 0; });
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="atualizar"]');
    await esperar(() => document.querySelector('#boot-saidas [data-mod="seele/estilo"]').dataset.resolvido === 'atualizar',
      'ATUALIZAR não marcou a escolha na linha');
    const baixou = await page.evaluate(() => chamadas('instalar_mod_do_catalogo'));
    assert.deepEqual(baixou.map(c => c.args), [{ id: 'seele/estilo', versao: '4.0.0', uso: 'obter' }],
      `ATUALIZAR não baixou a versão da 6 pelo «obter»: ${JSON.stringify(baixou)}`);
    // O `resolver_mod_de_api_antiga` só ESTAGIA no backend (src/escolhas_de_mods.rs):
    // o banco não muda, e o `hospedar` aplica depois da cópia.
    const resolveu = await page.evaluate(() => chamadas('resolver_mod_de_api_antiga'));
    assert.deepEqual(resolveu.map(c => c.args), [{ servidor: 'srv-1', id: 'seele/estilo', saida: { atualizar: { hash: 'h40' } } }],
      `ATUALIZAR não estagiou o hash da 4.0.0 no servidor que recusou: ${JSON.stringify(resolveu)}`);
    assert.equal((await page.evaluate(() => chamadas('hospedar'))).length, 0, 'ATUALIZAR hospedou sozinho');
    const linhaEstilo = (await page.evaluate(() => saidas())).find(v => v.mod === 'seele/estilo');
    assert.match(linhaEstilo.texto, /A APLICAR AO HOSPEDAR: ATUALIZAR PARA 4\.0\.0/, `a linha não diz que a escolha é a aplicar: ${linhaEstilo.texto}`);
    assert.doesNotMatch(linhaEstilo.texto, /ATUALIZADO/, 'a linha diz «feito» antes de aplicar');
    assert.equal(await page.evaluate(() => $('antes-hospedar').getAttribute('aria-disabled')), 'true', 'HOSPEDAR aceso com dois MODs pendentes');
    assert.doesNotMatch(await page.evaluate(() => $('antes-resumo').textContent), /[Nn]ada foi mudado|nada mudou/,
      'o resumo afirma que nada mudou');

    // 6. Uma recusa da saída é dita, e não hospeda.
    await page.evaluate(() => { testCalls.length = 0; testTable.resolver_mod_de_api_antiga = () => { throw 'ServidorNoAr'; }; });
    await page.click('#boot-saidas [data-mod="seele/mesa"] [data-saida="desligar"]');
    await esperar(() => /PARE A HOSPEDAGEM/.test($('boot-saidas').textContent), 'a recusa da saída não é dita debaixo das saídas');
    assert.equal((await page.evaluate(() => chamadas('hospedar'))).length, 0, 'com a saída recusada, hospedou');
    assert.equal(await page.evaluate(() => document.querySelector('#boot-saidas [data-mod="seele/mesa"]').dataset.resolvido ?? null), null,
      'a saída recusada marcou a linha como escolhida');
    await page.evaluate(() => { testTable.resolver_mod_de_api_antiga = null; });

    // 5. DESLIGAR os outros dois acende o HOSPEDAR, que diz que aplica as
    //    escolhas, e hospeda o mesmo servidor.
    await page.evaluate(() => { testCalls.length = 0; });
    await page.click('#boot-saidas [data-mod="seele/mesa"] [data-saida="desligar"]');
    await page.click('#boot-saidas [data-mod="x/sumido"] [data-saida="desligar"]');
    await esperar(() => $('antes-hospedar').getAttribute('aria-disabled') === 'false', 'com os três MODs escolhidos, o HOSPEDAR continua apagado');
    assert.deepEqual((await page.evaluate(() => chamadas('resolver_mod_de_api_antiga'))).map(c => c.args), [
      { servidor: 'srv-1', id: 'seele/mesa', saida: 'desligar' },
      { servidor: 'srv-1', id: 'x/sumido', saida: 'desligar' },
    ], 'DESLIGAR não estagiou «desligar» no servidor que recusou');
    assert.equal(await page.evaluate(() => $('antes-mods').dataset.estado), 'leia', 'o bloco dos MODs não diz LEIA ANTES (escolhido, a aplicar)');
    assert.match(await page.evaluate(() => $('antes-hospedar-motivo').textContent), /Aplica as 3 escolhas de MOD depois da cópia/,
      'o botão do fim não diz que aplica as escolhas');
    assert.equal(await page.evaluate(() => $('antes-hospedar').textContent), 'APLICAR AS ESCOLHAS E HOSPEDAR',
      'o botão do fim não diz que aplica');
    assert.doesNotMatch(await page.evaluate(() => $('antes-versao-frase').textContent), /só rodam no Servidor 0\.15/,
      'depois de escolhidos, os MODs continuam prendendo o servidor ao 0.15 na frase');
    await page.click('#antes-hospedar');
    await esperar(() => chamadas('hospedar').length === 1, 'HOSPEDAR não hospedou');
    assert.equal((await page.evaluate(() => chamadas('hospedar')))[0].args.servidor, 'srv-1', 'HOSPEDAR hospedou outro servidor');

    // 5b. Reaberta, a tela mostra as escolhas que o backend guardou, como «a aplicar».
    await page.evaluate(() => {
      testTable.antes_de_hospedar = {
        servidor: 'srv-1', banco: 'seele.db', esquema: { Gravado: 13 }, linha_dos_dados: '0.15', verificado: true,
        motivo: null, acoes: [], converte: true, futuro: false, vai_copiar: true, copias: [],
        mods_de_api_antiga: [['seele/estilo', 4]],
        escolhas_pendentes: [{ id: 'seele/estilo', saida: 'desligar' }],
      };
      window.reaberta = hospedarDaLista({ id: 'srv-1', nome: 'Casa', versao: '0.15.1', caminho: null });
    });
    await esperar(() => document.querySelector('#boot-saidas [data-mod="seele/estilo"]')?.dataset.resolvido === 'desligar',
      'a escolha guardada no backend não apareceu na tela reaberta');
    let reaberta = (await page.evaluate(() => saidas()))[0];
    assert.match(reaberta.texto, /A APLICAR AO HOSPEDAR: DESLIGAR/);
    assert.deepEqual(reaberta.botoes.map(b => b.saida), ['desfazer'], 'a escolha guardada não pode ser desfeita');
    assert.equal(await page.evaluate(() => $('antes-hospedar').getAttribute('aria-disabled')), 'false',
      'com a única escolha guardada, o HOSPEDAR continua apagado');

    // 5c. DESFAZER: `desfazer_escolha_de_mod` do servidor e do MOD, a linha volta
    //     com as saídas, e o MOD volta a impedir.
    await page.evaluate(() => { testCalls.length = 0; });
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="desfazer"]');
    await esperar(() => document.querySelector('#boot-saidas [data-mod="seele/estilo"]').dataset.resolvido === undefined,
      'DESFAZER não tirou a escolha da linha');
    assert.deepEqual((await page.evaluate(() => chamadas('desfazer_escolha_de_mod'))).map(c => c.args),
      [{ servidor: 'srv-1', id: 'seele/estilo' }], 'DESFAZER não pediu ao backend que esquecesse a escolha');
    reaberta = (await page.evaluate(() => saidas()))[0];
    assert.deepEqual(reaberta.botoes.map(b => b.saida), ['atualizar', 'desligar'], `DESFAZER não devolveu as saídas: ${JSON.stringify(reaberta)}`);
    assert.doesNotMatch(reaberta.texto, /A APLICAR/, 'a linha continua dizendo que vai aplicar');
    assert.equal(await page.evaluate(() => $('antes-hospedar').getAttribute('aria-disabled')), 'true',
      'desfeita a escolha, o HOSPEDAR continua aceso');

    // 5d. Uma recusa do DESFAZER é dita, e a escolha fica.
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="desligar"]');
    await esperar(() => document.querySelector('#boot-saidas [data-mod="seele/estilo"]').dataset.resolvido === 'desligar', 'DESLIGAR não marcou');
    await page.evaluate(() => { testTable.desfazer_escolha_de_mod = () => { throw { NaoGuardou: 'disco cheio' }; }; });
    await page.click('#boot-saidas [data-mod="seele/estilo"] [data-saida="desfazer"]');
    await esperar(() => /NÃO FOI GUARDADA: disco cheio/.test($('boot-saidas').textContent), 'a recusa do DESFAZER não é dita');
    assert.equal(await page.evaluate(() => document.querySelector('#boot-saidas [data-mod="seele/estilo"]').dataset.resolvido), 'desligar',
      'a recusa do DESFAZER tirou a escolha da tela');
    await page.evaluate(() => { testTable.desfazer_escolha_de_mod = null; testTable.antes_de_hospedar = null; });

    // 7. O catálogo fora do ar: a falha dele, e não «não há versão».
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

    // 8. HOSPEDAR PELO SERVIDOR 0.15: a tela das versões, com a 0.15 escolhida.
    vistas = await desenhadas();
    await page.click('#antes-versao-lista [data-saida="servidor-0.15"]');
    await esperar(() => !$('tela-preparar').hidden && !$('preparar-versao-aviso').hidden,
      'a saída da 0.15 não abriu a tela das versões com uma escolhida e a frase dela');
    const preparar = await page.evaluate(() => ({
      antes: $('tela-antes-de-hospedar').hidden,
      escolhida: [...document.querySelectorAll('#preparar-lista-versoes li')]
        .filter(li => li.querySelector('button')?.textContent === 'ESCOLHIDA')
        .map(li => li.textContent),
      aviso: $('preparar-versao-aviso').textContent,
      confirmar: $('preparar-confirmar').textContent,
    }));
    assert.equal(preparar.antes, true, 'a tela ANTES DE HOSPEDAR ficou por cima da de preparar');
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
  console.log('saídas de quem hospeda: a tela própria com os três blocos, ATUALIZAR e DESLIGAR por MOD sem hospedar sozinhos, o 0.15 como escolha do servidor, a recusa dita, o catálogo fora do ar dito, e o HOSPEDAR só com tudo resolvido.');
})().catch(e => { console.error(e); process.exit(1); });
