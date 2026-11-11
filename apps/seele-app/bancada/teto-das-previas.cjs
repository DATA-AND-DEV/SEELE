// As prévias de imagem da conversa têm teto: o que a janela guarda não cresce
// sem fim, e a imagem grande demais é dita, e não some.
//
// # Por que ela existe
//
// A varredura do incidente da capa em 4K (10/11/2026) deixou este irmão sem
// conserto. A prévia de um anexo e a de um link iam à janela do tamanho que
// vieram, e os dois mapas que as guardam (`previas`, `previasDeLink`, em
// `tela-sessao.js`) só cresciam até o fim da sessão: cada prévia aberta ficava
// na memória da janela para sempre, como um `data:` de megabytes.
//
// O teto de pixels e a redução são do Rust, e têm dono lá
// (`seele-sessao --all-features reducao::`, `seele-ffi preview_of`, e os dois
// produtores em `dois_produtores_da_previa.rs`). **Esta bancada é da janela**:
// `prever_anexo` e `previa_de_link` respondem daqui o que o Rust responderia.
//
// # O que ela protege, dito em voz alta
//
// 1. **O bolso dos anexos tem teto de imagens.** A que passa do teto despeja a
//    mais antiga, que volta a ser o botão PRÉVIA **dizendo que foi recolhida**
//    — e reabri-la baixa de novo, porque a janela já não a tem.
// 2. **E tem teto de texto.** Poucas prévias enormes não cabem pelo número, e
//    saem pelo tamanho somado dos `data:`.
// 3. **A recusa por pixels é dita no bloco, com as medidas e o máximo**, e a
//    imagem que não abre tem a frase dela. Recusa não ocupa o bolso.
// 4. **A imagem de link grande demais é dita no lugar dela**, e não vira só um
//    link sem explicação — e não é buscada de novo a cada desenho.
// 5. **O bolso dos links tem o mesmo teto, e despejar não busca de novo
//    sozinho.** A recolhida vira o convite com o domínio; quem manda buscar é
//    o clique. Apagar a entrada faria cada desenho buscar a que saiu e
//    despejar outra: uma batida sem fim na porta de quem controla o endereço.
//
//   node apps/seele-app/bancada/teto-das-previas.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");

// O GIF de `gif-na-conversa.cjs`: três quadros, 64×48, escrito e não gerado.
const GIF =
  "R0lGODlhQAAwAIEAAP8AAAAAAAAAAAAAACH/C05FVFNDQVBFMi4wAwEAAAAh+QQADAAAACwAAAAAQAAw" +
  "AAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJsqXLlzBj" +
  "ypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAAh+QQBDAABACwAAAAAQAAwAIEA/wAAAAAA" +
  "AAAAAAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJsqXL" +
  "lzBjypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAAh+QQBDAABACwAAAAAQAAwAIEAAP8A" +
  "AAAAAAAAAAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJ" +
  "sqXLlzBjypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAA7";
const URI = `data:image/gif;base64,${GIF}`;

const REGRAS = {
  limit: 4 * 1024 * 1024,
  types: ["image/png", "image/jpeg", "image/gif", "image/webp"],
};
const agora = Math.floor(Date.now() / 1000);

/** Uma mensagem com um anexo de imagem. */
function comAnexo(id, anexo) {
  return {
    id, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 4000 + id,
    body: `anexo ${anexo}`, own: false, edited: false,
    attachment: { id: anexo, file_name: `foto-${anexo}.gif`, declared_type: "image/gif", byte_size: 402, expired: false },
  };
}

/** Uma mensagem com um link direto de imagem. */
function comLink(id, url) {
  return {
    id, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 4000 + id,
    body: `olha ${url}`, own: false, edited: false, attachment: null,
  };
}

/**
 * Abre a sessão com `mensagens`, e uma ponte que responde `prever_anexo` e
 * `previa_de_link` pelo que `roteiro` disser, anotando cada pedido.
 *
 * `roteiro` roda **na página**: `{ anexos: { [id]: resposta }, links: { [url]: falha } }`,
 * e `pesada` é o número de anexos (a partir de `pesadaDesde`) que respondem
 * um `data:` de `pesoDaPesada` caracteres.
 */
