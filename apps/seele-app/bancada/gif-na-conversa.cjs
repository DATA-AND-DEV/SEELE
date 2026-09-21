// Um GIF vira imagem na conversa, e uma página que fala de um GIF não vira.
//
// # Por que ela existe
//
// A pergunta «o preview de GIF funciona?» não tinha resposta em lugar nenhum
// deste repositório. O que havia era `frontend.rs`, que confere o **texto** de
// `tela-sessao.js` — que `comLinks` chama `pareceImagem`, que a busca passa por
// `previa_de_link`. Isso prova que as linhas estão escritas. Não prova que uma
// imagem aparece: o `<img>` podia nascer com `src` vazio, a folha podia
// escondê-lo, a CSP podia recusar o `data:`, e os três casos passariam naquela
// suíte inteira. É a diferença que o CLAUDE.md chama de «existir não é
// funcionar».
//
// Aqui o GIF é um GIF de verdade — três quadros, 64×48, assinatura `GIF89a` —,
// e o que se mede é o que o navegador desenhou: `naturalWidth`, o retângulo na
// tela, e quantas buscas saíram.
//
// # As duas metades, e qual delas está aqui
//
// O caminho tem duas: a janela, que decide **o que buscar** e desenha; e o
// Rust, que busca, confere os bytes contra o tipo alegado e monta o `data:`.
//
// Esta bancada é da primeira. `previa_de_link` e `prever_anexo` respondem daqui
// o que o Rust responderia. A segunda tem dono e é conferida lá:
// `seele-core --lib preview::`, que julga `image/gif` contra bytes de GIF.
//
// # O que ela protege, dito em voz alta
//
// 1. **Um link direto de GIF vira imagem desenhada.** Não um `<img>` existindo:
//    uma imagem com tamanho natural e retângulo na tela.
// 2. **A página de um site da lista é buscada; a de qualquer outro, não.**
//    `tenor.com/view/…` é uma página, e é assim que quase todo mundo manda um
//    GIF — o Rust a resolve até a mídia que ela declara. Já
//    `exemplo.br/um-gif-legal` continua link cru: buscá-la entregaria o IP de
//    quem lê a um domínio que outra pessoa colou na conversa.
//
//    E dois que **não** são Tenor, um para cada jeito de errar a comparação:
//    `naotenor.com` termina nas letras de `tenor.com` e passa por um
//    `endsWith` sem o ponto; `tenor.com.exemplo.br` começa com elas e passa por
//    um `startsWith` ou um `includes`. Nos dois casos quem cola o link escolhe
//    o domínio que esta janela busca. A contagem de buscas é a prova.
// 3. **Um anexo `image/gif` oferece a prévia e a desenha.** O `telas.cjs`
//    simulava `regras_de_previa` sem `image/gif`, então nenhuma bancada teria
//    visto este caminho quebrar.
//
//   node apps/seele-app/bancada/gif-na-conversa.cjs

const assert = require("node:assert/strict");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");

// Três quadros, 64×48, `GIF89a` com bloco de repetição NETSCAPE2.0. Escrito
// aqui e não gerado: uma bancada que depende de um gerador de imagens instalado
// na máquina é uma bancada que não roda.
const GIF =
  "R0lGODlhQAAwAIEAAP8AAAAAAAAAAAAAACH/C05FVFNDQVBFMi4wAwEAAAAh+QQADAAAACwAAAAAQAAw" +
  "AAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJsqXLlzBj" +
  "ypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAAh+QQBDAABACwAAAAAQAAwAIEA/wAAAAAA" +
  "AAAAAAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJsqXL" +
  "lzBjypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAAh+QQBDAABACwAAAAAQAAwAIEAAP8A" +
  "AAAAAAAAAAAIWgABCBxIsKDBgwgTKlzIsKHDhxAjSpxIsaLFixgzatzIsaPHjyBDihxJsqTJkyhTqlzJ" +
  "sqXLlzBjypxJs6bNmzhz6tzJs6fPn0CDCh1KtKjRo0iTKl3KtGnRgAA7";
const URI = `data:image/gif;base64,${GIF}`;
const LARGURA_DO_GIF = 64;

