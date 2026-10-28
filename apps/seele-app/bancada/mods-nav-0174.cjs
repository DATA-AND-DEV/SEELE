// Navegação real da API 7: instância, registro, roteador de eventos e renderer
// do produto. Somente a resposta do executor é fixture; não monta HTML de MOD.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {servir,respostas,retrato}=require('./telas.cjs');
(async()=>{
 const pasta=process.argv[2]||'/tmp/seele-mods-nav-0174';fs.mkdirSync(pasta,{recursive:true});
 const server=servir();await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 const browser=await require('./playwright.cjs').chromium().launch({headless:true});
 try{for(const width of [1280,390,320]){
  const page=await browser.newPage({viewport:{width,height:900},reducedMotion:'reduce'}),erros=[];
  page.on('pageerror',e=>erros.push(e.message));
  await page.addInitScript(t=>{window.__TAURI__={core:{invoke:async cmd=>Object.hasOwn(t,cmd)?t[cmd]:null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}}},respostas({microfones:[],saidas:[],amigos:[],contatos_da_agenda:[]}));
  await page.goto(`http://127.0.0.1:${server.address().port}`);await page.waitForTimeout(300);
  await page.evaluate(foto=>{
   for(const el of document.querySelectorAll('.tela'))el.hidden=true;
   $('tela-sessao').hidden=false;window.desenhar(foto);
   for(const rotulo of ['Estilo','Perfis','Mesa']){
    const mod={id:`bancada/${rotulo.toLowerCase()}`,api:7};
    let instancia;
    const executor={entregar(dados){
     if(dados.nome!=='acao')return;
     const conjunto=superficiesDoMod(mod,instancia);
     conjunto.criar({id:'pagina',tipo:'pagina',titulo:rotulo,imersiva:false});
     conjunto.de('pagina').montar([{forma:'titulo',dentro:`${rotulo} de demonstração`},{forma:'texto',dentro:'Contribuição de bancada pela API 7; transporte e conteúdo ilustrativos.'}]);
    }};
    instancia=new InstanciaDeMod(mod.id,'fixture',geracaoDaSessao,executor);
    instancia.estado=ESTADOS_DE_MOD.ativa;modsCarregados.set(mod.id,instancia);
    contribuicoesDosMods.registrar(mod,instancia,{ponto:'servidor.navegacao',modo:'adicionar',rotulo,acaoPrincipal:'abrir'});
   }
   redesenharAsEntradasDeMod();
  },retrato());
  const nav=page.locator('.abas-superiores'),mesa=nav.getByRole('button',{name:'Mesa — de bancada/mesa'}),conversa=nav.getByRole('button',{name:'Conversa',exact:true});
  assert.equal(await nav.locator('.entrada-de-mod').count(),3);
  assert.equal(await page.locator('.painel-canais #lista-mods-navegacao').count(),0);
  await mesa.click();await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  await conversa.click();assert.equal(await page.locator('#palco-de-paginas').isVisible(),false);
  await mesa.click();await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  assert.equal(await page.locator('#palco-de-paginas [data-mod="bancada/mesa"]').count(),1,'reabrir deixou DOM duplicado');
  await page.waitForTimeout(100);await mesa.focus();await page.keyboard.press('ArrowLeft');
  assert.equal(await page.evaluate(()=>document.activeElement.dataset.modDaAcao),'bancada/perfis', await page.evaluate(()=>document.activeElement.outerHTML));
  await page.keyboard.press('Enter');await page.getByRole('heading',{name:'Perfis de demonstração'}).waitFor({state:'visible'});
  await page.waitForTimeout(100);await nav.getByRole('button',{name:'Perfis — de bancada/perfis'}).focus();await page.keyboard.press('Home');assert.equal(await conversa.evaluate(el=>el===document.activeElement),true);
  await page.keyboard.press('End');assert.equal(await mesa.evaluate(el=>el===document.activeElement),true);
  await page.keyboard.press('Enter');await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false,'MOD causou overflow horizontal');
  assert.deepEqual(erros,[]);
  await page.screenshot({path:path.join(pasta,`mods-mesa-${width}.png`)});
  console.log(`${width}: 3 contribuições, Mesa/Conversa/Mesa, Arrow/Home/End/Enter, foco e overflow PASS`);
  await page.close();
 }}finally{await browser.close();server.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