async function abrir(navegador, url, mensagens, roteiro = {}) {
  const aba = await navegador.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = [];
  aba.on("pageerror", (e) => erros.push("pageerror: " + String(e.message).slice(0, 200)));
  aba.on("console", (m) => {
    if (m.type() === "error") erros.push("console: " + m.text().slice(0, 200));
  });
  const r = retrato();
  const tabela = respostas({
    messages: mensagens,
    regras_de_previa: REGRAS,
    dominios_de_gif: [],
    snapshot: r,
  });
  await aba.addInitScript(({ t, uri, roteiro }) => {
    window.__anexos = [];
    window.__links = [];
    const pesada = roteiro.pesoDaPesada ? uri + "A".repeat(roteiro.pesoDaPesada - uri.length) : null;
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          if (cmd === "prever_anexo") {
            window.__anexos.push(args.anexo);
            const escrita = roteiro.anexos?.[args.anexo];
            if (escrita) return { attachment: args.anexo, image: null, claimed: "image/gif", found: "image/gif", ...escrita };
            return { attachment: args.anexo, image: pesada ?? uri, claimed: "image/gif", found: "image/gif", refusal: null };
          }
          if (cmd === "previa_de_link") {
            window.__links.push(args.url);
            // **A bancada falha, e não pendura.** Um despejo que busca de novo
            // sozinho gira sem fim dentro das microtarefas (a resposta chega,
            // a conversa redesenha, o desenho busca a que saiu), e a página
            // nunca mais responde: medido, esta bancada ficava de pé para
            // sempre. Com o teto de buscas a roda para, e a contagem a denuncia.
            if (window.__links.length > 400) throw "NaoEImagem";
            const falha = roteiro.links?.[args.url];
            if (falha) throw falha;
            return uri;
          }
          return Object.hasOwn(t, cmd) ? t[cmd] : null;
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
    };
  }, { t: tabela, uri: URI, roteiro });
  await aba.goto(url);
  await aba.waitForTimeout(700);
  await aba.evaluate(() => {
    for (const secao of document.querySelectorAll("section.tela")) secao.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
  });
  await aba.evaluate((x) => window.desenhar(x), r);
  await aba.waitForTimeout(300);
  await aba.evaluate("sincronizarMensagens(99)");
  await aba.waitForTimeout(600);
  return { aba, erros };
}

/** Aperta o botão PRÉVIA de um anexo e espera a resposta ser desenhada. */
async function verPrevia(aba, anexo) {
  await aba.evaluate((id) => document.querySelector(`[data-anexo-previa="${id}"]`).click(), anexo);
  await aba.waitForFunction(
    (id) => !document.querySelector(`[data-anexo-previa="${id}"][disabled]`) && previas.has(id),
    anexo,
  );
}

