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
  // 0.19: sem barra superior. As entradas moram na coluna de canais, sob «DOS
  // MODS»; a volta à conversa é escolher um canal. Em janela estreita a coluna
  // é a gaveta, e a porta dela (`.linha-canais`) a abre.
  assert.equal(await page.locator('.abas-superiores').count(),0,'a barra superior voltou no desktop');
  const nav=page.locator('.painel-canais #lista-mods-navegacao');
  const abrirAColuna=async()=>{const porta=page.locator('.linha-canais');if(await porta.isVisible().catch(()=>false)&&await porta.getAttribute('aria-expanded')!=='true')await porta.click();};
  const marca=(id)=>page.evaluate(i=>document.querySelector(`#lista-mods-navegacao [data-mod-da-acao="${i}"]`)?.getAttribute('aria-current')??null,id);
  const mesa=nav.getByRole('button',{name:'Mesa — de bancada/mesa'}),perfis=nav.getByRole('button',{name:'Perfis — de bancada/perfis'});
  assert.equal(await nav.locator('.entrada-de-mod').count(),3);
  await abrirAColuna();assert.equal(await page.locator('#cabeca-mods-navegacao').isHidden(),false,'o rótulo DOS MODS não aparece');
  // A coluna aberta cabe na janela e continua larga para ler um canal (os 180px
  // da `desktop-estreito.cjs`). Medida aqui, com o nome dela: em 390px a gaveta
  // saía 14px da janela e a falha vinha lá embaixo como «MOD causou overflow»,
  // acusando o MOD de um transbordo que acontece sem MOD nenhum.
  const coluna=await page.locator('#painel-canais').evaluate(el=>{const r=el.getBoundingClientRect();return {left:Math.round(r.left),right:Math.round(r.right),width:Math.round(r.width),janela:innerWidth}});
  assert.ok(coluna.left>=0&&coluna.right<=coluna.janela&&coluna.width>=180,`${width}: a coluna de canais sai da janela ou encolheu demais: ${JSON.stringify(coluna)}`);
  await mesa.click();await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  assert.equal(await marca('bancada/mesa'),'page','a entrada da página aberta não está marcada');
  await abrirAColuna();await page.locator('#lista-linhas button[data-linha]').first().click();
  await page.waitForTimeout(150);assert.equal(await page.locator('#palco-de-paginas').isVisible(),false,'escolher um canal não voltou à conversa');
  assert.equal(await marca('bancada/mesa'),null);
  await abrirAColuna();await mesa.click();await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  assert.equal(await page.locator('#palco-de-paginas [data-mod="bancada/mesa"]').count(),1,'reabrir deixou DOM duplicado');
  await page.waitForTimeout(100);await abrirAColuna();await mesa.focus();await page.keyboard.press('Shift+Tab');
  assert.equal(await page.evaluate(()=>document.activeElement.dataset.modDaAcao),'bancada/perfis', await page.evaluate(()=>document.activeElement.outerHTML));
  await page.keyboard.press('Enter');await page.getByRole('heading',{name:'Perfis de demonstração'}).waitFor({state:'visible'});
  assert.equal(await marca('bancada/perfis'),'page');
  const alvo=await mesa.boundingBox();assert.ok(alvo&&alvo.height>=44,`a entrada tem menos de 44px: ${JSON.stringify(alvo)}`);
  await page.waitForTimeout(100);await abrirAColuna();await mesa.focus();await page.keyboard.press('Enter');await page.getByRole('heading',{name:'Mesa de demonstração'}).waitFor({state:'visible'});
  const largo=await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(n=>n.getBoundingClientRect().right>innerWidth+1&&n.getClientRects().length).slice(0,6).map(n=>{const c=getComputedStyle(n);return `${n.tagName}.${n.className}#${n.id}:${Math.round(n.getBoundingClientRect().left)}-${Math.round(n.getBoundingClientRect().right)} w=${c.width} min=${c.minWidth} max=${c.maxWidth} pos=${c.position} vw=${innerWidth}`}));
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false,'MOD causou overflow horizontal: '+JSON.stringify(largo));
  assert.deepEqual(erros,[]);
  await page.screenshot({path:path.join(pasta,`mods-mesa-${width}.png`)});
  console.log(`${width}: 3 contribuições em DOS MODS, Mesa/canal/Mesa, Tab/Enter, aria-current, 44px e overflow PASS`);
  await page.close();
 }}finally{await browser.close();server.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
