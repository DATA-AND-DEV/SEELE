// **O link do navegador na porta de quem hospeda, com o aviso de quem entra
// por ele** (D-web-6 e D-web-7, saída C). Com o HTML, o CSS e as camadas
// reais; só a ponte nativa é simulada.
//
// O navegador não abre UDP e perde só o degrau 4 do ADR 0022 (o furo pelo
// ponto de encontro). O comando `link_do_navegador` responde o link e quem de
// fora entra por ele (`AlcanceDoNavegador`, conferido no Rust em
// `so_o_degrau_4_tira_o_navegador_de_fora`); esta bancada mede o que quem
// hospeda lê ao lado do botão.
//
// # O que ela mede
//
// 1. **Alcança:** o link aparece, e o aviso diz que entra de qualquer rede.
// 2. **Só pelo encontro:** o link **continua** aparecendo (quem está na mesma
//    rede entra), e o aviso diz que pelo navegador só a rede de casa entra, por
//    quê, e o que resolve, com a porta deste servidor.
// 3. **Não se sabe:** o aviso diz que pode não abrir de fora, sem prometer.
// 4. **Só na rede**, como o convite, e um alcance que a casca não conhece cai
//    no «não se sabe».
// 5. **Sem janelas:** a linha some, e a nota diz por quê.
// 6. **O pedido leva o convite que a tela mostra**, e esquecer a porta esconde
//    tudo.
//
// Rodar:  node apps/seele-app/bancada/link-do-navegador.cjs

const assert = require('node:assert/strict');
const { servir, respostas } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

