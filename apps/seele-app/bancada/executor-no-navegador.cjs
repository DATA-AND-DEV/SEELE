// O executor dos MODs no navegador, num Chromium de verdade, com o motor de
// verdade (ui/mods-no-navegador.js, ui/executor-de-mod.html e o `.wasm` do
// `seele-motor-web`).
//
//     node apps/seele-app/bancada/executor-no-navegador.cjs \
//       target/wasm32-wasip1/release/seele_motor_web.wasm
//
// O `.wasm` vem do `cargo xtask web`, e por isso ela roda no job
// `bancadas-de-mod` do `ci.yml`, depois dele, e não no `bancadas`.
//
// O que só se prova aqui é o isolamento: que o MOD não alcança a origem da
// página, nem a rede, nem um armazenamento que sobreviva; e que, se ele um dia
// sair do motor, a segunda parede (o `iframe` opaco e a CSP dele) ainda segura.
// Quem dá o veredito da rede é o servidor desta bancada, que conta cada pedido
// às canárias, e não o MOD.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const daUi = (nome) => path.join(raiz, "ui", nome);
const [caminhoDoMotor] = process.argv.slice(2);
if (!caminhoDoMotor || !fs.existsSync(caminhoDoMotor)) {
  console.error("uso: node apps/seele-app/bancada/executor-no-navegador.cjs <seele_motor_web.wasm>  (o `cargo xtask web` o compila)");
  process.exit(2);
}

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("executor-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};

// ---------------------------------------------------------------- a página

// **Uma cópia**, e não a `ui/`: um arquivo de bancada esquecido lá iria para o
// instalador (`tools/carga-da-casca.py` diz o mesmo).
const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "executor-no-navegador-"));
for (const nome of ["mods-runtime.js", "mods-no-navegador.js", "executor-de-mod.html"]) {
  fs.copyFileSync(daUi(nome), path.join(pasta, nome));
}
fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
fs.writeFileSync(
  path.join(pasta, "index.html"),
  '<!doctype html><meta charset="utf-8"><title>bancada</title>\n' +
    '<script src="mods-runtime.js"></script>\n<script src="mods-no-navegador.js"></script>\n',
);

// O Worker que saiu do motor: o que um defeito do QuickJS **e do navegador**
// daria a um MOD. Ele roda no lugar do anfitrião, depois do preâmbulo, tenta
// tudo direto e conta o que conseguiu.
const WORKER_FUGIDO = `
"use strict";
const r = {};
const tentar = async (nome, fn) => {
  try { r[nome] = "ok: " + String(await Promise.race([fn(), new Promise((ok) => setTimeout(() => ok("(sem resposta)"), 1500))])).slice(0, 80); }
  catch (e) { r[nome] = "lançou: " + String((e && e.name) || e).slice(0, 80); }
};
onmessage = async (e) => {
  if (e.data.tipo !== "subir") return;
  const c = e.data.fonte;
  r.origem = String(self.origin);
  await tentar("fetch_mesma_origem", () => fetch(c + "/fetch").then((x) => x.status));
  await tentar("websocket", () => new Promise((ok, falha) => { const w = new WebSocket(c.replace("http", "ws") + "/ws"); w.onopen = () => ok("abriu"); w.onerror = () => falha(new Error("erro")); }));
  await tentar("importScripts", () => { importScripts(c + "/import.js"); return "importou"; });
  await tentar("indexedDB", () => new Promise((ok, falha) => { const q = indexedDB.open("seele-identidade"); q.onsuccess = () => ok("abriu"); q.onerror = () => falha(q.error); }));
  await tentar("caches", () => caches.open("x").then(() => "abriu"));
  // A marca de uma abertura anterior, e a desta.
  await tentar("caches_marca_anterior", () => caches.keys().then((k) => k.filter((n) => n.startsWith("marca-")).join(",") || "(nenhuma)"));
  await tentar("caches_gravar", () => caches.open("marca-" + Date.now()).then((c) => c.put("https://x.invalid/m", new Response("1"))).then(() => "gravou"));
  await tentar("opfs", () => navigator.storage.getDirectory().then(() => "abriu"));
  // Um Worker aninhado tem um global novo: o que este Worker tirou de si, ele
  // teria de volta.
  await tentar("worker_aninhado", () => new Promise((ok, falha) => {
    const u = URL.createObjectURL(new Blob([\`(async () => { const r = []; try { await caches.open("aninhado"); r.push("caches abriu"); } catch (e) { r.push("caches: " + e.name); } try { indexedDB.open("x"); r.push("idb abriu"); } catch (e) { r.push("idb: " + e.name); } postMessage(r.join("; ")); })();\`], { type: "text/javascript" }));
    const w = new Worker(u); w.onmessage = (e) => ok(e.data); w.onerror = (e) => falha(new Error("erro: " + e.message));
  }));
  await tentar("broadcast", () => { new BroadcastChannel("seele").postMessage("fugi"); return "postou"; });
  postMessage({ tipo: "mensagem", corpo: JSON.stringify({ n: 1, tipo: "fuga", valor: r }) });
  postMessage({ tipo: "subiu" });
};
`;

