const path=require('path');
const raiz=path.resolve(__dirname,'../../../..');
const pasta=process.argv[2]||path.join(require('os').tmpdir(),'seele-visual-0174-low');
const bench=require(path.join(raiz,'apps/seele-app/bancada/navegacao-0-17.cjs'));
const normal=bench.CENAS.find(c=>c.nome==='app-entrada');
const casos=[];
const checar=(seletores)=>`await new Promise(ok=>setTimeout(ok,200));
 for(const seletor of ${JSON.stringify(seletores)}) {
  const el=[...document.querySelectorAll(seletor)].find(n=>n.getClientRects().length && getComputedStyle(n).visibility!=='hidden');
  if(!el)throw new Error('Ação não visível: '+seletor);
  el.scrollIntoView({block:'center',inline:'nearest'});
  const r=el.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
  if(r.left<0||r.right>innerWidth+1||r.top<0||r.bottom>innerHeight+1||!(hit===el||el.contains(hit)))throw new Error('Ação cortada/encoberta: '+seletor+' '+JSON.stringify({left:r.left,right:r.right,top:r.top,bottom:r.bottom,hit:hit?.id||hit?.className}));
 }
 if(document.documentElement.scrollWidth>innerWidth+1)throw new Error('Overflow horizontal');`;
for(const width of [1280,1920,390])casos.push({...normal,nome:`inicio-${width}`,viewport:{width,height:900},resposta:{amigos:[],contatos_da_agenda:[],meu_codigo:'ALEX-EXMP-9999-ZZZZ',estado_da_sala:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}},passos:`document.dispatchEvent(new CustomEvent('seele:estado-pessoal-direto',{detail:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}})); if(!document.getElementById('boot-sala-estado').hidden)throw Error('Hint duplicado');`});
for(const width of [320,390,768]){
 const home={...normal,nome:`baixa-inicio-${width}`,viewport:{width,height:600},resposta:{amigos:[],contatos_da_agenda:[],meu_codigo:'ALEX-EXMP-9999-ZZZZ',estado_da_sala:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}},passos:checar(['#boot-conversa-pessoal','#botao-conectar','#boot-copiar-codigo','#boot-amigo-adicionar','#botao-hospedar','#boot-perfil','#botao-server'])};casos.push(home);
 for(const nome of ['app-chamada','app-hospedar-preparar-novo','app-hospedar-legado-mods-resolvidos']){
  const cena=bench.CENAS.find(c=>c.nome===nome);if(!cena)throw Error('Cena não existe '+nome);
  const alvos=nome==='app-chamada'?['.chamada-acoes-estreitas button:first-child,#botao-mudo','.chamada-acoes-estreitas button:nth-child(2),#botao-surdo','.chamada-acoes-estreitas button:last-child,#operador-sair']:nome.includes('preparar')?['#preparar-confirmar','#preparar-voltar']:['#antes-hospedar','#antes-voltar'];
  casos.push({...cena,nome:`baixa-${nome}-${width}`,viewport:{width,height:600},passos:`${cena.passos}; ${checar(alvos)}`});
 }
 const pessoal=`window.__vistaBaixa=criarVistaDaConversaPessoal({acoes:{voltar(){},criar(){},chamar(){},atender(){},recusar(){},copiar(){},microfone(){},compartilhar(){},encerrar(){},podeCompartilhar:()=>false,enviarTexto(){}}});__vistaBaixa.elemento.id='pessoal-baixa';document.body.append(__vistaBaixa.elemento);__vistaBaixa.abrir({apelido:'Pessoa local'});__vistaBaixa.estado({estado:'dentro',apelido:'Outra pessoa'});__vistaBaixa.texto({id:1,de:'par',texto:'Mensagem de prova do compositor.'});`;
 casos.push({...normal,nome:`baixa-pessoal-${width}`,viewport:{width,height:600},passos:pessoal+checar(['#pessoal-baixa .pessoal-web-rodape button:first-child','#pessoal-baixa .pessoal-web-rodape button:last-child','#pessoal-baixa textarea','#pessoal-baixa .pessoal-web-escrever button'])});
}
const filtrados=process.env.SEELE_LOW_SO?casos.filter(c=>c.nome.includes(process.env.SEELE_LOW_SO)):casos;bench.CENAS.splice(0,bench.CENAS.length,...filtrados);
process.argv=['node','low',pasta];bench.principal();
