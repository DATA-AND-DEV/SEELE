// Regressões de geometria/estado com UI real e ponte fixture, sem backend nativo.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { servir, respostas, retrato } = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
  const server = servir(); await new Promise(ok => server.listen(0,'127.0.0.1',ok));
  try {
    for (const engine of ['chromium','webkit']) {
      const browser = await engines[engine]().launch({headless:true});
      try {
        if(process.env.SEELE_PREVIEW_DIR) fs.mkdirSync(process.env.SEELE_PREVIEW_DIR,{recursive:true});
        const page = await browser.newPage({viewport:{width:1280,height:800}});
        const errors=[]; page.on('pageerror',e=>errors.push(e.message));
        await page.addInitScript(table => {
          window.testTable=table;
          window.__TAURI__={core:{invoke:async cmd=>testTable[cmd]??null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested(){},isMaximized:async()=>false})}};
        },respostas({snapshot:null,microfones:[],saidas:[],pacotes_no_cache:[],aceites_de_mods:[]}));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.evaluate(r=>{testTable.snapshot=r;$('tela-boot').hidden=true;$('tela-sessao').hidden=false;desenhar(r);},retrato());
        for(const zoom of [1,1.25,2]) for(const value of ['', 'mensagem', 'linha 1\nlinha 2\nlinha 3']) {
          await page.setViewportSize({width:Math.round(1280/zoom),height:Math.round(800/zoom)});
          await page.evaluate(({value})=>{
            $('tela-sessao').hidden=true;
            $('campo-mensagem').value=value;cresceOCampo();
          },{zoom,value});
          assert.notEqual(await page.locator('#campo-mensagem').evaluate(e=>e.style.height),'0px');
          await page.evaluate(()=>{$('tela-sessao').hidden=false;});
          await page.waitForFunction(()=>{ const e=document.getElementById("campo-mensagem"); return e.clientHeight >= Math.min(e.scrollHeight,160)-1; },null,{timeout:3000});
          const measured=await page.locator('#campo-mensagem').evaluate(e=>{const s=getComputedStyle(e);return {width:e.clientWidth,height:e.clientHeight,scroll:e.scrollHeight,min:parseFloat(s.lineHeight)+parseFloat(s.paddingTop)+parseFloat(s.paddingBottom)};});
          assert.ok(measured.width>=80,JSON.stringify(measured));
          assert.ok(measured.height>=measured.min-1,JSON.stringify({engine,zoom,value,measured}));
          assert.ok(measured.height>=Math.min(measured.scroll,160)-1,JSON.stringify({engine,zoom,value,measured}));
        }
        await page.setViewportSize({width:1280,height:800});
        await page.evaluate(()=>{$('campo-mensagem').value='';cresceOCampo();});
        // Uma apresentação válida substitui o nome visível, mas não seus detalhes.
        await page.evaluate(()=>{
          entrarNaGeracao(1);
          window.i=new InstanciaDeMod('teste/perfis','hash',geracaoDaSessao,{entregar(){},encerrar:async()=>{}});
          i.estado=ESTADOS_DE_MOD.ativa;modsCarregados.set('teste/perfis',i);
          window.handle=contribuicoesDosMods.registrar({id:'teste/perfis'},i,{ponto:'pessoa.cartao',modo:'substituir',alvo:'2',conteudo:[{forma:'texto',chave:'nome',dentro:'THORFINN'}]}).handle;
          desenhar(testTable.snapshot);
        });
        await page.evaluate(()=>{
          const c=document.createElement('canvas');c.width=400;c.height=180;
          const ctx=c.getContext('2d'),g=ctx.createLinearGradient(0,0,400,180);
          g.addColorStop(0,'#153d6e');g.addColorStop(.5,'#ed8535');g.addColorStop(1,'#234851');ctx.fillStyle=g;ctx.fillRect(0,0,400,180);
          testTable.ler_imagem_mod={papel:'imagem',bytes:256,uri:c.toDataURL('image/png')};
          contribuicoesDosMods.registrar({id:'teste/perfis'},i,{ponto:'pessoa.capa',modo:'substituir',alvo:String(testTable.snapshot.me),conteudo:{doServidor:{canal:1,campo:'image',pedido:{transporte:'volume',path:'volume:capa'}}}});
          desenhar(testTable.snapshot);
        });
        await page.waitForTimeout(300);
        // A imagem passa sob identidade, estado e controles: nenhuma placa local.
        const superficies=await page.locator('.operador[data-com-capa]').evaluate(e=>{
          const seletor='.operador-inicial,.operador-rotulo,.operador-nome,#voz-estado,.operador-glifo,#botao-server-sessao,#operador-sair,#operador-vista';
          return [...e.querySelectorAll(seletor)].filter(x=>x.dataset.ativo!=='sim').map(x=>({id:x.id||x.className,fundo:getComputedStyle(x).backgroundColor}));
        });
        assert.ok(superficies.length>=8);
        for(const s of superficies) assert.equal(s.fundo,'rgba(0, 0, 0, 0)',JSON.stringify(s));
        const perfil=page.locator('.operador[data-com-capa] .operador-quem');
        await perfil.hover();
        const hover=await perfil.locator('.operador-inicial').evaluate(e=>getComputedStyle(e).borderColor);
        await page.mouse.move(500,300);
        await perfil.focus();
        const foco=await perfil.evaluate(e=>({anel:getComputedStyle(e).outlineWidth,borda:getComputedStyle(e.querySelector('.operador-inicial')).borderColor}));
        assert.equal(foco.anel,'2px');assert.equal(foco.borda,hover);
        await perfil.evaluate(e=>e.blur());
        const estados=await page.locator('#botao-mudo').evaluate(e=>{
          const antes=e.getAttribute('data-ativo');e.dataset.ativo='sim';
          const ativo={fundo:getComputedStyle(e).backgroundColor,cor:getComputedStyle(e).color};
          if(antes===null)e.removeAttribute('data-ativo');else e.dataset.ativo=antes;
          return ativo;
        });
        assert.notEqual(estados.fundo,'rgba(0, 0, 0, 0)');
        assert.notEqual(estados.fundo,estados.cor);
        if(process.env.SEELE_PREVIEW_DIR) await page.screenshot({path:`${process.env.SEELE_PREVIEW_DIR}/${engine}-perfis-capa.png`});
        const linha=page.locator('li.pessoa-apresentada[data-pessoa-id="2"]').first();
        assert.equal(await linha.locator('.pessoa-apresentada-faixa .pessoa-nome').count(),0);
        assert.equal(await linha.locator('.pessoa-miolo').innerText(),'THORFINN');
        assert.equal(await linha.locator(':scope > details').getAttribute('open'),null);
        await linha.locator(':scope > details > summary').click();
        assert.ok((await linha.locator('.pessoa-nativo .pessoa-nome').innerText()).includes('Lia'));
        await page.evaluate(()=>{contribuicoesDosMods.revogar(handle,{id:'teste/perfis',instancia:i});desenhar(testTable.snapshot);});
        assert.equal(await page.locator('li.pessoa-apresentada[data-pessoa-id="2"]').count(),0);
        assert.ok(await page.locator('#lista-roster').innerText().then(t=>t.includes('Lia')));
        assert.deepEqual(errors,[]);
        const mobile=await browser.newPage({viewport:{width:390,height:844}});
        await mobile.addInitScript(()=>localStorage.setItem('seele.casca','celular'));
        await mobile.goto(`http://127.0.0.1:${server.address().port}`);
        await mobile.evaluate(async()=>{
          // A carga real também entrega a identidade à entrada, depois do WASM.
          // Espere essa entrega antes de instalar a fixture: a promessa tardia
          // substituía o formulário de teste no meio do fill (também no HEAD).
          // Observe só a carga já em voo; sem WASM no checkout, sua falha é
          // esperada aqui: esta prova usa identidade fixture e mede geometria.
          await identidadePrometida?.catch(() => null);
          window.entradasNaAba=0;
          await CELULAR.ajudante('entrada').usar({ler:()=>({estado:'numa-aba',endereco:'servidor-de-teste:8383',impressao:'ab'.repeat(32),versaoDoServidor:'2.0'}),entrar:async()=>{entradasNaAba++;return {erro:'sem-rede'};},copiarLink:async()=>true});
        });
        await mobile.locator('#cel-entrada-apelido').fill('Teste');
        await mobile.locator('#cel-entrada-apelido').scrollIntoViewIfNeeded();
        const campoMovel=await mobile.locator('#cel-entrada-apelido').boundingBox();
        assert.ok(campoMovel.y>=0 && campoMovel.y+campoMovel.height<=844);
        assert.equal(await mobile.locator('.cel-entrada-pe .botao:not(.botao-secundario)').count(),1);
        assert.ok(await mobile.getByText('ENTRAR NESTA ABA',{exact:true}).isVisible());
        assert.equal(await mobile.evaluate(()=>entradasNaAba),0);
        if(process.env.SEELE_PREVIEW_DIR) await mobile.screenshot({path:`${process.env.SEELE_PREVIEW_DIR}/${engine}-mobile.png`});
        await mobile.getByRole('button',{name:'ENTRAR',exact:true}).click();
        await mobile.waitForFunction(()=>entradasNaAba===1);
        assert.equal(await mobile.evaluate(()=>entradasNaAba),1);
        await mobile.close();
        console.log(`${engine}: oculto/visível, vazio/texto/multilinha, viewport CSS equivalente a zoom 100/125/200, substituição/detalhes/revogação PASS`);
        if(process.env.SEELE_PREVIEW_DIR){fs.mkdirSync(process.env.SEELE_PREVIEW_DIR,{recursive:true});await page.screenshot({path:`${process.env.SEELE_PREVIEW_DIR}/${engine}-desktop.png`});}
      } finally {await browser.close();}
    }
  } finally {server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
