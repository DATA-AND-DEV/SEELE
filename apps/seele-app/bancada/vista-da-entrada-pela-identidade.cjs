// **A entrada pela identidade do navegador põe a sessão nova na vista da
// janela** (defeito de isolamento do E3, achado pela E2E de cargos do G;
// causa lida pelo browser-signaling). Na casca larga, com a `ponte-web.js` de
// verdade e sem duble do Tauri.
//
// O caminho do defeito: a pessoa está em S (a vista); entra em T pelo COLAR UM
// LINK, que vai pela identidade (`identidade.entrar` → `invocarNaPonte("connect")`),
// por DENTRO da ponte — sem o `invoke("connect")` da casca, que é quem punha a
// vista. A ponte punha T na vista dela; a janela ficava com S. A tela mostrava T
// e todo comando saía carimbado para S.
//
// Esta bancada põe S e T pela costura da ponte (`sessaoDaPonte.atual = …`, que
// é o que o `connect` dela faz com uma sessão aberta), entrega a resposta da
// entrada de T ao `depoisDeConectar` (como a identidade entrega) e mede:
//
// 1. a vista da janela passa a ser T, e a época da vista sobe;
// 2. um comando da tela (`send_message`) vai a T, e nenhum a S;
// 3. `ver_sessao` S, e o comando seguinte vai a S;
// 4. no caminho do desktop (a vista já em T), entregar de novo a mesma entrada
//    não muda nada.
//
// Rodar:  node apps/seele-app/bancada/vista-da-entrada-pela-identidade.cjs [--webkit]

const assert = require("node:assert/strict");
const { servir } = require("./telas.cjs");
const pw = require("./playwright.cjs");

(async () => {
  const webkit = process.argv.includes("--webkit");
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const motor = webkit ? require(process.env.PLAYWRIGHT ?? "playwright").webkit : pw.chromium();
  const browser = await motor.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript(() => {
      try { localStorage.setItem("seele.casca", "larga"); } catch {}
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(() => typeof PONTE_NO_LUGAR_DO_TAURI !== "undefined" && PONTE_NO_LUGAR_DO_TAURI);

    await page.evaluate(async () => {
      window.anotado = [];
      window.falsa = (nome) => {
        const sala = {
          server: nome, me: 1, nickname: "Alex", voice_rooms: [], presentes: [],
          channels: [{ id: 1, name: "geral", open: true }], messages_revision: 1,
          may_manage_voice_rooms: false, may_kick: false, may_ban: false, may_remove_message: false,
          may_move_person: false, may_customise_server: false, may_delete_rooms: false, tela: null, transmissoes: [], ended: null,
        };
        const s = {
          alvo: { endereco: `${nome.toLowerCase()}.exemplo:8383`, impressao: nome, conhecido: true },
          acabou: false,
          retrato: () => ({ sala, contados: {}, telemetria: { rtt_ms: 10, jitter_ms: 1, loss_fraction: 0, signal: 90, sync_band: "Nominal" } }),
          enlace: () => "Online",
          atender: (comando, args) => {
            anotado.push({ sessao: nome, comando });
            if (comando === "messages") return { resposta: [] };
            return { resposta: null };
          },
          encerrar: async () => { s.acabou = true; },
        };
        s.snapshot = () => retratoDaSessao(s);
        return s;
      };
      // S, aberta e na vista da janela, como depois de uma entrada pela casca.
      sessaoDaPonte.atual = falsa("S");
      const entradaS = await invoke("ver_sessao", { sessao: 1 });
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(entradaS.snapshot);
    });
    assert.equal(await page.evaluate(() => vistaDaJanela.sessao), 1, "S não está na vista da janela");

    // A entrada em T pela identidade: a ponte a abre por dentro (a vista DELA
    // vai a T) e entrega a resposta do `connect` dela ao `depoisDeConectar`.
    const depois = await page.evaluate(() => {
      const epocaAntes = epocaDaVista;
      const t = falsa("T");
      sessaoDaPonte.atual = t;
      const resposta = {
        carimbo: carimboDaPonte(t), vista: 2, snapshot: t.snapshot(), veredito: "Known", geracao: 2,
        origem_da_impressao: "Lista", a_lista_guardou: false,
      };
      depoisDeConectar(resposta, t.alvo.endereco);
      return { vista: vistaDaJanela.sessao, subiu: epocaDaVista > epocaAntes };
    });
    // 1.
    assert.equal(depois.vista, 2, `a vista da janela ficou em S depois da entrada em T: ${JSON.stringify(depois)}`);
    assert.equal(depois.subiu, true, "a época da vista não subiu: uma resposta atrasada de S seria desenhada em T");

    // 2. O comando da tela vai a T.
    await page.evaluate(async () => {
      anotado.length = 0;
      await invoke("send_message", { channel: 1, text: "oi" }).catch(() => {});
    });
    let foram = await page.evaluate(() => anotado.filter((a) => a.comando === "send_message").map((a) => a.sessao));
    assert.deepEqual(foram, ["T"], `o comando da tela de T saiu para: ${JSON.stringify(foram)}`);

    // 3. De volta a S pela troca de vista.
    await page.evaluate(async () => {
      await verSessao(1);
      anotado.length = 0;
      await invoke("send_message", { channel: 1, text: "voltei" }).catch(() => {});
    });
    foram = await page.evaluate(() => anotado.filter((a) => a.comando === "send_message").map((a) => a.sessao));
    assert.deepEqual(foram, ["S"], `depois de ver S, o comando saiu para: ${JSON.stringify(foram)}`);

    // 4. A entrada que já está na vista (o caminho do desktop) não muda nada.
    const igual = await page.evaluate(() => {
      const antes = epocaDaVista;
      const mudou = porNaVistaAEntrada({ carimbo: { sessao: vistaDaJanela.sessao, geracao: 1 } });
      return { mudou, mesma: epocaDaVista === antes };
    });
    assert.deepEqual(igual, { mudou: false, mesma: true }, `a entrada já na vista mexeu na época: ${JSON.stringify(igual)}`);

    assert.deepEqual(erros.filter((e) => !/seele_web\.wasm|404/.test(e)), [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("vista da entrada pela identidade: T entra na vista da janela, a época sobe, o comando da tela vai a T, ver S volta a S, e a entrada já na vista não mexe em nada.");
})().catch((e) => { console.error(e); process.exit(1); });
