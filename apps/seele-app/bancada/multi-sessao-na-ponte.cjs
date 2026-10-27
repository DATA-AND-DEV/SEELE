// **Três sessões na página, pela ponte web de verdade** (E3 da 0.17;
// fronteiras.md v2, §7). Sem duble do Tauri: a casca larga com a `ponte-web.js`
// real, e três sessões de mentira postas pela costura de sempre
// (`sessaoDaPonte.atual = …`, que agora SOMA ao registro). Cada sessão de
// mentira responde `atender` como o `.wasm` e anota o que recebeu.
//
// # O que ela mede (a casca + a ponte)
//
// 1. `sessoes()` lista as três, com a vista e o foco; a trilha as desenha.
// 2. **O carimbo escolhe a sessão**: um `send_message` com a vista em A vai a
//    A, mesmo com a troca para B começando no mesmo tique; depois da troca, o
//    seguinte vai a B. Nenhum vai à sessão errada.
// 3. **O evento de C** (ao fundo) não chega à tela: conta como não lida na
//    placa dele.
// 4. **O foco de voz em ordem**: TRAZER A VOZ PARA CÁ em B tira A da sala
//    (`leave_voice_room` em A) ANTES de B ganhar o foco; só uma sessão tem a
//    voz.
// 5. **Sair de B** não mexe em A nem em C.
//
// O transporte de verdade (WebRTC, três servidores) é a bancada do
// browser-signaling (`sessao-no-navegador.cjs`); esta prova a casca e a ponte.
//
// Rodar:  node apps/seele-app/bancada/multi-sessao-na-ponte.cjs