// ----------------------------------------- 1 e 3. o teto de imagens, e a recusa
async function oBolsoDosAnexosTemTetoDeImagens(navegador, url) {
  const teto = 48;
  const ids = Array.from({ length: teto + 1 }, (_, i) => 101 + i);
  const PIXELS = 900;
  const ILEGIVEL = 901;
  const mensagens = [...ids, PIXELS, ILEGIVEL].map((anexo, i) => comAnexo(i + 1, anexo));
  const { aba, erros } = await abrir(navegador, url, mensagens, {
    anexos: {
      [PIXELS]: { refusal: { kind: "TooManyPixels", width: 9000, height: 5000, limit: 36000000 } },
      [ILEGIVEL]: { refusal: { kind: "Unreadable" } },
    },
  });

  const declarado = await aba.evaluate(() => ({ ...TETO_DAS_PREVIAS, doBolso: previas.teto === TETO_DAS_PREVIAS }));
  assert.equal(declarado.imagens, teto, "o teto de imagens mudou, e esta bancada mede o de 48");
  assert.ok(declarado.doBolso && declarado.texto > 0, "o bolso dos anexos não usa o teto declarado");

  // 3. As duas recusas novas, ditas no bloco — antes das imagens, para provar
  //    que recusa não ocupa o bolso.
  await verPrevia(aba, PIXELS);
  await verPrevia(aba, ILEGIVEL);
  const recusas = await aba.evaluate(() =>
    [...document.querySelectorAll(".anexo-recusa")].map((r) => r.textContent));
  assert.equal(recusas.length, 2, `as duas recusas não foram escritas no bloco: ${JSON.stringify(recusas)}`);
  assert.ok(
    recusas[0].includes("PIXELS DEMAIS") && recusas[0].includes("9000 × 5000") && recusas[0].includes("36 MEGAPIXELS"),
    `a recusa por pixels não diz as medidas e o máximo: «${recusas[0]}»`);
  assert.ok(recusas[1].includes("NÃO ABRIU"), `a imagem ilegível não tem a frase dela: «${recusas[1]}»`);
  assert.ok(recusas.every((r) => !r.includes("FALHA NÃO IDENTIFICADA") && !r.includes("undefined")),
    `uma recusa de prévia chegou à tela sem frase: ${JSON.stringify(recusas)}`);

  // 1. Até o teto, todas ficam.
  for (const anexo of ids.slice(0, teto)) await verPrevia(aba, anexo);
  const noTeto = await aba.evaluate(() => ({
    imagens: previas.imagens,
    desenhadas: document.querySelectorAll(".anexo-desenho img").length,
    recolhidas: document.querySelectorAll('[data-recolhida="sim"]').length,
  }));
  assert.deepEqual(noTeto, { imagens: teto, desenhadas: teto, recolhidas: 0 },
    "no teto exato, alguma prévia já tinha saído — ou as recusas ocuparam o bolso");

  // A que passa do teto despeja a mais antiga, e a conversa o diz.
  await verPrevia(aba, ids[teto]);
  await aba.waitForTimeout(200);
  const depois = await aba.evaluate((primeira) => {
    const botao = document.querySelector(`[data-anexo-previa="${primeira}"]`);
    return {
      imagens: previas.imagens,
      desenhadas: document.querySelectorAll(".anexo-desenho img").length,
      guardada: previas.has(primeira),
      botao: botao ? { recolhida: botao.dataset.recolhida, dica: botao.title, texto: botao.textContent } : null,
      anuncio: document.getElementById("anuncio").textContent,
    };
  }, ids[0]);
  assert.equal(depois.imagens, teto,
    `o bolso dos anexos passou do teto: ${depois.imagens} imagens guardadas, e ele só cresce até a sessão acabar`);
  assert.equal(depois.desenhadas, teto, "a prévia despejada continuou desenhada, segurando a memória dela");
  assert.equal(depois.guardada, false, "a mais antiga não foi a despejada");
  assert.ok(depois.botao, "a prévia despejada sumiu sem deixar o botão que a reabre");
  assert.equal(depois.botao.recolhida, "sim", "o botão da prévia despejada não diz que ela foi recolhida");
  assert.ok(/recolhida/.test(depois.botao.dica) && /baixa o arquivo de novo/.test(depois.botao.dica),
    `a dica do botão promete que reabrir é de graça, e não é: «${depois.botao.dica}»`);
  assert.ok(/recolhida/.test(depois.anuncio), `a prévia saiu da tela sem anúncio: «${depois.anuncio}»`);

  // Reabrir a recolhida baixa de novo (a janela já não a tem), e despeja outra.
  await verPrevia(aba, ids[0]);
  await aba.waitForTimeout(200);
  const reaberta = await aba.evaluate((primeira) => ({
    pedidos: window.__anexos.filter((a) => a === primeira).length,
    imagens: previas.imagens,
    desenhada: !!document.querySelector(`[data-anexo-fechar="${primeira}"]`),
  }), ids[0]);
  assert.deepEqual(reaberta, { pedidos: 2, imagens: teto, desenhada: true },
    "reabrir a prévia recolhida não a baixou de novo, não a desenhou, ou estourou o teto");

  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro no console");
  await aba.close();
}

// --------------------------------------------------------- 2. o teto de texto
async function oBolsoDosAnexosTemTetoDeTexto(navegador, url) {
  const ids = [201, 202, 203, 204, 205];
  // Um `data:` de 5,5 MiB: o que um GIF no teto de bytes da prévia pesa.
  const peso = Math.floor(5.5 * 1024 * 1024);
  const { aba, erros } = await abrir(navegador, url, ids.map((anexo, i) => comAnexo(i + 1, anexo)), { pesoDaPesada: peso });
  const teto = await aba.evaluate(() => TETO_DAS_PREVIAS.texto);
  assert.ok(peso * 4 <= teto && peso * 5 > teto,
    "o teto de texto mudou, e esta bancada mede o que quatro prévias enormes enchem e a quinta estoura");
  for (const anexo of ids) await verPrevia(aba, anexo);
  await aba.waitForTimeout(200);
  const depois = await aba.evaluate((primeira) => ({
    imagens: previas.imagens,
    texto: previas.texto,
    guardada: previas.has(primeira),
    recolhida: document.querySelector(`[data-anexo-previa="${primeira}"]`)?.dataset.recolhida ?? null,
  }), ids[0]);
  assert.ok(depois.texto <= teto,
    `o bolso guarda ${depois.texto} caracteres de data:, acima do teto de ${teto}: o número de imagens não o segurou`);
  assert.deepEqual({ imagens: depois.imagens, guardada: depois.guardada, recolhida: depois.recolhida },
    { imagens: 4, guardada: false, recolhida: "sim" },
    "a quinta prévia enorme não despejou a mais antiga, ou a despejou sem dizer");
  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro no console");
  await aba.close();
}

