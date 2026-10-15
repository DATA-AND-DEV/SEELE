// O que a casca chama sozinha, no navegador, com a ponte de verdade.
//
// Tarefa 3.1 da F4, o primeiro dos dois guardas da matriz da F8
// (`F8/paridade.md`): **o que a casca chama sem gesto nunca volta com
// recusa.** A F8 mediu, no Chromium e com um duble, 17 comandos na carga. Uma
// lista escrita à mão envelhece no primeiro comando novo; esta bancada carrega
// a `ui/` inteira num Chromium, **sem duble e sem toque**, deixa a ponte
// (`ui/ponte-web.js`) atender, e reprova se algum comando voltar recusado, a
// fazer ou desconhecido, ou se a página estourar.
//
//     node apps/seele-app/bancada/ponte-sem-gesto.cjs
//
// As duas cascas: o celular (390 px, toque) e o PC (1280 px, ponteiro). A
// marca de cada uma é conferida também.
//
// **Com os cabeçalhos da origem** (`web/_headers`, D-web-2), como a origem a
// serve: a CSP da página inteira, e nenhuma violação dela na carga, nas duas
// cascas. Com o `.wasm` da página compilado (`cargo xtask web`), ele vai junto,
// e a carga o compila debaixo da CSP.
//
// **O que ela ainda não alcança:** a sessão. Sem a sessão pelo navegador (2.6),
// os que a casca chama sozinha dentro dela (`estado_da_porta`,
// `mods_instalados`, o `snapshot` a cada poucos segundos) não acontecem aqui.
// Quando a sessão existir, esta bancada entra nela antes de medir.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const raiz = path.resolve(__dirname, "..");

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("ponte-sem-gesto: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

/** Quanto a casca fica sozinha depois de carregar: a carga e os primeiros temporizadores. */
const SOZINHA_MS = 5000;

// **Uma cópia**, e não a `ui/`: um arquivo de bancada esquecido lá iria para o
// instalador (`tools/carga-da-casca.py` diz o mesmo).
const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "ponte-sem-gesto-"));
fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
const WASM_DA_PAGINA = path.resolve(raiz, "..", "..", "target", "wasm32-unknown-unknown", "release", "seele_web.wasm");
const comOWasm = fs.existsSync(WASM_DA_PAGINA);
if (comOWasm) fs.copyFileSync(WASM_DA_PAGINA, path.join(pasta, "seele_web.wasm"));

const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".wasm": "application/wasm",
};

function servir() {
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404);
      resposta.end();
      return;
    }
    resposta.writeHead(200, {
      ...cabecalhosDaOrigem(`/${nome}`),
      "content-type": TIPOS[path.extname(arquivo)] ?? "application/octet-stream",
    });
    fs.createReadStream(arquivo).pipe(resposta);
  });
}

async function medir(navegador, url, casca, opcoes) {
  const contexto = await navegador.newContext(opcoes);
  const aba = await contexto.newPage();
  const estouros = [];
  aba.on("pageerror", (erro) => estouros.push(String(erro.message).slice(0, 240)));
  // Toda violação da CSP da origem: pelo evento, e pelo console, que a diz
  // também quando ela é de um recurso que a página nem chegou a pedir por
  // script.
  const violacoes = [];
  aba.on("console", (m) => /Content Security Policy/i.test(m.text()) && violacoes.push(m.text().slice(0, 240)));
  await aba.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) => {
      console.error(`Content Security Policy: ${e.effectiveDirective} recusou ${e.blockedURI || "(embutido)"}`);
    });
  });
  await aba.goto(url);
  await aba.waitForTimeout(SOZINHA_MS);
  const visto = await aba.evaluate(() => ({
    marca: document.documentElement.dataset.casca,
    lugar: PONTE_NO_LUGAR_DO_TAURI,
    chamadas: [...CHAMADAS_DA_PONTE.entries()],
    contas: [...CONTAS_DA_PONTE.entries()],
  }));
  const cabecalhosDaPagina = (await (await fetch(url)).headers.get("content-security-policy")) ?? "";
  await contexto.close();
  const falhas = [];
  if (!visto.lugar) falhas.push(`${casca}: a ponte não se pôs no lugar do Tauri`);
  if (visto.marca !== casca) falhas.push(`${casca}: a marca saiu ${visto.marca}`);
  for (const [comando, contas] of visto.contas) {
    falhas.push(`${casca}: a casca chamou \`${comando}\` sozinha, e a ponte respondeu ${JSON.stringify(contas)}`);
  }
  for (const estouro of estouros) falhas.push(`${casca}: a página estourou: ${estouro}`);
  for (const violacao of violacoes) falhas.push(`${casca}: a CSP da origem recusou: ${violacao}`);
  const daOrigem = cabecalhosDaOrigem("/index.html")["content-security-policy"];
  if (!daOrigem || cabecalhosDaPagina !== daOrigem) falhas.push(`${casca}: a página veio sem a CSP da origem, e a bancada não a mediu`);
  if (visto.chamadas.length === 0) falhas.push(`${casca}: a casca não chamou nada, e a bancada não mediu nada`);
  const chamados = visto.chamadas.map(([comando]) => comando).sort();
  console.log(`  ${casca}: ${chamados.length} comandos sem gesto: ${chamados.join(", ")}`);
  return falhas;
}

(async () => {
  const servidor = servir();
  await new Promise((pronto) => servidor.listen(0, "127.0.0.1", pronto));
  const url = `http://127.0.0.1:${servidor.address().port}/index.html`;
  const navegador = await chromium().launch();
  let falhas = [];
  try {
    falhas = [
      ...(await medir(navegador, url, "celular", {
        viewport: { width: 390, height: 844 },
        hasTouch: true,
        isMobile: true,
      })),
      ...(await medir(navegador, url, "larga", { viewport: { width: 1280, height: 860 } })),
    ];
  } finally {
    await navegador.close();
    servidor.close();
    fs.rmSync(pasta, { recursive: true, force: true });
  }
  terminou = true;
  if (falhas.length === 0) {
    console.log(`ponte-sem-gesto: nada do que a casca chama sozinha volta recusado, e a CSP da origem não recusa nada, nas duas cascas (${comOWasm ? "com" : "sem"} o .wasm da página).`);
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
