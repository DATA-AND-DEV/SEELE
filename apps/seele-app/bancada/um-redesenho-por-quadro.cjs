// **Um redesenho por quadro, por mais eventos que cheguem nele** (pendência 63;
// a `carga-cartoes` da M3 travou a página em 10/10).
//
// Cada pessoa da sessão manda um `PersonState` por segundo a todas as conexões,
// cada um vira um `RosterChanged`, e a janela respondia a cada um com um
// `atualizar()`: o retrato inteiro pela ponte e a lista inteira redesenhada. Com
// 251 pessoas e um cartão de MOD em cada, a fila crescia sem parar — medido em
// `tools/lista-grande.cjs`: 4,9 s de atraso em 5 s, e 68 quadros em vez de 600.
//
// Com o HTML, o CSS e o registro reais; só a ponte é simulada.
//
// # O que ela mede
//
// 1. **Uma rajada dentro de um quadro pede um retrato só**, e o desenho que sai
//    dele é o novo: 251 `RosterChanged` de uma vez, e depois de três quadros
//    houve um `snapshot` pedido, e o nome trocado está no que foi desenhado. O
//    `#tela-sessao` fica escondido durante o caso, para o tique de 500 ms da
//    sessão (que só puxa com ela à vista) não contar nem desenhar por ele.
// 2. **251 pessoas com cartões, sem fila**: 251 eventos por segundo por 4 s,
//    vindos de um Worker — de fora da página, como os da ponte; um
//    `setInterval` daqui atrasaria junto com ela e a rajada encolheria até
//    caber —, e nenhum espera mais de 1 s para ser atendido.
//
// Com o conserto revertido (um `atualizar()` por evento), o caso 1 conta 251
// retratos, e o caso 2, segundos de fila.
//
// Rodar:  node apps/seele-app/bancada/um-redesenho-por-quadro.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const PESSOAS = 251;
const JANELA_MS = 4000;
const ATRASO_MAXIMO_MS = 1000;

// A fonte dos eventos, servida da própria origem: a política da bancada é a do
// produto (`script-src 'self'`), e um Worker de `blob:` não passaria nela.
const RAJADA = `
onmessage = ({ data: { porSegundo, janela } }) => {
  const agora = () => performance.timeOrigin + performance.now();
  const fim = agora() + janela;
  const passo = 1000 / porSegundo;
  let proximo = agora();
  let enviados = 0;
  const tique = () => {
    const t = agora();
    while (proximo <= t && proximo < fim) {
      postMessage({ saiu: proximo });
      enviados += 1;
      proximo += passo;
    }
    if (t < fim) setTimeout(tique, 4);
    else postMessage({ acabou: true, enviados });
  };
  tique();
};`;

