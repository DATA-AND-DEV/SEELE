const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { origemDoProduto } = require('./origem-do-produto.cjs');
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
function pagina({search='',copiar=async()=>{}}={}) {
 const els = new Map();
 const el = id => {if(!els.has(id)) els.set(id,{value:'',textContent:'',hidden:false,disabled:false,handlers:{},filhos:[],dataset:{},atributos:{},addEventListener(t,f){this.handlers[t]=f;},replaceChildren(...n){this.filhos=n;},setAttribute(n,v){this.atributos[n]=v;},removeAttribute(n){delete this.atributos[n];},scrollIntoView(){this.rolou=true;}});return els.get(id);};
 // `dataset` e os atributos: o convite que não serve marca o campo, e a cópia que falha marca a frase dela.
 const selecionados=[];
 // `replaceChildren` e `createElement`: a lista de etapas que a página da 0.19 desenha (`49758837`).
 const pendentes=[];let medicoes=0;
 const enlace = {visto:{refletido:true,sinal:{aqui:{}},candidatos:[1],antesDeFechar:{ice:'failed',conexao:'failed'}},contas:{candidatosRecusados:0},close(){},medirCaminho:async()=>{medicoes++;return {pares:[{pedidosEnviados:3,respostasRecebidas:0}],estado:{ice:'checking'}};}};
 const ctx=vm.createContext({console,URL,URLSearchParams,setTimeout,clearTimeout,performance,
  document:{getElementById:el,createElement:(tag)=>({tag,dataset:{},textContent:'',filhos:[],setAttribute(){},append(...n){this.filhos.push(...n);}}),createRange:()=>({selectNodeContents(n){this.no=n;}})},location:{hash:'',pathname:'/diagnostico-rede',search},history:{replaceState(){}},
  getSelection:()=>({removeAllRanges(){selecionados.length=0;},addRange(f){selecionados.push(f.no);}}),
  fetch:async()=>({arrayBuffer:async()=>new ArrayBuffer(0)}),WebAssembly:{instantiate:async()=>({instance:{exports:{}}})},
  prepararCertificadoDtls:async()=>({}),fronteiraDoSinal:()=>({}),window:{addEventListener(){}},navigator:{clipboard:{writeText:copiar}},
  criarEnlaceRtc:()=>({...enlace,ready:new Promise((resolve,reject)=>pendentes.push({resolve,reject}))})});
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../ui/diagnostico-rede.js'),'utf8'),ctx);
 el('convite').value=origemDoProduto+'/#192.0.2.1:8383?enc=rendzvous.seele.sh:8384/203.0.113.1:8383&fp='+'ab'.repeat(32);
 return {el,pendentes,medicoes:()=>medicoes,selecionados,ctx};
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
test('diagnóstico IPv6 não reintroduz IPv4 pela resposta do encontro',()=>{
 const source=fs.readFileSync(path.join(__dirname,'../ui/diagnostico-rede.js'),'utf8').split('(() => {')[0];
 const ctx=contexto();vm.runInContext(source,ctx);
 let pedido;
 const sinal={montar:p=>{pedido=p;return {corpo:'ok'};},ler:()=>({aqui:{endereco:'203.0.113.7:8383',dicas:['192.168.0.2:8383','[2804:14c::1]:8383']}})};
 const filtrado=ctx.sinalDaFamilia(sinal,6);
 filtrado.montar({credencial:'nao-exportar',candidatos:['203.0.113.8:5000','[2804:14c::2]:5001']});
 assert.deepEqual(Array.from(pedido.candidatos),['[2804:14c::2]:5001']);
 assert.equal(pedido.credencial,'nao-exportar');assert.equal(filtrado.enviados.size,1);
 const r=filtrado.ler('resposta');assert.equal(r.aqui.endereco,'[2804:14c::1]:8383');assert.equal(r.aqui.dicas.length,0);
});
test('diagnóstico por família não inventa candidato nem envia POST sem candidato',()=>{
 const source=fs.readFileSync(path.join(__dirname,'../ui/diagnostico-rede.js'),'utf8').split('(() => {')[0];
 const ctx=contexto();vm.runInContext(source,ctx);
 let chamadas=0;const sinal={montar:()=>{chamadas++;return {corpo:'ok'};},ler:()=>({aqui:{endereco:'[2804:14c::1]:8383',dicas:[]}})};
 const f=ctx.sinalDaFamilia(sinal,4);
 assert.equal(f.montar({candidatos:['[2804:14c::2]:5001','nome.local:5000']}).erro,'SemCandidatoIPv4');assert.equal(chamadas,0);
 assert.equal(f.ler('').semFamilia,true);
 assert.equal(f.enviados.size,0);
});

// ---- o TESTAR A CONEXÃO e o «copiar diagnóstico» (lacuna 8 da análise para a 1.0) ----
// O que a página anota de quem chegou por uma tela de erro, o que o botão copia, e o que ela diz.
const fora=()=>{const ctx=vm.createContext({URL,URLSearchParams});vm.runInContext(fs.readFileSync(path.join(__dirname,'../ui/diagnostico-rede.js'),'utf8').split('(() => {')[0],ctx);return ctx;};
// Um relatório como a página o monta, com o que o navegador mediu num caminho que não abriu.
const medido=()=>({teste:'servidor-hospedado-3',horario:'2026-11-12T12:00:00.000Z',etapas:[{etapa:'Discando',ms:12},{etapa:'ChamandoPeloPonto',ms:340}],conectou:false,motivo:'CaminhoNaoAbriu',
 ice:'failed',conexao:'failed',refletido:true,sinal:'aqui',dtls:null,anuncios:2,enderecosEnviados:3,enderecosDoServidor:2,antesDeFechar:{ice:'failed',conexao:'failed'},candidatosRecusados:0,duracaoMs:15012,
 caminho:{ms:14000,estado:{ice:'failed',conexao:'failed'},locais:{host_oculta:1,srflx_ipv4:1},remotos:{srflx_ipv4:1,host_ipv6:1},
  pares:[{local:'srflx_ipv4',remoto:'srflx_ipv4',estado:'failed',nomeado:false,pedidosEnviados:7,respostasRecebidas:0,pedidosRecebidos:null,respostasEnviadas:null,bytesEnviados:null,bytesRecebidos:null},
         {local:'host_oculta',remoto:'host_ipv6',estado:'in-progress',nomeado:false,pedidosEnviados:21,respostasRecebidas:null,pedidosRecebidos:null,respostasEnviadas:null,bytesEnviados:null,bytesRecebidos:null}]}});
test('o texto copiado traz o resultado da medida e a falha vista, em poucas linhas',()=>{
 const texto=fora().resumoDoDiagnostico(medido(),'NatNaoAbriu');
 const linhas=texto.split('\n');
 assert.ok(linhas.length<=14,`o resumo tem ${linhas.length} linhas, e era para caber numa mensagem`);
 for(const parte of ['falha vista no SEELE: NatNaoAbriu','resultado: não conectou (CaminhoNaoAbriu) em 15012 ms','Discando 12 ms > ChamandoPeloPonto 340 ms',
  'ponto de encontro: aqui · chamadas 2 · endereço refletido sim','endereços: enviados 3 · do servidor 2 · recusados 0','estado: ice failed · conexão failed',
  'candidatos deste lado: host_oculta 1, srflx_ipv4 1','candidatos do servidor: srflx_ipv4 1, host_ipv6 1','pares: 2 (failed 1, in-progress 1) · pedidos enviados 28 · respostas recebidas 0','2026-11-12T12:00:00.000Z'])
  assert.ok(texto.includes(parte),`falta «${parte}» em:\n${texto}`);
 // Com conexão, o resultado diz que conectou, e sem falha vista diz que não foi informada.
 const ok=fora().resumoDoDiagnostico({...medido(),conectou:true,motivo:undefined,duracaoMs:842},null);
 assert.ok(ok.includes('resultado: conectou em 842 ms')&&ok.includes('falha vista no SEELE: não informada'),ok);
});
test('o texto copiado não leva endereço, porta, convite, impressão nem nome; e ausência não vira zero',()=>{
 // O relatório com tudo o que NÃO pode sair, posto onde um descuido o leria: campos a mais que o resumo não conhece.
 const sujo={...medido(),convite:'https://casa.exemplo/#203.0.113.7:8383?fp='+'ab'.repeat(32)+'&convite=7K4MSEGREDO',alvos:['203.0.113.7:8383','[2001:db8::42]:8383'],impressao:'ab'.repeat(32),apelido:'ana',servidor:'Casa do Lago'};
 sujo.caminho.pares[0].endereco='203.0.113.7';sujo.caminho.pares[0].porta=54321;
 const texto=fora().resumoDoDiagnostico(sujo,'SemResposta');
 assert.doesNotMatch(texto,/203\.0|2001:|8383|54321|7K4M|abab|casa\.exemplo|Casa do Lago|\bana\b|https?:|seele:\/\//);
 // Só o respondido de quem foi medido: o segundo par não deu respostas (null), e a soma é a do primeiro, e não «não medido» nem um zero inventado.
 assert.ok(texto.includes('respostas recebidas 0'),texto);
 const semMedida=medido();for(const par of semMedida.caminho.pares){par.pedidosEnviados=null;par.respostasRecebidas=null;}
 semMedida.dtls=null;semMedida.anuncios=undefined;delete semMedida.refletido;
 const t2=fora().resumoDoDiagnostico(semMedida,null);
 assert.ok(t2.includes('pedidos enviados não medido · respostas recebidas não medido'),t2);
 assert.ok(t2.includes('chamadas não medido · endereço refletido não medido'),t2);
 assert.ok(t2.includes('dtls não medido'),t2);
 const semCaminho=medido();delete semCaminho.caminho;
 assert.ok(fora().resumoDoDiagnostico(semCaminho,null).includes('caminho: não medido'));
 const indisponivel=medido();indisponivel.caminho={estado:{ice:'checking'},indisponivel:true};
 assert.ok(fora().resumoDoDiagnostico(indisponivel,null).includes('caminho: não medido'));
});
test('a falha vista só entra se for das que oferecem o teste: lista fechada, igual à de frases.js',()=>{
 const c=fora();
 assert.equal(c.erroVistoNaConsulta('?erro=NatNaoAbriu'),'NatNaoAbriu');
 assert.equal(c.erroVistoNaConsulta('?familia=4&erro=SinalMudo'),'SinalMudo');
 for(const ruim of ['','?erro=','?erro=Banned','?erro=203.0.113.7','?erro=ana','?erro=NatNaoAbriu%20e%20mais','?erro=<b>x</b>','?outra=NatNaoAbriu'])
  assert.equal(c.erroVistoNaConsulta(ruim),null,`«${ruim}» entrou como falha vista`);
 const frases=fs.readFileSync(path.join(__dirname,'../ui/frases.js'),'utf8');
 const bloco=frases.split('const FALHAS_QUE_OFERECEM_O_TESTE_DE_CONEXAO = Object.freeze([')[1]?.split(']')[0];
 assert.ok(bloco,'a lista das falhas que oferecem o teste saiu de frases.js');
 const daTela=[...bloco.matchAll(/"([A-Za-z0-9]+)"/g)].map(m=>m[1]);
 assert.ok(daTela.length>=5,`só ${daTela.length} falhas lidas de frases.js`);
 assert.deepEqual([...vm.runInContext('ERROS_QUE_A_PAGINA_ANOTA',c)].sort(),daTela.sort());
});
test('o convite do app (seele://) e o link do navegador dão o mesmo corpo; o que não é convite é dito',()=>{
 const c=fora();const corpo='192.0.2.1:8383?enc=rendzvous.seele.sh:8384/203.0.113.1:8383&fp='+'ab'.repeat(32);
 assert.equal(c.corpoDoConvite('  seele://'+corpo+' '),corpo);
 assert.equal(c.corpoDoConvite(origemDoProduto+'/#'+corpo),corpo);
 for(const ruim of ['','   ','um texto qualquer','http://exemplo.br/#'+corpo,'javascript:alert(1)']) assert.throws(()=>c.corpoDoConvite(ruim),/convite/i,`«${ruim}» passou`);
});
test('um convite que não serve é dito, e nada é testado nem oferecido para copiar',async()=>{
 for(const [colado,frase] of [['',/Cole o convite/],['seele://192.0.2.1:8383',/Convite incompleto/],['seele://192.0.2.1:8383?fp='+'ab'.repeat(32),/impressão do servidor ou o ponto de encontro/],
  ['seele://192.0.2.1:8383?enc=outro.exemplo:8384/203.0.113.1:8383&fp='+'ab'.repeat(32),/outro ponto de encontro/]]){
  const p=pagina();p.el('convite').value=colado;p.el('copiar').disabled=true;
  await p.el('testar').handlers.click();
  assert.match(p.el('estado').textContent,frase,`«${colado}»: ${p.el('estado').textContent}`);
  assert.match(p.el('estado').textContent,/Nada foi testado\.$/);
  assert.equal(p.pendentes.length,0,'o teste começou com um convite que não serve');
  assert.equal(p.el('resultado').textContent,'');assert.equal(p.el('resumo').textContent,'');
  assert.equal(p.el('copiar').disabled,true);assert.equal(p.el('testar').disabled,false);
  assert.equal(p.el('convite').atributos['aria-invalid'],'true');
 }
});
async function medirNaPagina(opcoes){
 const p=pagina(opcoes);const tarefa=p.el('testar').handlers.click();await new Promise(r=>setTimeout(r,5));
 p.pendentes[0].reject({FalhaDoEnlace:{motivo:'CaminhoNaoAbriu',detalhe:{ice:'closed',conexao:'closed'}}});await tarefa;return p;
}
test('copiar manda o resumo que está na tela, com a falha vista, e a página diz o que copiou',async()=>{
 const copiados=[];const p=await medirNaPagina({search:'?erro=NatNaoAbriu',copiar:async(t)=>{copiados.push(t);}});
 assert.match(p.el('visto').textContent,/NatNaoAbriu/);assert.equal(p.el('visto').hidden,false);
 const naTela=p.el('resumo').textContent;
 assert.ok(naTela.includes('falha vista no SEELE: NatNaoAbriu')&&naTela.includes('resultado: não conectou (CaminhoNaoAbriu)'),naTela);
 assert.equal(JSON.parse(p.el('resultado').textContent).erroVisto,'NatNaoAbriu');
 await p.el('copiar').handlers.click();
 assert.deepEqual(copiados,[naTela],'o que foi copiado não é o que a tela mostra');
 // O convite da bancada tem endereço, porta e impressão: nada disso sai.
 assert.doesNotMatch(copiados[0],/192\.0\.2|203\.0\.113|8383|8384|abab|rendzvous/);
 assert.match(p.el('copia').textContent,/^Copiado: o texto do quadro acima, inteiro e só ele\./);
 assert.equal(p.el('copia').dataset.tipo,undefined);
});
test('a cópia que o navegador recusa é dita, e o texto fica selecionado',async()=>{
 const p=await medirNaPagina({copiar:async()=>{throw new Error('NotAllowedError');}});
 await p.el('copiar').handlers.click();
 assert.match(p.el('copia').textContent,/^Não foi copiado/);
 assert.equal(p.el('copia').dataset.tipo,'erro');
 assert.deepEqual(p.selecionados,[p.el('resumo')],'o texto não ficou selecionado para a cópia à mão');
 // Sem falha na consulta, a página não inventa uma.
 assert.equal(p.el('visto').textContent,'');assert.ok(p.el('resumo').textContent.includes('falha vista no SEELE: não informada'));
});
