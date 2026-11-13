// **Com a prévia aberta, o nome do arquivo continua deitado.**
//
// O defeito (relatado pelo dono na 1.0.0, 13/11/2026, com captura): no cartão
// do anexo, ao apertar PRÉVIA a imagem abria AO LADO do nome, e a coluna do
// nome («colado-1788404894943.png») encolhia até um caractere de largura — o
// nome em pé, uma letra por linha, o SALVAR no meio e o «1.3 MB» quebrado.
//
// A causa: o cartão do desktop é uma grade de quatro colunas
// (`ic nome salvar previa`), e a figura não tinha área. Com a prévia aberta o
// botão PRÉVIA sai, a área dele fica vazia, e a figura — colocada sozinha pela
// grade — caía nela: uma coluna `auto` do tamanho da imagem, que tirava toda a
// largura da coluna do nome (`minmax(0, 1fr)`, com `overflow-wrap: anywhere`).
//
// O que esta bancada mede, no DOM desenhado, em três janelas e com imagens
// deitada, em pé e pequena, um nome comprido e uma recusa:
//
//   · o nome do relato fica numa linha só, com a largura do próprio texto;
//   · um nome comprido nunca fica mais estreito que 120 px;
//   · o tamanho fica numa linha só, e o SALVAR na fileira do nome;
//   · a figura (ou a frase da recusa) vem embaixo da fileira, na largura do
//     cartão, e a imagem deitada a ocupa;
//   · nada sai do cartão, e o cartão não sai da conversa.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/anexo-com-previa-aberta.cjs
//
// Com `SEELE_PREVIEW_DIR=<pasta>`, ela deixa lá uma captura por janela.

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

const agora = Math.floor(Date.now() / 1000);
const MB = 1048576;

/** Os anexos da cena: o id, o nome, e a imagem que a prévia devolve (`null` é recusa). */
const ANEXOS = [
  { id: 11, nome: "colado-1788404894943.png", bytes: Math.round(1.3 * MB), imagem: [1600, 900] },
  { id: 12, nome: "colado-1788404894944.png", bytes: Math.round(1.4 * MB), imagem: [900, 1600] },
  { id: 13, nome: "icone.png", bytes: 263 * 1024, imagem: [120, 80] },
  { id: 14, nome: "captura-da-tela-do-servidor-com-um-nome-bem-comprido-2026-11-13-as-10-42-07.png", bytes: 2 * MB, imagem: [1280, 720] },
  { id: 15, nome: "nao-e-imagem.png", bytes: 300 * 1024, imagem: null },
];

function conversa() {
  return ANEXOS.map((anexo, i) => ({
    id: i + 1, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 600 + i * 30, body: "", own: false, edited: false,
    attachment: { id: anexo.id, file_name: anexo.nome, declared_type: "image/png", byte_size: anexo.bytes, expired: false },
  }));
}

/** As caixas de um cartão com a prévia aberta. */
const medir = (id) => {
  const estado = document.querySelector(`[data-anexo-estado="${id}"]`);
  const cartao = estado.closest(".anexo");
  const caixa = (no) => {
    if (!no) return null;
    const r = no.getBoundingClientRect();
    return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height };
  };
  const nome = cartao.querySelector(".anexo-arquivo");
  const tamanho = cartao.querySelector(".anexo-tamanho");
  // A largura do texto numa linha só, medida num clone que não quebra.
  const regua = nome.cloneNode(true);
  regua.style.cssText = "position:absolute;visibility:hidden;white-space:nowrap;width:auto;max-width:none;";
  document.body.append(regua);
  const natural = regua.getBoundingClientRect().width;
  regua.remove();
  const linhasDe = (no) => Math.round(no.getBoundingClientRect().height / parseFloat(getComputedStyle(no).lineHeight));
  const r = cartao.getBoundingClientRect();
  const fora = [...cartao.querySelectorAll("*")].filter((no) => {
    const c = no.getBoundingClientRect();
    if (c.width === 0 || c.height === 0) return false;
    return c.left < r.left - 1 || c.right > r.right + 1 || c.top < r.top - 1 || c.bottom > r.bottom + 1;
  }).map((no) => no.className || no.localName);
  const estilo = getComputedStyle(cartao);
  return {
    cartao: caixa(cartao),
    miolo: r.width - parseFloat(estilo.paddingLeft) - parseFloat(estilo.paddingRight) - parseFloat(estilo.borderLeftWidth) - parseFloat(estilo.borderRightWidth),
    nome: caixa(nome), natural, linhasDoNome: linhasDe(nome),
    tamanho: caixa(tamanho), linhasDoTamanho: linhasDe(tamanho), textoDoTamanho: tamanho.textContent,
    salvar: caixa(cartao.querySelector(".anexo-salvar")),
    previa: cartao.querySelectorAll(".anexo-previa").length,
    figura: caixa(cartao.querySelector(".anexo-desenho")),
    imagem: caixa(cartao.querySelector(".anexo-desenho img")),
    recusa: caixa(cartao.querySelector(".anexo-recusa")),
    fechar: caixa(cartao.querySelector(".anexo-previa-fechar")),
    conversa: caixa(document.querySelector("#lista-mensagens")),
    fora,
  };
};

