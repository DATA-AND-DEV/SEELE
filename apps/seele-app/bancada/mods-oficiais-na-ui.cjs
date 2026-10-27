// **Os MODs oficiais 4.0.0 (API 6) na UI da 0.17, com o DOM de verdade** (MESA,
// PERFIS e ESTILO), contra um servidor REAL que exige o MOD.
//
//   node apps/seele-app/bancada/mods-oficiais-na-ui.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <pasta-do-mod> [--webkit] [--capturas pasta]
//
// A página é a `ui/` de produto com a ponte web, o `.wasm` da sessão e o motor
// dos MODs (o QuickJS num `iframe` opaco). Entra pelo COLAR UM LINK, aceita os
// MODs e o pacote vem do catálogo (servido pela bancada, assinado com a chave
// dela) — o mesmo caminho do caso 14 da `sessao-no-navegador.cjs`. O pacote é
// uma CÓPIA descartável (só `mod.json`, `cliente/`, `servidor/` e os `arquivos`
// declarados), e o perfil do navegador é um contexto novo: nada do usuário.
//
// Por MOD, o que se confere (visibilidade computada, caixa > 0, dentro da janela):
//  - a superfície monta no DOM (a entrada da navegação, o que a entrada abre);
//  - um gesto real da UI chega ao MOD e a resposta volta à tela;
//  - ESTILO: o `tema` aplica na janela;
//  - nenhum erro no console nem `pageerror`.
//
// SIMULADO: nada na ponte (é a ponte web real). O catálogo é da bancada. REAL: a
// `ui/`, o motor, o servidor e o pacote.

"use strict";

const { spawn } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");
const assert = require("node:assert/strict");
const { chromium, webkit } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const { fixarAIdentidade, impressaoDaPagina } = require("./identidade-da-bancada.cjs");

const raiz = path.resolve(__dirname, "..");
const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const posicionais = argumentos.filter((a, i) => !a.startsWith("--") && !["--capturas", "--roteiro"].includes(argumentos[i - 1]));
const [caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, pastaDoMod] = posicionais;
const WEBKIT = argumentos.includes("--webkit");
const pastaDeCapturas = opcao("--capturas");
if (![caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, pastaDoMod].every((c) => c && fs.existsSync(c))) {
  console.error("uso: node apps/seele-app/bancada/mods-oficiais-na-ui.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <pasta-do-mod> [--webkit] [--capturas pasta]");
  process.exit(2);
}
if (pastaDeCapturas) fs.mkdirSync(pastaDeCapturas, { recursive: true });

const ENDERECO_DO_CATALOGO = "https://mods.seele.sh";
const VIEWPORT = { width: 1440, height: 900 };

const doServidorErr = [];
const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};
/**
 * **Defeito do PRODUTO já reportado, medido e não escondido**: o que um roteiro
 * prova e a UI hoje não cumpre. Aparece na saída com o rótulo DEFEITO, e só
 * reprova a bancada com `--estrito`.
 */
const defeitos = [];
const defeito = (caso, condicao, detalhe) => {
  if (!condicao) defeitos.push(`${caso}: ${detalhe}`);
};
const ESTRITO = process.argv.includes("--estrito");

/** Uma cópia descartável do pacote: o que um pacote publicado leva. */
function copiarOPacote() {
  const manifesto = JSON.parse(fs.readFileSync(path.join(pastaDoMod, "mod.json"), "utf8"));
  const copia = fs.mkdtempSync(path.join(os.tmpdir(), "mod-oficial-"));
  fs.copyFileSync(path.join(pastaDoMod, "mod.json"), path.join(copia, "mod.json"));
  for (const dir of ["cliente", "servidor"]) fs.cpSync(path.join(pastaDoMod, dir), path.join(copia, dir), { recursive: true });
  // `--diagnostico-reach-curto`: o `servidor_da_bancada` e o `seele_web.wasm` COMPILADOS ANTES de o
  // teto do `reach` subir de 32 para 48 bytes recusam itens maiores (`ModsIndisponiveis`); com este
  // modo os itens são cortados em 32 bytes, SÓ para exercitar o código do MOD até o binário novo.
  if (process.argv.includes("--diagnostico-reach-curto")) {
    const reescrito = JSON.parse(fs.readFileSync(path.join(copia, "mod.json"), "utf8"));
    const corta = (t) => { let r = t; while (Buffer.byteLength(r) > 32) r = r.slice(0, -1); return r.trimEnd(); };
    reescrito.reach = reescrito.reach.map(corta);
    fs.writeFileSync(path.join(copia, "mod.json"), JSON.stringify(reescrito, null, 2));
  }
  // `--diagnostico-sem-arquivos`: SÓ para isolar uma causa; o pacote deixa de ser o publicado.
  if (process.argv.includes("--diagnostico-sem-arquivos")) {
    const reescrito = { ...manifesto };
    delete reescrito.arquivos;
    fs.writeFileSync(path.join(copia, "mod.json"), JSON.stringify(reescrito, null, 2));
  }
  for (const arquivo of process.argv.includes("--diagnostico-sem-arquivos") ? [] : manifesto.arquivos ?? []) {
    fs.mkdirSync(path.dirname(path.join(copia, arquivo)), { recursive: true });
    fs.copyFileSync(path.join(pastaDoMod, arquivo), path.join(copia, arquivo));
  }
  return { copia, manifesto };
}

