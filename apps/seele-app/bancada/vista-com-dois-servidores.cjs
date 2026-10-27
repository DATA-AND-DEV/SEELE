// **A troca de vista na web com DOIS servidores reais** (E3; o defeito que a
// E2E de cargos do G achou e o ui-ux consertou em `tela-boot.js`).
//
//   node apps/seele-app/bancada/vista-com-dois-servidores.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <chromium|webkit> <playwright> [--json saída] [--reverter]
//
// A página de produto inteira (`index.html`, `base.js`, `tela-boot.js`, a ponte,
// a identidade, a sessão e o enlace WebRTC), contra dois `servidor_da_bancada`
// reais, S e T. As duas entradas vão pelo COLAR UM LINK: `identidade.abrir`,
// `identidade.entrar` e o `depoisDeConectar` da tela — o caminho em que a vista
// da janela não trocava. O que se prova:
//
// 1. **entrar em S**: a vista da janela é S;
// 2. **entrar em T** pelo link: a vista da janela passa a T (e a época sobe), e
//    a placa de T diz `na_vista`;
// 3. **todo comando sai para T**: `send_message` pela casca (`invoke`) chega à
//    conversa de T, e não à de S;
// 4. **voltar a S** (`ver_sessao`): o comando seguinte chega a S, e não a T.
//
// `--reverter` serve o `tela-boot.js` sem a chamada `porNaVistaAEntrada` (só na
// cópia servida): os casos 2 e 3 têm de reprovar.

"use strict";

const fs = require("fs");
const http = require("http");
const os = require("os");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const [wasm, servidorDaBancada, motor, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && argumentos[i - 1] !== "--json",
);
const saidaJson = opcao("--json");
const REVERTER = argumentos.includes("--reverter");
if (!wasm || !servidorDaBancada || !motor || !playwright) {
  console.error("uso: node vista-com-dois-servidores.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <chromium|webkit> <playwright> [--json saída] [--reverter]");
  process.exit(2);
}
const raiz = path.resolve(__dirname, "..");
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

const vivos = [];
process.on("exit", () => {
  for (const processo of vivos) {
    try {
      processo.kill("SIGKILL");
    } catch {
      // Já saiu.
    }
  }
});

