// R4 na SUBIDA (seele-enlace): a página manda voz a 50/s (160 B, número de sequência) enquanto
// 3 anexos e 1 tela sobem por 4 canais "u" concorrentes; o `examples/eco.rs` do enlace devolve a
// voz e conta a que recebeu. Mede a voz enviada, a recebida pelo anfitrião, a ecoada, o atraso, a
// vazão e os descartes do sistema por buffer cheio (`netstat -s -p udp`, macOS).
//
//   cargo build --release -p seele-enlace --example eco
//   node crates/seele-enlace/bancada/subida.cjs target/release/examples/eco <chromium|webkit> \
//        <canal|global> [MiB por canal=32] [vezes=1] [--playwright=<node_modules/playwright>]
//
// Política "canal": cada canal escreve enquanto o SEU bufferedAmount < 1 MiB.
// Política "global": os quatro juntos < 128 KiB, revezando (o R4 do lado da página).
// O Playwright vem de --playwright=, de PLAYWRIGHT, ou da resolução normal do Node; nunca de um
// caminho de máquina (a regra de apps/seele-app/bancada/playwright.cjs).
const { spawn, execFileSync } = require("child_process");
const posicionais = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const [binario, navegador = "chromium", politica = "canal", mibArg = "32", vezesArg = "1"] = posicionais;
const escuta = "127.0.0.1:0";
const daLinha = process.argv.find((a) => a.startsWith("--playwright="))?.slice("--playwright=".length);
const mod = require(daLinha || process.env.PLAYWRIGHT || "playwright");

function udpCheios() {
  try {
    const t = execFileSync("netstat", ["-s", "-p", "udp"]).toString();
    const m = t.match(/(\d+) dropped due to full socket buffers/);
    return m ? Number(m[1]) : null;
  } catch (e) { return null; }
}

function iniciarEco() {
  return new Promise((ok, falha) => {
    const p = spawn(binario, [escuta, "--sem-eco-de-volume"], { stdio: ["ignore", "pipe", "inherit"] });
    let buf = ""; const linhas = [];
    let pronto = false;
    p.stdout.on("data", (d) => {
      buf += d.toString();
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const l = buf.slice(0, i); buf = buf.slice(i + 1); linhas.push(l);
        const m = l.match(/^ESCUTA \[?([^\]\s]+?)\]?:(\d+) FP ([0-9a-f]{64})$/);
        if (m && !pronto) { pronto = true; ok({ p, ip: m[1] === "::" ? "127.0.0.1" : m[1], porta: Number(m[2]), fp: m[3], linhas }); }
      }
    });
    p.on("exit", (c) => { if (!pronto) falha(new Error("eco saiu " + c)); });
  });
}

