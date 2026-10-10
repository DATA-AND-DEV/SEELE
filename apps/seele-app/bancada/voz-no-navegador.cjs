// A voz do navegador num Chromium de verdade, com microfone falso e um laço
// local no lugar do servidor (Tarefa 3.3 da F4, lote C).
//
//     node apps/seele-app/bancada/voz-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm
//
// O `.wasm` vem do `cargo xtask web`. A página é a `ui/voz-web.js` e a
// `ui/voz-worklet.js` de verdade, com o `.wasm` de verdade: o microfone falso
// do Chromium entra pelo worklet de captura, passa pelo portão e pelo relógio
// do `.wasm`, pelo `AudioEncoder`, vira datagrama, volta pelo laço com o SSRC
// de outra pessoa (o que o servidor faz), é separado por alça, decodificado,
// e toca pela mistura do `.wasm` dentro do worklet de reprodução.
//
// O que ela prova:
//
// 1. **a voz anda de ponta a ponta**, sem quadro sem carimbo e sem som sem
//    cabeçalho;
// 2. **o mudo** para o envio, e **o isolamento** cala a saída;
// 3. **os controles sobrevivem a uma reconexão** (o Step 1 da Tarefa 3.3):
//    mudo, isolamento, modo e régua, vestidos de novo num transporte novo com
//    outro SSRC. Os ganhos são por alça e recomeçam com a recepção; o ganho
//    por pessoa chega com a sessão (2.6);
// 4. **o `messageerror` do worklet é dito**: um `WebAssembly.Module` mandado à
//    porta, que o Chromium recusa, volta como falha, e não como silêncio.
//
// E mede quanto o `.wasm` leva para compilar dentro do worklet.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const [caminhoDoWasm] = process.argv.slice(2);
if (!caminhoDoWasm || !fs.existsSync(caminhoDoWasm)) {
  console.error("uso: node apps/seele-app/bancada/voz-no-navegador.cjs <seele_web.wasm>  (o `cargo xtask web` o compila)");
  process.exit(2);
}

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("voz-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};

// **Uma cópia**, e não a `ui/`: um arquivo de bancada esquecido lá iria para o
// instalador (`tools/carga-da-casca.py` diz o mesmo).
const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "voz-no-navegador-"));
for (const nome of ["voz-web.js", "voz-worklet.js"]) {
  fs.copyFileSync(path.join(raiz, "ui", nome), path.join(pasta, nome));
}
fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));

