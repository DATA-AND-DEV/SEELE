// O wasm da voz dentro de um AudioWorklet, num OfflineAudioContext (que roda
// sem gesto, e por isso também no Simulador).
(async () => {
  const resultado = { agente: navigator.userAgent, tentativas: [] };
  try {
    const bytes = await (await fetch("voz_em_wasm.wasm")).arrayBuffer();
    const segundos = 10;
    const ctx = new OfflineAudioContext(1, 48000 * segundos, 48000);
    await ctx.audioWorklet.addModule("worklet.js");
    const no = new AudioWorkletNode(ctx, "voz-em-wasm");
    no.connect(ctx.destination);
    const ouvidas = [];
    let avisar = null;
    no.port.onmessage = (m) => {
      ouvidas.push(m.data);
      avisar?.(m.data);
    };
    const esperar = (ms) =>
      new Promise((pronto) => {
        avisar = (d) => (d.tipo === "pronto" || d.tipo === "erro") && pronto(d);
        setTimeout(() => pronto({ tipo: "sem-resposta" }), ms);
      });
    // 1. O módulo compilado aqui, mandado pela porta (`?via=bytes` pula).
    let via = null;
    const soBytes = new URLSearchParams(location.search).get("via") === "bytes";
    if (!soBytes) try {
      const modulo = await WebAssembly.compile(bytes);
      no.port.postMessage({ tipo: "modulo", modulo });
      const r = await esperar(3000);
      resultado.tentativas.push({ caminho: "modulo-pela-porta", resposta: r });
      if (r.tipo === "pronto") via = r.via;
    } catch (falha) {
      resultado.tentativas.push({ caminho: "modulo-pela-porta", erro: String(falha) });
    }
    // 2. Senão, os bytes, compilados lá dentro.
    if (!via) {
      no.port.postMessage({ tipo: "bytes", bytes: bytes.slice(0) });
      const r = await esperar(3000);
      resultado.tentativas.push({ caminho: "bytes-compilados-no-worklet", resposta: r });
      if (r.tipo === "pronto") via = r.via;
    }
    resultado.via = via;
    const t0 = performance.now();
    await ctx.startRendering();
    resultado.render_ms = Math.round(performance.now() - t0);
    resultado.segundos_de_audio = segundos;
    await new Promise((r) => setTimeout(r, 500));
    resultado.lote = ouvidas.find((d) => d.tipo === "lote") ?? null;
    resultado.andamento = ouvidas.filter((d) => d.tipo === "andamento").at(-1) ?? null;
    resultado.erros = ouvidas.filter((d) => d.tipo === "erro");
  } catch (falha) {
    resultado.falha = String(falha);
  }
  await fetch("resultado", { method: "POST", body: JSON.stringify(resultado) });
  document.body.append(JSON.stringify(resultado));
})();
