// UI real, ponte Tauri simulada. Oráculos do defeito atual; não homologa mídia nativa.
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const { servir, respostas, retrato } = require(path.join(root, 'apps/seele-app/bancada/telas.cjs'));
const { chromium } = require(process.env.PLAYWRIGHT || path.resolve(root, '../SEELE-MOD-PERFIS/node_modules/playwright'));
(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium.launch({headless:true});
  try {
    const page = await browser.newPage({viewport:{width:1280,height:860}});
    await page.addInitScript(table => {
      window.testTable=table; window.testCalls=[];
      window.__TAURI__={
        core:{invoke:async(cmd,args)=>{
          testCalls.push({cmd,args});
          if(cmd==='assistir' && args.quero===false && window.deferStop)
            return new Promise(ok=>{window.finishStop=ok;});
          return testTable[cmd]??null;
        }},
        event:{listen:async()=>()=>{}},
        window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}
      };
    },respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[]}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r=>{
      testTable.snapshot=r; $('tela-boot').hidden=true; $('tela-sessao').hidden=false; desenhar(r);
    },retrato());
    const multiple=await page.evaluate(()=>{
      fonteArmada=1;
      desenharBotoesDeTela({...testTable.snapshot,tela:{e_minha:false,de:2}});
      return {disabled:$('compartilhar-comecar').disabled,title:$('compartilhar-comecar').title};
    });
    assert.equal(multiple.disabled,true);
    console.log('CONFIRMED MULTIPLE SHARES: another publisher disables start',JSON.stringify(multiple));
    const cinema=await page.evaluate(async()=>{
      testTable.snapshot.tela={de:2,e_minha:false,altura:0,quadros:0,kbps:0,espectadores:1,parada:null,medida:false,pedido:null};
      desenhar(testTable.snapshot);
      abrirChamada(); telaEmCurso=7; telaQuerida=7;
      await trocarCinema(true); await pararDeVer();
      // The publisher is still transmitting; the snapshot keeps its screen present.
      botaoDeCinema(true);
      return {noCinema,telaEmCurso,telaQuerida,hiddenSiblings:document.querySelectorAll('[data-fora-do-cinema="sim"]').length};
    });
    assert.equal(cinema.noCinema,true); assert.equal(cinema.telaEmCurso,null);
    assert(cinema.hiddenSiblings>0);
    console.log('CONFIRMED CINEMA: stopping viewing leaves fullscreen layout hiding navigation',JSON.stringify(cinema));
    const race=await page.evaluate(async()=>{
      await trocarCinema(false); testCalls.length=0;
      telaEmCurso=7; telaQuerida=7; window.deferStop=true;
      const switching=trocarDeTransmissao(8);
      await pararDeVer(); window.deferStop=false; window.finishStop(); await switching;
      return {telaQuerida,calls:testCalls.filter(c=>c.cmd==='assistir')};
    });
    assert.equal(race.telaQuerida,null);
    assert(race.calls.some(c=>c.args.tela===8 && c.args.quero===true));
    console.log('CONFIRMED STOP RACE: pending switch subscribes after stop choice',JSON.stringify(race));
    // ScreenOpened has another guard; this proves an obsolete request, not video resurfacing.
  } finally {await browser.close(); server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