fs.writeFileSync(
  path.join(pasta, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>voz</title>
<button id="ligar" type="button">ligar</button>
<script src="voz-web.js"></script>
<script src="bancada.js"></script>`,
);

// O que a página da bancada faz: abre a voz no clique e entra num laço local.
fs.writeFileSync(
  path.join(pasta, "bancada.js"),
  `"use strict";
// O servidor de mentira: devolve cada datagrama com o SSRC de outra pessoa,
// como o servidor de verdade repassa a voz de quem fala.
function lacoLocal(outro) {
  let ouvinte = null;
  return {
    enviar(datagrama) {
      const copia = datagrama.slice();
      new DataView(copia.buffer).setUint32(1, outro);
      queueMicrotask(() => ouvinte && ouvinte(copia));
      return true;
    },
    ouvir(fn) {
      ouvinte = fn;
      return () => { if (ouvinte === fn) ouvinte = null; };
    },
  };
}
window.bancada = { eventos: [], pronta: null, voz: null, bytes: null, lacoLocal };
const carregado = fetch("seele_web.wasm")
  .then((r) => r.arrayBuffer())
  .then(async (bytes) => ({ bytes, instancia: (await WebAssembly.instantiate(bytes, {})).instance }));
document.getElementById("ligar").addEventListener("click", () => {
  window.bancada.pronta = (async () => {
    const { bytes, instancia } = await carregado;
    window.bancada.bytes = bytes;
    const voz = criarVozDoNavegador({
      wasm: instancia.exports,
      bytesDoWasm: bytes,
      aoAcontecer: (evento) => window.bancada.eventos.push(evento),
    });
    window.bancada.voz = voz;
    await voz.ligar();
    voz.modo(2);
    voz.entrar(lacoLocal(900), 77);
    return voz.reproducaoPronta();
  })();
});

// O messageerror: um nó de reprodução à parte, sobre o mesmo worklet, recebe
// um WebAssembly.Module pela porta. O Chromium o recusa, e o worklet tem de
// dizer que recusou.
window.bancada.provarOMessageerror = async () => {
  const ctx = new AudioContext();
  await ctx.audioWorklet.addModule("voz-worklet.js");
  const no = new AudioWorkletNode(ctx, "seele-reproducao-da-voz", {
    numberOfInputs: 0,
    outputChannelCount: [2],
    processorOptions: { wasm: window.bancada.bytes.slice(0) },
  });
  no.connect(ctx.destination);
  const ouvido = new Promise((resolve) => {
    no.port.onmessage = (e) => { if (e.data.tipo === "falha") resolve(e.data.texto); };
    setTimeout(() => resolve(null), 3000);
  });
  no.port.postMessage({ tipo: "chegou", modulo: await WebAssembly.compile(window.bancada.bytes) });
  const texto = await ouvido;
  await ctx.close();
  return texto;
};
`,
);

const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; frame-ancestors 'none'";
const TIPOS = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".wasm": "application/wasm" };

function servir() {
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo)) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, {
      "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream",
      "content-security-policy": CSP,
    });
    resposta.end(fs.readFileSync(arquivo));
  });
}

const esperar = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function principal() {
  // O Chromium primeiro: sem ele, nenhum servidor fica de pé segurando o
  // processo.
  const navegador = await chromium().launch({
    args: ["--mute-audio", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  });
  const servidor = servir();
  await new Promise((resolve) => servidor.listen(0, "127.0.0.1", resolve));
  const endereco = `http://127.0.0.1:${servidor.address().port}/`;
  try {
    const aba = await navegador.newPage();
    const doConsole = [];
    aba.on("pageerror", (erro) => doConsole.push(`pageerror: ${erro.message}`));
    aba.on("console", (m) => m.type() === "error" && doConsole.push(m.text()));
    await aba.goto(endereco);
    await aba.click("#ligar");
    const pronta = await aba.evaluate(() => window.bancada.pronta);
    confere("abrir", pronta === true, `o .wasm não ficou pronto no worklet: ${JSON.stringify(await aba.evaluate(() => window.bancada.eventos))}`);
    const contas = () => aba.evaluate(() => window.bancada.voz.contas());
    const compilou = await aba.evaluate(() => window.bancada.eventos.find((e) => e.tipo === "reproducao-pronta"));

    // 1. De ponta a ponta.
    await esperar(3000);
    const c1 = await contas();
    const p1 = c1.pagina;
    const fonte1 = c1.reproducao && c1.reproducao.contas.fontes["1"];
    confere("ponta a ponta", p1.capturados >= 100, `o microfone entregou ${p1.capturados} quadros em 3 s`);
    confere("ponta a ponta", p1.enviados >= p1.noPortao - 5, `com o microfone aberto, ${p1.enviados} enviados de ${p1.noPortao} quadros que passaram pelo portão: ${JSON.stringify(p1)}`);
    // O `usedtx` do Chromium segurava 17 quadros (340 ms) dentro do codificador.
    confere("ponta a ponta", p1.noCodificador <= 3, `${p1.noCodificador} quadros presos no codificador: a voz sai ${p1.noCodificador * 20} ms atrasada`);
    confere("ponta a ponta", p1.chegados >= p1.enviados - 5, `${p1.chegados} chegados de ${p1.enviados} enviados pelo laço`);
    confere("ponta a ponta", p1.decodificados >= p1.chegados - 5, `${p1.decodificados} decodificados de ${p1.chegados} chegados`);
    confere("ponta a ponta", p1.semCarimbo === 0, `${p1.semCarimbo} pedaços de Opus saíram sem o carimbo do quadro`);
    confere("ponta a ponta", p1.semCabecalho === 0, `${p1.semCabecalho} quadros decodificados sem o cabeçalho`);
    confere("ponta a ponta", fonte1 && fonte1.tocados >= 50, `a alça 1 tocou ${fonte1 && fonte1.tocados} quadros: ${JSON.stringify(c1.reproducao)}`);
    confere("ponta a ponta", c1.reproducao && c1.reproducao.blocosComSom > 0, "a mistura não tocou som nenhum");
    confere("ponta a ponta", p1.falhas.length === 0, `falhas: ${JSON.stringify(p1.falhas)}`);

    // 2. O mudo para o envio; o isolamento cala a saída.
    await aba.evaluate(() => window.bancada.voz.mudo(true));
    await esperar(300);
    const antesDoMudo = (await contas()).pagina.enviados;
    await esperar(1000);
    const noMudo = (await contas()).pagina.enviados;
    confere("mudo", noMudo === antesDoMudo, `no mudo saíram ${noMudo - antesDoMudo} datagramas`);
    await aba.evaluate(() => window.bancada.voz.mudo(false));

    await aba.evaluate(() => window.bancada.voz.isolar(true));
    await esperar(300);
    const antesDoIsolamento = (await contas()).reproducao.blocosComSom;
    await esperar(1000);
    const noIsolamento = (await contas()).reproducao.blocosComSom;
    confere("isolamento", noIsolamento === antesDoIsolamento, `isolado, ${noIsolamento - antesDoIsolamento} blocos tocaram som`);
    await aba.evaluate(() => window.bancada.voz.isolar(false));
    await esperar(1000);
    const depoisDoIsolamento = (await contas()).reproducao.blocosComSom;
    confere("isolamento", depoisDoIsolamento > noIsolamento, "depois do isolamento a mistura não voltou a tocar");

    // 3. Os controles sobrevivem a uma reconexão.
    //
    // **Um de cada vez**, porque um esconde o outro: com o mudo ligado nada
    // chega, e uma mistura calada por falta de voz passaria por isolamento.
    await aba.evaluate(() => {
      const voz = window.bancada.voz;
      voz.mudo(true);
      voz.isolar(true);
      voz.modo(1);
      voz.regua(-50000);
      voz.reconectar(window.bancada.lacoLocal(901), 78);
    });
    await esperar(300);
    const r0 = await contas();
    await esperar(1000);
    const r1 = await contas();
    confere("reconexão", r1.pagina.enviados === r0.pagina.enviados, `o mudo não atravessou a reconexão: ${r1.pagina.enviados - r0.pagina.enviados} datagramas saíram`);
    confere("reconexão", r1.portao && Math.abs(r1.portao.abertura_dbfs + 50) < 0.01, `a régua não atravessou a reconexão: ${JSON.stringify(r1.portao)}`);
    confere("reconexão", r1.controles.modo === 1, `o modo guardado é ${r1.controles.modo}`);

    // Sem o mudo, a voz volta a andar pelo transporte novo, e o isolamento,
    // que ninguém desligou, tem de continuar calando a saída.
    await aba.evaluate(() => {
      const voz = window.bancada.voz;
      voz.mudo(false);
      voz.modo(2);
    });
    await esperar(500);
    const r2 = await contas();
    await esperar(1000);
    const r3 = await contas();
    confere("reconexão", r3.pagina.enviados > r2.pagina.enviados + 20, "depois da reconexão a voz não voltou a sair");
    confere("reconexão", r3.pagina.decodificados > r2.pagina.decodificados + 20, "depois da reconexão a voz não voltou a chegar");
    confere("reconexão", r3.reproducao.blocosComSom === r2.reproducao.blocosComSom, `o isolamento não atravessou a reconexão: ${r3.reproducao.blocosComSom - r2.reproducao.blocosComSom} blocos tocaram som`);

    await aba.evaluate(() => window.bancada.voz.isolar(false));
    await esperar(1000);
    const r4 = await contas();
    confere("reconexão", r4.reproducao.blocosComSom > r3.reproducao.blocosComSom, "depois da reconexão a mistura não voltou a tocar");
    confere("reconexão", r4.reproducao.contas.fontes["1"] !== undefined, `a recepção nova não recomeçou as alças: ${JSON.stringify(Object.keys(r4.reproducao.contas.fontes))}`);
    confere("reconexão", r4.pagina.falhas.length === 0, `falhas: ${JSON.stringify(r4.pagina.falhas)}`);

    // 4. O messageerror é dito.
    const dito = await aba.evaluate(() => window.bancada.provarOMessageerror());
    confere("messageerror", typeof dito === "string" && dito.includes("messageerror"), `o worklet ficou calado diante de um Module na porta: ${dito}`);

    confere("console", doConsole.length === 0, doConsole.join(" | "));
    const final = await contas();
    console.log(
      `voz-no-navegador: o .wasm compilou no worklet em ${compilou ? compilou.compilouEmMs.toFixed(1) : "?"} ms; ` +
        `${final.pagina.enviados} enviados, ${final.pagina.decodificados} decodificados, ` +
        `${final.reproducao.quadros} quadros misturados, ${final.reproducao.contas.fades} fades`,
    );
  } finally {
    await navegador.close();
    servidor.close();
  }
}

principal()
  .catch((erro) => falhas.push(`a bancada estourou: ${erro && erro.stack ? erro.stack : erro}`))
  .finally(() => {
    terminou = true;
    fs.rmSync(pasta, { recursive: true, force: true });
    if (falhas.length) {
      for (const f of falhas) console.error(`FALHOU — ${f}`);
      process.exitCode = 1;
    } else {
      console.log("voz-no-navegador: ok");
    }
  });
