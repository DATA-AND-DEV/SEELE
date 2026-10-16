// O MOD comum: só a API que o prelúdio do desktop monta, sem saber onde roda.
// A página confere cada resposta. Descartável.
console.log("mod comum de pé");
SeeleUI.aoEvento((ev) => {
  if (ev.nome === "ping") {
    SeeleMods.request("eco", "pong", ev.valor);
  } else if (ev.nome === "cpu") {
    // Um trabalho de CPU puro, medido dentro do MOD.
    const t0 = Date.now();
    let x = 0;
    for (let i = 0; i < 1000000; i++) x = (x + i * 7) % 1000003;
    SeeleMods.request("eco", "cpu", { ms: Date.now() - t0, x });
  } else if (ev.nome === "json") {
    // O que um MOD faz o tempo todo: montar uma árvore e serializá-la.
    const t0 = Date.now();
    let tamanho = 0;
    for (let i = 0; i < 200; i++) {
      const arvore = { tipo: "lista", itens: Array.from({ length: 50 }, (_, j) => ({ tipo: "texto", texto: "item " + j, classe: "c" + (j % 5) })) };
      tamanho += JSON.stringify(arvore).length;
    }
    SeeleMods.request("eco", "json", { ms: Date.now() - t0, tamanho });
  }
});
(async () => {
  // O retrato, e não o `snapshot` nem a `regiao`, que saíram com as APIs 3 a 5.
  const retrato = await SeeleUI.retrato();
  const capacidades = SeeleUI.capacidades();
  await new Promise((ok) => setTimeout(ok, 30));
  let n = 0;
  await new Promise((ok) => {
    const id = setInterval(() => { n += 1; if (n === 3) { clearInterval(id); ok(); } }, 10);
  });
  const erro = await SeeleMods.request("nao-existe", "x", 1).then(() => "resolveu", (e) => "rejeitou: " + e.message);
  let grande;
  try {
    await SeeleMods.request("eco", "grande", "é".repeat(7000));
    grande = "passou";
  } catch (e) {
    grande = e.message.slice(0, 40);
  }
  await SeeleMods.request("eco", "pronto", {
    retrato,
    capacidades,
    intervalos: n,
    erro,
    grande,
    superficies: typeof SeeleUI.superficies,
    console: typeof console.log,
  });
})();
