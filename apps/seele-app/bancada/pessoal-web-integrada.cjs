// Página completa + encontro Rust + WASM + RTC. Só o microfone é uma fonte
// sintética: nenhuma captura real e nenhuma saída audível durante a prova.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const { spawn } = require('node:child_process');
const engines = require('./playwright.cjs');
const raiz = path.resolve(__dirname, '../../..'), ui = path.join(raiz, 'apps/seele-app/ui');
const headersOrigem = fs.readFileSync(path.join(raiz, 'apps/seele-app/web/_headers'), 'utf8');
const csp = headersOrigem.match(/^  Content-Security-Policy: (.+)$/m)[1];
const mime = { '.html':'text/html', '.js':'text/javascript', '.css':'text/css', '.wasm':'application/wasm', '.svg':'image/svg+xml', '.woff2':'font/woff2' };
(async () => {
  const server = http.createServer((req, res) => {
    if (req.url === '/chamada/1') {
      const proxy = http.request('http://127.0.0.1:19886/chamada/1', { method: req.method, headers: req.headers }, reply => {
        res.writeHead(reply.statusCode, reply.headers); reply.pipe(res);
      });
      proxy.on('error', () => { res.statusCode = 502; res.end(); }); req.pipe(proxy); return;
    }
    const nome = req.url.split('?')[0] === '/' ? 'index.html' : decodeURIComponent(req.url.split('?')[0].slice(1));
    const file = nome === 'seele_web.wasm' ? path.join(raiz, 'target/wasm32-unknown-unknown/release/seele_web.wasm') : path.resolve(ui, nome);
    if ((nome !== 'seele_web.wasm' && !file.startsWith(ui + path.sep)) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.statusCode=404;res.end();return; }
    res.setHeader('Content-Type', mime[path.extname(file)] || 'application/octet-stream');
    if (nome === 'index.html') res.setHeader('Content-Security-Policy', csp);
    for (const linha of headersOrigem.split('\n/*\n')[1].split('\n/')[0].split('\n')) {
      const h = linha.match(/^  ([^:]+): (.+)$/); if (h) res.setHeader(h[1], h[2]);
    }
    if (nome === 'pessoal-web-entrada.js') {
      res.end(fs.readFileSync(file, 'utf8').replace('criarServicoDeChamadasWeb({', 'criarServicoDeChamadasWeb({ urlDoEncontro: location.origin + "/chamada/1", iceServers: [],'));
    } else res.end(fs.readFileSync(file));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const origem = `http://127.0.0.1:${server.address().port}`;
  const ponto = spawn(path.join(raiz, 'target/debug/seele-encontro'), ['--porta','0','--rede-local','--sinal','127.0.0.1:19886','--origem',origem], { stdio: ['ignore','pipe','pipe'] });
  let logs = ''; ponto.stderr.on('data', b => logs += b);
  try {
    let pronto = false;
    for (let n=0;n<50;n++) { try { const r=await fetch(origem+'/chamada/1'); if(r.ok){pronto=true;break;} } catch {} await new Promise(ok=>setTimeout(ok,100)); }
    assert.ok(pronto, logs);
    for (const engine of (process.env.ENGINE ? [process.env.ENGINE] : ['chromium','webkit'])) {
      const browser = await engines[engine]().launch({ headless: true });
      try {
        const contexto = await browser.newContext({ viewport: { width: 390, height: 800 } });
        await contexto.addInitScript(() => {
          window.etapasDoAudio = [];
          const resume = AudioContext.prototype.resume;
          AudioContext.prototype.resume = function(...args) {
            const etapa = { etapa: this.fonteSintetica ? "fonte-sintetica-resume" : "produto-resume", gesto: navigator.userActivation?.isActive, estado: this.state };
            etapasDoAudio.push(etapa);
            try{return resume.apply(this,args).catch(e=>{etapa.erro=e.name;throw e;});}catch(e){etapa.erro=e.name;throw e;}
          };
          const play = HTMLMediaElement.prototype.play;
          HTMLMediaElement.prototype.play = function(...args) {
            const etapa = { etapa: "saida-play", gesto: navigator.userActivation?.isActive, muted: this.muted };
            etapasDoAudio.push(etapa);
            try{return play.apply(this,args).catch(e=>{etapa.erro=e.name;throw e;});}catch(e){etapa.erro=e.name;throw e;}
          };
          const Worklet = AudioWorkletNode;
          window.AudioWorkletNode = class extends Worklet {
            constructor(ctx,nome,opts) {const etapa={etapa:'worklet-'+nome};etapasDoAudio.push(etapa);try{super(ctx,nome,opts);}catch(e){etapa.erro=e.name;throw e;}}
          };
          const criar = Document.prototype.createElement;
          Document.prototype.createElement = function(tag, ...args) {
            const el = criar.call(this, tag, ...args);
            if (String(tag).toLowerCase() === 'audio') el.muted = true;
            return el;
          };
          window.fontesDaProva = [];
          Object.defineProperty(MediaDevices.prototype, "getUserMedia", { configurable: true, value: async function fonteSinteticaDaProva() {
            etapasDoAudio.push({etapa:"gum-sintetico",gesto:navigator.userActivation?.isActive});
            let ctx;try{ctx=new AudioContext({sampleRate:48000});}catch(e){etapasDoAudio.push({etapa:'fonte-construtor',erro:e.name});throw e;}
            const oscillator = ctx.createOscillator(), destino = ctx.createMediaStreamDestination();
            ctx.fonteSintetica = true;
            oscillator.frequency.value = 440; oscillator.connect(destino); oscillator.start(); await ctx.resume();
            const track = destino.stream.getAudioTracks()[0], stop = track.stop.bind(track);
            let encerrada = false;
            track.stop = () => { if (encerrada) return; encerrada = true; stop(); try { oscillator.stop(); } catch {} void ctx.close().catch(() => {}); };
            fontesDaProva.push(track);
            return destino.stream;
          } });
        });
        const a = await contexto.newPage(), b = await contexto.newPage();
        const erros = [];
        for (const p of [a,b]) p.on('pageerror', e=>erros.push(e.message));
        await a.goto(origem);
        await a.getByRole('button',{name:'CONVERSAR POR LINK',exact:true}).click();
        await a.getByLabel('Seu nome', {exact:true}).fill('Anfitrião');
        await a.getByRole('button',{name:'CRIAR CONVERSA',exact:true}).click();
        const campoLink = a.getByLabel('Link da conversa');
        await campoLink.waitFor({state:'visible'});
        const link = await campoLink.inputValue();
        await b.goto(link);
        assert.equal(new URL(b.url()).hash, '', 'convite ficou na barra');
        for (const p of [a,b]) {
          await p.evaluate(() => {
            window.vozesDaProva=[]; window.enlacesDaProva=[];
            const criarEnlace=criarEnlaceDaChamadaWeb;
            criarEnlaceDaChamadaWeb=args=>{const enlace=criarEnlace(args);enlacesDaProva.push(enlace);return enlace;};
            const criar=criarVozDoNavegador;
            window.falhasDaProva=[];
            criarVozDoNavegador = args => {
              const original=args.aoAcontecer;
              const voz=criar({...args,aoAcontecer:e=>{ if(e.tipo==='falha')falhasDaProva.push(e);original(e); }});
              const ligar=voz.ligar;
              voz.ligar=async(...a)=>{try{return await ligar(...a);}catch(e){falhasDaProva.push({nome:e.name,texto:e.message});throw e;}};
              vozesDaProva.push(voz);return voz;
            };
          });
        }
        await b.getByLabel('Seu nome',{exact:true}).fill('Convidado');
        await b.getByRole('button',{name:'ENTRAR NA CONVERSA',exact:true}).click();
        await a.getByRole('button',{name:'ATENDER',exact:true}).click();
        for (const p of [a,b]) await p.locator('.pessoal-web-estado').filter({hasText:'Conversa conectada'}).waitFor();
        for (const [p,texto] of [[a,'Mensagem do anfitrião'],[b,'Mensagem do convidado']]) {
          await p.getByRole('textbox',{name:'Mensagem',exact:true}).fill(texto);
          await p.getByRole('button',{name:'ENVIAR',exact:true}).click();
        }
        for (const p of [a,b]) await p.waitForFunction(()=>document.querySelectorAll('.pessoal-web-mensagem').length===2);
        for (const p of [a,b]) {
          assert.equal(await p.evaluate(()=>navigator.mediaDevices.getUserMedia.name),'fonteSinteticaDaProva','a prova não pode pedir microfone real');
          await p.getByRole('button',{name:'LIGAR MICROFONE',exact:true}).click();
          await p.getByRole('button',{name:'DESLIGAR MICROFONE',exact:true}).waitFor({timeout:15000}).catch(async e => { console.error(engine, await p.evaluate(()=>({falhas:falhasDaProva,etapas:etapasDoAudio})), await p.locator('.pessoal-web').innerText(), erros); throw e; });
          assert.ok(await p.evaluate(()=>fontesDaProva.some(t=>t.readyState==='live')));
        }
        for (const p of [a,b]) {
          await p.waitForFunction(async () => {
            const c = await vozesDaProva[0]?.contas();
            if(c)window.contasIntegradasDaProva={enviados:c.pagina.enviados,decodificados:c.pagina.decodificados};
            return c?.pagina.enviados > 0 && c?.pagina.decodificados > 0;
          }, null, { timeout: 15000 });
        }
        // Uma permissão pendente não pode reativar captura depois de SAIR.
        await b.getByRole('button',{name:'DESLIGAR MICROFONE',exact:true}).click();
        await b.evaluate(()=>{
          const original=MediaDevices.prototype.getUserMedia;
          Object.defineProperty(MediaDevices.prototype,'getUserMedia',{configurable:true,value:async function permissaoPendenteDaProva(...args){
            const stream=await original.apply(this,args);
            return new Promise(ok=>{window.liberarPermissaoDaProva=()=>ok(stream);});
          }});
        });
        await b.getByRole('button',{name:'LIGAR MICROFONE',exact:true}).click();
        await b.waitForFunction(()=>typeof liberarPermissaoDaProva==='function');
        await b.getByRole('button',{name:'SAIR DA CONVERSA',exact:true}).click();
        await b.evaluate(()=>liberarPermissaoDaProva());

        await a.locator('.pessoal-web-estado').filter({hasText:'Conversa encerrada'}).waitFor({timeout:10000});
        for (const p of [a,b]) await p.waitForFunction(()=>fontesDaProva.every(t=>t.readyState==='ended'));
        async function voltarPeloMesmoLink() {
          await b.getByRole('button',{name:'ENTRAR NA CONVERSA',exact:true}).click();
          await a.getByRole('button',{name:'ATENDER',exact:true}).click();
          for(const p of [a,b])await p.locator('.pessoal-web-estado').filter({hasText:'Conversa conectada'}).waitFor();
          assert.equal(await b.evaluate(()=>location.hash),'');
        }
        await voltarPeloMesmoLink();
        await b.getByRole('textbox',{name:'Mensagem',exact:true}).fill('Depois de voltar');
        await b.getByRole('button',{name:'ENVIAR',exact:true}).click();
        for(const p of [a,b])await p.locator('.pessoal-web-conteudo').filter({hasText:'Depois de voltar'}).waitFor();
        assert.equal(await a.locator('.pessoal-web-mensagem').count(),1,'IDs de mensagem de chamadas diferentes não colidem');
        await b.evaluate(()=>enlacesDaProva.at(-1).abortar());
        for(const p of [a,b])await p.locator('.pessoal-web-estado').filter({hasText:'Conversa encerrada'}).waitFor();
        await voltarPeloMesmoLink();
        await b.getByRole('button',{name:'SAIR DA CONVERSA',exact:true}).click();
        await a.locator('.pessoal-web-estado').filter({hasText:'Conversa encerrada'}).waitFor();
        await a.getByRole('button',{name:'CRIAR CONVERSA',exact:true}).click();
        await campoLink.waitFor({state:'visible'});assert.notEqual(await campoLink.inputValue(),link);
        assert.equal(await a.locator('.pessoal-web-mensagem').count(),0);
        await a.getByRole('button',{name:'ENCERRAR CONVITE',exact:true}).click();
        for(const p of [a,b]) {
          const etapas=await p.evaluate(()=>etapasDoAudio.filter(e=>['saida-play','produto-resume'].includes(e.etapa)));
          assert.ok(etapas.length>=2&&etapas.every(e=>e.gesto===true&&!e.erro),JSON.stringify(etapas));
        }
        console.log(JSON.stringify({engine,audio:await Promise.all([a,b].map(p=>p.evaluate(()=>contasIntegradasDaProva))),cspReal:true,gestosAtivos:true,permissaoTardiaLiberada:true}));
        assert.deepEqual(erros,[]);
        console.log(`${engine}: UI real, convite limpo, aceite, chat bidirecional, voz sintética enviada/decodificada nos dois sentidos permissão tardia, saída/perda/reentrada pelo mesmo link e novo convite limpo PASS`);
      } finally { await browser.close(); }
    }
  } catch(error) { console.error(logs); throw error; }
  finally { ponto.kill();server.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
