// A página do app no spike: guarda uma identidade como o celular guardaria,
// sobe os executores e mede. Os três executores têm a forma de
// `ui/mods-runtime.js`: iniciar, entregar e encerrar. Descartável.
//
// ?nav=<nome>&fase=1  custo, MOD comum, MOD hostil (grava marcas), recursos
// ?nav=<nome>&fase=2  MOD hostil de novo (procura as marcas), depois limpa
"use strict";

const q = new URLSearchParams(location.search);
const NAV = q.get("nav") || "desconhecido";
const FASE = Number(q.get("fase") || "1");
// As capacidades da API 5, a mais nova publicada (seele-proto/src/mods.rs:91-101).
const CAPS = ["regiao", "tema", "cartoes", "arquivo", "superficies", "contribuicoes", "estilos", "classes", "volume"];
const CANARIO = "http://127.0.0.1:8096/canario";
const UDP = { A: 8097, B: 8098, AB: 8099 };
const $o = document.getElementById("o");
const log = (t) => { $o.textContent += "\n" + t; };
const espera = (ms) => new Promise((ok) => setTimeout(ok, ms));
const mandar = (caminho, corpo) => fetch(caminho, { method: "POST", body: JSON.stringify(corpo) });
const ordena = (xs) => xs.slice().sort((a, b) => a - b);
const pct = (xs, p) => { const s = ordena(xs); return s.length ? Math.round(s[Math.min(s.length - 1, Math.floor(s.length * p))] * 100) / 100 : null; };

let PRELUDIO = null;
let BYTES = null;
let MODULO = null;
let CODIGO_DO_WORKER = null;

// ---------------------------------------------------------------- executores

function executorQuickJS() {
  let w = null;
  return {
    nome: "A",
    iniciar(fonte, aoMensagem) {
      return new Promise((ok) => {
        const t0 = performance.now();
        w = new Worker("candidato-quickjs-worker.js");
        w.onmessage = (e) => {
          const m = e.data;
          if (m.tipo === "subiu" || m.tipo === "falhou") ok({ ...m, total_ms: performance.now() - t0 });
          aoMensagem(m);
        };
        w.onerror = (e) => ok({ tipo: "falhou", erro: "worker: " + e.message });
        w.postMessage({ tipo: "iniciar", modulo: MODULO, fonte, capacidades: CAPS });
      });
    },
    entregar(json) { w.postMessage({ tipo: "entregar", json }); },
    pedirMemoria() { w.postMessage({ tipo: "memoria" }); },
    encerrar() { w.terminate(); },
  };
}

function executorEmIframe(nome, url, mensagemDeInicio) {
  let f = null;
  let ouvir = null;
  return {
    nome,
    iniciar(fonte, aoMensagem) {
      return new Promise((ok) => {
        const t0 = performance.now();
        f = document.createElement("iframe");
        f.sandbox = "allow-scripts";
        f.src = url;
        ouvir = (e) => {
          if (!f || e.source !== f.contentWindow) return;
          const m = e.data;
          if (m.tipo === "carregou") { f.contentWindow.postMessage(mensagemDeInicio(fonte), "*"); return; }
          if (m.tipo === "subiu" || m.tipo === "falhou") ok({ ...m, total_ms: performance.now() - t0 });
          aoMensagem(m);
        };
        addEventListener("message", ouvir);
        setTimeout(() => ok({ tipo: "falhou", erro: "o iframe não respondeu em 15 s" }), 15000);
        document.body.append(f);
      });
    },
    entregar(json) { f.contentWindow.postMessage({ tipo: "entregar", json }, "*"); },
    pedirMemoria() { if (nome === "AB") f.contentWindow.postMessage({ tipo: "memoria" }, "*"); },
    encerrar() { removeEventListener("message", ouvir); if (f) f.remove(); f = null; },
  };
}

const FABRICAS = {
  A: executorQuickJS,
  B: () => executorEmIframe("B", "candidato-iframe.html", (fonte) => ({ tipo: "iniciar", preludio: PRELUDIO, fonte, capacidades: CAPS })),
  AB: () => executorEmIframe("AB", "candidato-ab.html", (fonte) => ({ tipo: "iniciar", codigoDoWorker: CODIGO_DO_WORKER, bytes: BYTES, fonte, capacidades: CAPS })),
};

// ---------------------------------------------------------------- a ponte

