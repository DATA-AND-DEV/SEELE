// A capa de uma pessoa (`pessoa.capa`; rascunho da API 6, §4; tarefa 6 da F3),
// com o HTML, o CSS, o registro e o carregador reais; só a ponte nativa é
// simulada. No molde de `avatares-do-mod.cjs`.
//
// # O que ela mede
//
// 1. Registrada para uma pessoa, a capa pinta o cartão dela na chamada: o
//    cartão ganha `data-pessoa-da-capa`, e a imagem é a do MOD.
// 2. **O halo de quem fala e a pastilha ficam acima da capa**: no centro da
//    pastilha, o elemento que recebe o clique é ela, e não a capa.
// 3. **A capa recusada** deixa o cartão com o fundo liso, e o MOD recebe o
//    `midia` com o ponto e o alvo (rascunho §7.1).
// 4. **Com movimento reduzido**, o cartão usa o quadro estático, e não a
//    imagem que se move.
// 5. **O cabeçalho SEU PERFIL do desktop veste a capa de quem usa** (D-capa-1,
//    do dono; rascunho §4, «o SEU PERFIL nativo, por um aplicador do
//    produto»; achado da F8, 10/10): o título e o FECHAR ficam sobre placas, e
//    não perdem contraste sobre as três capas de prova.
//
// Rodar:  node apps/seele-app/bancada/capa.cjs

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
          if (cmd === 'ler_imagem_mod' && window.recusarCapa) return { papel: 'som', bytes: 64, uri: 'data:audio/wav;base64,' };
          return testTable[cmd] ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(r => {
      window.foto = cor => {
        const canvas = document.createElement('canvas');
        canvas.width = 96;
        canvas.height = 32;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = cor; ctx.fillRect(0, 0, 96, 32);
        // JPEG: o quadro estático sai em PNG, e as duas não se confundem.
        return canvas.toDataURL('image/jpeg');
      };
      window.daCapa = foto('#32e6a2');
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      testTable.snapshot = r;
      $('tela-boot').hidden = true;
      $('tela-sessao').hidden = false;
      desenhar(r);
      desenharChamada(r);
      // A vista da chamada na frente: o cartão precisa de caixa para ser medido.
      $('vista-conversa').hidden = true;
      $('vista-chamada').hidden = false;
      entrarNaGeracao(1);
      window.entregues = [];
      window.instanciaTeste = new InstanciaDeMod('teste/perfis', 'hash', geracaoDaSessao, {
        entregar: async m => { entregues.push(m); },
        encerrar: async () => {},
      });
      instanciaTeste.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set('teste/perfis', instanciaTeste);
      window.registrarCapa = (alvo, path = 'volume:capa') => contribuicoesDosMods.registrar(
        { id: 'teste/perfis' }, instanciaTeste,
        { ponto: 'pessoa.capa', modo: 'substituir', alvo,
          conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path } } } },
      ).handle;
    }, retrato());
    // Lia (id 2) fala no retrato da bancada? Quem falar, o cartão dela é o que
    // mostra o halo; a capa vai para ela.
    const alvo = await page.evaluate(() => {
      const pessoas = testTable.snapshot.voice_rooms.flatMap(s => s.people);
      return String((pessoas.find(p => !p.is_self) ?? pessoas[0]).id);
    });
    const cartao = `.chamada-cartao[data-pessoa-da-capa="${alvo}"]`;

    // **A legibilidade, medida** (rascunho §4; tarefa 6, Step 4): todo texto do
    // produto sobre a capa mantém pelo menos o contraste que tem sem ela,
    // conferido contra uma capa branca, uma preta e uma **da cor da própria
    // tinta** — a pior que existe para cada texto, controle ou borda —, com a
    // pessoa calada e falando. O fundo efetivo de um texto é a primeira cor
    // opaca entre ele e o cartão; sem nenhuma, é a capa.
    //
    // A terceira capa não é enfeite: o cinza apagado do MUDO, sem placa,
    // **ganha** contraste sobre a branca (4,11 → 4,86) e sobre a preta (→ 4,32),
    // porque fica entre as duas; só a capa da cor dele mostra que, sem placa,
    // ele some (→ 1). Medido na tarefa 6.
    const CAPAS_DE_PROVA = [['branca', [255, 255, 255]], ['preta', [0, 0, 0]], ['da cor da tinta', 'tinta']];
    const falar = (sim) => page.evaluate(([alvo, sim]) => {
      const r = testTable.snapshot;
      for (const p of r.voice_rooms.flatMap(s => s.people)) if (String(p.id) === alvo) p.speaking = sim;
      desenharChamada(r);
    }, [alvo, sim]);
    const medirEm = (seletorDoQuadro, cor) => page.evaluate(([seletor, corDaCapa]) => {
      const quadro = document.querySelector(seletor);
      const rgb = (texto) => (texto.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) => [r, g, b]
        .map((c) => { const v = c / 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; })
        .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
      // **A capa é pintada por cima do fundo do próprio quadro** (ela é a
      // primeira camada dele): chegando ao quadro, o fundo é a capa, e não a cor
      // dele. Olhar a cor do quadro primeiro diria «painel» na barra do
      // operador, que tem fundo opaco, onde se vê a capa.
      const fundo = (el, tinta) => {
        for (let n = el; n; n = n.parentElement) {
          if (n === quadro && corDaCapa) return corDaCapa === 'tinta' ? tinta : corDaCapa;
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] >= 0.99)) return c.slice(0, 3);
        }
        return [0, 0, 0];
      };
      // Os textos, e os controles — botões e glifos —, com a cor e a borda.
      const medidos = [...quadro.querySelectorAll('*')].filter((e) => e.getClientRects().length && (
        [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())
        || e.tagName === 'BUTTON' || e.hasAttribute('data-glifo')));
      // A cor de um controle só conta quando ele pinta com ela: texto próprio,
      // um glifo, ou um botão sem texto dentro (o microfone, o fone). Um botão
      // que embrulha textos — a identidade de quem usa — não pinta a cor dele;
      // os textos de dentro são medidos cada um.
      const temTexto = (e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      const pintaCor = (e) => temTexto(e) || e.hasAttribute('data-glifo')
        || (e.tagName === 'BUTTON' && ![...e.querySelectorAll('*')].some(temTexto));
      const saida = {};
      medidos.forEach((e, i) => {
        const estilo = getComputedStyle(e);
        const cor = rgb(estilo.color).slice(0, 3);
        if (pintaCor(e)) saida[`${e.id || e.className}#${i}`] = Number(razao(cor, fundo(e, cor)).toFixed(2));
        const borda = rgb(estilo.borderTopColor);
        if (parseFloat(estilo.borderTopWidth) > 0 && borda.length >= 3 && (borda.length < 4 || borda[3] > 0)) {
          saida[`${e.id || e.className}#${i} (borda)`] = Number(razao(borda.slice(0, 3), fundo(e, borda.slice(0, 3))).toFixed(2));
        }
      });
      return saida;
    }, [seletorDoQuadro, cor]);
    const medir = (cor) => medirEm(`.chamada-cartao[data-pessoa-da-capa="${alvo}"]`, cor);

    await falar(false);
    const semCapa = { calado: await medir(null) };
    await falar(true);
    semCapa.falando = await medir(null);
    await falar(false);

    // 1. A capa pinta o cartão.
    const registro = await page.evaluate(alvo => {
      try {
        window.handleCapa = registrarCapa(alvo);
        desenharChamada(testTable.snapshot);
        return '';
      } catch (erro) { return erro.message; }
    }, alvo);
    assert.equal(registro, '', `pessoa.capa foi recusada no registro: ${registro}`);
    await page.waitForFunction(seletor => {
      const el = document.querySelector(seletor);
      const camada = el?.querySelector(':scope > .capa-camada');
      return camada && getComputedStyle(camada).backgroundImage.includes(window.daCapa);
    }, cartao, { timeout: 5000 });

    // A legibilidade, com as três capas de prova, calado e falando.
    const pioras = [];
    for (const [nomeDaCapa, cor] of CAPAS_DE_PROVA) {
      for (const sim of [false, true]) {
        await falar(sim);
        const com = await medir(cor);
        const sem = semCapa[sim ? 'falando' : 'calado'];
        for (const [texto, antes] of Object.entries(sem)) {
          if ((com[texto] ?? 0) < antes - 0.01) pioras.push(`${texto} (${sim ? 'falando' : 'calado'}, capa ${nomeDaCapa}): ${antes} → ${com[texto]}`);
        }
      }
    }
    await falar(false);
    assert.deepEqual(pioras, [], 'um texto do produto perdeu contraste sobre a capa');

    // E a capa de quem usa, na barra do operador: medida antes sem capa, para
    // a comparação de cada texto e controle.
    const barraSemCapa = await medirEm('#tela-sessao .operador', null);
    // O cabeçalho do perfil, sem capa, para a mesma comparação (o caso 5).
    await page.evaluate(() => abrirPerfil());
    const perfilSemCapa = await medirEm('#perfil .perfil-cabeca', null);
    await page.evaluate(() => fecharPerfil());
    const nucleo = await page.evaluate(() => {
      contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.capa', modo: 'substituir', alvo: String(testTable.snapshot.me),
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:eu' } } },
      });
      desenhar(testTable.snapshot);
      return new Promise(ok => setTimeout(() => {
        const barra = document.querySelector('#tela-sessao .operador');
        const camada = barra?.querySelector(':scope > .capa-camada')?.getBoundingClientRect();
        // Contra a caixa de dentro da barra (`client*`): a borda de 1 px dela é
        // do produto, e a capa cobre o resto.
        ok({
          vestida: Boolean(barra?.dataset.comCapa),
          cobre: Boolean(camada && barra && Math.abs(camada.width - barra.clientWidth) < 1
            && Math.abs(camada.height - barra.clientHeight) < 1),
        });
      }, 500));
    });
    // **A capa de quem usa pinta o fundo da barra inteira** (o pedido do dono,
    // reafirmado em 08/10), e o MUDO, o fone e o SAIR ficam sobre placas
    // opacas do núcleo: cada texto e cada controle — a cor e a borda — mantém o
    // contraste que tinha sem capa, com as três capas de prova.
    assert.ok(nucleo.vestida && nucleo.cobre, `a capa de quem usa não pinta a barra inteira: ${JSON.stringify(nucleo)}`);
    for (const [nomeDaCapa, cor] of CAPAS_DE_PROVA) {
      const com = await medirEm('#tela-sessao .operador', cor);
      for (const [medido, antes] of Object.entries(barraSemCapa)) {
        if ((com[medido] ?? 0) < antes - 0.01) pioras.push(`operador ${medido} (capa ${nomeDaCapa}): ${antes} → ${com[medido]}`);
      }
    }
    assert.deepEqual(pioras, [], 'um texto ou controle da barra do operador perdeu contraste sobre a capa');

    // 5. O cabeçalho SEU PERFIL, com a capa de quem usa já registrada.
    await page.evaluate(() => abrirPerfil());
    const perfil = await page.evaluate(() => new Promise(ok => setTimeout(() => {
      const cabeca = document.querySelector('#perfil .perfil-cabeca');
      const camada = cabeca?.querySelector(':scope > .capa-camada')?.getBoundingClientRect();
      ok({
        vestida: cabeca?.dataset.comCapa === 'sim',
        daPessoa: cabeca?.dataset.pessoaDaCapa,
        eu: String(testTable.snapshot.me),
        cobre: Boolean(camada && cabeca && Math.abs(camada.width - cabeca.clientWidth) < 1
          && Math.abs(camada.height - cabeca.clientHeight) < 1),
      });
    }, 300)));
    assert.ok(perfil.vestida && perfil.cobre && perfil.daPessoa === perfil.eu,
      `o cabeçalho SEU PERFIL não veste a capa de quem usa: ${JSON.stringify(perfil)}`);
    for (const [nomeDaCapa, cor] of CAPAS_DE_PROVA) {
      const com = await medirEm('#perfil .perfil-cabeca', cor);
      for (const [medido, antes] of Object.entries(perfilSemCapa)) {
        if ((com[medido] ?? 0) < antes - 0.01) pioras.push(`perfil ${medido} (capa ${nomeDaCapa}): ${antes} → ${com[medido]}`);
      }
    }
    await page.evaluate(() => fecharPerfil());
    assert.deepEqual(pioras, [], 'o título ou o FECHAR do SEU PERFIL perdeu contraste sobre a capa');

    // 2. A pastilha e o nome ficam acima da capa: no centro de cada um, o
    //    elemento que recebe o clique é ele (ou um filho dele).
    const porCima = await page.evaluate(seletor => {
      const el = document.querySelector(seletor);
      return ['.chamada-pastilha:not([hidden])', '.chamada-cartao-nome'].map(alvoInterno => {
        const no = el.querySelector(alvoInterno);
        if (!no) return `${alvoInterno}: ausente`;
        const caixa = no.getBoundingClientRect();
        const noCentro = document.elementFromPoint(caixa.left + caixa.width / 2, caixa.top + caixa.height / 2);
        return no.contains(noCentro) ? '' : `${alvoInterno}: coberto por ${noCentro?.className}`;
      }).filter(Boolean);
    }, cartao);
    assert.deepEqual(porCima, [], 'a capa ficou por cima do que o produto escreve no cartão');

    // 3. Com movimento reduzido, o quadro estático.
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.evaluate(() => redesenharCapas());
    const comMovimentoReduzido = await page.evaluate(seletor => {
      const camada = document.querySelector(seletor)?.querySelector(':scope > .capa-camada');
      const fundo = camada ? getComputedStyle(camada).backgroundImage : '';
      return { temImagem: fundo.startsWith('url('), eAOriginal: fundo.includes(window.daCapa) };
    }, cartao);
    assert.ok(comMovimentoReduzido.temImagem && !comMovimentoReduzido.eAOriginal,
      `com movimento reduzido, o cartão não usou o quadro estático: ${JSON.stringify(comMovimentoReduzido)}`);
    await page.emulateMedia({ reducedMotion: 'no-preference' });

    // 4. A capa recusada: fundo liso, e o MOD sabe, com o ponto e o alvo.
    await page.evaluate(alvo => {
      contribuicoesDosMods.revogar(handleCapa);
      window.recusarCapa = true;
      entregues.length = 0;
      window.handleCapa = registrarCapa(alvo, 'volume:recusada');
      desenharChamada(testTable.snapshot);
    }, alvo);
    await page.waitForFunction(() => entregues.some(m => m.nome === 'midia'), null, { timeout: 5000 });
    const recusada = await page.evaluate(seletor => {
      const el = document.querySelector(seletor);
      const camada = el?.querySelector(':scope > .capa-camada');
      return {
        fundo: camada ? getComputedStyle(camada).backgroundImage : 'none',
        comCapa: el?.dataset.comCapa ?? null,
        aviso: entregues.find(m => m.nome === 'midia') ?? null,
      };
    }, cartao);
    assert.ok(!recusada.fundo.startsWith('url(') && !recusada.comCapa,
      `a capa recusada deixou imagem no cartão: ${JSON.stringify(recusada)}`);
    assert.equal(recusada.aviso?.ponto, 'pessoa.capa', `o midia não disse o ponto: ${JSON.stringify(recusada.aviso)}`);
    assert.equal(String(recusada.aviso?.alvo), alvo, `o midia não disse o alvo: ${JSON.stringify(recusada.aviso)}`);

    // 6. **O uso e o bolso** (rascunho §4 e §12; tarefa 6, Step 5b): a capa pede
    //    ao Rust o uso `"capa"`, que a reduz ao lugar dela, e o bolso da capa
    //    conta os bytes **decodificados** que ele devolve — o custo na janela é o
    //    quadro, e não o arquivo. O avatar não nomeia uso.
    const bolso = await page.evaluate(async (alvo) => {
      window.recusarCapa = false;
      const esperar = () => new Promise(ok => setTimeout(ok, 400));
      const ultimoPedido = () => testCalls.filter(c => c.cmd === 'ler_imagem_mod').at(-1)?.args ?? {};
      contribuicoesDosMods.revogar(handleCapa);
      entregues.length = 0;
      // Um arquivo grande que decodifica pouco: aceita.
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 20 * 1024 * 1024, uri: daCapa, decodificados: 1024 * 1024 };
      window.handleCapa = registrarCapa(alvo, 'volume:leve');
      desenharChamada(testTable.snapshot);
      await esperar();
      const pediuOUso = ultimoPedido().uso;
      const leveAceita = Boolean(document.querySelector(`[data-pessoa-da-capa="${alvo}"][data-com-capa]`));
      // Um arquivo pequeno que decodifica 200 MiB: recusada, e dita.
      contribuicoesDosMods.revogar(handleCapa);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa, decodificados: 200 * 1024 * 1024 };
      window.handleCapa = registrarCapa(alvo, 'volume:pesada');
      desenharChamada(testTable.snapshot);
      await esperar();
      const pesadaRecusada = !document.querySelector(`[data-pessoa-da-capa="${alvo}"][data-com-capa]`)
        && entregues.some(m => m.nome === 'midia' && m.ponto === 'pessoa.capa');
      // O avatar não nomeia uso.
      const avatar = contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.avatar', modo: 'substituir', alvo,
        conteudo: { doServidor: { canal: 1, campo: 'image', pedido: { transporte: 'volume', path: 'volume:avatar' } } },
      }).handle;
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      desenhar(testTable.snapshot);
      await esperar();
      const avatarSemUso = ultimoPedido().uso;
      // Revogados os dois: a seção seguinte cita um avatar que a pessoa não tem.
      contribuicoesDosMods.revogar(avatar);
      contribuicoesDosMods.revogar(handleCapa);
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      return { pediuOUso, leveAceita, pesadaRecusada, avatarSemUso: avatarSemUso ?? null };
    }, alvo);
    assert.equal(bolso.pediuOUso, 'capa', `a capa não pediu o uso ao Rust: ${JSON.stringify(bolso)}`);
    assert.ok(bolso.leveAceita, `a capa de arquivo grande e pouco decodificada foi recusada: ${JSON.stringify(bolso)}`);
    assert.ok(bolso.pesadaRecusada, `a capa de 200 MiB decodificados não foi recusada e dita: ${JSON.stringify(bolso)}`);
    assert.equal(bolso.avatarSemUso, null, `o avatar nomeou um uso: ${JSON.stringify(bolso)}`);

    // 7. **No máximo seis capas animadas ao mesmo tempo** (rascunho §4; tarefa
    //    6, Step 5c): escolhidas pela vista — as visíveis, e entre elas quem
    //    falou por último. As outras mostram o quadro parado. Oito pessoas na
    //    sala e a capa de quem usa: nove capas; a última da grade é quem falou.
    const animadas = await page.evaluate(async () => {
      window.recusarCapa = false;
      testTable.ler_imagem_mod = { papel: 'imagem', bytes: 256, uri: daCapa };
      const r = testTable.snapshot;
      const sala = r.voice_rooms[0];
      const modelo = sala.people.find(p => !p.is_self);
      const deAntes = sala.people.slice();
      for (let id = 10; id < 17; id += 1) sala.people.push({ ...modelo, id, nickname: `p${id}`, speaking: false });
      const ids = sala.people.map(p => String(p.id));
      // A de quem usa já está de pé (a seção do núcleo a registrou), e uma
      // segunda seria `contribuicao-ja-existe`.
      const handles = ids.filter(id => id !== String(r.me)).map(id => registrarCapa(id, `volume:capa-${id}`));
      desenhar(r);
      desenharChamada(r);
      await new Promise(ok => setTimeout(ok, 600));
      // Quem fala agora é a última da grade.
      sala.people.at(-1).speaking = true;
      desenharChamada(r);
      await new Promise(ok => setTimeout(ok, 300));
      sala.people.at(-1).speaking = false;
      desenharChamada(r);
      await new Promise(ok => setTimeout(ok, 300));
      const capas = [...document.querySelectorAll('[data-com-capa]')];
      const visiveis = capas.filter(e => {
        const c = e.getBoundingClientRect();
        return c.width > 0 && c.height > 0 && c.bottom > 0 && c.top < innerHeight;
      });
      const usaOriginal = e => e.querySelector(':scope > .capa-camada')?.style.backgroundImage.includes(daCapa);
      const ultima = document.querySelector(`.chamada-cartao[data-pessoa-da-capa="${ids.at(-1)}"]`);
      const resultado = {
        capas: capas.length,
        visiveis: visiveis.length,
        animadas: capas.filter(usaOriginal).length,
        animadasFora: capas.filter(e => !visiveis.includes(e) && usaOriginal(e)).length,
        quemFalouAnima: Boolean(ultima && usaOriginal(ultima)),
      };
      for (const h of handles) contribuicoesDosMods.revogar(h);
      sala.people.splice(0, sala.people.length, ...deAntes);
      desenhar(r);
      desenharChamada(r);
      return resultado;
    });
    assert.ok(animadas.capas >= 9, `só ${animadas.capas} capas na tela: o cenário não se montou`);
    assert.equal(animadas.animadas, Math.min(6, animadas.visiveis), `animam ${JSON.stringify(animadas)}`);
    assert.equal(animadas.animadasFora, 0, `uma capa fora da vista anima: ${JSON.stringify(animadas)}`);
    assert.ok(animadas.quemFalouAnima, `quem falou por último não anima: ${JSON.stringify(animadas)}`);

    // 5. **A citação `daPessoa`** (rascunho §4; tarefa 6, Step 4b; o bloqueio A
    //    do PERFIS): um cartão que cita a capa da pessoa usa a imagem já
    //    carregada — nenhum pedido de mídia novo, e os bytes contam uma vez, na
    //    contribuição da capa. Sem contribuição para a imagem citada, nada: o
    //    retrato fica na inicial, sem erro.
    const citacao = await page.evaluate(async (alvo) => {
      window.recusarCapa = false;
      contribuicoesDosMods.revogar(handleCapa);
      window.handleCapa = registrarCapa(alvo, 'volume:citada');
      desenharChamada(testTable.snapshot);
      await new Promise(ok => setTimeout(ok, 400));
      const pedidosAntes = testCalls.filter(c => c.cmd === 'ler_imagem_mod').length;
      const ditos = [];
      const registrarDeAntes = registrarNoAnfitriao;
      registrarNoAnfitriao = (...argumentos) => { ditos.push(String(argumentos[1])); return registrarDeAntes(...argumentos); };
      const cartao = contribuicoesDosMods.registrar({ id: 'teste/perfis' }, instanciaTeste, {
        ponto: 'pessoa.cartao', modo: 'substituir', alvo,
        conteudo: [{
          forma: 'caixa', chave: 'faixa', fundoDeMidia: { daPessoa: 'capa' },
          dentro: [{ forma: 'texto', chave: 't', dentro: 'cartão' }, { forma: 'retrato', chave: 'r', daPessoa: 'avatar' }],
        }],
      }).handle;
      desenhar(testTable.snapshot);
      await new Promise(ok => setTimeout(ok, 400));
      registrarNoAnfitriao = registrarDeAntes;
      const linha = document.querySelector('li.pessoa-apresentada[data-apresentada-por="teste/perfis"]');
      const comFundo = [...(linha?.querySelectorAll('.pessoa-miolo *') ?? [])].some(e => e.style.backgroundImage.includes(window.daCapa));
      const retrato = linha?.querySelector('.pessoa-miolo [data-estado-do-retrato]');
      const montagem = contribuicoesDosMods.porHandle.get(cartao)?.montadas?.get(alvo);
      const bytesNoCartao = montagem
        ? Object.entries(montagem.renderer).filter(([k, v]) => /^bytes/.test(k) && typeof v === 'number').reduce((a, [, v]) => a + v, 0)
        : null;
      contribuicoesDosMods.revogar(cartao);
      return {
        comFundo,
        pedidosNovos: testCalls.filter(c => c.cmd === 'ler_imagem_mod').length - pedidosAntes,
        bytesNoCartao,
        retrato: retrato?.dataset.estadoDoRetrato ?? null,
        recusas: ditos.filter(d => /retrato|fundo/.test(d)),
      };
    }, alvo);
    assert.ok(citacao.comFundo, `o cartão que cita a capa não a mostrou: ${JSON.stringify(citacao)}`);
    assert.equal(citacao.pedidosNovos, 0, `citar a capa abriu ${citacao.pedidosNovos} pedido(s) de mídia novo(s)`);
    assert.equal(citacao.bytesNoCartao, 0, `a capa citada contou ${citacao.bytesNoCartao} bytes de novo no cartão`);
    assert.equal(citacao.retrato, 'inicial', `o retrato que cita um avatar que não existe saiu ${citacao.retrato}`);
    assert.deepEqual(citacao.recusas, [], 'citar uma imagem que a pessoa não tem virou recusa');

    assert.deepEqual(errors, []);
    console.log('capa: o cartão pintado, o texto e o núcleo legíveis sobre as placas, o SEU PERFIL vestido, o quadro estático com movimento reduzido, a recusa dita com o ponto e o alvo, e a citação daPessoa sem pedido novo.');
  } finally {
    await browser.close();
    await new Promise(ok => server.close(ok));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