function arquivosDoPacote(pasta, dentro = "") {
  const arquivos = [];
  for (const entrada of fs.readdirSync(path.join(pasta, dentro), { withFileTypes: true })) {
    const relativo = dentro ? `${dentro}/${entrada.name}` : entrada.name;
    if (!dentro && entrada.name === "dados") continue;
    if (entrada.isDirectory()) arquivos.push(...arquivosDoPacote(pasta, relativo));
    else if (entrada.isFile()) arquivos.push(relativo);
  }
  return arquivos;
}

const TIPOS = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".wasm": "application/wasm", ".svg": "image/svg+xml", ".png": "image/png", ".json": "application/json", ".woff2": "font/woff2" };

function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "mods-oficiais-na-ui-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
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

function subirOServidor(args) {
  const processo = spawn(caminhoDoServidor, args, { stdio: ["pipe", "pipe", "pipe"], env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? "warn" } });
  processo.stderr.on("data", (pedaco) => doServidorErr.push(String(pedaco)));
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

function chaveDoMinisign() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519");
  const publica = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const numero = crypto.randomBytes(8);
  const pub = `untrusted comment: chave da bancada\n${Buffer.concat([Buffer.from("Ed"), numero, publica]).toString("base64")}\n`;
  const assinar = (dados) => {
    const assinatura = crypto.sign(null, dados, privateKey);
    const comentario = "assinado pela bancada";
    const global = crypto.sign(null, Buffer.concat([assinatura, Buffer.from(comentario)]), privateKey);
    return `untrusted comment: assinatura da bancada\n${Buffer.concat([Buffer.from("Ed"), numero, assinatura]).toString("base64")}\ntrusted comment: ${comentario}\n${global.toString("base64")}\n`;
  };
  return { pub, assinar };
}

async function servirOCatalogo(contexto, chave, exigido, pasta, titulo) {
  const arquivos = arquivosDoPacote(pasta);
  // A API da entrada do catálogo é a que o manifesto do PRÓPRIO pacote declara (6 nos 4.0.0, 7 nas variantes): nunca uma 6
  // reescrita como 7, e nunca uma 7 como 6.
  const api = JSON.parse(fs.readFileSync(path.join(pasta, "mod.json"), "utf8")).api;
  const catalogo = Buffer.from(JSON.stringify({
    esquema: 1,
    gerado_em: Math.floor(Date.now() / 1000),
    mods: [{ id: exigido.id, titulo, versoes: [{ versao: exigido.version, api, hash: exigido.hash, arquivos }] }],
  }));
  const revogacoes = Buffer.from(JSON.stringify({ esquema: 1, mods: [] }));
  const servidos = new Map([
    ["/catalogo.json", catalogo],
    ["/catalogo.json.minisig", Buffer.from(chave.assinar(catalogo))],
    ["/revogacoes.json", revogacoes],
    ["/revogacoes.json.minisig", Buffer.from(chave.assinar(revogacoes))],
  ]);
  for (const relativo of arquivos) servidos.set(`/mods/${exigido.id}/${exigido.version}/${relativo}`, fs.readFileSync(path.join(pasta, relativo)));
  const pedidos = [];
  await contexto.route(`${ENDERECO_DO_CATALOGO}/**`, (rota) => {
    const caminho = new URL(rota.request().url()).pathname;
    pedidos.push(caminho);
    const corpo = servidos.get(caminho);
    return corpo ? rota.fulfill({ status: 200, body: corpo, headers: { "access-control-allow-origin": "*" } }) : rota.fulfill({ status: 404, body: "" });
  });
  return { pedidos };
}

