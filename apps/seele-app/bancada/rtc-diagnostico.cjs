const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../ui/rtc-web.js'), 'utf8');
function contexto(PC) {
  const ctx = vm.createContext({ RTCPeerConnection: PC, console, setTimeout, clearTimeout, setInterval, clearInterval,
    performance, crypto: require('node:crypto').webcrypto, Uint8Array, ArrayBuffer, TextEncoder, TextDecoder,
    ReadableStream, WritableStream, TransformStream, ByteLengthQueuingStrategy, AbortController });
  vm.runInContext(source, ctx);
  return ctx;
}
function dados() {
  return new Map([
    ['local-secret', {type:'local-candidate', candidateType:'host', address:'2001:db8::42', port:54321, url:'stun:secret.example'}],
    ['remote-secret', {type:'remote-candidate', candidateType:'srflx', address:'203.0.113.7', port:8383}],
    ['pair-secret', {type:'candidate-pair', localCandidateId:'local-secret', remoteCandidateId:'remote-secret', state:'in-progress', nominated:false, requestsSent:7, responsesReceived:0, requestsReceived:2, responsesSent:2}],
  ]);
}
test('diagnóstico exporta apenas famílias, tipos e contadores; ausência não vira zero', () => {
  const d = contexto().resumirCaminhoRtc(dados());
  assert.equal(d.locais.host_ipv6,1); assert.equal(d.remotos.srflx_ipv4,1);
  assert.equal(d.pares[0].pedidosEnviados,7); assert.equal(d.pares[0].respostasRecebidas,0);
  assert.equal(d.pares[0].bytesEnviados,null);
  assert.doesNotMatch(JSON.stringify(d), /secret|2001:|203\.0|54321|8383|stun:|CandidateId/);
});
test('mDNS e campos desconhecidos não são tratados como IPv6 e exportação é limitada', () => {
  const stats=dados(); stats.set('oculto',{type:'local-candidate',candidateType:'host',address:'private-name.local'});
  stats.set('ruim',{type:'local-candidate',candidateType:'segredo',address:'hostname'});
  for(let i=0;i<40;i++) stats.set('extra'+i,{type:'candidate-pair',state:'segredo', requestsSent:NaN, responsesSent:-1});
  const d=contexto().resumirCaminhoRtc(stats);
  assert.equal(d.locais.host_oculta,1); assert.equal(d.locais.desconhecido_oculta,1);
  assert.equal(d.pares.length,32); assert.equal(d.pares[1].pedidosEnviados,null);
  assert.doesNotMatch(JSON.stringify(d),/private-name|hostname|segredo|NaN/);
});
class Canal extends EventTarget { constructor(){super();this.readyState='connecting';this.bufferedAmount=0;} }
class PC extends EventTarget {
  constructor(){super();this.connectionState='connecting';this.iceConnectionState='checking';this.iceGatheringState='gathering';this.sctp={transport:{state:'new'}};}
  createDataChannel(){return new Canal();}
  createOffer(){return new Promise(()=>{});}
  getStats(){return Promise.resolve(dados());}
  close(){this.connectionState=this.iceConnectionState='closed';}
}
test('falha guarda estado anterior ao close sem alterar prazo ou transporte', async () => {
  const c=contexto(PC);
  const e=c.criarEnlaceRtc({alvos:['203.0.113.7:8383'],impressao:'ab'.repeat(32),certificado:{certificado:{},impressao:'ab'.repeat(32)},prazo:20});
  await assert.rejects(e.ready);
  const antes=e.visto.antesDeFechar;
  assert.equal(antes.ice,'checking'); assert.equal(antes.conexao,'connecting');
  const d=await e.medirCaminho(); assert.equal(d.estado.ice,'checking');
  assert.equal(d.pares[0].pedidosEnviados,7);
  e.close(); assert.equal(e.visto.antesDeFechar.ice,'checking');
});
function pagina() {
 const els = new Map();
 const el = id => {if(!els.has(id)) els.set(id,{value:'',textContent:'',hidden:false,disabled:false,handlers:{},addEventListener(t,f){this.handlers[t]=f;}});return els.get(id);};
 const pendentes=[];let medicoes=0;
 const enlace = {visto:{refletido:true,sinal:{aqui:{}},candidatos:[1],antesDeFechar:{ice:'failed',conexao:'failed'}},contas:{candidatosRecusados:0},close(){},medirCaminho:async()=>{medicoes++;return {pares:[{pedidosEnviados:3,respostasRecebidas:0}],estado:{ice:'checking'}};}};
 const ctx=vm.createContext({console,URL,URLSearchParams,setTimeout,clearTimeout,performance,
  document:{getElementById:el},location:{hash:'',pathname:'/diagnostico-rede',search:''},history:{replaceState(){}},
  fetch:async()=>({arrayBuffer:async()=>new ArrayBuffer(0)}),WebAssembly:{instantiate:async()=>({instance:{exports:{}}})},
  prepararCertificadoDtls:async()=>({}),fronteiraDoSinal:()=>({}),window:{addEventListener(){}},navigator:{clipboard:{writeText:async()=>{}}},
  criarEnlaceRtc:()=>({...enlace,ready:new Promise((resolve,reject)=>pendentes.push({resolve,reject}))})});
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../ui/diagnostico-rede.js'),'utf8'),ctx);
 el('convite').value='https://seele.sh/#192.0.2.1:8383?enc=rendzvous.seele.sh:8384/203.0.113.1:8383&fp='+'ab'.repeat(32);
 return {el,pendentes,medicoes:()=>medicoes};
}
test('página exporta medição antes da falha e interrompe coleta ao terminar',async()=>{
 const p=pagina();const tarefa=p.el('testar').handlers.click();
 await new Promise(r=>setTimeout(r,5));
 p.pendentes[0].reject({FalhaDoEnlace:{motivo:'CaminhoNaoAbriu',detalhe:{ice:'closed',conexao:'closed'}}}); await tarefa;
 const rel=JSON.parse(p.el('resultado').textContent);
 assert.equal(rel.teste,'servidor-hospedado-3');assert.equal(rel.caminho.pares[0].pedidosEnviados,3);assert.equal(rel.antesDeFechar.ice,'failed');
 const n=p.medicoes();await new Promise(r=>setTimeout(r,320));assert.equal(p.medicoes(),n);
});
test('cancelar e testar novamente não mistura resultados nem cancela a coleta nova',async()=>{
 const p=pagina();const antigo=p.el('testar').handlers.click();await new Promise(r=>setTimeout(r,5));
 p.el('parar').handlers.click();const novo=p.el('testar').handlers.click();await new Promise(r=>setTimeout(r,5));
 p.pendentes[0].reject(new Error('antiga'));await antigo;assert.equal(p.el('resultado').textContent,'');
 p.pendentes[1].resolve();await novo;const rel=JSON.parse(p.el('resultado').textContent);assert.equal(rel.conectou,true);
 const n=p.medicoes();await new Promise(r=>setTimeout(r,320));assert.equal(p.medicoes(),n);
});
