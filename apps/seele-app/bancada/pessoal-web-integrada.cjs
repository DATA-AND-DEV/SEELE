// Página completa + encontro Rust + WASM + RTC. Só o microfone é uma fonte
// sintética: nenhuma captura real e nenhuma saída audível durante a prova.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const { spawn } = require('node:child_process');
const engines = require('./playwright.cjs');
const segundosDeEnsaio = Number(process.env.SEELE_SOAK_SECONDS || 0);
const portaDoSinal = Number(process.env.SEELE_PROVA_SINAL_PORT || 19886);
assert.ok(Number.isInteger(segundosDeEnsaio) && segundosDeEnsaio >= 0 && segundosDeEnsaio <= 3600);
assert.ok(Number.isInteger(portaDoSinal) && portaDoSinal > 1024 && portaDoSinal <= 65535);
const amostrasDoEnsaio = [];
const registroPendenteDaProva = process.env.SEELE_PROVA_REGISTRO_PENDENTE === '1';
const soPreparoDoConvite = process.argv.includes('--so-preparo');
const cancelarPreparoDoConvite = process.argv.includes('--cancelar-preparo');
const soFormularioInicial = process.argv.includes('--so-formulario');
assert.ok(!soPreparoDoConvite || registroPendenteDaProva, '--so-preparo exige SEELE_PROVA_REGISTRO_PENDENTE=1');
assert.ok(!cancelarPreparoDoConvite || registroPendenteDaProva, '--cancelar-preparo exige SEELE_PROVA_REGISTRO_PENDENTE=1');
const raiz = path.resolve(__dirname, '../../..'), ui = path.join(raiz, 'apps/seele-app/ui');
const headersOrigem = fs.readFileSync(path.join(raiz, 'apps/seele-app/web/_headers'), 'utf8');
const csp = headersOrigem.match(/^  Content-Security-Policy: (.+)$/m)[1];
const mime = { '.html':'text/html', '.js':'text/javascript', '.css':'text/css', '.wasm':'application/wasm', '.svg':'image/svg+xml', '.woff2':'font/woff2' };
function instrumentarPagina() {
            window.operacoesDoConviteDaProva=[];
            const criarServico=criarServicoDeChamadasWeb;
            criarServicoDeChamadasWeb=async args=>{
              const servico=await criarServico(args);
              return Object.freeze({...servico,
                abrirConvite(...a){operacoesDoConviteDaProva.push('abrir');return servico.abrirConvite(...a);},
                chamar(...a){operacoesDoConviteDaProva.push('chamar');return servico.chamar(...a);},
              });
            };
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
}
(async () => {
  const server = http.createServer((req, res) => {
    if (req.url === '/chamada/1') {
      const proxy = http.request(`http://127.0.0.1:${portaDoSinal}/chamada/1`, { method: req.method, headers: req.headers }, reply => {
        res.writeHead(reply.statusCode, reply.headers); reply.pipe(res);
      });
      proxy.on('error', () => { res.statusCode = 502; res.end(); }); req.pipe(proxy); return;
    }
    const nome = req.url.split('?')[0] === '/' ? 'index.html' : decodeURIComponent(req.url.split('?')[0].slice(1));
    const file = nome === 'seele_web.wasm' ? (process.env.SEELE_CHAMADA_WASM || path.join(ui, 'seele_web.wasm')) : path.resolve(ui, nome);
    if ((nome !== 'seele_web.wasm' && !file.startsWith(ui + path.sep)) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.statusCode=404;res.end();return; }
    res.setHeader('Content-Type', mime[path.extname(file)] || 'application/octet-stream');
    if (nome === 'index.html') res.setHeader('Content-Security-Policy', csp);
    for (const linha of headersOrigem.split('\n/*\n')[1].split('\n/')[0].split('\n')) {
      const h = linha.match(/^  ([^:]+): (.+)$/); if (h) res.setHeader(h[1], h[2]);
    }
    if (nome === 'pessoal-web-entrada.js') {
      res.end(`(${instrumentarPagina.toString()})();\n` + fs.readFileSync(file, 'utf8')
        .replace('      if (preparo) await preparo.pronto;', process.argv.includes('--reverter-preparo') ? '' : '      if (preparo) await preparo.pronto;')
        .replace('criarServicoDeChamadasWeb({', 'criarServicoDeChamadasWeb({ origemDosLinks: location.origin, urlDoEncontro: location.origin + "/chamada/1", iceServers: [],'));
    } else res.end(fs.readFileSync(file));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const origem = `http://127.0.0.1:${server.address().port}`;
  const ponto = spawn(path.join(raiz, 'target/debug/seele-encontro'), ['--porta','0','--rede-local','--sinal',`127.0.0.1:${portaDoSinal}`,'--origem',origem], { stdio: ['ignore','pipe','pipe'] });
  let logs = ''; ponto.stderr.on('data', b => logs += b);
  try {
    let pronto = false;
    for (let n=0;n<50;n++) { try { const r=await fetch(origem+'/chamada/1'); if(r.ok){pronto=true;break;} } catch {} await new Promise(ok=>setTimeout(ok,100)); }
    assert.ok(pronto, logs);
    for (const engine of (process.env.ENGINE ? [process.env.ENGINE] : ['chromium','webkit'])) {
      const browser = await engines[engine]().launch({ headless: true });
      try {
        if (soFormularioInicial) {
          for (let tentativa=1;tentativa<=10;tentativa++) {
            const contexto=await browser.newContext({viewport:{width:390,height:800}});
            const pagina=await contexto.newPage();
            const erros=[];
            pagina.on('pageerror',e=>erros.push(e.message));
            await pagina.addInitScript(()=>{
              window.eventosDoFormularioDaProva=[];
              const retrato=(tipo,alvo)=>({tipo,pronto:document.readyState,
                ajudante:typeof globalThis.CELULAR?.ajudante('conversaPessoal')?.abrir,
                alvoConectado:alvo?.isConnected,
                oculta:document.querySelector('.pessoal-web')?.hidden,
                fase:document.querySelector('.pessoal-web')?.dataset.fase,
                nomeVisivel:!!document.querySelector('.pessoal-web-nome')?.getClientRects().length});
              const ehConversar=no=>no?.getAttribute?.('aria-label')==='CONVERSAR POR LINK'||no?.textContent?.trim()==='CONVERSAR POR LINK';
              for(const tipo of ['pointerdown','pointerup','click']) for(const captura of [true,false]) {
                document.addEventListener(tipo,e=>{
                  const alvo=e.target.closest?.('button');
                  if(ehConversar(alvo)) {
                    eventosDoFormularioDaProva.push(retrato(tipo+(captura?'-captura':'-bolha'),alvo));
                    if(tipo==='click'&&!captura) queueMicrotask(()=>eventosDoFormularioDaProva.push(retrato('apos-click',alvo)));
                  }
                },captura);
              }
              new MutationObserver(mudancas=>{
                for(const m of mudancas) for(const no of m.removedNodes) {
                  if(ehConversar(no)||[...no.querySelectorAll?.('button')||[]].some(ehConversar)) eventosDoFormularioDaProva.push(retrato('botao-removido',no));
                }
              }).observe(document,{subtree:true,childList:true});
            });
            try {
              await pagina.goto(origem);
              await pagina.getByRole('button',{name:'CONVERSAR POR LINK',exact:true}).click();
              await pagina.getByLabel('Seu nome',{exact:true}).waitFor({state:'visible',timeout:5000});
              assert.deepEqual(erros,[]);
              console.log(JSON.stringify({engine,tentativa,resultado:'PASS',eventos:await pagina.evaluate(()=>eventosDoFormularioDaProva)}));
            } catch(erro) {
              console.error(JSON.stringify({engine,tentativa,resultado:'FALHOU',erros,diagnostico:await pagina.evaluate(()=>({eventos:eventosDoFormularioDaProva,html:document.querySelector('.pessoal-web')?.outerHTML}))}));
              throw erro;
            } finally { await contexto.close(); }
          }
          continue;
        }
        const contexto = await browser.newContext({ viewport: { width: 390, height: 800 } });
        const prepararContexto = contexto => contexto.addInitScript(() => {
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
        const contextoB = await browser.newContext({ viewport: { width: 390, height: 800 } });
        await Promise.all([prepararContexto(contexto), prepararContexto(contextoB)]);
        const a = await contexto.newPage(), b = await contextoB.newPage();
        const erros = [];
        for (const p of [a,b]) p.on('pageerror', e=>erros.push(e.message));
        await a.goto(origem);
        await a.evaluate(()=>{
          window.cliquesIniciaisDaProva=[];
          document.addEventListener('click',e=>{
            const botao=e.target.closest('button');
            if (botao?.getAttribute('aria-label')==='CONVERSAR POR LINK' || botao?.textContent.trim()==='CONVERSAR POR LINK') {
              cliquesIniciaisDaProva.push({pronto:document.readyState,ajudante:typeof globalThis.CELULAR?.ajudante('conversaPessoal')?.abrir,
                oculta:document.querySelector('.pessoal-web')?.hidden});
            }
          });
        });
        await a.getByRole('button',{name:'CONVERSAR POR LINK',exact:true}).click();
        await a.getByLabel('Seu nome', {exact:true}).fill('Anfitrião').catch(async erro=>{
          console.error('diagnóstico do formulário inicial',engine,await a.evaluate(()=>({cliques:cliquesIniciaisDaProva,vistas:[...document.querySelectorAll('.pessoal-web')].map(el=>({oculta:el.hidden,fase:el.dataset.fase,estado:el.querySelector('.pessoal-web-estado')?.textContent,falha:el.querySelector('.pessoal-web-erro')?.textContent,nomeVisivel:!!el.querySelector('.pessoal-web-nome')?.getClientRects().length}))})),erros);
          throw erro;
        });
        await a.getByRole('button',{name:'CRIAR CONVERSA',exact:true}).click();
        const campoLink = a.getByLabel('Link da conversa');
        await campoLink.waitFor({state:'visible'});
        const link = await campoLink.inputValue();
        // Latência controlada na recepção automática: o clique no convite
        // continua permitido enquanto o Registrar real ainda não respondeu.
        let liberarRegistro, registrarRetido;
        const registroRetido = new Promise(ok=>registrarRetido=ok);
        const liberar = new Promise(ok=>liberarRegistro=ok);
        if (registroPendenteDaProva) {
          let reteve = false;
          await b.route('**/chamada/1', async route=>{
            if (!reteve && route.request().method()==='POST' && route.request().postData()?.includes('"Registrar"')) {
              reteve = true; registrarRetido(); await liberar;
            }
            await route.continue();
          });
        }
        await b.goto(link);
        if (registroPendenteDaProva) {
          let prazo;
          try { await Promise.race([registroRetido, new Promise((_,nao)=>prazo=setTimeout(()=>nao(Error('Registrar não chegou à barreira')),10000))]); }
          finally { clearTimeout(prazo); }
        }
        assert.equal(new URL(b.url()).hash, '', 'convite ficou na barra');
        await b.getByLabel('Seu nome',{exact:true}).fill('Convidado');
        await b.getByRole('button',{name:'ENTRAR NA CONVERSA',exact:true}).click();
        if (registroPendenteDaProva) {
          await b.waitForTimeout(150);
          assert.equal(await b.locator('.pessoal-web-erro').isVisible(),false,'clique cedo foi recusado durante preparo do convite');
          assert.equal(await a.getByRole('button',{name:'ATENDER',exact:true}).isVisible(),false,'chamou antes de concluir o preparo');
          if (cancelarPreparoDoConvite) await b.getByRole('button',{name:'VOLTAR',exact:true}).click();
          liberarRegistro();
        }
        if (cancelarPreparoDoConvite) {
          await b.waitForFunction(()=>seelePessoalWeb.estadoDaSala.estado==='NoAr');
          // A recepção pessoal pode terminar, mas o convite fechado não volta.
          await b.waitForTimeout(150);
          assert.deepEqual(await b.evaluate(()=>operacoesDoConviteDaProva),[],'convite cancelado foi aberto ou chamado depois da resposta tardia');
          assert.equal(await b.locator('.pessoal-web').isVisible(),false);
          assert.equal(await a.getByRole('button',{name:'ATENDER',exact:true}).isVisible(),false);
          await b.getByRole('button',{name:'CONVERSAR POR LINK',exact:true}).click();
          await b.getByRole('button',{name:'CRIAR CONVERSA',exact:true}).waitFor({state:'visible'});
          assert.deepEqual(erros,[]);
          console.log(`${engine}: VOLTAR durante Registrar retido não abriu nem chamou o convite após liberação PASS`);
          await contexto.close();await contextoB.close();continue;
        }
        await a.getByRole('button',{name:'ATENDER',exact:true}).click().catch(async erro => {
          console.error('diagnóstico do toque',engine,await Promise.all([a,b].map(p=>p.evaluate(()=>({
            fase:document.querySelector('.pessoal-web')?.dataset.fase,
            estado:document.querySelector('.pessoal-web-estado')?.textContent,
            falha:document.querySelector('.pessoal-web-erro')?.textContent,
          })))),erros);
          throw erro;
        });
        for (const p of [a,b]) await p.locator('.pessoal-web-estado').filter({hasText:'Conversa conectada'}).waitFor();
        for (const [p,nomes] of [[a,['Anfitrião','Convidado']],[b,['Convidado','Anfitrião']]]) {
          await p.waitForFunction(esperados=>JSON.stringify([...document.querySelectorAll('.pessoal-web-participante-nome')].map(el=>el.textContent))===JSON.stringify(esperados),nomes);
          assert.equal(await p.evaluate(()=>fontesDaProva.length),0,'aceitar a conversa abre saída, sem pedir captura');
        }
        if (soPreparoDoConvite) {
          await b.getByRole('button',{name:'SAIR DA CONVERSA',exact:true}).click();
          await a.locator('.pessoal-web-estado').filter({hasText:'Conversa encerrada'}).waitFor();
          assert.deepEqual(erros,[]);
          console.log(`${engine}: Registrar retido + clique cedo aguardou preparo e chamou o anfitrião correto PASS`);
          await contexto.close();await contextoB.close();continue;
        }
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
        // Retenção opcional: áudio sintético contínuo e mensagens nos dois
        // sentidos. Não mede latência física nem atravessamento de NAT/5G.
        if (segundosDeEnsaio) {
          const inicio = Date.now();
          let anteriores = null;
          while (true) {
            const medidas = await Promise.all([a,b].map(p => p.evaluate(async () => ({
              estado: document.querySelector('.pessoal-web')?.dataset.fase,
              contas: (await vozesDaProva[0].contas()).pagina,
              faixas: fontesDaProva.map(t => t.readyState),
              falhas: falhasDaProva,
            }))));
            for (const [i,m] of medidas.entries()) {
              assert.equal(m.estado, 'dentro');
              assert.deepEqual(m.falhas, []);
              assert.ok(m.faixas.includes('live'));
              if (anteriores) {
                assert.ok(m.contas.enviados > anteriores[i].contas.enviados, 'envio de voz parou');
                assert.ok(m.contas.decodificados > anteriores[i].contas.decodificados, 'recepção de voz parou');
              }
            }
            const decorrido = Date.now() - inicio;
            amostrasDoEnsaio.push({engine,decorridoMs:decorrido,medidas});
            if (process.env.SEELE_PROVA_RELATORIO) fs.writeFileSync(process.env.SEELE_PROVA_RELATORIO, JSON.stringify({concluido:false,segundosDeEnsaio,amostras:amostrasDoEnsaio},null,2));
            console.log(`${engine}: retenção ${Math.floor(decorrido/1000)}s, voz progredindo nos dois sentidos`);
            for (const [i,p] of [a,b].entries()) {
              const texto = `retenção ${engine} ${i} ${decorrido}`;
              await p.getByRole('textbox',{name:'Mensagem',exact:true}).fill(texto);
              await p.getByRole('button',{name:'ENVIAR',exact:true}).click();
              await [b,a][i].getByText(texto,{exact:true}).waitFor({timeout:10000});
            }
            if (decorrido >= segundosDeEnsaio * 1000) break;
            anteriores = medidas;
            await new Promise(ok => setTimeout(ok,Math.min(30000,segundosDeEnsaio * 1000 - decorrido)));
          }
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
    if (process.env.SEELE_PROVA_RELATORIO) fs.writeFileSync(process.env.SEELE_PROVA_RELATORIO, JSON.stringify({concluido:true,segundosDeEnsaio,amostras:amostrasDoEnsaio},null,2));
  } catch(error) { console.error(logs); throw error; }
  finally { ponto.kill();server.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
