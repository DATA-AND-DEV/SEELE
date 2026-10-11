// **A lista grande sob a rajada de `PersonState`** — medida, e não guarda
// (pedido da orquestradora, 10/10, depois da `carga-cartoes` travar a página).
//
// Na M3, com 251 pessoas e o MOD de carga pondo um `pessoa.cartao` no teto em
// cada uma e as 128 contribuições sem alvo, o WebContent ficou acima de 100% de
// CPU por minutos e nenhum cartão apareceu; com 10 pessoas, apareceram na hora.
// A suspeita, pela leitura: cada sessão manda um `PersonState` por segundo
// (`TELEMETRY_INTERVAL`), cada um marca `changed.roster`, que vira um
// `Event::RosterChanged` sem junção (`seele-ffi`, `notify`), e cada evento faz
// a janela chamar `atualizar()` — o retrato inteiro pela ponte e o `desenhar`
// inteiro. Com N pessoas, N redesenhos completos por segundo.
//
// Aqui, com o HTML, o CSS e o registro reais, e só a ponte simulada (o retrato
// passa por JSON ida e volta, como pelo IPC): o custo de um `desenhar`, e 5 s
// de N eventos por segundo vindos de um Worker — de fora da página, como os da
// ponte — atendidos de três jeitos: nenhum (`tique`, só o `setInterval` de
// 500 ms da sessão), um `atualizar()` por evento (`rajada`, o que a janela faz
// hoje) e no máximo um por quadro (`juntada`). Sai o atraso máximo de um
// evento (a fila), quantos quadros a página pintou, e onde o tempo dela foi,
// pelo próprio Chromium.
//
// Medido em 10/10 (macOS, Chromium do Playwright; o WebKit da janela não
// entra, e a ponte de verdade custa mais que o JSON daqui):
//
//   251 pessoas, cartões + 128, rajada: a janela de 5 s levou 9,9 s, o último
//     evento esperou 4,9 s na fila, 68 quadros em vez de 600 — a fila cresce
//     enquanto os eventos chegarem, e eles não param;
//   o mesmo, juntada: fila de 0,1 s, mas a página a 99% (88 quadros);
//   o mesmo, tique: 14% e 548 quadros;
//   só os cartões, rajada: 100% com 100 pessoas, fila a partir de 200, 3,5 s
//     com 251; só as 128 sem alvo, com 251: 98%, sem fila;
//   sem MOD nenhum, 251, rajada: 99% e 232–377 quadros.
//
// Depois do conserto (`6717dc2c`, um `atualizar()` por quadro), pelo ouvinte
// de verdade (`evento`): 251 com cartões + 128, fila de 0,1 s e a página a
// 99% (93 quadros); só os cartões, 100% (138); sem MOD, 98% (558). A fila
// acabou; o custo de refazer a lista inteira a cada quadro, não.
//
// Rodar:  node tools/lista-grande.cjs [pessoas:variante:modo ...]
// (sem argumento, 11 e 251 pessoas × sem e com o MOD × os quatro modos; a
// variante é `sem`, `cartoes`, `agregado` ou `mod`, que é as duas juntas)

// Em `tools/`, e não em `bancada/`: é uma medida, que imprime uma tabela e
// não reprova nada, e lá toda `.cjs` é prova que o CI roda
// (`toda_bancada_da_pasta_roda_em_algum_lugar`). O servidor e o Playwright são
// os das bancadas.
const { servir, respostas, retrato } = require("../apps/seele-app/bancada/telas.cjs");
const { chromium } = require("../apps/seele-app/bancada/playwright.cjs");

// A fonte dos eventos, num Worker servido da própria origem: a política da
// bancada é a do produto (`script-src 'self'`), e um Worker de `blob:` não
// passaria nela.
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

