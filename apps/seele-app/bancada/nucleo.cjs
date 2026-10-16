// O núcleo de confiança no desktop (rascunho da API 6, §10; tarefa 9 da F3),
// com o HTML, o CSS e o tema reais; só a ponte nativa é simulada. No molde de
// `capa.cjs`.
//
// # O que ela mede
//
// 1. **O tema não alcança os controles do núcleo** (Steps 1 e 2): o MUDO
//    (desligado e ligado), o fone, o SAIR, o «no ar» e o veredito têm a mesma
//    cor, o mesmo fundo efetivo e a mesma borda com e sem o tema do servidor —
//    inclusive sob o tema hostil, de acento igual ao fundo, que a API 6
//    recusa pelos pares e que esta bancada aplica por baixo dela, como a do
//    celular (`tools/bancada-do-celular.py`), para ver que o núcleo aguenta
//    mesmo assim. O fundo efetivo é o primeiro opaco atrás da peça: um MUDO
//    transparente sobre a barra mostraria o fundo do tema. **E com os tokens
//    por região no `.operador`** (tarefa 17i, item 3; pedido da orquestradora,
//    10/10): os três temas escritos como a região `operador.barra`, que pinta
//    o contêiner onde o MUDO, o fone e o SAIR moram — sozinhos e por cima do
//    tema da sessão.
// 2. **O guarda 11** (Step 3), um por item: a portaria, o MUDO, o fone, o «no
//    ar», o SAIR, o veredito, a moderação, o aceite, e a atribuição e a saída
//    de uma superfície aberta, e **a faixa de quem liga** (`#toque`, com o
//    ATENDER e o RECUSAR; a 7d, tarefa 26 da F7; Step 5), acesa por um `Tocou`
//    de `seele://toque` com quem usa dentro de um servidor, como a F7 a
//    desenha. (a) Nenhum mora dentro do contêiner de um ponto
//    que monta nós de MOD (`CONTEINERES_DOS_PONTOS`); os dois que só pintam
//    ficam de fora, com o motivo: `servidor.aparencia` é o tema, medido no 1,
//    e `pessoa.capa` é uma imagem atrás das placas (tarefa 6). (b) Com a capa
//    de quem usa e uma contribuição em `operador.barra` montadas, nada cobre
//    os que estão à vista: no centro de cada um, quem recebe o clique é ele.
//    Numa bancada, e não em texto no `frontend.rs`: os contêineres são
//    seletores sobre o DOM, e alguns só existem desenhados.
// 3. **O modal de MOD não cobre a barra do operador** (Step 4b; decisão da
//    orquestradora, 09/10: o recorte). A 1400 e a 900 de largura, com um
//    diálogo modal alto e largo aberto: o MUDO, o fone e o SAIR ficam
//    acordados e no furo do véu — a 900, com a gaveta fechada, a porta dela, e
//    aberta, o MUDO —; o resto da sessão continua inerte e coberto; o Tab fica
//    no diálogo, e o Esc o fecha; e o diálogo nunca cruza a barra, rolando por
//    dentro. **E a faixa de quem liga** (Step 5): o ATENDER e o RECUSAR
//    acordados e alcançáveis, com a ligação chegando depois do modal (a 1400)
//    e com ela já tocando quando ele abre (a 900), e o diálogo sem cruzá-la.
//
// Rodar:  node apps/seele-app/bancada/nucleo.cjs

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

