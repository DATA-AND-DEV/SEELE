const repo=process.env.SEELE_REPO||require('node:path').resolve(__dirname,'../../../..');
const {CENAS,principal}=require(repo+'/apps/seele-app/bancada/navegacao-0-17.cjs');
const contatos=['Lia','Rafael','Marina'].map((nome,i)=>({codigo:`ABCD-EFGH-123${i}-AAAA`,nome,apelido:null,visto_em:null,direcao:'recebi',bloqueado:false,amigo:i===0,alvo:`@ABCD-EFGH-123${i}-AAAA`}));
const medidas=`await new Promise(ok=>setTimeout(ok,400));
const raiz=document.getElementById('perfil');
if(raiz.scrollWidth>innerWidth+1)throw Error('Overflow raiz');
if(document.getElementById('perfil-nota-do-apelido').getClientRects().length)throw Error('Microdescrição ocupa espaço');
for(const el of raiz.querySelectorAll('button,input,select,textarea')){
 if(el.disabled||!el.getClientRects().length)continue;
 el.scrollIntoView({block:'center',inline:'nearest'});el.focus({preventScroll:true});
 const r=el.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
 if(r.left<0||r.right>innerWidth+1||r.top<0||r.bottom>innerHeight+1||!(hit===el||el.contains(hit)))throw Error('Controle fora/encoberto '+el.id);
}
const fechar=document.getElementById('perfil-fechar').getBoundingClientRect();if(fechar.bottom>innerHeight||fechar.top<0)throw Error('Saída perdeu viewport');
for(const el of [raiz,...raiz.querySelectorAll('*')])if(el.scrollTop)el.scrollTop=0;
document.activeElement.blur();`;
const cenas=[];
for(const width of [320,390,768,1280])for(const height of [600,900])for(const preenchida of [false,true])cenas.push({nome:`perfil-${preenchida?'contatos':'vazio'}-${width}x${height}`,casca:'app',viewport:{width,height},resposta:{meu_codigo:'ALEX-EXMP-9999-ZZZZ',amigos:[],contatos_da_agenda:preenchida?contatos:[],estado_da_sala:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null}},passos:`abrirPerfil(); ${medidas}`});
CENAS.splice(0,CENAS.length,...cenas);
process.argv=['node','perfil',repo+'/docs/evidencias/visual-0.17.4/perfil-compacto'];
principal().catch(e=>{console.error(e);process.exitCode=1});
