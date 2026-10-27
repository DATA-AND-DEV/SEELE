// H264 de canvas artificial, WebRTC real e LeitorDeTela do WASM do produto.
// Nunca chama o seletor real de desktop. Safari sem TrackProcessor recebe a
// amostra H264 produzida pelo Chromium; não se declara captura suportada.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const engines=require('./playwright.cjs');
const wasmPath=process.argv[2];if(!wasmPath)throw new Error('uso: compartilhar-pessoal-web.cjs caminho/seele_web.wasm');
const ui=path.join(__dirname,'../ui');
const originalSps=fs.readFileSync(path.join(ui,'palco-imagem.js'),'utf8');
const at=originalSps.indexOf('function codecDoSps(');const sps=originalSps.slice(at,originalSps.indexOf('\n}\n',at)+3);
(async()=>{
 const server=http.createServer((req,res)=>{
  if(req.url==='/codec-sps.js'){res.setHeader('Content-Type','text/javascript');return res.end(sps);}
  if(req.url==='/seele_web.wasm'){res.setHeader('Content-Type','application/wasm');return res.end(fs.readFileSync(wasmPath));}
  const files=['rtc-chamada-web.js','compartilhar-web.js','compartilhar-pessoal-web.js'];
  if(files.includes(req.url.slice(1))){res.setHeader('Content-Type','text/javascript');return res.end(fs.readFileSync(path.join(ui,req.url.slice(1))));}
  res.setHeader('Content-Type','text/html');res.end('<!doctype html><div id="a"></div><div id="b"></div>'+['codec-sps.js',...files].map(f=>`<script src="/${f}"></script>`).join(''));
 });await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 let sample=null;
 try{for(const engine of ['chromium','webkit']){
  const browser=await engines[engine]().launch({headless:true});
  try{
   const page=await browser.newPage({viewport:{width:1000,height:700}});
   if(process.env.SEELE_PREVIEW_DIR){fs.mkdirSync(process.env.SEELE_PREVIEW_DIR,{recursive:true});await page.exposeFunction('preview',()=>page.screenshot({path:path.join(process.env.SEELE_PREVIEW_DIR,`${engine}-tela-pessoal-h264.png`)}));}
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   const result=await page.evaluate(async sample=>{
    const check=(v,m)=>{if(!v)throw new Error(m);};const tick=()=>new Promise(ok=>setTimeout(ok,20));
    const until=async f=>{const start=performance.now();while(!f()){if(performance.now()-start>10000)throw new Error('prazo mídia: '+JSON.stringify(window.events));await tick();}};
    const wasmBytes=await(await fetch('/seele_web.wasm')).arrayBuffer();
    async function service(){const wasm=(await WebAssembly.instantiate(wasmBytes,{})).instance.exports;return {wasm,operar(input){const b=new TextEncoder().encode(JSON.stringify(input));const p=wasm.alocar(b.length);try{new Uint8Array(wasm.memory.buffer).set(b,p);wasm.chamada_operar(p,b.length);const r=JSON.parse(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer,wasm.saida_ptr(),wasm.saida_len())));if(r.erro)throw Error(r.erro);return r;}finally{wasm.liberar(p,b.length);}}};}
    const sa=await service(),sb=await service();let pa,pb;
    pa=criarEnlaceDaChamadaWeb({papel:'quem-liga',enviarSinal:s=>pb.receberSinalAutenticado(s)});
    pb=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:s=>pa.receberSinalAutenticado(s)});
    await pb.iniciar();await pa.iniciar();await Promise.all([pa.pronto,pb.pronto]);
    window.events=[];const packets=[];let a,b;
    const transport={ouvir:fn=>pa.tela.ouvir(fn),enviar:bytes=>{packets.push([...bytes]);return pa.tela.enviar(bytes);}};
    a=criarCompartilhamentoPessoalWeb({servico:sa,transporte:transport,palco:document.getElementById('a'),enviarControle:c=>b.receberControle(c),aoEstado:(tipo,detalhe)=>events.push({de:'a',tipo,detalhe})});
    b=criarCompartilhamentoPessoalWeb({servico:sb,transporte:pb.tela,palco:document.getElementById('b'),enviarControle:c=>a.receberControle(c),aoEstado:(tipo,detalhe)=>events.push({de:'b',tipo,detalhe})});
    const frames=()=>events.filter(e=>e.de==='b'&&e.tipo==='quadro').length;
    function artificial(color){const c=document.createElement('canvas');c.width=320;c.height=180;const ctx=c.getContext('2d');let n=0;const paint=()=>{ctx.fillStyle=color;ctx.fillRect(0,0,320,180);ctx.fillStyle='white';ctx.fillRect(100+(n++%20),60,20,20);};paint();const stream=c.captureStream(20);const timer=setInterval(paint,50);return{stream,stop:()=>{clearInterval(timer);stream.getTracks().forEach(t=>t.stop());}};}
    const pixel=()=>[...document.querySelector('#b canvas').getContext('2d').getImageData(10,10,1,1).data];
    let captured=false,oldPackets,newPackets;
    if(typeof MediaStreamTrackProcessor==='function'&&typeof VideoEncoder==='function'){
     captured=true;const red=artificial('#e02020');a.usarMidia(red.stream);await until(()=>frames()>1);
     check(pixel()[0]>150&&pixel()[2]<80,'H264 vermelho real');a.parar();await until(()=>document.getElementById('b').hidden);red.stop();
     check(red.stream.getTracks().every(t=>t.readyState==='ended')&&!a.noAr,'parar solta captura');oldPackets=packets.splice(0);
     const blue=artificial('#2040e0');const before=frames();a.usarMidia(blue.stream);await until(()=>frames()>before+1);
     check(pixel()[2]>150&&pixel()[0]<80,'reinício H264 azul real');
     b.receberControle('TelaParou');check(!document.getElementById('b').hidden,'controle atrasado não fecha fluxo novo');
     for(const bytes of oldPackets)await pa.tela.enviar(new Uint8Array(bytes));await tick();
     check(!document.getElementById('b').hidden&&pixel()[2]>150,'geração velha não apaga nem pinta nova');
     if(window.preview)await window.preview();newPackets=packets.splice(0);
     a.parar();await until(()=>document.getElementById('b').hidden);blue.stop();
     // Resolver o seletor após fechar não deixa captura órfã.
     let deliver;Object.defineProperty(navigator.mediaDevices,'getDisplayMedia',{configurable:true,value:()=>new Promise(ok=>deliver=ok)});
     const pending=a.compartilhar();a.fechar();await pending;const late=artificial('#ffffff');deliver(late.stream);await tick();
     check(late.stream.getTracks().every(t=>t.readyState==='ended'),'captura tardia fechada');late.stop();
     // Relógio acelerado só para o seletor: prazo recusa e faixas tardias param.
     const holder=document.createElement('div');document.body.append(holder);
     const timed=criarCompartilhamentoPessoalWeb({servico:sa,transporte:pa.tela,palco:holder,enviarControle:()=>{}});
     const nativeTimeout=window.setTimeout;window.setTimeout=(fn,ms,...args)=>nativeTimeout(fn,ms===30000?20:ms,...args);
     let expired=false;try{await timed.compartilhar();}catch{expired=true;}finally{window.setTimeout=nativeTimeout;}
     check(expired,'prazo de captura recusa');const tooLate=artificial('#ffffff');deliver(tooLate.stream);await tick();
     check(tooLate.stream.getTracks().every(t=>t.readyState==='ended'),'captura após prazo fechada');tooLate.stop();timed.fechar();holder.remove();

    }else{
     check(sample,'amostra H264 necessária para receptor');oldPackets=sample.oldPackets;newPackets=sample.newPackets;
     const live=oldPackets.filter(bytes=>!sa.operar({acao:'tela_desempacotar',bytes}).resposta.Fechar);
     for(const bytes of live)await pa.tela.enviar(new Uint8Array(bytes));await until(()=>frames()>0);
     check(pixel()[0]>150,'WebKit decodifica H264 real vermelho');const before=frames();
     for(const bytes of newPackets)await pa.tela.enviar(new Uint8Array(bytes));await until(()=>frames()>before);
     check(pixel()[2]>150,'WebKit reinício azul');for(const bytes of oldPackets)await pa.tela.enviar(new Uint8Array(bytes));await tick();
     check(!document.getElementById('b').hidden&&pixel()[2]>150,'WebKit geração velha ignorada');if(window.preview)await window.preview();a.fechar();
    }
    const errors=events.filter(e=>e.tipo==='erro');check(errors.length===0,'erros: '+JSON.stringify(errors));
    const decoded=frames();b.fechar();b.fechar();pa.abortar();pb.abortar();
    check(!document.querySelector('#a canvas')&&!document.querySelector('#b canvas'),'canvas removidos');
    return {captured,decoded,errors,oldPackets,newPackets};
   },sample);
   if(result.captured)sample={oldPackets:result.oldPackets,newPackets:result.newPackets};
   console.log(JSON.stringify({engine,version:browser.version(),captura:result.captured?'canvas real':'indisponível: receptor com amostra real',quadrosDecodificados:result.decoded,erros:result.errors}));
   assert.ok(result.decoded>0);
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
