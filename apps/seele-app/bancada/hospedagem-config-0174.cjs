// Composição de hospedagem e configurações 0.17.4, com a ponte nativa simulada.
// Reutiliza os contratos/CSP/cenas de navegação; não sobe servidor nem altera dados.
// PLAYWRIGHT=/caminho/playwright node este-arquivo.cjs /tmp/capturas [filtro]
const { CENAS, principal } = require('./navegacao-0-17.cjs');
const filtro = process.argv[3] || '';
const nomes = new Set([
  'app-hospedar-lista', 'app-hospedar-preparar-novo', 'app-hospedar-preparar-adotado',
  'app-hospedar-legado-mods', 'app-hospedar-copia-falhou', 'app-hospedar-nao-verificado',
  'app-config-atalhos', 'app-config-aparencia', 'app-config-identidade',
  'app-config-versoes', 'app-config-malha', 'app-config-atualizacao-achada',
]);
const cenas = CENAS.filter(c => nomes.has(c.nome));
cenas.push({nome:'app-config-porta',casca:'app',sessao:true,
  passos:'await abrirServer("tela-sessao"); abrirSecao("secao-porta");',
  resposta:{estado_da_porta:{hospedando:true,aberto:false,tem_senha:true,aceita_convites:true,portaria_ligada:true,pendentes:0,alcance:'Internet'}}},
  {nome:'app-config-servidor',casca:'app',sessao:true,passos:'await abrirServer("tela-sessao"); abrirSecao("secao-servidor");'});
const montagem = [];
for (const width of [1280,390]) for (const cena of cenas.filter(c => c.nome.includes(filtro))) {
  const raiz = cena.nome.startsWith('app-config') ? '#tela-server'
    : cena.nome.includes('preparar') ? '#tela-preparar'
      : cena.nome.endsWith('lista') ? '#tela-servidores' : '#tela-antes-de-hospedar';
  const inicio = cena.passos.includes(';') ? cena.passos.replace(/; hospedar\(\)/g,'; await hospedar()') : 'await ' + cena.passos;
  const conferir = `
    await new Promise(ok => setTimeout(ok, 300));
    const alvo = document.querySelector(${JSON.stringify(raiz)});
    if (!alvo || alvo.hidden) throw Error('Cena não abriu a superfície esperada');
    if (alvo.scrollWidth > alvo.clientWidth + 2) throw Error('Rolagem horizontal na superfície: ' + alvo.scrollWidth + '/' + alvo.clientWidth);
    const barra = alvo.querySelector('.area-barra');
    if (barra) {
      const r = barra.getBoundingClientRect();
      if (r.bottom > innerHeight + 2 || r.top < 0) throw Error('Barra de decisão fora da janela');
      for (const botao of barra.querySelectorAll('button')) {
        if (!botao.getClientRects().length) continue;
        const b = botao.getBoundingClientRect();
        if (b.bottom > innerHeight + 2 || b.right > innerWidth + 2) throw Error('Ação fora da janela: '+botao.id+' '+JSON.stringify({top:b.top,bottom:b.bottom,right:b.right}));
      }
    }
    ${/copia-falhou|nao-verificado/.test(cena.nome) ? `
      const botaoFinal = document.getElementById('antes-hospedar');
      if (!botaoFinal.disabled && botaoFinal.getAttribute('aria-disabled') !== 'true') throw Error('A recusa de segurança deixou de bloquear');
      if (document.getElementById('antes-hospedar-motivo').hidden) throw Error('Motivo do bloqueio ficou escondido');
    ` : ''}
  `;
  montagem.push({...cena,nome:cena.nome+'-'+width,viewport:{width,height:900},passos:inicio+';'+conferir,espera:100});
}
CENAS.splice(0,CENAS.length,...montagem);
// O filtro já foi aplicado antes de duplicar as larguras.
process.argv = [process.argv[0],process.argv[1],process.argv[2] || '/tmp/seele-hospedagem-config-0174'];
principal().catch(erro=>{console.error(erro);process.exitCode=1;});
