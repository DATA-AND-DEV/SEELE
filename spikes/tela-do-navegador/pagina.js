// A medida da Tarefa 3.10 (D-web-3): o que o VideoEncoder deste navegador
// codifica a 1080p, e o fluxo que ele produz, para o OpenH264 do desktop ler.
"use strict";
// Um marco por etapa, para saber onde a página parou num navegador que não diz.
const marco = (texto) => fetch("/marco", { method: "POST", body: String(texto) }).catch(() => {});
marco("o script rodou");
(async () => {
  try {
  const r = { ua: navigator.userAgent, temVideoEncoder: typeof VideoEncoder !== "undefined",
    temGetDisplayMedia: !!(navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia), suporte: [], fluxos: [] };
  const PERFIS = [
    ["avc1.42E01F", "Constrained Baseline 3.1"], ["avc1.42E028", "Constrained Baseline 4.0"],
    ["avc1.42E02A", "Constrained Baseline 4.2"], ["avc1.42001F", "Baseline 3.1"],
    ["avc1.4D0028", "Main 4.0"], ["avc1.640028", "High 4.0"],
  ];
  const base = { width: 1920, height: 1080, bitrate: 6_000_000, framerate: 30 };
  if (r.temVideoEncoder) {
    for (const [codec, nome] of PERFIS) {
      for (const format of ["annexb", "avc"]) {
        try {
          const s = await VideoEncoder.isConfigSupported({ ...base, codec, avc: { format } });
          r.suporte.push({ codec, nome, format, suportado: s.supported });
        } catch (e) { r.suporte.push({ codec, nome, format, erro: String(e) }); }
      }
    }
    await marco("isConfigSupported: " + JSON.stringify(r.suporte.map((x) => [x.codec, x.format, x.suportado ?? x.erro])));
    // Codifica com o primeiro de cada perfil que o navegador diz aceitar em annexb.
    const tela = new OffscreenCanvas(1920, 1080);
    const g = tela.getContext("2d");
    const desenhar = (n) => {
      g.fillStyle = "#fff"; g.fillRect(0, 0, 1920, 1080);
      g.fillStyle = "#111"; g.font = "13px monospace";
      for (let y = 0; y < 1080; y += 18) g.fillText(`fn quadro_${(y + n * 3) % 997}() -> Result<(), ErroDeQuadro> { let x = ${y * n}; }`.repeat(2), 8, y + 14);
      g.fillStyle = `hsl(${n * 6}, 70%, 50%)`; g.fillRect(200 + n * 10, 300, 400, 300);
    };
    for (const [codec, nome] of PERFIS) {
      if (!r.suporte.some((s) => s.codec === codec && s.format === "annexb" && s.suportado)) continue;
      const pedacos = [];
      let erro = null;
      const enc = new VideoEncoder({
        output: (chunk) => { const b = new Uint8Array(chunk.byteLength); chunk.copyTo(b); pedacos.push({ tipo: chunk.type, b }); },
        error: (e) => { erro = String(e); },
      });
      try {
        enc.configure({ ...base, codec, avc: { format: "annexb" } });
        for (let n = 0; n < 60; n++) {
          desenhar(n);
          const quadro = new VideoFrame(tela, { timestamp: n * 33333 });
          enc.encode(quadro, { keyFrame: n === 0 || n === 30 });
          quadro.close();
        }
        await marco(codec + ": 60 quadros entregues, esperando o flush");
        // Com prazo: no Safari 27 o flush de um perfil que ele disse aceitar
        // pode não voltar nunca, nem com erro.
        const voltou = await Promise.race([enc.flush().then(() => true), new Promise((ok) => setTimeout(() => ok(false), 10000))]);
        if (!voltou) erro = (erro ? erro + "; " : "") + `o flush não voltou em 10 s, com ${pedacos.length} pedaços saídos`;
        try { enc.close(); } catch (_) {}
      } catch (e) { erro = String(e); }
      const total = pedacos.reduce((t, p) => t + p.b.length, 0);
      const junto = new Uint8Array(total + pedacos.length * 4);
      // Cada unidade de acesso com o tamanho na frente, para o decodificador as separar.
      let o = 0;
      for (const p of pedacos) { new DataView(junto.buffer).setUint32(o, p.b.length); junto.set(p.b, o + 4); o += 4 + p.b.length; }
      await fetch(`/fluxo?codec=${codec}`, { method: "POST", body: junto });
      r.fluxos.push({ codec, nome, unidades: pedacos.length, chaves: pedacos.filter((p) => p.tipo === "key").length,
        bytes: total, maior: Math.max(0, ...pedacos.map((p) => p.b.length)), erro,
        // O perfil que o SPS do primeiro quadro-chave diz (profile_idc, constraint, level_idc).
        sps: (() => { const k = pedacos.find((p) => p.tipo === "key"); if (!k) return null;
          const b = k.b; for (let i = 0; i + 7 < b.length; i++) if (b[i] === 0 && b[i + 1] === 0 && b[i + 2] === 1 && (b[i + 3] & 0x1f) === 7) return [b[i + 4], b[i + 5], b[i + 6]];
          return null; })() });
    }
  }
  await fetch("/resultado", { method: "POST", body: JSON.stringify(r) });
  document.body.textContent = "feito";
  } catch (e) {
    // Uma medida que morre calada não mede nada: o erro vai ao servidor.
    await fetch("/resultado", { method: "POST", body: JSON.stringify({ ua: navigator.userAgent, lancou: String(e && e.stack || e) }) });
  }
})();
