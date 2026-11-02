// Executa o transmissor do produto com falhas e eventos atrasados determinísticos.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const context=vm.createContext({console,Uint8Array,Promise,setTimeout,clearTimeout});
vm.runInContext(fs.readFileSync(require('node:path').join(__dirname,'../ui/compartilhar-web.js'),'utf8'),context);
const factory=vm.runInContext('criarATransmissaoDoNavegador',context);
const tick=()=>new Promise(ok=>setImmediate(ok));
function track(kind){return {kind,readyState:'live',listeners:{},addEventListener(n,cb){this.listeners[n]=cb;},stop(){this.readyState='ended';},ended(){this.readyState='ended';this.listeners.ended?.();},getSettings(){return {width:640,height:360};}};}
function media(){const tracks=[track('video'),track('audio')];return {getTracks:()=>tracks,getVideoTracks:()=>[tracks[0]],getAudioTracks:()=>[tracks[1]]};}
(async()=>{
 let fail=false,openReject;const calls=[];
 const tx=factory({sessao:{atender(cmd){calls.push(cmd);if(fail&&cmd==='compartilhar_tela')return {erro:'recusado'};return {resposta:cmd==='codificacao_da_tela'?{largura:640,altura:360}:[]};},abrirUmFluxo(){return new Promise((ok,bad)=>{openReject=bad;});}},wasm:{},eu:()=>1,emitir(){}});
 const rejected=media();fail=true;assert.throws(()=>tx.comecar(rejected,{}));assert.equal(tx.noAr,false);assert.ok(rejected.getTracks().every(t=>t.readyState==='ended'));fail=false;
 const first=media();tx.comecar(first,{});first.getAudioTracks()[0].ended();assert.equal(tx.noAr,true,'áudio terminar não encerra vídeo');
 tx.aoDobrar({ScreenShareStarted:{person:1,screen:10}});await tick();
 tx.parar();const second=media();tx.comecar(second,{});first.getVideoTracks()[0].ended();assert.equal(tx.noAr,true,'ended antigo não encerra nova captura');
 openReject(new Error('abertura antiga falhou'));await tick();assert.equal(tx.noAr,true,'falha atrasada não encerra reinício');
 second.getVideoTracks()[0].ended();assert.equal(tx.noAr,false);assert.ok(second.getTracks().every(t=>t.readyState==='ended'));
 tx.parar();assert.equal(tx.noAr,false);
 console.log('ciclo captura: recusa libera faixas, fim do áudio, fim atrasado, abertura atrasada, fim do vídeo e parada repetida OK');
})().catch(e=>{console.error(e);process.exitCode=1;});
