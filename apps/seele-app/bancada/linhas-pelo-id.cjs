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
//    duas vezes, nenhuma mutação na lista.
// 3. **Quem entra numa sala leva o próprio nó**: a linha é movida para o grupo
//    da sala, e não recriada; as outras ficam.
// 4. **Quem sai perde o nó, e quem chega ganha um**; as outras ficam.
// 5. **Com um cartão de MOD em cada linha**, um `PersonState` continua mexendo
//    só na linha da pessoa, e o cartão de cada uma é o mesmo nó, no documento,
//    na linha dela.
//
// Com o conserto revertido (a lista repovoada a cada retrato), o caso 1 conta
// mutações fora da linha e nós trocados em todas as outras.
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

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
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
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    await page.evaluate((r) => {
      testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, BASE);
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

    // 2. O mesmo retrato, de novo: o tique de 500 ms o redesenha, e nada muda.
    const caso2 = await page.evaluate(async () => {
      const parar = observarALista();
      await new Promise((ok) => setTimeout(ok, 1200));
      return parar().length;
    });
    assert.equal(caso2, 0, `o mesmo retrato, redesenhado pelo tique, fez ${caso2} mutações na lista`);

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

    // 5. Com um cartão de MOD em cada linha.
    const caso5 = await page.evaluate(async ({ alvo }) => {
      entrarNaGeracao(1);
      const instancia = new InstanciaDeMod("bancada/cartao", "hash", geracaoDaSessao, {
        entregar: async () => {},
        encerrar: async () => {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("bancada/cartao", instancia);
      for (const p of testTable.snapshot.presentes) {
        contribuicoesDosMods.registrar({ id: "bancada/cartao" }, instancia, {
          ponto: "pessoa.cartao", modo: "substituir", alvo: String(p.id), prioridade: 0,
          conteudo: [{ forma: "texto", chave: "c", dentro: `cartão de ${p.nickname}` }],
        });
      }
      await quadros(3);
      desenhar(testTable.snapshot);
      const cartaoDa = (li) => li?.querySelector(".contribuicao-de-mod") ?? null;
      const antes = linhasPorApelido();
      const cartoes = new Map([...antes].map(([nome, li]) => [nome, cartaoDa(li)]));
      const comCartao = [...cartoes.values()].filter(Boolean).length;
      const parar = observarALista();
      const novo = structuredClone(testTable.snapshot);
      const quem = novo.voice_rooms[0].people.find((p) => p.nickname === alvo);
      quem.muted = true;
      const chegou = await pelaPonte(novo, (d) =>
        d?.voice_rooms?.[0]?.people?.find((p) => p.nickname === alvo)?.muted === true);
      const registros = parar();
      const depois = linhasPorApelido();
      const donos = registros.map(donoDaMutacao);
      return {
        chegou,
        comCartao,
        foraDaLinha: donos.filter((d) => d !== alvo).length,
        mutacoes: registros.length,
        trocadas: [...antes].filter(([nome, li]) => depois.get(nome) !== li).length,
        cartoesTrocados: [...cartoes].filter(([nome, c]) =>
          !c || cartaoDa(depois.get(nome)) !== c || !c.isConnected).length,
        estado: depois.get(alvo)?.querySelector(".pastilha")?.textContent ?? null,
      };
    }, { alvo: apelido(5) });
    assert.ok(caso5.chegou, "o PersonState com cartões não chegou à tela");
    assert.equal(caso5.comCartao, PESSOAS - 1 + 1, `só ${caso5.comCartao} linhas desenharam o cartão do MOD`);
    assert.equal(
      caso5.foraDaLinha, 0,
      `com cartões, um PersonState de ${apelido(5)} mexeu fora da linha dela: ` +
        `${caso5.foraDaLinha} de ${caso5.mutacoes} mutações`,
    );
    assert.equal(caso5.trocadas, 0, `com cartões, ${caso5.trocadas} linhas trocaram de nó`);
    assert.equal(caso5.cartoesTrocados, 0, `${caso5.cartoesTrocados} cartões de MOD trocaram de nó ou saíram da linha`);
    assert.equal(caso5.estado, "MUDO", `a linha de ${apelido(5)} não diz que ela está muda`);

    assert.deepEqual(errors, []);
    console.log(
      `linhas-pelo-id: com ${PESSOAS} pessoas, um PersonState mexeu só na linha da pessoa ` +
        `(${caso1.mutacoes} mutações, nenhuma fora dela) e o foco ficou; o mesmo retrato não ` +
        "mexeu em nada; quem entrou na sala levou o nó; quem saiu perdeu o dele e quem chegou " +
        `ganhou um; com cartões de MOD, ${caso5.mutacoes} mutações, todas na linha, e nenhum ` +
        "cartão trocado.",
    );
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
