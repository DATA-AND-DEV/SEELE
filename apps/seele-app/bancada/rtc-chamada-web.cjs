// Dois RTCPeerConnection reais. Sinalização autenticada é fixture: não prova
// assinatura, serviço remoto, microfone, codecs nem áudio físico.
const assert = require('node:assert/strict');
const fs = require('node:fs'); const path = require('node:path'); const http = require('node:http');
const engines = require('./playwright.cjs');
(async () => {
 const server = http.createServer((req,res) => {
  res.setHeader('Content-Type',req.url==='/rtc-chamada-web.js'?'text/javascript':'text/html');
  res.end(req.url==='/rtc-chamada-web.js'?fs.readFileSync(path.join(__dirname,'../ui/rtc-chamada-web.js')):'<!doctype html><script src="/rtc-chamada-web.js"></script>');
 });
 await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
 try { for(const engine of ['chromium','webkit']) {
  const browser=await engines[engine]().launch({headless:true});
  try {
   const page=await browser.newPage();
   await page.addInitScript(()=>{
    window.pcs=[];window.channels=[];const Native=RTCPeerConnection;
    window.RTCPeerConnection=class extends Native {constructor(config){super(config);pcs.push(this);}createDataChannel(...args){const c=super.createDataChannel(...args);channels.push(c);return c;}};
   });
   await page.goto(`http://127.0.0.1:${server.address().port}`);
   const result=await page.evaluate(async()=>{
    const check=(v,m)=>{if(!v)throw new Error(m);};
    const tick=()=>new Promise(ok=>setTimeout(ok,10));
    const until=async f=>{const start=performance.now();while(!f()){if(performance.now()-start>5000)throw new Error('prazo da bancada');await tick();}};
    const rejected=async p=>{try{await p;return false;}catch{return true;}};
    const received={ctl:[],voz:[],tela:[]};const signals=[];
    let a,b;
    a=criarEnlaceDaChamadaWeb({papel:'quem-liga',enviarSinal:async s=>{signals.push(s);await b.receberSinalAutenticado(s);}});
    b=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:async s=>{signals.push(s);await a.receberSinalAutenticado(s);}});
    b.controle.ouvir(x=>received.ctl.push([...x]));b.tela.ouvir(x=>received.tela.push([...x]));
    await b.iniciar();await a.iniciar();await Promise.all([a.pronto,b.pronto]);
    const descriptions=signals.filter(s=>s.Oferta||s.Resposta);
    check(descriptions.length===2&&descriptions[0].Oferta&&descriptions[1].Resposta,'oferta/resposta autenticadas');
    check(descriptions.every(s=>Object.values(s)[0].impressao.length===32),'fingerprint local');
    check(signals.some(s=>s.Candidato),'candidatos incrementais enviados');
    check(channels.every((c,i)=>c.id===i%4&&c.negotiated&&c.ordered===(i%4!==1)),'IDs e ordenação');
    let perfilRecebido=null;b.perfil.ouvir(bytes=>perfilRecebido=[...bytes]);
    check(a.perfil.enviar(new Uint8Array([1,2,3])),'perfil opcional aberto');await until(()=>perfilRecebido);
    check(perfilRecebido.join(',')==='1,2,3','perfil no mesmo DTLS sem tocar controle');
    check(channels.filter(c=>c.id===1).every(c=>c.maxRetransmits===0),'voz datagrama');
    a.transporteDaVoz.enviar(new Uint8Array([0]));await until(()=>b.contas.vozDescartada===1);
    b.transporteDaVoz.ouvir(x=>received.voz.push([...x]));check(received.voz.length===0,'voz antiga não toca ao ligar receptor');
    for(let n=0;n<16;n++)await a.controle.enviar(new Uint8Array([n,0,255]));
    await a.transporteDaVoz.enviar(new Uint8Array([7,8,9]));
    const quadro=new Uint8Array(16384).fill(37);await a.tela.enviar(quadro);
    await until(()=>received.ctl.length===16&&received.voz.length&&received.tela.length);
    check(received.ctl.every((x,i)=>x[0]===i&&x[2]===255),'controle binário ordenado');
    check(received.voz[0].join(',')==='7,8,9','voz bytes');
    check(received.tela[0].length===16384&&received.tela[0].every(x=>x===37),'tela bytes');
    // Contrapressão determinística sobre canais reais; a fila não cresce sem teto.
    const screen=channels[2];let buffered=65536;
    Object.defineProperty(screen,'bufferedAmount',{get:()=>buffered,configurable:true});
    let delivered=false;const pending=a.tela.enviar(new Uint8Array([42])).then(()=>delivered=true);
    await tick();check(!delivered,'tela deve esperar');
    await a.controle.enviar(new Uint8Array([99]));await until(()=>received.ctl.length===17);
    check(!delivered,'controle não espera a fila da tela');
    const voice=channels[1];Object.defineProperty(voice,'bufferedAmount',{get:()=>65536,configurable:true});
    check(await a.transporteDaVoz.enviar(new Uint8Array([1]))===false,'voz descarta sob pressão');
    delete voice.bufferedAmount;
    buffered=0;screen.dispatchEvent(new Event('bufferedamountlow'));await pending;
    check(a.contas.esperasPorPressao===1&&a.contas.vozDescartada===1,'contas de pressão');
    await until(()=>received.tela.length===2);
    const beforeFrame=received.tela.length;
    const largeFrame=Uint8Array.from({length:250*1024},(_,i)=>i%251);
    const writer=a.tela.criarEscrita().getWriter();await writer.write(largeFrame);await writer.close();
    await until(()=>received.tela.slice(beforeFrame).reduce((n,x)=>n+x.length,0)>=largeFrame.length);
    const reconstructed=received.tela.slice(beforeFrame).flat();
    check(reconstructed.length===largeFrame.length&&reconstructed.every((x,i)=>x===largeFrame[i]),'quadro250KiB íntegro e ordenado');

    // Injeta somente a notificação de perda; offer/answer, ICE restart, DTLS
    // e SCTP abaixo são dos motores reais. Não simula travessia de outro NAT.
    const caller=pcs[0];
    Object.defineProperty(caller,'connectionState',{get:()=> 'disconnected',configurable:true});
    Object.defineProperty(caller,'iceConnectionState',{get:()=> 'disconnected',configurable:true});
    caller.dispatchEvent(new Event('connectionstatechange'));
    check(a.estado==='recuperando','queda não encerra imediatamente');
    check(a.transporteDaVoz.enviar(new Uint8Array([8]))===false,'voz não enfileira na recuperação');
    const inicioRestart=performance.now();
    while(signals.filter(s=>s.Oferta||s.Resposta).length<4||caller.signalingState!=='stable'){
      if(performance.now()-inicioRestart>15000)throw new Error('restart real não completou SDP');
      await tick();
    }
    delete caller.connectionState;delete caller.iceConnectionState;
    caller.dispatchEvent(new Event('connectionstatechange'));
    await until(()=>a.estado==='aberta'&&b.estado==='aberta');
    const descriptionsAfterRestart=signals.filter(s=>s.Oferta||s.Resposta);
    check(descriptionsAfterRestart[0].Oferta.sdp.match(/a=ice-ufrag:([^\r\n]+)/)[1]!==descriptionsAfterRestart[2].Oferta.sdp.match(/a=ice-ufrag:([^\r\n]+)/)[1],'restart troca geração ICE');
    const beforeRestartText=received.ctl.length;
    await a.controle.enviar(new Uint8Array([77]));await until(()=>received.ctl.length===beforeRestartText+1);
    check(a.diagnostico.reinicios===1&&b.diagnostico.reinicios===1,'restart coordenado dos dois lados');

    check(await rejected(a.controle.enviar(new Uint8Array(16385))),'limite de mensagem');
    check(a.transporteDaVoz.enviar(new Uint8Array(1101))===false,'voz devolve boolean síncrono e limita tamanho');
    buffered=65536;const blocked=a.tela.enviar(new Uint8Array([4]));await tick();a.abortar();a.abortar();
    check(await rejected(blocked),'abort solta envio bloqueado');b.abortar();
    check(pcs.slice(0,2).every(pc=>pc.connectionState==='closed'),'peer connections fechados');
    check(await rejected(a.iniciar())&&a.estado==='encerrada','iniciar depois de abort não reabre');
    // Recusa de SDP que contradiz a impressão autenticada.
    const bad=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:async()=>{throw new Error('não deve responder');}});
    const forged=structuredClone(descriptions[0]);forged.Oferta.impressao[0]^=1;
    check(await rejected(bad.receberSinalAutenticado(forged)),'impressão divergente recusada');
    check(bad.estado==='encerrada','recusa fecha');
    const noIce=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:async()=>{throw new Error('não deve responder');}});
    const missing=structuredClone(descriptions[0]);missing.Oferta.sdp=missing.Oferta.sdp.replace(/^a=ice-pwd:.*\r?\n/gm,'');
    check(await rejected(noIce.receberSinalAutenticado(missing))&&noIce.estado==='encerrada','SDP sem credenciais ICE recusado');

    // Callback de sinalização atrasado não ressuscita a chamada cancelada.
    let release,entered=false;
    const controller=new AbortController();
    const late=criarEnlaceDaChamadaWeb({papel:'quem-liga',sinal:controller.signal,enviarSinal:()=>{entered=true;return new Promise(ok=>release=ok);}});
    const starting=late.iniciar();await until(()=>entered);controller.abort();
    check(await rejected(starting),'abort solta sinalização');release();await tick();
    check(late.estado==='encerrada'&&pcs.at(-1).connectionState==='closed','callback tardio não ativa');
    let turnRefused=false;try{criarEnlaceDaChamadaWeb({papel:'quem-liga',enviarSinal:async()=>{},iceServers:[{urls:'turn:fixture.invalid'}]});}catch{turnRefused=true;}
    check(turnRefused,'TURN recusado');
    // Sem consumidor/conexão, a fila confiável continua limitada a 1 MiB.
    const queued=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:async()=>{}});
    const queuedWrites=Array.from({length:64},()=>rejected(queued.tela.enviar(new Uint8Array(16384))));
    check(await rejected(queued.tela.enviar(new Uint8Array([1]))),'teto de fila');
    queued.abortar();check((await Promise.all(queuedWrites)).every(Boolean),'abort solta toda a fila');
    // A sinalização continua limitada, com prazo próprio antes do ICE.
    const timed=criarEnlaceDaChamadaWeb({papel:'quem-recebe',enviarSinal:async()=>{}});
    const began=performance.now();await timed.iniciar();
    check(await rejected(timed.pronto),'tentativa sem resposta vence');
    const deadlineMs=performance.now()-began;
    check(deadlineMs>=29500&&deadlineMs<33000&&timed.estado==='encerrada','prazo de sinalização de trinta segundos');
    check(timed.diagnostico.motivo==='prazo-sinalizacao','diagnóstico sem SDP nem endereços');

    return {controle:received.ctl.length,voz:received.voz.length,tela:received.tela.length,signals:signals.length,deadlineMs,peersFechados:pcs.every(pc=>pc.connectionState==='closed')};
   });
   assert.equal(result.peersFechados,true);console.log(JSON.stringify({engine,version:browser.version(),...result}));
  }finally{await browser.close();}
 }}finally{server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
