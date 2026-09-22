// UI real em Chromium, ponte Tauri simulada. Oráculos dos defeitos encontrados.
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const { servir, respostas, retrato } = require(path.join(root, 'apps/seele-app/bancada/telas.cjs'));
const { chromium } = require(process.env.PLAYWRIGHT || path.resolve(root, '../SEELE-MOD-PERFIS/node_modules/playwright'));
(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium.launch({headless: true});
  try {
    const page = await browser.newPage({viewport:{width:1280,height:860}});
    await page.addInitScript(table => {
      window.testTable = table; window.testCalls = [];
      window.__TAURI__ = {
        core: {invoke: async (cmd,args) => {
          window.testCalls.push({cmd,args});
          if (cmd === 'send_message' && window.deferSend) return new Promise((ok,fail) => { window.rejectSend = fail; });
          return window.testTable[cmd] ?? null;
        }},
        event: {listen: async () => () => {}},
        window: {getCurrentWindow: () => ({onCloseRequested(){},isMaximized:async()=>false})}
      };
    }, respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[]}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => { testTable.snapshot=r; $('tela-boot').hidden=true; $('tela-sessao').hidden=false; desenhar(r); }, retrato());
    const stuck = await page.evaluate(() => {
      document.activeElement?.blur(); testCalls.length=0;
      window.dispatchEvent(new KeyboardEvent('keydown',{code:'Space',key:' ',bubbles:true}));
      $('campo-mensagem').focus();
      window.dispatchEvent(new KeyboardEvent('keyup',{code:'Space',key:' ',bubbles:true}));
      const result={falando,calls:testCalls.filter(c=>c.cmd==='set_talking')};
      segurarFala(false);
      return result;
    });
    assert.equal(stuck.falando,true);
    assert.deepEqual(stuck.calls.map(c=>c.args.talking),[true]);
    console.log('CONFIRMED PTT: release while composer focused leaves talking=true',JSON.stringify(stuck));
    const draft = await page.evaluate(async () => {
      linhaAberta=1; anexoPendente=null; window.deferSend=true;
      $('campo-mensagem').value='first message';
      const pending=enviar({preventDefault(){}});
      $('campo-mensagem').value='new draft';
      window.rejectSend('NotConnected'); await pending;
      return $('campo-mensagem').value;
    });
    assert.equal(draft,'first message');
    console.log('CONFIRMED COMPOSER: failed previous send overwrote newer draft:',draft);
    const previews=await page.evaluate(() => [
      'https://tracking.invalid/visitor.png', 'http://127.0.0.1:9999/private.png'
    ].map(url=>({url,automatic:pareceImagem(url)})));
    assert(previews.every(p=>p.automatic));
    console.log('CONFIRMED PREVIEW SELECTION (no network request):',JSON.stringify(previews));
    const scroll=await page.evaluate(() => {
      mensagens=Array.from({length:200},(_,i)=>({id:i+1,channel:1,author:2,author_nickname:'Lia',at_seconds:1726900000+i,body:`message ${i} with content`,own:false,edited:false}));
      desenharMensagens(); const list=$('lista-mensagens'); list.scrollTop=400;
      const before=list.scrollTop; desenharMensagens(); return {before,after:list.scrollTop,count:list.children.length};
    });
    console.log('SCROLL OBSERVATION:',JSON.stringify(scroll));
  } finally { await browser.close(); server.close(); }
})().catch(e=>{console.error(e);process.exitCode=1;});
