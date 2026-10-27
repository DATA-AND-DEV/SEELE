// A página batendo à porta de um servidor hospedado REAL, com a portaria
// ligada (E7, pedido do H em pedidos.md: «o navegador batendo à porta NÃO foi
// medido»).
//
//   node apps/seele-app/bancada/portaria-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <portaria-da-bancada> <chromium|webkit> <playwright> [--json saída]
//
// - `<servidor_da_bancada>`: o `Hospedagem` do app (crates/seele-server/examples), com o banco
//   numa pasta temporária;
// - `<portaria-da-bancada>`: a ferramenta que decide pela portaria de verdade
//   (`seele_server::portaria::{ligar, pedidos, decidir}`), abrindo o banco dele. É o papel do
//   dono no app, sem a casca (o fonte está em `medidas/portaria-2026-10-24/*.rs.txt`, que o passo «a portaria-da-bancada» do ci.yml monta).
//
// A página é a `ui/` de produto, a mesma da `sessao-no-navegador.cjs`, e entra pela identidade
// (o COLAR UM LINK). O que se prova, cada um com o resultado na saída:
// 1. **a batida**: com a portaria ligada, a entrada vira espera (`espera-de-admissao`, com o
//    `AdmissionPending` do servidor), e o pedido aparece em `portaria::pedidos` com o apelido da
//    página;
// 2. **o ADMITIR**: o dono decide sim, e a página, batendo de novo (como a tela de espera bate),
//    entra;
// 3. **o RECUSAR**: outra página bate, o dono decide não, e ela recebe `admissao-recusada`, com a
//    frase da recusa;
// 4. **a desistência e o retorno**: uma terceira bate e desiste (`agoraNao`). Volta pelo mesmo
//    link, com a mesma chave e no mesmo pedido (a fila não ganha uma segunda pessoa), e entra
//    depois do ADMITIR.
// E as frases que a casca mostra, lidas da `frases.js` de produto.

"use strict";

const { spawn, execFileSync } = require("node:child_process");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");

const argumentos = process.argv.slice(2);
const saidaJson = argumentos.includes("--json") ? argumentos[argumentos.indexOf("--json") + 1] : null;
const [caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, ferramenta, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--json"),
);
if (![caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, ferramenta, playwright].every(Boolean)) {
  console.error("uso: node portaria-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <portaria-da-bancada> <chromium|webkit> <playwright> [--json saída]");
  process.exit(2);
}
const raiz = path.resolve(__dirname, "..");
// `--reverter`: a prova por reversão, NA CÓPIA que esta bancada serve (nunca na
// árvore). A `identidade-web.js` servida perde o tratamento da portaria
// (`AdmissionPending` e `AdmissionDenied`), como era antes do E7, e a
// bancada tem de reprovar.
const reverter = argumentos.includes("--reverter");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
  ".woff2": "font/woff2",
};

function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "portaria-no-navegador-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
  if (reverter) {
    const caminho = path.join(pasta, "identidade-web.js");
    const fonte = fs.readFileSync(caminho, "utf8");
    const sem = fonte
      .replace(/    if \(razao === "AdmissionPending"\)[^\n]*\n/, "")
      .replace(/    if \(razao === "AdmissionDenied"\)[^\n]*\n/, "");
    if (sem === fonte) throw new Error("--reverter: a correção não foi achada na cópia; a reversão não reverteu nada");
    fs.writeFileSync(caminho, sem);
  }
  const indice = path.join(pasta, "index.html");
  const html = fs.readFileSync(indice, "utf8");
  if (!html.includes('src="rtc-web.js"')) {
    fs.writeFileSync(indice, html.replace('<script src="sessao-web.js"></script>', '<script src="rtc-web.js"></script>\n<script src="sessao-web.js"></script>'));
  }
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
}