// ------------------------------------------------- 4 e 5. as imagens de link
async function oBolsoDosLinksTemTetoENaoBuscaDeNovo(navegador, url) {
  const teto = 48;
  const links = Array.from({ length: teto + 1 }, (_, i) => `https://exemplo${i}.br/foto-${i}.gif`);
  const GRANDE = "https://grande.exemplo.br/enorme.png";
  const mensagens = [comLink(1, GRANDE), ...links.map((link, i) => comLink(i + 2, link))];
  const { aba, erros } = await abrir(navegador, url, mensagens, {
    links: { [GRANDE]: { PixelsDemais: { largura: 9000, altura: 5000, teto: 36000000 } } },
  });
  await aba.waitForFunction((n) => window.__links.length >= n, teto + 2);
  await aba.waitForTimeout(600);

  // 4. A imagem de link grande demais é dita no lugar dela.
  const grande = await aba.evaluate(() =>
    [...document.querySelectorAll(".previa-convite-frase")].map((f) => f.textContent).find((t) => t.includes("PIXELS")) ?? null);
  assert.ok(grande, "a imagem de link recusada por pixels virou só um link, sem uma palavra na conversa");
  assert.ok(grande.includes("9000 × 5000") && grande.includes("36 MEGAPIXELS"),
    `a recusa da imagem de link não diz as medidas e o máximo: «${grande}»`);

  // 5. O teto, e a recolhida como convite.
  const medir = () => aba.evaluate(() => ({
    imagens: previasDeLink.imagens,
    desenhadas: document.querySelectorAll("img.previa-de-link").length,
    recolhidas: [...document.querySelectorAll(".previa-convite-frase")].filter((f) => f.textContent.includes("recolhida")).length,
    buscas: window.__links.length,
  }));
  const depois = await medir();
  assert.equal(depois.imagens, teto,
    `o bolso dos links passou do teto: ${depois.imagens} imagens guardadas`);
  assert.equal(depois.desenhadas, teto, "a imagem de link despejada continuou desenhada");
  assert.equal(depois.recolhidas, 1, "a imagem de link despejada sumiu sem o convite que diz que ela foi recolhida");
  assert.equal(depois.buscas, teto + 2, "alguma imagem de link foi buscada mais de uma vez");

  // **Despejar não busca de novo sozinho**: a conversa redesenhada cinco vezes
  // não sai para a rede nenhuma vez.
  for (let i = 0; i < 5; i += 1) {
    await aba.evaluate("desenharMensagens()");
    await aba.waitForTimeout(80);
  }
  const parado = await medir();
  assert.equal(parado.buscas, teto + 2,
    `redesenhar a conversa buscou de novo ${parado.buscas - teto - 2} imagens despejadas: ` +
    "cada desenho busca a que saiu e despeja outra, sem fim");
  assert.equal(parado.recolhidas, 1, "o redesenho mudou quais imagens estão recolhidas");

  // O clique no convite busca a recolhida de novo, e o teto continua valendo.
  await aba.evaluate(() =>
    [...document.querySelectorAll(".previa-convite")].find((c) => c.textContent.includes("recolhida"))
      .querySelector("button").click());
  await aba.waitForFunction((n) => window.__links.length >= n, teto + 3);
  await aba.waitForTimeout(300);
  const reaberta = await medir();
  assert.deepEqual({ imagens: reaberta.imagens, buscas: reaberta.buscas, recolhidas: reaberta.recolhidas },
    { imagens: teto, buscas: teto + 3, recolhidas: 1 },
    "o convite da imagem recolhida não a buscou de novo, ou o teto não valeu na volta");

  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro no console");
  await aba.close();
}

async function principal() {
  const chromium = require("./playwright.cjs").chromium();
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;
  const navegador = await chromium.launch({ headless: true });
  try {
    await oBolsoDosAnexosTemTetoDeImagens(navegador, url);
    await oBolsoDosAnexosTemTetoDeTexto(navegador, url);
    await oBolsoDosLinksTemTetoENaoBuscaDeNovo(navegador, url);
    console.log(
      "teto das prévias: o bolso dos anexos despeja pela conta e pelo texto e diz o que recolheu, " +
      "a recusa por pixels e a imagem ilegível têm frase, e o bolso dos links despeja sem buscar de novo",
    );
  } finally {
    // O fecho no `finally`: sem ele, a primeira asserção que falha deixa o
    // Chromium e o servidor de pé, e a bancada pendura em vez de falhar.
    await navegador.close();
    servidor.close();
  }
}

principal().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
