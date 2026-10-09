// A bancada do executor no navegador, sozinha numa página, para o Safari.
(async () => {
  const caso = location.pathname.includes("/fugido/") ? "fugido" : "normal";
  const r = { caso, ua: navigator.userAgent };
  const esperar = (teste, ms = 8000) => new Promise((ok) => {
    const t0 = performance.now();
    const ver = () => { const v = teste(); if (v) ok(v); else if (performance.now() - t0 > ms) ok(null); else setTimeout(ver, 10); };
    ver();
  });
  const subir = async (fonte, { mudo = false } = {}) => {
    const falas = [], falhas = [];
    const executor = executorNoNavegador("bancada", 1, "h");
    const t0 = performance.now();
    await executor.iniciar({ fonte, capacidades: ["regiao", "tema", "cartoes", "arquivo", "superficies", "contribuicoes", "estilos", "classes", "volume"] }, (m) => {
      falas.push(m);
      if (mudo || typeof m.n !== "number") return;
      const resp = m.tipo === "snapshot" ? { ok: true, valor: { servidor: "bancada" } }
        : m.tipo === "pedido" && m.id === "eco" ? { ok: true, valor: m.valor }
        : m.tipo === "regiao" ? { ok: true, valor: null } : { ok: false, erro: "desconhecido" };
      executor.entregar({ tipo: "resposta", n: m.n, ...resp }).catch((e) => falhas.push("entregar: " + e.message));
    }, (e) => falhas.push(String(e)));
    return { executor, falas, falhas, subida_ms: Math.round(performance.now() - t0) };
  };
  try {
    if (caso === "normal") {
      const fonte = await (await fetch("mod-comum.js")).text();
      const b = await subir(fonte);
      const pronto = await esperar(() => b.falas.find((m) => m.canal === "pronto"));
      r.comum = pronto ? pronto.valor : { falhas: b.falhas };
      r.subida_ms = b.subida_ms;
      const h = await subir(`
        const portas = ["window", "document", "parent", "self", "postMessage", "fetch", "XMLHttpRequest", "WebSocket", "indexedDB", "localStorage", "caches", "importScripts", "navigator", "WebAssembly", "Worker", "BroadcastChannel"];
        SeeleMods.request("eco", "hostil", { achadas: portas.filter((p) => typeof globalThis[p] !== "undefined") });
      `);
      r.hostil = (await esperar(() => h.falas.find((m) => m.canal === "hostil")))?.valor ?? { falhas: h.falhas };
      const l = await subir("SeeleUI.aoEvento(() => { while (true) {} });");
      let ultimo = performance.now(), maior = 0;
      const id = setInterval(() => { const a = performance.now(); maior = Math.max(maior, a - ultimo); ultimo = a; }, 10);
      await l.executor.entregar({ tipo: "evento", nome: "x" });
      r.laco = await esperar(() => l.falhas.find((f) => f.includes("interrompido")));
      clearInterval(id);
      r.travou_ms = Math.round(maior);
    } else {
      const b = await subir(location.origin + "/canario", { mudo: true });
      r.fugido = (await esperar(() => b.falas.find((m) => m.tipo === "fuga")))?.valor ?? { falhas: b.falhas };
    }
  } catch (e) {
    r.lancou = String(e && e.stack || e);
  }
  await fetch("/resultado", { method: "POST", body: JSON.stringify(r) });
  document.body.textContent = "feito: " + JSON.stringify(r);
})();
