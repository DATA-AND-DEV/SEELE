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
   for(const platform of ['windows','macos','web','outro']) {
    const messages=await page.evaluate(platform=>{$('barra-janela').dataset.plataforma=platform;return [fraseDeErro("SemResposta"),PERMISSAO_DE_TELA.Negada.nota,fraseDeErro("ScreenShareUnavailable")];},platform);
    if(platform==='macos')assert.match(messages.join(' '),/Ajustes do Sistema/);
    else assert.doesNotMatch(messages.join(' '),/Mac|macOS|Ajustes do Sistema/);
    if(platform==='web')assert.match(messages.join(' '),/navegador/);
   }
   const borders=await page.evaluate(()=>{const s=getComputedStyle(document.querySelector('.trilha-mais'));return [s.borderLeftColor,s.borderRightColor,s.borderLeftWidth,s.borderRightWidth];});
   assert.equal(borders[0],borders[1]);assert.equal(borders[2],borders[3]);
   await page.locator('#campo-mensagem').click();
   assert.equal(await page.evaluate(()=>getComputedStyle(document.querySelector('.compor')).borderTopWidth),'0px');
   await page.evaluate(()=>abrirCompartilhar());
   await page.locator('[data-fonte="1"]').click();
   await page.locator('#compartilhar-comecar').click();
   await page.waitForFunction(()=>$('compartilhar').hidden&&$('operador-vista').dataset.modo==='parar');
   const controls=page.locator('.chamada-controles');
   // Distingue parar e trocar; mantém o alvo DOM enquanto snapshots chegam.
   const stop=page.getByRole('button',{name:'PARAR DE COMPARTILHAR',exact:true}).filter({visible:true});
   const swap=page.getByRole('button',{name:'TROCAR TELA',exact:true}).filter({visible:true});
   assert.equal(await swap.count(),1);
   await swap.click();await page.locator('[data-fonte="2"]').click();await page.locator('#compartilhar-comecar').click();
   await page.waitForFunction(()=>$('compartilhar').hidden&&!comecando);
   const order=await page.evaluate(()=>calls.filter(c=>['compartilhar_tela','parar_de_compartilhar'].includes(c.cmd)));
   assert.deepEqual(order.map(c=>c.cmd),['compartilhar_tela','parar_de_compartilhar','compartilhar_tela']);assert.equal(order[2].args.fonte,2);
   // pointerdown em SVG, snapshot repinta, pointerup no mesmo alvo.
   const svg=stop.locator('svg');const box=await svg.boundingBox();assert.ok(box);
   await svg.evaluate(el=>window.alvoDoGesto=el);
   await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();
   await page.evaluate(()=>desenharChamada(table.snapshot));
   assert.equal(await svg.evaluate(el=>el===window.alvoDoGesto),true,'snapshot preserva o alvo do gesto');
   await page.mouse.up();
   await page.waitForFunction(()=>table.snapshot.minha_transmissao===null);
   await page.waitForFunction(()=>$('operador-trocar-tela').hidden);
   await page.evaluate(()=>abrirCompartilhar());await page.locator('[data-fonte="1"]').click();await page.locator('#compartilhar-comecar').click();
   await page.waitForFunction(()=>$('compartilhar').hidden&&!comecando);
   if(process.env.SEELE_PREVIEW_DIR)await page.screenshot({path:require('node:path').join(process.env.SEELE_PREVIEW_DIR,engine+'-chamada.png')});
   await page.evaluate(()=>{window.saiuDoServidor=0;sairDoServidorParaAEntrada=async()=>{window.saiuDoServidor++;};});
   await page.locator('#chamada-controles').getByRole('button',{name:'SAIR DO SERVIDOR',exact:true}).click();
   assert.equal(await page.evaluate(()=>window.saiuDoServidor),1);
   assert.equal(await page.evaluate(()=>calls.filter(c=>c.cmd==='leave_voice_room').length),0);
   // Os ícones têm ações distintas: X sai da call, porta sai do servidor.
   await page.getByRole('button',{name:'SAIR DA SALA',exact:true}).filter({visible:true}).click();
   assert.equal(await page.evaluate(()=>calls.filter(c=>c.cmd==='leave_voice_room').length),1);
   assert.equal(await page.evaluate(()=>calls.filter(c=>c.cmd==='disconnect').length),0);
   await page.evaluate(async()=>{
    window.corromperLista=true;
    table.avisos_dos_dados_locais=['Não consegui ler os dados locais. Arquivos preservados para recuperação.'];
    await lerOsServidoresDaEntrada();
   });
   assert.equal(await page.evaluate(()=>$('boot-dados-locais').hidden),false);
   assert.match(await page.locator('#boot-dados-locais').textContent(),/preservados/);
   assert.equal(await page.evaluate(()=>$('boot-servidores').hidden),true,'corrupção não aparece como lista de instalação nova');
   assert.deepEqual(errors,[]);
   console.log(engine+': permissão por ambiente, bordas, seleção, início, troca, parar durante repintura, reinício e saída da call OK');
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