const arredondar = (visto) => JSON.stringify(visto, (_, v) => (typeof v === "number" ? Math.round(v * 10) / 10 : v));

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [largura, altura] of [[1600, 917], [900, 1600], [760, 560]]) {
      const page = await browser.newPage({ viewport: { width: largura, height: altura } });
      page.setDefaultTimeout(10000);
      const erros = [];
      page.on("pageerror", (erro) => erros.push(String(erro)));
      await page.addInitScript(
        ({ table, anexos }) => {
          window.testTable = table;
          // A prévia é um `data:` inteiro, como o Rust a monta: aqui, um PNG do
          // tamanho pedido, pintado numa tela desta página.
          const previa = (id) => {
            const anexo = anexos.find((a) => a.id === id);
            if (!anexo?.imagem) {
              return { attachment: id, image: null, claimed: "image/png", found: null, refusal: { kind: "DidNotArrive" } };
            }
            const tela = document.createElement("canvas");
            [tela.width, tela.height] = anexo.imagem;
            const tinta = tela.getContext("2d");
            const degrade = tinta.createLinearGradient(0, 0, tela.width, tela.height);
            degrade.addColorStop(0, "#23404f");
            degrade.addColorStop(1, "#f2521f");
            tinta.fillStyle = degrade;
            tinta.fillRect(0, 0, tela.width, tela.height);
            return { attachment: id, image: tela.toDataURL("image/png"), claimed: "image/png", found: "image/png", refusal: null };
          };
          window.__TAURI__ = {
            core: {
              invoke: async (cmd, args) => (cmd === "prever_anexo" ? previa(args.anexo)
                : Object.hasOwn(table, cmd) ? table[cmd] : null),
            },
            event: { listen: async () => () => {} },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        },
        {
          table: respostas({ messages: conversa(), snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }),
          anexos: ANEXOS,
        },
      );
      await page.goto(`http://127.0.0.1:${server.address().port}/`);
      await page.evaluate((r) => {
        window.testTable.snapshot = r;
        $("tela-boot").hidden = true;
        $("tela-sessao").hidden = false;
        desenhar(r);
      }, retrato());
      await page.evaluate(() => sincronizarMensagens(99));
      await page.waitForFunction((n) => document.querySelectorAll("#lista-mensagens .anexo").length === n, ANEXOS.length);

      const onde = `${largura}×${altura}`;
      // Fechado, o cartão do relato: é contra ele que «a mesma fileira» se mede.
      const fechado = await page.evaluate(medir, 11);
      assert.equal(fechado.previa, 1, `${onde}: a cena não ofereceu a PRÉVIA: ${arredondar(fechado)}`);

      // PRÉVIA em todos, pelo clique de verdade, e a espera da figura.
      for (const anexo of ANEXOS) {
        await page.locator(`button[data-anexo-previa="${anexo.id}"]`).click();
        await page.waitForFunction((id) => {
          const cartao = document.querySelector(`[data-anexo-estado="${id}"]`).closest(".anexo");
          const imagem = cartao.querySelector(".anexo-desenho img");
          return imagem ? imagem.complete && imagem.naturalWidth > 0 : Boolean(cartao.querySelector(".anexo-recusa"));
        }, anexo.id);
      }
      // O redesenho da conversa (o que toda mensagem nova faz) monta o cartão
      // pelo outro caminho, o de `blocoDeAnexo`: a medida vale para os dois.
      await page.evaluate(() => desenharMensagens());
      if (process.env.SEELE_PREVIEW_DIR) {
        await page.evaluate(() => document.querySelector(`[data-anexo-estado="11"]`).closest("li").scrollIntoView({ block: "start" }));
        await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `anexo-com-previa-${largura}x${altura}.png`) });
      }

      for (const anexo of ANEXOS) {
        const visto = await page.evaluate(medir, anexo.id);
        const qual = `${onde}, «${anexo.nome.slice(0, 28)}»`;
        const dito = arredondar(visto);
        const embaixo = visto.figura ?? visto.recusa;
        assert.ok(embaixo, `${qual}: a prévia não abriu: ${dito}`);

        // O nome: deitado. O do relato cabe inteiro numa linha; o comprido
        // pode quebrar ou cortar, e nunca letra a letra.
        assert.ok(visto.nome.width >= 120 || visto.nome.width >= visto.natural - 1,
          `${qual}: com a prévia aberta o nome do arquivo ficou com ${Math.round(visto.nome.width)} px de largura (o texto pede ${Math.round(visto.natural)}): ${dito}`);
        if (visto.natural <= 300) {
          assert.equal(visto.linhasDoNome, 1, `${qual}: com a prévia aberta o nome do arquivo quebrou em ${visto.linhasDoNome} linhas: ${dito}`);
        }
        assert.equal(visto.linhasDoTamanho, 1, `${qual}: o tamanho («${visto.textoDoTamanho}») quebrou em ${visto.linhasDoTamanho} linhas: ${dito}`);

        // A fileira: o SALVAR ao lado do nome, e a figura embaixo dos dois.
        const fundoDaFileira = Math.max(visto.nome.bottom, visto.tamanho.bottom, visto.salvar.bottom);
        assert.ok(visto.salvar.left >= visto.nome.right - 1, `${qual}: o SALVAR não está à direita do nome: ${dito}`);
        assert.ok(visto.salvar.top < visto.tamanho.bottom && visto.salvar.bottom > visto.nome.top,
          `${qual}: o SALVAR saiu da fileira do nome: ${dito}`);
        assert.ok(embaixo.top >= fundoDaFileira - 1, `${qual}: a prévia não está embaixo da fileira do nome, do tamanho e do SALVAR: ${dito}`);
        assert.ok(Math.abs(embaixo.width - visto.miolo) <= 1, `${qual}: a prévia não ocupa a largura do cartão (${Math.round(embaixo.width)} de ${Math.round(visto.miolo)} px): ${dito}`);
        if (visto.imagem && anexo.imagem[0] >= 1280) {
          assert.ok(visto.imagem.width >= visto.miolo - 3, `${qual}: a imagem deitada não ocupa a largura do cartão (${Math.round(visto.imagem.width)} de ${Math.round(visto.miolo)} px): ${dito}`);
        }
        if (visto.imagem) assert.ok(visto.imagem.height <= 322, `${qual}: a imagem passou do teto de altura: ${dito}`);

        // Nada sai do cartão, e o cartão não sai da conversa.
        assert.deepEqual(visto.fora, [], `${qual}: há partes do cartão para fora dele: ${dito}`);
        assert.ok(visto.cartao.left >= visto.conversa.left - 1 && visto.cartao.right <= visto.conversa.right + 1,
          `${qual}: o cartão sai da conversa: ${dito}`);
      }

      // O cartão do relato, em números, para o relatório.
      const aberto = await page.evaluate(medir, 11);
      assert.ok(Math.abs(aberto.nome.width - fechado.nome.width) <= 1 || aberto.nome.width >= aberto.natural - 1,
        `${onde}: o nome ficou mais estreito com a prévia aberta (${Math.round(fechado.nome.width)} → ${Math.round(aberto.nome.width)} px)`);
      assert.deepEqual(erros, [], `${onde}: a página lançou: ${erros.join(" | ")}`);
      console.log(`${onde}: cartão de ${Math.round(aberto.cartao.width)} px, nome em ${aberto.linhasDoNome} linha de ${Math.round(aberto.nome.width)} px, «${aberto.textoDoTamanho}» em ${aberto.linhasDoTamanho}, imagem de ${Math.round(aberto.imagem.width)}×${Math.round(aberto.imagem.height)} embaixo`);
      await page.close();
    }
    console.log("anexo com prévia aberta: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