async function naPagina({ ip, porta, fp, politica, porCanal }) {
  const t0 = performance.now();
  const A = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  const U = Array.from(crypto.getRandomValues(new Uint8Array(32)), (x) => A[x % A.length]).join("");
  const fpColon = fp.toUpperCase().match(/../g).join(":");
  const espera = (t) => new Promise((r) => setTimeout(r, t));
  const pc = new RTCPeerConnection({ iceServers: [] });
  const ctl = pc.createDataChannel("ctl", { negotiated: true, id: 0 });
  const voz = pc.createDataChannel("voz", { negotiated: true, id: 1, ordered: false, maxRetransmits: 0 });
  ctl.binaryType = voz.binaryType = "arraybuffer";
  const oferta = await pc.createOffer();
  const sdp = oferta.sdp.replace(/a=ice-ufrag:.*\r\n/, `a=ice-ufrag:${U}\r\n`).replace(/a=ice-pwd:.*\r\n/, `a=ice-pwd:${U}\r\n`);
  await pc.setLocalDescription({ type: "offer", sdp });
  const mid = (sdp.match(/a=mid:(\S+)/) || [])[1] || "0";
  const fam = ip.includes(":") ? "IP6" : "IP4";
  await pc.setRemoteDescription({ type: "answer", sdp: ["v=0", "o=- 1 1 IN IP4 0.0.0.0", "s=-", "t=0 0", `a=group:BUNDLE ${mid}`, "a=ice-lite",
    `m=application ${porta} UDP/DTLS/SCTP webrtc-datachannel`, `c=IN ${fam} ${ip}`, `a=mid:${mid}`,
    `a=ice-ufrag:${U}`, `a=ice-pwd:${U}`, `a=fingerprint:sha-256 ${fpColon}`, "a=setup:passive",
    "a=sctp-port:5000", "a=max-message-size:16385",
    `a=candidate:1 1 UDP 2130706431 ${ip} ${porta} typ host`, "a=end-of-candidates", ""].join("\r\n") });
  await Promise.race([new Promise((r) => (ctl.onopen = r)), espera(10000)]);
  if (ctl.readyState !== "open") return { erro: "ctl não abriu" };
  // A voz: seq em 4 bytes depois do byte de versão.
  const enviados = new Map(); const ecos = [];
  voz.onmessage = (e) => { const d = new DataView(e.data); const s = d.getUint32(1); const ida = enviados.get(s); if (ida !== undefined) { ecos.push({ s, ida, rtt: performance.now() - ida }); enviados.delete(s); } };
  let seq = 0;
  const pacote = new Uint8Array(160); pacote[0] = 1;
  const relogio = setInterval(() => { new DataView(pacote.buffer).setUint32(1, seq); try { voz.send(pacote); enviados.set(seq, performance.now()); } catch (e) {} seq++; }, 20);
  await espera(1500);                              // repouso
  // 4 canais de volume.
  const canais = [];
  for (let i = 0; i < 4; i++) {
    const u = pc.createDataChannel("u", { protocol: "seele/2" });
    u.binaryType = "arraybuffer";
    u.bufferedAmountLowThreshold = politica === "global" ? 32 * 1024 : 256 * 1024;
    canais.push(u);
  }
  await Promise.all(canais.map((u) => new Promise((r) => (u.onopen = r))));
  const TOTAL = porCanal * 1024 * 1024;
  const corpo = new Uint8Array(16385); corpo[0] = 0;
  const ALTO_CANAL = 1024 * 1024, ALTO_GLOBAL = 128 * 1024;
  const ini = performance.now();
  const restante = canais.map(() => TOTAL);
  const somaBuffer = () => canais.reduce((t, u) => t + u.bufferedAmount, 0);
  let vez = 0;
  while (restante.some((r) => r > 0)) {
    let mandou = false;
    for (let k = 0; k < canais.length; k++) {
      const i = (vez + k) % canais.length; const u = canais[i];
      if (restante[i] <= 0) continue;
      const cabe = politica === "global" ? somaBuffer() < ALTO_GLOBAL : u.bufferedAmount < ALTO_CANAL;
      if (!cabe) continue;
      const n = Math.min(16384, restante[i]);
      u.send(n === 16384 ? corpo : corpo.subarray(0, n + 1));
      restante[i] -= n; mandou = true;
      if (politica === "global") break;          // um por vez, revezando
    }
    vez++;
    if (!mandou) await new Promise((r) => { let f = false; const um = () => { if (!f) { f = true; r(); } }; canais.forEach((u) => (u.onbufferedamountlow = um)); setTimeout(um, 50); });
  }
  for (const u of canais) u.send(new Uint8Array([1]));
  while (somaBuffer() > 0) await espera(10);
  const fim = performance.now();
  await espera(1500);                              // depois
  clearInterval(relogio);
  await espera(800);
  // Por janela de 500 ms: enviadas, ecoadas, perdidas.
  const janelas = [];
  for (let t = 0; t < fim + 1500 - t0; t += 500) janelas.push({ t, enviadas: 0, ecoadas: 0, rtts: [] });
  const tudo = new Map(); ecos.forEach((e) => tudo.set(e.s, e));
  for (let s = 0; s < seq; s++) {
    // o instante do envio: os ecoados têm; os perdidos ficam em `enviados`
    const ida = tudo.has(s) ? tudo.get(s).ida : enviados.get(s);
    if (ida === undefined) continue;
    const j = janelas[Math.floor((ida - t0) / 500)]; if (!j) continue;
    j.enviadas++; if (tudo.has(s)) { j.ecoadas++; j.rtts.push(tudo.get(s).rtt); }
  }
  const fase = (t) => (t + t0 < ini ? "antes" : t + t0 <= fim ? "rajada" : "depois");
  const resumo = { antes: { e: 0, r: 0, rtts: [] }, rajada: { e: 0, r: 0, rtts: [] }, depois: { e: 0, r: 0, rtts: [] } };
  for (const j of janelas) { const f = resumo[fase(j.t)]; f.e += j.enviadas; f.r += j.ecoadas; f.rtts.push(...j.rtts); }
  const p = (v, k) => { v = v.slice().sort((a, b) => a - b); return v.length ? Math.round(v[Math.min(v.length - 1, Math.floor(k * v.length))] * 10) / 10 : null; };
  const saida = {};
  for (const [f, v] of Object.entries(resumo)) saida[f] = { enviadas: v.e, ecoadas: v.r, perda_pct: v.e ? Math.round((1 - v.r / v.e) * 1000) / 10 : null, p50: p(v.rtts, 0.5), p95: p(v.rtts, 0.95), p99: p(v.rtts, 0.99) };
  pc.close();
  return { politica, voz_enviada_total: seq, rajada_ms: Math.round(fim - ini), mbit_s: Math.round(4 * TOTAL * 8 / (fim - ini) / 1000), fases: saida,
    janelas_da_rajada: janelas.filter((j) => fase(j.t) === "rajada").map((j) => [j.t, j.enviadas, j.ecoadas]) };
}

