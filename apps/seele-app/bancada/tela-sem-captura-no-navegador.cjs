// **O navegador que não compartilha a tela diz isso antes de qualquer aperto**
// (caso 12 do browser-signaling, medido no WebKit; igual na 0.16).
//
// A transmissão do navegador lê os quadros da captura pelo
// `MediaStreamTrackProcessor`, que o WebKit (Safari) não tem. A sonda olhava só
// o `getDisplayMedia` e o `VideoEncoder`, dizia «sim», e a transmissão quebrava
// no meio. Esta bancada roda a casca do NAVEGADOR (a `ponte-web.js` de verdade,
// sem duble do Tauri) no Chromium, com o `MediaStreamTrackProcessor` tirado da
// página — o que o WebKit é —, e mede:
//
// 1. a sonda diz «não», e o `compartilhar_tela` recusa ANTES do seletor, com a
//    frase do navegador (e o `getDisplayMedia` nunca é chamado);
// 2. a caixa de compartilhar diz «ESTE NAVEGADOR NÃO COMPARTILHA A TELA», e não
//    «esta versão»;
// 3. o COMPARTILHAR do rodapé fica à vista, com `aria-disabled` e o porquê no
//    `title`; o palco diz o mesmo.
// 4. Com o `MediaStreamTrackProcessor` (o Chromium como ele é), a mesma página
//    não marca nada como indisponível.
//
// Rodar:  node apps/seele-app/bancada/tela-sem-captura-no-navegador.cjs

const assert = require("node:assert/strict");
const { servir } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const abrir = async ({ semProcessador }) => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
      const erros = [];
      page.on("pageerror", (e) => erros.push(e.message));
      await page.addInitScript((sem) => {
        if (sem) delete globalThis.MediaStreamTrackProcessor;
        window.__pediuCaptura = 0;
        if (navigator.mediaDevices) {
          navigator.mediaDevices.getDisplayMedia = async () => { window.__pediuCaptura += 1; throw new Error("não devia chegar aqui"); };
        }
      }, semProcessador);
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForFunction(() => typeof PONTE_NO_LUGAR_DO_TAURI !== "undefined");
      return { page, erros };
    };

    // 1–3. Sem o processador (o WebKit).
    let { page, erros } = await abrir({ semProcessador: true });
    assert.equal(await page.evaluate(() => PONTE_NO_LUGAR_DO_TAURI), true, "a bancada não está na casca do navegador");
    assert.equal(await page.evaluate(() => temCapturaParaCodificar()), false, "sem o processador, a página diz que captura");
    assert.equal(await page.evaluate(() => sondarATela("avc1.42001f")), false, "a sonda diz que esta página compartilha");

    // A recusa vem antes do seletor (com uma sessão de mentira na ponte, só para
    // passar do `comASessao`).
    const recusa = await page.evaluate(async () => {
      const antes = sessaoDoPedido;
      sessaoDoPedido = { transmissao: { comecar() { throw new Error("começou"); } } };
      try {
        await ATENDIMENTOS_DA_PONTE.compartilhar_tela({ limites: {} });
        return null;
      } catch (falha) {
        return falha;
      } finally {
        sessaoDoPedido = antes;
      }
    });
    assert.match(recusa?.RecusaDaPonte?.frase ?? "", /este navegador não compartilha a tela/, `a recusa: ${JSON.stringify(recusa)}`);
    assert.equal(await page.evaluate(() => window.__pediuCaptura), 0, "o seletor da captura abriu num navegador que não transmite");

    const tela = await page.evaluate(() => {
      permissaoDeTela = "NaoSeSabe";
      desenharPermissaoDeTela("NaoSeSabe");
      desenharORodapeDoOperador({ name: "Sala principal" });
      const botao = $("operador-vista");
      return {
        caixa: $("compartilhar-permissao").textContent,
        botao: { visivel: !botao.hidden, aria: botao.getAttribute("aria-disabled"), title: botao.title },
        palco: porQueNaoHaTela(),
      };
    });
    assert.match(tela.caixa, /ESTE NAVEGADOR NÃO COMPARTILHA A TELA/, `a caixa: ${tela.caixa}`);
    assert.doesNotMatch(tela.caixa, /ESTA VERSÃO/, `a caixa culpa a versão: ${tela.caixa}`);
    assert.equal(tela.botao.visivel, true, "o COMPARTILHAR sumiu em vez de dizer por quê");
    assert.equal(tela.botao.aria, "true", "o COMPARTILHAR não está marcado como indisponível");
    assert.match(tela.botao.title, /Este navegador não compartilha a tela/, `o porquê do botão: ${tela.botao.title}`);
    assert.match(tela.palco ?? "", /Este navegador não compartilha a tela/, `o palco: ${tela.palco}`);
    assert.deepEqual(erros.filter((e) => !/seele_web\.wasm|404/.test(e)), [], `erros na página: ${erros.join(" | ")}`);
    await page.close();

    // 4. Com o processador, nada fica indisponível.
    ({ page, erros } = await abrir({ semProcessador: false }));
    assert.equal(await page.evaluate(() => temCapturaParaCodificar()), true, "com o processador, a página diz que não captura");
    const comTela = await page.evaluate(() => {
      permissaoDeTela = "Concedida";
      desenharORodapeDoOperador({ name: "Sala principal" });
      return { aria: $("operador-vista").getAttribute("aria-disabled"), palco: porQueNaoHaTela() };
    });
    assert.equal(comTela.aria, "false", "com a captura, o COMPARTILHAR ficou indisponível");
    assert.equal(comTela.palco, null);
    await page.close();
  } finally {
    await browser.close();
    server.close();
  }
  console.log("tela sem captura no navegador: a sonda diz não, a recusa vem antes do seletor, a caixa e o botão dizem que é o navegador, e com a captura nada fica indisponível.");
})().catch((e) => { console.error(e); process.exit(1); });