// ---------------------------------------------------------------- o servidor

const canarias = [];
const TIPOS = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };
let iframeDaVez = null;

/**
 * O `iframe` de verdade, com outro anfitrião no lugar do do motor e o hash da
 * CSP refeito. O preâmbulo e o resto do `iframe` continuam os do produto.
 */
function iframeCom(anfitriao) {
  const html = fs.readFileSync(path.join(pasta, "executor-de-mod.html"), "utf8");
  const trocado = html.replace(/  \/\/ <anfitrião>[\s\S]*?\/\/ <\/anfitrião>\n/, () => `  function anfitriaoDoMotor() {\n${anfitriao}\n  }\n`);
  if (trocado === html) throw new Error("a bancada não achou o anfitrião no iframe: as marcas <anfitrião> saíram?");
  const script = trocado.split("<script>")[1].split("</script>")[0];
  const hash = crypto.createHash("sha256").update(script).digest("base64");
  return trocado.replace(/'sha256-[^']*'/, `'sha256-${hash}'`);
}

const servidor = http.createServer((pedido, resposta) => {
  const url = new URL(pedido.url, "http://x");
  if (url.pathname.startsWith("/canario")) {
    canarias.push(`${pedido.method} ${url.pathname} de ${pedido.headers.origin ?? "-"}`);
    resposta.writeHead(204, { "access-control-allow-origin": "*" });
    resposta.end();
    return;
  }
  const nome = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
  const arquivo = path.join(pasta, nome);
  if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo)) {
    resposta.writeHead(404);
    resposta.end();
    return;
  }
  // Os cabeçalhos da origem (`web/_headers`): a CSP da página nela, e o
  // `iframe` com a dele no `<meta>`, pedindo só para ser emoldurado pela página.
  const cabecalhos = {
    ...cabecalhosDaOrigem(`/${nome}`),
    "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream",
    "cache-control": "no-store",
  };
  let corpo = fs.readFileSync(arquivo);
  if (nome === "executor-de-mod.html" && iframeDaVez !== null) corpo = iframeDaVez;
  resposta.writeHead(200, cabecalhos);
  resposta.end(corpo);
});

// ---------------------------------------------------------------- os casos

const MOD_COMUM = fs.readFileSync(path.resolve(raiz, "../../crates/seele-motor-web/tests/mod-comum.js"), "utf8");

/** Sobe um MOD com o executor e uma janela de mentira que responde como a casca. */
const NA_PAGINA = `
window.bancada = {
  async subir(fonte, { capacidades = ["regiao", "tema", "cartoes", "arquivo", "superficies", "contribuicoes", "estilos", "classes", "volume"], lento = 0, mudo = false } = {}) {
    const falas = [], falhas = [];
    const executor = executorNoNavegador("bancada", 1, "h");
    const instancia = new InstanciaDeMod("bancada", "h", 1, executor);
    instancia.estado = ESTADOS_DE_MOD.ativa;
    const responder = (m) => {
      if (mudo || typeof m.n !== "number") return;
      const r = m.tipo === "snapshot" ? { ok: true, valor: { servidor: "bancada" } }
        : m.tipo === "pedido" && m.id === "eco" ? { ok: true, valor: m.valor }
        : m.tipo === "regiao" ? { ok: true, valor: null }
        : { ok: false, erro: "desconhecido" };
      executor.entregar({ tipo: "resposta", n: m.n, ...r }).catch((e) => falhas.push("entregar: " + e.message));
    };
    const t0 = performance.now();
    await executor.iniciar({ fonte, capacidades }, async (m) => {
      falas.push(m);
      if (lento) await new Promise((ok) => setTimeout(ok, lento));
      responder(m);
    }, (e) => falhas.push(String(e)));
    return { executor, instancia, falas, falhas, subida_ms: performance.now() - t0 };
  },
  esperar(teste, ms = 8000) {
    return new Promise((ok) => {
      const t0 = performance.now();
      const ver = () => { const v = teste(); if (v) ok(v); else if (performance.now() - t0 > ms) ok(null); else setTimeout(ver, 10); };
      ver();
    });
  },
  // O maior intervalo entre duas batidas de 10 ms: quanto a página travou.
  monitorar() {
    let ultimo = performance.now(), maior = 0;
    const id = setInterval(() => { const a = performance.now(); maior = Math.max(maior, a - ultimo); ultimo = a; }, 10);
    return () => { clearInterval(id); return Math.round(maior); };
  },
};
`;

