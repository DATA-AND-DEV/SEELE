// A recepção medida pela página, num Chromium de verdade: o jitter e a perda
// da voz que ela ouve, e o relato que vai ao servidor (protocolo 13).
//
//     node apps/seele-app/bancada/recepcao-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm
//
// O `.wasm` vem do `cargo xtask web`. A página é a `ui/voz-web.js` e a
// `ui/voz-worklet.js` de verdade, com o `.wasm` de verdade: o microfone falso
// do Chromium passa pelo portão, pelo relógio e pelo `AudioEncoder`, vira
// datagrama, e volta por um laço local com o SSRC de outra pessoa — **menos um
// datagrama em cada dez**, que o laço joga fora. O que chega é decodificado e
// toca pelos buffers de jitter do `.wasm` dentro do worklet, que é onde a
// perda e o jitter são contados (`Reproducao::recepcao`, pela janela do
// `seele_voz::recepcao`, a do desktop).
//
// O que ela prova, e é o que os testes de Rust não alcançam — a fiação entre a
// página, a porta do worklet e a segunda instância do `.wasm`:
//
// 1. **a página mede**: pedida a cada 250 ms, como a volta da medida da sessão
//    pede, a recepção volta como o evento `recepcao`, com a perda perto dos
//    10% que o laço jogou fora e o jitter em milissegundos — e não o zero fixo
//    de antes;
// 2. **o relato sai uma vez por segundo**, e não a cada pedido, com as
//    unidades do protocolo (a perda de 0 a 1), e é o da janela;
// 3. **a janela desliza**: com o laço limpo, a perda relatada volta a zero em
//    poucos segundos;
// 4. **fora da sala nada é medido**: depois do `sair`, o pedido diz que não
//    há o que medir, e nenhum evento chega — nem o de um pedido de antes;
// 5. **uma voz nova não herda a medida da velha**, nem a resposta de um
//    pedido que estava no ar quando a velha saiu.
//
// O que ela **não** prova: que o relato chega ao servidor (isso é da
// `sessao-no-navegador.cjs`, com o servidor de verdade), nem perda de rede de
// verdade — o laço é local, e o jitter aqui é o do agendador do Chromium.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const [caminhoDoWasm] = process.argv.slice(2);
if (!caminhoDoWasm || !fs.existsSync(caminhoDoWasm)) {
  console.error("uso: node apps/seele-app/bancada/recepcao-no-navegador.cjs <seele_web.wasm>  (o `cargo xtask web` o compila)");
  process.exit(2);
}

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("recepcao-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};

// **Uma cópia**, e não a `ui/`: um arquivo de bancada esquecido lá iria para o
// instalador.
const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "recepcao-no-navegador-"));
for (const nome of ["voz-web.js", "voz-worklet.js"]) {
  fs.copyFileSync(path.join(raiz, "ui", nome), path.join(pasta, nome));
}
fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));

