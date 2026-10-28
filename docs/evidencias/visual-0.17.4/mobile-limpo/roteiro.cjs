// Renderização da casca celular real, com IPC/dados simulados. Sem pares,
// servidor SEELE, microfone ou captura de tela. Reusa navegação-0-17.cjs.
const path=require('node:path'),fs=require('node:fs');
const raiz=path.resolve(__dirname,'../../../..');
const nav=require(path.join(raiz,'apps/seele-app/bancada/navegacao-0-17.cjs'));
const modelos=[['inicio','app-entrada'],['perfil','web-celular-perfil'],['sessao','web-celular-sessao'],['chamada','web-celular-chamada']];
const auditoria=String.raw`
await new Promise(r=>setTimeout(r,100));
if(document.documentElement.dataset.casca!=='celular')throw Error('não é a casca celular');
if(document.querySelector('.telemetria,.cel-telemetria,#cel-secao-conexao,#chamada-duracao,#cel-avisos-faixa'))throw Error('métricas/badge residual no DOM mobile');
if(document.querySelector('.estado-sem-sessao'))throw Error('rodapé fictício');
if(/USE FONE|Use fones para evitar eco|Quem tem seu código pode|Escolher pelo nome fixa|ATRASO —|JITTER —/.test(document.body.innerText))throw Error('dica padrão residual');
if(document.documentElement.scrollWidth>innerWidth+1)throw Error('transbordamento horizontal');
const interativos=[...document.querySelectorAll('button,a,input,textarea,select')].filter(e=>!e.closest('[hidden],[inert]')&&e.getClientRects().length&&e.getBoundingClientRect().width>0);
const nome=e=>e.getAttribute('aria-label')||e.getAttribute('aria-labelledby')?.split(/\s+/).map(id=>document.getElementById(id)?.textContent||'').join(' ')||[...(e.labels||[])].map(l=>l.textContent).join('')||e.textContent.trim()||e.getAttribute('placeholder')||e.title||'';
const semNome=interativos.filter(e=>!nome(e).trim());if(semNome.length)throw Error('controle sem nome acessível: '+semNome.map(e=>e.outerHTML.slice(0,240)).join(' | '));
`;
const cenas=modelos.map(([nome,origem])=>{const modelo=nav.CENAS.find(c=>c.nome===origem);if(!modelo)throw Error(origem);return{...modelo,nome,casca:'celular',viewport:{width:390,height:844},toque:true,resposta:{...(modelo.resposta||{}),apelido_local:'Alex',meu_codigo:'CSM7-NBFR-CAYQ-EV4Y',estado_da_sala:{estado:'NoAr',registro:{registro:'Registrado'},firewall:null},pessoal_identidade:{publica:Array(32).fill(1),receber:false},amigos:[],contatos_da_agenda:[],conhecidos:[]},passos:`${modelo.passos?'await ('+modelo.passos+');':''}\n${auditoria}`};});
nav.CENAS.splice(0,nav.CENAS.length,...cenas);
process.argv=['node',__filename,__dirname];
nav.principal().then(()=>{const arquivo=path.join(__dirname,'relatorio.json');const linhas=JSON.parse(fs.readFileSync(arquivo));if(linhas.some(r=>r.casca!=='celular'||r.erros.length))throw Error('captura inválida');fs.writeFileSync(path.join(__dirname,'verificacao.json'),JSON.stringify({tipo:'UI real com IPC e dados simulados',viewport:'390x844',casca:'celular',semConexaoComPares:true,semCapturaFisica:true,verificacoes:['casca celular efetiva','nenhuma métrica/uptime/badge no DOM','nenhum rodapé fictício','nenhuma dica padrão listada','sem transbordamento horizontal','controles visíveis com nomes acessíveis'],cenas:linhas.map(r=>({nome:r.cena,passou:true,erros:r.erros}))},null,2));}).catch(e=>{console.error(e);process.exitCode=1;});
