// **A lista de pessoas reconciliada pelo id** (pendência 63, a parte (b); lote
// AX da F5).
//
// Cada pessoa manda um `PersonState` por segundo, e a janela respondia a cada
// retrato refazendo a lista inteira: todos os nós trocados a cada quadro. Na
// árvore de acessibilidade isso é a lista sumindo e voltando — medido no app
// pela F3 em 11/10, com 50 pessoas a lista inteira estava na árvore em 18 de 38
// consultas (`docs/evidencias/m3-custo-dos-mods/arvore-ax.py`). A sonda mede o
// sintoma no app de verdade; esta bancada prende a causa, no navegador, com o
// HTML, o CSS e o registro reais e só a ponte simulada.
//
// # O que ela mede, com 50 pessoas (20 numa sala, 30 fora das salas)
//
// 1. **Um `PersonState` mexe só na linha daquela pessoa**, pelo caminho do
//    evento (`RosterChanged` e o `atualizar()` do quadro): toda mutação da lista
//    cai dentro da linha dela, cada linha é o mesmo nó (`===`), a linha dela diz
//    o estado novo, e o foco de quem navega pela lista fica onde estava.
// 2. **O mesmo retrato não mexe em nada**: com o tique de 500 ms redesenhando
//    duas vezes, nenhuma mutação na lista — e o retrato desenhado é outro
//    objeto no fim, senão um tique parado passaria sem medir nada.
// 3. **Quem entra numa sala leva o próprio nó**: a linha é movida para o grupo
//    da sala, e não recriada; as outras ficam.
// 4. **Quem sai perde o nó, e quem chega ganha um**; as outras ficam.
// 5. **Os nós de MOD, ponto de montagem por ponto**, cada um numa página nova:
//    `pessoa.cartao` substituir com conteúdo, o mesmo sem conteúdo com o
//    cartão de uma região da API 3 (o caminho do PERFIS), os cartões da API 3
//    na linha nativa (nos três ramos: FORA DE SALA, as salas, o saguão),
//    `pessoa.cartao` adicionar, `pessoa.identidade`, `pessoa.detalhes` e
//    `sala.acoes`. Um `PersonState` não tira nenhum nó de MOD do documento —
//    o observador vê os removidos —, cada um é o mesmo nó na mesma linha, e
//    nenhuma mutação cai fora da linha da pessoa. Um marcador tirado de um
//    ponto só reprova aqui.
// 6. **A coluna de salas**, que também lista quem está em cada sala e tem o
//    ENTRAR NA SALA: um `PersonState` mexe só na linha da pessoa nela, o
//    ENTRAR NA SALA de outra sala é o mesmo nó, e o mesmo retrato não mexe em
//    nada. Com 100 pessoas, a sonda viu o botão sair da árvore com a coluna
//    refeita a cada quadro.
// 7. **Ainda nela**: quem sai de uma sala tira só a linha dela, e o nome de
//    uma sala renomeada — botão com ouvinte próprio, que agora atravessa os
//    retratos — abre o RENOMEAR com o nome de agora, e não o do quadro em que
//    o botão nasceu.
// 8. **A ordem é a do retrato** — depois de um PersonState, de uma entrada,
//    de uma saída e de uma chegada, e depois de uma reordenação (a primeira da
//    sala vai para o fim), na lista e na coluna de salas —, e cada linha é o
//    mesmo nó.
//
// Com o conserto revertido (a lista repovoada a cada retrato), o caso 1 conta
// mutações fora da linha e nós trocados em todas as outras; com a coluna de
// salas repovoada, o caso 6.
//
// Rodar:  node apps/seele-app/bancada/linhas-pelo-id.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const PESSOAS = 50;
const NA_SALA = 20;

const apelido = (id) => `p-${String(id).padStart(3, "0")}`;
const pessoa = (id, extra = {}) => ({
  id,
  nickname: apelido(id),
  speaking: false,
  muted: false,
  total_isolation: false,
  signal: 92,
  sync_band: "Nominal",
  is_self: id === 1,
  ...extra,
});