// A forma de `PreviewRules`, com os quatro tipos que `preview_rules()` monta de
// `ImageFormat::ALL`. Uma lista escrita aqui com três seria a bancada aprovando
// o que o produto recusa — ou, pior, deixando de ver o que ele aceita.
const REGRAS = {
  limit: 4 * 1024 * 1024,
  types: ["image/png", "image/jpeg", "image/gif", "image/webp"],
};

const agora = Math.floor(Date.now() / 1000);

/** A lista fechada, como `dominios_de_gif` a devolve. */
const DOMINIOS = ["tenor.com", "giphy.com", "imgur.com"];

/** Os dois que têm de ser buscados, e os dois que não podem ser. */
const BUSCA = "https://exemplo.br/dancando.gif";
const PAGINA_DA_LISTA = "https://tenor.com/view/gato-dancando-gif-12345";
const PAGINA_DE_FORA = "https://exemplo.br/um-gif-legal";
const SUFIXO_SEM_PONTO = "https://naotenor.com/colher-ip";
const PREFIXO_DE_MENTIRA = "https://tenor.com.exemplo.br/colher-ip";

const MENSAGENS = [
  {
    id: 1, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 300,
    body: `olha isso ${BUSCA}`,
    own: false, edited: false, attachment: null,
  },
  {
    id: 2, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 240,
    body: `e esse aqui ${PAGINA_DA_LISTA}`,
    own: false, edited: false, attachment: null,
  },
  {
    id: 3, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 200,
    body: `vê esse tb ${PAGINA_DE_FORA} e ${SUFIXO_SEM_PONTO} e ${PREFIXO_DE_MENTIRA}`,
    own: false, edited: false, attachment: null,
  },
  {
    id: 4, channel: 1, author: 1, author_nickname: "Alex", at_seconds: agora - 120,
    body: "mandando por anexo", own: true, edited: false,
    attachment: {
      id: 77, file_name: "dancando.gif", declared_type: "image/gif",
      byte_size: 402, expired: false,
    },
  },
];

/** O que o navegador desenhou de uma imagem, medido e não suposto. */
function medida(seletor) {
  const img = document.querySelector(seletor);
  if (!img) return null;
  const caixa = img.getBoundingClientRect();
  return {
    // O começo do `src`, e não ele inteiro: o resto são 536 caracteres de
    // base64 que não dizem nada numa mensagem de falha.
    tipo: img.src.slice(0, img.src.indexOf(",") + 1),
    completa: img.complete,
    larguraNatural: img.naturalWidth,
    alturaNatural: img.naturalHeight,
    visivel: caixa.width > 0 && caixa.height > 0,
  };
}

async function principal() {
  const { chromium } = require(process.env.PLAYWRIGHT
    ?? "/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright");
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;

  const navegador = await chromium.launch({ headless: true });
  try {
    await medir(navegador, url);
  } finally {
    // **O fecho no `finally`, e não depois das asserções.** Sem isto, a
    // primeira asserção que falha deixa o Chromium e o servidor de pé, e o
    // processo do Node não tem por onde acabar: a bancada pendura em vez de
    // falhar. Pendurada, ela para a suíte inteira e não diz o motivo — que é
    // pior do que não existir, porque parece problema de outra coisa.
    await navegador.close();
    servidor.close();
  }
}