async function subir(candidato, fonte) {
  const ex = FABRICAS[candidato]();
  const inst = { ex, falas: [], consoles: [], erros: [], esperas: [], memoria: null };
  const resolver = (msg, eErro) => {
    for (const w of inst.esperas.slice()) {
      if (eErro ? w.aceitaErro : w.filtro(msg)) {
        inst.esperas.splice(inst.esperas.indexOf(w), 1);
        w.ok(eErro ? { erro: msg } : msg);
      }
    }
  };
  const aoMensagem = (m) => {
    if (m.tipo === "fala") {
      let msg;
      try { msg = JSON.parse(m.json); } catch (_) { return; }
      inst.falas.push(msg);
      if (msg.n != null) {
        let resp;
        if (msg.tipo === "snapshot") resp = { n: msg.n, ok: true, valor: { servidor: "bancada" } };
        else if (msg.tipo === "pedido" && (msg.id === "eco" || msg.id === "sonda")) resp = { n: msg.n, ok: true, valor: msg.valor };
        else if (msg.tipo === "regiao") resp = { n: msg.n, ok: true, valor: null };
        else resp = { n: msg.n, ok: false, erro: "desconhecido" };
        ex.entregar(JSON.stringify(resp));
      }
      resolver(msg, false);
    } else if (m.tipo === "console") {
      inst.consoles.push(`${m.metodo}: ${m.texto}`.slice(0, 200));
    } else if (m.tipo === "erro-na-volta") {
      inst.erros.push(String(m.erro).slice(0, 200));
      resolver(m.erro, true);
    } else if (m.tipo === "memoria") {
      inst.memoria = m;
      resolver({ memoria: m }, false);
    }
  };
  inst.subida = await ex.iniciar(fonte, aoMensagem);
  // O que já chegou conta: uma fala pode chegar antes de alguém esperá-la
  // (o relatório do MOD hostil chega antes do «subiu»).
  inst.consumidas = new Set();
  inst.esperar = (filtro, prazo = 15000, aceitaErro = false) => new Promise((ok) => {
    const ja = inst.falas.find((m) => !inst.consumidas.has(m) && filtro(m));
    if (ja) { inst.consumidas.add(ja); ok(ja); return; }
    const w = { filtro, ok: (m) => { inst.consumidas.add(m); ok(m); }, aceitaErro };
    inst.esperas.push(w);
    setTimeout(() => { const i = inst.esperas.indexOf(w); if (i >= 0) { inst.esperas.splice(i, 1); ok({ prazo: true }); } }, prazo);
  });
  inst.canal = (canal) => (m) => m.tipo === "pedido" && m.canal === canal;
  inst.evento = (nome, valor) => ex.entregar(JSON.stringify({ tipo: "evento", nome, valor }));
  return inst;
}

// O maior intervalo entre duas batidas de 10 ms na página: quanto ela travou.
function monitorar() {
  let ultimo = performance.now();
  let maior = 0;
  const id = setInterval(() => { const a = performance.now(); maior = Math.max(maior, a - ultimo); ultimo = a; }, 10);
  return () => { clearInterval(id); return Math.round(Math.max(maior, performance.now() - ultimo)); };
}

const rss = async (marco) => (await fetch(`/rss?nav=${encodeURIComponent(NAV)}&marco=${encodeURIComponent(marco)}`)).json();

// ---------------------------------------------------------------- a identidade da página

function noBanco(nome, modo, fazer) {
  return new Promise((ok, falha) => {
    const r = indexedDB.open(nome, 1);
    r.onupgradeneeded = () => r.result.createObjectStore("chaves");
    r.onsuccess = () => {
      const db = r.result;
      const t = db.transaction("chaves", modo);
      const pedido = fazer(t.objectStore("chaves"));
      t.oncomplete = () => { db.close(); ok(pedido && pedido.result); };
      t.onerror = () => { db.close(); falha(t.error); };
    };
    r.onerror = () => falha(r.error);
  });
}

const broadcasts = [];
new BroadcastChannel("seele").onmessage = (e) => broadcasts.push(String(e.data));

async function prepararAPagina() {
  let chave = await noBanco("seele-identidade", "readonly", (s) => s.get("identidade")).catch(() => null);
  if (!chave) {
    chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    await noBanco("seele-identidade", "readwrite", (s) => s.put(chave, "identidade"));
  }
  window.SeeleChave = chave;
  localStorage.setItem("seele-marca-da-pagina", "1");
  document.cookie = "seele_pagina=1; max-age=3600";
}