fs.writeFileSync(
  path.join(pasta, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>recepção</title>
<button id="ligar" type="button">ligar</button>
<script src="voz-web.js"></script>
<script src="bancada.js"></script>`,
);

fs.writeFileSync(
  path.join(pasta, "bancada.js"),
  `"use strict";
// O servidor de mentira: devolve cada datagrama com o SSRC de outra pessoa, e
// joga fora um em cada \`umEm\` (zero é nenhum).
const laco = { umEm: 10, passaram: 0, perdidos: 0 };
function lacoLocal(outro) {
  let ouvinte = null;
  let n = 0;
  return {
    enviar(datagrama) {
      n += 1;
      if (laco.umEm && n % laco.umEm === 0) { laco.perdidos += 1; return true; }
      laco.passaram += 1;
      const copia = datagrama.slice();
      new DataView(copia.buffer).setUint32(1, outro);
      queueMicrotask(() => { if (ouvinte) ouvinte(copia); });
      return true;
    },
    ouvir(fn) {
      ouvinte = fn;
      return () => { if (ouvinte === fn) ouvinte = null; };
    },
  };
}
window.bancada = { medidas: [], falhas: [], pedidos: 0, atendidos: 0, voz: null, laco, lacoLocal, pronta: null };
const carregado = fetch("seele_web.wasm")
  .then((r) => r.arrayBuffer())
  .then(async (bytes) => ({ bytes, instancia: (await WebAssembly.instantiate(bytes, {})).instance }));
document.getElementById("ligar").addEventListener("click", () => {
  window.bancada.pronta = (async () => {
    const { bytes, instancia } = await carregado;
    const voz = criarVozDoNavegador({
      wasm: instancia.exports,
      bytesDoWasm: bytes,
      aoAcontecer: (evento) => {
        if (evento.tipo === "recepcao") window.bancada.medidas.push({ quando: performance.now(), ...evento.medida });
        if (evento.tipo === "falha") window.bancada.falhas.push(evento);
      },
    });
    window.bancada.voz = voz;
    await voz.ligar();
    // Aberto: a voz sai sem parar, e a janela tem fala para contar.
    voz.modo(2);
    voz.entrar(lacoLocal(900), 77);
    const pronta = await voz.reproducaoPronta();
    // A volta da medida da sessão (\`VOLTA_DA_MEDIDA_MS\`, na \`sessao-web.js\`).
    setInterval(() => {
      window.bancada.pedidos += 1;
      if (voz.pedirARecepcao()) window.bancada.atendidos += 1;
    }, 250);
    return pronta;
  })();
});
`,
);

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
      ...cabecalhosDaOrigem(`/${nome}`),
      "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream",
    });
    resposta.end(fs.readFileSync(arquivo));
  });
}

const esperar = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function principal() {
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
    confere("abrir", pronta === true, "o .wasm não ficou pronto no worklet");
    const lido = () => aba.evaluate(() => ({ ...window.bancada, voz: undefined, lacoLocal: undefined, pronta: undefined }));
    const relatosDe = (medidas) => medidas.filter((m) => m.relato).map((m) => ({ quando: m.quando, ...m.relato }));

    // 1 e 2. Dez por cento de perda, por nove segundos.
    await esperar(9000);
    const ruim = await lido();
    const relatos = relatosDe(ruim.medidas);
    const ultima = ruim.medidas.at(-1);
    const relato = relatos.at(-1);
    const dito = JSON.stringify({ laco: ruim.laco, medidas: ruim.medidas.length, ultima, relatos });
    confere("a página mede", ruim.laco.perdidos >= 20, `o laço só jogou fora ${ruim.laco.perdidos} datagramas: a voz não saiu (${dito})`);
    confere("a página mede", ruim.medidas.length >= 25, `${ruim.medidas.length} medidas em 9 s, pedidas a cada 250 ms: ${dito}`);
    confere("a página mede", ultima && Math.abs(ultima.perda - 0.1) < 0.04, `a perda de agora não é a que o laço jogou fora (10%): ${dito}`);
    confere("a página mede", ultima && Number.isFinite(ultima.jitter_ms) && ultima.jitter_ms > 0 && ultima.jitter_ms < 100, `o jitter de agora não é uma medida em milissegundos: ${dito}`);
    confere("o relato", relatos.length >= 4 && relatos.length <= 9, `${relatos.length} relatos em 9 s, e o compasso é de um por segundo: ${dito}`);
    confere(
      "o relato",
      relatos.every((r, i) => i === 0 || r.quando - relatos[i - 1].quando >= 900),
      `relatos a menos de um segundo um do outro: ${JSON.stringify(relatos.map((r) => Math.round(r.quando)))}`,
    );
    confere("o relato", relato && Math.abs(relato.perda - 0.1) < 0.04, `a perda relatada não é a que o laço jogou fora (10%): ${dito}`);
    confere("o relato", relato && relato.jitter_ms > 0 && relato.jitter_ms < 100, `o jitter relatado não é uma medida em milissegundos: ${dito}`);

    // 3. A janela desliza: o laço limpo, e a perda relatada volta a zero.
    await aba.evaluate(() => {
      window.bancada.laco.umEm = 0;
      window.bancada.medidas.length = 0;
    });
    await esperar(8000);
    const limpo = await lido();
    const depois = relatosDe(limpo.medidas).at(-1);
    confere("a janela desliza", depois && depois.perda <= 0.02, `oito segundos depois de o laço parar de perder, o relato diz ${JSON.stringify(depois)}`);
    // A de agora é contada desde que o buffer nasceu, como no desktop: desce,
    // e não esquece.
    const agora = limpo.medidas.at(-1);
    confere("a janela desliza", agora && agora.perda > 0.02 && agora.perda < 0.09, `a perda de agora, contada desde o começo, é ${JSON.stringify(agora)}`);

    // 4. Fora da sala nada é medido.
    const fora = await aba.evaluate(async () => {
      const b = window.bancada;
      // Um pedido no ar no instante da saída: a resposta dele não é dita.
      b.voz.pedirARecepcao();
      b.voz.sair();
      b.medidas.length = 0;
      const atendidosAntes = b.atendidos;
      await new Promise((ok) => setTimeout(ok, 1500));
      return { medidas: b.medidas.length, atendidos: b.atendidos - atendidosAntes, direto: b.voz.pedirARecepcao() };
    });
    confere("fora da sala", fora.medidas === 0 && fora.atendidos === 0 && fora.direto === false, `fora da sala a recepção seguiu medindo: ${JSON.stringify(fora)}`);

    // 5. Uma voz nova não herda a medida da velha — nem a resposta de um
    // pedido que estava no ar quando a velha saiu. Uma voz com perda por
    // quatro segundos, e então, no mesmo instante: o pedido, a saída e a
    // entrada da nova, pelo laço limpo.
    await aba.evaluate(() => {
      const b = window.bancada;
      b.laco.umEm = 10;
      b.voz.entrar(b.lacoLocal(901), 78);
    });
    await esperar(4000);
    const velha = (await lido()).medidas.at(-1);
    confere("uma voz nova", velha && velha.perda > 0.05, `a voz com perda não mediu a perda: ${JSON.stringify(velha)}`);
    await aba.evaluate(() => {
      const b = window.bancada;
      b.voz.pedirARecepcao();
      b.voz.sair();
      b.medidas.length = 0;
      b.laco.umEm = 0;
      b.voz.entrar(b.lacoLocal(902), 79);
    });
    await esperar(1200);
    const nova = (await lido()).medidas;
    confere("uma voz nova", nova.length >= 2, `a voz nova não mediu: ${JSON.stringify(nova)}`);
    confere(
      "uma voz nova",
      nova.every((m) => m.relato === null) && nova.every((m) => m.perda === null || m.perda < 0.02),
      `a voz nova começou com a medida da velha: ${JSON.stringify(nova)}`,
    );

    const fim = await lido();
    confere("falhas", fim.falhas.length === 0, `falhas da voz: ${JSON.stringify(fim.falhas)}`);
    confere("console", doConsole.length === 0, `erros no console: ${JSON.stringify(doConsole)}`);
    if (!falhas.length) {
      console.log(
        `recepcao-no-navegador: com 10% jogados fora, a página mediu ${(ultima.perda * 100).toFixed(1)}% e ${ultima.jitter_ms.toFixed(2)} ms de jitter, ` +
          `relatou ${(relato.perda * 100).toFixed(1)}% e ${relato.jitter_ms.toFixed(2)} ms em ${relatos.length} relatos de 9 s, ` +
          `e com o laço limpo o relato voltou a ${(depois.perda * 100).toFixed(1)}%`,
      );
    }
  } finally {
    await navegador.close();
    servidor.close();
    fs.rmSync(pasta, { recursive: true, force: true });
  }
}

principal()
  .catch((erro) => falhas.push(`a bancada quebrou: ${erro && erro.stack ? erro.stack : erro}`))
  .finally(() => {
    terminou = true;
    if (falhas.length) {
      for (const f of falhas) console.error(`FALHOU — ${f}`);
      process.exit(1);
    }
  });
