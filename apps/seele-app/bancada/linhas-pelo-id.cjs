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
// 9. **O `<details>` que a pessoa abre continua aberto** através do tique.
// 10. **A coluna de canais** (pendência 64): o foco do teclado no botão do
//    segundo canal atravessa o tique — com o mesmo retrato, com outro canal
//    mudando de nome, e quando ele vira o aberto —, o botão é o mesmo nó e
//    continua no documento; um canal que entra ganha um nó, um que sai perde o
//    dele, os outros ficam; e o nó de MOD do `canal.item` não sai do documento.
// 11. **Um id repetido num grupo do MOD** de `servidor.navegacao` não duplica
//    a fileira da sala, com o ENTRAR NA SALA, nem a do canal.
//
// Com o conserto revertido (a lista repovoada a cada retrato), o caso 1 conta
// mutações fora da linha e nós trocados em todas as outras; com a coluna de
// salas repovoada, o caso 6; com a de canais, o caso 10.
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
    //    Só a que mudou de lugar se move: um nó inserido no grupo da sala e um
    //    na coluna, e o foco no nome de quem está no meio fica (M1 da revisão:
    //    mandar uma linha para trás movia todas as do meio, e o foco caía).
    const caso8 = await page.evaluate(async ({ meio }) => {
      const antes = linhasPorApelido();
      const botao = antes.get(meio)?.querySelector("button.pessoa-nome-porta");
      botao?.focus();
      const focado = Boolean(botao) && document.activeElement === botao;
      const ulDaSala = antes.get(meio)?.closest("ul.roster-pessoas");
      const ulDaColuna = [...document.querySelectorAll("#lista-voice_rooms li.voice_room-pessoa")]
        .find((li) => li.querySelector(".voice_room-pessoa-nome")?.textContent === meio)?.closest("ul");
      const registros = [];
      const observador = new MutationObserver((rs) => registros.push(...rs));
      for (const ul of [ulDaSala, ulDaColuna]) if (ul) observador.observe(ul, { childList: true });
      const novo = structuredClone(testTable.snapshot);
      const sala = novo.voice_rooms[0].people;
      sala.push(sala.shift());
      const ultimo = sala[sala.length - 1].nickname;
      const chegou = await pelaPonte(novo, (d) => d?.voice_rooms?.[0]?.people?.at(-1)?.nickname === ultimo);
      registros.push(...observador.takeRecords());
      observador.disconnect();
      const inseridos = (ul) => registros.filter((r) => r.target === ul).reduce((n, r) => n + r.addedNodes.length, 0);
      const depois = linhasPorApelido();
      return {
        chegou,
        focado,
        focoFicou: focado && document.activeElement === botao,
        inseridosNaSala: inseridos(ulDaSala),
        inseridosNaColuna: inseridos(ulDaColuna),
        naSala: sala.length,
        tela: ordemNaTela(),
        retrato: ordemDoRetrato(desenhado),
        coluna: ordemNaColuna(),
        colunaDoRetrato: desenhado.voice_rooms.flatMap((s) => s.people.map((p) => p.nickname)),
        trocadas: [...antes].filter(([nome, li]) => depois.get(nome) !== li).length,
      };
    }, { meio: apelido(10) });
    assert.ok(caso8.chegou, "a ordem nova não chegou à tela");
    assert.ok(caso8.focado, `a bancada não conseguiu pôr o foco no nome de ${apelido(10)}`);
    assert.equal(
      caso8.inseridosNaSala, 1,
      `a primeira de ${caso8.naSala} pessoas foi para o fim e ${caso8.inseridosNaSala} linhas foram tiradas e ` +
        "postas de novo no grupo da sala: só a que mudou de lugar se move",
    );
    assert.equal(caso8.inseridosNaColuna, 1, `na coluna de salas, ${caso8.inseridosNaColuna} linhas foram tiradas e postas de novo`);
    assert.ok(caso8.focoFicou, `o foco no nome de ${apelido(10)} caiu com a reordenação`);
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
      // M2 da revisão: numa linha apresentada por MOD, quem começa a falar muda
      // a classe do detalhe nativo do «⋯» (`pessoa` → `pessoa falando`), e ele
      // não pode ser refeito — com a identidade do MOD dentro, ela sairia do
      // documento.
      {
        tipo: "identidade-apresentada", nome: "pessoa.identidade numa linha apresentada, com a pessoa começando a falar",
        seletor: '.contribuicao-de-mod[data-ponto="pessoa.identidade"]', campo: "speaking", estado: "TRANSMITINDO",
      },
    ];
    const caso5 = [];
    for (const ponto of PONTOS) {
      const aberta = await abrir(browser, url, RETRATO_MOD);
      const r = await aberta.page.evaluate(async ({ tipo, seletor, grupos, alvo, campo }) => {
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
        if (tipo === "identidade-apresentada") {
          registrarMod("bancada/identidade-apresentada", [
            ...porPessoa("pessoa.cartao", "substituir"),
            ...porPessoa("pessoa.identidade", "adicionar"),
          ]);
        }
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

        // O detalhe nativo do «⋯», quando a linha é apresentada por um MOD.
        const detalheDe = () => linhasPorApelido().get(alvo)?.querySelector("details.pessoa-nativo > .pessoa") ?? null;
        const detalhe = detalheDe();
        const parar = observarALista();
        const novo = structuredClone(testTable.snapshot);
        novo.voice_rooms[0].people.find((p) => p.nickname === alvo)[campo] = true;
        novo.presentes.find((p) => p.nickname === alvo)[campo] = true;
        const chegou = await pelaPonte(novo, (d) =>
          d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === alvo)?.[campo] === true);
        const registros = parar();
        const removidos = registros.flatMap((reg) => [...reg.removedNodes]);
        const depois = new Set(lista.querySelectorAll(seletor));
        const linhaDoAlvo = linhasPorApelido().get(alvo);
        return {
          chegou,
          detalheTrocado: detalhe !== null && detalheDe() !== detalhe,
          nos: nos.length,
          esperados,
          foraDaLinha: registros.map(donoDaMutacao).filter((d) => d !== alvo).length,
          mutacoes: registros.length,
          trocados: nos.filter((no) => !depois.has(no) || !no.isConnected || dono(no) !== donos.get(no)).length,
          saiuDoDocumento: nos.filter((no) => removidos.some((m) => m === no || m.contains?.(no))).length,
          estado: linhaDoAlvo?.querySelector(".pastilha")?.textContent ?? null,
        };
      }, {
        tipo: ponto.tipo, seletor: ponto.seletor, grupos: Boolean(ponto.grupos), alvo: apelido(5),
        campo: ponto.campo ?? "muted",
      });
      assert.deepEqual(aberta.errors, [], `${ponto.nome}: erros na página`);
      await aberta.page.close();
      assert.ok(r.chegou, `${ponto.nome}: o PersonState não chegou à tela`);
      assert.ok(r.nos > 0 && r.nos === r.esperados, `${ponto.nome}: ${r.nos} nós de MOD na lista, e eram ${r.esperados}`);
      assert.equal(
        r.saiuDoDocumento, 0,
        `${ponto.nome}: ${r.saiuDoDocumento} de ${r.nos} nós de MOD saíram do documento com um PersonState de ` +
          `${apelido(5)}: ou o marcador deste ponto não está na linha montada, ou um nó do produto com ` +
          "o nó de MOD dentro foi refeito",
      );
      assert.equal(r.foraDaLinha, 0, `${ponto.nome}: ${r.foraDaLinha} de ${r.mutacoes} mutações fora da linha de ${apelido(5)}`);
      assert.equal(r.trocados, 0, `${ponto.nome}: ${r.trocados} nós de MOD trocaram de nó, de lugar, ou saíram da lista`);
      assert.equal(r.estado, ponto.estado ?? "MUDO", `${ponto.nome}: a linha de ${apelido(5)} não diz o estado novo`);
      assert.ok(!r.detalheTrocado, `${ponto.nome}: o detalhe nativo do «⋯» de ${apelido(5)} foi refeito`);
      caso5.push(`${ponto.nome} (${r.nos})`);
    }

    // 9. O `<details>` que a pessoa abre continua aberto. O «⋯» de uma linha
    //    apresentada por MOD — onde moram o diagnóstico nativo e o volume — e o
    //    SINAL E ESTADO de uma linha nativa: abertos à mão, atravessam o tique
    //    sem mutação e com o foco no lugar, e o volume continua alcançável.
    //    O que a linha montada pede continua valendo quando o pedido muda: quem
    //    fica muda tem o SINAL E ESTADO aberto, e fechado quando deixa de ficar.
    const pagina9 = await abrir(browser, url, BASE);
    const caso9 = await pagina9.page.evaluate(async ({ apresentada, nativa, muda }) => {
      registrarMod("bancada/apresenta", [{
        ponto: "pessoa.cartao", modo: "substituir", alvo: String(testTable.snapshot.presentes
          .find((p) => p.nickname === apresentada).id), prioridade: 0,
        conteudo: [{ forma: "texto", chave: "c", dentro: `cartão de ${apresentada}` }],
      }]);
      await quadros(3);
      desenhar(testTable.snapshot);
      const tique = () => new Promise((ok) => setTimeout(ok, 1200));
      const linhaDe = (nome) => linhasPorApelido().get(nome);

      // O «⋯» da linha apresentada, aberto pelo resumo, com o foco nele.
      const linha = linhaDe(apresentada);
      const mais = linha.querySelector("details.pessoa-nativo");
      const resumo = mais.querySelector("summary");
      resumo.click();
      resumo.focus();
      const abriu = mais.open;
      let parar = observarALista();
      const antes = desenhado;
      await tique();
      const noTique = parar().filter((r) => donoDaMutacao(r) === apresentada).length;
      const redesenhou = desenhado !== antes;
      const maisFicou = linhaDe(apresentada)?.querySelector("details.pessoa-nativo") === mais && mais.open;
      const focoNoResumo = document.activeElement === resumo;
      // O volume, dentro do «⋯», ao alcance do teclado através de outro tique.
      const volume = mais.querySelector("input.volume");
      volume?.focus();
      await tique();
      const volumeAlcancavel = Boolean(volume) && volume.isConnected && document.activeElement === volume
        && volume.checkVisibility() && mais.open;

      // O SINAL E ESTADO de uma linha nativa, aberto à mão.
      const diagnostico = linhaDe(nativa).querySelector("details.pessoa-diagnostico");
      diagnostico.querySelector("summary").click();
      parar = observarALista();
      await tique();
      const noTiqueNativa = parar().filter((r) => donoDaMutacao(r) === nativa).length;
      const nativaFicou = diagnostico.isConnected && diagnostico.open;

      // Quem fica muda: o produto abre o SINAL E ESTADO dela, e fecha quando ela
      // volta — e a linha aberta à mão segue aberta através dos dois retratos.
      const daMuda = () => linhaDe(muda).querySelector("details.pessoa-diagnostico");
      const fechadaAntes = !daMuda().open;
      const comMudo = structuredClone(testTable.snapshot);
      comMudo.voice_rooms[0].people.find((p) => p.nickname === muda).muted = true;
      await pelaPonte(comMudo, (d) => d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === muda)?.muted === true);
      const abriuComMudo = daMuda().open;
      const semMudo = structuredClone(comMudo);
      semMudo.voice_rooms[0].people.find((p) => p.nickname === muda).muted = false;
      await pelaPonte(semMudo, (d) => d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === muda)?.muted === false);
      const fechouSemMudo = !daMuda().open;
      return {
        abriu, redesenhou, noTique, maisFicou, focoNoResumo, volumeAlcancavel,
        noTiqueNativa, nativaFicou, fechadaAntes, abriuComMudo, fechouSemMudo,
        nativaSeguiuAberta: diagnostico.isConnected && diagnostico.open,
        maisSeguiuAberto: mais.isConnected && mais.open,
      };
    }, { apresentada: apelido(3), nativa: apelido(4), muda: apelido(6) });
    assert.deepEqual(pagina9.errors, [], "caso 9: erros na página");
    await pagina9.page.close();
    assert.ok(caso9.abriu, "o «⋯» não abriu com o clique no resumo");
    assert.ok(caso9.redesenhou, "caso 9: o tique não redesenhou, e o caso não mediu nada");
    assert.ok(
      caso9.maisFicou,
      `o «⋯» de ${apelido(3)}, aberto pela pessoa, fechou no tique com o mesmo retrato ` +
        `(${caso9.noTique} mutações na linha): quem usa teclado não alcança o volume nem o diagnóstico`,
    );
    assert.equal(caso9.noTique, 0, `o tique com o mesmo retrato fez ${caso9.noTique} mutações na linha de ${apelido(3)}`);
    assert.ok(caso9.focoNoResumo, "o foco saiu do resumo do «⋯» no tique");
    assert.ok(caso9.volumeAlcancavel, "o volume dentro do «⋯» não ficou ao alcance do teclado através de um tique");
    assert.ok(caso9.nativaFicou, `o SINAL E ESTADO de ${apelido(4)}, aberto pela pessoa, fechou no tique`);
    assert.equal(caso9.noTiqueNativa, 0, `o tique fez ${caso9.noTiqueNativa} mutações na linha de ${apelido(4)}`);
    assert.ok(caso9.fechadaAntes, `o SINAL E ESTADO de ${apelido(6)} já estava aberto antes de ela ficar muda`);
    assert.ok(caso9.abriuComMudo, `${apelido(6)} ficou muda e o SINAL E ESTADO dela não abriu`);
    assert.ok(caso9.fechouSemMudo, `${apelido(6)} deixou de estar muda e o SINAL E ESTADO dela não fechou`);
    assert.ok(caso9.nativaSeguiuAberta && caso9.maisSeguiuAberto, "o que a pessoa abriu fechou com um PersonState de outra pessoa");

    // 10. A coluna de canais (pendência 64). O foco do teclado no botão do
    //     segundo canal atravessa o tique de 500 ms — com o mesmo retrato, com
    //     um retrato em que outro canal muda de nome, e quando ele mesmo vira o
    //     aberto —; um canal que entra ganha um nó, um que sai perde o dele, e
    //     os outros ficam com o mesmo. Com a coluna repovoada a cada retrato, o
    //     botão saía do documento e o foco caía no `<body>`, em até meio
    //     segundo e com a sessão parada.
    //
    //     Pelo tique, e não pelo evento: é o tique que redesenha a sessão
    //     parada, e é nele que quem navega pelo teclado perdia o lugar.
    const CANAIS = [
      { id: 1, name: "geral", open: true },
      { id: 2, name: "combinados", open: false },
      { id: 3, name: "avisos", open: false },
      { id: 4, name: "musica", open: false },
    ];
    const comCanais = { ...BASE, channels: CANAIS };
    const pagina10 = await abrir(browser, url, comCanais);
    const caso10 = await pagina10.page.evaluate(async ({ focado, renomeado, nomeNovo, sai, chega }) => {
      const coluna = document.getElementById("lista-linhas");
      const tique = () => new Promise((ok) => setTimeout(ok, 1200));
      const botoes = () => new Map([...coluna.querySelectorAll("button[data-linha]")].map((b) => [b.dataset.linha, b]));
      const fileiras = () => new Map([...botoes()].map(([id, b]) => [id, b.closest("li")]));
      const observar = () => {
        const registros = [];
        const observador = new MutationObserver((rs) => registros.push(...rs));
        observador.observe(coluna, { subtree: true, childList: true, attributes: true, characterData: true });
        return () => {
          registros.push(...observador.takeRecords());
          observador.disconnect();
          return registros;
        };
      };
      // O canal da fileira em que uma mutação caiu, ou `null` fora de fileira.
      const donoDaMutacao = (registro) => {
        const no = registro.target.nodeType === Node.ELEMENT_NODE ? registro.target : registro.target.parentElement;
        return no?.closest?.("li")?.querySelector("button[data-linha]")?.dataset.linha ?? null;
      };
      const trocadas = (antes, depois, fora = null) =>
        [...antes].filter(([id, no]) => id !== fora && depois.get(id) !== no).map(([id]) => id);
      const ondeEstaOFoco = () => {
        const ativo = document.activeElement;
        return ativo?.dataset?.linha ? `canal ${ativo.dataset.linha}` : (ativo?.tagName ?? "nada");
      };
      const id = String(focado);

      // a) O mesmo retrato, redesenhado pelo tique.
      const botao = botoes().get(id);
      botao?.focus();
      const antesA = botoes();
      const fileirasA = fileiras();
      const focadoA = Boolean(botao) && document.activeElement === botao;
      let parar = observar();
      let retrato = desenhado;
      await tique();
      const mesmo = {
        focado: focadoA,
        redesenhou: desenhado !== retrato && desenhado !== null,
        mesmoNo: botoes().get(id) === botao,
        noDocumento: Boolean(botao?.isConnected),
        focoFicou: document.activeElement === botao,
        foco: ondeEstaOFoco(),
        mutacoes: parar().length,
        trocados: trocadas(antesA, botoes()),
        fileirasTrocadas: trocadas(fileirasA, fileiras()),
      };

      // b) Outro canal muda de nome, pelo tique. O foco vai de novo ao botão
      //    do segundo canal que está na tela, para este passo medir sozinho.
      const botaoB = botoes().get(id);
      botaoB?.focus();
      const focadoB = Boolean(botaoB) && document.activeElement === botaoB;
      const antesB = botoes();
      const fileirasB = fileiras();
      parar = observar();
      retrato = desenhado;
      const comNome = structuredClone(testTable.snapshot);
      comNome.channels.find((c) => c.id === renomeado).name = nomeNovo;
      testTable.snapshot = comNome;
      await tique();
      const registrosB = parar();
      const outro = {
        focado: focadoB,
        redesenhou: desenhado !== retrato && desenhado?.channels?.find((c) => c.id === renomeado)?.name === nomeNovo,
        mesmoNo: botoes().get(id) === botaoB,
        noDocumento: Boolean(botaoB?.isConnected),
        focoFicou: document.activeElement === botaoB,
        foco: ondeEstaOFoco(),
        mutacoes: registrosB.length,
        foraDaFileira: registrosB.map(donoDaMutacao).filter((d) => d !== String(renomeado)).length,
        trocados: trocadas(antesB, botoes()),
        fileirasTrocadas: trocadas(fileirasB, fileiras()),
        rotulo: botoes().get(String(renomeado))?.querySelector(".linha-rotulo")?.textContent ?? null,
        renomearDiz: coluna.querySelector(`button[data-renomear-linha="${renomeado}"]`)?.getAttribute("aria-label") ?? null,
      };

      // c) Um canal sai e outro entra, pelo tique.
      const botaoC = botoes().get(id);
      botaoC?.focus();
      const antesC = botoes();
      const fileirasC = fileiras();
      const mudado = structuredClone(testTable.snapshot);
      mudado.channels = mudado.channels.filter((c) => c.id !== sai);
      mudado.channels.push({ id: chega, name: "recem-criado", open: false });
      testTable.snapshot = mudado;
      await tique();
      const depoisC = botoes();
      const fileirasDepoisC = fileiras();
      const fileiraQueEntrou = fileirasDepoisC.get(String(chega));
      const entraESai = {
        chegou: desenhado?.channels?.some((c) => c.id === chega) && !desenhado?.channels?.some((c) => c.id === sai),
        saiu: !depoisC.has(String(sai)) && !fileirasC.get(String(sai))?.isConnected,
        nasceu: Boolean(fileiraQueEntrou) && ![...fileirasC.values()].includes(fileiraQueEntrou)
          && ![...antesC.values()].includes(depoisC.get(String(chega))),
        trocados: trocadas(antesC, depoisC, String(sai)),
        fileirasTrocadas: trocadas(fileirasC, fileirasDepoisC, String(sai)),
        focoFicou: Boolean(botaoC) && document.activeElement === botaoC,
        ordem: [...depoisC.keys()],
        ordemDoRetrato: (desenhado?.channels ?? []).map((c) => String(c.id)),
      };

      // d) O RENOMEAR do canal renomeado, que atravessou os retratos, abre
      //    com o nome de agora, e não com o do quadro em que o botão nasceu.
      coluna.querySelector(`button[data-renomear-linha="${renomeado}"]`)?.click();
      await quadros(2);
      const valor = document.getElementById("nomear-valor")?.value ?? null;
      document.getElementById("nomear-cancelar")?.click();

      // e) O canal com o foco vira o aberto: é o Enter de quem navega pelo
      //    teclado (o `alternarCanal` pede o retrato, e ele vem com o canal
      //    aberto). A classe passa de `linha` a `linha aberto`, e o botão só
      //    continua o mesmo nó porque o feitio é a primeira classe
      //    (`mesmoFeitio`): uma variante posta antes da base o refaria, e o
      //    foco cairia no `<body>` (I1 da revisão do lote C64).
      const botaoE = botoes().get(id);
      botaoE?.focus();
      const focadoE = Boolean(botaoE) && document.activeElement === botaoE;
      const primeiroE = botoes().get("1");
      retrato = desenhado;
      const abrindo = structuredClone(testTable.snapshot);
      for (const canal of abrindo.channels) canal.open = canal.id === focado;
      testTable.snapshot = abrindo;
      await tique();
      const abriu = {
        focado: focadoE,
        redesenhou: desenhado !== retrato && desenhado?.channels?.find((c) => c.id === focado)?.open === true
          && desenhado?.channels?.find((c) => c.id === 1)?.open === false,
        mesmoNo: Boolean(botaoE) && botoes().get(id) === botaoE,
        noDocumento: Boolean(botaoE?.isConnected),
        focoFicou: Boolean(botaoE) && document.activeElement === botaoE,
        foco: ondeEstaOFoco(),
        classe: botaoE?.className ?? null,
        atual: botaoE?.getAttribute("aria-current") ?? null,
        primeiroMesmoNo: Boolean(primeiroE) && botoes().get("1") === primeiroE,
        primeiroClasse: primeiroE?.className ?? null,
        primeiroAtual: primeiroE?.getAttribute("aria-current") ?? null,
      };
      return { mesmo, outro, entraESai, valor, abriu };
    }, { focado: 2, renomeado: 4, nomeNovo: "musica ao vivo", sai: 3, chega: 5 });
    assert.deepEqual(pagina10.errors, [], "caso 10: erros na página");
    await pagina10.page.close();
    const { mesmo, outro, entraESai } = caso10;
    assert.ok(mesmo.focado, "a bancada não conseguiu pôr o foco no botão do segundo canal");
    assert.ok(mesmo.redesenhou, "caso 10: o tique não redesenhou em 1,2 s, e o caso não mediu nada");
    assert.ok(
      mesmo.mesmoNo && mesmo.noDocumento && mesmo.focoFicou,
      `o mesmo retrato, redesenhado pelo tique, tirou o foco do segundo canal: o botão ${mesmo.mesmoNo ? "é" : "não é"} ` +
        `o mesmo nó, ${mesmo.noDocumento ? "está" : "saiu"} do documento, e o foco está em ${mesmo.foco}. ` +
        "Quem navega a coluna de canais pelo teclado perde o lugar a cada meio segundo",
    );
    assert.deepEqual(mesmo.trocados, [], `o mesmo retrato trocou o botão dos canais ${mesmo.trocados.join(", ")}`);
    assert.deepEqual(mesmo.fileirasTrocadas, [], `o mesmo retrato trocou a fileira dos canais ${mesmo.fileirasTrocadas.join(", ")}`);
    assert.equal(mesmo.mutacoes, 0, `o mesmo retrato, redesenhado pelo tique, fez ${mesmo.mutacoes} mutações na coluna de canais`);
    assert.ok(outro.focado, "a bancada não conseguiu pôr o foco de novo no botão do segundo canal");
    assert.ok(outro.redesenhou, "caso 10: o nome novo do quarto canal não chegou à tela pelo tique");
    assert.ok(
      outro.mesmoNo && outro.noDocumento && outro.focoFicou,
      `o quarto canal mudou de nome e o foco saiu do segundo: o botão ${outro.mesmoNo ? "é" : "não é"} o mesmo nó, ` +
        `${outro.noDocumento ? "está" : "saiu"} do documento, e o foco está em ${outro.foco}`,
    );
    assert.deepEqual(outro.trocados, [], `um canal mudou de nome e o botão dos canais ${outro.trocados.join(", ")} trocou de nó`);
    assert.deepEqual(outro.fileirasTrocadas, [], `um canal mudou de nome e a fileira dos canais ${outro.fileirasTrocadas.join(", ")} trocou de nó`);
    assert.ok(outro.mutacoes > 0, "o nome novo não mudou nada na coluna: a fileira do canal ficou velha");
    assert.equal(
      outro.foraDaFileira, 0,
      `o quarto canal mudou de nome e ${outro.foraDaFileira} de ${outro.mutacoes} mutações caíram fora da fileira dele`,
    );
    assert.equal(outro.rotulo, "musica ao vivo", `a fileira do canal renomeado diz ${JSON.stringify(outro.rotulo)}`);
    assert.equal(
      outro.renomearDiz, "renomear o canal musica ao vivo",
      `o lápis do canal renomeado diz ${JSON.stringify(outro.renomearDiz)} ao leitor de tela`,
    );
    assert.ok(entraESai.chegou, "caso 10: a saída e a entrada de um canal não chegaram à tela pelo tique");
    assert.ok(entraESai.saiu, "o terceiro canal saiu do retrato e a fileira dele ficou no documento");
    assert.ok(entraESai.nasceu, "o canal que entrou não ganhou uma fileira nova: herdou o nó de outro canal");
    assert.deepEqual(
      entraESai.trocados, [],
      `um canal saiu e outro entrou, e o botão dos canais ${entraESai.trocados.join(", ")} trocou de nó`,
    );
    assert.deepEqual(
      entraESai.fileirasTrocadas, [],
      `um canal saiu e outro entrou, e a fileira dos canais ${entraESai.fileirasTrocadas.join(", ")} trocou de nó`,
    );
    assert.ok(entraESai.focoFicou, "um canal saiu e outro entrou, e o foco saiu do segundo canal");
    assert.deepEqual(entraESai.ordem, entraESai.ordemDoRetrato, "a ordem da coluna de canais não é a do retrato");
    assert.equal(
      caso10.valor, "musica ao vivo",
      `o RENOMEAR do canal renomeado abriu com ${JSON.stringify(caso10.valor)}, e não com o nome de agora`,
    );
    const { abriu } = caso10;
    assert.ok(abriu.focado, "caso 10 (e): a bancada não conseguiu pôr o foco no botão do segundo canal");
    assert.ok(abriu.redesenhou, "caso 10 (e): o retrato com o segundo canal aberto não chegou à tela pelo tique");
    assert.ok(
      abriu.mesmoNo && abriu.noDocumento && abriu.focoFicou,
      `o segundo canal, com o foco, virou o aberto e o foco saiu dele: o botão ${abriu.mesmoNo ? "é" : "não é"} o ` +
        `mesmo nó, ${abriu.noDocumento ? "está" : "saiu"} do documento, e o foco está em ${abriu.foco}. ` +
        "Quem abre um canal pelo teclado perde o lugar",
    );
    assert.equal(abriu.classe, "linha aberto", `o canal aberto tem a classe ${JSON.stringify(abriu.classe)}`);
    assert.equal(abriu.atual, "true", `o canal aberto tem aria-current ${JSON.stringify(abriu.atual)}`);
    assert.ok(abriu.primeiroMesmoNo, "o primeiro canal deixou de ser o aberto e o botão dele trocou de nó");
    assert.equal(abriu.primeiroClasse, "linha", `o canal que deixou de ser o aberto tem a classe ${JSON.stringify(abriu.primeiroClasse)}`);
    assert.equal(abriu.primeiroAtual, null, `o canal que deixou de ser o aberto ainda tem aria-current ${JSON.stringify(abriu.primeiroAtual)}`);

    // 10, o ponto de MOD da fileira: o `canal.item`, dentro do botão do canal.
    //     Um retrato em que outro canal muda de nome não tira nenhum nó de MOD
    //     do documento, e cada um fica no botão do canal dele.
    const pagina10m = await abrir(browser, url, comCanais);
    const caso10m = await pagina10m.page.evaluate(async ({ renomeado, nomeNovo }) => {
      registrarMod("bancada/canal", testTable.snapshot.channels.map((c) => ({
        ponto: "canal.item", modo: "adicionar", alvo: String(c.id), prioridade: 0,
        conteudo: [{ forma: "texto", chave: "c", dentro: `selo de ${c.name}` }],
      })));
      await quadros(3);
      desenhar(testTable.snapshot);
      const coluna = document.getElementById("lista-linhas");
      const seletor = '.contribuicao-de-mod[data-ponto="canal.item"]';
      const dono = (no) => no.closest("button[data-linha]")?.dataset.linha ?? null;
      const nos = [...coluna.querySelectorAll(seletor)];
      const donos = new Map(nos.map((no) => [no, dono(no)]));
      const registros = [];
      const observador = new MutationObserver((rs) => registros.push(...rs));
      observador.observe(coluna, { subtree: true, childList: true, attributes: true, characterData: true });
      const novo = structuredClone(testTable.snapshot);
      novo.channels.find((c) => c.id === renomeado).name = nomeNovo;
      testTable.snapshot = novo;
      await new Promise((ok) => setTimeout(ok, 1200));
      registros.push(...observador.takeRecords());
      observador.disconnect();
      const removidos = registros.flatMap((r) => [...r.removedNodes]);
      return {
        redesenhou: desenhado?.channels?.find((c) => c.id === renomeado)?.name === nomeNovo,
        nos: nos.length,
        esperados: testTable.snapshot.channels.length,
        doCanalCerto: nos.filter((no) => donos.get(no) === no.dataset.destino).length,
        saiuDoDocumento: nos.filter((no) => removidos.some((m) => m === no || m.contains?.(no))).length,
        trocados: nos.filter((no) => !no.isConnected || dono(no) !== donos.get(no)).length,
      };
    }, { renomeado: 4, nomeNovo: "musica ao vivo" });
    assert.deepEqual(pagina10m.errors, [], "caso 10, canal.item: erros na página");
    await pagina10m.page.close();
    assert.ok(caso10m.redesenhou, "caso 10, canal.item: o nome novo não chegou à tela pelo tique");
    assert.ok(
      caso10m.nos > 0 && caso10m.nos === caso10m.esperados && caso10m.doCanalCerto === caso10m.nos,
      `caso 10, canal.item: ${caso10m.nos} nós de MOD na coluna (${caso10m.doCanalCerto} no botão do canal deles), ` +
        `e eram ${caso10m.esperados}`,
    );
    assert.equal(
      caso10m.saiuDoDocumento, 0,
      `canal.item: ${caso10m.saiuDoDocumento} de ${caso10m.nos} nós de MOD saíram do documento com um canal ` +
        "mudando de nome: ou o marcador deste ponto não está na fileira montada, ou o botão com o nó de MOD dentro foi refeito",
    );
    assert.equal(caso10m.trocados, 0, `canal.item: ${caso10m.trocados} nós de MOD trocaram de canal ou saíram da coluna`);

    // 11. Um id repetido num grupo do MOD de `servidor.navegacao` não duplica a
    //     fileira, nem na coluna de salas nem na de canais. O `emGrupos` punha a
    //     mesma fileira duas vezes no vetor; o `repovoar` a punha uma vez só, e
    //     a reconciliação faz da segunda um nó a mais — a sala, com o ENTRAR
    //     NA SALA, duas vezes (regressão do lote AX, consertada no lote C64).
    //     Dois retratos e um tique: a duplicata só nasce quando já há nó velho.
    const pagina11 = await abrir(browser, url, BASE);
    const caso11 = await pagina11.page.evaluate(async () => {
      registrarMod("bancada/navegacao", [{
        ponto: "servidor.navegacao", modo: "substituir", prioridade: 0,
        conteudo: {
          salas: { grupos: [{ titulo: "MESA", ids: [1, 1] }] },
          canais: { grupos: [{ titulo: "MESA", ids: [2, 2] }] },
        },
      }]);
      await quadros(3);
      desenhar(testTable.snapshot);
      desenhar(testTable.snapshot);
      const salas = document.getElementById("lista-voice_rooms");
      const canais = document.getElementById("lista-linhas");
      const entrar = () => salas.querySelector('button[data-voice_room="1"]');
      const contar = () => ({
        salas: salas.querySelectorAll("li.voice_room").length,
        entrar: salas.querySelectorAll("button[data-voice_room]").length,
        canais: canais.querySelectorAll("button[data-linha]").length,
      });
      const titulos = (lista) => [...lista.querySelectorAll("li.canais-grupo")].map((g) => g.textContent);
      const antes = entrar();
      const nosRetratos = contar();
      const retrato = desenhado;
      await new Promise((ok) => setTimeout(ok, 1200));
      return {
        redesenhou: desenhado !== retrato && desenhado !== null,
        nosRetratos,
        noTique: contar(),
        salasNoRetrato: testTable.snapshot.voice_rooms.length,
        canaisNoRetrato: testTable.snapshot.channels.length,
        mesmoEntrar: antes !== null && entrar() === antes,
        gruposDasSalas: titulos(salas),
        gruposDosCanais: titulos(canais),
      };
    });
    assert.deepEqual(pagina11.errors, [], "caso 11: erros na página");
    await pagina11.page.close();
    assert.ok(caso11.redesenhou, "caso 11: o tique não redesenhou em 1,2 s, e o caso não mediu nada");
    assert.ok(
      caso11.gruposDasSalas.includes("MESA") && caso11.gruposDosCanais.includes("MESA"),
      `caso 11: o arranjo do MOD não chegou às colunas (salas ${JSON.stringify(caso11.gruposDasSalas)}, ` +
        `canais ${JSON.stringify(caso11.gruposDosCanais)}), e o caso não mediu nada`,
    );
    for (const [quando, conta] of [["depois de dois retratos", caso11.nosRetratos], ["depois do tique", caso11.noTique]]) {
      assert.ok(
        conta.salas === caso11.salasNoRetrato && conta.entrar === caso11.salasNoRetrato,
        `um id de sala repetido no arranjo do MOD duplicou a fileira da sala: ${quando}, ${conta.salas} li.voice_room ` +
          `e ${conta.entrar} ENTRAR para ${caso11.salasNoRetrato} salas`,
      );
      assert.equal(
        conta.canais, caso11.canaisNoRetrato,
        `um id de canal repetido no arranjo do MOD duplicou a fileira do canal: ${quando}, ${conta.canais} botões ` +
          `para ${caso11.canaisNoRetrato} canais`,
      );
    }
    assert.ok(caso11.mesmoEntrar, "caso 11: o ENTRAR da sala 1 trocou de nó com o tique");

    console.log(
      `linhas-pelo-id: com ${PESSOAS} pessoas, um PersonState mexeu só na linha da pessoa ` +
        `(${caso1.mutacoes} mutações, nenhuma fora dela) e o foco ficou; o mesmo retrato não ` +
        "mexeu em nada (e foi redesenhado); quem entrou na sala levou o nó; quem saiu perdeu o " +
        "dele e quem chegou ganhou um; a ordem é a do retrato, também depois de uma " +
        `reordenação; na coluna de salas, ${caso6.doEvento} mutações, todas na linha da ` +
        "pessoa, o ENTRAR NA SALA o mesmo nó, quem saiu da sala levou só a linha dela, e o " +
        "RENOMEAR abriu com o nome de agora; e nenhum nó de MOD saiu do documento nem trocou " +
        `de linha, ponto por ponto: ${caso5.join("; ")}. Na coluna de canais, o foco no ` +
        "segundo canal atravessou o tique com o mesmo retrato e com outro canal mudando de nome " +
        `(${outro.mutacoes} mutações, todas na fileira dele), quem saiu perdeu o nó, quem entrou ` +
        `ganhou um, o foco ficou no canal que virou o aberto, e os ${caso10m.nos} nós do canal.item ficaram ` +
        "no documento; um id repetido no arranjo do MOD não duplicou nenhuma sala nem canal.",
    );
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
