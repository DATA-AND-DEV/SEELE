// **A página entra na sala pessoal de quem tem o app, pelo código** (ADR 0068),
// de ponta a ponta e sem nada de mentira no meio: a página inteira (`ui/`) num
// navegador de verdade, um `seele-encontro` de verdade (o ENC/3 e o
// `POST /sinal/1`) e uma sala pessoal de verdade (`Hospedagem::iniciar_sala`,
// pelo `crates/seele-server/examples/sala_da_bancada.rs`).
//
//     node apps/seele-app/bancada/sala-pelo-codigo.cjs \
//       target/debug/seele-encontro \
//       target/debug/examples/sala_da_bancada \
//       target/pacote-web-build/wasm32-unknown-unknown/release/seele_web.wasm \
//       [--webkit]
//
// O que se prova:
// 1. **a entrada**: `connect("@CÓDIGO")` pela ponte acha a sala pelo sinal,
//    fecha o DTLS com a impressão que o ponto disse, confere a apresentação
//    contra o código no `.wasm`, toca (a sala vê o código DA PÁGINA), é
//    atendida e abre uma sessão comum, que a ponte lista como ligação;
// 2. **o pino**: a chave de quem atendeu fica guardada no contato, e a segunda
//    ligação entra como «conhecida»;
// 3. **um código sem sala** dá `NaoLocalizou { NinguemMora }`, que é por onde a
//    casca recua para a conversa direta;
// 4. **um ponto de antes da 1.0** (um proxy que tira a impressão do `AQUI`) dá
//    `NaoLocalizou { SemImpressao }`, e nada é discado;
// 5. **um ponto hostil** (um proxy que troca a impressão) não entra: a conexão
//    segura não se verifica, a recusa é `CodigoNaoBate`, e a sala não toca;
// 6. **o pino de outra chave** não entra: `SePassouPorCodigo`, antes do
//    `Hello`, e a sala não toca;
// 7. **pela casca**, o `irParaOServidor("@CÓDIGO")` de verdade chega à sessão.
//
// **Não mede NAT.** Tudo é loopback: não há tradução, mapeamento nem filtro.
// O sinal vai por HTTP em claro, e não por HTTPS atrás de um proxy.

"use strict";

const dgram = require("node:dgram");
const fs = require("node:fs");
const http = require("node:http");
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");
const { spawn } = require("node:child_process");
const { chromium, webkit } = require("./playwright.cjs");

const [encontro, sala, wasm] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
if (![encontro, sala, wasm].every((caminho) => caminho && fs.existsSync(caminho))) {
  console.error(
    "uso: node apps/seele-app/bancada/sala-pelo-codigo.cjs <seele-encontro> <sala_da_bancada> <seele_web.wasm> [--webkit]\n" +
      "  (cargo build -p seele-encontro; cargo build -p seele-server --example sala_da_bancada; cargo xtask web)",
  );
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");
const TIPOS = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm", ".svg": "image/svg+xml", ".png": "image/png", ".json": "application/json", ".woff2": "font/woff2" };

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
  else console.log(`PASS ${caso}`);
};

function portaLivre(tipo) {
  return new Promise((ok) => {
    if (tipo === "udp") {
      const s = dgram.createSocket("udp4");
      s.bind(0, "127.0.0.1", () => { const { port } = s.address(); s.close(() => ok(port)); });
    } else {
      const s = net.createServer();
      s.listen(0, "127.0.0.1", () => { const { port } = s.address(); s.close(() => ok(port)); });
    }
  });
}

/** A `ui/` com o `.wasm` ao lado. Sem os cabeçalhos da origem: o sinal daqui é HTTP em claro. */
function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "sala-pelo-codigo-"));
  fs.cpSync(UI, pasta, { recursive: true });
  fs.copyFileSync(wasm, path.join(pasta, "seele_web.wasm"));
  const servidor = http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream", "cache-control": "no-store" });
    resposta.end(fs.readFileSync(arquivo));
  });
  servidor.pasta = pasta;
  return servidor;
}

function esperarLinha(processo, padrao) {
  return new Promise((ok, falha) => {
    const ouvir = (dados) => { if (padrao.test(dados.toString())) ok(); };
    processo.stdout?.on("data", ouvir);
    processo.stderr?.on("data", ouvir);
    processo.on("exit", (codigo) => falha(new Error(`${processo.spawnfile} saiu com ${codigo}`)));
  });
}

