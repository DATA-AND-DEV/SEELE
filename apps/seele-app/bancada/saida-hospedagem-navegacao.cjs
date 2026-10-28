// Corridas reais da casca, com ponte determinística: respostas antigas não
// ressuscitam uma sessão encerrada nem roubam a entrada/foco de outra chegada.
const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium, webkit } = require('./playwright.cjs');
const s = (id) => ({carimbo:{sessao:id,geracao:1},nome:`Servidor ${id}`,endereco:`s${id}:8383`,na_vista:id===1,foco_de_voz:id===1,na_sala_de_voz:null,onde:id===1?'Guardado':'Outro',enlace:'online',encerrada:null,nao_lidas:0,mudou:false});
(async()=>{
 const server=servir();await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 const browser=await (process.argv.includes('--webkit') ? webkit() : chromium()).launch({headless:true});
 try {
  const page=await browser.newPage({viewport:{width:1280,height:860}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(base=>{
   window.tabela=base;window.chamadas=[];window.ouvintes=new Map();window.pendentes=[];
   window.__TAURI__={core:{invoke:async(cmd,args)=>{
    chamadas.push({cmd,args});
    if(window.segurar===cmd){window.segurar=null;return new Promise((resolve,reject)=>pendentes.push({cmd,args,resolve,reject}));}
    if(cmd==='disconnect'){tabela.sessoes=tabela.sessoes.filter(s=>s.carimbo.sessao!==args.carimbo.sessao);return null;}
    if(cmd==='ver_sessao')return {carimbo:{sessao:args.sessao,geracao:1},vista:args.sessao,snapshot:tabela.snapshot};
    if(cmd==='parar_de_hospedar'){tabela.meu_servidor=null;emitir({sessao:1,geracao:1,evento:{Ended:{reason:'ServerShutdown'}}});return true;}
    return structuredClone(tabela[cmd]??null);
   }},event:{listen:async(name,fn)=>{if(!ouvintes.has(name))ouvintes.set(name,[]);ouvintes.get(name).push(fn);return()=>{};}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};
   window.emitir=payload=>{for(const fn of ouvintes.get('seele://event')??[])fn({payload});};
   window.aguardarPedido=async()=>{for(let i=0;i<100&&!pendentes.length;i++)await new Promise(ok=>setTimeout(ok,0));if(!pendentes.length)throw new Error('IPC esperado não foi emitido');};
  },{...respostas({microfones:[],saidas:[],meu_servidor:{alvo:'s1:8383',nome:'Casa'}}),snapshot:retrato({server:'Casa'}),sessoes:[s(1),s(2)]});
  if (process.argv.includes('--mutante-navegacao')) {
   const fs=require('node:fs');const path=require('node:path');
   const file=path.join(__dirname,'../ui/camada-sessoes.js');
   const src=fs.readFileSync(file,'utf8');
   const guarda='  epocaDaVista += 1;\n  trocasDeVista += 1;\n  trocaEmCurso = null;';
   assert.ok(src.includes(guarda));
   await page.route('**/camada-sessoes.js*',route=>route.fulfill({contentType:'text/javascript',body:src.replace(guarda,'')}));
  }
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.evaluate(async()=>{for(const t of document.querySelectorAll('section.tela'))t.hidden=true;$('tela-sessao').hidden=false;await recarregarSessoes();desenhar(tabela.snapshot);});
  // A saída já revoga os eventos enquanto aguarda IPC. Se o transporte
  // falha, devolve a vista original sem inventar uma saída bem-sucedida.
  const saidaRecusada=await page.evaluate(async()=>{
   segurar='disconnect';window.saidaPendente=ejetar().catch(()=>{});
   emitir({sessao:1,geracao:1,evento:{Ended:{reason:'ServerShutdown'}}});
   const durante={vista:vistaDaJanela.sessao,fim:!$('tela-fim').hidden};
   pendentes.shift().reject(new Error('transporte indisponível'));await saidaPendente;
   return {durante,vistaDepois:vistaDaJanela.sessao};
  });
  assert.equal(saidaRecusada.durante.vista,null);assert.equal(saidaRecusada.durante.fim,false);
  assert.equal(saidaRecusada.vistaDepois,1,'falha de saída perdeu a sessão ainda aberta');
  // A consulta foi feita ANTES de SAIR; a resposta chega depois da saída.
  const afterExit=await page.evaluate(async()=>{
   const velha=structuredClone(tabela.sessoes);segurar='sessoes';window.leituraVelha=recarregarSessoes();
   await ejetar();pendentes.shift().resolve(velha);await leituraVelha;
   await pararDeHospedarOMeuServidor();
   return {vista:vistaDaJanela.sessao,boot:!$('tela-boot').hidden,fim:!$('tela-fim').hidden,ids:sessoesNaTela.map(s=>s.carimbo.sessao)};
  });
  assert.equal(afterExit.vista,null,'leitura antiga ressuscitou a sessão encerrada');
  assert.equal(afterExit.boot,true,'parar hospedagem roubou a entrada');assert.equal(afterExit.fim,false,'SAIR e PARAR ofereceram reconexão indevida');
  await page.evaluate(async()=>{await recarregarSessoes();await verSessao(2);$('campo-mensagem').value='rascunho da sessão 2';});
  // A pessoa recebe/abre outra chegada enquanto uma volta de sessão aguarda IPC.
  const navigation=await page.evaluate(async()=>{
   segurar='ver_sessao';window.voltaPendente=verSessao(1);await aguardarPedido();
   irParaAEntradaSemSair();const foco=document.activeElement.id;
   pendentes.shift().resolve({carimbo:{sessao:1,geracao:1},vista:1,snapshot:tabela.snapshot});await voltaPendente;
   return {boot:!$('tela-boot').hidden,sessao:!$('tela-sessao').hidden,focoAntes:foco,focoDepois:document.activeElement.id,rascunho:estadoPorSessao.get(2)?.campo,comandos:chamadas.map(c=>c.cmd)};
  });
  assert.equal(navigation.boot,true,'resposta velha roubou a entrada de outra chegada');assert.equal(navigation.sessao,false);
  assert.equal(navigation.focoDepois,navigation.focoAntes,'troca velha roubou foco');
  assert.equal(navigation.rascunho,'rascunho da sessão 2');assert.equal(navigation.comandos.includes('focar_voz'),false);
  // A vista já respondeu/desenhou, mas espera a lista para anunciar/focar:
  // uma nova chegada nesse intervalo também vence o anúncio atrasado.
  const depoisDaResposta=await page.evaluate(async()=>{
   segurar='sessoes';window.focoPendente=verSessao(2);await aguardarPedido();
   irParaAEntradaSemSair();const foco=document.activeElement.id;
   pendentes.shift().resolve(structuredClone(tabela.sessoes));await focoPendente;
   return {boot:!$('tela-boot').hidden,focoAntes:foco,focoDepois:document.activeElement.id};
  });
  assert.equal(depoisDaResposta.boot,true);assert.equal(depoisDaResposta.focoDepois,depoisDaResposta.focoAntes,'anúncio de vista antigo roubou foco após reler sessões');
  assert.deepEqual(errors,[]);console.log('PASS: saída + parar hospedagem, leitura antiga, navegação antes/depois do IPC, rascunho e foco preservados');
 }finally{await browser.close();await new Promise(ok=>server.close(ok));}
})().catch(e=>{console.error(e);process.exitCode=1;});
