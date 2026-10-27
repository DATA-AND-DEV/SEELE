// Mede somente a espera da UI antes da montagem: ponte e executor são fixtures.
// Aceite, assinatura e execução real continuam nas bancadas próprias.
const assert = require('node:assert/strict');
const fs = require('node:fs'); const path = require('node:path');
const { servir, respostas } = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
 const server = servir(); await new Promise(ok => server.listen(0, '127.0.0.1', ok));
 try { for (const engine of ['chromium','webkit']) {
  const browser = await engines[engine]().launch({headless:true});
  try { for (const antes of [true,false]) {
   const page = await browser.newPage();
   await page.addInitScript(t => { window.table=t;window.__TAURI__={core:{invoke:async cmd=>table[cmd]??null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}}; }, respostas({snapshot:null,amigos:[],contatos_da_agenda:[],microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[]}));
   if(antes) await page.route('**/base.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../ui/base.js'),'utf8').replace('if (geracaoDaSessao && geracaoDaSessao !== anterior) queueMicrotask(pedirCargaDosMods);','')}));
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   await page.waitForFunction(()=>!conferindoMods);
   await page.evaluate(()=>{
    window.mounted=[];window.catalogRequests=0;
    table.mods_instalados=[{id:'fixture/mod',hash:'hash-autorizado',client:true}];
    pedirAoServidor=async()=>{catalogRequests++;return {ok:true,mods:[{id:'fixture/mod',hash:'hash-autorizado'}]};};
    montarOMod=async mod=>mounted.push({id:mod.id,ms:performance.now()-window.entered});
    window.entered=performance.now();entrarNaGeracao(1);
   });
   await page.waitForFunction(()=>mounted.length===1,{},{timeout:6000});
   const result=await page.evaluate(()=>({ms:mounted[0].ms,requests:catalogRequests}));
   assert.equal(result.requests,1); assert.ok(antes?result.ms>2000:result.ms<1000,JSON.stringify(result));
   if(!antes) {
    // Entradas repetidas não montam de novo; hash diferente continua recusado.
    await page.evaluate(async()=>{entrarNaGeracao(1);await carregarMods();});
    assert.equal(await page.evaluate(()=>mounted.length),1);
    await page.evaluate(async()=>{modsCarregados.clear();table.mods_instalados[0].hash='outro';await carregarMods();});
    assert.equal(await page.evaluate(()=>mounted.length),1);
    assert.equal(await page.evaluate(()=>estadoDosMods.get('fixture/mod').fase),'outra-versao');
    // Troca enquanto a conferência anterior aguarda: reexecuta na geração nova.
    await page.evaluate(async()=>{
     table.mods_instalados[0].hash='hash-autorizado';
     let finish; const original=pedirAoServidor;
     pedirAoServidor=()=>new Promise(ok=>finish=ok);
     window.inFlight=carregarMods();
     while(!finish) await Promise.resolve();
     entrarNaGeracao(2);await Promise.resolve();
     pedirAoServidor=original;finish({ok:true,mods:[]});await inFlight;
    });
    await page.waitForFunction(()=>mounted.length===2);
   }
   console.log(JSON.stringify({engine,caso:antes?'antes-timer':'entrada-imediata',...result}));
   await page.close();
  }} finally {await browser.close();}
 }} finally {server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
