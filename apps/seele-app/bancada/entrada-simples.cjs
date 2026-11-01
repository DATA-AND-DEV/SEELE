// Entrada e preparação reais, com ponte fixture; sem hospedar nem alterar dados.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas } = require('./telas.cjs');
const pw = require('./playwright.cjs');
(async () => {
 const server=servir(); await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 const pasta=process.env.SEELE_PREVIEW_DIR;
 if(pasta) fs.mkdirSync(pasta,{recursive:true});
 try { for(const engine of ['chromium','webkit']) {
  const browser=await pw[engine]().launch({headless:true});
  try { for(const width of [1280,390,320]) {
   const page=await browser.newPage({viewport:{width,height:800}}), errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.addInitScript(table=>{
    window.__TAURI__={core:{invoke:async cmd=>table[cmd]??null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};
   },respostas({snapshot:null,microfones:[],saidas:[],amigos:[],linhas_para_hospedar:[{linha:2,servidor:'2.0',apis:[7],esta:true,instalada:true,tem_pacote:true}]}));
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   await page.locator('#botao-hospedar').waitFor({state:'visible'});
   assert.equal(await page.locator('#campo-nome-publico,.boot-leitura,.boot-detalhes').count(),0);
   for(const tela of ['boot','preparar']) {
    if(tela==='preparar') await page.evaluate(()=>abrirPreparar());
    await page.waitForTimeout(300);
    const geometria=await page.locator(`#tela-${tela}`).evaluate(e=>({width:e.clientWidth,scroll:e.scrollWidth}));
    assert.ok(geometria.scroll<=geometria.width+1,JSON.stringify({engine,width,tela,geometria}));
    const alvo=page.locator(tela==='boot'?'#botao-hospedar':'#preparar-confirmar');
    await alvo.scrollIntoViewIfNeeded();
    const rect=await alvo.boundingBox();assert.ok(rect.y>=0 && rect.y+rect.height<=801,JSON.stringify(rect));
    if(pasta) await page.screenshot({path:path.join(pasta,`${engine}-${tela}-${width}.png`)});
   }
   if(width===1280) {
    await page.evaluate(async()=>{fecharPreparar();await abrirServer('tela-boot');abrirSecao('secao-audio');});
    await page.waitForTimeout(200);
    if(pasta) await page.screenshot({path:path.join(pasta,`${engine}-config-audio.png`)});
   }
   assert.deepEqual(errors,[]); await page.close();
  }
  const mobile=await browser.newPage({viewport:{width:390,height:844},isMobile:true,hasTouch:true});
  await mobile.addInitScript(()=>localStorage.setItem('seele.casca','celular'));
  await mobile.goto(`http://127.0.0.1:${server.address().port}`);
  // **O início do celular é a caixa de `#tela-boot`** desde a 0.18 (`celular-entrada.css`: `.cel-entrada[data-inicio]` sai e a
  // `.boot-caixa` entra; na 0.19, o painel «Conectar»). A `.cel-entrada` fica para a «ENTRAR EM» e o colar de um link. A promessa é a
  // mesma: o caminho para entrar está à vista, o campo aceita o link e o CONECTAR cabe na janela, a 390 e a 320.
  await mobile.locator('#tela-boot .inicio-conectar').waitFor({state:'visible'});
  assert.equal(await mobile.locator('.cel-entrada').isVisible(),false,'o início mostra a seção da ENTRAR EM');
  const campo=mobile.locator('#tela-boot .inicio-conectar input');
  const conectar=mobile.getByRole('button',{name:'CONECTAR',exact:true});
  await conectar.waitFor({state:'visible'});
  await mobile.waitForTimeout(200);
  assert.equal(await mobile.evaluate(()=>document.querySelector('#tela-boot').scrollWidth<=innerWidth+1),true,'o início rola de lado a 390');
  if(pasta) await mobile.screenshot({path:path.join(pasta,`${engine}-web-mobile.png`)});
  await mobile.setViewportSize({width:320,height:568});
  await mobile.waitForTimeout(200);
  assert.equal(await mobile.evaluate(()=>document.querySelector('#tela-boot').scrollWidth<=innerWidth+1),true,'o início rola de lado a 320');
  for(const alvo of [campo,conectar]){
   await alvo.scrollIntoViewIfNeeded();
   const r=await alvo.boundingBox();assert.ok(r && r.y>=0&&r.y+r.height<=568&&r.x>=0&&r.x+r.width<=321,JSON.stringify(r));
  }
  await campo.fill('seele://exemplo.br:8383');
  assert.equal(await campo.inputValue(),'seele://exemplo.br:8383','o campo do início não aceita o link');
  if(pasta) await mobile.screenshot({path:path.join(pasta,`${engine}-web-colar-320.png`)});
  await mobile.close();
  console.log(`${engine}: entrada/preparar 1280/390/320, ações acessíveis, sem campos removidos ou erros PASS`);
  } finally {await browser.close();}
 }} finally {server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
