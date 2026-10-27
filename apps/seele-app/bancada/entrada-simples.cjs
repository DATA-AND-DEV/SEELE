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
  await mobile.locator('.cel-entrada').waitFor({state:'visible'});
  await mobile.getByRole('button',{name:'COLAR UM LINK',exact:true}).waitFor({state:'visible'});
  await mobile.waitForTimeout(200);
  const detalhes=mobile.locator('.cel-entrada-identidade');
  assert.equal(await detalhes.getAttribute('open'),null);
  assert.equal(await mobile.locator('.cel-entrada-origem-nova').count(),0);
  if(pasta) await mobile.screenshot({path:path.join(pasta,`${engine}-web-mobile.png`)});
  await detalhes.locator('summary').click();
  assert.ok(await detalhes.locator('button').isVisible());
  await detalhes.locator('button').click();
  await mobile.waitForFunction(()=>!document.querySelector('.cel-entrada-identidade'));
  assert.equal(await mobile.evaluate(()=>localStorage.getItem('seele.origem-nova-dita')),'1');
  await mobile.setViewportSize({width:320,height:568});
  const colar=mobile.getByRole('button',{name:'COLAR UM LINK',exact:true});
  assert.ok(await colar.isVisible());
  const r=await colar.boundingBox();assert.ok(r.y>=0&&r.y+r.height<=568,JSON.stringify(r));
  await colar.click();
  assert.ok(await mobile.locator('textarea:visible,input:visible').count()>0);
  if(pasta) await mobile.screenshot({path:path.join(pasta,`${engine}-web-colar-320.png`)});
  await mobile.close();
  console.log(`${engine}: entrada/preparar 1280/390/320, ações acessíveis, sem campos removidos ou erros PASS`);
  } finally {await browser.close();}
 }} finally {server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
