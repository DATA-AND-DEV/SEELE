// Configuração do serviço JS com WASM e rede controlados: sem mídia ou rede real.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const crypto=require('node:crypto').webcrypto;
async function prova(opcoes={}){
 const cfg={origem_dos_links:'https://produto.exemplo.invalid',url_do_encontro:'https://sinal.exemplo.invalid/chamada/1',ice_servers:[{urls:'stun:sinal.exemplo.invalid:1234'}]};
 const chamadas=[],pedidos=[],enc=new TextEncoder(),dec=new TextDecoder();let saida=0;
 const wasm={memory:{buffer:new ArrayBuffer(65536)},alocar:()=>0,liberar(){},saida_ptr:()=>32768,saida_len:()=>saida,
  chamada_operar(ptr,tamanho){const pedido=JSON.parse(dec.decode(new Uint8Array(this.memory.buffer,ptr,tamanho)));pedidos.push(pedido.acao);
   const valor=pedido.acao==='configuracao'?(opcoes.falharConfiguracao?{erro:'Operacao'}:cfg):pedido.acao==='codigo_da_chave'?{codigo:'CODIGO-CONTROLADO'}:pedido.acao==='convite_ler'?{convite:{dono:Array(32).fill(1),expira_em:9999999999}}:{bytes:[]};
   const bytes=enc.encode(JSON.stringify(valor));new Uint8Array(this.memory.buffer).set(bytes,32768);saida=bytes.length;
  }};
 const ctx={URL,TextEncoder,TextDecoder,crypto,AbortController,DOMException,setTimeout,clearTimeout,
  criarEnlaceDaChamadaWeb(){throw Error('Não deve abrir mídia');}};vm.createContext(ctx);
 vm.runInContext(fs.readFileSync(__dirname+'/../ui/chamada-web.js','utf8'),ctx);
 const origem=opcoes.origemDosLinks??cfg.origem_dos_links;
 const criar=()=>ctx.criarServicoDeChamadasWeb({wasm,identidade:{publica:Array(32).fill(0),assinar:async()=>({primeira:Array(32).fill(0),segunda:Array(32).fill(0)})},
  buscar:async url=>{chamadas.push(url);throw Error('Rede controlada');},...opcoes});
 if(opcoes.falharConfiguracao){await assert.rejects(criar,/Operacao/);assert.deepEqual(pedidos,['configuracao']);return;}
 const servico=await criar();
 try{
  assert.ok(servico.abrirConvite(origem+'/#chamada/prova').dono);
  assert.ok(servico.abrirConvite('#chamada/prova').dono);
  assert.throws(()=>servico.abrirConvite('https://nao-autorizado.exemplo.invalid/#chamada/prova'),e=>e.origemDoConviteInvalida===true);
  await assert.rejects(servico.criarConvite({apelido:'Pessoa'}),/Rede controlada/);
  assert.deepEqual(chamadas,[opcoes.urlDoEncontro??cfg.url_do_encontro]);
  assert.equal(pedidos.filter(p=>p==='configuracao').length,1);
 }finally{servico.destruir();}
}
(async()=>{await prova();await prova({origemDosLinks:'http://127.0.0.1:12345',urlDoEncontro:'http://127.0.0.1:12346/chamada/1',iceServers:[]});await prova({falharConfiguracao:true});console.log('Configuração: defaults Rust, overrides locais e falha sem fallback PASS');})().catch(e=>{console.error(e);process.exitCode=1});