async function umaVez() {
  const antes = udpCheios();
  const eco = await iniciarEco();
  const browser = await mod[navegador].launch({ headless: true });
  const page = await browser.newPage();
  await page.goto("about:blank");
  let r;
  try { r = await page.evaluate(naPagina, { ip: eco.ip, porta: eco.porta, fp: eco.fp, politica, porCanal: Number(mibArg) }); }
  catch (e) { r = { erro: String(e) }; }
  r.navegador = `${navegador} ${browser.version()}`;
  await browser.close();
  await new Promise((ok) => setTimeout(ok, 1500));
  r.eco = eco.linhas.filter((l) => l.startsWith("VOZ") || l.startsWith("FECHOU") || l.startsWith("BUFFER"));
  eco.p.kill();
  const depois = udpCheios();
  r.udp_descartados_por_buffer_cheio_no_sistema = antes !== null && depois !== null ? depois - antes : "não lido";
  const voz = r.eco.find((l) => l.startsWith("VOZ")) || "";
  r.voz_recebida_no_anfitriao = Number((voz.match(/recebida=(\d+)/) || [])[1]);
  r.perda_ate_o_anfitriao_pct = Math.round((1 - r.voz_recebida_no_anfitriao / r.voz_enviada_total) * 1000) / 10;
  return r;
}

(async () => {
  const vezes = [];
  for (let i = 0; i < Number(vezesArg); i++) vezes.push(await umaVez());
  console.log(JSON.stringify(vezes.length === 1 ? vezes[0] : vezes.map((r) => ({
    navegador: r.navegador, politica: r.politica, enviada: r.voz_enviada_total, recebida: r.voz_recebida_no_anfitriao,
    perda_ate_o_anfitriao_pct: r.perda_ate_o_anfitriao_pct, p95_rajada_ms: r.fases && r.fases.rajada.p95,
    p99_rajada_ms: r.fases && r.fases.rajada.p99, mbit_s: r.mbit_s, descartes_do_sistema: r.udp_descartados_por_buffer_cheio_no_sistema,
  })), null, 1));
})().catch((e) => { console.error(e); process.exit(1); });