const CONVITE = 'seele://203.0.113.7:8383?fp=' + 'ab'.repeat(32) + '&v=1.0';
// A origem vem do produto (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`, a D-web-2),
// lida do `uri.rs` como o `xtask/web/links.mjs` faz: escrita aqui, ela seria a
// segunda cópia que o guarda `a_origem_do_web_app_e_escrita_num_lugar_so` recusa.
const ORIGEM = /pub const ORIGEM_DO_NAVEGADOR: &str = "([^"]+)";/.exec(
  require('node:fs').readFileSync(require('node:path').join(__dirname, '../../../crates/seele-proto/src/uri.rs'), 'utf8'),
)?.[1];
assert.ok(ORIGEM, 'a ORIGEM_DO_NAVEGADOR não foi achada no uri.rs: a leitura desta bancada quebrou');
const LINK = 'https://' + ORIGEM + '/#203.0.113.7:8383?fp=' + 'ab'.repeat(32) + '&v=1.0&wh=' + 'cd'.repeat(32) + '&ate=1794700800';

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
    const erros = [];
    page.on('pageerror', e => erros.push(e.message));
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
    }, respostas({}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    /** A porta com o link do navegador respondendo `resposta` (ou recusando). */
    const comAlcance = async (resposta) => {
      await page.evaluate(({ resposta, convite }) => {
        testCalls.length = 0;
        testTable.link_do_navegador = () => {
          if (resposta.recusa) throw resposta.recusa;
          return resposta;
        };
        guardarOLinkDaPorta(convite);
        abrirPorta();
      }, { resposta, convite: CONVITE });
      try {
        await page.waitForFunction(() => testCalls.some(c => c.cmd === 'link_do_navegador')
          && !$('porta-navegador-nota').hidden, null, { timeout: 5000 });
      } catch (falha) {
        assert.fail(`a porta não escreveu o link do navegador: erros ${erros.join(' | ') || 'nenhum'}`);
      }
      return page.evaluate(() => {
        const ve = (id) => $(id).getClientRects().length > 0;
        return {
          pedido: testCalls.find(c => c.cmd === 'link_do_navegador').args,
          linha: ve('porta-navegador'),
          linhaNaConfig: !$('porta-navegador-config').hidden,
          campo: $('porta-link-navegador').value,
          aviso: ve('porta-navegador-aviso') ? $('porta-navegador-aviso').textContent : null,
          avisoNaConfig: $('porta-navegador-aviso-config').textContent,
          curto: $('porta-navegador-aviso').classList.contains('convite-alcance-curto'),
          nota: $('porta-navegador-nota').textContent,
        };
      });
    };
    const resposta = (alcance) => ({ link: LINK, ate: 1794700800, renova: 1794200000, alcance, porta: 8383 });

    // 1. Alcança.
    let tela = await comAlcance(resposta('Alcanca'));
    assert.deepEqual(tela.pedido, { convite: CONVITE }, 'o pedido não levou o convite que a porta mostra');
    assert.ok(tela.linha && tela.linhaNaConfig, 'o link do navegador não aparece');
    assert.equal(tela.campo, LINK);
    assert.match(tela.aviso ?? '', /QUALQUER REDE/, `o aviso de quem alcança: ${tela.aviso}`);
    assert.equal(tela.curto, false, 'quem alcança ganhou a cor de quem não alcança');
    assert.equal(tela.avisoNaConfig, tela.aviso, 'o aviso da configuração não é o do diálogo');
    assert.match(tela.nota, /Vale até \d\d\/\d\d, às \d\d:\d\d/, `a nota não diz até quando: ${tela.nota}`);

    // 2. Só pelo encontro: o link fica, e o aviso diz por quê e o que resolve.
    tela = await comAlcance(resposta('SoPeloEncontro'));
    assert.ok(tela.linha && tela.campo === LINK, 'o link do navegador sumiu quando só o encontro alcança');
    assert.match(tela.aviso ?? '', /SÓ QUEM ESTÁ NA SUA REDE ENTRA/, `o aviso não diz quem entra: ${tela.aviso}`);
    assert.match(tela.aviso, /o navegador não usa o encontro/, `o aviso não diz por quê: ${tela.aviso}`);
    assert.match(tela.aviso, /porta UDP 8383/, `o aviso não diz qual porta abrir: ${tela.aviso}`);
    assert.match(tela.aviso, /IPv6/, `o aviso não oferece o IPv6: ${tela.aviso}`);
    assert.equal(tela.curto, true, 'o aviso de quem só alcança pelo encontro não tem a cor do alcance curto');

    // 3. Não se sabe.
    tela = await comAlcance(resposta('NaoSeSabe'));
    assert.ok(tela.linha, 'o link do navegador sumiu quando não se sabe');
    assert.match(tela.aviso ?? '', /PODE NÃO ABRIR DE FORA/, `o aviso de quando não se sabe: ${tela.aviso}`);
    assert.doesNotMatch(tela.aviso, /QUALQUER REDE/, 'o «não se sabe» prometeu alcance');

    // 4. Só na rede, e um alcance desconhecido.
    tela = await comAlcance(resposta('SoNaRede'));
    assert.match(tela.aviso ?? '', /SÓ FUNCIONA NA SUA REDE/, `o aviso de só na rede: ${tela.aviso}`);
    tela = await comAlcance(resposta('UmDegrauNovo'));
    assert.match(tela.aviso ?? '', /PODE NÃO ABRIR DE FORA/, `um alcance desconhecido não caiu no «não se sabe»: ${tela.aviso}`);

    // 5. Sem janelas.
    tela = await comAlcance({ recusa: 'SemJanelas' });
    assert.equal(tela.linha, false, 'sem link, a linha do navegador ficou');
    assert.equal(tela.aviso, null, 'sem link, o aviso ficou');
    assert.match(tela.nota, /Sem link do navegador/, `sem janelas, a nota não diz por quê: ${tela.nota}`);

    // 6. Esquecer a porta esconde tudo.
    await page.evaluate(() => esquecerOLinkDaPorta());
    await page.waitForFunction(() => $('porta-navegador-nota').hidden && $('porta-navegador').hidden
      && $('porta-navegador-aviso').hidden, null, { timeout: 5000 });

    assert.deepEqual(erros, [], `erros na página: ${erros.join(' | ')}`);
    console.log('link-do-navegador: o link do navegador aparece com o aviso de quem entra por ele, nos quatro alcances, e não some quando só o encontro alcança.');
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
