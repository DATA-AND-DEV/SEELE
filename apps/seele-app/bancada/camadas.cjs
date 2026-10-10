// As camadas da API 6 (rascunho §5; tarefa 7 da F3), com o HTML, o CSS e o
// roteador reais; só a ponte nativa é simulada. No molde de `capa.cjs`.
//
// # O que ela mede
//
// 1. **A `folha`** (Step 4), que substitui o `painel` na 6: mora em
//    `#palco-de-paineis`, empilha até três por MOD — a quarta é recusada com o
//    número, também ao reabrir —, só a de cima aparece, e a saída é do
//    produto, com o `descartou` quando a pessoa aceita perder o rascunho.
// 2. **O `painel` na 6 é recusado pelo nome**, com «use folha» no `seele.log`;
//    até a 5 ele continua, e a `folha`, que a 5 não tem, abre como `dialogo`.
// 3. **O teto de superfícies de pé chega à 6 pelo nome**, com o número: ele
//    chegava como `falhou`.
// 4. **A 300 px, nenhum título de folha, painel ou diálogo quebra dentro de
//    uma palavra**; uma palavra maior que a coluna é a única exceção. O título
//    quebrava «EDITA / R / PERFIL» (achado da tarefa 7, Step 4).
// 5. **A mídia do servidor por `volume`, em todo lugar** (Step 5b; rascunho
//    §5, a pergunta da MESA): o `fundo` de uma `tela`, a `midia`, o
//    `fundoDeMidia` de uma caixa, o `retrato` e a `pessoa.capa`, com
//    `pedido.transporte = 'volume'`, leem pelo fluxo do ADR 0048 — o
//    `ler_imagem_mod` —, e nenhum pelo controle (`mod_request`).
// 6. **A montagem por partes** (Step 5c; rascunho §12): uma ficha que inteira
//    passa dos 12 KiB de uma mensagem se monta num esqueleto e quatro
//    `superficie-atualizar` de ~9 KiB, e aparece inteira; trocar uma parte
//    não remonta as outras; uma chave que não existe é recusada pelo nome, e
//    uma parte que leva a superfície além de 4096 nós, com os números — e a
//    superfície fica como estava.
// 7. **Os formulários validam o que declaram** (Step 6a; rascunho §5): uma
//    regra violada — `obrigatorio`, `tamanhoMaximo`, `minimo`/`maximo`,
//    `inteiro` — impede o `submeter` sem ida ao MOD, marca o campo com a frase
//    do produto e leva o foco a ele; corrigido, o envio chega.
// 8. **A pilha de inércia deixa acordado o que a casca pede** (Step 5d, o
//    lado da F3): com um modal de MOD aberto, o que `CELULAR.acordadosNoModal()`
//    devolve — no celular, a faixa de voz e as abas — continua alcançável, e o
//    resto da sessão dorme. `inert` desce, então a pilha desce também: não
//    adormece o ancestral que os contém. O desenho e a medida no aparelho são
//    da F8 (a cena `folhas-e-mods`); aqui, um dublê do `CELULAR` no desktop.
// 9. **A recusa do servidor volta aos campos** (Step 6b; rascunho §5, o B7 do
//    ESTILO): `formulario.erros({ texto, fundo })` marca os dois campos de
//    uma vez, com a frase, e leva o foco ao primeiro; mexer num campo apaga só
//    a marca dele; uma chave que não existe é recusada pelo nome, sem marcar
//    nada; e um mapa vazio limpa.
// 10. **O `sujo` é derivado** (Step 6c; rascunho §5, o bloqueio D do
//    PERFIS): um campo declarado com `valor` (o rascunho) e `inicial` (o
//    gravado) nasce sujo, e fechar pergunta; voltar ao gravado o limpa, e
//    fechar não pergunta; e mexer num campo sem `inicial` suja, sem o MOD
//    chamar `suja`.
// 11. **A ação destrutiva é confirmada pelo produto** (Step 6d; rascunho §5,
//    o A13 e o B7 do ESTILO): um `botao` com `confirmar` abre a confirmação
//    do produto, alcançável com o modal aberto, e o evento `botao` só chega
//    ao MOD depois do sim; cancelar não manda nada; um botão sem `confirmar`
//    manda na hora.
// 12. **O `erro` vale em toda forma de entrada** (Step 6e; rascunho §5, o
//    ESTILO §4): a `escolha` e a `cor` mostram o `erro` do MOD como o campo,
//    recebem o `formulario.erros` e sujam a superfície; o hexadecimal
//    incompleto continua marcado enquanto se digita, sem frase.
//
// Rodar:  node apps/seele-app/bancada/camadas.cjs

