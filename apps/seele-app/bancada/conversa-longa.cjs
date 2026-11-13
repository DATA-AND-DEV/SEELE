// **Numa conversa longa, nenhuma linha passa por cima da seguinte.**
//
// O defeito (relatado com uma captura da 0.19.6, em 12/11/2026): com histórico
// bastante para a conversa rolar, os anexos, o vídeo e um link de várias linhas
// saíam por cima da mensagem de baixo — em qualquer tamanho de janela, e não só
// na mínima. A lista é uma coluna flexível que rola; cada linha tinha
// `min-height: 24px` e podia encolher, e o navegador espremia todas até 24px
// antes de começar a rolar. Quem segura é o `.mensagens > li { flex-shrink: 0 }`
// de `tela-sessao.css`; sem ele esta bancada reprova.
//
// A `desktop-estreito.cjs` prende o mesmo conserto na janela mínima, com texto.
// Esta prende a cena do relato: imagens coladas em sequência, um vídeo e um
// link comprido, em janelas comuns.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/conversa-longa.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

const agora = Math.floor(Date.now() / 1000);
const DIA = 86400;
// Um link de loja, do tamanho do do relato: seis linhas a 1600 px.
const LINK =
  "https://loja.exemplo/maleta-007-executiva-luxo-tipo-couro-com-segredo-numerico/up/MLBU3536428472" +
  "?gallery_type=horizontal&sizeForPhoto=416&pdp_filters=official_store%3A445322#polycard_client%3D" +
  "recommendations_recoview-selleritems-eshops%26reco_backend%3Drecoview-selleritems-eshops%26reco_item_pos" +
  "%3D2%26reco_id%3D1f7e0d&tracking_id%3D17fe400d89405e4229fb6ca07f2879c0%26source%3Deshops%26seller_id" +
  "%3D2934767674%26category_id%3DMLB28144";

/** Doze dias da mesma conversa: 120 mensagens, que não cabem em janela nenhuma. */
function conversa() {
  let id = 0;
  const m = (quando, autor, nome, body, attachment = null) =>
    ({ id: ++id, channel: 1, author: autor, author_nickname: nome, at_seconds: quando, body, own: autor === 1, edited: false, attachment });
  const png = (n, kb) => ({ id: 100 + n, file_name: `colado-17884044${n}.png`, declared_type: "image/png", byte_size: kb * 1024, expired: false });
  const mensagens = [];
  for (let d = 12; d >= 1; d--) {
    const t = agora - d * DIA;
    mensagens.push(m(t, 1, "Alex", "oi"), m(t + 60, 1, "Alex", "https://seele.exemplo/"));
    mensagens.push(m(t + 200, 2, "Lia", "", png(d * 3, 263)), m(t + 260, 2, "Lia", "", png(d * 3 + 1, 1300)), m(t + 300, 2, "Lia", "Alo?"));
    mensagens.push(
      m(t + 900, 2, "Lia", "me ouves?"),
      m(t + 960, 2, "Lia", "", { id: 500 + d, file_name: "karaoke_1080p_1788756078434.mp4", declared_type: "video/mp4", byte_size: 140 * 1048576, expired: false }),
    );
    mensagens.push(m(t + 1500, 1, "Alex", LINK), m(t + 1600, 2, "Lia", "", png(d * 3 + 2, 1400)), m(t + 1700, 1, "Alex", "https://codigo.exemplo/ax"));
  }
  return mensagens;
}

/** Quantas linhas têm conteúdo à vista passando da própria caixa, e quantas chegam à de baixo. */
const medir = () => {
  const lista = document.querySelector(".mensagens");
  const itens = [...lista.children];
  let sobrepostas = 0, espremidas = 0, pior = 0;
  for (let i = 0; i < itens.length; i++) {
    const li = itens[i], caixa = li.getBoundingClientRect();
    // Só o que a pessoa vê e que está no fluxo da linha: nada escondido, nada
    // posicionado por fora (o menu de ações de uma mensagem, por exemplo).
    const aVista = [...li.querySelectorAll("*")].filter((e) => {
      const estilo = getComputedStyle(e), r = e.getBoundingClientRect();
      if (["absolute", "fixed"].includes(estilo.position) || estilo.visibility === "hidden" || estilo.display === "none") return false;
      if (r.width === 0 || r.height === 0 || e.closest("[hidden]")) return false;
      for (let p = e.parentElement; p && p !== li; p = p.parentElement) {
        if (["absolute", "fixed"].includes(getComputedStyle(p).position)) return false;
      }
      return true;
    });
    const fundo = Math.max(caixa.bottom, ...aVista.map((e) => e.getBoundingClientRect().bottom));
    if (fundo > caixa.bottom + 1) { espremidas++; pior = Math.max(pior, fundo - caixa.bottom); }
    if (i + 1 < itens.length && fundo > itens[i + 1].getBoundingClientRect().top + 1) sobrepostas++;
  }
  return {
    linhas: itens.length, sobrepostas, espremidas, piorPx: Math.round(pior),
    rola: lista.scrollHeight > lista.clientHeight + 1,
    anexos: lista.querySelectorAll(".anexo").length,
  };
};

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [largura, altura] of [[1600, 919], [1280, 720], [900, 600]]) {
      const page = await browser.newPage({ viewport: { width: largura, height: altura } });
      const erros = [];
      page.on("pageerror", (erro) => erros.push(String(erro)));
      await page.addInitScript(
        (table) => {
          window.testTable = table;
          window.__TAURI__ = {
            core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
            event: { listen: async () => () => {} },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        },
        respostas({ messages: conversa(), snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }),
      );
      await page.goto(`http://127.0.0.1:${server.address().port}/`);
      await page.evaluate((r) => {
        window.testTable.snapshot = r;
        $("tela-boot").hidden = true;
        $("tela-sessao").hidden = false;
        desenhar(r);
      }, retrato());
      await page.evaluate(() => sincronizarMensagens(99));
      await page.waitForFunction(() => document.querySelectorAll(".mensagens > li").length >= 120);

      const visto = await page.evaluate(medir);
      const onde = `${largura}×${altura}`;
      // A cena tem de ser a do relato, ou a bancada passaria por não ter montado nada.
      assert.ok(visto.rola, `${onde}: a conversa não chegou a rolar, e sem rolar o defeito não aparece: ${JSON.stringify(visto)}`);
      assert.ok(visto.anexos >= 40, `${onde}: a cena não desenhou os anexos: ${JSON.stringify(visto)}`);
      assert.equal(visto.espremidas, 0, `${onde}: há linhas com o conteúdo passando da própria caixa: ${JSON.stringify(visto)}`);
      assert.equal(visto.sobrepostas, 0, `${onde}: há linhas por cima da seguinte: ${JSON.stringify(visto)}`);
      assert.deepEqual(erros, [], `${onde}: a página lançou: ${erros.join(" | ")}`);
      console.log(`${onde}: ${visto.linhas} linhas, ${visto.anexos} anexos, nenhuma por cima da outra`);
      await page.close();
    }
    console.log("conversa longa: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
