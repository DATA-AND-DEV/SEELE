// **O isolamento das sessões, na casca** (E3 da 0.17; contrato, «ISOLAMENTO
// FUNCIONAL»; fronteiras.md v2, §3, §4 e §8). Com o HTML e os scripts reais;
// a ponte é um duble que fala o contrato novo — `carimbo` em todo comando de
// sessão, `seele://event` como `{ sessao, geracao, vista, evento }`, respostas
// de estado como `NaSessao { carimbo, valor }` — e conta o que recebeu.
//
// # As quatro provas, do lado da casca
//
// 1. **Evento tardio depois de trocar a vista**: um `seele://event` da sessão
//    A, emitido depois de a vista ir para B, não chega aos ouvintes da tela, e
//    a placa de A é relida.
// 2. **Promessa resolvida depois da troca**: um `messages` pedido com a vista
//    em A e respondido (carimbado por A) depois da troca é recusado com
//    `ForaDaVista` — nada de A é aplicado na tela de B.
// 3. **Reconexão em segundo plano**: um evento de C com geração nova não muda a
//    vista nem o foco; um evento de C com geração VELHA é descartado.
// 4. **Clique durante a troca**: um `send_message` disparado com a vista em A,
//    no mesmo tique em que a troca para B começa, sai com o carimbo de A; e um
//    `SessaoNaoConfere` da MESMA sessão é refeito uma vez com a geração nova,
//    e nunca mandado a outra.
//
// Isto prova a casca. O nativo (main.rs, com servidores reais) é do
// hosting-lifecycle; a web com `sessao-web.js` reentrante, do browser-signaling.
//
// Rodar:  node apps/seele-app/bancada/isolamento-das-sessoes.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const carimbo = (sessao, geracao = 1) => ({ sessao, geracao });
const sessao = (id, nome, extra = {}) => ({
  carimbo: carimbo(id), endereco: `${nome.toLowerCase().replace(/ /g, "-")}.exemplo:8383`, nome,
  na_vista: false, foco_de_voz: false, na_sala_de_voz: null, onde: "Outro", enlace: "online",
  encerrada: null, nao_lidas: 0, mudou: false, ...extra,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((base) => {
      window.recebidos = [];
      window.ouvintes = new Map();
      window.adiadas = [];
      const tabela = {
        ...base,
        sessoes: [
          { ...base.__sessoes[0], na_vista: true, foco_de_voz: true },
          base.__sessoes[1],
          base.__sessoes[2],
        ],
      };
      window.tabela = tabela;
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            recebidos.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            // As respostas que a bancada segura e solta na ordem que quiser.
            if (window.segurarTodos?.has(cmd)) {
              return new Promise((resolver, recusar) => adiadas.push({ cmd, args, resolver, recusar }));
            }
            if (cmd === "ver_sessao") {
              const id = args.sessao;
              tabela.sessoes = tabela.sessoes.map((s) => ({ ...s, na_vista: s.carimbo.sessao === id }));
              return { carimbo: { sessao: id, geracao: 1 }, vista: 100 + id, snapshot: tabela.snapshot };
            }
            // A resposta que a bancada segura para soltar depois.
            if (window.segurar === cmd) {
              return new Promise((resolver, recusar) => adiadas.push({ cmd, args, resolver, recusar }));
            }
            if (typeof window.responder?.[cmd] === "function") return window.responder[cmd](args);
            return structuredClone(tabela[cmd] ?? null);
          },
        },
        event: {
          listen: async (nome, fn) => {
            if (!ouvintes.has(nome)) ouvintes.set(nome, []);
            ouvintes.get(nome).push(fn);
            return () => {};
          },
        },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
      window.emitir = (nome, payload) => {
        for (const fn of ouvintes.get(nome) ?? []) fn({ event: nome, id: 0, payload });
      };
    }, {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null }),
      snapshot: retrato({ server: "A" }),
      __sessoes: [sessao(1, "A"), sessao(2, "B"), sessao(3, "C")],
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate(async () => {
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      await recarregarSessoes();
      // Um ouvinte da tela, pelo `listen` da casca, como os de tela-sessao.js.
      window.daTela = [];
      await listen("seele://event", ({ payload }) => daTela.push(payload));
    });

    const vista = () => page.evaluate(() => ({ ...vistaDaJanela }));
    assert.deepEqual(await vista(), { sessao: 1, geracao: 1 }, "a vista não veio da lista de sessões");

    // 4a. O clique durante a troca: o carimbo é o da vista no instante do clique.
    await page.evaluate(() => {
      recebidos.length = 0;
      invoke("send_message", { channel: 1, body: "oi" }).catch(() => {});
      verSessao(2);
    });
    await page.waitForFunction(() => vistaDaJanela.sessao === 2);
    const envio = await page.evaluate(() => recebidos.find((r) => r.cmd === "send_message"));
    assert.deepEqual(envio.args.carimbo, { sessao: 1, geracao: 1 },
      `o envio disparado com a vista em A saiu com outro carimbo: ${JSON.stringify(envio)}`);

    // 1. O evento tardio de A, depois de a vista ir para B, não chega à tela.
    await page.evaluate(() => {
      daTela.length = 0;
      emitir("seele://event", { sessao: 1, geracao: 1, vista: false, evento: { MessageReceived: { channel: 1 } } });
      emitir("seele://event", { sessao: 2, geracao: 1, vista: true, evento: { MessageReceived: { channel: 9 } } });
    });
    const naTela = await page.evaluate(() => daTela);
    assert.deepEqual(naTela, [{ MessageReceived: { channel: 9 } }],
      `a tela recebeu evento de outra sessão, ou não recebeu o da vista: ${JSON.stringify(naTela)}`);
    assert.ok((await page.evaluate(() => contasDaVista.eventosDesviados)) >= 1, "o evento de A não foi contado como desviado");

    // 2. A promessa de A, resolvida depois da troca para C, não é aplicada.
    await page.evaluate(() => {
      window.segurar = "messages";
      window.resultado = null;
      invoke("messages", { channel: 1 })
        .then((v) => { resultado = { ok: v }; })
        .catch((f) => { resultado = { falha: f }; });
    });
    await page.waitForFunction(() => adiadas.length === 1);
    const pedidoDeB = await page.evaluate(() => adiadas[0].args.carimbo);
    assert.deepEqual(pedidoDeB, { sessao: 2, geracao: 1 }, "o `messages` não saiu com o carimbo de B");
    await page.evaluate(async () => {
      window.segurar = null;
      await verSessao(3);
      adiadas[0].resolver({ carimbo: { sessao: 2, geracao: 1 }, valor: [{ id: 99, body: "de B" }] });
    });
    await page.waitForFunction(() => resultado !== null);
    const resultado = await page.evaluate(() => resultado);
    assert.ok(resultado.falha?.ForaDaVista?.sessao === 2,
      `a resposta de B chegou à tela de C: ${JSON.stringify(resultado)}`);
    assert.match(await page.evaluate(() => fraseDeErro({ ForaDaVista: { sessao: 2, comando: "messages" } })), /NOUTRO SERVIDOR/,
      "a recusa ForaDaVista não tem frase");

    // 3. Reconexão de A em segundo plano: a vista e o foco não mudam; a geração
    //    velha de A é descartada.
    await page.evaluate(() => {
      daTela.length = 0;
      emitir("seele://event", { sessao: 1, geracao: 2, vista: false, evento: { LinkStateChanged: "Online" } });
    });
    assert.deepEqual(await vista(), { sessao: 3, geracao: 1 }, "a reconexão de A em segundo plano mexeu na vista");
    assert.equal(await page.evaluate(() => geracoesDasSessoes.get(1)), 2, "a geração nova de A não foi anotada");
    await page.evaluate(() => {
      emitir("seele://event", { sessao: 3, geracao: 0, vista: true, evento: { Velho: true } });
    });
    assert.deepEqual(await page.evaluate(() => daTela), [], "um evento de geração velha da vista chegou à tela");
    const focos = await page.evaluate(() => recebidos.filter((r) => r.cmd === "focar_voz").length);
    assert.equal(focos, 0, "trocar de vista ou reconectar pediu o foco de voz sozinho");

    // 4b. `SessaoNaoConfere` da MESMA sessão: refeito uma vez com a geração nova.
    await page.evaluate(() => {
      recebidos.length = 0;
      let vez = 0;
      window.responder = {
        open_channel: (args) => {
          vez += 1;
          if (vez === 1) throw { SessaoNaoConfere: { esperado: 2, veio: args.carimbo.geracao } };
          return null;
        },
      };
    });
    await page.evaluate(() => invoke("open_channel", { channel: 1 }));
    const aberturas = await page.evaluate(() => recebidos.filter((r) => r.cmd === "open_channel").map((r) => r.args.carimbo));
    assert.deepEqual(aberturas, [{ sessao: 3, geracao: 1 }, { sessao: 3, geracao: 2 }],
      `o refeito não foi uma vez, na mesma sessão, com a geração nova: ${JSON.stringify(aberturas)}`);

    // ---- As corridas (revisão do Codex): promessas soltas fora de ordem. ----
    const entrada = (sessao) => ({ carimbo: { sessao, geracao: geracoesDe(sessao) }, vista: 200 + sessao, snapshot: null });
    const geracoesDe = (s) => (s === 1 ? 2 : 1);

    // R1. Duas trocas, para 2 e para 3; a de 3 responde ANTES da de 2. A vista
    //     é 3 — a do último pedido —, e a resposta tardia de 2 não a muda.
    await page.evaluate(() => {
      window.segurarTodos = new Set(["ver_sessao"]);
      adiadas.length = 0;
      window.trocas = [invoke("ver_sessao", { sessao: 2 }), invoke("ver_sessao", { sessao: 3 })]
        .map((p) => p.then((v) => ({ ok: v.carimbo.sessao }), (f) => ({ falha: f })));
    });
    await page.waitForFunction(() => adiadas.length === 2);
    await page.evaluate((e) => {
      adiadas[1].resolver(e[1]);
    }, [entrada(2), entrada(3)]);
    await page.waitForFunction(() => vistaDaJanela.sessao === 3);
    await page.evaluate((e) => { adiadas[0].resolver(e[0]); }, [entrada(2), entrada(3)]);
    const trocas = await page.evaluate(() => Promise.all(trocas));
    assert.equal(await page.evaluate(() => vistaDaJanela.sessao), 3, "a resposta tardia da troca para 2 mudou a vista");
    assert.ok(trocas[0].falha?.VistaSuperada, `a troca superada não foi recusada: ${JSON.stringify(trocas)}`);
    await page.evaluate(() => { window.segurarTodos = new Set(); recebidos.length = 0; return invoke("open_channel", { channel: 1 }); });
    assert.equal(await page.evaluate(() => recebidos.find((r) => r.cmd === "open_channel").args.carimbo.sessao), 3,
      "depois das trocas fora de ordem, o comando saiu com o carimbo de outra sessão");

    // R2. O refeito de A (SessaoNaoConfere), solto depois da troca para C, passa
    //     pelo mesmo pós-processamento: ForaDaVista, e não o NaSessao cru.
    await page.evaluate(async () => {
      window.segurarTodos = new Set(["ver_sessao"]);
      adiadas.length = 0;
      const ida = invoke("ver_sessao", { sessao: 1 });
      await new Promise((r) => setTimeout(r, 0));
      adiadas[0].resolver({ carimbo: { sessao: 1, geracao: 2 }, vista: 301, snapshot: null });
      await ida;
      window.segurarTodos = new Set();
      let vez = 0;
      window.responder = {
        snapshot: (args) => {
          vez += 1;
          if (vez === 1) throw { SessaoNaoConfere: { esperado: 3, veio: args.carimbo.geracao } };
          return new Promise((resolver) => { window.soltarORefeito = () => resolver({ carimbo: { sessao: 1, geracao: 3 }, valor: { de: "A" } }); });
        },
      };
      window.doRefeito = null;
      invoke("snapshot").then((v) => { doRefeito = { ok: v }; }, (f) => { doRefeito = { falha: f }; });
    });
    await page.waitForFunction(() => typeof soltarORefeito === "function");
    await page.evaluate(async () => {
      window.responder = {};
      await invoke("ver_sessao", { sessao: 3 });
      soltarORefeito();
    });
    await page.waitForFunction(() => doRefeito !== null);
    const refeito = await page.evaluate(() => doRefeito);
    assert.equal(refeito.falha?.ForaDaVista?.sessao, 1, `o refeito de A chegou cru à tela de C: ${JSON.stringify(refeito)}`);

    // R4a. Geração velha da vista: uma resposta carimbada com geração abaixo
    //      da última vista daquela sessão é recusada.
    await page.evaluate(() => {
      anotarGeracao({ sessao: 3, geracao: 5 });
      window.responder = { messages: () => ({ carimbo: { sessao: 3, geracao: 4 }, valor: ["velha"] }) };
    });
    const velha = await page.evaluate(() => invoke("messages", { channel: 1 }).then((v) => ({ ok: v }), (f) => ({ falha: f })));
    assert.equal(velha.falha?.ForaDaVista?.motivo, "geracao-velha", `a resposta de geração velha foi aceita: ${JSON.stringify(velha)}`);

    // R4b. A → B → A: a resposta pedida na primeira visita a A, solta na
    //      segunda, não vale.
    await page.evaluate(async () => {
      window.responder = {};
      await invoke("ver_sessao", { sessao: 1 });
      window.responder = { messages: () => new Promise((resolver) => { window.soltarAPrimeira = () => resolver({ carimbo: { sessao: 1, geracao: 3 }, valor: ["primeira visita"] }); }) };
      window.daPrimeira = null;
      invoke("messages", { channel: 1 }).then((v) => { daPrimeira = { ok: v }; }, (f) => { daPrimeira = { falha: f }; });
      window.responder = {};
      await invoke("ver_sessao", { sessao: 2 });
      await invoke("ver_sessao", { sessao: 1 });
      soltarAPrimeira();
    });
    await page.waitForFunction(() => daPrimeira !== null);
    const primeira = await page.evaluate(() => daPrimeira);
    assert.equal(primeira.falha?.ForaDaVista?.motivo, "outra-visita",
      `a resposta da primeira visita a A valeu na segunda: ${JSON.stringify(primeira)}`);
    await page.evaluate(() => verSessao(3));

    // E o TRAZER A VOZ PARA CÁ manda o carimbo da sessão em que foi apertado.
    await page.evaluate(async () => {
      recebidos.length = 0;
      window.responder = {};
      await recarregarSessoes();
    });
    assert.equal(await page.evaluate(() => $("voz-foco").hidden), false, "com três sessões, a linha da voz não aparece");
    assert.match(await page.evaluate(() => $("voz-foco-frase").textContent), /A VOZ ESTÁ EM A/, "a linha da voz não diz onde a voz está");
    // A linha do operador diz aonde a tecla vai, sem prometer voz aqui.
    const linhaDaVoz = await page.evaluate(() => $("voz-estado").textContent);
    assert.match(linhaDaVoz, /fala em A|microfone é de A/, `a linha do operador não diz que a voz é de A: ${linhaDaVoz}`);
    assert.doesNotMatch(linhaDaVoz, /ouvindo/, `a linha do operador diz «ouvindo» numa vista sem voz: ${linhaDaVoz}`);
    // A tecla de falar e um clique na tela, nesta vista, não trazem a voz.
    await page.evaluate(() => { recebidos.length = 0; document.activeElement?.blur(); });
    await page.keyboard.down("Space");
    await page.waitForTimeout(150);
    await page.keyboard.up("Space");
    await page.mouse.click(900, 400);
    await page.waitForTimeout(150);
    const implicitos = await page.evaluate(() => recebidos.filter((r) => ["focar_voz", "enter_voice_room"].includes(r.cmd)));
    assert.deepEqual(implicitos, [], `a tecla ou um clique trouxe a voz sem o ato: ${JSON.stringify(implicitos)}`);
    await page.click("#voz-foco-trazer");
    await page.waitForFunction(() => recebidos.some((r) => r.cmd === "focar_voz"));
    const foco = await page.evaluate(() => recebidos.find((r) => r.cmd === "focar_voz").args);
    assert.deepEqual(foco, { carimbo: { sessao: 3, geracao: 1 } }, `o TRAZER mandou outro carimbo: ${JSON.stringify(foco)}`);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("isolamento das sessões na casca: o clique leva o carimbo de onde foi dado; o evento de outra sessão e o de geração velha não chegam à tela; a promessa resolvida depois da troca é recusada (ForaDaVista); a reconexão em segundo plano não mexe na vista nem no foco; o refeito é na mesma sessão; o TRAZER A VOZ manda o carimbo da vista.");
})().catch((e) => { console.error(e); process.exit(1); });
