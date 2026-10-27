// **O aviso antes de atualizar para além da quebra** (E4 da 0.17). Com o HTML,
// o CSS e os scripts reais; só a ponte nativa é simulada, com a forma do
// contrato do compat (`procurar_atualizacao` → `fronteira`, `pede_aviso`,
// `adiada`; `escolher_atualizacao`; `instalar_atualizacao { confirmada }`).
//
// # O que ela mede
//
// 1. **Da 0.16 para a 0.17** (`pede_aviso: true`): INSTALAR abre o aviso e não
//    instala nada; o aviso é um `alertdialog` com o foco no CANCELAR e diz a
//    WebRTC, a 0.16 que não conversa, e a 0.16 guardada ao lado.
// 2. **CANCELAR** (e o Escape) diz `cancelar` ao Rust e não instala.
// 3. **ADIAR** diz `adiar` e não instala; a procura seguinte, com `adiada`,
//    diz que já foi adiada.
// 4. **PROSSEGUIR** instala com `confirmada` igual à versão achada.
// 5. **Da 0.17.0 para a 0.17.1** (`pede_aviso: false`): sem aviso nenhum, e o
//    pedido de sempre, sem `confirmada`.
//
// Rodar:  node apps/seele-app/bancada/aviso-antes-de-atualizar.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const CRUZA = {
  instalada: "0.16.0", versao: "0.17.0", notas: "WebRTC.",
  fronteira: { tipo: "cruza", de: "1", para: "2" }, pede_aviso: true, adiada: false,
};
const MESMA = {
  instalada: "0.17.0", versao: "0.17.1", notas: null,
  fronteira: { tipo: "mesma", linha: "2" }, pede_aviso: false, adiada: false,
};

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          const r = tabela[cmd];
          return typeof r === "function" ? r(args) : (r ?? null);
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ microfones: [], saidas: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    const procurar = async (achado) => {
      await page.evaluate(async (a) => {
        tabela.procurar_atualizacao = a;
        chamadas.length = 0;
        await abrirServer("tela-boot");
        abrirSecao("secao-atualizacao");
        await procurarAtualizacao();
      }, achado);
    };
    const de = (cmd) => page.evaluate((c) => chamadas.filter((x) => x.cmd === c), cmd);
    const aberto = () => page.evaluate(() => !$("travessia").hidden);

    // 1. O aviso abre, e nada é instalado.
    await procurar(CRUZA);
    assert.match(await page.evaluate(() => $("atualizacao-travessia").textContent), /Servidor 1\.0 para o Servidor 2\.0/,
      "o painel não diz, antes do botão, que a versão do servidor muda");
    await page.click("#atualizacao-instalar");
    assert.equal(await aberto(), true, "INSTALAR numa versão que cruza a quebra não abriu o aviso");
    assert.equal((await de("instalar_atualizacao")).length, 0, "o aviso abriu e mesmo assim instalou");
    const aviso = await page.evaluate(() => ({
      papel: $("travessia").getAttribute("role"),
      foco: document.activeElement?.id,
      texto: $("travessia").textContent,
    }));
    assert.equal(aviso.papel, "alertdialog", "o aviso não é um alertdialog");
    assert.equal(aviso.foco, "travessia-cancelar", `o foco não nasceu no CANCELAR: ${aviso.foco}`);
    for (const precisa of [/WebRTC/, /não conversa com a 0\.16\.0/, /0\.16\.0 continua disponível/, /CANCELAR/, /ADIAR/, /INSTALAR A 0\.17\.0/]) {
      assert.match(aviso.texto, precisa, `o aviso não diz ${precisa}: ${aviso.texto}`);
    }

    // 2. CANCELAR pelo Escape.
    await page.keyboard.press("Escape");
    assert.equal(await aberto(), false, "o Escape não fechou o aviso");
    assert.deepEqual((await de("escolher_atualizacao")).map((c) => c.args), [{ escolha: "cancelar" }],
      "o Escape não disse «cancelar» ao Rust");
    assert.equal((await de("instalar_atualizacao")).length, 0, "cancelar instalou");
    assert.match(await page.evaluate(() => $("atualizacao-estado").textContent), /CANCELADA/, "cancelar não diz que cancelou");

    // 3. ADIAR, e a procura seguinte diz que foi adiada.
    await procurar(CRUZA);
    await page.click("#atualizacao-instalar");
    await page.click("#travessia-adiar");
    assert.deepEqual((await de("escolher_atualizacao")).map((c) => c.args), [{ escolha: "adiar" }], "ADIAR não disse «adiar»");
    assert.equal((await de("instalar_atualizacao")).length, 0, "adiar instalou");
    await procurar({ ...CRUZA, adiada: true });
    assert.match(await page.evaluate(() => $("atualizacao-travessia").textContent), /já adiou/, "a procura depois de adiar não diz que foi adiada");
    await page.click("#atualizacao-instalar");
    assert.equal(await page.evaluate(() => $("travessia-adiada").hidden), false, "o aviso de uma adiada não diz que foi adiada");

    // 4. PROSSEGUIR instala com a versão confirmada.
    await page.evaluate(() => { chamadas.length = 0; });
    await page.click("#travessia-prosseguir");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "instalar_atualizacao"));
    assert.deepEqual((await de("instalar_atualizacao")).map((c) => c.args), [{ confirmada: "0.17.0" }],
      "PROSSEGUIR não mandou a versão confirmada");
    assert.equal(await aberto(), false, "o aviso ficou aberto depois de prosseguir");

    // 5. Na mesma versão do servidor, sem aviso, e o pedido de sempre. Depois de
    //    um INSTALAR que deu certo o app reabre; aqui a ponte de mentira volta,
    //    e os botões são religados como num painel novo.
    await page.evaluate(() => { $("atualizacao-instalar").disabled = false; $("atualizacao-procurar").disabled = false; });
    await procurar(MESMA);
    assert.equal(await page.evaluate(() => $("atualizacao-travessia").hidden), true, "uma atualização compatível fala em mudar a versão do servidor");
    await page.click("#atualizacao-instalar");
    assert.equal(await aberto(), false, "uma atualização compatível abriu o aviso");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "instalar_atualizacao"));
    assert.deepEqual((await de("instalar_atualizacao")).map((c) => c.args), [undefined],
      "uma atualização compatível mandou um campo a mais");

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("aviso antes de atualizar: abre só na quebra, com o foco no CANCELAR; cancelar (Esc) e adiar não instalam; prosseguir confirma a versão; a compatível segue sem aviso.");
})().catch((e) => { console.error(e); process.exit(1); });