const assert = require("node:assert/strict");
const { servir } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript(() => {
      try { localStorage.setItem("seele.casca", "larga"); } catch {}
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForTimeout(500);

    await page.evaluate(async () => {
      window.anotado = [];
      const falsa = (nome, salaOcupada) => {
        const sala = {
          server: nome, me: 1, nickname: "Alex", voice_rooms: [{
            id: 1, name: `Sala de ${nome}`, limit: 8, password_required: false, occupied_by_us: salaOcupada,
            channel: 1, people: [], sync: null,
          }],
          presentes: [], channels: [{ id: 1, name: "geral", open: true }], messages_revision: 1,
          may_manage_voice_rooms: false, may_kick: false, may_ban: false, may_remove_message: false,
          may_move_person: false, may_customise_server: false, may_delete_rooms: false, tela: null, transmissoes: [], ended: null,
        };
        const s = {
          alvo: { endereco: `${nome.toLowerCase()}.exemplo:8383`, impressao: nome },
          acabou: false,
          retrato: () => ({ sala, contados: {}, telemetria: { rtt_ms: 10, jitter_ms: 1, loss_fraction: 0, signal: 90, sync_band: "Nominal" } }),
          enlace: () => "Online",
          atender: (comando, args) => {
            anotado.push({ sessao: nome, comando, args });
            if (comando === "leave_voice_room") sala.voice_rooms[0].occupied_by_us = false;
            if (comando === "enter_voice_room") sala.voice_rooms[0].occupied_by_us = true;
            if (comando === "messages") return { resposta: [] };
            return { resposta: null };
          },
          encerrar: async () => { s.acabou = true; anotado.push({ sessao: nome, comando: "encerrar" }); },
        };
        s.snapshot = () => retratoDaSessao(s);
        return s;
      };
      // A primeira ganha o foco (ninguém o tinha); as outras, não.
      sessaoDaPonte.atual = falsa("A", true);
      sessaoDaPonte.atual = falsa("B", false);
      sessaoDaPonte.atual = falsa("C", false);
      // A vista volta para A, como se a pessoa a tivesse escolhido.
      const entrada = await invoke("ver_sessao", { sessao: 1 });
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(entrada.snapshot);
      await recarregarSessoes();
      window.daTela = [];
      await listen("seele://event", ({ payload }) => daTela.push(payload));
    });

    // 1. As três, a vista e o foco.
    const lista = await page.evaluate(() => sessoesNaTela.map((s) => [s.carimbo.sessao, s.nome, s.na_vista, s.foco_de_voz]));
    assert.deepEqual(lista, [[1, "A", true, true], [2, "B", false, false], [3, "C", false, false]],
      `a ponte não lista as três sessões com a vista e o foco: ${JSON.stringify(lista)}`);
    assert.equal(await page.evaluate(() => $("trilha-sessoes").children.length), 3, "a trilha não desenha as três");

    // 2. O carimbo escolhe a sessão, no clique.
    await page.evaluate(async () => {
      anotado.length = 0;
      const envio = invoke("send_message", { channel: 1, body: "para A" });
      const troca = verSessao(2);
      await Promise.allSettled([envio, troca]);
      await invoke("send_message", { channel: 1, body: "para B" });
      // O clique em A cujo pedido chega à ponte DEPOIS da troca (no app, o
      // IPC é assíncrono): o carimbo é o de A, e é a A que ele vai.
      await invocarNaPonte("send_message", { channel: 1, body: "tardio de A", carimbo: { sessao: 1, geracao: 1 } });
    });
    const envios = await page.evaluate(() => anotado.filter((a) => a.comando === "send_message").map((a) => [a.sessao, a.args.body, "carimbo" in a.args]));
    assert.deepEqual(envios, [["A", "para A", false], ["B", "para B", false], ["A", "tardio de A", false]],
      `um envio foi à sessão errada, ou o carimbo chegou ao .wasm: ${JSON.stringify(envios)}`);

    // 3. O evento de C, ao fundo, não chega à tela, e vira não lida.
    await page.evaluate(() => {
      daTela.length = 0;
      emitirNaPonte("seele://event", { sessao: 3, geracao: 1, vista: false, evento: "MessagesChanged" });
      sessoesDaPonte.get(3).naoLidas = 1;
    });
    assert.deepEqual(await page.evaluate(() => daTela), [], "o evento de C chegou à tela de B");
    await page.evaluate(() => recarregarSessoes());
    assert.equal(await page.evaluate(() => sessoesNaTela.find((s) => s.carimbo.sessao === 3).nao_lidas), 1, "a placa de C não conta a não lida");

    // 4. O foco em ordem: A sai da sala ANTES de B ganhar a voz.
    await page.evaluate(() => { anotado.length = 0; });
    await page.click("#voz-foco-trazer");
    await page.waitForFunction(() => focoDaPonte === 2);
    const ordem = await page.evaluate(() => anotado.map((a) => `${a.sessao}:${a.comando}`));
    assert.ok(ordem.includes("A:leave_voice_room"), `A não saiu da sala ao perder a voz: ${ordem}`);
    const comVoz = await page.evaluate(() => sessoesNaPonte().filter((s) => s.foco_de_voz).map((s) => s.nome));
    assert.deepEqual(comVoz, ["B"], `mais de uma sessão com a voz, ou nenhuma: ${comVoz}`);

    // 4b. A época da troca na ponte: um pedido de troca mais velho que o
    //     último aplicado (chegando depois dele) não muda a vista.
    const epocas = await page.evaluate(async () => {
      await invocarNaPonte("ver_sessao", { sessao: 1, epoca: 1000 });
      const velho = await invocarNaPonte("ver_sessao", { sessao: 3, epoca: 999 }).then(() => "aceito", (f) => f);
      return { vista: vistaDaPonte, velho };
    });
    assert.equal(epocas.vista, 1, `um pedido de troca mais velho mudou a vista da ponte: ${JSON.stringify(epocas)}`);
    assert.ok(epocas.velho?.VistaSuperada, `o pedido velho não foi recusado: ${JSON.stringify(epocas)}`);
    await page.evaluate(() => invocarNaPonte("ver_sessao", { sessao: 2, epoca: 1001 }));

    // 4c. A cadeia única do foco (o achado do Codex no registro web): uma
    //     troca para C que espera a vez, com C caindo antes dela, não dá a voz
    //     a C; e, com o foco vazio depois de uma queda, uma entrada nova ao
    //     lado das outras não o toma sozinha.
    const cadeia = await page.evaluate(async () => {
      const antes = focoDaPonte;
      const troca = focarAVozNaPonte(3).then(() => "trocou", (f) => f);
      sessoesDaPonte.get(3).acabou = true;
      const resultado = await troca;
      sessoesDaPonte.get(3).acabou = false;
      return { antes, depois: focoDaPonte, resultado };
    });
    assert.equal(cadeia.resultado, "NotConnected", `a troca para uma sessão caída na espera foi feita: ${JSON.stringify(cadeia)}`);
    assert.equal(cadeia.depois, cadeia.antes, `o foco foi para uma sessão caída: ${JSON.stringify(cadeia)}`);
    const roubo = await page.evaluate(async () => {
      await naCadeiaDoFoco(() => { focoDaPonte = 0; });
      sessaoDaPonte.atual = { alvo: { endereco: "d.exemplo:8383", impressao: "D" }, acabou: false, retrato: () => null, enlace: () => "Online", atender: () => ({ resposta: null }), encerrar: async () => {} };
      const depois = focoDaPonte;
      const id = vistaDaPonte;
      sessoesDaPonte.delete(id);
      vistaDaPonte = 2;
      return depois;
    });
    assert.equal(roubo, 0, "uma entrada nova tomou o foco vazio depois de uma queda, com outras sessões abertas");

    // 5. Sair de B não mexe em A nem em C.
    await page.evaluate(() => invoke("encerrar_sessao", { carimbo: { sessao: 2, geracao: 1 } }));
    const depois = await page.evaluate(() => sessoesNaPonte().map((s) => s.nome));
    assert.deepEqual(depois, ["A", "C"], `sair de B levou outra junto: ${depois}`);
    assert.equal(await page.evaluate(() => sessoesDaPonte.get(1).acabou), false, "A acabou junto com B");

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("multi-sessão na ponte web: três sessões listadas; o envio vai à sessão do clique, sem o carimbo chegar ao .wasm; o evento de fundo não chega à tela e conta como não lida; o foco tira a antiga da sala antes de a nova ganhar a voz; sair de uma não mexe nas outras.");
})().catch((e) => { console.error(e); process.exit(1); });