async function carregar() {
  PRELUDIO = await (await fetch("preludio.js")).text();
  const t0 = performance.now();
  BYTES = await (await fetch("motor.wasm")).arrayBuffer();
  const t1 = performance.now();
  MODULO = await WebAssembly.compile(BYTES);
  const t2 = performance.now();
  const anfitriao = await (await fetch("motor-anfitriao.js")).text();
  const worker = await (await fetch("candidato-quickjs-worker.js")).text();
  CODIGO_DO_WORKER = anfitriao + "\n" + worker.replace('importScripts("motor-anfitriao.js");', "");
  return { motor_bytes: BYTES.byteLength, baixar_ms: Math.round(t1 - t0), compilar_ms: Math.round(t2 - t1), preludio_bytes: PRELUDIO.length };
}

// ---------------------------------------------------------------- as medidas

const FONTE_COMUM = () => fetch("mod-comum.js").then((r) => r.text());
const FONTE_HOSTIL = () => fetch("mod-hostil.js").then((r) => r.text());
const sonda = (candidato) => `const SONDA = ${JSON.stringify({ candidato, fase: FASE, canario: CANARIO, udp: UDP[candidato] })};\n`;
const NAVEGACOES = {
  location: "location.href = C('navegou-location');",
  formulario: "const f = document.createElement('form'); f.action = C('navegou-form'); f.method = 'POST'; document.body.append(f); f.submit();",
  refresh: "const m = document.createElement('meta'); m.httpEquiv = 'refresh'; m.content = '0;url=' + C('navegou-refresh'); document.head.append(m);",
};

async function custo(candidato, fonteComum) {
  const r = { candidato };
  const inst = await subir(candidato, fonteComum);
  r.subida = { ms: Math.round(inst.subida.total_ms), erro: inst.subida.erro };
  const pronto = await inst.esperar(inst.canal("pronto"), 15000, true);
  r.pronto = pronto.valor || pronto;
  // A API: o que o prelúdio do desktop prometeu, conferido daqui.
  const v = pronto.valor || {};
  r.api_igual_ao_desktop = Boolean(
    v.snap && v.snap.servidor === "bancada" &&
    JSON.stringify(v.capacidades) === JSON.stringify(CAPS) &&
    v.intervalos === 3 && v.erro === "rejeitou: desconhecido" &&
    String(v.grande).startsWith("mensagem-grande: pedido tem 14062 bytes") &&
    v.superficies === "object" && v.console === "function" &&
    inst.consoles.includes("log: mod comum de pé"));
  r.console = inst.consoles.slice(0, 3);
  // A volta página → MOD → página.
  const voltas = [];
  for (let i = 0; i < 200; i++) {
    const t0 = performance.now();
    const p = inst.esperar((m) => m.tipo === "pedido" && m.canal === "pong" && m.valor === i, 5000);
    inst.evento("ping", i);
    const x = await p;
    if (!x.prazo) voltas.push(performance.now() - t0);
  }
  r.volta_ms = { n: voltas.length, p50: pct(voltas, 0.5), p95: pct(voltas, 0.95) };
  // Trabalho de CPU, medido dentro do MOD.
  const cpu = [];
  const json = [];
  for (let i = 0; i < 3; i++) {
    const p = inst.esperar(inst.canal("cpu"), 10000, true);
    inst.evento("cpu");
    const x = await p;
    cpu.push(x.valor ? x.valor.ms : x);
    const pj = inst.esperar(inst.canal("json"), 10000, true);
    inst.evento("json");
    const y = await pj;
    json.push(y.valor ? y.valor.ms : y);
  }
  r.cpu_1e6_iteracoes_ms = cpu;
  r.json_200_arvores_ms = json;
  if (candidato !== "B") {
    const pm = inst.esperar((m) => m.memoria, 3000);
    inst.ex.pedirMemoria();
    const m = await pm;
    r.memoria = m.memoria || m;
  }
  r.erros = inst.erros;
  inst.ex.encerrar();

  // Dez instâncias, uma depois da outra, cada uma até o «pronto».
  await espera(500);
  const antes = await rss(`${candidato}-antes`);
  const t0 = performance.now();
  const dez = [];
  for (let i = 0; i < 10; i++) {
    const x = await subir(candidato, fonteComum);
    await x.esperar(x.canal("pronto"), 15000, true);
    dez.push(x);
  }
  r.dez_ate_pronto_ms = Math.round(performance.now() - t0);
  await espera(1000);
  const com = await rss(`${candidato}-com-10`);
  r.rss_10_instancias_kb = com.kb != null && antes.kb != null ? com.kb - antes.kb : null;
  r.rss_processos = com.processos;
  for (const x of dez) x.ex.encerrar();
  await espera(1000);
  const depois = await rss(`${candidato}-depois`);
  r.rss_depois_de_encerrar_kb = depois.kb != null && antes.kb != null ? depois.kb - antes.kb : null;
  return r;
}