function subirOServidor() {
  const processo = spawn(caminhoDoServidor, [], { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

/** A portaria de verdade, pelo banco do servidor. */
const portaria = (banco, ...args) => JSON.parse(execFileSync(ferramenta, [banco, ...args], { encoding: "utf8" }).trim());

async function abrirUmaPagina(browser, endereco) {
  const contexto = await browser.newContext();
  const aba = await contexto.newPage();
  await aba.goto(`${endereco}index.html`);
  return { contexto, aba };
}

/** Abre o link e entra, pela identidade da página. */
const entrar = (aba, link, apelido) =>
  aba.evaluate(
    async ({ link, apelido }) => {
      const identidade = await identidadeDaPagina();
      const lido = await identidade.abrir(link);
      const entrada = await identidade.entrar(apelido);
      return { lido, entrada, depois: identidade.ler() };
    },
    { link, apelido },
  );

/** Bate de novo, como a tela de espera bate (a tentativa continua aberta). */
const baterDeNovo = (aba, apelido) =>
  aba.evaluate(async (apelido) => (await identidadeDaPagina()).entrar(apelido), apelido);

(async () => {
  const servidor = await subirOServidor();
  const banco = path.join(os.tmpdir(), `servidor-da-bancada-${servidor.processo.pid}`, "mundo", "seele.db");
  const R = { banco, casos: {} };
  R.ligada = portaria(banco, "ligar");
  const pagina = servirAPagina();
  await new Promise((ok) => pagina.listen(0, "127.0.0.1", ok));
  const endereco = `http://localhost:${pagina.address().port}/`;
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch({
    args: navegador === "webkit" ? [] : ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  });
  R.navegador = `${navegador} ${browser.version()}`;
  // Sem o convite: quem não tem convite bate à porta. Com ele, o convite já
  // admitiria (ADR 0030), e a portaria não seria perguntada.
  const semConvite = servidor.link.replace(/([?&])convite=[^&]*&?/, "$1").replace(/[?&]$/, "");
  try {
    // ---------------------------------------------------- 1 e 2. bater e ADMITIR
    {
      const a = await abrirUmaPagina(browser, endereco);
      const primeira = await entrar(a.aba, semConvite, "ana");
      const fila = portaria(banco, "pedidos");
      const daAna = fila.find((p) => p.apelido === "ana");
      const decisao = daAna ? portaria(banco, "decidir", daAna.impressao, "sim") : null;
      const depois = await baterDeNovo(a.aba, "ana");
      R.casos.admitir = {
        primeira: primeira.entrada,
        estadoNaEspera: primeira.depois?.estado ?? null,
        naFila: daAna ?? null,
        decisao,
        depoisDoAdmitir: { ok: depois.ok === true, erro: depois.erro ?? null, servidor: depois.resposta?.snapshot?.server ?? null },
      };
      await a.contexto.close();
    }
    // --------------------------------------------------------------- 3. RECUSAR
    {
      const b = await abrirUmaPagina(browser, endereco);
      const primeira = await entrar(b.aba, semConvite, "bia");
      const daBia = portaria(banco, "pedidos").find((p) => p.apelido === "bia" && !p.decidido);
      const decisao = daBia ? portaria(banco, "decidir", daBia.impressao, "nao") : null;
      const depois = await baterDeNovo(b.aba, "bia");
      R.casos.recusar = { primeira: primeira.entrada?.erro ?? null, decisao, depois };
      await b.contexto.close();
    }
    // ------------------------------------------- 4. a desistência e o retorno
    {
      const c = await abrirUmaPagina(browser, endereco);
      const primeira = await entrar(c.aba, semConvite, "caio");
      await c.aba.evaluate(async () => (await identidadeDaPagina()).agoraNao());
      const naFilaAntes = portaria(banco, "pedidos").filter((p) => p.apelido === "caio");
      const volta = await entrar(c.aba, semConvite, "caio");
      const naFilaDepois = portaria(banco, "pedidos").filter((p) => p.apelido === "caio");
      const doCaio = naFilaDepois.find((p) => !p.decidido);
      const decisao = doCaio ? portaria(banco, "decidir", doCaio.impressao, "sim") : null;
      const depois = await baterDeNovo(c.aba, "caio");
      R.casos.desistirEVoltar = {
        primeira: primeira.entrada?.erro ?? null,
        naFilaAntes,
        estadoNaVolta: volta.lido?.estado ?? null,
        voltaDeu: volta.entrada?.erro ?? (volta.entrada?.ok ? "ok" : null),
        naFilaDepois,
        decisao,
        depoisDoAdmitir: { ok: depois.ok === true, erro: depois.erro ?? null },
      };
      // As frases que a casca mostra, da `frases.js` de produto.
      R.frases = await c.aba.evaluate(() => ({
        espera: typeof MOTIVOS === "object" ? MOTIVOS.AdmissionPending ?? null : null,
        recusa: typeof MOTIVOS === "object" ? MOTIVOS.AdmissionDenied ?? null : null,
      }));
      await c.contexto.close();
    }
  } finally {
    await browser.close();
    pagina.close();
    servidor.processo.stdin.end();
  }
  const c = R.casos;
  R.veredito = {
    bateuEEsperou:
      c.admitir.primeira?.erro === "espera-de-admissao" &&
      c.admitir.primeira?.motivo?.Refused?.reason === "AdmissionPending" &&
      Boolean(c.admitir.naFila),
    entrouDepoisDoAdmitir: c.admitir.depoisDoAdmitir.ok,
    recusadaComAFrase: c.recusar.primeira === "espera-de-admissao" && c.recusar.depois?.erro === "admissao-recusada" && Boolean(R.frases.recusa),
    voltouNoMesmoPedido:
      c.desistirEVoltar.naFilaAntes.length === 1 &&
      c.desistirEVoltar.naFilaDepois.length === 1 &&
      c.desistirEVoltar.naFilaDepois[0].impressao === c.desistirEVoltar.naFilaAntes[0].impressao &&
      c.desistirEVoltar.estadoNaVolta === "ja-conhecido",
    entrouDepoisDeVoltar: c.desistirEVoltar.depoisDoAdmitir.ok,
  };
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
