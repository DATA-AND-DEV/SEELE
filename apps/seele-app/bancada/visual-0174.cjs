// UI do produto com transporte simulado, sem áudio ou rede externa.
// PLAYWRIGHT=/caminho/playwright node apps/seele-app/bancada/visual-0174.cjs /tmp/capturas
const fs = require('node:fs');
const path = require('node:path');
const {servir, respostas, retrato} = require('./telas.cjs');
async function main() {
  if (process.argv.includes('--extras')) return extras();
  const pasta = process.argv[2] || '/tmp/seele-visual-0174'; fs.mkdirSync(pasta,{recursive:true});
  const servidor = servir(); await new Promise(ok=>servidor.listen(0,'127.0.0.1',ok));
  const browser = await require('./playwright.cjs').chromium().launch({headless:true});
  const relatorio=[];
  try {
    for (const largura of [320,390,768,1280,1920]) {
      const page = await browser.newPage({viewport:{width:largura,height:900},reducedMotion:'reduce'});
      const erros=[]; page.on('pageerror',e=>erros.push(e.stack));
      const tabela=respostas({microfones:[{id:'mic',name:'Microfone interno',is_default:true}],saidas:[{id:'out',name:'Saída padrão',is_default:true}],meu_codigo:'ALEX-EXMP-9999-ZZZZ',amigos:[],contatos_da_agenda:[],estado_da_sala:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}});
      tabela.snapshot = retrato();
      await page.addInitScript(tabela=>{
        window.__TAURI__={core:{invoke:async(cmd)=>Object.hasOwn(tabela,cmd)?structuredClone(tabela[cmd]):null},event:{listen:async()=>()=>{}},window:{getCurrentWindow:()=>({onCloseRequested:()=>{},isMaximized:async()=>false})}};
      },tabela);
      await page.goto(`http://127.0.0.1:${servidor.address().port}/`); await page.waitForTimeout(700);
      await page.evaluate(() => {
        const fisico = estadoDaSala;
        document.dispatchEvent(new CustomEvent('seele:estado-pessoal-direto', {detail:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}}));
        if (estadoDaSala !== fisico) throw new Error('Atendimento direto sobrescreveu o estado físico');
        if (document.getElementById('boot-codigo').dataset.recebe !== 'sim') throw new Error('Código direto registrado não ficou disponível');
      });
      for (const cena of ['inicio','perfil','audio','sessao','chamada','pessoal']) {
        await page.evaluate(({cena,foto})=>{
          if(cena==='perfil') abrirPerfil();
          if(cena==='audio'){fecharPerfil();abrirServer('tela-boot');document.getElementById('secao-audio').click();}
          if(cena==='sessao') { document.getElementById('tela-server').hidden=true; for(const el of document.querySelectorAll('.tela'))el.hidden=true; document.getElementById('tela-sessao').hidden=false; window.desenhar(foto); }
          if(cena==='chamada') abrirChamada();
          if(cena==='pessoal') { const vista=criarVistaDaConversaPessoal({acoes:{voltar(){},criar(){},chamar(){},atender(){},recusar(){},copiar(){},microfone(){},compartilhar(){},encerrar(){},podeCompartilhar:()=>false,enviarTexto(){}}}); vista.elemento.id='vista-pessoal-bancada'; document.body.append(vista.elemento); vista.abrir({apelido:'Alex'}); }
        },{cena,foto:retrato()}).catch(e=>erros.push(cena+':'+e.message));
        await page.waitForTimeout(200);
        const raiz={inicio:'#tela-boot',perfil:'#perfil',audio:'#tela-server',sessao:'#tela-sessao',chamada:'#tela-sessao',pessoal:'#vista-pessoal-bancada'}[cena];
        const medidas=await page.locator(raiz).evaluate(el=>({largura:el.clientWidth,conteudo:el.scrollWidth,fundo:getComputedStyle(el).backgroundColor,rolagens:[...el.querySelectorAll('*')].filter(n=>n.clientWidth>0&&n.scrollWidth>n.clientWidth+2&&getComputedStyle(n).overflowX!=='auto'&&getComputedStyle(n).overflowX!=='scroll').slice(0,8).map(n=>n.id||n.className)}));
        await page.screenshot({path:path.join(pasta,`${cena}-${largura}.png`)});
        relatorio.push({cena,largura,...medidas,erros:[...erros]});
      }
      await page.close();
    }
  } finally {await browser.close();servidor.close();}
  fs.writeFileSync(path.join(pasta,'medidas.json'),JSON.stringify(relatorio,null,2));
  console.log(JSON.stringify(relatorio.map(({cena,largura,conteudo,rolagens,erros})=>({cena,largura,conteudo,rolagens,erros})),null,2));
  if(relatorio.some(r=>r.conteudo>r.largura+2||r.erros.length))process.exitCode=1;
}
async function extras() {
  const {CENAS,principal}=require('./navegacao-0-17.cjs');
  const nomes=['app-auth-aperto','app-auth-espera','app-porta','app-portaria-da-entrada','app-sessao-moderar','app-sessao-ajuda','app-sessao-queda','app-sessao-fim','web-celular-auth-aperto','web-celular-auth-espera','web-celular-consentimento-mods'];
  const originais=CENAS.filter(c=>nomes.includes(c.nome));
  originais.push({nome:'app-cargos-editor',casca:'app',sessao:true,espera:500,
    resposta:{pedir_cargos:{quadro:{revisao:1,combinacao:'restringe_vence',proprietario:1,eu:{pessoa:1,proprietario:true},
      cargos:[{id:0,nome:'@everyone',posicao:0,todos:true,concede:['send_message']},{id:1,nome:'Membro',posicao:1,concede:['send_message','speak']},{id:2,nome:'Moderador',posicao:2,cor:'azul',concede:['kick']}],atribuicoes:[{pessoa:1,cargos:[2]}],apelidos:[]}}},
    passos:`abrirCargos(); await new Promise(ok=>setTimeout(ok,500)); document.querySelector('#cargos-lista [data-cargo="2"]').click();`});
  const cenas=originais.flatMap(c=>[1280,390].map(width=>{
    const seletor=c.nome.includes('cargos')?'#cargos':c.nome.includes('consentimento')?'#mods-aceite':c.nome.includes('portaria-da-entrada')?'#portaria':c.nome.endsWith('-porta')?'#porta':c.nome.includes('moderar')?'#moderar':c.nome.includes('ajuda')?'#ajuda':c.nome.includes('queda')?'#bateria':c.nome.includes('fim')?'#tela-fim':'#tela-auth';
    const medir=`await new Promise(ok=>setTimeout(ok,300));
      let raiz=document.querySelector(${JSON.stringify(seletor)});
      if (raiz.hidden && ${JSON.stringify(c.nome)}.includes('auth-aperto')) raiz=document.getElementById('tela-sessao');
      if (!raiz || raiz.hidden) throw new Error('Cena sem raiz visível');
      if (raiz.scrollWidth>raiz.clientWidth+2) throw new Error('Transbordamento horizontal no fluxo');
      for (const el of raiz.querySelectorAll('button,input,select,textarea,a[href]')) {
        if(el.disabled || !el.getClientRects().length)continue;
        el.scrollIntoView({block:'center',inline:'nearest'});el.focus({preventScroll:true});
        const r=el.getBoundingClientRect(),no=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
        if(r.left<0 || r.right>innerWidth+1 || r.top<0 || r.bottom>innerHeight+1 || !(no===el || el.contains(no))) throw new Error('Controle encoberto: '+(el.id||el.textContent));
      }
      for(const el of [raiz,...raiz.querySelectorAll('*')]) if(el.scrollTop)el.scrollTop=0;`;
    return {...c,nome:`extra-${c.nome}-${width}`,viewport:{width,height:900},toque:c.casca==='celular',passos:`${c.passos}; ${medir}`};
  }));
  CENAS.splice(0,CENAS.length,...cenas);
  const pasta=process.argv[2]||'/tmp/seele-visual-0174';
  process.argv=['node','visual-0174.cjs',pasta];
  await principal();
}
main().catch(e=>{console.error(e);process.exitCode=1;});
