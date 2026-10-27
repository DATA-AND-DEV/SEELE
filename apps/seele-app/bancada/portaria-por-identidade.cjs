// **A portaria lembra QUEM bateu, e não quantos** (achado estático do Codex,
// prioridade E7). Com o HTML e os scripts reais; a ponte é um duble que muda
// de hospedagem quando a bancada manda.
//
// # Os casos que reprovavam na lógica de contagem
//
// 1. **Trocar de hospedagem entre duas leituras**, com um pedido em cada: o
//    DEPOIS dado ao pedido de A não cala o de B.
// 2. **Na mesma hospedagem**, um pedido calado que é decidido e um novo que
//    chega (a contagem fica em 1): o novo aparece.
// 3. **Parar A e subir B durante o diálogo**, com a MESMA impressão: a decisão
//    não é aplicada a B; nada é decidido, e a camada diz por quê.
// 4. **Da entrada** (a sessão escondida), o VER QUEM É abre a porta à vista e
//    diz de qual servidor ela é.
//
// Rodar:  node apps/seele-app/bancada/portaria-por-identidade.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const estado = (servidor, geracao, pendentes) => ({
  hospedando: true, aberto: false, tem_senha: true, aceita_convites: false, portaria_ligada: true,
  pendentes, alcance: "RedeLocal", hospedagem: { servidor, geracao, nome: servidor === "A" ? "Casa do Lago" : "Oficina" },
});
const pedido = (impressao, bateu_em) => ({
  impressao, apelido: `quem-${impressao.slice(0, 2)}`, segredo: "senha", observacao: null, bateu_em, batidas: 1,
  decidido_em: null, admitido: null,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          // `{ __lanca: X }` é uma recusa: o `invoke` rejeita com X.
          if (tabela[cmd] && typeof tabela[cmd] === "object" && "__lanca" in tabela[cmd]) throw structuredClone(tabela[cmd].__lanca);
          return structuredClone(tabela[cmd] ?? null);
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ meu_servidor: { nome: "Casa do Lago", alvo: "127.0.0.1:8383" } }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    const faixa = () => page.evaluate(() => !$("portaria-batendo").hidden);
    const porta = async (e, p) => {
      await page.evaluate(({ e, p }) => { tabela.estado_da_porta = e; tabela.pedidos_da_portaria = p; return atualizarPorta(); }, { e, p });
    };

    // 0. A estrutura: a camada da porta não mora dentro de tela nenhuma (o
    //    candidato do diagnóstico E7: na 0.16 ela estava dentro de
    //    `#tela-sessao`, e da entrada abria escondida).
    assert.equal(await page.evaluate(() => $("portaria").closest("section.tela")?.id ?? null), null,
      "a camada da porta voltou para dentro de uma tela, e some com ela");

    // 1. A com um pedido; DEPOIS; B com outro pedido: a faixa volta.
    await porta(estado("A", 1, 1), [pedido("aa11", 100)]);
    assert.equal(await faixa(), true, "o pedido de A não acendeu a faixa");
    await page.click("#portaria-batendo-depois");
    assert.equal(await faixa(), false, "o DEPOIS não calou o pedido de A");
    await porta(estado("B", 2, 1), [pedido("bb22", 200)]);
    assert.equal(await faixa(), true, "o DEPOIS dado em A calou o pedido de B");

    // 2. Mesma hospedagem: o calado é decidido e um novo chega; a conta fica em 1.
    await page.click("#portaria-batendo-depois");
    await porta(estado("B", 2, 1), [{ ...pedido("bb22", 200), decidido_em: 300, admitido: true }, pedido("cc33", 310)]);
    assert.equal(await faixa(), true, "o pedido novo, com a mesma contagem, ficou calado");

    // 2b. **Desenhar a fila que ninguém vê não cala** (diagnóstico do H, b6):
    //     a camada escondida, e depois dentro de um ancestral escondido.
    await porta(estado("B", 2, 1), [pedido("ff66", 600)]);
    assert.equal(await faixa(), true, "o pedido ff66 não acendeu a faixa");
    await page.evaluate(() => desenharPortaria());
    await porta(estado("B", 2, 1), [pedido("ff66", 600)]);
    assert.equal(await faixa(), true, "desenhar a fila com a camada fechada calou o pedido");
    await page.evaluate(() => {
      const caixa = document.createElement("div");
      caixa.id = "caixa-escondida";
      caixa.hidden = true;
      document.body.append(caixa);
      caixa.append($("portaria"));
      $("portaria").hidden = false;
      return desenharPortaria();
    });
    await porta(estado("B", 2, 1), [pedido("ff66", 600)]);
    assert.equal(await faixa(), true, "desenhar a fila dentro de um ancestral escondido (0×0) calou o pedido");
    await page.evaluate(() => { $("portaria").hidden = true; document.body.append($("portaria")); $("caixa-escondida").remove(); });

    // 4. Da entrada: VER QUEM É abre a porta à vista, dizendo de qual servidor.
    await page.evaluate(() => {
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-boot").hidden = false;
    });
    await page.click("#portaria-batendo-ver");
    await page.waitForFunction(() => $("portaria-qual-servidor").textContent !== "");
    const aberta = await page.evaluate(() => ({
      visivel: $("portaria").getClientRects().length > 0,
      qual: $("portaria-qual-servidor").textContent,
    }));
    assert.equal(aberta.visivel, true, "da entrada, a porta abre escondida dentro da sessão");
    assert.match(aberta.qual, /OFICINA/, `a porta não diz de qual servidor é: ${aberta.qual}`);

    // 3. Parar A e subir B durante o diálogo, com a mesma impressão.
    await page.evaluate(() => fecharPortaria());
    await porta(estado("A", 3, 1), [pedido("dd44", 400)]);
    await page.evaluate(() => abrirPortaria());
    await page.waitForFunction(() => document.querySelectorAll("#portaria-fila button").length > 0);
    await page.evaluate(() => [...document.querySelectorAll("#portaria-fila button")].find((b) => b.textContent === "ADMITIR").click());
    await page.waitForFunction(() => !$("moderar").hidden);
    // Enquanto o diálogo está aberto, A para e B sobe com um pedido da MESMA impressão.
    await page.evaluate(() => {
      tabela.estado_da_porta = { ...tabela.estado_da_porta, hospedagem: { servidor: "B", geracao: 4, nome: "Oficina" } };
      chamadas.length = 0;
    });
    await page.click("#moderar-confirmar");
    await page.waitForTimeout(300);
    const decisoes = await page.evaluate(() => chamadas.filter((c) => c.cmd === "decidir_pedido"));
    assert.deepEqual(decisoes, [], `a decisão aberta em A foi aplicada a B: ${JSON.stringify(decisoes)}`);
    assert.match(await page.evaluate(() => $("portaria-erro").textContent), /MUDOU DEPOIS QUE ESTA DECISÃO FOI ABERTA/,
      "a camada não diz por que nada foi decidido");

    // E na mesma hospedagem a decisão sai, com a hospedagem do Rust junto
    // (`{servidor, geracao}`, sem o nome): o `decidir_pedido` a confere.
    await porta(estado("A", 5, 1), [pedido("ee55", 500)]);
    await page.evaluate(() => desenharPortaria());
    await page.evaluate(() => [...document.querySelectorAll("#portaria-fila button")].find((b) => b.textContent === "ADMITIR").click());
    await page.evaluate(() => { chamadas.length = 0; });
    await page.click("#moderar-confirmar");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "decidir_pedido"));
    const decidida = await page.evaluate(() => chamadas.find((c) => c.cmd === "decidir_pedido").args);
    assert.deepEqual(decidida, { impressao: "ee55", admitir: true, hospedagem: { servidor: "A", geracao: 5 } },
      `a decisão não leva a hospedagem do Rust: ${JSON.stringify(decidida)}`);

    // 5. O portão do Rust: a janela acha que é a mesma, e o Rust recusa com
    //    `HospedagemNaoConfere` (subiu de novo entre a conferência e o ato). Dito,
    //    e a fila relida.
    await porta(estado("A", 6, 1), [pedido("ff77", 700)]);
    await page.evaluate(() => desenharPortaria());
    await page.evaluate(() => {
      tabela.decidir_pedido = { __lanca: { HospedagemNaoConfere: { agora: { servidor: "A", geracao: 7 } } } };
    });
    await page.evaluate(() => [...document.querySelectorAll("#portaria-fila button")].find((b) => b.textContent === "RECUSAR").click());
    await page.click("#moderar-confirmar");
    await page.waitForFunction(() => /MUDOU DEPOIS QUE ESTA DECISÃO FOI ABERTA/.test($("portaria-erro").textContent) && !$("portaria-erro").hidden,
      null, { timeout: 5000 });
    await page.evaluate(() => { tabela.decidir_pedido = null; });

    // 6. REVOGAR também leva a hospedagem do diálogo.
    await porta(estado("A", 8, 0), [{ ...pedido("gg88", 800), decidido_em: 810, admitido: true }]);
    await page.evaluate(() => desenharPortaria());
    await page.evaluate(() => { chamadas.length = 0; [...document.querySelectorAll("#portaria-decididos button")].find((b) => b.textContent === "REVOGAR").click(); });
    await page.click("#moderar-confirmar");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "revogar_admissao"));
    const revogada = await page.evaluate(() => chamadas.find((c) => c.cmd === "revogar_admissao").args);
    assert.deepEqual(revogada, { impressao: "gg88", hospedagem: { servidor: "A", geracao: 8 } },
      `a revogação não leva a hospedagem do Rust: ${JSON.stringify(revogada)}`);

    // 7. Sem a hospedagem do Rust (um backend de antes), o ato não inventa uma.
    await porta({ ...estado("A", 0, 1), hospedagem: undefined }, [pedido("hh99", 900)]);
    await page.evaluate(() => desenharPortaria());
    await page.evaluate(() => { chamadas.length = 0; [...document.querySelectorAll("#portaria-fila button")].find((b) => b.textContent === "ADMITIR").click(); });
    await page.click("#moderar-confirmar");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "decidir_pedido"), null, { timeout: 5000 }).catch(async () => {
      assert.fail(`o ADMITIR sem a hospedagem do Rust não decidiu: ${JSON.stringify(await page.evaluate(() => ({
        chamadas: chamadas.map((c) => c.cmd), erro: $("portaria-erro").textContent, moderar: $("moderar").hidden,
        fila: $("portaria-fila").textContent })))}`);
    });
    assert.equal(await page.evaluate(() => chamadas.find((c) => c.cmd === "decidir_pedido").args.hospedagem), null,
      "sem a hospedagem do Rust, a janela mandou a geração dela");

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("portaria por identidade: o DEPOIS de A não cala B; um pedido novo com a mesma contagem aparece; a decisão aberta em A não vale em B; da entrada, a porta abre à vista dizendo de qual servidor é.");
})().catch((e) => { console.error(e); process.exit(1); });
