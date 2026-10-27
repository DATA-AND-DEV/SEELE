// A página pelo caminho do ponto de encontro (contrato da 0.17, D7), num
// navegador real, contra um `seele-encontro` real.
//
//   node apps/seele-app/bancada/rtc-sinal.cjs <seele-encontro> <anfitrião> <seele_web.wasm> <chromium|webkit> <playwright> [--json saída]
//
// - `<seele-encontro>`: o binário, que sobe com `--sinal 127.0.0.1:<porta>`,
//   `--origem` da página e `--rede-local`. O STUN Binding e o ENC/3 ficam na
//   porta UDP, e o `POST /sinal/1` na TCP.
// - `<anfitrião>`: o binário do anfitrião. Pode ser:
//   - o `seeled` real da 0.17 (`--seeled`), que se registra no ponto pelo
//     ENC/3 sozinho (`SEELE_ENCONTRO`);
//   - ou o `examples/eco.rs` do `seele-enlace` (padrão). Nesse caso, a escuta
//     ENC/3 do anfitrião é esta bancada: ela registra as duas marcas
//     (`MORA3`) e responde o `CHAMADO` com o `ATENDE` e o endereço do eco.
//     É o papel da escuta do `seele-server`, até o porte dele estar pronto.
//   O anfitrião escuta em `[::]:0`, com pilha dupla, como no produto.
//
// Os casos, cada um com o resultado na saída:
// - **aqui**: o alvo do link não responde (`192.0.2.1`, TEST-NET). Depois da
//   espera do caminho direto, a página tenta o refletido pelo STUN do ponto e
//   manda o `CHAMA` pelo sinal. A escuta recebe o `CHAMADO` com o U da página e
//   responde `ATENDE`; o `AQUI` traz o endereço do anfitrião, e a página
//   conecta;
// - **ninguem**: uma impressão que ninguém registrou, e a falha
//   `NinguemNoPonto`;
// - **calado**: a escuta registrada não responde, e a falha `AnfitriaoCalado`;
// - **sem STUN nem sinal**: o ponto do bilhete numa porta morta, e a falha
//   `SemRefletido`;
// - **recusa**: uma enxurrada de `CHAMA` do mesmo IP antes, e a falha
//   `PontoRecusou`.
//
// **Não mede NAT.** Tudo é loopback: não há tradução, mapeamento nem filtro.

"use strict";