function subirUmServidor() {
  const processo = spawn(servidorDaBancada, [], { stdio: ["pipe", "pipe", "inherit"] });
  vivos.push(processo);
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "vista-dois-servidores-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(wasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(motor, path.join(pasta, "seele_motor_web.wasm"));
  if (REVERTER) {
    const tela = path.join(pasta, "tela-boot.js");
    const texto = fs.readFileSync(tela, "utf8");
    const de = "  porNaVistaAEntrada(resposta);\n";
    if (!texto.includes(de)) throw new Error("a reversão não casa com o tela-boot.js");
    fs.writeFileSync(tela, texto.replace(de, ""));
    console.error("vista-com-dois-servidores: REVERSÃO — o tela-boot.js servido sem o porNaVistaAEntrada");
  }
  const servidor = http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Na página: entra pelo link, como o COLAR UM LINK, até o `depoisDeConectar`. */
function entrarPeloLink(aba, link, apelido) {
  return aba.evaluate(
    async ({ link, apelido }) => {
      const identidade = await identidadeDaPagina();
      const lido = await identidade.abrir(link);
      const entrada = await identidade.entrar(apelido);
      if (!entrada?.ok) return { erro: entrada };
      depoisDeConectar(entrada.resposta, lido.endereco ?? entrada.resposta?.snapshot?.endereco);
      await new Promise((r) => setTimeout(r, 400));
      return { carimbo: entrada.resposta.carimbo, vista: vistaDaJanela.sessao };
    },
    { link, apelido },
  );
}

/** Na página: o que cada sessão da ponte tem na conversa, e quem está na vista. */
function lerAsSessoes(textos) {
  const porSessao = {};
  for (const [id, sessao] of sessoesDaPonte) {
    const conversa = JSON.stringify(sessao.atender("messages", {})?.resposta ?? []);
    porSessao[id] = Object.fromEntries(textos.map((t) => [t, conversa.includes(t)]));
  }
  return { vistaDaJanela: vistaDaJanela.sessao, vistaDaPonte, porSessao };
}

/** Na página: uma mensagem pela casca (o `invoke` de `base.js`, com o carimbo da vista). */
async function mandarPelaCasca(texto) {
  const sessao = sessoesDaPonte.get(vistaDaJanela.sessao);
  const canal = sessao?.retrato?.()?.sala?.channels?.[0]?.id ?? 1;
  try {
    await invoke("open_channel", { channel: canal });
    await invoke("send_message", { channel: canal, body: texto });
    return { ok: true };
  } catch (falha) {
    return { falha: JSON.stringify(falha) };
  }
}

(async () => {
  const S = await subirUmServidor();
  const T = await subirUmServidor();
  const pagina = await servirAPagina();
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  const R = { reversao: REVERTER };
  try {
    const contexto = await browser.newContext();
    const aba = await contexto.newPage();
    aba.on("pageerror", (e) => console.error(`[página] ${e.message}`));
    await aba.addInitScript(() => {
      try {
        localStorage.setItem("seele.casca", "larga");
      } catch {
        // Sem armazenamento.
      }
    });
    await aba.goto(`http://localhost:${pagina.address().port}/index.html`);
    await aba.waitForFunction(() => typeof identidadeDaPagina === "function" && typeof depoisDeConectar === "function", null, { timeout: 15_000 });

    R.emS = await entrarPeloLink(aba, S.link, "ana");
    R.emT = await entrarPeloLink(aba, T.link, "ana");
    const idS = R.emS.carimbo?.sessao;
    const idT = R.emT.carimbo?.sessao;
    R.placas = await aba.evaluate(async () => (await invoke("sessoes")).map((s) => ({ sessao: s.carimbo.sessao, na_vista: s.na_vista })));
    R.mandouEmT = await aba.evaluate(mandarPelaCasca, "PARA T, depois da entrada");
    await new Promise((r) => setTimeout(r, 800));
    R.depoisDeT = await aba.evaluate(lerAsSessoes, ["PARA T, depois da entrada", "PARA S, depois de voltar"]);
    R.voltouAS = await aba.evaluate(async (carimbo) => {
      try {
        // Como a trilha das sessões pede (`camada-sessoes.js`): pela sessão.
        await invoke("ver_sessao", { sessao: carimbo.sessao });
        return { vista: vistaDaJanela.sessao };
      } catch (falha) {
        return { falha: JSON.stringify(falha) };
      }
    }, R.emS.carimbo);
    R.mandouEmS = await aba.evaluate(mandarPelaCasca, "PARA S, depois de voltar");
    await new Promise((r) => setTimeout(r, 800));
    R.depoisDeS = await aba.evaluate(lerAsSessoes, ["PARA T, depois da entrada", "PARA S, depois de voltar"]);
    R.navegador = `${navegador} ${browser.version()}`;
    const t = R.depoisDeT.porSessao;
    const s = R.depoisDeS.porSessao;
    R.veredito = {
      entrarEmS: R.emS.vista === idS && typeof idS === "number",
      entrarEmT: R.emT.vista === idT && idT !== idS && R.placas.some((p) => p.sessao === idT && p.na_vista),
      comandoVaiAT: Boolean(t[idT]?.["PARA T, depois da entrada"]) && !t[idS]?.["PARA T, depois da entrada"],
      voltarASVaiAS: R.voltouAS.vista === idS && Boolean(s[idS]?.["PARA S, depois de voltar"]) && !s[idT]?.["PARA S, depois de voltar"],
    };
  } finally {
    await browser.close();
    pagina.close();
    S.processo.stdin.end();
    T.processo.stdin.end();
  }
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  if (REVERTER) {
    const reprovaram = ["entrarEmT", "comandoVaiAT"].filter((nome) => !R.veredito[nome]);
    console.error(`vista-com-dois-servidores: reversão — reprovaram ${reprovaram.length} de 2 (${reprovaram.join(", ")})`);
    process.exit(reprovaram.length === 2 ? 0 : 1);
  }
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
