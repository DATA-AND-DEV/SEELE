
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
    const u = URL.createObjectURL(new Blob([`(async () => { const r = []; try { await caches.open("aninhado"); r.push("caches abriu"); } catch (e) { r.push("caches: " + e.name); } try { indexedDB.open("x"); r.push("idb abriu"); } catch (e) { r.push("idb: " + e.name); } postMessage(r.join("; ")); })();`], { type: "text/javascript" }));
    const w = new Worker(u); w.onmessage = (e) => ok(e.data); w.onerror = (e) => falha(new Error("erro: " + e.message));
  }));
  await tentar("broadcast", () => { new BroadcastChannel("seele").postMessage("fugi"); return "postou"; });
  postMessage({ tipo: "mensagem", corpo: JSON.stringify({ n: 1, tipo: "fuga", valor: r }) });
  postMessage({ tipo: "subiu" });
};
