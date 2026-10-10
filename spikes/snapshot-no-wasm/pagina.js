// O custo do retrato num navegador: o mesmo laço do `medir.mjs`, e o
// resultado mandado ao servidor do spike.
(async () => {
  const marcar = (texto) => {
    const linha = document.createElement("div");
    linha.textContent = texto;
    document.body.append(linha);
  };
  try {
    const bytes = await (await fetch("snapshot_no_wasm.wasm")).arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const e = instance.exports;
    const dec = new TextDecoder();
    const VOLTAS = 300;
    const resultados = [];
    for (const [pessoas, salas, canais] of [[10, 3, 5], [50, 5, 10], [250, 10, 20], [500, 10, 40], [1000, 10, 40]]) {
      e.montar(pessoas, salas, canais);
      const tempos = [];
      const soWasm = [];
      for (let i = 0; i < VOLTAS + 30; i++) {
        const t0 = performance.now();
        const n = e.retratar();
        const t1 = performance.now();
        JSON.parse(dec.decode(new Uint8Array(e.memory.buffer, e.saida_ptr(), n)));
        const t2 = performance.now();
        if (i >= 30) {
          tempos.push(t2 - t0);
          soWasm.push(t1 - t0);
        }
      }
      // **A média de um lote**, além da mediana: o relógio do navegador sem
      // isolamento anda de 0,1 ms em 0,1 ms (o do Safari, às vezes de 1 ms), e
      // uma volta de 0,05 ms some nele. O lote não depende da resolução.
      const t0Lote = performance.now();
      for (let i = 0; i < VOLTAS; i++) {
        const n = e.retratar();
        JSON.parse(dec.decode(new Uint8Array(e.memory.buffer, e.saida_ptr(), n)));
      }
      const mediaDoLote = (performance.now() - t0Lote) / VOLTAS;
      const mediana = (lista) => [...lista].sort((a, b) => a - b)[Math.floor(lista.length / 2)];
      const p95 = (lista) => [...lista].sort((a, b) => a - b)[Math.floor(lista.length * 0.95)];
      const r = {
        pessoas, salas, canais,
        bytes: e.saida_len(),
        media_do_lote_ms: +mediaDoLote.toFixed(4),
        mediana_ms: +mediana(tempos).toFixed(3),
        p95_ms: +p95(tempos).toFixed(3),
        so_wasm_mediana_ms: +mediana(soWasm).toFixed(3),
      };
      resultados.push(r);
      marcar(JSON.stringify(r));
    }
    // O relógio do Safari é grosso (100 µs, ou 1 ms com o isolamento): a
    // mediana de 300 voltas ainda separa o que importa, que é a casa do ms.
    await fetch("resultado", {
      method: "POST",
      body: JSON.stringify({ agente: navigator.userAgent, hash_de_prova: [e.hash_de_prova(), e.hash_de_prova()], resultados }),
    });
    marcar("feito");
  } catch (falha) {
    await fetch("resultado", { method: "POST", body: JSON.stringify({ agente: navigator.userAgent, falha: String(falha) }) });
  }
})();
