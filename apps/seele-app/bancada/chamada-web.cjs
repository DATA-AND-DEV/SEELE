// Duas páginas reais + encontro Rust local + WASM estrito + RTC sem TURN.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const {spawn} = require('node:child_process');
const engines = require('./playwright.cjs');
const raiz = path.resolve(__dirname, '../../..');
const wasm = process.env.SEELE_CHAMADA_WASM || path.join(raiz,'target/wasm32-unknown-unknown/release/seele_web.wasm');
(async()=>{
 const server = http.createServer((req,res)=>{
  const nome = req.url.slice(1);
  if(nome === 'wasm') {res.end(fs.readFileSync(wasm));return;}
  if(['chamada-web.js','rtc-chamada-web.js'].includes(nome)) {res.setHeader('Content-Type','application/javascript');res.end(fs.readFileSync(path.join(raiz,'apps/seele-app/ui',nome)));return;}
  res.setHeader('Content-Type','text/html');res.end('<!doctype html><script src="/rtc-chamada-web.js"></script><script src="/chamada-web.js"></script><p>Chamada real local</p>');
 });
 await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const origem=`http://127.0.0.1:${server.address().port}`;
 const encontro=spawn(path.join(raiz,'target/debug/seele-encontro'),['--porta','0','--rede-local','--sinal','127.0.0.1:19885','--origem',origem],{stdio:['ignore','pipe','pipe']});
 let logs='';encontro.stderr.on('data',b=>logs+=b);encontro.stdout.on('data',b=>logs+=b);
 try {
  for(let i=0;i<50;i++){try{await fetch('http://127.0.0.1:19885/chamada/1');break;}catch{await new Promise(r=>setTimeout(r,100));}}
  for(const [motor,tipo] of Object.entries({chromium:engines.chromium(),webkit:engines.webkit()})){
   const navegador=await tipo.launch({headless:true});
   try{
    const contexto=await navegador.newContext(); const a=await contexto.newPage(),b=await contexto.newPage();
    for(const pagina of [a,b]){
     pagina.on('pageerror',e=>console.error(motor,e));
     await pagina.goto(origem);
     await pagina.evaluate(async()=>{
      const bytes=await(await fetch('/wasm')).arrayBuffer();
      const {instance}=await WebAssembly.instantiate(bytes,{});
      window.estados=[];window.textos=[];window.pares=[];
      window.servico=await criarServicoDeChamadasWeb({wasm:instance.exports,urlDoEncontro:'http://127.0.0.1:19885/chamada/1',iceServers:[],
       aoEstado:e=>estados.push(e),aoTexto:e=>textos.push(e),aoPar:p=>{pares.push(!!p);window.ultimoPar=p;}});
     });
    }
    const criacaoCancelada=await a.evaluate(async()=>{
      let buscas=0,publicou=false,liberar,capturou;
      const auxiliar=await criarServicoDeChamadasWeb({wasm:servico.wasm,iceServers:[],buscar:async()=>{buscas++;throw new Error('não deve buscar');},aoEstado:e=>{if(e.estado==='recebendo')publicou=true;}});
      const original=crypto.subtle.sign.bind(crypto.subtle);
      const capturada=new Promise(r=>capturou=r);
      crypto.subtle.sign=(...args)=>{const assinatura=original(...args);return new Promise((ok,nao)=>{liberar=()=>assinatura.then(ok,nao);capturou();});};
      try{
        const criando=auxiliar.criarConvite({apelido:'Cancelada'}).then(()=>false,()=>true);
        await capturada;auxiliar.destruir();liberar();
        return {recusou:await criando,buscas,publicou};
      }finally{crypto.subtle.sign=original;auxiliar.destruir();}
    });
    assert.deepEqual(criacaoCancelada,{recusou:true,buscas:0,publicou:false});
    const criptografia = await a.evaluate(async()=>{
      const zero=()=>Array(32).fill(0), sig=()=>({primeira:zero(),segunda:zero()});
      const chave=await crypto.subtle.generateKey('Ed25519',false,['sign','verify']);
      const publica=Array.from(new Uint8Array(await crypto.subtle.exportKey('raw',chave.publicKey)));
      const assinar=async bytes=>{const b=new Uint8Array(await crypto.subtle.sign('Ed25519',chave.privateKey,new Uint8Array(bytes)));return {primeira:Array.from(b.slice(0,32)),segunda:Array.from(b.slice(32))};};
      const agora=Math.floor(Date.now()/1000);
      const convite={id:Array(32).fill(2),dono:publica,expira_em:agora+300,assinatura:sig()};
      convite.assinatura=await assinar(servico.operar({acao:'convite_mensagem',convite}).bytes);
      servico.operar({acao:'convite_verificar',convite});
      const fraco=structuredClone(convite);fraco.dono=zero();fraco.dono[0]=1;fraco.assinatura={primeira:[...fraco.dono],segunda:zero()};
      let recusas=0;
      try{servico.operar({acao:'convite_verificar',convite:fraco});}catch{recusas++;}
      const fp=Array(32).fill(7),textoFp=fp.map(x=>x.toString(16).padStart(2,'0')).join(':');
      const p={papel:'QuemLiga',epoca:Array(32).fill(3),nonce:Array(16).fill(4),convite:convite.id,chamada:Array(32).fill(5),remetente:publica,destinatario:Array(32).fill(6),geracao:1,sequencia:1,valido_ate:agora+60,
      operacao:{Sinal:{Oferta:{sdp:'v=0\r\na=fingerprint:sha-256 '+textoFp+'\r\n',impressao:fp}}},assinatura:sig()};
      p.assinatura=await assinar(servico.operar({acao:'pedido_mensagem',pedido:p}).bytes);
      servico.operar({acao:'pedido_verificar',pedido:p});
      const alterar=[q=>q.nonce[0]++,q=>q.convite[0]++,q=>q.chamada[0]++,q=>q.destinatario[0]++,q=>q.remetente[0]++,q=>q.geracao++,q=>q.sequencia++,q=>q.papel='QuemRecebe',q=>q.epoca[0]++,q=>q.operacao.Sinal.Oferta.sdp+='a=sendonly\r\n',q=>q.assinatura.segunda[0]++];
      for(const mudar of alterar){const q=structuredClone(p);mudar(q);try{servico.operar({acao:'pedido_verificar',pedido:q});}catch{recusas++;}}
      return {valida:true,recusas,esperadas:alterar.length+1};
    });
    assert.equal(criptografia.recusas,criptografia.esperadas);
    console.log(JSON.stringify({motor,criptografia}));
    const link=await a.evaluate(async()=> (await servico.criarConvite({apelido:'Receptor'})).link);
    await b.evaluate(link=>servico.abrirConvite(link),link);
    assert.equal(await b.evaluate(()=>servico.ler().estado),'livre');
    await b.evaluate(()=>servico.chamar({apelido:'Visitante'}));
    await a.waitForFunction(()=>servico.ler().estado==='tocando',{timeout:12000});
    assert.equal(await b.evaluate(()=>pares.includes(true)),false);
    await a.evaluate(()=>servico.atender());
    await Promise.all([a,b].map(p=>p.waitForFunction(()=>servico.ler().estado==='dentro',null,{timeout:20000})));
    await b.evaluate(()=>servico.enviarTexto('mensagem direta autenticada'));
    await a.waitForFunction(()=>textos.some(t=>t.de==='par'&&t.texto==='mensagem direta autenticada'));
    // Fechar o transporte real simula perda do RTC sem invocar sair no serviço.
    await b.evaluate(()=>ultimoPar.abortar());
    await Promise.all([a,b].map(p=>p.waitForFunction(()=>servico.ler().estado==='encerrada',null,{timeout:10000})));
    await b.evaluate(()=>servico.chamar({apelido:'Mesmo link após perda RTC'}));
    await a.waitForFunction(()=>servico.ler().estado==='tocando');
    await a.evaluate(()=>servico.atender());
    await Promise.all([a,b].map(p=>p.waitForFunction(()=>servico.ler().estado==='dentro',null,{timeout:15000})));
    await b.evaluate(()=>servico.enviarTexto('nova chamada no mesmo link'));
    await a.waitForFunction(()=>textos.some(t=>t.texto==='nova chamada no mesmo link'));
    console.log(JSON.stringify({motor,perdaRtcTermina:true,mesmoLinkNovaChamada:true}));
    await a.evaluate(()=>servico.revogarConvite());
    assert.equal(await a.evaluate(()=>servico.ler().estado),'dentro');
    await a.evaluate(()=>servico.enviarTexto('aceita continua depois de revogar'));
    await b.waitForFunction(()=>textos.some(t=>t.de==='par'&&t.texto.includes('continua')));
    // Falha da VPS não derruba mídia/chat já diretos.
    await b.route('**/chamada/1',route=>route.abort());
    await new Promise(r=>setTimeout(r,2500));
    assert.equal(await b.evaluate(()=>servico.ler().estado),'dentro');
    await b.evaluate(()=>servico.enviarTexto('VPS fora, canal direto vivo'));
    await a.waitForFunction(()=>textos.some(t=>t.texto.includes('VPS fora')));
    await b.unroute('**/chamada/1');
    await b.evaluate(()=>servico.encerrar());
    await a.waitForFunction(()=>servico.ler().estado==='encerrada',null,{timeout:10000});
    const link2=await a.evaluate(async()=> (await servico.criarConvite({apelido:'Receptor'})).link);
    await b.evaluate(link=>servico.abrirConvite(link),link2);
    await b.evaluate(()=>servico.chamar({apelido:'Recusada'}));
    await a.waitForFunction(()=>servico.ler().estado==='tocando');
    await a.evaluate(()=>servico.recusar());
    await b.waitForFunction(()=>servico.ler().estado==='encerrada');
    assert.equal(await b.evaluate(()=>servico.ler().motivo),'recusar');
    const link3=await a.evaluate(async()=> (await servico.criarConvite({apelido:'Receptor'})).link);
    await b.evaluate(link=>servico.abrirConvite(link),link3);
    await b.evaluate(async()=>{const tentativa=servico.chamar({apelido:'Cancelada antes do POST'});servico.cancelar();await tentativa.catch(()=>{});});
    assert.equal(await b.evaluate(()=>servico.ler().estado),'encerrada');
    // O POST de atender fez efeito, mas sua resposta ainda não voltou ao dono.
    // Cancelar nesta janela precisa encerrar a chamada já aceita no encontro.
    await b.evaluate(()=>servico.chamar({apelido:'Aceite em trânsito'}));
    await a.waitForFunction(()=>servico.ler().estado==='tocando');
    let liberar, processou;
    const aguardar=new Promise(r=>liberar=r), recebido=new Promise(r=>processou=r);
    await a.route('**/chamada/1',async route=>{
      const req=route.request();let operacao;try{operacao=JSON.parse(req.postData()||'{}').operacao;}catch{}
      if(operacao==='Atender'){const resposta=await route.fetch();processou();await aguardar;try{await route.fulfill({response:resposta});}catch{};}
      else await route.continue();
    });
    await a.evaluate(()=>{window.aceitePendente=servico.atender().catch(()=>false);});
    await recebido;
    await a.evaluate(()=>servico.cancelar());liberar();
    await b.waitForFunction(()=>servico.ler().estado==='encerrada',null,{timeout:10000});
    assert.equal(await a.evaluate(()=>servico.ler().estado),'encerrada');
    await a.unroute('**/chamada/1');
    console.log(JSON.stringify({motor,recusa:true,cancelamentoAntesPost:true,cancelamentoAceiteProcessado:true}));
    for(const p of [a,b])await p.evaluate(()=>servico.destruir());
    console.log(JSON.stringify({motor,convite:true,aceite:true,textoBidirecional:true,revogadaAceitaContinua:true,encerramentoRemoto:true}));
   }finally{await navegador.close();}
  }
 }catch(e){console.error(logs);throw e;}finally{encontro.kill();server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