const dgram = require("dgram");
const fs = require("fs");
const http = require("http");
const net = require("net");
const path = require("path");
const { spawn } = require("child_process");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const posicionais = argumentos.filter((a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--json"));
const [encontro, anfitriao, wasm, navegador = "chromium", playwright] = posicionais;
const comSeeled = argumentos.includes("--seeled");
const saidaJson = opcao("--json");
if (!encontro || !anfitriao || !wasm || !playwright) {
  console.error("uso: node rtc-sinal.cjs <seele-encontro> <anfitrião> <seele_web.wasm> <chromium|webkit> <playwright> [--seeled] [--json saída]");
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");
const MAGIA3 = "SEELE-ENC/3";
const TAMANHO3 = 384;

/** Uma porta livre do sistema (UDP ou TCP). */
function portaLivre(tipo) {
  return new Promise((ok) => {
    if (tipo === "udp") {
      const s = dgram.createSocket("udp4");
      s.bind(0, "127.0.0.1", () => {
        const porta = s.address().port;
        s.close(() => ok(porta));
      });
    } else {
      const s = net.createServer();
      s.listen(0, "127.0.0.1", () => {
        const porta = s.address().port;
        s.close(() => ok(porta));
      });
    }
  });
}

function esperarLinha(processo, padrao) {
  return new Promise((ok, falha) => {
    let resto = "";
    const ouvir = (dados) => {
      resto += dados.toString();
      let fim;
      while ((fim = resto.indexOf("\n")) >= 0) {
        const linha = resto.slice(0, fim);
        resto = resto.slice(fim + 1);
        const achado = linha.match(padrao);
        if (achado) ok(achado);
      }
    };
    processo.stdout?.on("data", ouvir);
    processo.stderr?.on("data", ouvir);
    processo.on("exit", (codigo) => falha(new Error(`${processo.spawnfile} saiu com ${codigo}`)));
  });
}

/** Uma linha do ENC/3, completada até o tamanho fixo. */
function datagrama(linha) {
  const bytes = Buffer.alloc(TAMANHO3, ".");
  bytes.write(`${linha}\n`);
  return bytes;
}

/** A linha útil de um datagrama do ENC/3. */
const linhaDe = (bytes) => bytes.toString("utf8").split("\n")[0];

/**
 * A escuta ENC/3 do anfitrião, no lugar da do `seele-server`: registra as duas
 * marcas pelo `MORA3` (com a volta do `DESAFIO`) e responde o `CHAMADO` com o
 * `ATENDE`. `modo`: `"atende"` ou `"calada"`.
 */
async function escutaDoAnfitriao({ ponto, fp, enderecoDoAnfitriao, modo }) {
  const socket = dgram.createSocket("udp4");
  await new Promise((ok) => socket.bind(0, "127.0.0.1", ok));
  const marcas = { escuta: `${fp.slice(0, 16)}e`, servidor: `${fp.slice(0, 16)}s` };
  const chamados = [];
  const registradas = new Set();
  socket.on("message", (bytes) => {
    const linha = linhaDe(bytes);
    const campos = linha.split(" ");
    if (campos[0] !== MAGIA3) return;
    if (campos[1] === "DESAFIO") socket.send(datagrama(`${MAGIA3} MORA3 ${campos[2]} ${campos[3]}`), ponto.porta, "127.0.0.1");
    else if (campos[1] === "MOROU") registradas.add(campos[2]);
    else if (campos[1] === "CHAMADO") {
      const [, , escuta, id, credencial, candidatos] = campos;
      chamados.push({ escuta, credencial, candidatos });
      if (modo === "atende" && escuta === marcas.escuta) {
        socket.send(datagrama(`${MAGIA3} ATENDE ${escuta} ${id} ${enderecoDoAnfitriao} -`), ponto.porta, "127.0.0.1");
      }
    }
  });
  for (const marca of [marcas.escuta, marcas.servidor]) {
    socket.send(datagrama(`${MAGIA3} MORA3 ${marca} -`), ponto.porta, "127.0.0.1");
  }
  const ate = Date.now() + 3000;
  while (registradas.size < 2 && Date.now() < ate) await new Promise((r) => setTimeout(r, 20));
  return {
    registradas: [...registradas],
    chamados,
    trocarModo: (novo) => {
      modo = novo;
    },
    fechar: () => socket.close(),
  };
}

function servirAPagina() {
  const servidor = http.createServer((pedido, resposta) => {
    const arquivos = {
      "/rtc-web.js": [path.join(UI, "rtc-web.js"), "text/javascript"],
      "/sessao-web.js": [path.join(UI, "sessao-web.js"), "text/javascript"],
      "/seele_web.wasm": [wasm, "application/wasm"],
    };
    const achado = arquivos[pedido.url];
    if (achado) {
      resposta.writeHead(200, { "Content-Type": achado[1] });
      resposta.end(fs.readFileSync(achado[0]));
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script><script src="/sessao-web.js"></script><p>rtc-sinal</p>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Roda na página: uma tentativa pelo caminho do ponto. */
async function tentarNaPagina({ alvos, fp, ponto, urlDoSinal, enxurrada }) {
  const bytes = await (await fetch("/seele_web.wasm")).arrayBuffer();
  const instancia = (await WebAssembly.instantiate(bytes, {})).instance.exports;
  const sinal = fronteiraDoSinal(instancia);
  const respostasDaEnxurrada = {};
  if (enxurrada) {
    // A enxurrada corre junto com a chamada de verdade, que sai depois da
    // espera do caminho direto (1,5 s): começando antes, o balde (8 por
    // segundo) já teria enchido de novo quando a chamada saísse.
    const corpo = sinal.montar({ impressao: "ab".repeat(32), credencial: "A".repeat(32), candidatos: [] }).corpo;
    // Em ondas a cada 50 ms, de 1,0 a 2,5 s: o balde (que enche a 8 por
    // segundo) fica vazio quando a chamada de verdade sai, por volta de 1,5 s.
    for (let quando = 1000; quando <= 2500; quando += 50) {
      setTimeout(() => {
        for (let i = 0; i < enxurrada; i++) {
          fetch(urlDoSinal, { method: "POST", body: corpo, headers: { "Content-Type": "text/plain" } })
            .then((r) => r.text())
            .catch((e) => `falhou ${e}`)
            .then((t) => {
              const chave = String(t).split(" ").slice(0, 2).join(" ");
              respostasDaEnxurrada[chave] = (respostasDaEnxurrada[chave] ?? 0) + 1;
            });
        }
      }, quando);
    }
  }
  const etapas = [];
  const certificado = await prepararCertificadoDtls();
  const t0 = performance.now();
  const enlace = criarEnlaceRtc({
    alvos,
    impressao: fp,
    certificado,
    bilhete: { ponto, aviso: "127.0.0.1:9" },
    sinal,
    urlDoSinal: () => urlDoSinal,
    aoMudar: (mudanca) => etapas.push({ t: Math.round(performance.now() - t0), ...mudanca }),
  });
  try {
    await enlace.ready;
    const visto = enlace.visto;
    // Uma ida e volta pelo controle, para provar que é o anfitrião.
    const { readable, writable } = enlace.controle();
    const escritor = writable.getWriter();
    await escritor.write(new Uint8Array([1, 2, 3]));
    const { value } = await readable.getReader().read();
    enlace.close();
    return {
      abriu: true,
      ms: Math.round(performance.now() - t0),
      etapas,
      sinal: visto.sinal,
      refletido: visto.refletido,
      eco: Array.from(value ?? []),
    };
  } catch (falha) {
    return { abriu: false, ms: Math.round(performance.now() - t0), etapas, falha, ...(enxurrada ? { respostasDaEnxurrada } : {}) };
  }
}

(async () => {
  const portaUdp = await portaLivre("udp");
  const portaTcp = await portaLivre("tcp");
  const morta = await portaLivre("udp");
  const pagina = await servirAPagina();
  const origem = `http://127.0.0.1:${pagina.address().port}`;
  const ponto = { porta: portaUdp };
  const processoDoPonto = spawn(
    encontro,
    ["--porta", String(portaUdp), "--rede-local", "--sinal", `127.0.0.1:${portaTcp}`, "--origem", origem],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  await esperarLinha(processoDoPonto, /sinal do navegador atendendo/);

  // Com `--seeled`, o anfitrião é o `servidor_da_bancada` (o `Hospedagem` do
  // app): a primeira linha dele é o JSON com o endereço e a impressão, e ele se
  // registra no ponto pelo `SEELE_ENCONTRO`. O tubo da entrada fica aberto
  // enquanto a bancada roda.
  const processoDoAnfitriao = spawn(anfitriao, comSeeled ? [] : ["[::]:0"], {
    stdio: [comSeeled ? "pipe" : "ignore", "pipe", "inherit"],
    env: { ...process.env, SEELE_ENCONTRO: `127.0.0.1:${portaUdp}` },
  });
  let ipDoAnfitriao;
  let portaDoAnfitriao;
  let fp;
  if (comSeeled) {
    const [linha] = await esperarLinha(processoDoAnfitriao, /^\{.*\}$/);
    const dito = JSON.parse(linha);
    fp = dito.impressao;
    [, ipDoAnfitriao, portaDoAnfitriao] = String(dito.endereco).match(/^(.*):(\d+)$/) ?? [];
    // O registro no ponto acontece no reavivamento: espera o primeiro.
    await new Promise((r) => setTimeout(r, 2000));
  } else {
    [, ipDoAnfitriao, portaDoAnfitriao, fp] = await esperarLinha(processoDoAnfitriao, /^ESCUTA (\S+):(\d+) FP ([0-9a-f]{64})$/);
  }
  // O anfitrião em `[::]`: no loopback IPv4, a página o alcança por 127.0.0.1.
  const enderecoDoAnfitriao = `127.0.0.1:${portaDoAnfitriao}`;
  const escuta = comSeeled ? null : await escutaDoAnfitriao({ ponto, fp, enderecoDoAnfitriao, modo: "atende" });

  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  const resultado = {
    navegador: `${navegador} ${browser.version()}`,
    anfitriao: { escuta: `${ipDoAnfitriao}:${portaDoAnfitriao}`, real: comSeeled },
    registradas: escuta?.registradas ?? null,
    casos: {},
  };
  const urlDoSinal = `http://127.0.0.1:${portaTcp}/sinal/1`;
  const naPagina = async (args) => {
    const aba = await browser.newPage();
    await aba.goto(`${origem}/`);
    const lido = await aba.evaluate(tentarNaPagina, args);
    await aba.close();
    return lido;
  };
  const inalcancavel = ["192.0.2.1:9"];
  try {
    resultado.casos.aqui = await naPagina({ alvos: inalcancavel, fp, ponto: `127.0.0.1:${portaUdp}`, urlDoSinal });
    resultado.casos.aqui.chamados = escuta ? escuta.chamados.splice(0) : null;
    resultado.casos.ninguem = await naPagina({ alvos: inalcancavel, fp: "cd".repeat(32), ponto: `127.0.0.1:${portaUdp}`, urlDoSinal });
    escuta?.trocarModo("calada");
    resultado.casos.calado = escuta
      ? await naPagina({ alvos: inalcancavel, fp, ponto: `127.0.0.1:${portaUdp}`, urlDoSinal })
      : { pulado: "com o seeled real a escuta não se cala" };
    resultado.casos.semStunNemSinal = await naPagina({
      alvos: inalcancavel,
      fp,
      ponto: `127.0.0.1:${morta}`,
      urlDoSinal: `http://127.0.0.1:${morta}/sinal/1`,
    });
    resultado.casos.recusa = await naPagina({ alvos: inalcancavel, fp, ponto: `127.0.0.1:${portaUdp}`, urlDoSinal, enxurrada: 10 });
  } finally {
    await browser.close();
    escuta?.fechar();
    processoDoAnfitriao.kill();
    processoDoPonto.kill();
    pagina.close();
  }
  const motivo = (caso) => caso?.falha?.FalhaDoEnlace?.motivo ?? null;
  resultado.veredito = {
    aqui: resultado.casos.aqui.abriu === true && Boolean(resultado.casos.aqui.sinal?.aqui),
    ninguem: motivo(resultado.casos.ninguem) === "NinguemNoPonto",
    calado: comSeeled || motivo(resultado.casos.calado) === "AnfitriaoCalado",
    semStunNemSinal: motivo(resultado.casos.semStunNemSinal) === "SemRefletido",
    recusa: motivo(resultado.casos.recusa) === "PontoRecusou",
  };
  const texto = JSON.stringify(resultado, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  process.exit(Object.values(resultado.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
