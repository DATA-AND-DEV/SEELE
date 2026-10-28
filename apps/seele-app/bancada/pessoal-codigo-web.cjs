// Identidade durável/agenda real + descoberta assinada + RTC direto por código.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { spawn } = require('node:child_process');
const engines = require('./playwright.cjs');
const raiz = path.resolve(__dirname, '../../..');
const wasm = process.env.SEELE_CHAMADA_WASM || path.join(raiz, 'apps/seele-app/ui/seele_web.wasm');
(async () => {
 const servidor = http.createServer((req, res) => {
  const nome = req.url.slice(1);
  if (nome === 'wasm') { res.end(fs.readFileSync(wasm)); return; }
  if (['identidade-web.js', 'chamada-web.js', 'rtc-chamada-web.js'].includes(nome)) {
   res.setHeader('Content-Type', 'application/javascript'); res.end(fs.readFileSync(path.join(raiz, 'apps/seele-app/ui', nome))); return;
  }
  res.setHeader('Content-Type', 'text/html');
  res.end(`<!doctype html><script>const PONTE_NO_LUGAR_DO_TAURI=true; async function instanciarOWasmDaSessao(){return (await WebAssembly.instantiate(await(await fetch('/wasm')).arrayBuffer(),{})).instance.exports;}</script><script src="/identidade-web.js"></script><script src="/rtc-chamada-web.js"></script><script src="/chamada-web.js"></script>`);
 });
 await new Promise(r => servidor.listen(0, '127.0.0.1', r));
 const origem = `http://127.0.0.1:${servidor.address().port}`;
 const ponto = http.createServer(); await new Promise(r => ponto.listen(0, '127.0.0.1', r)); const porta = ponto.address().port; await new Promise(r => ponto.close(r));
 const urlDoEncontro = `http://127.0.0.1:${porta}/chamada/1`;
 const encontro = spawn(path.join(raiz, 'target/debug/seele-encontro'), ['--porta', '0', '--rede-local', '--sinal', `127.0.0.1:${porta}`, '--origem', origem], { stdio: ['ignore','pipe','pipe'] });
 let logs = ''; encontro.stdout.on('data', b => logs += b); encontro.stderr.on('data', b => logs += b);
 try {
  let pronto = false;
  for (let i = 0; i < 50; i++) { try { await fetch(urlDoEncontro); pronto = true; break; } catch { await new Promise(r => setTimeout(r, 100)); } }
  assert.ok(pronto, logs);
  for (const [motor, tipo] of Object.entries({ chromium: engines.chromium(), webkit: engines.webkit() })) {
   const navegador = await tipo.launch({ headless: true });
   try {
    const contextoA = await navegador.newContext(), contextoB = await navegador.newContext();
    const a = await contextoA.newPage(), concorrente = await contextoA.newPage(), b = await contextoB.newPage();
    await Promise.all([a, concorrente, b].map(p => p.goto(origem)));
    // Duas primeiras abas disputam a criação: a transação escolhe uma só chave.
    const codigos = await Promise.all([a, concorrente].map(p => p.evaluate(() => meuCodigoPessoalWeb())));
    assert.equal(codigos[0], codigos[1]); await concorrente.close();
    const codigoA = codigos[0], codigoB = await b.evaluate(() => meuCodigoPessoalWeb()); assert.notEqual(codigoA, codigoB);
    await a.evaluate(async codigo => {
     const chave = (await identidadePessoalDoNavegador()).chave;
     let exportavel = true; try { await crypto.subtle.exportKey('pkcs8', chave.privateKey); } catch { exportavel = false; }
     if (exportavel) throw Error('chave privada exportável');
     await noBanco('readwrite', loja => loja.put({ impressao: 'servidor-antigo', apelido: 'antes', marcador: 173 }));
     await alterarContatoPessoalWeb(codigo, c => ({ ...c, nome: 'Nome que eu escolhi', amigo: true }));
     const outraChave = Array.from(new Uint8Array(await crypto.subtle.exportKey('raw', (await crypto.subtle.generateKey('Ed25519', false, ['sign','verify'])).publicKey)));
     const outro = await operarCodigoPessoalWeb({ acao: 'codigo_da_chave', chave: outraChave });
     await alterarContatoPessoalWeb(outro, c => ({ ...c, nome: 'Nome que eu escolhi', amigo: true }));
    }, codigoB);
    await a.reload(); assert.equal(await a.evaluate(() => meuCodigoPessoalWeb()), codigoA);
    assert.equal(await a.evaluate(async () => (await servidorConhecido('servidor-antigo')).marcador), 173);
    const amigos = await a.evaluate(() => contatosPessoaisWeb(true)); assert.equal(amigos.length, 2); assert.ok(amigos.every(c => c.nome === 'Nome que eu escolhi')); assert.notEqual(amigos[0].codigo, amigos[1].codigo);
    for (const p of [a, b]) await p.evaluate(async urlDoEncontro => {
     window.estados = []; window.textos = []; window.pares = [];
     window.servico = await criarServicoDeChamadasWeb({ urlDoEncontro, iceServers: [], obterPerfil: async apelido => {
       const tela=document.createElement('canvas');tela.width=tela.height=16;
       tela.getContext('2d').fillRect(0,0,16,16);
       return {apelido,retrato:tela.toDataURL('image/png')};
     }, aoPerfil: perfil => guardarPerfilPessoalWeb(perfil),
     aoEstado: e => estados.push(e), aoTexto: e => textos.push(e), aoPar: p => pares.push(!!p) });
     await servico.receberPorCodigo({ apelido: 'Mesmo nome' });
    }, urlDoEncontro);
    await b.evaluate(codigo => servico.chamarCodigo({ codigo, apelido: 'Mesmo nome' }), codigoA);
    await a.waitForFunction(() => servico.ler().estado === 'tocando');
    assert.equal(await a.evaluate(() => servico.ler().apelido), 'Mesmo nome');
    assert.equal(await a.evaluate(() => servico.ler().codigo), codigoB);
    assert.equal(await a.evaluate(async codigo => (await contatosPessoaisWeb()).find(c => c.codigo === codigo).direcao, codigoB), 'perdida');
    assert.equal(await b.evaluate(() => pares.includes(true)), false);
    await a.evaluate(() => servico.atender());
    assert.equal(await a.evaluate(async codigo => (await contatosPessoaisWeb()).find(c => c.codigo === codigo).direcao, codigoB), 'recebi');
    await Promise.all([a,b].map(p => p.waitForFunction(() => servico.ler().estado === 'dentro', null, { timeout: 20000 })));
    await Promise.all([a,b].map(p=>p.waitForFunction(()=>servico.ler().apelido==='Mesmo nome')));
    assert.notEqual(codigoA,codigoB,'nomes iguais preservam identidades diferentes');
    await a.waitForFunction(async codigo=>(await lerPerfilPessoalWeb(codigo))?.retrato?.startsWith('data:image/png;base64,'),codigoB);
    await b.waitForFunction(async codigo=>(await lerPerfilPessoalWeb(codigo))?.retrato?.startsWith('data:image/png;base64,'),codigoA);
    assert.equal(await a.evaluate(codigo=>lerPerfilPessoalWeb(codigo,Array(32).fill(0)),codigoB),undefined,'cache não reutiliza foto sob outra chave');
    assert.equal(await a.evaluate(codigo=>lerPerfilPessoalWeb(codigo).then(p=>p.apelido),codigoB),'Mesmo nome');
    await b.evaluate(() => servico.enviarTexto('por código, direto'));
    await a.waitForFunction(() => textos.some(t => t.texto === 'por código, direto'));
    await b.evaluate(() => servico.encerrar());
    await a.waitForFunction(() => servico.ler().estado === 'encerrada');
    // Inverte as pontas usando os mesmos códigos e identidades persistentes.
    await a.evaluate(codigo => servico.chamarCodigo({ codigo, apelido: 'Mesmo nome' }), codigoB);
    await b.waitForFunction(() => servico.ler().estado === 'tocando');
    await b.evaluate(() => servico.recusar());
    await a.waitForFunction(() => servico.ler().estado === 'encerrada');
    await a.evaluate(codigo => servico.chamarCodigo({ codigo, apelido: 'Mesmo nome' }), codigoB);
    await b.waitForFunction(() => servico.ler().estado === 'tocando');
    await a.evaluate(() => servico.cancelar());
    await b.waitForFunction(() => servico.ler().estado === 'encerrada');
    await a.evaluate(codigo => alterarContatoPessoalWeb(codigo, c => ({ ...c, chavePublica: 'pino-de-outra-chave' })), codigoB);
    assert.equal(await a.evaluate(async codigo => { try { await servico.chamarCodigo({ codigo, apelido: 'Mesmo nome' }); return false; } catch(e) { return /ChaveMudou/.test(e.message); } }, codigoB), true);
    await a.evaluate(codigo => alterarContatoPessoalWeb(codigo, c => ({ ...c, chavePublica: null, bloqueado: true })), codigoB);
    const toquesAntes = await a.evaluate(() => estados.filter(e => e.estado === 'tocando').length);
    await b.evaluate(codigo => servico.chamarCodigo({ codigo, apelido: 'Mesmo nome' }), codigoA);
    await b.waitForFunction(() => servico.ler().estado === 'encerrada');
    assert.equal(await a.evaluate(() => estados.filter(e => e.estado === 'tocando').length), toquesAntes, 'bloqueado não pode tocar');
    assert.equal(await a.evaluate(()=>retratoParaChamadaPessoal('https://exemplo.invalid/foto.png')),null,'não busca retrato remoto');
    assert.equal(await a.evaluate(async codigo=>(await contatosPessoaisWeb()).find(c=>c.codigo===codigo).nome,codigoB),'Nome que eu escolhi','perfil remoto não troca renomeação local');
    const recusasDoPerfil=await a.evaluate(async codigo=>{
      const perfil=await lerPerfilPessoalWeb(codigo);
      const uri=perfil.retrato;
      const alterar=editar=>{const b=Uint8Array.from(atob(uri.slice(22)),c=>c.charCodeAt(0));editar(b);return 'data:image/png;base64,'+btoa(Array.from(b,b=>String.fromCharCode(b)).join(''));};
      const vetores=[
        {v:1,apelido:'x'.repeat(129),retrato:null},
        {v:1,apelido:'x\ny',retrato:null},
        {v:1,apelido:'Lia',retrato:'https://exemplo.invalid/foto.png'},
        {v:1,apelido:'Lia',retrato:'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4='},
        {v:1,apelido:'Lia',retrato:'data:image/png;base64,'+btoa('x'.repeat(8193))},
        {v:1,apelido:'Lia',retrato:'data:image/png;base64,AAAA'},
        {v:1,apelido:'Lia',retrato:alterar(b=>new DataView(b.buffer).setUint32(16,129))},
        {v:1,apelido:'Lia',retrato:uri.slice(0,66)},
      ];
      let recusas=0;for(const p of vetores)try{await validarPerfilDaChamadaPessoal(p);}catch{recusas++;}
      if(recusas!==vetores.length)throw Error('perfil inválido aceito');
      await validarPerfilDaChamadaPessoal({v:1,apelido:'Lia',retrato:uri});
      await validarPerfilDaChamadaPessoal({v:1,apelido:'Lia',retrato:null});
      return recusas;
    },codigoB);
    assert.equal(recusasDoPerfil,8);
    const perfilAntes=await a.evaluate(codigo=>lerPerfilPessoalWeb(codigo),codigoB);
    await a.reload();
    await contextoA.setOffline(true);
    assert.deepEqual(await a.evaluate(codigo=>lerPerfilPessoalWeb(codigo),codigoB),perfilAntes,'cache da última conexão sobrevive à recarga e leitura offline');
    await contextoA.setOffline(false);
    const publicaB=await b.evaluate(async()=> (await identidadePessoalDoNavegador()).publica);
    await a.evaluate(async ({codigo,publica})=>guardarPerfilPessoalWeb({codigo,publica,apelido:'Mesmo nome',retrato:null}),{codigo:codigoB,publica:publicaB});
    assert.equal(await a.evaluate(codigo=>lerPerfilPessoalWeb(codigo).then(p=>p.retrato),codigoB),null,'remover foto substitui a cópia anterior');
    console.log(JSON.stringify({ motor, identidadePersistente: true, primeirasAbasConvergem: true, servidorAntigoPreservado: true, nomesIguaisNaoColidem: true, codigoNosDoisSentidos: true, aceiteRecusaCancelamento: true, pinoIncorretoRecusado: true, bloqueadoNaoToca: true, perfilPngCacheOffline: true, recusasDoPerfil }));
   } finally { await navegador.close(); }
  }
 } finally { encontro.kill('SIGTERM'); await new Promise(r => servidor.close(r)); }
})().catch(e => { console.error(e); process.exitCode = 1; });
