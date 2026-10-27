// **O código de quem ficou para trás, na sessão web** (a auditoria Astra,
// item 4), num navegador real, contra um servidor de verdade
// (`crates/seele-server/examples/servidor_da_bancada.rs`).
//
//   node apps/seele-app/bancada/ficou-para-tras-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída] [--reverter leitura|closed]
//
// A sessão é a de produto (`abrirASessaoDoNavegador`, `sessao-web.js`), sobre o
// enlace de produto (`rtc-web.js`). A bancada envolve o transporte só para
// fazer o fecho do anfitrião chegar com um CÓDIGO e na ordem que ela escolhe —
// as duas rotas que o navegador tem (a leitura do controle que falha com
// `{corte}`, e o `closed` que resolve com `{fechada}`) — e depois solta a
// conexão de verdade. O que se prova, numa sessão nova por caso:
//
// 1. **`CODIGO_FICOU_PARA_TRAS`, a leitura primeiro**: a despedida
//    `FellBehind` é dobrada UMA vez, sem `Ended`, a sessão volta sozinha com
//    geração nova, o histórico fica, e uma mensagem nova depois da volta sai;
// 2. **o mesmo código, o `closed` primeiro**: igual;
// 3. **o código 0** (o fecho comum): a volta, e nenhuma despedida `FellBehind`;
// 4. **um código desconhecido** (`0x5345_0031`): a volta, e nenhuma despedida.
//
// `--reverter leitura` serve o `sessao-web.js` sem guardar o código da rota da
// leitura; `--reverter closed`, sem guardar o da rota do `closed`. Só na
// memória; o caso da rota tirada tem de reprovar.

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const [wasm, servidorDaBancada, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && !["--json", "--reverter"].includes(argumentos[i - 1]),
);
const saidaJson = opcao("--json");
const REVERTER = opcao("--reverter");
if (!wasm || !servidorDaBancada || !playwright) {
  console.error("uso: node ficou-para-tras-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída] [--reverter leitura|closed]");
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");

/** O `CODIGO_FICOU_PARA_TRAS` lido do `seele-proto`, e não escrito aqui. */
function codigoDoProto() {
  const fonte = fs.readFileSync(path.join(__dirname, "../../../crates/seele-proto/src/transport.rs"), "utf8");
  const achado = fonte.match(/pub const CODIGO_FICOU_PARA_TRAS: u32 = 0x([0-9a-fA-F_]+);/);
  if (!achado) throw new Error("o CODIGO_FICOU_PARA_TRAS sumiu do seele-proto");
  return parseInt(achado[1].replace(/_/g, ""), 16);
}

let servidorVivo = null;
process.on("exit", () => {
  try {
    servidorVivo?.kill("SIGKILL");
  } catch {
    // Já saiu.
  }
});

function subirOServidor() {
  const processo = spawn(servidorDaBancada, [], { stdio: ["pipe", "pipe", "inherit"] });
  servidorVivo = processo;
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

function sessaoServida() {
  let fonte = fs.readFileSync(path.join(UI, "sessao-web.js"), "utf8");
  const reversoes = {
    leitura: ["        guardarOCodigo(falha?.corte);\n", ""],
    closed: ["        guardarOCodigo(motivo?.fechada);\n", ""],
  };
  if (REVERTER) {
    const [de, para] = reversoes[REVERTER] ?? [];
    if (!de || !fonte.includes(de)) throw new Error(`a reversão «${REVERTER}» não casa com o sessao-web.js`);
    fonte = fonte.replace(de, para);
    console.error(`ficou-para-tras: REVERSÃO «${REVERTER}» no sessao-web.js servido`);
  }
  return fonte;
}

function servirAPagina() {
  const sessao = sessaoServida();
  const servidor = http.createServer((pedido, resposta) => {
    if (pedido.url === "/rtc-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(fs.readFileSync(path.join(UI, "rtc-web.js")));
      return;
    }
    if (pedido.url === "/sessao-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(sessao);
      return;
    }
    if (pedido.url === "/seele_web.wasm") {
      resposta.writeHead(200, { "Content-Type": "application/wasm" });
      resposta.end(fs.readFileSync(wasm));
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script><script src="/sessao-web.js"></script>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Na página: os quatro casos. */
async function provarNaPagina({ link: texto, CODIGO }) {
  const espera = (ms) => new Promise((r) => setTimeout(r, ms));
  const bytes = await (await fetch("/seele_web.wasm")).arrayBuffer();
  const instanciar = async () => (await WebAssembly.instantiate(bytes, {})).instance.exports;
  const leitor = await instanciar();
  const b = new TextEncoder().encode(texto);
  const p = leitor.alocar(b.length);
  new Uint8Array(leitor.memory.buffer).set(b, p);
  leitor.analisar_link(p, b.length);
  leitor.liberar(p, b.length);
  const { link } = JSON.parse(new TextDecoder().decode(new Uint8Array(leitor.memory.buffer, leitor.saida_ptr(), leitor.saida_len()).slice()));

  /**
   * O transporte de produto, envolvido: `derrubar(codigo, ordem)` faz o fecho
   * do anfitrião chegar com o código, pelas duas rotas, na ordem pedida, e
   * solta a conexão de verdade.
   */
  function transporteEnvolvido(controle) {
    return (opcoes) => {
      const real = abrirOEnlace(opcoes);
      let erroDoControle = null;
      let resolverClosed = null;
      const closed = new Promise((resolver) => {
        resolverClosed = resolver;
        real.closed.then(resolver, resolver);
      });
      const envolvido = Object.create(real);
      Object.defineProperty(envolvido, "closed", { value: closed });
      envolvido.controle = () => {
        const fluxo = real.controle();
        const ponte = new TransformStream({
          start(c) {
            erroDoControle = (motivo) => c.error(motivo);
          },
        });
        fluxo.readable.pipeTo(ponte.writable).catch(() => {});
        return { readable: ponte.readable, writable: fluxo.writable };
      };
      controle.derrubar = async (codigo, ordem) => {
        if (ordem === "leitura") {
          erroDoControle?.({ corte: codigo });
          await espera(30);
          resolverClosed({ fechada: codigo });
        } else {
          resolverClosed({ fechada: codigo });
          await espera(30);
          erroDoControle?.({ corte: codigo });
        }
        await espera(30);
        real.close(0);
      };
      return envolvido;
    };
  }

  async function umCaso(nome, codigo, ordem) {
    const eventos = [];
    const controle = {};
    const chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    const alvo = { endereco: link.alvo, alternativos: link.alternativos ?? [], impressao: link.impressao, bilhete: link.bilhete ?? null, chave };
    let transporteDaVez = 0;
    const envolver = transporteEnvolvido(controle);
    const sessao = await abrirASessaoDoNavegador({
      wasm: await instanciar(),
      alvo,
      apelido: `pessoa-${nome}`,
      convite: link.convite ?? null,
      emitir: (evento) => eventos.push(evento),
      // A primeira conexão é a envolvida; as da volta são as de produto.
      transporte: (opcoes) => (transporteDaVez++ === 0 ? envolver(opcoes) : abrirOEnlace(opcoes)),
    });
    await sessao.aperto;
    const canal = sessao.retrato()?.sala?.channels?.[0]?.id ?? 0;
    sessao.atender("open_channel", { channel: canal });
    sessao.atender("send_message", { channel: canal, body: `antes da queda (${nome})` });
    await espera(600);
    const antes = sessao.atender("messages", {})?.resposta ?? [];
    const geracaoAntes = sessao.geracaoDoEnlace;
    await controle.derrubar(codigo, ordem);
    // A volta: a sessão reconecta sozinha.
    const ate = performance.now() + 15_000;
    while (performance.now() < ate && (sessao.voltando || sessao.contas.voltas === 0)) await espera(100);
    await espera(300);
    sessao.atender("open_channel", { channel: canal });
    sessao.atender("send_message", { channel: canal, body: `depois da volta (${nome})` });
    await espera(800);
    const depois = sessao.atender("messages", {})?.resposta ?? [];
    const texto = (lista) => JSON.stringify(lista);
    const R = {
      codigo,
      ordem,
      ficouParaTras: sessao.contas.ficouParaTras,
      voltas: sessao.contas.voltas,
      voltou: sessao.contas.voltas >= 1 && !sessao.voltando,
      geracaoAntes,
      geracaoDepois: sessao.geracaoDoEnlace,
      ended: eventos.filter((e) => e && e.Ended).length,
      historicoAntes: texto(antes).includes(`antes da queda (${nome})`),
      historicoDepois: texto(depois).includes(`antes da queda (${nome})`),
      novaDepois: texto(depois).includes(`depois da volta (${nome})`),
    };
    await sessao.encerrar();
    return R;
  }

  return {
    leituraPrimeiro: await umCaso("leitura", CODIGO, "leitura"),
    closedPrimeiro: await umCaso("closed", CODIGO, "closed"),
    codigoZero: await umCaso("zero", 0, "leitura"),
    desconhecido: await umCaso("desconhecido", 0x5345_0031, "closed"),
  };
}

(async () => {
  const CODIGO = codigoDoProto();
  const servidor = await subirOServidor();
  const pagina = await servirAPagina();
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  let R = {};
  try {
    const aba = await browser.newPage();
    aba.on("pageerror", (e) => console.error(`[página] ${e.message}`));
    await aba.goto(`http://127.0.0.1:${pagina.address().port}/`);
    R.casos = await Promise.race([
      aba.evaluate(provarNaPagina, { link: servidor.link, CODIGO }),
      new Promise((resolver) => setTimeout(() => resolver({ pendurou: true }), 180_000)),
    ]);
    R.navegador = `${navegador} ${browser.version()}`;
    R.reversao = REVERTER;
    R.codigo = `0x${CODIGO.toString(16)}`;
  } finally {
    await browser.close();
    pagina.close();
    servidor.processo.stdin.end();
  }
  const c = R.casos ?? {};
  const voltouInteira = (caso) =>
    caso && caso.voltou && caso.ended === 0 && caso.geracaoDepois > caso.geracaoAntes && caso.historicoDepois && caso.novaDepois;
  R.veredito = c.pendurou
    ? { pendurou: false }
    : {
        leituraPrimeiro: voltouInteira(c.leituraPrimeiro) && c.leituraPrimeiro.ficouParaTras === 1,
        closedPrimeiro: voltouInteira(c.closedPrimeiro) && c.closedPrimeiro.ficouParaTras === 1,
        codigoZero: voltouInteira(c.codigoZero) && c.codigoZero.ficouParaTras === 0,
        desconhecido: voltouInteira(c.desconhecido) && c.desconhecido.ficouParaTras === 0,
      };
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  if (REVERTER) {
    const caso = { leitura: "leituraPrimeiro", closed: "closedPrimeiro" }[REVERTER];
    const reprovou = !R.veredito[caso];
    console.error(`ficou-para-tras: reversão «${REVERTER}» — ${caso} ${reprovou ? "REPROVOU" : "passou (a reversão não pegou)"}`);
    process.exit(reprovou ? 0 : 1);
  }
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
