// UI real, IPC simulado; Chromium e WebKit no Mac, não Windows nativo.
const assert = require('node:assert/strict');
const {servir, respostas, retrato} = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
 const server=servir(); await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 try { for(const engine of ['chromium','webkit']) {
  const browser=await engines[engine]().launch({headless:true});
  try {
   const page=await browser.newPage({viewport:{width:1280,height:860}});
   page.setDefaultTimeout(10000);
   const errors=[];page.on('pageerror',e=>errors.push(e.message));
   await page.addInitScript(table=>{
    window.table=table;window.calls=[];window.listeners={};
    window.__TAURI__={core:{invoke:async(cmd,args)=>{
     calls.push({cmd,args});
     if(cmd==='conhecidos' && window.corromperLista)throw 'dados locais corrompidos';
     if(cmd==='compartilhar_tela')table.snapshot.minha_transmissao={pedido:args.limites,tela:7};
     if(cmd==='parar_de_compartilhar')table.snapshot.minha_transmissao=null;
     if(cmd==='leave_voice_room')table.snapshot.voice_rooms.forEach(r=>r.occupied_by_us=false);
     return table[cmd]??null;
    }},event:{listen:async(n,cb)=>{listeners[n]=cb;return ()=>{};}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};
   },respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[],previas_consentidas:[],permissao_de_tela:'Concedida',exclusao_do_som_da_captura:'Excluido',fontes_de_tela:[{id:1,nome:'Monitor',monitor:true,largura:1920,altura:1080},{id:2,nome:'Janela',monitor:false,largura:800,altura:600}]}));
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   await page.evaluate(r=>{table.snapshot=r;table.snapshot.voice_rooms[0].occupied_by_us=true;document.getElementById('tela-boot').hidden=true;document.getElementById('tela-sessao').hidden=false;desenhar(r);},retrato());

   for (const view of ['chat','call']) {
    await page.evaluate(view=>view==='chat'?fecharChamada():abrirChamada(),view);
    for(const width of [1600,1240,1100,900,800,700,640,480]) {
     await page.setViewportSize({width,height:760});
     await page.evaluate(()=>{alternarCanais(false);alternarPessoas(false);});
     const inspect=selector=>page.locator(selector).evaluate(el=>{
      const r=el.getBoundingClientRect();return {width:r.width,height:r.height,left:r.left,right:r.right,bottom:r.bottom,visible:!!el.getClientRects().length,viewport:innerWidth};
     });
     const main=await inspect('.painel-linha');
     assert.ok(main.width>=200 && main.left>=0 && main.right<=width,`${view} ${width}: chat ${JSON.stringify(main)}`);
     if(width<=1240)assert.ok(main.right>=width-32,`${view} ${width}: coluna vazia rouba espaço ${JSON.stringify(main)}`);
     assert.equal(await page.locator('.trilha').isVisible(),true,`${width}: servidores acessíveis`);
     if(width<=900 || view==='call') {
      const toggle=page.locator('.sessao-navegacao .linha-canais');
      await toggle.click();
      const panel=await inspect('.painel-canais');
      assert.ok(panel.visible&&panel.width>=180&&panel.left>=0&&panel.right<=width,`${view} ${width}: canais ${JSON.stringify(panel)}`);
      assert.equal(await toggle.getAttribute('aria-expanded'),'true');
      await page.keyboard.press('Escape');
      assert.equal(await toggle.getAttribute('aria-expanded'),'false');
      if(view==='call')assert.equal(await page.locator('#vista-chamada').isVisible(),true,'Escape fecha só a gaveta');
     }
     if(width<=1240) {
      const toggle=page.locator('.sessao-navegacao .linha-pessoas');
      await toggle.click();
      const panel=await inspect('.painel-pessoas');
      assert.ok(panel.visible&&panel.width>=180&&panel.left>=0&&panel.right<=width,`${view} ${width}: pessoas ${JSON.stringify(panel)}`);
      await page.keyboard.press('Escape');
      assert.equal(await toggle.getAttribute('aria-expanded'),'false');
      if(view==='call')assert.equal(await page.locator('#vista-chamada').isVisible(),true,'Escape fecha só a gaveta');
     }
     if(width===640&&process.env.SEELE_PREVIEW_DIR)await page.screenshot({path:require('node:path').join(process.env.SEELE_PREVIEW_DIR,engine+'-'+view+'-640.png')});
    }
   }
   assert.deepEqual(errors,[]);
   console.log(engine+': desktop 1600→480px, chat/chamada, servidores, canais, pessoas e Escape OK');
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