async function medir(navegador, url) {
  const aba = await navegador.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = [];
  aba.on("pageerror", (e) => erros.push("pageerror: " + String(e.message).slice(0, 200)));
  aba.on("console", (m) => {
    if (m.type() === "error") erros.push("console: " + m.text().slice(0, 200));
  });

  const r = retrato();
  const tabela = respostas({
    messages: MENSAGENS,
    regras_de_previa: REGRAS,
    dominios_de_gif: DOMINIOS,
    previa_de_link: URI,
    prever_anexo: { image: URI },
    snapshot: r,
  });

  // A ponte anota **para onde** a prévia de link saiu: a contagem é o que prova
  // que uma página não foi buscada, e sem o registro a prova não existe.
  await aba.addInitScript((t) => {
    window.__buscas = [];
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          if (cmd === "previa_de_link") window.__buscas.push(args.url);
          return Object.hasOwn(t, cmd) ? t[cmd] : null;
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
    };
  }, tabela);

  await aba.goto(url);
  await aba.waitForTimeout(700);
  await aba.evaluate(() => {
    for (const secao of document.querySelectorAll("section.tela")) secao.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
  });
  await aba.evaluate((x) => window.desenhar(x), r);
  await aba.waitForTimeout(300);
  await aba.evaluate("sincronizarMensagens(99)");
  await aba.waitForTimeout(900);

  // ------------------------------------------------- 1. o link direto de GIF
  const link = await aba.evaluate(medida, "img.previa-de-link");
  assert.ok(link, "um link direto de `.gif` não virou imagem nenhuma na conversa");
  assert.equal(link.tipo, "data:image/gif;base64,", "a prévia chegou à tela com outro tipo de mídia");
  assert.equal(link.larguraNatural, LARGURA_DO_GIF,
    "o `<img>` existe e o navegador não decodificou o GIF: `src` vazio, `data:` recusado pela CSP, ou bytes cortados");
  assert.ok(link.visivel, "a imagem decodificou e não ocupa lugar nenhum: a folha a escondeu");

  // ---------------- 2. quais endereços foram buscados, e quais não podiam ser
  const buscas = await aba.evaluate(() => window.__buscas);
  assert.deepEqual(buscas.slice().sort(), [BUSCA, PAGINA_DA_LISTA].sort(),
    "as buscas que saíram não são exatamente o link direto e a página da lista fechada — " +
    `um endereço a mais aqui é o IP de quem lê indo para um domínio que outra pessoa colou: ${JSON.stringify(buscas)}`);
  assert.ok(!buscas.includes(PAGINA_DE_FORA),
    "uma página fora da lista foi buscada: o guarda de `pareceImagem` caiu");
  assert.ok(!buscas.includes(SUFIXO_SEM_PONTO),
    "`naotenor.com` passou por Tenor: a comparação de domínio perdeu o ponto, " +
    "e todo domínio que termine nessas letras entrou na lista fechada");
  assert.ok(!buscas.includes(PREFIXO_DE_MENTIRA),
    "`tenor.com.exemplo.br` passou por Tenor: a comparação virou `startsWith` " +
    "ou `includes`, e quem cola o link escolhe o domínio que esta janela busca");

  const desenhadas = await aba.evaluate(
    () => [...document.querySelectorAll("img.previa-de-link")].length);
  assert.equal(desenhadas, 2,
    "o desenho não bate com as duas buscas que saíram");

  // -------------------------------------------------- 3. o anexo `image/gif`
  const ofereceu = await aba.evaluate(() => !!document.querySelector('[data-anexo-previa="77"]'));
  assert.ok(ofereceu,
    "um anexo `image/gif` não ofereceu prévia: `regras_de_previa` deixou de trazer os quatro tipos de `ImageFormat::ALL`");
  await aba.evaluate(() => document.querySelector('[data-anexo-previa="77"]').click());
  await aba.waitForTimeout(700);
  const anexo = await aba.evaluate(medida, ".anexo-desenho img");
  assert.ok(anexo, "o botão foi apertado e a prévia do anexo não desenhou");
  assert.equal(anexo.larguraNatural, LARGURA_DO_GIF, "a prévia do anexo não decodificou o GIF");
  assert.ok(anexo.visivel, "a prévia do anexo decodificou e não ocupa lugar nenhum");

  if (process.env.RETRATO) {
    await aba.screenshot({ path: process.env.RETRATO });
    console.log(`retrato em ${process.env.RETRATO}`);
  }

  // Um erro de página que não derrubou nenhuma asserção ainda é um defeito, e
  // uma bancada que o engole é uma bancada que mente por omissão.
  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro no console");

  console.log(
    `GIF: link direto e página da lista desenham ${link.larguraNatural}×${link.alturaNatural}, ` +
    "página de fora e sufixo sem ponto não são buscados, anexo `image/gif` oferece e desenha",
  );
}

principal().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