async function esperar(aba, funcao, arg, prazoMs = 10_000) {
  const fim = Date.now() + prazoMs;
  let ultimo;
  for (;;) {
    ultimo = await aba.evaluate(funcao, arg);
    if (ultimo && ultimo.pronto) return ultimo;
    if (Date.now() > fim) return ultimo;
    await new Promise((r) => setTimeout(r, 150));
  }
}

/** Visível: sem `hidden`/display/visibility, caixa > 0 e dentro da janela. */
const MEDIDOR = `
  window.__visivel = (el) => {
    if (!el) return false;
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      const e = getComputedStyle(n);
      if (n.hidden || e.display === "none" || e.visibility === "hidden") return false;
    }
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && r.right > 0 && r.bottom > 0 && r.left < innerWidth && r.top < innerHeight;
  };
  window.__textos = (id) => [...document.querySelectorAll('[data-mod="' + id + '"]')].filter(window.__visivel).map((n) => n.innerText || n.textContent);
`;

module.exports = { MEDIDOR };

if (require.main === module) {
  (async () => {
    const { copia, manifesto } = copiarOPacote();
    const id = manifesto.id;
    const curto = id.split("/")[1];
    const doConsole = [];
    const servidorDaPagina = servirAPagina();
    await new Promise((ok) => servidorDaPagina.listen(0, "127.0.0.1", ok));
    const endereco = `http://localhost:${servidorDaPagina.address().port}/`;
    const navegador = WEBKIT
      ? await webkit().launch()
      : await chromium().launch({ args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", "--autoplay-policy=no-user-gesture-required"] });
    let servidor = null;
    try {
      // A pessoa é o dono do servidor (a impressão fixa do contexto): o ESTILO e
      // a MESA têm gestos de quem administra.
      const contexto = await navegador.newContext({ permissions: WEBKIT ? [] : ["microphone"], viewport: VIEWPORT, acceptDownloads: true });
      await fixarAIdentidade(contexto);
      const aba = await contexto.newPage();
      aba.on("console", (m) => { if (["error", "warning"].includes(m.type())) doConsole.push(`${m.type()}: ${m.text().slice(0, 300)} @ ${m.location().url}:${m.location().lineNumber}`); });
      aba.on("pageerror", (e) => doConsole.push(`pageerror: ${e.message} @ ${(e.stack ?? "").split("\n").slice(1, 3).join(" | ")}`));
      await aba.goto(`${endereco}index.html`);
      const dona = await impressaoDaPagina(aba);
      servidor = await subirOServidor(["--local", "--proprietario", dona, copia]);
      const exigido = servidor.mod;
      assert.ok(exigido && exigido.id === id && /^[0-9a-f]{64}$/.test(exigido.hash), `o servidor não exige ${id}: ${JSON.stringify(exigido)}`);

      // O pacote entra no ARMAZÉM da página, endereçado pelo hash que o servidor anunciou, como o
      // armazém o guarda depois de uma instalação (`guardarOPacote`, em `ponte-web.js`); o `.wasm`
      // confere o conteúdo contra o hash a cada leitura. Assim a bancada roda com o `.wasm` de
      // PRODUÇÃO (sem a porta `prova_chave_do_catalogo`, que só o build de prova tem): o que não
      // se exercita aqui é o download do catálogo, que é a `sessao-no-navegador.cjs` (caso 14).
      const arquivos = arquivosDoPacote(copia).map((relativo) => [relativo, fs.readFileSync(path.join(copia, relativo)).toString("base64")]);
      await aba.evaluate(async ({ hash, mid, versao, arquivos }) => {
        const bytes = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        await guardarOPacote({ hash, id: mid, versao, arquivos: arquivos.map(([caminho, b64]) => [caminho, bytes(b64)]) });
      }, { hash: exigido.hash, mid: exigido.id, versao: exigido.version, arquivos });
      await aba.evaluate(MEDIDOR);

      // A entrada: COLAR UM LINK, aceitar os MODs, `depoisDeConectar`.
      const entrou = await aba.evaluate(async ({ link, alvo }) => {
        const identidade = await identidadeDaPagina();
        await identidade.abrir(link);
        const primeira = await identidade.entrar("dona");
        if (primeira.erro !== "mods-a-aceitar") return { primeira: JSON.parse(JSON.stringify(primeira, (k, v) => (typeof v === "bigint" ? String(v) : v))) };
        const aceita = await identidade.aceitarOsMods(primeira.conjunto);
        if (aceita.ok) depoisDeConectar(aceita.resposta, alvo);
        return { primeira: { erro: primeira.erro, mods: primeira.mods.map((m) => m.id) }, aceita: { ok: aceita.ok, erro: aceita.erro } };
      }, { link: servidor.link, alvo: servidor.endereco });
      assert.ok(entrou.aceita?.ok === true && entrou.primeira.mods.includes(id), `a entrada com o MOD exigido: ${JSON.stringify(entrou)}`);
      const subiu = await esperar(aba, (mid) => {
        const instancia = modsCarregados.get(mid);
        return { pronto: Boolean(instancia) && instancia.estado === ESTADOS_DE_MOD.ativa && instancia.executor?.nome === "navegador", estado: instancia?.estado ?? null, fase: estadoDosMods.get(mid) ?? null };
      }, id, 25_000);
      assert.ok(subiu.pronto, `o MOD ${id} não subiu no executor do navegador: ${JSON.stringify(subiu)}`);

      // A tela da chegada (o primeiro contato): ENTRAR NO SERVIDOR leva à sessão.
      await aba.waitForFunction(() => !$("tela-auth").hidden || !$("tela-sessao").hidden, null, { timeout: 15_000 });
      if (await aba.evaluate(() => !$("tela-auth").hidden)) {
        await aba.waitForFunction(() => !$("auth-botao").disabled && !$("auth-botao").hidden, null, { timeout: 15_000 });
        await aba.click("#auth-botao", { timeout: 8000 });
      }
      await aba.waitForFunction(() => !$("tela-sessao").hidden, null, { timeout: 15_000 });
      // ------------------------------------------------------ os roteiros por MOD
      const roteiro = require(`./mods-oficiais-na-ui/${opcao("--roteiro") ?? curto}.cjs`);
      const ctx = {
        aba, id, falhas, confere, defeito, esperar, doConsole,
        captura: async (nome) => { if (pastaDeCapturas) await aba.screenshot({ path: path.join(pastaDeCapturas, `${curto}-${WEBKIT ? "webkit" : "chromium"}-${nome}.png`) }); },
      };
      await roteiro(ctx);

      // O console: nenhum erro do produto nem do MOD.
      // Um defeito do produto, intermitente (depende de o retrato chegar com o MOD ainda
      // subindo): `modsQueOuvemATela` (ui/base.js) lê `instancia.api` de uma entrada nula.
      const dasInstancias = doConsole.filter((l) => /reading 'api'|evaluating 'instancia\.api'/.test(l));
      defeito(curto, dasInstancias.length === 0, `o desenho do retrato estoura em \`modsQueOuvemATela\` com um MOD ainda subindo (\`instancia.api\` de uma entrada nula em modsCarregados, ui/base.js:3677; ${dasInstancias.length}x): ${dasInstancias[0]?.slice(0, 120)}`);
      const ruins = doConsole.filter((l) => !dasInstancias.includes(l)).filter((l) => !/Failed to load resource|favicon|net::ERR|permissions policy|sem janela do Tauri|RecusaDaPonte/i.test(l) &&
        // O `screenshot` do Playwright no WebKit injeta uma folha que a CSP da origem recusa: linha 0 do documento, e sem `--capturas` ela não aparece: não é do produto.
        !/Refused to apply a stylesheet.* @ \S*(index|executor-de-mod)\.html:0$/.test(l));
      confere(curto, ruins.length === 0, `erros no console: ${JSON.stringify(ruins.slice(0, 6))}`);
    } finally {
      await navegador.close();
      servidorDaPagina.close();
      if (servidor) { servidor.processo.stdin.end(); servidor.processo.kill(); }
      fs.rmSync(copia, { recursive: true, force: true });
    }
    for (const d of defeitos) console.warn(`DEFEITO (do produto, reportado): ${d}`);
    if (falhas.length || (ESTRITO && defeitos.length)) {
      console.error(`mods-oficiais-na-ui (${id}): ${falhas.length + (ESTRITO ? defeitos.length : 0)} falha(s)\n- ${[...falhas, ...(ESTRITO ? defeitos : [])].join("\n- ")}`);
      process.exit(1);
    }
    console.log(`mods-oficiais-na-ui (${id}, ${WEBKIT ? "webkit" : "chromium"}): ok${defeitos.length ? ` (com ${defeitos.length} defeito(s) do produto reportado(s))` : ""}`);
  })().catch((erro) => {
    for (const d of defeitos) console.warn(`DEFEITO (do produto, reportado): ${d}`);
    if (doServidorErr.length) console.error(`stderr do servidor:\n${doServidorErr.join("").slice(-1500)}`);
    if (falhas.length) console.error(`falhas até aqui:\n- ${falhas.join("\n- ")}`);
    console.error(erro);
    process.exit(1);
  });
}
