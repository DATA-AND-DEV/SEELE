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

    assert.deepEqual(errors, []);
    console.log('camadas: a folha empilha três, a de cima aparece, a quarta e a reaberta são recusadas com o número, a saída e o descartou são do produto; o painel na 6 é recusado com «use folha», e até a 5 continua; o teto de superfícies chega à 6 pelo nome; e nenhum título quebra dentro de uma palavra a 300 px; a mídia por volume, nos cinco lugares, lê pelo ler_imagem_mod; e a ficha que não cabe numa mensagem se monta por partes.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