const pessoa = (id) => ({
  id,
  nickname: `p-${String(id).padStart(3, "0")}`,
  speaking: false,
  muted: false,
  total_isolation: false,
  signal: 92,
  sync_band: "Nominal",
  is_self: id === 1,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    // **Todos os ouvintes do canal**, e não o último: a casca registra vários
    // `listen("seele://event")`, e é o conjunto deles que um evento alcança.
    await page.addInitScript((table) => {
      window.testTable = table;
      window.retratosPedidos = 0;
      const ouvintes = new Map();
      window.emitir = (payload) => {
        for (const fn of ouvintes.get("seele://event") ?? []) fn({ payload });
      };
      window.__TAURI__ = {
        core: {
          invoke: async (cmd) => {
            // O retrato pela ponte: serializado de um lado, lido do outro.
            if (cmd === "snapshot") {
              retratosPedidos += 1;
              return JSON.parse(JSON.stringify(testTable.snapshot));
            }
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
    }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
    await page.route("**/__rajada.js", (rota) =>
      rota.fulfill({ contentType: "text/javascript", body: RAJADA }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    const r = retrato({ presentes: Array.from({ length: PESSOAS }, (_, i) => pessoa(i + 1)) });
    await page.evaluate((r) => {
      testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
      // O cartão de cada pessoa, do tamanho do da carga da M3 (32 linhas).
      entrarNaGeracao(1);
      const instancia = new InstanciaDeMod("m3/carga", "hash", geracaoDaSessao, {
        entregar: async () => {},
        encerrar: async () => {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("m3/carga", instancia);
      for (const p of r.presentes) {
        contribuicoesDosMods.registrar({ id: "m3/carga" }, instancia, {
          ponto: "pessoa.cartao", modo: "substituir", alvo: String(p.id), prioridade: 0,
          conteudo: Array.from({ length: 32 }, (_, i) => ({
            forma: "texto", chave: `l${i}`, dentro: `cartão ${p.nickname} · linha ${i + 1}`,
          })),
        });
      }
      desenhar(r);
    }, r);

    // 1. A rajada dentro de um quadro.
    const umQuadro = await page.evaluate(async (n) => {
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => ok()));
      $("tela-sessao").hidden = true;
      await quadro();
      await quadro();
      const antes = retratosPedidos;
      testTable.snapshot = structuredClone(testTable.snapshot);
      testTable.snapshot.presentes[1].nickname = "p-002 renomeada";
      for (let i = 0; i < n; i += 1) emitir("RosterChanged");
      await quadro();
      await quadro();
      await quadro();
      const pedidos = retratosPedidos - antes;
      const nome = desenhado?.presentes?.[1]?.nickname ?? null;
      $("tela-sessao").hidden = false;
      return { pedidos, nome };
    }, PESSOAS);
    assert.equal(
      umQuadro.pedidos, 1,
      `${PESSOAS} eventos num quadro pediram ${umQuadro.pedidos} retratos; um por quadro é o teto`,
    );
    assert.equal(
      umQuadro.nome, "p-002 renomeada",
      `a rajada não chegou à tela: o que foi desenhado ainda diz ${JSON.stringify(umQuadro.nome)}`,
    );

    // 2. A lista grande sob o ritmo do servidor.
    const sobARajada = await page.evaluate(async ({ porSegundo, janela }) => {
      const agora = () => performance.timeOrigin + performance.now();
      const worker = new Worker("/__rajada.js");
      let atrasoMaximo = 0;
      let atendidos = 0;
      let enviados = 0;
      let falha = null;
      const inicio = performance.now();
      await new Promise((ok) => {
        worker.onerror = (e) => { falha = e.message || "o worker não subiu"; ok(); };
        worker.onmessage = ({ data }) => {
          if (data.acabou) { enviados = data.enviados; ok(); return; }
          atendidos += 1;
          atrasoMaximo = Math.max(atrasoMaximo, agora() - data.saiu);
          emitir("RosterChanged");
        };
        worker.postMessage({ porSegundo, janela });
      });
      worker.terminate();
      return { falha, enviados, atendidos, atrasoMaximo, decorrido: performance.now() - inicio };
    }, { porSegundo: PESSOAS, janela: JANELA_MS });
    assert.equal(sobARajada.falha, null, `a rajada não saiu: ${sobARajada.falha}`);
    assert.ok(
      sobARajada.enviados >= PESSOAS * (JANELA_MS / 1000) * 0.9,
      `o Worker mandou ${sobARajada.enviados} eventos; a medida não tem a rajada que diz ter`,
    );
    assert.equal(sobARajada.atendidos, sobARajada.enviados, "a página não atendeu todos os eventos");
    assert.ok(
      sobARajada.atrasoMaximo < ATRASO_MAXIMO_MS,
      `com ${PESSOAS} pessoas e um cartão em cada, um evento esperou ` +
        `${Math.round(sobARajada.atrasoMaximo)} ms na fila (a janela de ${JANELA_MS} ms levou ` +
        `${Math.round(sobARajada.decorrido)} ms): a página está redesenhando por evento`,
    );

    assert.deepEqual(errors, []);
    console.log(
      `um-redesenho-por-quadro: ${PESSOAS} eventos num quadro pediram um retrato e chegaram à tela; ` +
        `${sobARajada.enviados} eventos em ${JANELA_MS / 1000} s com ${PESSOAS} cartões, ` +
        `o maior atraso ${Math.round(sobARajada.atrasoMaximo)} ms.`,
    );
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