async function hostil(candidato, fonteHostil) {
  const r = { candidato };
  const inst = await subir(candidato, sonda(candidato) + fonteHostil);
  r.subida = inst.subida.erro || "subiu";
  const res = await inst.esperar(inst.canal("resultado"), 30000, true);
  r.sondas = res.valor || res;
  if (FASE === 1) {
    // CPU sem fim: a página trava?
    const parar = monitorar();
    const pc = inst.esperar(inst.canal("cpu"), 8000, true);
    inst.evento("cpu-sem-fim");
    const c = await pc;
    r.cpu_sem_fim = c.valor || c;
    r.pagina_travou_ms = parar();
    // Memória sem teto?
    const pm = inst.esperar(inst.canal("memoria"), 15000, true);
    inst.evento("memoria");
    const m = await pm;
    r.memoria = m.valor || m;
  }
  r.erros = inst.erros;
  r.console = inst.consoles.slice(0, 5);
  inst.ex.encerrar();
  // As navegações, cada uma num executor novo: elas tiram o MOD da página.
  for (const [nome, codigo] of Object.entries(NAVEGACOES)) {
    const n = await subir(candidato, sonda(candidato) + `const C = (v) => SONDA.canario + '/' + SONDA.candidato + '/' + SONDA.fase + '/' + v;\ntry { ${codigo} } catch (e) { SeeleMods.request('sonda', 'navegacao', String(e)); }`);
    const x = await n.esperar(n.canal("navegacao"), 1500);
    r["navegacao_" + nome] = x.valor || (n.subida.erro ? "não subiu: " + n.subida.erro : "nenhum erro (pode ter navegado)");
    n.ex.encerrar();
  }
  return r;
}

async function conferirAPagina() {
  const chave = await noBanco("seele-identidade", "readonly", (s) => s.get("identidade")).catch(() => null);
  return {
    identidade_intacta: Boolean(chave && chave.privateKey && chave.privateKey.extractable === false),
    broadcasts_recebidos: broadcasts.slice(),
  };
}

async function limpar() {
  await new Promise((ok) => { const d = indexedDB.deleteDatabase("seele-identidade"); d.onsuccess = d.onerror = d.onblocked = ok; });
  localStorage.removeItem("seele-marca-da-pagina");
  document.cookie = "seele_pagina=; max-age=0";
}

async function depurar(candidato) {
  await carregar();
  const fonte = q.get("fonte") === "hostil" ? sonda(candidato) + (await FONTE_HOSTIL()) : await FONTE_COMUM();
  const inst = await subir(candidato, fonte);
  console.log("subida", JSON.stringify(inst.subida));
  await espera(Number(q.get("ms") || 3000));
  console.log("falas", JSON.stringify(inst.falas).slice(0, 2000));
  console.log("consoles", JSON.stringify(inst.consoles));
  console.log("erros", JSON.stringify(inst.erros));
}

(async () => {
  if (q.get("depurar")) { await depurar(q.get("depurar")); return; }
  const r = { nav: NAV, fase: FASE, ua: navigator.userAgent };
  await prepararAPagina();
  r.carga = await carregar();
  log(`motor ${r.carga.motor_bytes} B, compilado em ${r.carga.compilar_ms} ms`);
  const hostilFonte = await FONTE_HOSTIL();
  if (FASE === 1) {
    const comum = await FONTE_COMUM();
    r.custo = {};
    for (const c of ["A", "B", "AB"]) { log(`custo ${c}…`); r.custo[c] = await custo(c, comum); }
  }
  r.hostil = {};
  if (q.get("so") !== "custo") {
    for (const c of ["A", "B", "AB"]) { log(`hostil ${c}…`); r.hostil[c] = await hostil(c, hostilFonte); }
  }
  r.pagina = await conferirAPagina();
  if (FASE === 2) await limpar();
  await mandar("/r", r);
  log("pronto");
})().catch((e) => { log("ERRO " + e); mandar("/r", { nav: NAV, fase: FASE, erro: `${e.name}: ${e.message}\n${e.stack}` }); });
