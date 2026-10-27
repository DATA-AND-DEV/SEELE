// A permissão de rede local do Chrome (Local Network Access) no enlace da
// página: uma página de origem PÚBLICA discando um candidato de IP PRIVADO.
//
//   node apps/seele-app/bancada/rtc-rede-local.cjs <binário do eco> <playwright> [--json saída]
//
// O Chromium trata a página desta bancada (servida em 127.0.0.1) como pública
// pela bandeira `--ip-address-space-overrides`, e o eco do `seele-enlace` escuta
// em `[::]:0`, alcançado pelo IP privado desta máquina na rede local. Duas
// rodadas: sem permissão nenhuma dada, e com a permissão `local-network-access`
// concedida pelo Playwright (o que a pessoa faria ao aceitar a pergunta). Diz em
// cada uma se o enlace abriu e em quanto tempo, ou por que não.
//
// **Fora do CI, de propósito** (`SEM_LUGAR` em `xtask/src/check_runtime.rs`): precisa
// de uma interface de rede privada de verdade e concede a permissão do navegador
// dentro da bancada. Numa máquina sem endereço privado ela sai com 0 e diz
// «pulado». Roda só à mão, numa rede local explícita, e quem rodar registra a rede
// e o resultado. Não conta como validação executada.

"use strict";

const fs = require("fs");
const http = require("http");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const argumentos = process.argv.slice(2);
const [binario, playwright] = argumentos.filter((a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--json"));
const saidaJson = argumentos.includes("--json") ? argumentos[argumentos.indexOf("--json") + 1] : null;
if (!binario || !playwright) {
  console.error("uso: node rtc-rede-local.cjs <binário do eco> <playwright> [--json saída]");
  process.exit(2);
}

/** O primeiro IPv4 privado desta máquina, de uma placa que não é o loopback. */
function ipPrivado() {
  for (const lista of Object.values(os.networkInterfaces())) {
    for (const um of lista ?? []) {
      if (um.family === "IPv4" && !um.internal && /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(um.address)) return um.address;
    }
  }
  return null;
}

function subirOEco() {
  return new Promise((ok, falha) => {
    const processo = spawn(binario, ["[::]:0"], { stdio: ["ignore", "pipe", "inherit"] });
    let resto = "";
    processo.stdout.on("data", (dados) => {
      resto += dados.toString();
      const achado = resto.match(/ESCUTA \S+:(\d+) FP ([0-9a-f]{64})/);
      if (achado) ok({ processo, porta: Number(achado[1]), fp: achado[2] });
    });
    processo.on("exit", (codigo) => falha(new Error(`o eco saiu com ${codigo}`)));
  });
}

async function tentarNaPagina({ alvo, fp, controle }) {
  // O controle: um `fetch` ao mesmo IP privado. Se a página é mesmo pública
  // para o Chromium, o LNA vale para ele, e é isto que diz se a bandeira pegou.
  let fetchPrivado;
  try {
    await fetch(controle, { mode: "no-cors", cache: "no-store" });
    fetchPrivado = "passou";
  } catch (falha) {
    fetchPrivado = `recusado: ${falha}`;
  }
  const certificado = await prepararCertificadoDtls();
  const t0 = performance.now();
  const enlace = criarEnlaceRtc({ alvos: [alvo], impressao: fp, certificado });
  try {
    await enlace.ready;
    enlace.close();
    return { fetchPrivado, abriu: true, ms: Math.round(performance.now() - t0) };
  } catch (falha) {
    return { fetchPrivado, abriu: false, ms: Math.round(performance.now() - t0), falha };
  }
}

(async () => {
  const ip = ipPrivado();
  if (!ip) {
    console.log(JSON.stringify({ pulado: "esta máquina não tem IPv4 privado numa placa de rede" }));
    process.exit(0);
  }
  const eco = await subirOEco();
  const pagina = http.createServer((pedido, resposta) => {
    if (pedido.url === "/rtc-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(fs.readFileSync(path.join(__dirname, "..", "ui", "rtc-web.js")));
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script>');
  });
  await new Promise((ok) => pagina.listen(0, "127.0.0.1", ok));
  // Um segundo servidor, no IP privado: o alvo do `fetch` de controle.
  const privado = http.createServer((_, resposta) => resposta.end("ok"));
  await new Promise((ok) => privado.listen(0, ip, ok));
  const porta = pagina.address().port;
  const { chromium } = require(playwright);
  const browser = await chromium.launch({ args: [`--ip-address-space-overrides=127.0.0.1:${porta}=public`] });
  const R = { navegador: `chromium ${browser.version()}`, alvo: `${ip}:${eco.porta}`, rodadas: {} };
  try {
    for (const [nome, permissoes] of [
      ["sem permissão", []],
      ["com local-network-access", ["local-network-access"]],
    ]) {
      const contexto = await browser.newContext();
      try {
        if (permissoes.length) await contexto.grantPermissions(permissoes, { origin: `http://127.0.0.1:${porta}` });
      } catch (falha) {
        R.rodadas[nome] = { naoConcedida: String(falha.message ?? falha) };
        await contexto.close();
        continue;
      }
      const aba = await contexto.newPage();
      await aba.goto(`http://127.0.0.1:${porta}/`);
      R.rodadas[nome] = await aba.evaluate(tentarNaPagina, { alvo: R.alvo, fp: eco.fp, controle: `http://${ip}:${privado.address().port}/` });
      await contexto.close();
    }
  } finally {
    await browser.close();
    pagina.close();
    privado.close();
    eco.processo.kill();
  }
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
