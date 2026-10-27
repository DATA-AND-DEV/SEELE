// Identidade apresentada por PersonId: UI real, ponte fixture, duas engines.
const assert=require('node:assert/strict');
const fs=require('node:fs'); const path=require('node:path');
const {servir,respostas,retrato}=require('./telas.cjs');
const engines=require('./playwright.cjs');
(async()=>{
 const server=servir();await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 try{for(const engine of ['chromium','webkit']){
  const browser=await engines[engine]().launch({headless:true});
  try{
   const page=await browser.newPage({viewport:{width:1280,height:800}});
   await page.addInitScript(table=>{window.testTable=table;window.testCalls=[];window.__TAURI__={core:{invoke:async (cmd,args)=>{testCalls.push({cmd,args});return testTable[cmd]??null;}},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};},respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[],amigos:[],contatos_da_agenda:[],meu_codigo:"TESTE-PERFIL"}));
   if(process.argv.includes('--mutante-capa')) await page.route('**/camada-nomear.css',route=>route.fulfill({contentType:'text/css',body:fs.readFileSync(path.join(__dirname,'../ui/camada-nomear.css'),'utf8').replace('/* A capa absoluta pertence ao cabeçalho, não à caixa inteira do editor. */\n  position: relative;','position: static;')}));
   if(process.argv.includes('--mobile')) await page.route('**/',route=>route.fulfill({contentType:'text/html',body:fs.readFileSync(path.join(__dirname,'../ui/index.html'),'utf8').replace('<html lang="pt-BR">','<html lang="pt-BR" data-casca="celular">')}));
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   await page.evaluate(r=>{
    r.identidades_apresentadas={'1':['é'.repeat(16),'AB01'],'2':['é'.repeat(16),'AB02'],'3':['é'.repeat(16),'AB03']};
    r.nickname='é'.repeat(13)+'#AB01';
    for(const p of r.presentes)if([1,2,3].includes(p.id))p.nickname='é'.repeat(13)+'#AB0'+p.id;
    const sala=r.voice_rooms.find(s=>s.occupied_by_us);sala.people.push({...sala.people.find(p=>p.id===2),id:3,nickname:'é'.repeat(13)+'#AB03'});
    for(const sala of r.voice_rooms)for(const p of sala.people){if(p.id===1||p.id===2)p.nickname='é'.repeat(13)+(p.id===1?'#AB01':'#AB02');}
    testTable.snapshot=r;$('tela-boot').hidden=true;$('tela-sessao').hidden=false;desenhar(r);
   },retrato());
   const base='é'.repeat(16);
   for(const id of [1,2,3]) {
    const nome=page.locator(`#lista-roster [data-pessoa-id="${id}"] .pessoa-nome`).first();
    assert.ok((await nome.innerText()).startsWith(base));
   }
   assert.equal(await page.locator('.pessoa-tag-nativa').count(),0);
   await page.evaluate(()=>abrirPerfil());
   assert.equal(await page.locator('#perfil-apelido').inputValue(),base);
   assert.ok(!(await page.locator('#perfil-titulo').innerText()).includes('#AB01'));
   await page.evaluate(()=>fecharPerfil());
   const chat=await page.evaluate(()=>itemDeMensagem({author:2,author_nickname:'alias#AB02',body:'oi',at_seconds:1,id:1,own:false},0,false).querySelector('.mensagem-autor').textContent);
   assert.equal(chat,base);
   const fora=await page.evaluate(()=>grupoForaDoAr([{id:2,nickname:'alias#AB02'}]).textContent);
   assert.ok(fora.includes(base));assert.ok(!fora.includes('#AB02'));
   await page.evaluate(()=>{entrarNaGeracao(1);window.i=new InstanciaDeMod('teste/perfis','hash',geracaoDaSessao,{entregar(){},encerrar:async()=>{}});i.estado=ESTADOS_DE_MOD.ativa;modsCarregados.set('teste/perfis',i);contribuicoesDosMods.registrar({id:'teste/perfis'},i,{ponto:'pessoa.cartao',modo:'substituir',alvo:'2',conteudo:[{forma:'texto',chave:'nome',dentro:'PERSONAGEM'}]});desenhar(testTable.snapshot);});
   const faixa=page.locator('li.pessoa-apresentada[data-pessoa-id="2"] .pessoa-apresentada-faixa').first();
   assert.equal(await faixa.locator('.pessoa-tag-nativa').count(),0);
   assert.ok(!(await faixa.innerText()).includes(base));
   const projetado=await page.evaluate(()=>projetarORetrato(testTable.snapshot,{onde:'computador',regioes:[]}));
   assert.ok(projetado.pessoas.every(p=>!/#AB0[123]/.test(p.apelido)));
   // Volume mantém o alias interno distinto; o nome acessível não o revela.
   await page.evaluate(()=>{desenharChamada(testTable.snapshot);});
   for(const id of [2,3]) {
    const volume=page.locator(`#lista-roster [data-pessoa-id="${id}"] input.volume`).first();
    await volume.evaluate((e,id)=>{e.value=String(100+id*10);e.dispatchEvent(new Event('input',{bubbles:true}));},id);
    const chamadas=await page.evaluate(()=>testCalls.filter(c=>c.cmd==='set_volume'));
    assert.equal(chamadas.at(-1).args.nickname,'é'.repeat(13)+'#AB0'+id);
    assert.equal(chamadas.at(-1).args.percent,100+id*10);
   }
   await page.evaluate(()=>desenharModeracao(testTable.snapshot));
   for(const id of [2,3]) assert.ok((await page.locator(`#moderar-quem option[value="${id}"]`).textContent()).startsWith(base));
   const vazou=await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(e=>
    /#AB0[123]/.test([e.getAttribute('title'),e.getAttribute('aria-label'),e.childElementCount?'':e.textContent].join(' '))
   ).map(e=>e.id||e.className));
   assert.deepEqual(vazou,[]);
   if(process.argv.includes('--mobile')) {
    await page.evaluate(()=>{const abrir=CELULAR.ajudante('folhaDaPessoa');CELULAR.registrar('folhaDaPessoa',dados=>{window.folhaPessoaId=dados.id;return abrir(dados);});});
    for(const id of [1,2,3]) {
     await page.evaluate(id=>CELULAR.ajudante('abrirPessoa')('é'.repeat(13)+'#AB0'+id),id);
     assert.equal(await page.evaluate(()=>folhaPessoaId),String(id));
     assert.ok(!(await page.locator('#cel-folha-pessoa-titulo').innerText()).includes('#AB0'));
     await page.locator('#cel-folha-pessoa .cel-folha-acoes button').click();
    }
   }
   // Capa real do MOD + sala pessoal: o editor deve permanecer acima da imagem.
   await page.evaluate(()=>{
    const c=document.createElement('canvas');c.width=400;c.height=120;const ctx=c.getContext('2d');ctx.fillStyle='#34758c';ctx.fillRect(0,0,400,120);
    testTable.ler_imagem_mod={papel:'imagem',bytes:256,uri:c.toDataURL('image/png')};
    contribuicoesDosMods.registrar({id:'teste/perfis'},i,{ponto:'pessoa.capa',modo:'substituir',alvo:'1',conteudo:{doServidor:{canal:1,campo:'image',pedido:{transporte:'volume',path:'volume:capa'}}}});
    desenhar(testTable.snapshot);
   });
   for(const width of (process.argv.includes('--mobile')?[390,320]:[1280,390,320])) {
    await page.setViewportSize({width,height:800});
    if(width===1280) await page.locator('.operador-quem').click();
    else await page.evaluate(()=>abrirPerfil());
    await page.waitForFunction(()=>!document.getElementById('perfil-sala').hidden && document.querySelector('#perfil .capa-camada'));
    await page.waitForTimeout(100);
    const medida=await page.locator('#perfil .perfil-cabeca').evaluate(e=>{const h=e.getBoundingClientRect(),c=e.querySelector('.capa-camada').getBoundingClientRect();return {cabeca:h.height,capa:c.height,top:h.top,capaTop:c.top};});
    if(process.env.SEELE_PREVIEW_DIR){fs.mkdirSync(process.env.SEELE_PREVIEW_DIR,{recursive:true});await page.screenshot({path:path.join(process.env.SEELE_PREVIEW_DIR,`${engine}-${process.argv.includes('--mobile')?'mobile-':''}perfil-${width}.png`)});}
    assert.ok(Math.abs(medida.cabeca-medida.capa)<=2,JSON.stringify(medida));
    const campo=page.locator('#perfil-apelido');await campo.scrollIntoViewIfNeeded();await campo.fill('Nome editado');
    await page.locator('#perfil-salvar').click();
    await page.waitForFunction(()=>document.getElementById('perfil').hidden);
    assert.ok((await page.evaluate(()=>testCalls)).some(c=>c.cmd==='escolher_apelido'&&c.args.apelido==='Nome editado'));
   }
   // Sem metadado (servidor10), o alias original continua íntegro.
   assert.equal(await page.evaluate(()=>apelidoApresentado(2,'antigo#XXXX',{})),'antigo#XXXX');
   console.log(`${engine}: base32Unicode, tags internas, ações por identidade, editor com capa e fallback10 PASS`);
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