/** A sala de verdade: a primeira linha é o código, e cada toque é uma linha. */
function subirASala(ponto, extra = []) {
  const processo = spawn(sala, [ponto, ...extra], { stdio: ["pipe", "pipe", "inherit"] });
  const toques = [];
  return new Promise((resolver, rejeitar) => {
    const linhas = readline.createInterface({ input: processo.stdout });
    let primeira = true;
    linhas.on("line", (linha) => {
      const dito = JSON.parse(linha);
      if (primeira) { primeira = false; resolver({ processo, toques, ...dito }); } else toques.push(dito);
    });
    processo.once("exit", (codigo) => rejeitar(new Error(`a sala da bancada saiu com ${codigo} antes de dizer o código`)));
  });
}

/**
 * Um ponto de mentira **na frente do sinal de verdade**: repassa o `CHAMA` e
 * mexe na resposta. `mexer` recebe a linha do `AQUI` e devolve a que a página
 * lê. É o que um ponto de antes da 1.0 (sem a impressão) ou um ponto hostil
 * (com outra) fariam.
 */
function proxyDoSinal(portaDoSinal, origem, mexer) {
  const servidor = http.createServer((pedido, resposta) => {
    const cors = { "access-control-allow-origin": origem, "vary": "Origin" };
    if (pedido.method === "OPTIONS") {
      resposta.writeHead(204, { ...cors, "access-control-allow-methods": "POST", "access-control-allow-headers": "Content-Type" }).end();
      return;
    }
    const pedacos = [];
    pedido.on("data", (p) => pedacos.push(p));
    pedido.on("end", async () => {
      try {
        const corpo = Buffer.concat(pedacos);
        const real = await fetch(`http://127.0.0.1:${portaDoSinal}/sinal/1`, { method: "POST", body: corpo, headers: { "content-type": "text/plain", origin: origem } });
        const texto = await real.text();
        resposta.writeHead(real.status, { ...cors, "content-type": "text/plain; charset=utf-8" }).end(mexer(texto));
      } catch (falha) {
        resposta.writeHead(502, cors).end(String(falha));
      }
    });
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

const invocar = (aba, comando, args) =>
  aba.evaluate(async ({ comando, args }) => {
    try { return { ok: await window.__TAURI__.core.invoke(comando, args) }; } catch (erro) {
      return { erro: erro instanceof Error ? String(erro) : erro };
    }
  }, { comando, args });

(async () => {
  const portaUdp = await portaLivre("udp");
  const portaTcp = await portaLivre("tcp");
  const pagina = servirAPagina();
  await new Promise((ok) => pagina.listen(0, "127.0.0.1", ok));
  const origem = `http://127.0.0.1:${pagina.address().port}`;
  const ponto = `127.0.0.1:${portaUdp}`;
  const processos = [];
  const proxies = [];
  let navegador = null;
  try {
    const doPonto = spawn(encontro, ["--porta", String(portaUdp), "--rede-local", "--sinal", `127.0.0.1:${portaTcp}`, "--origem", origem], { stdio: ["ignore", "pipe", "pipe"] });
    processos.push(doPonto);
    await esperarLinha(doPonto, /sinal do navegador atendendo/);
    const daSala = await subirASala(ponto);
    processos.push(daSala.processo);
    const CODIGO = daSala.codigo;

    navegador = process.argv.includes("--webkit")
      ? await webkit().launch()
      : await chromium().launch({ args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", "--autoplay-policy=no-user-gesture-required"] });
    const contexto = await navegador.newContext(process.argv.includes("--webkit") ? {} : { permissions: ["microphone"] });
    // **Nada daqui fala com a internet.** A página, ao abrir, registra a caixa
    // da conversa direta no ponto de encontro do produto; numa bancada isso
    // seria um pedido à VPS de produção. Só o loopback passa.
    await contexto.route("**/*", (rota) => (/^https?:\/\/(127\.0\.0\.1|localhost)(:\d+)?\//.test(rota.request().url()) ? rota.continue() : rota.abort()));
    const aba = await contexto.newPage();
    const doConsole = [];
    aba.on("console", (m) => doConsole.push(`${m.type()}: ${m.text()}`));
    aba.on("pageerror", (e) => doConsole.push(`pageerror: ${e.message}`));
    await aba.addInitScript(() => { try { localStorage.setItem("seele.casca", "larga"); } catch {} });
    await aba.goto(`${origem}/index.html`);
    await aba.waitForFunction(() => typeof alvoDaSalaPessoalWeb === "function" && typeof window.__TAURI__?.core?.invoke === "function");
    // O ponto da bancada no lugar do de produção: o alvo é o de verdade, com o
    // ponto e o sinal trocados. `sinalDaBancada` muda entre os casos.
    await aba.evaluate(({ ponto, sinal }) => {
      window.sinalDaBancada = sinal;
      window.etapasDaBancada = [];
      window.__TAURI__.event.listen("seele://event", ({ payload }) => {
        const evento = payload && typeof payload === "object" && "evento" in payload ? payload.evento : payload;
        if (evento?.ConnectStageChanged) window.etapasDaBancada.push(evento.ConnectStageChanged.stage);
      });
      const original = alvoDaSalaPessoalWeb;
      window.alvoDaSalaPessoalWeb = async (escrito) => ({ ...(await original(escrito)), bilhete: { ponto }, urlDoSinal: () => window.sinalDaBancada });
    }, { ponto, sinal: `http://127.0.0.1:${portaTcp}/sinal/1` });
    const meu = await aba.evaluate(() => meuCodigoPessoalWeb());
    const pedido = { server: `@${CODIGO}`, nickname: "pagina", audio: true, joinSecret: null, tentativa: 1 };
    const sair = async () => {
      const abertas = (await invocar(aba, "sessoes", {})).ok ?? [];
      for (const s of abertas) await invocar(aba, "encerrar_sessao", { carimbo: s.carimbo });
      await aba.waitForTimeout(400);
    };

    // ------------------------------------------------------------ 1. a entrada
    const entrada = await invocar(aba, "connect", pedido);
    confere("1 a página entra na sala pelo código", entrada.ok?.snapshot?.server === "dona",
      `não entrou: ${JSON.stringify(entrada.erro ?? entrada.ok?.snapshot?.server)} · console: ${doConsole.slice(-6).join(" | ")}`);
    confere("1 a sala tocou com o código da página", daSala.toques.length === 1 && daSala.toques[0].tocou === meu && daSala.toques[0].apelido === "pagina",
      `os toques: ${JSON.stringify(daSala.toques)} (o código da página é ${meu})`);
    const etapas = await aba.evaluate(() => window.etapasDaBancada);
    confere("1 a casca ouviu que tocava", etapas.some((e) => e?.Tocando), `as etapas: ${JSON.stringify(etapas)}`);
    const veredito = entrada.ok?.veredito?.FirstContactVerified?.fingerprint;
    confere("1 o veredito é o primeiro contato, com a impressão da chave", /^[0-9a-f]{64}$/.test(veredito ?? ""), `o veredito: ${JSON.stringify(entrada.ok?.veredito)}`);
    const sessoes = (await invocar(aba, "sessoes", {})).ok ?? [];
    confere("1 a ponte lista a sessão como ligação", sessoes.length === 1 && sessoes[0].ligacao === CODIGO && sessoes[0].enlace === "online",
      `as sessões: ${JSON.stringify(sessoes)}`);
    const sentar = entrada.ok ? await invocar(aba, "enter_voice_room", { voiceRoom: entrada.ok.snapshot.voice_rooms[0].id, senha: null }) : { erro: "sem-sessao" };
    await aba.waitForTimeout(800);
    const depois = (await invocar(aba, "snapshot", {})).ok;
    confere("1 a página senta na sala de voz da ligação", !sentar.erro && depois?.voice_rooms?.[0]?.occupied_by_us === true,
      `sentar: ${JSON.stringify(sentar.erro)} · sala: ${JSON.stringify(depois?.voice_rooms?.[0])}`);

    // ---------------------------------------------------------------- 2. o pino
    const contato = await aba.evaluate((codigo) => noBancoPessoalWeb("contatos", "readonly", (loja) => loja.get(codigo)), CODIGO);
    const doPino = contato?.chavePublica ? await aba.evaluate((hex) => operarCodigoPessoalWeb({ acao: "codigo_da_chave", chave: hex.match(/../g).map((p) => parseInt(p, 16)) }), contato.chavePublica) : null;
    confere("2 a chave de quem atendeu ficou guardada no contato", doPino === CODIGO, `o contato: ${JSON.stringify(contato)}`);
    await sair();
    const segunda = await invocar(aba, "connect", { ...pedido, tentativa: 2 });
    confere("2 a segunda ligação entra, e a pessoa já é conhecida", segunda.ok?.veredito === "Known",
      `a segunda: ${JSON.stringify(segunda.erro ?? segunda.ok?.veredito)}`);
    confere("2 quem voltou em um minuto não toca de novo", daSala.toques.length === 1, `os toques: ${JSON.stringify(daSala.toques)}`);
    await sair();

    // ------------------------------------------------------- 3. um código sem sala
    const semSala = await aba.evaluate(async () => {
      // O código de uma chave que ninguém registrou.
      const codigo = await operarCodigoPessoalWeb({ acao: "codigo_da_chave", chave: Array(32).fill(9) });
      try { return { ok: await window.__TAURI__.core.invoke("connect", { server: `@${codigo}`, nickname: "pagina", audio: true, joinSecret: null, tentativa: 3 }) }; } catch (erro) { return { erro }; }
    });
    confere("3 um código sem sala é «ninguém mora»", semSala.erro?.NaoLocalizou?.motivo === "NinguemMora", `veio: ${JSON.stringify(semSala.erro ?? "entrou")}`);

    // ----------------------------------------------- 4. um ponto de antes da 1.0
    const antigo = await proxyDoSinal(portaTcp, origem, (texto) => texto.replace(/^(SEELE-SINAL\/1 AQUI \S+ \S+) \S+$/, "$1"));
    proxies.push(antigo);
    await aba.evaluate((url) => { window.sinalDaBancada = url; }, `http://127.0.0.1:${antigo.address().port}/sinal/1`);
    const antes = daSala.toques.length;
    const semImpressao = await invocar(aba, "connect", { ...pedido, tentativa: 4 });
    confere("4 um ponto de antes da 1.0 dá «sem impressão»", semImpressao.erro?.NaoLocalizou?.motivo === "SemImpressao", `veio: ${JSON.stringify(semImpressao.erro ?? "entrou")}`);
    confere("4 sem a impressão, a sala não tocou", daSala.toques.length === antes, `os toques: ${JSON.stringify(daSala.toques)}`);

    // ------------------------------------------------------- 5. um ponto hostil
    const hostil = await proxyDoSinal(portaTcp, origem, (texto) => texto.replace(/^(SEELE-SINAL\/1 AQUI \S+ \S+) \S+$/, `$1 ${Buffer.alloc(32, 0x5a).toString("base64url")}`));
    proxies.push(hostil);
    await aba.evaluate((url) => { window.sinalDaBancada = url; }, `http://127.0.0.1:${hostil.address().port}/sinal/1`);
    const trocada = await invocar(aba, "connect", { ...pedido, tentativa: 5 });
    confere("5 a impressão trocada pelo ponto não entra", trocada.erro?.CodigoNaoBate?.esperado === CODIGO && trocada.erro.CodigoNaoBate.veio === null,
      `veio: ${JSON.stringify(trocada.erro ?? "entrou")}`);
    confere("5 com a impressão trocada, a sala não tocou", daSala.toques.length === antes, `os toques: ${JSON.stringify(daSala.toques)}`);
    confere("5 nenhuma sessão ficou pendurada", ((await invocar(aba, "sessoes", {})).ok ?? []).length === 0, "a ponte ainda lista uma sessão");

    // ------------------------------------------------- 6. o pino de outra chave
    await aba.evaluate((url) => { window.sinalDaBancada = url; }, `http://127.0.0.1:${portaTcp}/sinal/1`);
    await aba.evaluate((codigo) => noBancoPessoalWeb("contatos", "readwrite", (loja) => {
      const pedido = loja.get(codigo);
      pedido.onsuccess = () => loja.put({ ...pedido.result, chavePublica: "07".repeat(32) });
      return pedido;
    }), CODIGO);
    const outraChave = await invocar(aba, "connect", { ...pedido, tentativa: 6 });
    confere("6 o pino de outra chave não entra", outraChave.erro?.SePassouPorCodigo?.codigo === CODIGO, `veio: ${JSON.stringify(outraChave.erro ?? "entrou")}`);
    confere("6 com o pino de outra chave, a sala não tocou (nada foi dito a ela)", daSala.toques.length === antes, `os toques: ${JSON.stringify(daSala.toques)}`);
    await aba.evaluate(({ codigo, chave }) => noBancoPessoalWeb("contatos", "readwrite", (loja) => {
      const pedido = loja.get(codigo);
      pedido.onsuccess = () => loja.put({ ...pedido.result, chavePublica: chave });
      return pedido;
    }), { codigo: CODIGO, chave: contato?.chavePublica });

    // ----------------------------------------------------------- 7. pela casca
    // Numa carga nova da página: os casos de cima falaram com a ponte por baixo
    // da casca, e a vista dela não acompanhou. A identidade e o contato são os
    // mesmos (o IndexedDB é do contexto).
    await aba.close();
    const casca = await contexto.newPage();
    const daCasca = [];
    casca.on("console", (m) => daCasca.push(`${m.type()}: ${m.text()}`));
    casca.on("pageerror", (e) => daCasca.push(`pageerror: ${e.message}`));
    await casca.addInitScript(() => { try { localStorage.setItem("seele.casca", "larga"); } catch {} });
    await casca.goto(`${origem}/index.html`);
    await casca.waitForFunction(() => typeof alvoDaSalaPessoalWeb === "function" && typeof irParaOServidor === "function" && typeof window.__TAURI__?.core?.invoke === "function");
    await casca.waitForTimeout(800);
    await casca.evaluate(({ ponto, sinal }) => {
      const original = alvoDaSalaPessoalWeb;
      window.alvoDaSalaPessoalWeb = async (escrito) => ({ ...(await original(escrito)), bilhete: { ponto }, urlDoSinal: () => sinal });
    }, { ponto, sinal: `http://127.0.0.1:${portaTcp}/sinal/1` });
    await casca.evaluate((codigo) => { irParaOServidor(`@${codigo}`, "pagina", null, "Dona", null, true); }, CODIGO);
    // De meio em meio segundo, e daqui: um `waitForFunction` que pergunta à
    // ponte a cada quadro deixa a página sem vez para os próprios relógios (a
    // coleta do ICE não anda, e a entrada fica parada em «tentando»).
    for (let espera = 0; espera < 60; espera += 1) {
      await casca.waitForTimeout(500);
      if (((await invocar(casca, "sessoes", {})).ok ?? []).length === 1) break;
      if (await casca.evaluate(() => !document.getElementById("boot-erro").hidden)) break;
    }
    await casca.waitForTimeout(1200);
    const pelaCasca = (await invocar(casca, "sessoes", {})).ok ?? [];
    const naTela = await casca.evaluate(() => ({
      erro: document.getElementById("boot-erro").hidden ? null : document.getElementById("boot-erro").textContent,
      telas: [...document.querySelectorAll("section.tela")].filter((t) => !t.hidden).map((t) => t.id),
      vista: document.getElementById("tela-sessao").dataset.vista ?? null,
    }));
    confere("7 pela casca, ligar para o código abre a sessão da ligação",
      pelaCasca.length === 1 && pelaCasca[0].ligacao === CODIGO && naTela.erro === null && naTela.vista === "ligacao",
      `as sessões: ${JSON.stringify(pelaCasca)} · a tela: ${JSON.stringify(naTela)} · console: ${daCasca.slice(-12).join(" | ")}`);
    for (const s of pelaCasca) await invocar(casca, "encerrar_sessao", { carimbo: s.carimbo });
  } catch (falha) {
    falhas.push(`a bancada estourou: ${falha?.stack ?? falha}`);
  } finally {
    await navegador?.close().catch(() => {});
    for (const proxy of proxies) proxy.close();
    for (const processo of processos) { try { processo.stdin?.end(); processo.kill(); } catch {} }
    pagina.close();
    fs.rmSync(pagina.pasta, { recursive: true, force: true });
  }
  if (falhas.length) {
    console.error(falhas.map((f) => `FALHOU ${f}`).join("\n"));
    process.exit(1);
  }
  console.log("ok: a página entra na sala pessoal pelo código, e não entra em quem não é o dono dele");
})();
