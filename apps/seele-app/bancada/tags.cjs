// Identidade apresentada por PersonId: UI real, ponte fixture, duas engines.
const assert=require('node:assert/strict');
const {servir,respostas,retrato}=require('./telas.cjs');
const engines=require('./playwright.cjs');
(async()=>{
 const server=servir();await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 try{for(const engine of ['chromium','webkit']){
  const browser=await engines[engine]().launch({headless:true});
  try{
   const page=await browser.newPage({viewport:{width:1280,height:800}});
   await page.addInitScript(table=>{window.testTable=table;window.__TAURI__={core:{invoke:async cmd=>testTable[cmd]??null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};},respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[]}));
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   await page.evaluate(r=>{
    r.identidades_apresentadas={'1':['é'.repeat(16),'AB01'],'2':['é'.repeat(16),'AB02']};
    r.nickname='é'.repeat(13)+'#AB01';
    for(const sala of r.voice_rooms)for(const p of sala.people){if(p.id===1||p.id===2)p.nickname='é'.repeat(13)+(p.id===1?'#AB01':'#AB02');}
    testTable.snapshot=r;$('tela-boot').hidden=true;$('tela-sessao').hidden=false;desenhar(r);
   },retrato());
   const base='é'.repeat(16);
   for(const tag of ['AB01','AB02']) {
    const nome=page.locator('#lista-roster .pessoa-nome-com-tag').filter({hasText:'#'+tag}).first();
    assert.equal(await nome.locator('.pessoa-nome').innerText(),base);
    assert.ok(await nome.locator('.pessoa-tag-nativa').isVisible());
    const dentro=await nome.evaluate(e=>{const a=e.getBoundingClientRect(),b=e.querySelector('.pessoa-tag-nativa').getBoundingClientRect();return b.left>=a.left && b.right<=a.right+1;});
    assert.ok(dentro,'tag não pode ser cortada junto com base');
   }
   await page.evaluate(()=>abrirPerfil());
   assert.equal(await page.locator('#perfil-apelido').inputValue(),base);
   assert.ok((await page.locator('#perfil-titulo').innerText()).includes('#AB01'));
   await page.evaluate(()=>fecharPerfil());
   const chat=await page.evaluate(()=>itemDeMensagem({author:2,author_nickname:'alias#AB02',body:'oi',at_seconds:1,id:1,own:false},0,false).querySelector('.mensagem-autor').textContent);
   assert.equal(chat,base+'#AB02');
   const fora=await page.evaluate(()=>grupoForaDoAr([{id:2,nickname:'alias#AB02'}]).textContent);
   assert.ok(fora.includes(base+'#AB02'));
   await page.evaluate(()=>{entrarNaGeracao(1);window.i=new InstanciaDeMod('teste/perfis','hash',geracaoDaSessao,{entregar(){},encerrar:async()=>{}});i.estado=ESTADOS_DE_MOD.ativa;modsCarregados.set('teste/perfis',i);contribuicoesDosMods.registrar({id:'teste/perfis'},i,{ponto:'pessoa.cartao',modo:'substituir',alvo:'2',conteudo:[{forma:'texto',chave:'nome',dentro:'PERSONAGEM'}]});desenhar(testTable.snapshot);});
   const faixa=page.locator('li.pessoa-apresentada[data-pessoa-id="2"] .pessoa-apresentada-faixa').first();
   assert.equal(await faixa.locator('.pessoa-tag-nativa').innerText(),'#AB02');
   assert.ok(!(await faixa.innerText()).includes(base));
   assert.ok(await faixa.locator('.pessoa-tag-nativa').isVisible());
   // Sem metadado (servidor10), o alias original continua íntegro.
   assert.equal(await page.evaluate(()=>apelidoApresentado(2,'antigo#XXXX',{})),'antigo#XXXX');
   console.log(`${engine}: base32Unicode, tags distintas, chat, offline, cartão MOD e fallback10 PASS`);
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