async function medir(browser, porta, n, variante, modo) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const erros = [];
  page.on("pageerror", (e) => erros.push(e.message));
  const r = retrato({ presentes: Array.from({ length: n }, (_, i) => pessoa(i + 1)) });
  await page.addInitScript((tabela) => {
    window.testTable = tabela;
    window.__TAURI__ = {
      core: {
        invoke: async (cmd) => {
          // O retrato pela ponte: serializado de um lado, lido do outro.
          if (cmd === "snapshot") return JSON.parse(JSON.stringify(testTable.snapshot));
          return testTable[cmd] ?? null;
        },
      },
      // Os ouvintes de verdade, para o modo `evento` passar pelo da sessão.
      event: {
        listen: async (nome, fn) => {
          (window.__ouvintes ??= {})[nome] = [...(window.__ouvintes[nome] ?? []), fn];
          return () => {};
        },
      },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
  await page.route("**/__rajada.js", (rota) =>
    rota.fulfill({ contentType: "text/javascript", body: RAJADA }));
  // Onde o tempo da página vai, pelo próprio Chromium: script, estilo e
  // layout, lidos no começo e no fim da janela.
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Performance.enable");
  const marcas = {};
  await page.exposeFunction("marcarMetricas", async (nome) => {
    const { metrics } = await cdp.send("Performance.getMetrics");
    marcas[nome] = Object.fromEntries(metrics.map((m) => [m.name, m.value]));
  });
  await page.goto(`http://127.0.0.1:${porta}`);
  return page.evaluate(async ({ r, variante, modo }) => {
    testTable.snapshot = r;
    $("tela-boot").hidden = true;
    $("tela-sessao").hidden = false;
    desenhar(r);
    // `cartoes`: um `pessoa.cartao` por pessoa, só. `agregado`: só as 128 sem
    // alvo. `mod`: as duas, como a variante `cartoes` da M3.
    if (variante !== "sem") {
      entrarNaGeracao(1);
      const instancia = new InstanciaDeMod("m3/carga", "hash", geracaoDaSessao, {
        entregar: async () => {},
        encerrar: async () => {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("m3/carga", instancia);
      const linhas = (prefixo) =>
        Array.from({ length: 32 }, (_, i) => ({ forma: "texto", chave: `l${i}`, dentro: `${prefixo} · linha ${i + 1}` }));
      for (const p of variante === "agregado" ? [] : r.presentes) {
        contribuicoesDosMods.registrar({ id: "m3/carga" }, instancia, {
          ponto: "pessoa.cartao", modo: "substituir", alvo: String(p.id), prioridade: 0,
          conteudo: linhas(`cartão ${p.nickname}`),
        });
      }
      const pontos = ["redes", "operador.barra", "compositor", "status.barra", "pessoas.lista", "compositor.ferramentas"];
      for (let i = 0; i < (variante === "cartoes" ? 0 : 128); i += 1) {
        contribuicoesDosMods.registrar({ id: "m3/carga" }, instancia, {
          ponto: pontos[i % pontos.length], modo: "adicionar", prioridade: 0,
          conteudo: linhas(`agregado ${i + 1}`),
        });
      }
    }
    const umQuadro = () => new Promise((ok) => requestAnimationFrame(() => ok()));
    // Um `desenhar` sozinho, com o quadro que ele pinta.
    const custos = [];
    for (let i = 0; i < 6; i += 1) {
      const t0 = performance.now();
      desenhar(JSON.parse(JSON.stringify(r)));
      await umQuadro();
      custos.push(performance.now() - t0);
    }
    custos.sort((a, b) => a - b);
    // Os cartões do MOD que a lista mostra: a primeira linha de cada um.
    const cartoesNaLista = () => [...document.querySelectorAll("#tela-sessao *")]
      .filter((e) => e.childElementCount === 0 && /^cartão p-\d{3} · linha 1$/.test(e.textContent)).length;
    const cartoes = cartoesNaLista();
    let quadros = 0;
    let contando = true;
    const contar = () => { if (contando) { quadros += 1; requestAnimationFrame(contar); } };
    requestAnimationFrame(contar);
    let ocupado = 0;
    let feitos = 0;
    let pedidos = 0;
    const original = window.desenhar;
    window.desenhar = (s) => { const t0 = performance.now(); original(s); ocupado += performance.now() - t0; feitos += 1; };
    const janela = 5000;
    await marcarMetricas("antes");
    const inicio = performance.now();
    // **Os eventos vêm de fora da página**, como os da ponte: um Worker os põe
    // na fila da página no ritmo do servidor, ela esteja livre ou não. Um
    // `setInterval` daqui não serviria — com a página ocupada ele mesmo
    // atrasa, e a rajada encolhe até caber (foi o que a medida anterior fez:
    // 98 de 1255). Cada evento leva a hora em que saiu; o atraso de quem é
    // atendido é a fila que a página tem pela frente.
    //
    // O tique: só o `setInterval` de 500 ms da sessão. A rajada: um
    // `atualizar()` por evento, como `tela-sessao.js` fazia até a pendência 63.
    // A juntada: no máximo um por quadro, e os que chegam no meio só marcam que
    // há o que pedir. O evento: um `RosterChanged` pelos ouvintes de verdade —
    // o que a janela faz com ele hoje, seja o que for.
    const agora = () => performance.timeOrigin + performance.now();
    const worker = modo === "tique" ? null : new Worker("/__rajada.js");
    let falhaDoWorker = null;
    if (worker) worker.onerror = (e) => { falhaDoWorker = e.message || "o worker não subiu"; acabou(); };
    let pendente = false;
    let atendidos = 0;
    let ultimoAtraso = 0;
    let atrasoMaximo = 0;
    let enviados = 0;
    let acabou = null;
    const acabado = new Promise((ok) => { acabou = ok; });
    const pedir = () => { pedidos += 1; atualizar(); };
    if (worker) {
      worker.onmessage = ({ data }) => {
        if (data.acabou) { enviados = data.enviados; acabou(); return; }
        atendidos += 1;
        ultimoAtraso = agora() - data.saiu;
        atrasoMaximo = Math.max(atrasoMaximo, ultimoAtraso);
        if (modo === "rajada") { pedir(); return; }
        if (modo === "evento") {
          for (const fn of window.__ouvintes?.["seele://event"] ?? []) fn({ payload: "RosterChanged" });
          return;
        }
        if (pendente) return;
        pendente = true;
        requestAnimationFrame(() => { pendente = false; pedir(); });
      };
      worker.postMessage({ porSegundo: r.presentes.length, janela });
    } else {
      acabou();
    }
    await new Promise((ok) => setTimeout(ok, janela));
    const decorrido = performance.now() - inicio;
    contando = false;
    await marcarMetricas("depois");
    const feitosNaJanela = feitos;
    const ocupadoNaJanela = ocupado;
    const atendidosNaJanela = atendidos;
    const atrasoNoFim = ultimoAtraso;
    // Quanto a fila leva para esvaziar depois que o Worker para de mandar: até
    // a página atender o último aviso dele, ou 120 s.
    const antesDeEsvaziar = performance.now();
    await Promise.race([acabado, new Promise((ok) => setTimeout(ok, 120000))]);
    const esvaziar_ms = performance.now() - antesDeEsvaziar;
    worker?.terminate();
    // Um Worker que não subiu mede uma página parada, e a tabela sairia limpa.
    if (falhaDoWorker || (worker && enviados === 0)) {
      throw new Error(`a rajada não saiu: ${falhaDoWorker ?? "nenhum evento enviado"}`);
    }
    window.desenhar = original;
    return {
      pessoas: r.presentes.length,
      variante,
      desenhar_ms: Number(custos[Math.floor(custos.length / 2)].toFixed(1)),
      cartoes_antes: cartoes,
      cartoes_depois: cartoesNaLista(),
      modo,
      pedidos,
      esvaziar_s: Number((esvaziar_ms / 1000).toFixed(1)),
      atualizar_feitos: feitosNaJanela,
      eventos_atendidos: atendidosNaJanela,
      eventos_enviados: enviados,
      atraso_no_fim_s: Number((atrasoNoFim / 1000).toFixed(1)),
      atraso_maximo_s: Number((atrasoMaximo / 1000).toFixed(1)),
      janela_s: Number((decorrido / 1000).toFixed(1)),
      ocupado_pct: Number(((100 * ocupadoNaJanela) / decorrido).toFixed(0)),
      quadros_em_5s: quadros,
    };
  }, { r, variante, modo }).finally(() => page.close()).then((m) => {
    const delta = (nome) => (marcas.depois[nome] - marcas.antes[nome]) * 1000;
    const pct = (nome) => Math.round((100 * delta(nome)) / (m.janela_s * 1000));
    return {
      ...m,
      erros: erros.length,
      script_pct: pct("ScriptDuration"),
      estilo_pct: pct("RecalcStyleDuration"),
      layout_pct: pct("LayoutDuration"),
      tarefas_pct: pct("TaskDuration"),
    };
  });
}

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const porta = server.address().port;
    const linhas = [];
    const casos = process.argv.slice(2);
    if (casos.length === 0) {
      for (const n of [11, 251]) {
        for (const variante of ["sem", "mod"]) {
          for (const modo of ["tique", "rajada", "juntada", "evento"]) casos.push(`${n}:${variante}:${modo}`);
        }
      }
    }
    for (const caso of casos) {
      const [n, variante, modo] = caso.split(":");
      if (!["sem", "cartoes", "agregado", "mod"].includes(variante) || !["tique", "rajada", "juntada", "evento"].includes(modo)) {
        throw new Error(`caso desconhecido: ${caso} (pessoas:sem|cartoes|agregado|mod:tique|rajada|juntada|evento)`);
      }
      linhas.push(await medir(browser, porta, Number(n), variante, modo));
    }
    console.log("| pessoas | MOD | modo | um desenhar (ms) | janela real | eventos enviados | atendidos na janela | atraso máx. | desenhos | quadros | página ocupada | script | estilo | layout | fila esvazia em | cartões | erros |");
    console.log("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (const l of linhas) {
      console.log(`| ${l.pessoas} | ${{ sem: "sem", cartoes: "só cartões", agregado: "só 128 sem alvo", mod: "cartões + 128" }[l.variante]} | ${l.modo} | ${l.desenhar_ms} | ${l.janela_s} s | ${l.eventos_enviados} | ${l.eventos_atendidos} | ${l.atraso_maximo_s} s | ${l.atualizar_feitos} | ${l.quadros_em_5s} | ${l.tarefas_pct}% | ${l.script_pct}% | ${l.estilo_pct}% | ${l.layout_pct}% | ${l.esvaziar_s} s | ${l.cartoes_antes} → ${l.cartoes_depois} | ${l.erros} |`);
    }
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
