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

   // ---- os extremos da janela: a mínima do `tauri.conf.json` (720×480), a de
   // 1280×800 e a de 2560×1440. Quatro defeitos de 09/11, cada um com a medida
   // que o acusa: caixas que se cobrem, e não o texto de uma regra.
   const caixas=sel=>page.locator(sel).evaluateAll(els=>els.filter(el=>el.getClientRects().length).map(el=>{const r=el.getBoundingClientRect();return {top:r.top,bottom:r.bottom,left:r.left,right:r.right};}));
   const cobre=(a,b)=>a.left<b.right-1&&b.left<a.right-1&&a.top<b.bottom-1&&b.top<a.bottom-1;

   // 1. Na janela mínima as linhas da conversa não se sobrepõem: a lista rola,
   //    e nenhuma linha encolhe para menos que o próprio texto.
   await page.setViewportSize({width:720,height:480});
   await page.evaluate(async()=>{fecharChamada();alternarCanais(false);alternarPessoas(false);if(typeof sincronizarMensagens==='function')await sincronizarMensagens(99);});
   await page.waitForFunction(()=>document.querySelectorAll('#lista-mensagens > li.mensagem').length>=2);
   const linhas=await page.locator('#lista-mensagens > li').evaluateAll(els=>els.map(el=>{
    const r=el.getBoundingClientRect();let fundo=r.bottom;
    for(const f of el.querySelectorAll('*')){const c=f.getBoundingClientRect();if(c.height>0)fundo=Math.max(fundo,c.bottom);}
    return {top:r.top,bottom:r.bottom,fundoDoConteudo:fundo};
   }));
   linhas.forEach((l,i)=>{
    assert.ok(l.fundoDoConteudo<=l.bottom+1,`${engine} 720x480: a linha ${i} da conversa encolheu para menos que o texto dela ${JSON.stringify(l)}`);
    if(i)assert.ok(l.top>=linhas[i-1].fundoDoConteudo-1,`${engine} 720x480: a linha ${i} da conversa começa em cima da anterior ${JSON.stringify([linhas[i-1],l])}`);
   });

   // 4. Na janela larga a linha de mensagem tem teto: 76 caracteres de corpo
   //    depois da calha da hora, e não a largura inteira do painel.
   await page.setViewportSize({width:2560,height:1440});
   const larga=await page.locator('#lista-mensagens > li.mensagem').first().evaluate(el=>{
    const regua=document.createElement('span');regua.style.cssText='position:absolute;visibility:hidden;width:100ch';
    el.querySelector('.corpo').append(regua);const ch=regua.getBoundingClientRect().width/100;regua.remove();
    return {colunas:el.querySelector('.mensagem-conteudo').getBoundingClientRect().width/ch,painel:el.parentElement.getBoundingClientRect().width/ch};
   });
   assert.ok(larga.painel>120,`${engine} 2560: o painel não ficou largo o bastante para a medida valer ${JSON.stringify(larga)}`);
   assert.ok(larga.colunas<=77,`${engine} 2560x1440: a linha de mensagem atravessa o painel (${Math.round(larga.colunas)} caracteres) ${JSON.stringify(larga)}`);

   // 2. A porta «SALAS E CANAIS» da chamada é o mesmo botão em toda largura em
   //    que aparece: o estilo dela não pode morar num `@media` que não vale.
   await page.evaluate(()=>abrirChamada());
   const porta={};
   for(const width of [800,1100,1280,1920]) {
    await page.setViewportSize({width,height:800});
    assert.equal(await page.locator('.sessao-navegacao .linha-canais').isVisible(),true,`${engine} chamada ${width}: a porta dos canais`);
    porta[width]=await page.locator('.sessao-navegacao .linha-canais').evaluate(el=>{const c=getComputedStyle(el);return {fundo:c.backgroundColor,cor:c.color,borda:c.borderTopWidth+' '+c.borderTopStyle+' '+c.borderTopColor,letra:c.fontSize+' '+c.letterSpacing,altura:c.minHeight};});
    assert.deepEqual(porta[width],porta[800],`${engine} chamada ${width}: a porta dos canais perdeu o estilo que tem a 800px`);
   }

   // 3. Na janela mínima a barra de controles da chamada não cobre o nome de
   //    ninguém com a vista em repouso (sem rolar).
   await page.setViewportSize({width:720,height:480});
   await page.evaluate(()=>{document.getElementById('vista-chamada').scrollTop=0;});
   const [barra]=await caixas('#chamada-controles');
   const nomes=await caixas('#chamada-grade .chamada-cartao-nome');
   assert.ok(barra&&nomes.length>=2,`${engine} 720x480: a chamada sem barra ou sem nomes ${JSON.stringify({barra,nomes})}`);
   for(const nome of nomes)assert.ok(!cobre(barra,nome),`${engine} 720x480: a barra de controles cobre o nome de um participante ${JSON.stringify({barra,nome})}`);
   const vista=(await caixas('#vista-chamada'))[0];
   assert.ok(barra.bottom<=vista.bottom+1,`${engine} 720x480: a barra de controles sai da vista ${JSON.stringify({barra,vista})}`);

   assert.deepEqual(errors,[]);
   console.log(engine+': desktop 1600→480px, chat/chamada, servidores, canais, pessoas e Escape OK; extremos 720×480, 1280×800 e 2560×1440 sem sobreposição e com teto de linha');
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