async function caso(pagina, nome, corpo) {
  try {
    await corpo();
  } catch (falha) {
    falhas.push(`${nome}: lançou ${falha.stack ?? falha}`);
  }
}

(async () => {
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const base = `http://127.0.0.1:${servidor.address().port}`;
  const navegador = await chromium().launch();
  try {
    const nova = async () => {
      const pagina = await navegador.newPage();
      const erros = [];
      const consoles = [];
      pagina.on("pageerror", (e) => erros.push(String(e)));
      pagina.on("console", (m) => consoles.push(`${m.type()}: ${m.text()}`));
      await pagina.goto(`${base}/`);
      await pagina.evaluate(NA_PAGINA);
      return { pagina, erros, consoles };
    };

    // 1. O MOD comum fala a API do desktop, por todo o caminho.
    await caso(null, "MOD comum", async () => {
      const { pagina, erros } = await nova();
      const r = await pagina.evaluate(async (fonte) => {
        const b = await bancada.subir(fonte);
        const pronto = await bancada.esperar(() => b.falas.find((m) => m.canal === "pronto"));
        return { pronto: pronto?.valor ?? null, falhas: b.falhas, subida_ms: b.subida_ms };
      }, MOD_COMUM);
      confere("MOD comum", r.pronto, `não chegou ao pronto (falhas: ${JSON.stringify(r.falhas)}; erros da página: ${erros})`);
      if (r.pronto) {
        confere("MOD comum", r.pronto.snap?.servidor === "bancada", `snapshot ${JSON.stringify(r.pronto.snap)}`);
        confere("MOD comum", r.pronto.intervalos === 3, `intervalos ${r.pronto.intervalos}`);
        confere("MOD comum", r.pronto.erro === "rejeitou: desconhecido", `erro ${r.pronto.erro}`);
        confere("MOD comum", String(r.pronto.grande).startsWith("mensagem-grande"), `grande ${r.pronto.grande}`);
        confere("MOD comum", r.pronto.superficies === "object", `superficies ${r.pronto.superficies}`);
      }
      console.log(`  MOD comum: ${r.pronto ? "pronto" : "NÃO"} em ${Math.round(r.subida_ms)} ms de subida`);
      await pagina.close();
    });

    // 2. O ciclo de vida: `InstanciaDeMod.encerrar` chega a `encerrada`.
    await caso(null, "encerrar", async () => {
      const { pagina } = await nova();
      const r = await pagina.evaluate(async (fonte) => {
        const b = await bancada.subir(fonte);
        await bancada.esperar(() => b.falas.find((m) => m.canal === "pronto"));
        const t0 = performance.now();
        const encerrou = await b.instancia.encerrar();
        return { encerrou, estado: b.instancia.estado, ms: performance.now() - t0, pend: b.executor.pendencias(), depois: await b.executor.entregar({ x: 1 }).then(() => "aceitou", (e) => e.message) };
      }, MOD_COMUM);
      confere("encerrar", r.encerrou === true && r.estado === "encerrada", `encerrou ${r.encerrou}, estado ${r.estado}`);
      confere("encerrar", r.depois === "instancia-nao-reservada", `depois de encerrada, entregar: ${r.depois}`);
      console.log(`  encerrar: ${r.estado} em ${Math.round(r.ms)} ms`);
      await pagina.close();
    });

    // 3. Um laço sem fim é cortado, a linha do registro diz o teto, e a página
    //    não trava. As frases são as do desktop: o `Falhou` e a linha vêm da
    //    mesma volta (`seele-motor`).
    await caso(null, "laço sem fim", async () => {
      const { pagina, consoles } = await nova();
      const r = await pagina.evaluate(async () => {
        const b = await bancada.subir("SeeleUI.aoEvento(() => { while (true) {} });");
        const parar = bancada.monitorar();
        await b.executor.entregar({ tipo: "evento", nome: "x" });
        const falhou = await bancada.esperar(() => b.falhas.find((f) => f === "o MOD lançou"));
        return { falhou, falhas: b.falhas, travou_ms: parar() };
      });
      confere("laço sem fim", r.falhou === "o MOD lançou", `falhas: ${JSON.stringify(r.falhas)}`);
      confere("laço sem fim", consoles.some((c) => c.startsWith("error: MOD bancada:") && c.includes("passou do prazo de 500 ms")),
        `a linha do teto não chegou ao console da página: ${JSON.stringify(consoles)}`);
      confere("laço sem fim", r.travou_ms < 200, `a página travou ${r.travou_ms} ms`);
      console.log(`  laço sem fim: «${r.falhou}», a página travou no máximo ${r.travou_ms} ms`);
      await pagina.close();
    });

    // 4. A fila de saída: um MOD que posta sem parar recebe a recusa, e a
    //    página não acumula mais do que o teto.
    await caso(null, "fila de saída", async () => {
      const { pagina } = await nova();
      const r = await pagina.evaluate(async () => {
        // A página lê devagar (5 ms cada). Depois que ela leu tudo, o MOD
        // conta o que o `postar` aceitou: a fila voltou a aceitar.
        const b = await bancada.subir(`
          let aceitas = 0, recusadas = 0;
          for (let i = 0; i < 200; i++) { if (seele.postar(JSON.stringify({ tipo: "x", i }))) aceitas++; else recusadas++; }
          setTimeout(() => seele.postar(JSON.stringify({ tipo: "conta", aceitas, recusadas })), 1500);
        `, { lento: 5, mudo: true });
        const conta = await bancada.esperar(() => b.falas.find((m) => m.tipo === "conta"), 5000);
        return { falas: b.falas.length, conta, falhas: b.falhas };
      });
      confere("fila de saída", r.conta?.aceitas === 64 && r.conta?.recusadas === 136, `o MOD contou ${JSON.stringify(r.conta)}`);
      confere("fila de saída", r.falas === 65, `a página recebeu ${r.falas} mensagens`);
      // Quem recusa é o motor: o `iframe` só mata um Worker que passe do teto,
      // e aqui ele não pode ter precisado.
      confere("fila de saída", r.falhas.length === 0, `falhas: ${JSON.stringify(r.falhas)}`);
      console.log(`  fila de saída: o MOD contou ${JSON.stringify(r.conta)}, e a fila voltou a aceitar`);
      await pagina.close();
    });

    // 5. A fila de entrada: entregar além do teto é recusado e dito.
    await caso(null, "fila de entrada", async () => {
      const { pagina } = await nova();
      const r = await pagina.evaluate(async () => {
        const b = await bancada.subir("SeeleUI.aoEvento(() => {});");
        const resultados = await Promise.all(Array.from({ length: 80 }, (_, i) => b.executor.entregar({ tipo: "evento", nome: "e" + i }).then(() => "ok", (e) => e.message)));
        return { aceitas: resultados.filter((x) => x === "ok").length, recusa: resultados.find((x) => x !== "ok") };
      });
      confere("fila de entrada", r.aceitas === 64 && r.recusa === "a fila de entrada do executor está cheia", JSON.stringify(r));
      console.log(`  fila de entrada: ${r.aceitas} aceitas, depois «${r.recusa}»`);
      await pagina.close();
    });

    // 6. Parar antes de o motor chegar: o MOD não sobe.
    await caso(null, "parada antes do motor", async () => {
      const { pagina } = await nova();
      const r = await pagina.evaluate(async () => {
        const executor = executorNoNavegador("bancada", 1, "h");
        const falas = [];
        const subida = executor.iniciar({ fonte: "seele.postar(JSON.stringify({ tipo: 'subi' }))", capacidades: [] }, (m) => falas.push(m), () => {});
        executor.pedirEncerramento();
        await subida;
        const encerrou = await executor.encerrou();
        await new Promise((ok) => setTimeout(ok, 300));
        return { encerrou, falas: falas.length };
      });
      confere("parada antes do motor", r.encerrou === true && r.falas === 0, JSON.stringify(r));
      await pagina.close();
    });

    // 7. O MOD dentro do motor: nenhuma das portas do navegador existe.
    await caso(null, "MOD hostil no motor", async () => {
      const { pagina } = await nova();
      const r = await pagina.evaluate(async (canario) => {
        const b = await bancada.subir(`
          const portas = ["window", "document", "parent", "self", "postMessage", "fetch", "XMLHttpRequest", "WebSocket", "indexedDB", "localStorage", "caches", "importScripts", "navigator", "WebAssembly", "Worker", "BroadcastChannel"];
          const achadas = portas.filter((p) => typeof globalThis[p] !== "undefined");
          let fuga = "não tentou";
          try { fuga = String(Function("return this")().fetch); } catch (e) { fuga = "lançou: " + e; }
          SeeleMods.request("eco", "hostil", { achadas, fuga });
        `);
        const fala = await bancada.esperar(() => b.falas.find((m) => m.canal === "hostil"));
        return fala?.valor ?? { falhas: b.falhas };
      }, `${base}/canario`);
      confere("MOD hostil no motor", Array.isArray(r.achadas) && r.achadas.length === 0, `o MOD alcança ${JSON.stringify(r)}`);
      console.log(`  MOD hostil no motor: portas achadas ${JSON.stringify(r.achadas)}`);
      await pagina.close();
    });

    // 8. A segunda parede: um Worker que saiu do motor.
    await caso(null, "Worker fugido", async () => {
      iframeDaVez = iframeCom(WORKER_FUGIDO);
      const { pagina } = await nova();
      await pagina.evaluate(async () => {
        const db = indexedDB.open("seele-identidade", 1);
        await new Promise((ok) => { db.onupgradeneeded = () => db.result.createObjectStore("chaves"); db.onsuccess = ok; });
      });
      const antes = canarias.length;
      const r = await pagina.evaluate(async (canario) => {
        const b = await bancada.subir(canario, { mudo: true });
        const fala = await bancada.esperar(() => b.falas.find((m) => m.tipo === "fuga"));
        return fala?.valor ?? { falhas: b.falhas };
      }, `${base}/canario`);
      iframeDaVez = null;
      const tocaram = canarias.slice(antes);
      confere("Worker fugido", tocaram.length === 0, `as canárias tocaram: ${tocaram.join("; ")}`);
      confere("Worker fugido", r.origem === "null", `a origem do Worker é ${r.origem}`);
      for (const porta of ["fetch_mesma_origem", "websocket", "importScripts", "indexedDB", "caches", "caches_gravar", "opfs", "worker_aninhado"]) {
        confere("Worker fugido", String(r[porta]).startsWith("lançou"), `${porta}: ${r[porta]}`);
      }
      console.log(`  Worker fugido: ${JSON.stringify(r)}`);
      await pagina.close();
    });

    // 9. A terceira linha: um Worker fugido que inunda a página é encerrado
    //    pelo `iframe` no teto que o motor garantiria.
    await caso(null, "Worker que inunda", async () => {
      iframeDaVez = iframeCom(`onmessage = (e) => { if (e.data.tipo !== "subir") return; postMessage({ tipo: "subiu" }); for (let i = 0; i < 500; i++) postMessage({ tipo: "mensagem", corpo: JSON.stringify({ i }) }); };`);
      const { pagina } = await nova();
      const r = await pagina.evaluate(async () => {
        const b = await bancada.subir("", { mudo: true, lento: 5 });
        await bancada.esperar(() => b.falhas.length > 0, 3000);
        const encerrou = await b.executor.encerrou();
        return { falas: b.falas.length, falhas: b.falhas, encerrou };
      });
      iframeDaVez = null;
      confere("Worker que inunda", r.falas <= 64 && r.falhas.some((f) => f.includes("passou do teto")), JSON.stringify(r).slice(0, 300));
      confere("Worker que inunda", r.encerrou === true, `a parada não foi confirmada: ${r.encerrou}`);
      console.log(`  Worker que inunda: ${r.falas} chegaram, e o iframe o encerrou`);
      await pagina.close();
    });
  } finally {
    await navegador.close();
    servidor.close();
    fs.rmSync(pasta, { recursive: true, force: true });
  }

  terminou = true;
  if (falhas.length) {
    console.error(`executor-no-navegador: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
    process.exit(1);
  }
  console.log("executor-no-navegador: os casos passam no Chromium.");
  process.exit(0);
})();