const assert = require('node:assert/strict');
const { servir, respostas, retrato } = require('./telas.cjs');
const { chromium } = require('./playwright.cjs');

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
      window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          testCalls.push({ cmd, args });
          return testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      entrarNaGeracao(1);
      // Um MOD de cada API, de pé, com o que a janela lhe entrega anotado.
      window.entregues = [];
      window.modDe = {};
      for (const api of [5, 6]) {
        const mod = { id: `teste/api${api}`, hash: `h${api}`, api };
        const inst = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, {
          pedirEncerramento() {}, async encerrou() { return true; },
          entregar(m) { entregues.push({ mod: mod.id, ...JSON.parse(JSON.stringify(m)) }); },
        });
        inst.estado = ESTADOS_DE_MOD.ativa;
        modsCarregados.set(mod.id, inst);
        modDe[api] = { mod, inst };
      }
      let n = 0;
      window.pedir = async (api, mensagem) => {
        const { mod, inst } = modDe[api];
        const numero = ++n;
        await atenderOMod(mod, inst, { ...mensagem, n: numero });
        return entregues.find(m => m.tipo === 'resposta' && m.n === numero && m.mod === mod.id) ?? null;
      };
      window.eventosDe = api => entregues.filter(m => m.mod === modDe[api].mod.id && m.tipo === 'evento');
      window.linhasDoLog = () => testCalls.filter(c => c.cmd === 'registrar_da_janela').map(c => c.args);
      window.folhasNoPalco = () => [...document.querySelectorAll('#palco-de-paineis > .folha-de-mod')]
        .map(f => ({
          chave: f.dataset.superficie,
          aparece: f.getClientRects().length > 0 && getComputedStyle(f).display !== 'none',
        }));
    }, retrato());

    // 1. Três folhas de pé; só a de cima aparece, no palco dos painéis.
    for (const id of ['um', 'dois', 'tres']) {
      const r = await page.evaluate(id => pedir(6, { tipo: 'superficie-criar', descricao: { id, tipo: 'folha', titulo: `Folha ${id}` } }), id);
      assert.equal(r?.ok, true, `a folha «${id}» foi recusada: ${JSON.stringify(r)}`);
    }
    let pilha = await page.evaluate(() => folhasNoPalco());
    assert.deepEqual(pilha.map(f => f.chave), ['um', 'dois', 'tres'], `as folhas não foram ao #palco-de-paineis: ${JSON.stringify(pilha)}`);
    assert.deepEqual(pilha.filter(f => f.aparece).map(f => f.chave), ['tres'],
      `na pilha, aparece mais de uma folha, ou não a de cima: ${JSON.stringify(pilha)}`);
    assert.equal(await page.evaluate(() => $('palco-de-paineis').hidden), false, 'o palco dos painéis ficou escondido com folhas de pé');

    // A quarta é recusada com o nome e o número.
    const quarta = await page.evaluate(() => pedir(6, { tipo: 'superficie-criar', descricao: { id: 'quatro', tipo: 'folha' } }));
    assert.equal(quarta?.ok, false, `a quarta folha foi aceita: ${JSON.stringify(quarta)}`);
    assert.equal(quarta?.erro, 'teto-de-folhas', `a quarta folha não foi recusada pelo nome: ${JSON.stringify(quarta)}`);
    assert.equal(quarta?.teto, 3, `a recusa da quarta folha não disse o número: ${JSON.stringify(quarta)}`);
    assert.equal(quarta?.tipo, 'resposta', 'a recusa trocou o tipo da mensagem da ponte');

    // A saída do produto fecha a de cima, e a de baixo volta a aparecer.
    await page.click('#palco-de-paineis > .folha-de-mod[data-superficie="tres"] .superficie-de-mod-sair');
    pilha = await page.evaluate(() => folhasNoPalco());
    assert.deepEqual(pilha.filter(f => f.aparece).map(f => f.chave), ['dois'],
      `fechar a de cima não devolveu a de baixo: ${JSON.stringify(pilha)}`);
    const fechou = await page.evaluate(() => eventosDe(6).filter(e => e.nome === 'fechar').at(-1));
    assert.equal(fechou?.superficie, 'tres', `o MOD não soube que a folha fechou: ${JSON.stringify(fechou)}`);
    assert.equal(fechou?.porque, 'saida-do-produto', `o MOD não soube que foi a saída do produto: ${JSON.stringify(fechou)}`);

    // **Reabrir também conta.** Com «tres» fechada, uma quarta cabe; e
    // reabrir «tres» depois passaria a pilha de três.
    const outra = await page.evaluate(() => pedir(6, { tipo: 'superficie-criar', descricao: { id: 'quatro', tipo: 'folha' } }));
    assert.equal(outra?.ok, true, `com uma folha fechada, a terceira de pé foi recusada: ${JSON.stringify(outra)}`);
    const reaberta = await page.evaluate(() => pedir(6, { tipo: 'superficie-mostrar', superficie: 'tres' }));
    assert.equal(reaberta?.erro, 'teto-de-folhas', `reabrir uma folha passou da pilha de três: ${JSON.stringify(reaberta)}`);
    const reabertaPorCriar = await page.evaluate(() => pedir(6, { tipo: 'superficie-criar', descricao: { id: 'tres', tipo: 'folha' } }));
    assert.equal(reabertaPorCriar?.erro, 'teto-de-folhas', `reabrir pelo criar passou da pilha de três: ${JSON.stringify(reabertaPorCriar)}`);
    assert.equal((await page.evaluate(() => folhasNoPalco())).length, 3, 'a folha recusada entrou no palco');

    // Com rascunho sujo, a saída do produto pergunta; descartar diz ao MOD.
    await page.evaluate(async () => {
      await pedir(6, { tipo: 'superficie-descartar', superficie: 'quatro' });
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'rascunho', tipo: 'folha', titulo: 'Editar', fecharComAlteracoes: 'confirmar' } });
      await pedir(6, { tipo: 'superficie-suja', superficie: 'rascunho', suja: true });
    });
    await page.click('#palco-de-paineis > .folha-de-mod[data-superficie="rascunho"] .superficie-de-mod-sair');
    assert.equal(await page.evaluate(() => $('moderar').hidden), false, 'fechar a folha com rascunho não perguntou');
    assert.equal(await page.evaluate(() => folhasNoPalco().some(f => f.chave === 'rascunho' && f.aparece)), true,
      'a folha com rascunho fechou antes da resposta');
    await page.click('#moderar-confirmar');
    const descartou = await page.evaluate(() => eventosDe(6).filter(e => e.nome === 'fechar').at(-1));
    assert.equal(descartou?.superficie, 'rascunho', `o MOD não soube que a folha com rascunho fechou: ${JSON.stringify(descartou)}`);
    assert.equal(descartou?.descartou, true, `o MOD não soube que o rascunho foi descartado: ${JSON.stringify(descartou)}`);

    // **Mostrar a de baixo a traz para cima**: «mostre aquela» é para ela ser
    // vista, e a de baixo não aparece.
    await page.evaluate(() => pedir(6, { tipo: 'superficie-mostrar', superficie: 'um' }));
    pilha = await page.evaluate(() => folhasNoPalco());
    assert.deepEqual(pilha.filter(f => f.aparece).map(f => f.chave), ['um'],
      `mostrar a folha de baixo não a trouxe para cima: ${JSON.stringify(pilha)}`);

    // 2. O `painel` na 6 é recusado pelo nome, com «use folha» no seele.log.
    const painel = await page.evaluate(() => pedir(6, { tipo: 'superficie-criar', descricao: { id: 'p', tipo: 'painel' } }));
    assert.equal(painel?.erro, 'tipo-desconhecido', `o painel abriu na API 6: ${JSON.stringify(painel)}`);
    assert.equal(painel?.tipoPedido, 'painel', `a recusa do painel não disse o tipo pedido: ${JSON.stringify(painel)}`);
    assert.ok(painel?.tipos?.includes('folha') && !painel.tipos.includes('painel'),
      `os tipos da API 6 não são os dela: ${JSON.stringify(painel?.tipos)}`);
    assert.ok(await page.evaluate(() => linhasDoLog().some(l => l.modId === 'teste/api6' && /painel/.test(l.oQue) && /use folha/.test(l.oQue))),
      `o seele.log não disse «use folha»: ${JSON.stringify(await page.evaluate(() => linhasDoLog()))}`);

    // 3. A décima terceira superfície de pé, na 6: o teto pelo nome.
    const teto = await page.evaluate(async () => {
      let ultima = null;
      for (let i = 0; i < 13; i += 1) {
        ultima = await pedir(6, { tipo: 'superficie-criar', descricao: { id: `pag-${i}`, tipo: 'pagina', visivel: false } });
        if (ultima?.ok === false) break;
      }
      return ultima;
    });
    assert.equal(teto?.erro, 'teto-de-superficies', `o teto de superfícies não chegou à 6 pelo nome: ${JSON.stringify(teto)}`);
    assert.equal(teto?.teto, 12, `o teto de superfícies não disse o número: ${JSON.stringify(teto)}`);

    // Até a 5, o painel continua, e a folha — que a 5 não tem — abre como
    // dialogo, com a linha no seele.log.
    const painel5 = await page.evaluate(() => pedir(5, { tipo: 'superficie-criar', descricao: { id: 'p5', tipo: 'painel' } }));
    assert.equal(painel5?.ok, true, `o painel deixou de abrir na API 5: ${JSON.stringify(painel5)}`);
    assert.ok(await page.evaluate(() => Boolean(document.querySelector('#palco-de-paineis > .painel-de-mod[data-superficie="p5"]'))),
      'o painel da API 5 não foi ao palco dos painéis');
    const folha5 = await page.evaluate(() => pedir(5, { tipo: 'superficie-criar', descricao: { id: 'f5', tipo: 'folha' } }));
    assert.equal(folha5?.ok, true, `a folha na API 5 foi recusada, e não aberta como antes: ${JSON.stringify(folha5)}`);
    assert.ok(await page.evaluate(() => Boolean(document.querySelector('#palco-de-camadas .dialogo-de-mod[data-superficie="f5"]'))),
      'a folha na API 5 não abriu como dialogo');
    assert.ok(await page.evaluate(() => linhasDoLog().some(l => l.modId === 'teste/api5' && /folha/.test(l.oQue) && /dialogo/.test(l.oQue))),
      'a folha da API 5, aberta como dialogo, não foi ao seele.log');

    // 4. Os títulos a 300 px. Cada palavra é medida pelo `Range` dela: mais de
    // uma linha é quebra dentro da palavra, e só vale para a palavra que, sem
    // quebrar, passa da largura da coluna (o cabeçalho, sem o preenchimento).
    const TITULOS = ['Editar perfil', 'Configurações do servidor de voz', 'Ficha', 'Mapa ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKL da mesa'];
    const quebras = await page.evaluate(async (titulos) => {
      // O MOD da 6 está no teto de doze, pela seção 3: começa do zero.
      superficiesDoMod(modDe[6].mod, modDe[6].inst).soltarTudo();
      const achadas = [];
      const medir = (raiz, onde) => {
        const titulo = raiz.querySelector('.superficie-de-mod-titulo');
        const cabeca = titulo.parentElement;
        const estilo = getComputedStyle(cabeca);
        const coluna = cabeca.clientWidth - parseFloat(estilo.paddingLeft) - parseFloat(estilo.paddingRight);
        const texto = titulo.firstChild;
        for (const palavra of texto.textContent.matchAll(/\S+/g)) {
          const faixa = document.createRange();
          faixa.setStart(texto, palavra.index);
          faixa.setEnd(texto, palavra.index + palavra[0].length);
          const linhas = new Set([...faixa.getClientRects()].filter(r => r.width > 0).map(r => Math.round(r.top)));
          if (linhas.size < 2) continue;
          const inteira = document.createElement('span');
          inteira.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap';
          inteira.textContent = palavra[0];
          titulo.append(inteira);
          const largura = inteira.getBoundingClientRect().width;
          inteira.remove();
          if (largura <= coluna) achadas.push(`${onde}: «${palavra[0]}» em ${linhas.size} linhas (${Math.round(largura)} px numa coluna de ${Math.round(coluna)} px)`);
        }
        // **E a saída fica na coluna, na borda direita**: dar ao título a
        // largura da maior palavra empurrava o FECHAR para fora (medido).
        const sair = raiz.querySelector('.superficie-de-mod-sair').getBoundingClientRect();
        const caixa = cabeca.getBoundingClientRect();
        const direita = caixa.right - parseFloat(estilo.paddingRight);
        if (sair.left < caixa.left || sair.right > direita + 0.5) achadas.push(`${onde}: o FECHAR saiu da coluna (${Math.round(sair.left)}–${Math.round(sair.right)} contra ${Math.round(caixa.left)}–${Math.round(direita)})`);
        else if (direita - sair.right > 0.5) achadas.push(`${onde}: o FECHAR não está na borda direita (${Math.round(direita - sair.right)} px antes dela)`);
      };
      let i = 0;
      for (const titulo of titulos) {
        i += 1;
        for (const [api, tipo, extra] of [[6, 'folha', {}], [5, 'painel', {}], [6, 'dialogo', { modal: false, tamanho: { largura: 300 } }]]) {
          const id = `titulo-${tipo}-${i}`;
          const r = await pedir(api, { tipo: 'superficie-criar', descricao: { id, tipo, titulo, ...extra } });
          if (!r?.ok) { achadas.push(`${tipo} «${titulo}» não abriu: ${JSON.stringify(r)}`); continue; }
          const raiz = document.querySelector(`[data-superficie="${id}"]`);
          medir(raiz, `${tipo} (${Math.round(raiz.getBoundingClientRect().width)} px) «${titulo}»`);
          await pedir(api, { tipo: 'superficie-descartar', superficie: id });
        }
      }
      return achadas;
    }, TITULOS);
    assert.deepEqual(quebras, [], 'um título de superfície quebrou dentro de uma palavra que cabia na coluna, ou a saída saiu do lugar');

    // 5. A mídia do servidor por volume, nos cinco lugares.
    const lugares = await page.evaluate(async () => {
      const canvas = document.createElement('canvas');
      canvas.width = 8; canvas.height = 8;
      canvas.getContext('2d').fillRect(0, 0, 8, 8);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 128, uri: canvas.toDataURL() };
      const doVolume = (lugar) => ({ canal: 1, campo: 'image', pedido: { transporte: 'volume', path: `volume:${lugar}` } });
      const antes = testCalls.length;
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'midias', tipo: 'pagina', titulo: 'Mídias' } });
      const montou = await pedir(6, { tipo: 'superficie-montar', superficie: 'midias', arvore: [
        { forma: 'tela', chave: 'mapa', largura: 64, altura: 64, fundo: { doServidor: doVolume('fundo-da-tela') } },
        { forma: 'midia', chave: 'foto', doServidor: doVolume('midia') },
        { forma: 'caixa', chave: 'cartaz', fundoDeMidia: { doServidor: doVolume('fundo-de-midia') }, dentro: [{ forma: 'texto', chave: 't', texto: 'x' }] },
        { forma: 'retrato', chave: 'rosto', doServidor: doVolume('retrato') },
      ] });
      contribuicoesDosMods.registrar(modDe[6].mod, modDe[6].inst, {
        ponto: 'pessoa.capa', modo: 'substituir', alvo: String(testTable.snapshot.me),
        conteudo: { doServidor: doVolume('capa') },
      });
      desenhar(testTable.snapshot);
      await new Promise(ok => setTimeout(ok, 600));
      const chamadas = testCalls.slice(antes);
      const caminhoDe = (c) => { try { return JSON.parse(c.args?.payload ?? '{}').path ?? ''; } catch { return ''; } };
      return {
        montou,
        peloVolume: chamadas.filter(c => c.cmd === 'ler_imagem_mod').map(caminhoDe),
        peloControle: chamadas.filter(c => c.cmd === 'mod_request').map(caminhoDe).filter(Boolean),
      };
    });
    assert.equal(lugares.montou?.ok, true, `a árvore com as mídias não montou: ${JSON.stringify(lugares.montou)}`);
    for (const lugar of ['fundo-da-tela', 'midia', 'fundo-de-midia', 'retrato', 'capa']) {
      assert.ok(lugares.peloVolume.includes(`volume:${lugar}`),
        `${lugar}: a mídia com transporte 'volume' não foi lida pelo ler_imagem_mod: ${JSON.stringify(lugares)}`);
    }
    assert.deepEqual(lugares.peloControle, [], `uma mídia com transporte 'volume' foi pedida pelo controle: ${JSON.stringify(lugares)}`);

    // 6. A montagem por partes.
    const partes = await page.evaluate(async () => {
      const SECOES = ['inventario', 'pericias', 'notas', 'historia'];
      const parte = (secao, quantos, prefixo = secao) => ({
        forma: 'caixa', chave: secao,
        dentro: Array.from({ length: quantos }, (_, i) => ({
          forma: 'texto', chave: `${prefixo}-${i}`,
          dentro: `${prefixo} ${i}: uma linha da ficha, com o que a seção guarda`,
        })),
      });
      const esqueleto = [{ forma: 'titulo', chave: 'nome', dentro: 'Ficha' }, ...SECOES.map((secao) => ({ forma: 'caixa', chave: secao, dentro: [] }))];
      const cheias = SECOES.map((secao) => parte(secao, 90));
      const tamanho = (valor) => new TextEncoder().encode(JSON.stringify(valor)).length;
      const medidas = {
        inteira: tamanho({ tipo: 'superficie-montar', superficie: 'ficha', arvore: [esqueleto[0], ...cheias] }),
        cadaParte: cheias.map((arvore) => tamanho({ tipo: 'superficie-atualizar', superficie: 'ficha', chave: arvore.chave, arvore })),
      };
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'ficha', tipo: 'pagina', titulo: 'Ficha' } });
      const montou = await pedir(6, { tipo: 'superficie-montar', superficie: 'ficha', arvore: esqueleto });
      const respostas = [];
      for (const arvore of cheias) {
        respostas.push(await pedir(6, { tipo: 'superficie-atualizar', superficie: 'ficha', chave: arvore.chave, arvore }));
      }
      const raiz = document.querySelector('[data-superficie="ficha"]');
      const texto = raiz?.textContent ?? '';
      const aparecem = SECOES.map((secao) => texto.includes(`${secao} 0:`) && texto.includes(`${secao} 89:`));
      // Trocar uma parte não remonta as outras: o elemento de uma linha do
      // inventário é o mesmo depois de trocar as perícias.
      const linhaDoInventario = [...raiz.querySelectorAll('*')].find((e) => e.childElementCount === 0 && e.textContent.startsWith('inventario 0:'));
      const trocou = await pedir(6, { tipo: 'superficie-atualizar', superficie: 'ficha', chave: 'pericias', arvore: parte('pericias', 3, 'pericias-nova') });
      const mesmoNo = linhaDoInventario?.isConnected === true;
      const textoDepois = raiz.textContent;
      const semChave = await pedir(6, { tipo: 'superficie-atualizar', superficie: 'ficha', chave: 'nao-existe', arvore: parte('nao-existe', 1) });
      const grandeDemais = await pedir(6, { tipo: 'superficie-atualizar', superficie: 'ficha', chave: 'notas', arvore: parte('notas', 4100, 'notas-demais') });
      const notasNaTela = raiz.textContent.includes('notas 89:') && !raiz.textContent.includes('notas-demais 0:');
      // **E a recusa não estraga a declaração**: a parte seguinte desenha a
      // ficha com as notas de antes, e não com as recusadas.
      await pedir(6, { tipo: 'superficie-atualizar', superficie: 'ficha', chave: 'historia', arvore: parte('historia', 2, 'historia-nova') });
      return {
        medidas, montou, respostas, aparecem, trocou, mesmoNo,
        pericias: { novas: textoDepois.includes('pericias-nova 2:'), velhas: textoDepois.includes('pericias 89:') },
        semChave, grandeDemais,
        notasFicaram: notasNaTela,
        notasDepoisDaProxima: raiz.textContent.includes('notas 89:') && !raiz.textContent.includes('notas-demais 0:')
          && raiz.textContent.includes('historia-nova 1:'),
      };
    });
    assert.ok(partes.medidas.inteira > 12 * 1024, `a ficha inteira não passa dos 12 KiB, e o caso não mede o que diz: ${JSON.stringify(partes.medidas)}`);
    assert.ok(partes.medidas.cadaParte.every((n) => n > 8 * 1024 && n < 12 * 1024), `cada parte não tem ~9 KiB: ${JSON.stringify(partes.medidas)}`);
    assert.equal(partes.montou?.ok, true, `o esqueleto não montou: ${JSON.stringify(partes.montou)}`);
    assert.ok(partes.respostas.every((r) => r?.ok === true), `uma parte foi recusada: ${JSON.stringify(partes.respostas)}`);
    assert.deepEqual(partes.aparecem, [true, true, true, true], 'a ficha montada por partes não aparece inteira');
    assert.equal(partes.trocou?.ok, true, `trocar uma parte foi recusado: ${JSON.stringify(partes.trocou)}`);
    assert.deepEqual(partes.pericias, { novas: true, velhas: false }, 'trocar as perícias não trocou o que estava sob a chave');
    assert.equal(partes.mesmoNo, true, 'trocar as perícias remontou o inventário');
    assert.equal(partes.semChave?.erro, 'chave-desconhecida', `uma chave que não existe não foi recusada pelo nome: ${JSON.stringify(partes.semChave)}`);
    assert.equal(partes.semChave?.chave, 'nao-existe', `a recusa da chave não disse qual: ${JSON.stringify(partes.semChave)}`);
    assert.equal(partes.grandeDemais?.erro, 'orcamento', `a parte grande demais não foi recusada pelo orçamento: ${JSON.stringify(partes.grandeDemais)}`);
    assert.equal(partes.grandeDemais?.teto, 4096, `a recusa do orçamento não disse o teto: ${JSON.stringify(partes.grandeDemais)}`);
    assert.ok(partes.grandeDemais?.tamanho > 4096, `a recusa do orçamento não disse o tamanho: ${JSON.stringify(partes.grandeDemais)}`);
    assert.equal(partes.grandeDemais?.perfil, 'superficie', `a recusa do orçamento não disse o perfil: ${JSON.stringify(partes.grandeDemais)}`);
    assert.equal(partes.notasFicaram, true, 'a parte recusada trocou as notas assim mesmo');
    assert.equal(partes.notasDepoisDaProxima, true, 'a parte recusada ficou na declaração, e a parte seguinte a desenhou');

    // 7. As regras declaradas dos formulários.
    await page.evaluate(async () => {
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'perfil', tipo: 'dialogo', modal: false, titulo: 'Perfil' } });
      await pedir(6, { tipo: 'superficie-montar', superficie: 'perfil', arvore: [{
        forma: 'formulario', chave: 'f', dentro: [
          { forma: 'campo', chave: 'nome', rotulo: 'Nome', obrigatorio: true },
          { forma: 'campo', chave: 'lema', rotulo: 'Lema', tamanhoMaximo: 10 },
          { forma: 'numero', chave: 'nivel', rotulo: 'Nível', minimo: 1, maximo: 20 },
          { forma: 'numero', chave: 'peso', rotulo: 'Peso', inteiro: true },
        ],
      }] });
    });
    const campo = (chave) => `[data-superficie="perfil"] [data-chave-do-mod="${chave}"] :is(input, textarea)`;
    const preencher = async (valores) => { for (const [chave, valor] of Object.entries(valores)) await page.fill(campo(chave), valor); };
    const enviar = () => page.evaluate(() => {
      const antes = eventosDe(6).filter((e) => e.nome === 'submeter').length;
      document.querySelector('[data-superficie="perfil"] form').requestSubmit();
      const marcados = [...document.querySelectorAll('[data-superficie="perfil"] [data-chave-do-mod][data-invalido="sim"]')]
        .map((e) => ({ chave: e.dataset.chaveDoMod, frase: e.dataset.mensagemDeErro ?? '', aria: e.querySelector('input,textarea')?.getAttribute('aria-invalid') }));
      return {
        foi: eventosDe(6).filter((e) => e.nome === 'submeter').length > antes,
        marcados,
        foco: document.activeElement?.closest('[data-chave-do-mod]')?.dataset.chaveDoMod ?? null,
      };
    });
    const VALIDOS = { nome: 'Alex', lema: 'curto', nivel: '5', peso: '70' };
    const CASOS = [
      ['obrigatorio', { nome: '' }, 'nome'],
      ['tamanhoMaximo', { lema: 'longo demais!' }, 'lema'],
      ['minimo/maximo', { nivel: '25' }, 'nivel'],
      ['inteiro', { peso: '70.5' }, 'peso'],
    ];
    for (const [regra, quebra, chave] of CASOS) {
      await preencher({ ...VALIDOS, ...quebra });
      const r = await enviar();
      assert.equal(r.foi, false, `${regra}: o submeter foi ao MOD com a regra violada`);
      assert.deepEqual(r.marcados.map((m) => m.chave), [chave], `${regra}: o campo marcado não é o da regra: ${JSON.stringify(r)}`);
      assert.ok(r.marcados[0].frase.length > 0 && r.marcados[0].aria === 'true', `${regra}: a marca não tem frase ou aria-invalid: ${JSON.stringify(r)}`);
      assert.equal(r.foco, chave, `${regra}: o foco não foi ao campo inválido: ${JSON.stringify(r)}`);
      // **Corrigir apaga a frase antes do envio**: quem conserta o campo não
      // fica lendo o erro que já resolveu.
      await page.fill(campo(chave), VALIDOS[chave]);
      const aindaMarcado = await page.evaluate((chave) => document.querySelector(`[data-superficie="perfil"] [data-chave-do-mod="${chave}"]`)?.dataset.invalido ?? null, chave);
      assert.equal(aindaMarcado, null, `${regra}: corrigir o campo não apagou a frase do produto`);
      await preencher({ ...VALIDOS, ...quebra });
      if (regra === 'tamanhoMaximo') {
        const contador = await page.evaluate(() => {
          const c = document.querySelector('[data-superficie="perfil"] [data-chave-do-mod="lema"] .regiao-de-mod-contador');
          return c ? { texto: c.textContent, passou: c.dataset.passou } : null;
        });
        assert.deepEqual(contador, { texto: '13/10', passou: 'sim' }, `o contador do tamanho não diz o que passou: ${JSON.stringify(contador)}`);
      }
    }
    await preencher(VALIDOS);
    const certo = await enviar();
    assert.equal(certo.foi, true, `com tudo certo, o submeter não chegou: ${JSON.stringify(certo)}`);
    assert.deepEqual(certo.marcados, [], `com tudo certo, um campo ficou marcado: ${JSON.stringify(certo)}`);
    const valores = await page.evaluate(() => eventosDe(6).filter((e) => e.nome === 'submeter').at(-1)?.valores);
    assert.deepEqual(valores, { nome: 'Alex', lema: 'curto', nivel: 5, peso: 70 }, 'o submeter não levou os valores');

    // 8. A inércia com o que a casca pede acordado.
    const inercia = await page.evaluate(async () => {
      // Sem nenhuma superfície de pé: a folha da API 5, na seção 2, abriu como
      // diálogo modal, e um modal que sobra deixa a sessão inerte por ele.
      for (const api of [5, 6]) superficiesDoMod(modDe[api].mod, modDe[api].inst).soltarTudo();
      const mudo = document.getElementById('botao-mudo');
      const sair = document.getElementById('operador-sair');
      const outro = document.querySelector('#tela-sessao textarea');
      const inerte = (e) => Boolean(e?.closest('[inert]'));
      globalThis.CELULAR = { acordadosNoModal: () => [mudo, sair] };
      try {
        await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'modal', tipo: 'dialogo', titulo: 'Modal' } });
        const durante = {
          mudo: inerte(mudo), sair: inerte(sair), outro: inerte(outro),
          dialogo: inerte(document.querySelector('[data-superficie="modal"]')),
          noMesmoRamo: outro?.closest('#tela-sessao') === mudo?.closest('#tela-sessao'),
        };
        await pedir(6, { tipo: 'superficie-descartar', superficie: 'modal' });
        return { durante, depois: { mudo: inerte(mudo), sair: inerte(sair), outro: inerte(outro) } };
      } finally {
        delete globalThis.CELULAR;
      }
    });
    assert.equal(inercia.durante.noMesmoRamo, true, `o caso não mede o que diz: o outro controle não está no ramo do MUDO: ${JSON.stringify(inercia)}`);
    assert.deepEqual(inercia.durante, { mudo: false, sair: false, outro: true, dialogo: false, noMesmoRamo: true },
      `com o modal aberto, o que a casca pede não ficou acordado, ou o resto não dormiu: ${JSON.stringify(inercia)}`);
    assert.deepEqual(inercia.depois, { mudo: false, sair: false, outro: false }, `fechar o modal não devolveu a sessão: ${JSON.stringify(inercia)}`);

    // 9. A recusa do servidor de volta aos campos.
    const marcadosEm = (superficie) => page.evaluate((superficie) => [...document.querySelectorAll(`[data-superficie="${superficie}"] [data-chave-do-mod][data-invalido="sim"]`)]
      .map((e) => `${e.dataset.chaveDoMod}: ${e.dataset.mensagemDeErro ?? ''}`), superficie);
    await page.evaluate(async () => {
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'estilo', tipo: 'dialogo', modal: false, titulo: 'Estilo' } });
      await pedir(6, { tipo: 'superficie-montar', superficie: 'estilo', arvore: [{
        forma: 'formulario', chave: 'f', dentro: [
          { forma: 'campo', chave: 'texto', rotulo: 'Texto', valor: '#ffffff' },
          { forma: 'campo', chave: 'fundo', rotulo: 'Fundo', valor: '#fefefe' },
          { forma: 'campo', chave: 'acento', rotulo: 'Acento', valor: '#ff6a00' },
        ],
      }] });
    });
    const doisDeUmaVez = await page.evaluate(() => pedir(6, { tipo: 'formulario-erros', superficie: 'estilo', erros: { texto: 'contraste 1,01 contra o fundo', fundo: 'contraste 1,01 contra o texto' } }));
    assert.equal(doisDeUmaVez?.ok, true, `formulario.erros foi recusado: ${JSON.stringify(doisDeUmaVez)}`);
    assert.deepEqual(await marcadosEm('estilo'), ['texto: contraste 1,01 contra o fundo', 'fundo: contraste 1,01 contra o texto'],
      'formulario.erros não marcou os dois campos com as frases');
    assert.equal(await page.evaluate(() => document.activeElement?.closest('[data-chave-do-mod]')?.dataset.chaveDoMod ?? null), 'texto',
      'o foco não foi ao primeiro campo marcado');
    await page.fill('[data-superficie="estilo"] [data-chave-do-mod="texto"] input', '#000000');
    assert.deepEqual(await marcadosEm('estilo'), ['fundo: contraste 1,01 contra o texto'], 'mexer num campo não apagou só a marca dele');
    const semChave = await page.evaluate(() => pedir(6, { tipo: 'formulario-erros', superficie: 'estilo', erros: { acento: 'x', 'nao-existe': 'y' } }));
    assert.equal(semChave?.erro, 'chave-desconhecida', `uma chave que não existe não foi recusada pelo nome: ${JSON.stringify(semChave)}`);
    assert.equal(semChave?.chave, 'nao-existe', `a recusa não disse qual chave: ${JSON.stringify(semChave)}`);
    assert.deepEqual(await marcadosEm('estilo'), ['fundo: contraste 1,01 contra o texto'], 'o pedido recusado marcou um campo assim mesmo');
    await page.evaluate(() => pedir(6, { tipo: 'formulario-erros', superficie: 'estilo', erros: {} }));
    assert.deepEqual(await marcadosEm('estilo'), [], 'um mapa vazio não limpou as marcas');

    // 10. O sujo derivado.
    const fecharPelaSaida = async () => {
      await page.click('[data-superficie="editor"] .superficie-de-mod-sair');
      return page.evaluate(() => ({
        perguntou: !$('moderar').hidden,
        aberta: Boolean(document.querySelector('[data-superficie="editor"]')?.isConnected),
      }));
    };
    await page.evaluate(async () => {
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'editor', tipo: 'dialogo', modal: false, titulo: 'Editor', fecharComAlteracoes: 'confirmar' } });
      await pedir(6, { tipo: 'superficie-montar', superficie: 'editor', arvore: [{
        forma: 'formulario', chave: 'f', dentro: [
          { forma: 'campo', chave: 'bio', rotulo: 'Bio', valor: 'o rascunho guardado', inicial: 'o gravado' },
          { forma: 'campo', chave: 'nome', rotulo: 'Nome', valor: 'Alex' },
        ],
      }] });
    });
    const comRascunho = await fecharPelaSaida();
    assert.deepEqual(comRascunho, { perguntou: true, aberta: true }, `o editor que reabre com rascunho não nasceu sujo: ${JSON.stringify(comRascunho)}`);
    await page.click('#moderar-cancelar');
    await page.fill('[data-superficie="editor"] [data-chave-do-mod="bio"] input', 'o gravado');
    const deVoltaAoGravado = await fecharPelaSaida();
    assert.deepEqual(deVoltaAoGravado, { perguntou: false, aberta: false }, `voltar ao gravado não limpou o sujo: ${JSON.stringify(deVoltaAoGravado)}`);
    await page.evaluate(() => pedir(6, { tipo: 'superficie-mostrar', superficie: 'editor' }));
    await page.fill('[data-superficie="editor"] [data-chave-do-mod="nome"] input', 'Outro nome');
    const mexido = await fecharPelaSaida();
    assert.deepEqual(mexido, { perguntou: true, aberta: true }, `mexer num campo não sujou sem o MOD chamar suja: ${JSON.stringify(mexido)}`);
    await page.click('#moderar-confirmar');
    const descartado = await page.evaluate(() => eventosDe(6).filter((e) => e.nome === 'fechar' && e.superficie === 'editor').at(-1));
    assert.equal(descartado?.descartou, true, `descartar o editor sujo não disse descartou: ${JSON.stringify(descartado)}`);

    // 11. O botão que pede confirmação.
    await page.evaluate(async () => {
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'aparencia', tipo: 'dialogo', titulo: 'Aparência' } });
      await pedir(6, { tipo: 'superficie-montar', superficie: 'aparencia', arvore: [
        { forma: 'botao', chave: 'salvar', dentro: 'SALVAR' },
        { forma: 'botao', chave: 'restaurar', dentro: 'RESTAURAR PADRÃO', variante: 'perigo', confirmar: 'isto muda a aparência para todo mundo neste servidor' },
      ] });
    });
    const botoesDe = (chave) => page.evaluate((chave) => eventosDe(6).filter((e) => e.nome === 'botao' && e.chave === chave).length, chave);
    const botaoDaAparencia = (chave) => `[data-superficie="aparencia"] button[data-chave-do-mod="${chave}"]`;
    await page.click(botaoDaAparencia('salvar'));
    assert.equal(await botoesDe('salvar'), 1, 'um botão sem confirmar não mandou o evento na hora');
    await page.click(botaoDaAparencia('restaurar'));
    const pergunta = await page.evaluate(() => ({ aberta: !$('moderar').hidden, alcancavel: !$('moderar').closest('[inert]'), texto: $('moderar').textContent }));
    assert.equal(pergunta.aberta, true, 'o botão com confirmar não abriu a confirmação do produto');
    assert.equal(pergunta.alcancavel, true, 'a confirmação abriu inerte, atrás do diálogo modal');
    assert.ok(pergunta.texto.includes('isto muda a aparência para todo mundo neste servidor'), `a confirmação não diz a consequência que o MOD escreveu: ${pergunta.texto.slice(0, 200)}`);
    assert.equal(await botoesDe('restaurar'), 0, 'o evento do botão chegou antes do sim');
    await page.click('#moderar-cancelar');
    assert.equal(await botoesDe('restaurar'), 0, 'cancelar a confirmação mandou o evento do botão');
    await page.click(botaoDaAparencia('restaurar'));
    await page.click('#moderar-confirmar', { timeout: 3000 });
    assert.equal(await botoesDe('restaurar'), 1, 'o sim na confirmação não mandou o evento do botão, uma vez');
    await page.evaluate(() => pedir(6, { tipo: 'superficie-descartar', superficie: 'aparencia' }));

    // 12. O erro na escolha e na cor.
    const marcaDe = (chave) => page.evaluate((chave) => {
      const parte = document.querySelector(`[data-superficie="cores"] [data-chave-do-mod="${chave}"]`);
      const caixa = parte?.querySelector('select, input.regiao-de-mod-hexa');
      return { invalido: parte?.dataset.invalido ?? null, frase: parte?.dataset.mensagemDeErro ?? null, aria: caixa?.getAttribute('aria-invalid') ?? null };
    }, chave);
    const arvoreDasCores = (comErro) => [{
      forma: 'formulario', chave: 'f', dentro: [
        { forma: 'escolha', chave: 'paleta', rotulo: 'Paleta', valor: 'noite', opcoes: [{ valor: 'noite' }, { valor: 'dia' }], ...(comErro ? { erro: 'esta paleta não combina com o tema' } : {}) },
        { forma: 'cor', chave: 'acento', rotulo: 'Acento', valor: '#ff6a00', ...(comErro ? { erro: 'contraste 1,2 contra o fundo' } : {}) },
      ],
    }];
    await page.evaluate(async (arvore) => {
      await pedir(6, { tipo: 'superficie-criar', descricao: { id: 'cores', tipo: 'dialogo', modal: false, titulo: 'Cores', fecharComAlteracoes: 'confirmar' } });
      await pedir(6, { tipo: 'superficie-montar', superficie: 'cores', arvore });
    }, arvoreDasCores(true));
    assert.deepEqual(await marcaDe('paleta'), { invalido: 'sim', frase: 'esta paleta não combina com o tema', aria: 'true' }, 'a escolha não mostrou o erro do MOD');
    assert.deepEqual(await marcaDe('acento'), { invalido: 'sim', frase: 'contraste 1,2 contra o fundo', aria: 'true' }, 'a cor não mostrou o erro do MOD');
    await page.evaluate((arvore) => pedir(6, { tipo: 'superficie-montar', superficie: 'cores', arvore }), arvoreDasCores(false));
    assert.deepEqual([await marcaDe('paleta'), await marcaDe('acento')], [{ invalido: null, frase: null, aria: null }, { invalido: null, frase: null, aria: null }],
      'o erro redeclarado como nenhum não saiu da escolha e da cor');
    const pelasChaves = await page.evaluate(() => pedir(6, { tipo: 'formulario-erros', superficie: 'cores', erros: { paleta: 'recusada pelo servidor', acento: 'recusada pelo servidor' } }));
    assert.equal(pelasChaves?.ok, true, `formulario.erros não alcança a escolha e a cor: ${JSON.stringify(pelasChaves)}`);
    assert.deepEqual([(await marcaDe('paleta')).frase, (await marcaDe('acento')).frase], ['recusada pelo servidor', 'recusada pelo servidor'], 'formulario.erros não marcou a escolha e a cor');
    await page.evaluate(() => pedir(6, { tipo: 'formulario-erros', superficie: 'cores', erros: {} }));
    // O hexadecimal incompleto: marcado, sem frase; completo, desmarcado.
    await page.fill('[data-superficie="cores"] [data-chave-do-mod="acento"] .regiao-de-mod-hexa', '#f2');
    assert.deepEqual(await marcaDe('acento'), { invalido: 'sim', frase: null, aria: 'true' }, 'o hexadecimal incompleto deixou de ficar marcado');
    await page.fill('[data-superficie="cores"] [data-chave-do-mod="acento"] .regiao-de-mod-hexa', '#ff6a00');
    assert.deepEqual(await marcaDe('acento'), { invalido: null, frase: null, aria: null }, 'o hexadecimal completo continuou marcado');
    // E a escolha mexida suja a superfície.
    await page.selectOption('[data-superficie="cores"] [data-chave-do-mod="paleta"] select', 'dia');
    await page.click('[data-superficie="cores"] .superficie-de-mod-sair');
    assert.equal(await page.evaluate(() => !$('moderar').hidden), true, 'trocar a escolha não sujou a superfície');
    await page.click('#moderar-confirmar');

    assert.deepEqual(errors, []);
    console.log('camadas: a folha empilha três, a de cima aparece, a quarta e a reaberta são recusadas com o número, a saída e o descartou são do produto; o painel na 6 é recusado com «use folha», e até a 5 continua; o teto de superfícies chega à 6 pelo nome; e nenhum título quebra dentro de uma palavra a 300 px; a mídia por volume, nos cinco lugares, lê pelo ler_imagem_mod; e a ficha que não cabe numa mensagem se monta por partes; e o formulário barra o envio que viola uma regra declarada; e o modal deixa acordado o que a casca pede; e a recusa do servidor volta aos campos; e o sujo é derivado do inicial; e a ação destrutiva só chega ao MOD depois do sim; e o erro vale na escolha e na cor.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