// Os temas de teste da bancada do celular (F8): PAPEL e o claro passam pelos
// pares; o hostil, não, e é aplicado por baixo da validação.
const TEMAS = {
  papel: { fundo: '#12100e', painel: '#1b1815', texto: '#f2ece0', apagado: '#a2988a', acento: '#c9a227', borda: '#4a423a' },
  claro: { fundo: '#f2ece0', painel: '#e6dfcf', texto: '#12100e', apagado: '#574e43', acento: '#a83a10', borda: '#b8ad9a' },
  hostil: { fundo: '#050403', painel: '#0a0806', texto: '#eae3cf', apagado: '#908574', acento: '#050403', borda: '#241f19' },
};
// **E os acentos que o texto nomeia** (`tema.nucleo`: «um guarda prova que
// acento #331704, #050403 e #0a0806 não mudam…»; a auditoria do v6, tabela 8):
// os temas de cima são os do celular, e dois dos três nunca eram acento. Lidos
// do texto, cada um sobre o hostil, por baixo da validação como ele.
const ACENTOS_DO_TEXTO = [...JSON.parse(fs.readFileSync(path.join(__dirname, '../../../api/v6.json'), 'utf8'))
  .tema.nucleo.matchAll(/#[0-9a-f]{6}/gi)].map((m) => m[0].toLowerCase());
assert.equal(ACENTOS_DO_TEXTO.length, 3, `o tema.nucleo do v6.json não nomeia mais três acentos: ${ACENTOS_DO_TEXTO}`);
for (const acento of ACENTOS_DO_TEXTO) TEMAS[`acento ${acento}`] = { ...TEMAS.hostil, acento };

(async () => {
  const server = servir();
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(table => {
      window.testTable = table;
      window.__TAURI__ = {
        core: { invoke: async (cmd) => testTable[cmd] ?? null },
        // Os ouvintes ficam guardados: a faixa do toque só acende por um
        // evento de `seele://toque`, e a seção 2 o entrega.
        event: { listen: async (nome, fn) => {
          (window.__ouvintes ??= {})[nome] = [...(window.__ouvintes[nome] ?? []), fn];
          return () => {};
        } },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      // O veredito de um primeiro contato, para a faixa estar à vista.
      mostrarVeredito({ FirstContact: { fingerprint: 'SHA256:aa:bb:cc' } });
    }, retrato());

    // As peças do núcleo, e a cor de cada uma: o texto, a borda e o fundo
    // efetivo. O MUDO é medido desligado e ligado.
    const medir = () => page.evaluate(() => {
      const fundoEfetivo = (el) => {
        for (let n = el; n; n = n.parentElement) {
          const c = getComputedStyle(n).backgroundColor;
          if (!/rgba\(0, 0, 0, 0\)|transparent/.test(c)) return c;
        }
        return 'nenhum';
      };
      const peca = (el) => {
        if (!el || el.closest('[hidden]') || !el.getClientRects().length) return null;
        const s = getComputedStyle(el);
        return { cor: s.color, borda: s.borderTopColor, fundo: fundoEfetivo(el) };
      };
      const mudo = document.getElementById('botao-mudo');
      const saida = {
        mudo: peca(mudo),
        fone: peca(document.getElementById('botao-surdo')),
        sair: peca(document.getElementById('operador-sair')),
        noAr: peca(document.getElementById('voz-estado')),
        veredito: peca(document.getElementById('veredito')),
      };
      const antes = mudo.dataset.ativo;
      mudo.dataset.ativo = 'sim';
      saida.mudoLigado = peca(mudo);
      if (antes === undefined) delete mudo.dataset.ativo; else mudo.dataset.ativo = antes;
      return saida;
    });

    const semTema = await medir();
    for (const [peca, medida] of Object.entries(semTema)) {
      assert.ok(medida, `a peça do núcleo «${peca}» não está à vista, e o caso não a mede`);
    }
    const diferencas = [];
    for (const [nome, valores] of Object.entries(TEMAS)) {
      const comTema = await page.evaluate((valores) => {
        temaDosMods.set('bancada', valores);
        escreverOTemaDaSessao();
      }, valores).then(medir);
      for (const [peca, medida] of Object.entries(semTema)) {
        for (const [parte, cor] of Object.entries(medida)) {
          if (comTema[peca]?.[parte] !== cor) diferencas.push(`${nome}: ${peca}.${parte} ${cor} → ${comTema[peca]?.[parte]}`);
        }
      }
      await page.evaluate(() => { temaDosMods.delete('bancada'); escreverOTemaDaSessao(); });
    }
    // Os mesmos três, como a região que mora em volta do núcleo: sozinha, e por
    // cima do tema da sessão.
    const daRegiao = ({ fundo, texto, apagado, acento, borda }) => ({ fundo, texto, apagado, acento, borda });
    for (const [nome, valores] of Object.entries(TEMAS)) {
      for (const [como, tema] of [
        ['só a região', { regioes: { 'operador.barra': daRegiao(valores) } }],
        ['a região sobre o tema', { ...TEMAS.papel, regioes: { 'operador.barra': daRegiao(valores) } }],
      ]) {
        const comRegiao = await page.evaluate((tema) => {
          temaDosMods.set('bancada', tema);
          escreverOTemaDaSessao();
          return document.querySelector('.operador').style.getPropertyValue('--seele-negro-painel');
        }, tema);
        assert.ok(comRegiao, `${nome}, ${como}: a região não pintou o contêiner do operador, e o caso não mede nada`);
        const medida = await medir();
        for (const [peca, semTemaDaPeca] of Object.entries(semTema)) {
          for (const [parte, cor] of Object.entries(semTemaDaPeca)) {
            if (medida[peca]?.[parte] !== cor) diferencas.push(`${nome}, ${como}: ${peca}.${parte} ${cor} → ${medida[peca]?.[parte]}`);
          }
        }
        await page.evaluate(() => { temaDosMods.delete('bancada'); escreverOTemaDaSessao(); });
      }
    }
    assert.deepEqual(diferencas, [], 'o tema alcançou um controle do núcleo');

    // As aberturas da seção 2b moram na página, e os temas também.
    await page.evaluate(({ temas }) => {
      window.__temasDoNucleo = temas;
      window.__escondidos = [
        ['portaria', () => avisarQueBatem(1), null, () => avisarQueBatem(0)],
        ['moderação', () => abrirModeracao(), null, () => fecharModeracao()],
        ['aceite', () => abrirAceiteDeMods('127.0.0.1:8384', [{ id: 'teste/mesa', nome: 'MESA', hash: 'h', versao: '1.0.0', api: 6 }], 'c1', null), null, () => fecharAceiteDeMods()],
        ['sala pessoal', async () => {
          testTable.meu_codigo = 'K9P2-CSM7-NBFR-AYQ3';
          await abrirPerfil();
          await new Promise((ok) => setTimeout(ok, 200));
        }, null, () => fecharPerfil()],
      ];
    }, { temas: { claro: TEMAS.claro, hostil: TEMAS.hostil } });

    // 2. O guarda 11.
    const guarda = await page.evaluate(async () => {
      // A capa de quem usa e uma contribuição alta na barra do operador, e uma
      // superfície aberta, por um MOD de API 6 de pé.
      entrarNaGeracao(1);
      const mod = { id: 'teste/nucleo', hash: 'h', api: 6 };
      const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, { pedirEncerramento() {}, async encerrou() { return true; }, entregar() {} });
      inst.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set(mod.id, inst);
      const tela = document.createElement('canvas');
      tela.width = 64; tela.height = 32;
      tela.getContext('2d').fillRect(0, 0, 64, 32);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: tela.toDataURL() };
      contribuicoesDosMods.registrar(mod, inst, { ponto: 'pessoa.capa', modo: 'substituir', alvo: String(testTable.snapshot.me),
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:capa' } } } });
      const linha = { forma: 'texto', chave: 't', dentro: 'uma linha do MOD na barra, longa o bastante para empurrar o que vier depois' };
      const primeira = contribuicoesDosMods.registrar(mod, inst, { ponto: 'operador.barra', modo: 'adicionar', conteudo: [linha] });
      await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 1, descricao: { id: 's', tipo: 'dialogo', modal: false, titulo: 'Superfície' } });
      desenhar(testTable.snapshot);
      await new Promise((ok) => setTimeout(ok, 300));
      // **A faixa de quem liga**, acesa como o Rust a acende: um `Tocou` por
      // `seele://toque`, com quem usa dentro de um servidor — o caso que
      // interessa, porque é onde as regiões de MOD existem (o ATENDER vira
      // «ATENDER (SAI DE …)»).
      testTable.onde_estou = 'Guardado';
      for (const ouvir of window.__ouvintes?.['seele://toque'] ?? []) {
        ouvir({ payload: { evento: 'Tocou', quem: 'aa11bb22', codigo: 'K9P2-CSM7-NBFR-AYQ3', apelido: 'rafa',
          nome_na_agenda: null, com_som: true } });
      }
      await new Promise((ok) => setTimeout(ok, 100));
      // **O desenho do MOD mira cada item** (o molde de `regioes-do-produto.cjs`):
      // `estilo.mover` vai a ±512 px, e um nó vai do canto da região até o
      // centro de cada item à vista.
      const canto = document.getElementById('operador-mods').getBoundingClientRect();
      const limite = (n) => Math.max(-512, Math.min(512, Math.round(n)));
      const mirados = ['#botao-mudo', '#botao-surdo', '#voz-estado', '#operador-sair', '#veredito',
        '#toque-atender', '#toque-recusar'].map((seletor, i) => {
        const centro = document.querySelector(seletor).getBoundingClientRect();
        return { forma: 'texto', chave: `mirado${i}`, dentro: '█'.repeat(6), estilo: { mover: {
          x: limite(centro.left + centro.width / 2 - canto.left - 8), y: limite(centro.top + centro.height / 2 - canto.top - 8) } } };
      });
      contribuicoesDosMods.revogar(primeira.handle, { id: mod.id, instancia: inst });
      contribuicoesDosMods.registrar(mod, inst, { ponto: 'operador.barra', modo: 'adicionar', conteudo: [...mirados, linha] });
      desenhar(testTable.snapshot);
      await new Promise((ok) => setTimeout(ok, 300));

      // Os que só pintam: o tema e a capa (ver o cabeçalho).
      const SO_PINTAM = new Set(['servidor.aparencia', 'pessoa.capa']);
      const conteineres = Object.entries(CONTEINERES_DOS_PONTOS).filter(([ponto]) => !SO_PINTAM.has(ponto));
      const ITENS = {
        portaria: '#portaria-batendo', mudo: '#botao-mudo', fone: '#botao-surdo', noAr: '#voz-estado',
        sair: '#operador-sair', veredito: '#veredito', moderacao: '#moderar', aceite: '#mods-aceite',
        atribuicao: '[data-superficie="s"] .superficie-de-mod-origem', saidaDaSuperficie: '[data-superficie="s"] .superficie-de-mod-sair',
        toque: '#toque', toqueAtender: '#toque-atender', toqueRecusar: '#toque-recusar',
      };
      const dentro = [];
      const cobertos = [];
      const ausentes = [];
      for (const [item, seletor] of Object.entries(ITENS)) {
        const el = document.querySelector(seletor);
        if (!el) { ausentes.push(item); continue; }
        for (const [ponto, conteiner] of conteineres) {
          if (el.matches(conteiner) || el.closest(conteiner)) dentro.push(`${item} dentro de ${ponto} (${conteiner})`);
        }
        if (el.closest('[hidden]') || !el.getClientRects().length) continue;
        // **O veredito é uma faixa da largura da janela**, e o centro dela é
        // fundo: o que precisa ficar livre é o que ele diz e o botão. Medido
        // em 11/10: a 1400, a faixa do toque cai sobre o meio vazio dela, com
        // o texto e o «dispensar» livres; a 1024, ela cobre o texto, e isso
        // foi à F7 (produto sobre produto, e não MOD).
        const partes = item === 'veredito'
          ? [...el.querySelectorAll('span, button')].filter((p) => p.getClientRects().length && p.textContent.trim())
          : [el];
        for (const parte of partes) {
          const r = parte.getBoundingClientRect();
          const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          if (!alvo || !(alvo === parte || parte.contains(alvo))) cobertos.push(`${item} coberto por <${alvo?.tagName?.toLowerCase()}${alvo?.id ? `#${alvo.id}` : ''}${alvo?.className ? `.${String(alvo.className).split(' ')[0]}` : ''}>`);
        }
      }
      return {
        dentro, cobertos, ausentes,
        comCapa: Boolean(document.querySelector('#tela-sessao .operador[data-com-capa]')),
        // Uma faixa escondida passaria pelo (b) sem ser medida: ela tem de
        // estar à vista, e o ATENDER tem de dizer de onde sai.
        toqueAVista: Boolean(document.getElementById('toque')?.getClientRects().length && !document.getElementById('toque').closest('[hidden]')),
        atenderDiz: document.getElementById('toque-atender')?.textContent ?? null,
        // Até onde o desenho da barra alcança: `estilo.mover` vai a ±512 px.
        toqueAoAlcance: (() => {
          const c = document.getElementById('operador-mods').getBoundingClientRect();
          const f = document.getElementById('toque-atender').getBoundingClientRect();
          return Math.abs(f.top + f.height / 2 - c.top) <= 512 && Math.abs(f.left + f.width / 2 - c.left) <= 512;
        })(),
        barraComMod: Boolean(document.querySelector('#operador-mods')?.textContent.includes('uma linha do MOD')),
        // **O cabeçalho diz de qual MOD é** (`superficies.garantias-do-produto[1]`;
        // a auditoria do v6, tabela 7): o (a) e o (b) conferiam que a atribuição
        // existe e não está coberta, e um texto vazio passava.
        atribuicaoDiz: document.querySelector('[data-superficie="s"] .superficie-de-mod-origem')?.textContent ?? null,
      };
    });
    assert.ok(guarda.comCapa && guarda.barraComMod, `o guarda não mede o que diz: a capa ou a barra do MOD não montou: ${JSON.stringify(guarda)}`);
    assert.ok(guarda.toqueAVista && /^ATENDER SAI DE /.test(guarda.atenderDiz ?? ''),
      `o guarda não mede a faixa de quem liga: ela não acendeu, ou acendeu fora de um servidor: ${JSON.stringify(guarda)}`);
    assert.deepEqual(guarda.ausentes, [], 'um item do núcleo sumiu do DOM, e o guarda não o mede');
    assert.deepEqual(guarda.dentro, [], 'um item do núcleo mora dentro do contêiner de um ponto de contribuição');
    assert.deepEqual(guarda.cobertos, [], 'uma contribuição cobre um item do núcleo');
    assert.equal(guarda.atribuicaoDiz, 'teste/nucleo', 'o cabeçalho da superfície não diz de qual MOD ela é');

    // 2b. **Os itens que a seção 2 só via escondidos** (`nucleo-de-confianca.
    //     itens[2]`, `[5]` e `[6]`; a auditoria do v6, tabela 9): a moderação, o
    //     aceite e a portaria estavam escondidos lá, e o (b) e o tema os pulavam.
    //     Aqui cada um à vista, com um MOD mirando o centro de quem fala por
    //     ele, como a seção 2 mira os outros: quem recebe o clique é o item, e as
    //     cores dele são as mesmas com e sem o tema — o claro e o hostil.
    // O que se mede de cada um; a abertura e o fechamento moram na página
    // (`__escondidos`, na mesma ordem).
    const ESCONDIDOS = [
      ['portaria', ['#portaria-batendo-texto', '#portaria-batendo-ver']],
      ['moderação', ['#moderar-titulo', '#moderar-fechar']],
      ['aceite', ['#mods-titulo', '#mods-aceitar']],
      // **A ligação, no PERFIL** (`itens[6]`): o SEU CÓDIGO, o COPIAR e o
      // «receber ligações», que a sala pessoal da F7 põe no PERFIL. (O
      // CHAMANDO ainda não existe na tela: é a tarefa 7f da F7.)
      ['sala pessoal', ['#perfil-sala-titulo', '#perfil-copiar-codigo', '#perfil-receber']],
    ];
    for (const [item, seletores] of ESCONDIDOS) {
      const visto = await page.evaluate(async ({ indice, seletores }) => {
        const [, abrir, , fechar] = window.__escondidos[indice];
        await abrir();
        await new Promise((ok) => setTimeout(ok, 200));
        const mod = { id: 'teste/mira', hash: 'h', api: 6 };
        const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, { pedirEncerramento() {}, async encerrou() { return true; }, entregar() {} });
        inst.estado = ESTADOS_DE_MOD.ativa;
        modsCarregados.set(mod.id, inst);
        const canto = document.getElementById('operador-mods').getBoundingClientRect();
        const limite = (n) => Math.max(-512, Math.min(512, Math.round(n)));
        const mirados = seletores.map((seletor, i) => {
          const r = document.querySelector(seletor).getBoundingClientRect();
          return { forma: 'texto', chave: `m${i}`, dentro: '█'.repeat(6), estilo: { mover: {
            x: limite(r.left + r.width / 2 - canto.left - 8), y: limite(r.top + r.height / 2 - canto.top - 8) } } };
        });
        const { handle } = contribuicoesDosMods.registrar(mod, inst, { ponto: 'operador.barra', modo: 'adicionar', conteudo: mirados });
        desenhar(testTable.snapshot);
        await new Promise((ok) => setTimeout(ok, 200));
        const medir = () => seletores.map((seletor) => {
          const el = document.querySelector(seletor);
          if (!el || el.closest('[hidden]') || !el.getClientRects().length) return { seletor, aVista: false };
          const r = el.getBoundingClientRect();
          const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          let fundo = 'rgba(0, 0, 0, 0)';
          for (let n = el; n && /rgba\(0, 0, 0, 0\)|transparent/.test(fundo); n = n.parentElement) fundo = getComputedStyle(n).backgroundColor;
          return { seletor, aVista: true, livre: Boolean(alvo && (alvo === el || el.contains(alvo))), cor: getComputedStyle(el).color, fundo, por: alvo && !(alvo === el || el.contains(alvo)) ? `${alvo.tagName.toLowerCase()}.${String(alvo.className).split(' ')[0]} em ${alvo.closest('[id]')?.id} (${[r.left, r.top].map(Math.round)})` : null };
        });
        const semTema = medir();
        const comTema = {};
        for (const [nome, valores] of Object.entries(window.__temasDoNucleo)) {
          temaDosMods.set('bancada', valores);
          escreverOTemaDaSessao();
          comTema[nome] = medir();
          temaDosMods.delete('bancada');
          escreverOTemaDaSessao();
        }
        contribuicoesDosMods.revogar(handle, { id: mod.id, instancia: inst });
        modsCarregados.delete(mod.id);
        await fechar();
        desenhar(testTable.snapshot);
        await new Promise((ok) => setTimeout(ok, 100));
        return { semTema, comTema };
      }, { indice: ESCONDIDOS.findIndex(([nome]) => nome === item), seletores }).catch((erro) => ({ erro: erro.message.split('\n')[0] }));
      assert.ok(!visto.erro, `${item}: ${visto.erro}`);
      for (const medida of visto.semTema) {
        assert.ok(medida.aVista, `${item}: ${medida.seletor} não ficou à vista, e o caso não o mede`);
        assert.ok(medida.livre, `${item}: um MOD cobre ${medida.seletor}: ${medida.por}`);
      }
      for (const [nome, medidas] of Object.entries(visto.comTema)) {
        for (const [i, medida] of medidas.entries()) {
          const sem = visto.semTema[i];
          assert.deepEqual({ cor: medida.cor, fundo: medida.fundo }, { cor: sem.cor, fundo: sem.fundo },
            `${item}: o tema ${nome} alcançou ${sem.seletor}`);
        }
      }
    }

    // 2c. **O dono de cada seção do «⋯»** (decisão da orquestradora, 16/10):
    //     a linha com o nome do MOD por cima de um `pessoa.detalhes`, do
    //     produto, é núcleo — sem ela, um MOD desenharia um «MODERAR» falso
    //     ao lado do verdadeiro. Na linha nativa, com um nó do MOD movido e
    //     escalado contra ela: descoberta, com as cores do produto sob todo
    //     tema da bancada, e acima de 4,5:1 (é letra de 10 px).
    const dono = await page.evaluate(async (temas) => {
      const r = testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const outra = r.voice_rooms.flatMap((sala) => sala.people).find((p) => !p.is_self);
      const mod = { id: 'teste/dono', hash: 'h', api: 6 };
      const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, { pedirEncerramento() {}, async encerrou() { return true; }, entregar() {} });
      inst.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set(mod.id, inst);
      const { handle } = contribuicoesDosMods.registrar(mod, inst, { ponto: 'pessoa.detalhes', modo: 'adicionar', alvo: String(outra.id),
        conteudo: [{ forma: 'texto', chave: 'falso', dentro: 'MODERAR', estilo: { mover: { x: 0, y: -60 }, escalar: 4 } }] });
      desenhar(r);
      await quadro();
      const detalhes = document.querySelector(`#lista-roster li.pessoa[data-pessoa-id="${outra.id}"] > details.pessoa-nativo`);
      if (detalhes) detalhes.open = true;
      await quadro();
      const origem = detalhes?.querySelector('.contribuicao-de-mod-origem');
      origem?.scrollIntoView({ block: 'center' });
      const rgb = (c) => (c.match(/[\d.]+/g) || []).slice(0, 3).map(Number);
      const luz = ([a, b, c]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
        return 0.2126 * f(a) + 0.7152 * f(b) + 0.0722 * f(c); };
      const medir = () => {
        if (!origem || !origem.getClientRects().length) return null;
        const caixa = origem.getBoundingClientRect();
        const alvo = document.elementFromPoint(caixa.left + Math.min(caixa.width, 40) / 2, caixa.top + caixa.height / 2);
        const s = getComputedStyle(origem);
        const [x, y] = [luz(rgb(s.color)), luz(rgb(s.backgroundColor))].sort((m, n) => n - m);
        return { texto: origem.textContent, livre: Boolean(alvo && origem.contains(alvo)), cor: s.color, fundo: s.backgroundColor,
          contraste: Number(((x + 0.05) / (y + 0.05)).toFixed(2)) };
      };
      const sem = medir();
      const comTemas = {};
      for (const [nome, valores] of Object.entries(temas)) {
        temaDosMods.set('bancada', valores);
        escreverOTemaDaSessao();
        comTemas[nome] = medir();
        temaDosMods.delete('bancada');
        escreverOTemaDaSessao();
      }
      contribuicoesDosMods.revogar(handle, { id: mod.id, instancia: inst });
      modsCarregados.delete(mod.id);
      desenhar(r);
      await quadro();
      return { sem, comTemas };
    }, TEMAS);
    assert.ok(dono.sem, 'o «⋯» da linha nativa não mostrou a linha de dono, e o caso não a mede');
    assert.equal(dono.sem.texto, 'teste/dono', 'a linha de dono não diz o MOD');
    assert.ok(dono.sem.livre, 'um nó do MOD, movido e escalado, cobre a linha de dono');
    assert.ok(dono.sem.contraste >= 4.5, `a linha de dono está a ${dono.sem.contraste}:1`);
    for (const [nome, medida] of Object.entries(dono.comTemas)) {
      assert.deepEqual({ cor: medida?.cor, fundo: medida?.fundo, livre: medida?.livre }, { cor: dono.sem.cor, fundo: dono.sem.fundo, livre: true },
        `sob o tema ${nome}, a linha de dono mudou ou foi coberta`);
    }

    // 3. O modal não cobre a barra.
    const abrirModal = () => page.evaluate(async () => {
      const mod = { id: 'teste/nucleo', hash: 'h', api: 6 };
      const inst = modsCarregados.get(mod.id);
      superficiesDoMod(mod, inst).soltarTudo();
      await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 2, descricao: { id: 'alto', tipo: 'dialogo', titulo: 'Alto e largo', tamanho: { largura: 1600 } } });
      await atenderOMod(mod, inst, { tipo: 'superficie-montar', n: 3, superficie: 'alto', arvore: [
        ...Array.from({ length: 80 }, (_, i) => ({ forma: 'texto', chave: `l${i}`, dentro: `linha ${i} do diálogo alto` })),
        { forma: 'campo', chave: 'a', rotulo: 'A' }, { forma: 'campo', chave: 'b', rotulo: 'B' },
      ] });
      await new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
    });
    const medirOModal = () => page.evaluate(() => {
      const alcancado = (el) => {
        if (!el || !el.getClientRects().length) return 'invisivel';
        if (el.closest('[inert]')) return 'inerte';
        const r = el.getBoundingClientRect();
        const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return alvo && (alvo === el || el.contains(alvo)) ? 'sim' : `coberto por ${alvo?.className ?? alvo?.tagName}`;
      };
      const cruza = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
      const caixa = document.querySelector('[data-superficie="alto"].dialogo-de-mod, [data-superficie="alto"] .dialogo-de-mod, .dialogo-de-mod[data-superficie="alto"]');
      const barra = document.querySelector('#tela-sessao .operador');
      const porta = document.querySelector('#tela-sessao .linha-canais');
      const corpo = caixa?.querySelector('.dialogo-de-mod-corpo');
      return {
        mudo: alcancado(document.getElementById('botao-mudo')),
        fone: alcancado(document.getElementById('botao-surdo')),
        sair: alcancado(document.getElementById('operador-sair')),
        // **A faixa de quem liga** (Step 5): uma ligação que chega com um modal
        // de MOD aberto é atendida ou recusada, e o diálogo nunca a cruza.
        atender: alcancado(document.getElementById('toque-atender')),
        recusar: alcancado(document.getElementById('toque-recusar')),
        cruzaToque: Boolean(caixa && document.getElementById('toque')?.getClientRects().length
          && cruza(caixa.getBoundingClientRect(), document.getElementById('toque').getBoundingClientRect())),
        porta: alcancado(porta),
        // **A portaria** (achado do passo 7; decisão da orquestradora, 16/10):
        // o VER QUEM É e o DEPOIS alcançáveis com o modal aberto, e o diálogo
        // nunca a cruza — como a faixa de quem liga.
        portariaVer: alcancado(document.getElementById('portaria-batendo-ver')),
        portariaDepois: alcancado(document.getElementById('portaria-batendo-depois')),
        cruzaPortaria: Boolean(caixa && document.getElementById('portaria-batendo')?.getClientRects().length
          && cruza(caixa.getBoundingClientRect(), document.getElementById('portaria-batendo').getBoundingClientRect())),
        resto: alcancado(document.querySelector('#tela-sessao textarea')),
        // **Coberto, e não só inerte**: um nó inerte não entra no teste de
        // clique, então quem recebe o clique no centro dele precisa ser o
        // modal — o véu ou a caixa —, e não o que estiver por baixo.
        //
        // **Da gaveta, a área que rola**, e não a lista de canais: com a
        // faixa de quem liga acesa, a tela desce até o pé dela (`--toque-ocupa`,
        // da versao/0.16), e a 900 × 700 a lista fica abaixo do que a área
        // mostra — cortada por ela, alcançável rolando. O centro da lista caía
        // então atrás da barra do operador, no furo do véu, e a prova media um
        // ponto que ninguém vê. A área que rola é a gaveta fora da barra, à
        // vista em toda largura.
        //
        // **E fora dos furos**: com a portaria acesa, o furo dela fica sobre o
        // compositor, e o centro dele caía no furo, onde quem está em cima é a
        // própria faixa. O ponto medido é o primeiro que nenhum recorte guarda,
        // e a lista de mensagens entra junto: a 1400, a faixa acesa fica sobre
        // o compositor inteiro.
        restoCoberto: ['#tela-sessao textarea', '#tela-sessao .canais-rolagem', '#lista-mensagens'].map((seletor) => {
          const el = document.querySelector(seletor);
          if (!el || !el.getClientRects().length) return null;
          const r = el.getBoundingClientRect();
          const furos = foraDoModal().recortes.filter((n) => n.getClientRects().length).map((n) => n.getBoundingClientRect());
          const y = r.top + Math.min(r.height / 2, 20);
          const x = [0.5, 0.1, 0.9, 0.3, 0.7].map((f) => r.left + r.width * f)
            .find((px) => !furos.some((f) => px >= f.left && px <= f.right && y >= f.top && y <= f.bottom));
          if (x === undefined) return null;
          return Boolean(document.elementFromPoint(x, y)?.closest('.camada-de-mod'));
        }).filter((v) => v !== null),
        cruzaBarra: Boolean(caixa && barra?.getClientRects().length && cruza(caixa.getBoundingClientRect(), barra.getBoundingClientRect())),
        cruzaPorta: Boolean(caixa && porta?.getClientRects().length && cruza(caixa.getBoundingClientRect(), porta.getBoundingClientRect())),
        rola: Boolean(corpo && corpo.scrollHeight > corpo.clientHeight),
        dentroDaJanela: Boolean(caixa && caixa.getBoundingClientRect().bottom <= innerHeight && caixa.getBoundingClientRect().right <= innerWidth),
      };
    });

    // A 1400, a ligação chega **depois** de o modal abrir: o furo do véu tem
    // de se refazer quando a faixa acende. (A seção 2 já a acendeu; ela é
    // apagada aqui e acesa de novo com o modal de pé.)
    const tocar = (evento) => page.evaluate((evento) => {
      for (const ouvir of window.__ouvintes?.['seele://toque'] ?? []) ouvir({ payload: evento });
      return new Promise((ok) => setTimeout(ok, 150));
    }, evento);
    await tocar({ evento: 'Parou', quem: 'aa11bb22', desfecho: 'recusada' });
    // **O foco entra na abertura e volta ao acionador** (`superficies.
    // garantias-do-produto[3]`, (a) e (c); a auditoria do v6, tabela 7): o Tab
    // de baixo passaria sem o foco de entrada, porque o primeiro Tab o puxa para
    // dentro, e nada conferia a volta. O acionador é a lupa do cabeçalho do
    // canal, do produto, que o modal adormece.
    await page.focus('#botao-buscar');
    // A portaria acesa **antes** do modal, a 1400; a 900, depois dele.
    await page.evaluate(() => avisarQueBatem(1));
    await abrirModal();
    const focoNaAbertura = await page.evaluate(() => Boolean(document.activeElement?.closest('[data-superficie="alto"]')));
    assert.ok(focoNaAbertura, 'o diálogo modal abriu, e o foco ficou fora dele até a primeira tecla');
    await tocar({ evento: 'Tocou', quem: 'aa11bb22', codigo: 'K9P2-CSM7-NBFR-AYQ3', apelido: 'rafa', nome_na_agenda: null, com_som: true });
    const largo = await medirOModal();
    assert.deepEqual({ ver: largo.portariaVer, depois: largo.portariaDepois, cruza: largo.cruzaPortaria },
      { ver: 'sim', depois: 'sim', cruza: false },
      `a 1400, com o modal aberto, a portaria não está alcançável, ou o diálogo a cruza: ${JSON.stringify(largo)}`);
    assert.deepEqual({ atender: largo.atender, recusar: largo.recusar, cruzaToque: largo.cruzaToque },
      { atender: 'sim', recusar: 'sim', cruzaToque: false },
      `a 1400, uma ligação que chega com o modal aberto não é atendida nem recusada, ou o diálogo a cruza: ${JSON.stringify(largo)}`);
    assert.deepEqual({ mudo: largo.mudo, fone: largo.fone, sair: largo.sair }, { mudo: 'sim', fone: 'sim', sair: 'sim' },
      `a 1400, com o modal aberto, a barra do operador não está alcançável: ${JSON.stringify(largo)}`);
    assert.equal(largo.resto, 'inerte', `a 1400, o resto da sessão escapou do modal: ${JSON.stringify(largo)}`);
    assert.ok(largo.restoCoberto.length >= 2 && largo.restoCoberto.every(Boolean), `a 1400, o resto da sessão não ficou sob o modal: ${JSON.stringify(largo)}`);
    assert.deepEqual({ cruzaBarra: largo.cruzaBarra, rola: largo.rola, dentroDaJanela: largo.dentroDaJanela }, { cruzaBarra: false, rola: true, dentroDaJanela: true },
      `a 1400, o diálogo alto cruza a barra, não rola por dentro, ou sai da janela: ${JSON.stringify(largo)}`);
    // O Tab fica no diálogo, e o Esc o fecha.
    const dentroDoAlto = () => page.evaluate(() => Boolean(document.activeElement?.closest('[data-superficie="alto"]')));
    const tabs = [];
    for (let i = 0; i < 6; i += 1) {
      await page.keyboard.press('Tab');
      tabs.push(await dentroDoAlto());
    }
    assert.deepEqual(tabs, [true, true, true, true, true, true], 'um Tab levou o foco para fora do diálogo modal');
    await page.keyboard.press('Escape');
    assert.equal(await page.evaluate(() => Boolean(document.querySelector('[data-superficie="alto"]')?.isConnected)), false, 'o Esc não fechou o diálogo modal');
    assert.equal(await page.evaluate(() => document.activeElement?.id ?? document.activeElement?.tagName), 'botao-buscar',
      'fechado o diálogo modal, o foco não voltou a quem o tinha antes');

    // A 900, a coluna é gaveta: fechada, a porta dela fica alcançável; aberta,
    // o MUDO.
    await page.setViewportSize({ width: 900, height: 700 });
    // A 900, a ligação já toca quando o modal abre; a portaria acende depois.
    await page.evaluate(() => avisarQueBatem(0));
    await abrirModal();
    await page.evaluate(() => avisarQueBatem(2));
    await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const estreito = await medirOModal();
    assert.deepEqual({ ver: estreito.portariaVer, depois: estreito.portariaDepois, cruza: estreito.cruzaPortaria },
      { ver: 'sim', depois: 'sim', cruza: false },
      `a 900, a portaria que acende com o modal aberto não está alcançável, ou o diálogo a cruza: ${JSON.stringify(estreito)}`);
    assert.deepEqual({ atender: estreito.atender, recusar: estreito.recusar, cruzaToque: estreito.cruzaToque },
      { atender: 'sim', recusar: 'sim', cruzaToque: false },
      `a 900, com a ligação tocando, o modal adormece ou cobre a faixa: ${JSON.stringify(estreito)}`);
    assert.equal(estreito.porta, 'sim', `a 900, com o modal aberto, a porta da gaveta não está alcançável: ${JSON.stringify(estreito)}`);
    assert.equal(estreito.resto, 'inerte', `a 900, o resto da sessão escapou do modal: ${JSON.stringify(estreito)}`);
    assert.ok(estreito.restoCoberto.length >= 1 && estreito.restoCoberto.every(Boolean), `a 900, o resto da sessão não ficou sob o modal: ${JSON.stringify(estreito)}`);
    assert.deepEqual({ cruzaPorta: estreito.cruzaPorta, rola: estreito.rola, dentroDaJanela: estreito.dentroDaJanela }, { cruzaPorta: false, rola: true, dentroDaJanela: true },
      `a 900, o diálogo cruza a porta da gaveta, não rola, ou sai da janela: ${JSON.stringify(estreito)}`);
    await page.click('#tela-sessao .linha-canais');
    await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const gavetaAberta = await medirOModal();
    assert.deepEqual({ mudo: gavetaAberta.mudo, sair: gavetaAberta.sair }, { mudo: 'sim', sair: 'sim' },
      `a 900, com a gaveta aberta e o modal aberto, o MUDO ou o SAIR não está alcançável: ${JSON.stringify(gavetaAberta)}`);
    assert.equal(gavetaAberta.cruzaBarra, false, `a 900, o diálogo cruza a barra da gaveta aberta: ${JSON.stringify(gavetaAberta)}`);
    assert.ok(gavetaAberta.restoCoberto.length >= 2 && gavetaAberta.restoCoberto.every(Boolean), `a 900, a gaveta aberta escapou do modal fora da barra: ${JSON.stringify(gavetaAberta)}`);
    await page.setViewportSize({ width: 1400, height: 900 });

    // **O não modal e os avisos não cobrem a portaria** (a mesma decisão): o
    // diálogo não modal mora embaixo, à direita, e os avisos também — os dois
    // onde a portaria acende. Um não modal largo, e quatro avisos, a 1400 e a
    // 900, com a portaria acesa.
    for (const [largura, altura] of [[1400, 900], [900, 700]]) {
      await page.setViewportSize({ width: largura, height: altura });
      const visto = await page.evaluate(async () => {
        const mod = { id: 'teste/nucleo', hash: 'h', api: 6 };
        const inst = modsCarregados.get(mod.id);
        superficiesDoMod(mod, inst).soltarTudo();
        avisarQueBatem(1);
        await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 20, descricao: { id: 'baixo', tipo: 'dialogo', modal: false, titulo: 'Não modal', tamanho: { largura: 1600 } } });
        await atenderOMod(mod, inst, { tipo: 'superficie-montar', n: 21, superficie: 'baixo', arvore: Array.from({ length: 40 }, (_, i) => ({ forma: 'texto', chave: `l${i}`, dentro: `linha ${i} do não modal` })) });
        for (let i = 0; i < 4; i += 1) {
          await atenderOMod(mod, inst, { tipo: 'superficie-criar', n: 30 + i, descricao: { id: `aviso${i}`, tipo: 'aviso', titulo: `um aviso do MOD, longo o bastante para alargar a fila ${i}` } });
        }
        await new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
        const alcancado = (el) => {
          if (!el || !el.getClientRects().length) return 'invisivel';
          if (el.closest('[inert]')) return 'inerte';
          const r = el.getBoundingClientRect();
          const alvo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          return alvo && (alvo === el || el.contains(alvo)) ? 'sim' : `coberto por ${alvo?.className ?? alvo?.tagName}`;
        };
        const cruza = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
        const faixa = document.getElementById('portaria-batendo').getBoundingClientRect();
        const caixa = document.querySelector('.dialogo-de-mod[data-superficie="baixo"], [data-superficie="baixo"] .dialogo-de-mod, [data-superficie="baixo"].dialogo-de-mod');
        const avisos = [...document.querySelectorAll('.aviso-de-mod')].filter((a) => a.getClientRects().length);
        const medido = {
          ver: alcancado(document.getElementById('portaria-batendo-ver')),
          depois: alcancado(document.getElementById('portaria-batendo-depois')),
          cruzaODialogo: Boolean(caixa && cruza(caixa.getBoundingClientRect(), faixa)),
          cruzaUmAviso: avisos.some((a) => cruza(a.getBoundingClientRect(), faixa)),
          avisos: avisos.length,
          dialogo: Boolean(caixa),
        };
        superficiesDoMod(mod, inst).soltarTudo();
        avisarQueBatem(0);
        return medido;
      });
      assert.deepEqual(visto, { ver: 'sim', depois: 'sim', cruzaODialogo: false, cruzaUmAviso: false, avisos: 4, dialogo: true },
        `a ${largura}, o diálogo não modal ou os avisos de um MOD cobrem a portaria: ${JSON.stringify(visto)}`);
    }
    await page.setViewportSize({ width: 1400, height: 900 });

    assert.deepEqual(errors, []);
    console.log('núcleo: o MUDO, o fone, o SAIR, o «no ar» e o veredito têm as cores do produto sob o papel, o claro e o hostil; e o guarda 11 vale para os treze itens, a faixa de quem liga entre eles, com a capa e a barra do MOD montadas; e o modal de MOD não cobre a barra, a faixa de quem liga nem a portaria, a 1400 e a 900, e continua modal no resto; e o não modal e os avisos não cobrem a portaria; e a linha de dono do «⋯» fica descoberta, acima de 4,5:1 e fora do tema.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
