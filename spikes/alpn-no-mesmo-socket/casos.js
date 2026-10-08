// Os casos da medição 3, escritos uma vez e rodados em três lugares: a bancada
// do Chromium (prova.py), o Safari do Simulador (prova.py safari-sim) e o
// iPhone (iphone.py). Descartável.
//
// `cfg` é o que o binário imprime ao subir, mais `host` (onde discar).
"use strict";

const Casos = (() => {
  const paraBytes = (h) => new Uint8Array(h.match(/../g).map((x) => parseInt(x, 16)));
  const paraHex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
  const adulterar = (h) => { const b = paraBytes(h); b[0] ^= 1; return paraHex(b); };
  const aleatorio = () => paraHex(crypto.getRandomValues(new Uint8Array(32)));
  const espera = (ms, valor) => new Promise((ok) => setTimeout(() => ok(valor), ms));
  const erroEmTexto = (e) => {
    const partes = [`${e && e.name}: ${e && e.message}`];
    if (e && e.source) partes.push(`source=${e.source}`);
    if (e && e.streamErrorCode != null) partes.push(`streamErrorCode=${e.streamErrorCode}`);
    return partes.join(" ");
  };

  // Abre e espera o `ready`, ou o prazo. Devolve o registro e, se abriu, a sessão.
  async function abrir(host, porta, hashes, { prazoMs = 60000, naoConfiavel = true } = {}) {
    const r = { alvo: `${host}:${porta}`, quantos_hashes: hashes.length, requireUnreliable: naoConfiavel };
    const t0 = performance.now();
    let wt;
    try {
      wt = new WebTransport(`https://${host}:${porta}/seele/1`, {
        serverCertificateHashes: hashes.map((h) => ({ algorithm: "sha-256", value: paraBytes(h) })),
        requireUnreliable: naoConfiavel,
      });
      wt.closed.catch(() => {});
      const v = await Promise.race([wt.ready.then(() => "abriu"), espera(prazoMs, "prazo")]);
      r.ms = Math.round(performance.now() - t0);
      if (v === "prazo") {
        r.erro = `nada em ${prazoMs} ms`;
        try { wt.close(); } catch (_) {}
        return { r };
      }
      r.abriu = true;
      return { r, wt };
    } catch (e) {
      r.ms = Math.round(performance.now() - t0);
      r.erro = erroEmTexto(e);
      return { r };
    }
  }

  // O datagrama de eco e um quadro de 4 bytes BE + corpo num fluxo bidirecional.
  async function eco(wt, r) {
    const w = wt.datagrams.createWritable ? wt.datagrams.createWritable().getWriter() : wt.datagrams.writable.getWriter();
    const rd = wt.datagrams.readable.getReader();
    await w.write(new Uint8Array([8, 1, 2, 3, 4]));
    const d = await Promise.race([rd.read(), espera(3000, null)]);
    r.datagrama_eco = Boolean(d && d.value && d.value.length === 5);
    r.maxDatagramSize = wt.datagrams.maxDatagramSize;
    w.releaseLock();
    rd.releaseLock();
    const s = await wt.createBidirectionalStream();
    const sw = s.writable.getWriter();
    const corpo = new TextEncoder().encode("\x08ola");
    const q = new Uint8Array(4 + corpo.length);
    new DataView(q.buffer).setUint32(0, corpo.length);
    q.set(corpo, 4);
    await sw.write(q);
    await sw.close();
    const sr = s.readable.getReader();
    const tudo = [];
    while (true) { const { value, done } = await sr.read(); if (done) break; tudo.push(...value); }
    r.quadro_eco = JSON.stringify(tudo) === JSON.stringify(Array.from(q));
  }

  async function caso(rotulo, host, porta, hashes, opcoes) {
    const { r, wt } = await abrir(host, porta, hashes, opcoes);
    r.rotulo = rotulo;
    if (wt) {
      try { await eco(wt, r); } catch (e) { r.erro_eco = erroEmTexto(e); }
      wt.close();
    }
    return r;
  }

  // 1 · o certificado curto com a mesma chave, e os controles.
  async function basicos(cfg, opcoes) {
    const h = cfg.host;
    return [
      await caso("A · curto da MESMA chave do longo", h, cfg.porta_a, [cfg.curto], opcoes),
      await caso("A · curto com um bit trocado", h, cfg.porta_a, [adulterar(cfg.curto)], opcoes),
      await caso("A · hash do longo (o servidor apresenta o curto)", h, cfg.porta_a, [cfg.longo], opcoes),
      await caso("A2 · curto de chave PRÓPRIA (controle)", h, cfg.porta_a2, [cfg.curto_outra_chave], opcoes),
      await caso("B · só o longo (1975–4096), hash certo", h, cfg.porta_b, [cfg.longo], opcoes),
    ];
  }

  // 2 · a lista longa, com o certo por último.
  async function lista(cfg, opcoes, tamanhos = [1, 15, 30, 60, 100, 200, 500]) {
    const res = [];
    for (const n of tamanhos) {
      const hashes = Array.from({ length: n - 1 }, aleatorio).concat([cfg.curto]);
      res.push(await caso(`A · ${n} hashes, o certo por último`, cfg.host, cfg.porta_a, hashes, opcoes));
    }
    return res;
  }

  // 3 · fora do ar contra a recusa. Só o tempo até o erro, e o texto dele.
  async function foraDoAr(cfg, opcoes, ninguem) {
    const res = [];
    const um = async (rotulo, host, porta, hashes) => {
      const { r, wt } = await abrir(host, porta, hashes, opcoes);
      if (wt) wt.close();
      r.rotulo = rotulo;
      res.push(r);
    };
    for (let i = 1; i <= 3; i++) await um(`recusa (hash errado) #${i}`, cfg.host, cfg.porta_a, [adulterar(cfg.curto)]);
    for (let i = 1; i <= 3; i++) await um(`porta fechada #${i}`, cfg.host, cfg.porta_fechada, [cfg.curto]);
    await um("mudo (lê e descarta)", cfg.host, cfg.porta_mudo, [cfg.curto]);
    if (ninguem) await um(`ninguém em ${ninguem}`, ninguem, cfg.porta_a, [cfg.curto]);
    return res;
  }

  // 4 · ficar aberto: um ping por datagrama a cada segundo, até a sessão cair.
  // Nada aqui depende do timer para medir: o servidor registra quando fechou e
  // por quê, e a página registra o `closed` e o `visibilitychange`.
  function ficarAberto(cfg, opcoes, aoEvento) {
    const t0 = performance.now();
    const ev = (o) => aoEvento({ t_s: Math.round((performance.now() - t0) / 100) / 10, oculta: document.hidden, ...o });
    let parar = false;
    (async () => {
      const { r, wt } = await abrir(cfg.host, cfg.porta_a, [cfg.curto], opcoes);
      ev({ ev: "abrir", ...r });
      if (!wt) return;
      wt.closed.then((info) => ev({ ev: "closed", info: JSON.stringify(info) }), (e) => ev({ ev: "closed com erro", erro: erroEmTexto(e) }));
      const w = wt.datagrams.createWritable ? wt.datagrams.createWritable().getWriter() : wt.datagrams.writable.getWriter();
      const rd = wt.datagrams.readable.getReader();
      let enviados = 0;
      let voltaram = 0;
      (async () => { while (true) { const { done } = await rd.read(); if (done) break; voltaram++; } })().catch(() => {});
      while (!parar) {
        try { await w.write(new Uint8Array([2, enviados & 255])); enviados++; } catch (e) { ev({ ev: "escrita falhou", erro: erroEmTexto(e) }); break; }
        if (enviados % 10 === 0) ev({ ev: "pings", enviados, voltaram });
        await espera(1000);
      }
      try { wt.close(); } catch (_) {}
    })();
    document.addEventListener("visibilitychange", () => ev({ ev: document.hidden ? "ficou oculta" : "voltou à frente" }));
    return () => { parar = true; };
  }

  // A configuração de áudio da rodada 5 do voz-no-navegador
  // (pagina.js:319-362): sessão `play-and-record`, AudioContext a 48 kHz
  // interativo, saída pela tag <audio>, e o microfone com eco, supressão e
  // ganho. No lugar da voz do outro, um tom baixo de 220 Hz.
  async function ligarAudioDoProduto(aoEvento) {
    if (navigator.audioSession) navigator.audioSession.type = "play-and-record";
    const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    const tom = ctx.createOscillator();
    tom.frequency.value = 220;
    const ganho = ctx.createGain();
    ganho.gain.value = 0.02;
    const destino = ctx.createMediaStreamDestination();
    tom.connect(ganho).connect(destino);
    tom.start();
    const elemento = document.createElement("audio");
    elemento.playsInline = true;
    elemento.srcObject = destino.stream;
    elemento.onpause = () => aoEvento({ ev: "tag <audio> pausada" });
    document.body.append(elemento);
    await elemento.play();
    if (navigator.mediaSession && window.MediaMetadata) navigator.mediaSession.metadata = new MediaMetadata({ title: "SEELE · medição 3" });
    ctx.onstatechange = () => aoEvento({ ev: `AudioContext ${ctx.state}` });
    await ctx.resume();
    const fonte = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 } });
    const faixa = fonte.getAudioTracks()[0];
    faixa.onmute = () => aoEvento({ ev: "microfone mudo pelo sistema" });
    faixa.onunmute = () => aoEvento({ ev: "microfone voltou" });
    const mic = ctx.createMediaStreamSource(fonte);
    const zero = ctx.createGain();
    zero.gain.value = 0;
    mic.connect(zero).connect(ctx.destination);
    aoEvento({ ev: "áudio do produto ligado", audioSession: navigator.audioSession ? navigator.audioSession.type : "(não existe)" });
  }

  return { basicos, lista, foraDoAr, ficarAberto, ligarAudioDoProduto };
})();