/** O retrato com quem está em cada sala e quem está no servidor. */
function retratoCom({ sala1, sala2 = [], presentes }) {
  const sala = (id, name, people, nossa, channel) => ({
    id, name, limit: 64, password_required: false, occupied_by_us: nossa, channel, people, sync: null,
  });
  return retrato({
    voice_rooms: [
      sala(1, "Sala principal", sala1, true, 1),
      sala(2, "Sala dois", sala2, false, 2),
    ],
    presentes,
  });
}

const todas = Array.from({ length: PESSOAS }, (_, i) => pessoa(i + 1));
const BASE = retratoCom({ sala1: todas.slice(0, NA_SALA), presentes: todas });

/**
 * Uma página nova, com a ponte simulada, os ajudantes dos casos e a sessão
 * desenhada a partir de `r`. Os casos de MOD abrem uma cada: um ponto de
 * montagem por vez, sem o registro de um caso anterior.
 */
async function abrir(browser, url, r) {
    const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript((table) => {
      window.testTable = table;
      const ouvintes = new Map();
      window.emitir = (payload) => {
        for (const fn of ouvintes.get("seele://event") ?? []) fn({ payload });
      };
      window.__TAURI__ = {
        core: {
          invoke: async (cmd) => {
            if (cmd === "snapshot") return JSON.parse(JSON.stringify(testTable.snapshot));
            return testTable[cmd] ?? null;
          },
        },
        event: {
          listen: async (nome, fn) => {
            if (!ouvintes.has(nome)) ouvintes.set(nome, []);
            ouvintes.get(nome).push(fn);
            return () => ouvintes.set(nome, ouvintes.get(nome).filter((f) => f !== fn));
          },
        },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };

      // As linhas pelo apelido, que nesta bancada é único.
      window.linhasPorApelido = () => {
        const linhas = new Map();
        for (const li of document.querySelectorAll("#lista-roster li.pessoa")) {
          const nome = li.querySelector(".pessoa-nome")?.textContent ?? "";
          linhas.set(nome.replace(" (você)", ""), li);
        }
        return linhas;
      };
      // Toda mutação da lista, de agora até `parar()`.
      window.observarALista = () => {
        const registros = [];
        const observador = new MutationObserver((rs) => registros.push(...rs));
        observador.observe(document.getElementById("lista-roster"), {
          subtree: true, childList: true, attributes: true, characterData: true,
        });
        return () => {
          registros.push(...observador.takeRecords());
          observador.disconnect();
          return registros;
        };
      };
      // De quem é a linha em que uma mutação caiu, ou `null` fora de linha.
      window.donoDaMutacao = (registro) => {
        const alvo = registro.target.nodeType === Node.ELEMENT_NODE
          ? registro.target
          : registro.target.parentElement;
        const linha = alvo?.closest?.("li.pessoa");
        if (!linha) return null;
        return (linha.querySelector(".pessoa-nome")?.textContent ?? "?").replace(" (você)", "");
      };
      // Um MOD de pé com estas contribuições, pelo registro de verdade.
      window.registrarMod = (id, contribuicoes) => {
        if (typeof geracaoDaSessao !== "number" || geracaoDaSessao < 1) entrarNaGeracao(1);
        const instancia = new InstanciaDeMod(id, "hash", geracaoDaSessao, {
          entregar: async () => {},
          encerrar: async () => {},
        });
        instancia.estado = ESTADOS_DE_MOD.ativa;
        modsCarregados.set(id, instancia);
        for (const c of contribuicoes) contribuicoesDosMods.registrar({ id }, instancia, c);
        return instancia;
      };
      // Os cartões de uma região da API 3 (`SeeleUI.cartoes`, o caminho do
      // PERFIS), simulada: um nó por pessoa, montado uma vez e devolvido igual,
      // como `RegiaoDeMod#cartaoDe`.
      window.regiaoDeCartoes = (id) => {
        const raizes = new Map();
        cartoesDosMods.set(id, {
          id,
          cartaoDe(pessoa) {
            const chave = String(pessoa);
            if (!raizes.has(chave)) {
              const raiz = document.createElement("div");
              raiz.className = "pessoa-cartao";
              raiz.dataset.mod = id;
              raiz.textContent = `cartão da região para ${chave}`;
              raizes.set(chave, raiz);
            }
            return raizes.get(chave);
          },
          midiasAnotadas: () => [],
        });
        redesenharAsPessoas();
      };
      // A ordem das linhas que o retrato pede: FORA DE SALA, as salas com
      // gente, e quem está no servidor fora delas.
      window.ordemDoRetrato = (r) => {
        const sentados = new Set(r.voice_rooms.flatMap((s) => s.people.map((p) => p.id)));
        const eu = r.presentes.find((p) => p.is_self);
        return [
          ...(r.voice_rooms.some((s) => s.occupied_by_us) || !eu ? [] : [eu.nickname]),
          ...r.voice_rooms.flatMap((s) => s.people.map((p) => p.nickname)),
          ...r.presentes.filter((p) => !sentados.has(p.id) && !p.is_self).map((p) => p.nickname),
        ];
      };
      window.ordemNaTela = () => [...document.querySelectorAll("#lista-roster li.pessoa")]
        .map((li) => (li.querySelector(".pessoa-nome")?.textContent ?? "").replace(" (você)", ""));
      window.ordemNaColuna = () => [...document.querySelectorAll("#lista-voice_rooms .voice_room-pessoa-nome")]
        .map((n) => n.textContent);
      window.quadros = async (n) => {
        for (let i = 0; i < n; i += 1) await new Promise((ok) => requestAnimationFrame(() => ok()));
      };
      // Um retrato novo pelo caminho do evento, e espera até ele ser o desenhado.
      window.pelaPonte = async (retrato, chegou) => {
        testTable.snapshot = retrato;
        emitir("RosterChanged");
        for (let i = 0; i < 60 && !chegou(desenhado); i += 1) await quadros(1);
        await quadros(2);
        return chegou(desenhado);
      };
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.goto(url);
    await page.evaluate((r) => {
      testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, r);
    return { page, errors };
}

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium().launch({ headless: true });
  try {
    const { page, errors } = await abrir(browser, url, BASE);
    const contadas = await page.evaluate(() => linhasPorApelido().size);
    assert.equal(contadas, PESSOAS, `a lista desenhou ${contadas} linhas, e o retrato tem ${PESSOAS} pessoas`);

    // 1. Um PersonState de uma pessoa da sala, pelo caminho do evento.
    const caso1 = await page.evaluate(async ({ base, alvo, foco }) => {
      const antes = linhasPorApelido();
      // O foco de quem navega pela lista, no nome de outra pessoa.
      const botao = antes.get(foco).querySelector("button.pessoa-nome-porta");
      botao?.focus();
      const focado = document.activeElement === botao;
      const parar = observarALista();
      const novo = structuredClone(base);
      const quem = novo.voice_rooms[0].people.find((p) => p.nickname === alvo);
      quem.speaking = true;
      quem.signal = 61;
      quem.sync_band = "Degraded";
      novo.presentes.find((p) => p.nickname === alvo).speaking = true;
      const chegou = await pelaPonte(novo, (d) =>
        d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === alvo)?.speaking === true);
      const registros = parar();
      const depois = linhasPorApelido();
      const donos = registros.map(donoDaMutacao);
      return {
        chegou,
        focado,
        focoFicou: focado && document.activeElement === botao,
        mutacoes: registros.length,
        foraDaLinha: donos.filter((d) => d !== alvo).length,
        linhasDeOutros: [...new Set(donos.filter((d) => d !== null && d !== alvo))],
        trocadas: [...antes].filter(([nome, li]) => depois.get(nome) !== li).map(([nome]) => nome),
        estado: depois.get(alvo)?.querySelector(".pastilha")?.textContent ?? null,
        sinal: depois.get(alvo)?.querySelector(".pessoa-sync-valor")?.textContent ?? null,
      };
    }, { base: BASE, alvo: apelido(7), foco: apelido(3) });
    assert.ok(caso1.chegou, "o PersonState não chegou à tela pelo caminho do evento");
    assert.ok(caso1.focado, "a bancada não conseguiu pôr o foco no nome de uma pessoa da lista");
    assert.ok(caso1.mutacoes > 0, "o PersonState não mudou nada na lista: a linha da pessoa ficou velha");
    assert.equal(
      caso1.foraDaLinha, 0,
      `um PersonState de ${apelido(7)} mexeu fora da linha dela: ${caso1.foraDaLinha} de ` +
        `${caso1.mutacoes} mutações, e ${caso1.trocadas.length} linhas trocaram de nó ` +
        `(outras linhas tocadas por dentro: ${caso1.linhasDeOutros.length}). ` +
        "A lista está sendo repovoada, e o leitor de tela a vê sumir e voltar",
    );
    assert.deepEqual(
      caso1.trocadas, [],
      `${caso1.trocadas.length} linhas trocaram de nó com um PersonState de uma pessoa só`,
    );
    assert.equal(caso1.estado, "TRANSMITINDO", `a linha de ${apelido(7)} não diz que ela fala`);
    assert.equal(caso1.sinal, "61", `a linha de ${apelido(7)} não diz o sinal novo`);
    assert.ok(caso1.focoFicou, "o foco que estava no nome de outra pessoa saiu dele com o PersonState");

    // A ordem na tela é a do retrato.
    const ordem1 = await page.evaluate(() => ({ tela: ordemNaTela(), retrato: ordemDoRetrato(desenhado) }));
    assert.deepEqual(ordem1.tela, ordem1.retrato, "depois de um PersonState, a ordem da lista não é a do retrato");

    // 2. O mesmo retrato, de novo: o tique de 500 ms o redesenha, e nada muda.
    //    O retrato desenhado tem de ser outro objeto no fim: sem isso, um tique
    //    parado passaria aqui sem medir nada.
    const caso2 = await page.evaluate(async () => {
      const antes = desenhado;
      const parar = observarALista();
      await new Promise((ok) => setTimeout(ok, 1200));
      return { mutacoes: parar().length, redesenhou: desenhado !== antes && desenhado !== null };
    });
    assert.ok(caso2.redesenhou, "o tique não redesenhou em 1,2 s: o caso 2 não mediu nada");
    assert.equal(caso2.mutacoes, 0, `o mesmo retrato, redesenhado pelo tique, fez ${caso2.mutacoes} mutações na lista`);

    // 3. Alguém de fora das salas entra na sala dois: o nó vai junto.
    const caso3 = await page.evaluate(async ({ alvo }) => {
      const grupos = () => new Map([...document.querySelectorAll("#lista-roster li.roster-grupo")]
        .map((g) => [g.querySelector(".roster-sala-nome")?.textContent, g]));
      const antes = linhasPorApelido();
      const gruposAntes = grupos();
      const novo = structuredClone(testTable.snapshot);
      const quem = novo.presentes.find((p) => p.nickname === alvo);
      novo.voice_rooms[1].people.push(quem);
      const chegou = await pelaPonte(novo, (d) => d?.voice_rooms?.[1]?.people?.length === 1);
      const depois = linhasPorApelido();
      const gruposDepois = grupos();
      const grupo = depois.get(alvo)?.closest("li.roster-grupo")?.querySelector(".roster-sala-nome")?.textContent;
      return {
        chegou,
        mesmoNo: antes.get(alvo) === depois.get(alvo),
        grupo,
        trocadas: [...antes].filter(([nome, li]) => nome !== alvo && depois.get(nome) !== li).length,
        gruposTrocados: [...gruposAntes].filter(([nome, g]) => gruposDepois.get(nome) !== g).map(([nome]) => nome),
        grupos: gruposDepois.size,
      };
    }, { alvo: apelido(40) });
    assert.ok(caso3.chegou, "a entrada na sala não chegou à tela");
    assert.equal(caso3.grupo, "Sala dois", `a linha de ${apelido(40)} não foi para o grupo da sala`);
    assert.ok(caso3.mesmoNo, `${apelido(40)} entrou numa sala e a linha dela foi recriada, e não movida`);
    assert.equal(caso3.trocadas, 0, `${caso3.trocadas} outras linhas trocaram de nó com uma entrada na sala`);
    const ordem3 = await page.evaluate(() => ({ tela: ordemNaTela(), retrato: ordemDoRetrato(desenhado) }));
    assert.deepEqual(ordem3.tela, ordem3.retrato, "depois de uma entrada na sala, a ordem da lista não é a do retrato");
    assert.equal(caso3.grupos, 3, `a lista ficou com ${caso3.grupos} grupos, e são três: as duas salas e fora delas`);
    assert.deepEqual(caso3.gruposTrocados, [], "um grupo que já existia trocou de nó quando uma sala ganhou alguém");

    // 4. Alguém sai do servidor, e alguém chega.
    const caso4 = await page.evaluate(async ({ sai, chega }) => {
      const antes = linhasPorApelido();
      const sem = structuredClone(testTable.snapshot);
      sem.presentes = sem.presentes.filter((p) => p.nickname !== sai);
      sem.voice_rooms[0].people = sem.voice_rooms[0].people.filter((p) => p.nickname !== sai);
      const saiu = await pelaPonte(sem, (d) => !d?.presentes?.some((p) => p.nickname === sai));
      const meio = linhasPorApelido();
      const com = structuredClone(sem);
      com.presentes.push({ ...com.presentes[1], id: 999, nickname: chega, is_self: false });
      const chegou = await pelaPonte(com, (d) => d?.presentes?.some((p) => p.nickname === chega));
      const depois = linhasPorApelido();
      return {
        saiu,
        chegou,
        foiEmbora: !antes.get(sai).isConnected && !meio.has(sai),
        nasceu: depois.has(chega),
        trocadas: [...antes].filter(([nome, li]) => nome !== sai && depois.get(nome) !== li).length,
      };
    }, { sai: apelido(12), chega: "recem-chegada" });
    assert.ok(caso4.saiu && caso4.chegou, "a saída ou a chegada não chegou à tela");
    assert.ok(caso4.foiEmbora, `${apelido(12)} saiu do servidor e a linha dela ficou`);
    assert.ok(caso4.nasceu, "quem chegou ao servidor não ganhou linha");
    assert.equal(caso4.trocadas, 0, `${caso4.trocadas} outras linhas trocaram de nó com uma saída e uma chegada`);
    const ordem4 = await page.evaluate(() => ({ tela: ordemNaTela(), retrato: ordemDoRetrato(desenhado) }));
    assert.deepEqual(ordem4.tela, ordem4.retrato, "depois de uma entrada na sala, uma saída e uma chegada, a ordem da lista não é a do retrato");

    // 8. A ordem muda: a primeira da sala principal vai para o fim. A lista e a
    //    coluna de salas ficam na ordem do retrato, e cada linha é o mesmo nó.
    const caso8 = await page.evaluate(async () => {
      const antes = linhasPorApelido();
      const novo = structuredClone(testTable.snapshot);
      const sala = novo.voice_rooms[0].people;
      sala.push(sala.shift());
      const ultimo = sala[sala.length - 1].nickname;
      const chegou = await pelaPonte(novo, (d) => d?.voice_rooms?.[0]?.people?.at(-1)?.nickname === ultimo);
      const depois = linhasPorApelido();
      return {
        chegou,
        naSala: sala.length,
        tela: ordemNaTela(),
        retrato: ordemDoRetrato(desenhado),
        coluna: ordemNaColuna(),
        colunaDoRetrato: desenhado.voice_rooms.flatMap((s) => s.people.map((p) => p.nickname)),
        trocadas: [...antes].filter(([nome, li]) => depois.get(nome) !== li).length,
      };
    });
    assert.ok(caso8.chegou, "a ordem nova não chegou à tela");
    assert.deepEqual(caso8.tela, caso8.retrato, "depois de uma reordenação, a ordem da lista não é a do retrato");
    assert.deepEqual(caso8.coluna, caso8.colunaDoRetrato, "depois de uma reordenação, a ordem da coluna de salas não é a do retrato");
    assert.equal(caso8.trocadas, 0, `${caso8.trocadas} linhas trocaram de nó com uma reordenação`);

    // 6. A coluna de salas, que também lista quem está em cada sala e tem o
    //    ENTRAR NA SALA: um PersonState mexe só na linha da pessoa nela, o
    //    botão é o mesmo nó, e o mesmo retrato não mexe em nada.
    const caso6 = await page.evaluate(async ({ alvo }) => {
      const coluna = document.getElementById("lista-voice_rooms");
      const entrar = () => [...coluna.querySelectorAll("button.voice_room-entrar")]
        .find((b) => b.textContent === "ENTRAR NA SALA") ?? null;
      const linhaNaSala = (nome) => [...coluna.querySelectorAll("li.voice_room-pessoa")]
        .find((li) => li.querySelector(".voice_room-pessoa-nome")?.textContent === nome) ?? null;
      const botao = entrar();
      const linha = linhaNaSala(alvo);
      const registros = [];
      const observador = new MutationObserver((rs) => registros.push(...rs));
      observador.observe(coluna, { subtree: true, childList: true, attributes: true, characterData: true });
      const novo = structuredClone(testTable.snapshot);
      novo.voice_rooms[0].people.find((p) => p.nickname === alvo).speaking = true;
      const chegou = await pelaPonte(novo, (d) =>
        d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === alvo)?.speaking === true);
      registros.push(...observador.takeRecords());
      const doEvento = registros.length;
      const fora = registros.filter((r) => {
        const no = r.target.nodeType === Node.ELEMENT_NODE ? r.target : r.target.parentElement;
        return no?.closest?.("li.voice_room-pessoa") !== linha;
      }).length;
      registros.length = 0;
      await new Promise((ok) => setTimeout(ok, 1200));
      registros.push(...observador.takeRecords());
      observador.disconnect();
      return {
        chegou,
        achou: Boolean(botao && linha),
        doEvento,
        fora,
        doTique: registros.length,
        mesmoBotao: botao !== null && entrar() === botao,
        mesmaLinha: linha !== null && linhaNaSala(alvo) === linha,
        marca: linhaNaSala(alvo)?.querySelector(".voice_room-pessoa-marca")?.textContent ?? null,
      };
    }, { alvo: apelido(9) });
    assert.ok(caso6.chegou, "o PersonState não chegou à coluna de salas");
    assert.ok(caso6.achou, "a coluna de salas não tem o ENTRAR NA SALA da sala dois, ou a linha da pessoa");
    assert.ok(caso6.doEvento > 0, "o PersonState não mudou nada na coluna de salas");
    assert.equal(
      caso6.fora, 0,
      `um PersonState de ${apelido(9)} mexeu fora da linha dela na coluna de salas: ${caso6.fora} de ` +
        `${caso6.doEvento} mutações. O ENTRAR NA SALA sai da árvore de acessibilidade a cada quadro`,
    );
    assert.ok(caso6.mesmoBotao, "o ENTRAR NA SALA trocou de nó com um PersonState de outra pessoa");
    assert.ok(caso6.mesmaLinha, `a linha de ${apelido(9)} na coluna de salas trocou de nó`);
    assert.equal(caso6.marca, "fala", `a linha de ${apelido(9)} na coluna de salas não diz que ela fala`);
    assert.equal(caso6.doTique, 0, `o mesmo retrato, redesenhado pelo tique, fez ${caso6.doTique} mutações na coluna de salas`);

    // 7. Ainda na coluna de salas: quem sai de uma sala tira só a linha dela, e
    //    o nome de uma sala renomeada, que é botão com ouvinte próprio e
    //    atravessa os retratos, abre o RENOMEAR com o nome de agora.
    const caso7 = await page.evaluate(async ({ sai, nomeNovo }) => {
      const coluna = document.getElementById("lista-voice_rooms");
      const linhasNaColuna = () => new Map([...coluna.querySelectorAll("li.voice_room-pessoa")]
        .map((li) => [li.querySelector(".voice_room-pessoa-nome")?.textContent, li]));
      const antes = linhasNaColuna();
      const novo = structuredClone(testTable.snapshot);
      novo.voice_rooms[0].people = novo.voice_rooms[0].people.filter((p) => p.nickname !== sai);
      novo.voice_rooms[1].name = nomeNovo;
      const chegou = await pelaPonte(novo, (d) => d?.voice_rooms?.[1]?.name === nomeNovo);
      const depois = linhasNaColuna();
      const botao = [...coluna.querySelectorAll("button.voice_room-nome")].find((b) => b.textContent === nomeNovo);
      botao?.click();
      await quadros(2);
      const valor = document.getElementById("nomear-valor")?.value ?? null;
      document.getElementById("nomear-cancelar")?.click();
      return {
        chegou,
        saiu: antes.has(sai) && !depois.has(sai),
        trocadas: [...antes].filter(([nome, li]) => nome !== sai && depois.get(nome) !== li).length,
        achouBotao: Boolean(botao),
        valor,
      };
    }, { sai: apelido(15), nomeNovo: "Sala dois, renomeada" });
    assert.ok(caso7.chegou, "a saída da sala e o nome novo não chegaram à coluna de salas");
    assert.ok(caso7.saiu, `${apelido(15)} saiu da sala e a linha dela ficou na coluna de salas`);
    assert.equal(caso7.trocadas, 0, `${caso7.trocadas} outras linhas da coluna de salas trocaram de nó com uma saída`);
    assert.ok(caso7.achouBotao, "o nome da sala renomeada não está na coluna de salas como botão");
    assert.equal(
      caso7.valor, "Sala dois, renomeada",
      `o RENOMEAR abriu com ${JSON.stringify(caso7.valor)}: o botão atravessou os retratos e ` +
        "ficou com o nome do quadro em que nasceu",
    );

    assert.deepEqual(errors, []);
    await page.close();

    // 5. Os nós de MOD, um ponto de montagem por vez, cada um numa página
    //    nova. Um PersonState de uma pessoa não tira nenhum nó de MOD do
    //    documento — nem por um instante: o observador vê os nós removidos —,
    //    cada nó é o mesmo, na mesma linha (ou no mesmo grupo), e nenhuma
    //    mutação cai fora da linha da pessoa. Quem usa está fora das salas, para
    //    os três ramos de `desenharPessoas` (FORA DE SALA, as salas, o saguão)
    //    montarem linhas, e as duas salas têm gente, para o `sala.acoes`.
    const RETRATO_MOD = (() => {
      const r = retratoCom({ sala1: todas.slice(1, NA_SALA), sala2: todas.slice(NA_SALA, NA_SALA + 5), presentes: todas });
      r.voice_rooms[0].occupied_by_us = false;
      return r;
    })();
    const PONTOS = [
      { tipo: "substituir", nome: "pessoa.cartao substituir, com conteúdo", seletor: '.contribuicao-de-mod[data-ponto="pessoa.cartao"]' },
      { tipo: "perfis", nome: "pessoa.cartao substituir sem conteúdo, o cartão da região (o PERFIS)", seletor: '.pessoa-cartao[data-mod="bancada/perfis"]' },
      { tipo: "api3", nome: "os cartões da API 3 na linha nativa", seletor: '.pessoa-cartao[data-mod="bancada/api3"]' },
      { tipo: "adicionar", nome: "pessoa.cartao adicionar", seletor: '.contribuicao-de-mod[data-ponto="pessoa.cartao"]' },
      { tipo: "identidade", nome: "pessoa.identidade", seletor: '.contribuicao-de-mod[data-ponto="pessoa.identidade"]' },
      { tipo: "detalhes", nome: "pessoa.detalhes", seletor: '.contribuicao-de-mod[data-ponto="pessoa.detalhes"]' },
      { tipo: "sala", nome: "sala.acoes", seletor: '.contribuicao-de-mod[data-ponto="sala.acoes"]', grupos: true },
    ];
    const caso5 = [];
    for (const ponto of PONTOS) {
      const aberta = await abrir(browser, url, RETRATO_MOD);
      const r = await aberta.page.evaluate(async ({ tipo, seletor, grupos, alvo }) => {
        const conteudo = (texto) => [{ forma: "texto", chave: "c", dentro: texto }];
        const porPessoa = (ponto, modo, comConteudo = true) => testTable.snapshot.presentes.map((p) => ({
          ponto, modo, alvo: String(p.id), prioridade: 0,
          ...(comConteudo ? { conteudo: conteudo(`${ponto} de ${p.nickname}`) } : {}),
        }));
        if (tipo === "substituir") registrarMod("bancada/substituir", porPessoa("pessoa.cartao", "substituir"));
        if (tipo === "perfis") {
          registrarMod("bancada/perfis", porPessoa("pessoa.cartao", "substituir", false));
          regiaoDeCartoes("bancada/perfis");
        }
        if (tipo === "api3") regiaoDeCartoes("bancada/api3");
        if (tipo === "adicionar") registrarMod("bancada/adicionar", porPessoa("pessoa.cartao", "adicionar"));
        if (tipo === "identidade") registrarMod("bancada/identidade", porPessoa("pessoa.identidade", "adicionar"));
        if (tipo === "detalhes") {
          registrarMod("bancada/detalhes", [
            ...porPessoa("pessoa.cartao", "substituir"),
            ...porPessoa("pessoa.detalhes", "adicionar"),
          ]);
        }
        if (tipo === "sala") {
          registrarMod("bancada/sala", testTable.snapshot.voice_rooms.map((s) => ({
            ponto: "sala.acoes", modo: "adicionar", alvo: String(s.id), prioridade: 0, conteudo: conteudo(`ações de ${s.name}`),
          })));
        }
        await quadros(3);
        desenhar(testTable.snapshot);

        const lista = document.getElementById("lista-roster");
        const dono = (no) => grupos
          ? no.closest("li.roster-grupo")?.querySelector(".roster-sala-nome")?.textContent ?? null
          : (no.closest("li.pessoa")?.querySelector(".pessoa-nome")?.textContent ?? "").replace(" (você)", "") || null;
        const nos = [...lista.querySelectorAll(seletor)];
        const donos = new Map(nos.map((no) => [no, dono(no)]));
        const esperados = grupos
          ? testTable.snapshot.voice_rooms.filter((s) => s.people.length > 0).length
          : lista.querySelectorAll("li.pessoa").length;

        const parar = observarALista();
        const novo = structuredClone(testTable.snapshot);
        novo.voice_rooms[0].people.find((p) => p.nickname === alvo).muted = true;
        novo.presentes.find((p) => p.nickname === alvo).muted = true;
        const chegou = await pelaPonte(novo, (d) =>
          d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === alvo)?.muted === true);
        const registros = parar();
        const removidos = registros.flatMap((reg) => [...reg.removedNodes]);
        const depois = new Set(lista.querySelectorAll(seletor));
        const linhaDoAlvo = linhasPorApelido().get(alvo);
        return {
          chegou,
          nos: nos.length,
          esperados,
          foraDaLinha: registros.map(donoDaMutacao).filter((d) => d !== alvo).length,
          mutacoes: registros.length,
          trocados: nos.filter((no) => !depois.has(no) || !no.isConnected || dono(no) !== donos.get(no)).length,
          saiuDoDocumento: nos.filter((no) => removidos.some((m) => m === no || m.contains?.(no))).length,
          estado: linhaDoAlvo?.querySelector(".pastilha")?.textContent ?? null,
        };
      }, { tipo: ponto.tipo, seletor: ponto.seletor, grupos: Boolean(ponto.grupos), alvo: apelido(5) });
      assert.deepEqual(aberta.errors, [], `${ponto.nome}: erros na página`);
      await aberta.page.close();
      assert.ok(r.chegou, `${ponto.nome}: o PersonState não chegou à tela`);
      assert.ok(r.nos > 0 && r.nos === r.esperados, `${ponto.nome}: ${r.nos} nós de MOD na lista, e eram ${r.esperados}`);
      assert.equal(
        r.saiuDoDocumento, 0,
        `${ponto.nome}: ${r.saiuDoDocumento} de ${r.nos} nós de MOD saíram do documento com um PersonState de ` +
          `${apelido(5)} — o marcador deste ponto não está na linha montada`,
      );
      assert.equal(r.foraDaLinha, 0, `${ponto.nome}: ${r.foraDaLinha} de ${r.mutacoes} mutações fora da linha de ${apelido(5)}`);
      assert.equal(r.trocados, 0, `${ponto.nome}: ${r.trocados} nós de MOD trocaram de nó, de lugar, ou saíram da lista`);
      assert.equal(r.estado, "MUDO", `${ponto.nome}: a linha de ${apelido(5)} não diz que ela está muda`);
      caso5.push(`${ponto.nome} (${r.nos})`);
    }

    console.log(
      `linhas-pelo-id: com ${PESSOAS} pessoas, um PersonState mexeu só na linha da pessoa ` +
        `(${caso1.mutacoes} mutações, nenhuma fora dela) e o foco ficou; o mesmo retrato não ` +
        "mexeu em nada (e foi redesenhado); quem entrou na sala levou o nó; quem saiu perdeu o " +
        "dele e quem chegou ganhou um; a ordem é a do retrato, também depois de uma " +
        `reordenação; na coluna de salas, ${caso6.doEvento} mutações, todas na linha da ` +
        "pessoa, o ENTRAR NA SALA o mesmo nó, quem saiu da sala levou só a linha dela, e o " +
        "RENOMEAR abriu com o nome de agora; e nenhum nó de MOD saiu do documento nem trocou " +
        `de linha, ponto por ponto: ${caso5.join("; ")}.`,
    );
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
