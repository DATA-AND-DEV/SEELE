// **O que a pessoa estava fazendo numa sessão é dela** (E3 da 0.17; requisito
// da revisão do Codex). Com o HTML e os scripts reais; a ponte é um duble do
// contrato novo (fronteiras.md v2), com três sessões abertas.
//
// # O que ela mede
//
// 1. **Rascunho**: escrito em A, trocar para B e apertar ENVIAR não manda
//    nada — nem a A nem a B — com o texto de A; o campo em B está vazio. Voltar
//    a A devolve o texto.
// 2. **Anexo**: escolhido em A, não aparece em B, e ENVIAR em B não sobe
//    arquivo nenhum. Voltar a A devolve o anexo.
// 3. **Busca**: aberta com um termo em A, fechada em B, e de volta em A.
// 4. **A pergunta dos MODs** aberta numa entrada a partir de A não aparece em
//    B, e não há como confirmá-la lá.
// 5. **A faixa do veredito** (PRIMEIRO CONTATO) de A não aparece em B nem em
//    C, e a de B não aparece em A.
//
// Não há citação nem resposta a mensagem neste produto (procurado em `ui/`):
// quando houver, ela entra no `guardarOEstadoDaVista` e aqui.
//
// Rodar:  node apps/seele-app/bancada/estado-por-sessao.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const sessao = (id, nome) => ({
  carimbo: { sessao: id, geracao: 1 }, endereco: `${nome.toLowerCase()}.exemplo:8383`, nome,
  na_vista: id === 1, foco_de_voz: id === 1, na_sala_de_voz: null, onde: "Outro", enlace: "online",
  encerrada: null, nao_lidas: 0, mudou: false,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((base) => {
      window.recebidos = [];
      const tabela = { ...base };
      window.tabela = tabela;
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            recebidos.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (cmd === "ver_sessao" && window.segurarAsTrocas) {
              return new Promise((resolver) => (window.trocasSeguras ??= []).push({ args, resolver }));
            }
            if (cmd === "ver_sessao") {
              tabela.sessoes = tabela.sessoes.map((s) => ({ ...s, na_vista: s.carimbo.sessao === args.sessao }));
              const nome = tabela.sessoes.find((s) => s.carimbo.sessao === args.sessao).nome;
              tabela.snapshot = { ...tabela.snapshot, server: nome };
              return { carimbo: { sessao: args.sessao, geracao: 1 }, vista: 100 + args.sessao, snapshot: tabela.snapshot };
            }
            return structuredClone(tabela[cmd] ?? null);
          },
        },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null }),
      snapshot: retrato({ server: "A" }),
      sessoes: [sessao(1, "A"), sessao(2, "B"), sessao(3, "C")],
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(async () => {
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      await recarregarSessoes();
      desenhar(tabela.snapshot);
    });
    await page.waitForFunction(() => linhaAberta !== null);

    // Em A: o texto, o anexo e a busca.
    await page.fill("#campo-mensagem", "texto que é de A");
    await page.evaluate(() => {
      guardarAnexo({ nome: "mapa-de-A.pdf", tamanho: 2048, caminho: "/tmp/mapa-de-A.pdf" });
      $("form-busca").hidden = false;
      $("campo-busca").value = "desvio";
      // A pergunta dos MODs de uma entrada feita a partir de A.
      $("mods-aceite").hidden = false;
    });

    // Para B.
    await page.evaluate(() => { recebidos.length = 0; return verSessao(2); });
    const emB = await page.evaluate(() => ({
      vista: vistaDaJanela.sessao,
      campo: $("campo-mensagem").value,
      anexo: !$("anexo-pendente").hidden,
      anexoNome: $("anexo-nome").textContent,
      busca: !$("form-busca").hidden,
      termo: $("campo-busca").value,
      aceite: !$("mods-aceite").hidden,
    }));
    assert.equal(emB.vista, 2, "a vista não foi para B");
    assert.equal(emB.campo, "", `o texto de A ficou no campo de B: ${emB.campo}`);
    assert.equal(emB.anexo, false, `o anexo de A aparece em B: ${emB.anexoNome}`);
    assert.equal(emB.busca, false, "a busca de A ficou aberta em B");
    assert.equal(emB.termo, "", "o termo da busca de A ficou em B");
    assert.equal(emB.aceite, false, "a pergunta dos MODs de A aparece em B");

    // ENVIAR em B: nada sai com o texto nem o arquivo de A.
    await page.waitForFunction(() => linhaAberta !== null);
    await page.click("#botao-enviar");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);
    const saidas = await page.evaluate(() => recebidos.filter((r) => ["send_message", "enviar_anexo"].includes(r.cmd)));
    assert.deepEqual(saidas, [], `ENVIAR em B mandou alguma coisa: ${JSON.stringify(saidas)}`);
    const comTextoDeA = await page.evaluate(() => recebidos.filter((r) => JSON.stringify(r.args ?? {}).includes("texto que é de A")));
    assert.deepEqual(comTextoDeA, [], `o texto de A saiu por algum comando: ${JSON.stringify(comTextoDeA)}`);

    // De volta a A: tudo está lá.
    await page.evaluate(() => verSessao(1));
    const emA = await page.evaluate(() => ({
      campo: $("campo-mensagem").value,
      anexo: !$("anexo-pendente").hidden,
      anexoNome: $("anexo-nome").textContent,
      busca: !$("form-busca").hidden,
      termo: $("campo-busca").value,
    }));
    assert.equal(emA.campo, "texto que é de A", `o rascunho de A sumiu: ${JSON.stringify(emA)}`);
    assert.equal(emA.anexo, true, "o anexo de A sumiu");
    assert.equal(emA.anexoNome, "mapa-de-A.pdf", `o anexo de A voltou outro: ${emA.anexoNome}`);
    assert.equal(emA.busca, true, "a busca de A não voltou aberta");
    assert.equal(emA.termo, "desvio", "o termo da busca de A sumiu");

    // R3. **Dois cliques antes de a primeira troca responder** (A → B → C, com
    //     C respondendo antes de B): o rascunho e o anexo de A são guardados
    //     UMA vez, e a tela de transição não os sobrescreve.
    await page.evaluate(async () => {
      window.segurarAsTrocas = true;
      window.trocasSeguras = [];
      window.duas = [verSessao(2), verSessao(3)];
      await new Promise((r) => setTimeout(r, 50));
      const resposta = (id) => {
        tabela.sessoes = tabela.sessoes.map((s) => ({ ...s, na_vista: s.carimbo.sessao === id }));
        return { carimbo: { sessao: id, geracao: 1 }, vista: 300 + id, snapshot: { ...tabela.snapshot, server: `S${id}` } };
      };
      trocasSeguras[1].resolver(resposta(3));
      await duas[1];
      trocasSeguras[0].resolver(resposta(2));
      await Promise.allSettled(duas);
      window.segurarAsTrocas = false;
    });
    assert.equal(await page.evaluate(() => vistaDaJanela.sessao), 3, "a troca mais nova (C) não ficou na vista");
    await page.evaluate(() => verSessao(1));
    const emADeNovo = await page.evaluate(() => ({ campo: $("campo-mensagem").value, anexo: $("anexo-nome").textContent }));
    assert.equal(emADeNovo.campo, "texto que é de A", `o segundo clique sobrescreveu o rascunho de A: ${JSON.stringify(emADeNovo)}`);
    assert.equal(emADeNovo.anexo, "mapa-de-A.pdf", `o segundo clique sobrescreveu o anexo de A: ${JSON.stringify(emADeNovo)}`);

    // E C, que nunca teve nada, não herda o que A tem agora.
    await page.evaluate(() => verSessao(3));
    assert.equal(await page.evaluate(() => $("campo-mensagem").value), "", "C herdou o rascunho de A");
    assert.equal(await page.evaluate(() => $("anexo-pendente").hidden), true, "C herdou o anexo de A");

    // 5. **A faixa do veredito é da sessão** (achado do CUA com três sessões:
    //    na vista de A, PRIMEIRO CONTATO mostrava a impressão de B, a última a
    //    entrar). A entra com a dela, B com a dela, e voltar a A mostra a de A.
    const faixa = () => page.evaluate(() => ({ visivel: !$("veredito").hidden, texto: $("veredito-texto").textContent }));
    await page.evaluate(async () => {
      await verSessao(1);
      mostrarVeredito({ FirstContact: { fingerprint: "b36b4e-impressao-de-A" } });
      await verSessao(2);
      mostrarVeredito({ FirstContact: { fingerprint: "767940-impressao-de-B" } });
      await verSessao(1);
    });
    const vereditoEmA = await faixa();
    assert.equal(vereditoEmA.visivel, true, "a faixa de A não voltou com A");
    assert.ok(vereditoEmA.texto.includes("b36b4e"), `na vista de A, a faixa não diz a impressão de A: ${vereditoEmA.texto}`);
    assert.ok(!vereditoEmA.texto.includes("767940"), `na vista de A, a faixa diz a impressão de B: ${vereditoEmA.texto}`);
    await page.evaluate(() => verSessao(3));
    assert.equal((await faixa()).visivel, false, "C, que não teve veredito, mostra a faixa de outra sessão");
    await page.evaluate(() => verSessao(2));
    assert.ok((await faixa()).texto.includes("767940"), "a faixa de B não voltou com B");

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("estado por sessão: o rascunho, o anexo e a busca de A não vão a B (ENVIAR em B não manda nada de A), voltam inteiros em A, e a pergunta dos MODs de A não aparece em B.");
})().catch((e) => { console.error(e); process.exit(1); });
