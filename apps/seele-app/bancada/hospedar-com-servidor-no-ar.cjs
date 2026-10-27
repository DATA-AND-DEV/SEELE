// **O HOSPEDAR AQUI com o servidor já no ar** (revisão visual do Codex sobre o
// E7). Com o HTML e os scripts reais; a ponte do Tauri é um duble.
//
// # O que ela mede
//
// 1. **Sem servidor no ar**, o rótulo é HOSPEDAR AQUI, sem descrição presa.
// 2. **Com ele no ar**, o rótulo é VER O SERVIDOR HOSPEDADO, ligado pelo
//    `aria-describedby` à frase que diz qual servidor; o clique NUNCA chama
//    `hospedar`, e entra no servidor pelo endereço desta máquina (`connect`).
// 3. **Com várias sessões e uma conversa já aberta nele**, o clique mostra
//    essa conversa (`ver_sessao` da sessão dele), sem `connect` e sem
//    `hospedar`; o VOLTAR AO MEU SERVIDOR faz o mesmo.
//
// Rodar:  node apps/seele-app/bancada/hospedar-com-servidor-no-ar.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const MEU = { nome: "Casa do Lago", alvo: "127.0.0.1:8383" };
const sessao = (n, nome, onde, extra = {}) => ({
  carimbo: { sessao: n, geracao: 1 }, endereco: onde === "Guardado" ? MEU.alvo : `198.51.100.${n}:8383`, nome,
  na_vista: false, foco_de_voz: false, na_sala_de_voz: null, onde, enlace: "online", encerrada: null,
  nao_lidas: 0, mudou: false, ...extra,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const abrir = async (tabelaExtra) => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
      const erros = [];
      page.on("pageerror", (e) => erros.push(e.message));
      await page.addInitScript((tabela) => {
        window.tabela = tabela;
        window.chamadas = [];
        window.__TAURI__ = {
          core: { invoke: async (cmd, args) => {
            chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            // `connect` fica pendente: o que se mede é que ele foi pedido.
            if (cmd === "connect") return new Promise(() => {});
            return tabela[cmd] ?? null;
          } },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      }, respostas(tabelaExtra));
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForTimeout(300);
      return { page, erros };
    };
    const lerBotao = (page) => page.evaluate(() => ({
      rotulo: $("botao-hospedar").textContent,
      estado: $("botao-hospedar").dataset.estado,
      descrito: $("botao-hospedar").getAttribute("aria-describedby"),
      frase: $("boot-meu-servidor-frase").textContent,
      bloco: !$("boot-meu-servidor").hidden,
    }));
    const pedidos = (page, nomes) => page.evaluate((n) => chamadas.filter((c) => n.includes(c.cmd)), nomes);

    // 1. Sem servidor no ar.
    let { page, erros } = await abrir({ meu_servidor: null });
    let b = await lerBotao(page);
    assert.equal(b.rotulo, "HOSPEDAR AQUI");
    assert.equal(b.descrito, null, "sem servidor no ar, o botão descreve uma frase escondida");
    assert.deepEqual(erros, []);
    await page.close();

    // 2. Com o servidor no ar, uma sessão só (sem o contrato de várias).
    ({ page, erros } = await abrir({ meu_servidor: MEU }));
    b = await lerBotao(page);
    assert.equal(b.rotulo, "VER O SERVIDOR HOSPEDADO", `o rótulo não diz o estado: ${b.rotulo}`);
    assert.equal(b.estado, "no-ar");
    assert.equal(b.descrito, "boot-meu-servidor-frase", "o botão não está ligado à frase do servidor no ar");
    assert.ok(b.bloco && /Casa do Lago/.test(b.frase), `a frase não diz qual servidor: ${b.frase}`);
    await page.evaluate(() => { chamadas.length = 0; });
    await page.click("#botao-hospedar");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "connect"));
    let feitos = await pedidos(page, ["hospedar", "connect"]);
    assert.ok(!feitos.some((c) => c.cmd === "hospedar"), "com o servidor no ar, o clique hospedou de novo");
    assert.equal(feitos.find((c) => c.cmd === "connect").args?.server, MEU.alvo, `entrou noutro lugar: ${JSON.stringify(feitos)}`);
    assert.deepEqual(erros, []);
    await page.close();

    // 3. Várias sessões, e uma conversa já aberta no servidor no ar.
    ({ page, erros } = await abrir({
      meu_servidor: MEU,
      sessoes: [sessao(1, "Casa do Lago", "Guardado"), sessao(2, "Oficina da Lia", "Outro")],
    }));
    await page.waitForFunction(() => Array.isArray(sessoesNaTela));
    for (const botao of ["#botao-hospedar", "#boot-voltar-ao-meu-servidor"]) {
      await page.evaluate(() => { chamadas.length = 0; });
      await page.click(botao);
      await page.waitForFunction(() => chamadas.some((c) => c.cmd === "ver_sessao"));
      feitos = await pedidos(page, ["hospedar", "connect", "ver_sessao"]);
      assert.deepEqual(feitos.map((c) => c.cmd), ["ver_sessao"], `${botao}: ${JSON.stringify(feitos)}`);
      assert.equal(feitos[0].args?.carimbo?.sessao ?? feitos[0].args?.sessao, 1, `${botao} mostrou outra sessão: ${JSON.stringify(feitos)}`);
      await page.evaluate(() => { $("tela-boot").hidden = false; });
    }
    // 4. Na entrada, SAIR de uma conversa a encerra pelo carimbo dela, e só ela;
    //    a do servidor hospedado diz que ele continua no ar.
    await page.evaluate(() => { for (const t of document.querySelectorAll("section.tela")) t.hidden = true; $("tela-boot").hidden = false; desenharSessoesNaEntrada(); chamadas.length = 0; });
    const sairs = await page.evaluate(() => [...document.querySelectorAll("#boot-sessoes-lista li")].map((li) => li.querySelectorAll("button")[1]?.getAttribute("aria-label")));
    assert.match(sairs[0] ?? "", /continua no ar/, `o SAIR do hospedado não diz que ele continua no ar: ${sairs}`);
    await page.evaluate(() => document.querySelectorAll("#boot-sessoes-lista li")[1].querySelectorAll("button")[1].click());
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "encerrar_sessao"));
    feitos = await pedidos(page, ["encerrar_sessao", "disconnect", "parar_de_hospedar"]);
    assert.deepEqual(feitos.map((c) => [c.cmd, c.args?.carimbo?.sessao]), [["encerrar_sessao", 2]], `SAIR fez outra coisa: ${JSON.stringify(feitos)}`);
    assert.deepEqual(erros, []);
    await page.close();
  } finally {
    await browser.close();
    server.close();
  }
  console.log("hospedar com o servidor no ar: o rótulo é VER O SERVIDOR HOSPEDADO, ligado à frase; o clique entra nele (ou mostra a conversa já aberta) e nunca hospeda de novo.");
})().catch((e) => { console.error(e); process.exit(1); });
