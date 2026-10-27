// **O crédito de recepção (0x04) do enlace da página**, num navegador real,
// contra o enlace de PRODUTO (`crates/seele-enlace/examples/eco.rs`).
//
//   node apps/seele-app/bancada/rtc-credito.cjs <binário do eco> <chromium|webkit> <caminho do playwright> [--json saída] [--sem-credito]
//
// O eco devolve o controle mensagem a mensagem e cada «u» inteiro por um «u»
// dele. A voz corre o tempo todo, a 50 mensagens/s, ecoada. O que se prova:
//
// 1. **um anexo grande nos dois sentidos**: 40 MiB sobem por um «u» da página
//    (ela respeita o crédito do anfitrião) e os mesmos 40 MiB descem pelo «u»
//    do anfitrião (a página concede crédito no ritmo em que lê), byte a byte;
// 2. **a página que não lê**: 30 MiB descem com a página parada 5 s antes de
//    ler. O anfitrião espera o crédito, ninguém corta, e os 30 MiB chegam
//    inteiros. O que a página tinha concedido e não lido nunca passou da soma
//    da conexão;
// 3. **o anfitrião que não lê**: a página escreve 8 MiB no controle sem ler o
//    eco por 3 s. O eco do anfitrião espera o crédito da página, deixa de ler,
//    e a página, sem crédito do anfitrião, espera em vez de estourar a fila
//    dele (que fecharia a conexão). Depois ela lê, e os 8 MiB voltam inteiros;
// 4. **a voz durante tudo isso**: perda e p50/p95/p99 por fase.
//
// `--sem-credito` serve à página um `rtc-web.js` com o crédito desligado, só
// em memória (a página não anuncia e não espera): a reversão. Os casos 2 e 3
// têm de reprovar.

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");
const { spawn } = require("child_process");

const [, , binario, navegador = "chromium", playwright, ...resto] = process.argv;
if (!binario || !playwright) {
  console.error("uso: node rtc-credito.cjs <binário do eco> <chromium|webkit> <caminho do playwright> [--json saída] [--sem-credito]");
  process.exit(2);
}
const opcao = (nome) => (resto.includes(nome) ? resto[resto.indexOf(nome) + 1] : null);
const saidaJson = opcao("--json");
const SEM_CREDITO = resto.includes("--sem-credito");
const UI = path.join(__dirname, "..", "ui");

function subirOEco() {
  return new Promise((ok, falha) => {
    const processo = spawn(binario, ["127.0.0.1:0"], { stdio: ["ignore", "pipe", "inherit"] });
    let linhas = "";
    const saida = [];
    processo.stdout.on("data", (dados) => {
      linhas += dados.toString();
      let fim;
      while ((fim = linhas.indexOf("\n")) >= 0) {
        const linha = linhas.slice(0, fim);
        linhas = linhas.slice(fim + 1);
        saida.push(linha);
        const achado = linha.match(/^ESCUTA (\S+):(\d+) FP ([0-9a-f]{64})$/);
        if (achado) ok({ processo, saida, ip: achado[1], porta: Number(achado[2]), fp: achado[3] });
      }
    });
    processo.on("exit", (codigo) => falha(new Error(`o eco saiu com ${codigo}`)));
  });
}

/** O `rtc-web.js` servido: o de produto, ou o sem crédito (só na memória). */
function fonteDaPagina() {
  let fonte = fs.readFileSync(path.join(UI, "rtc-web.js"), "utf8");
  if (!SEM_CREDITO) return fonte;
  const trocas = [
    // Quem escreve não espera crédito nenhum.
    ["const cabe = (bytes) => enviados + bytes <= limite;", "const cabe = () => true;"],
    // Quem lê nunca anuncia.
    ["    if (um.fim || !canais.has(um) || um.canal.readyState !== \"open\") return;", "    return;"],
  ];
  for (const [de, para] of trocas) {
    if (!fonte.includes(de)) throw new Error(`a reversão não casa com o rtc-web.js: ${de}`);
    fonte = fonte.replace(de, para);
  }
  console.error("rtc-credito: REVERSÃO — o crédito desligado no rtc-web.js servido");
  return fonte;
}

function servirAPagina() {
  const fonte = fonteDaPagina();
  const servidor = http.createServer((pedido, resposta) => {
    if (pedido.url === "/rtc-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(fonte);
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script><p>rtc-credito</p>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Roda na página. */
async function medirNaPagina({ ip, porta, fp }) {
  const R = { casos: {} };
  const espera = (ms) => new Promise((r) => setTimeout(r, ms));
  const MiB = 1024 * 1024;
  let enlace = null;
  const etapa = (nome) => (window.__etapaDaBancada = nome);
  /**
   * **O prazo de uma etapa**: vencido, a rodada para com a etapa e os
   * contadores do crédito (anunciados, recebidos, esperas, e o concedido e não
   * lido da conexão), em vez de esperar para sempre.
   */
  async function comPrazo(promessa, ms, nome) {
    etapa(nome);
    let relogio;
    const vencido = new Promise((_, rejeitar) => {
      relogio = setTimeout(async () => {
        let creditoEmAberto = null;
        try {
          creditoEmAberto = (await enlace?.getStats())?.creditoEmAberto ?? null;
        } catch {
          creditoEmAberto = null;
        }
        rejeitar({ prazoVencido: { etapa: nome, ms, contas: enlace ? { ...enlace.contas } : null, creditoEmAberto } });
      }, ms);
    });
    try {
      return await Promise.race([promessa, vencido]);
    } finally {
      clearTimeout(relogio);
    }
  }
  try {
  const certificado = await prepararCertificadoDtls();
  enlace = criarEnlaceRtc({ alvos: [`${ip}:${porta}`], impressao: fp, certificado });
  window.__enlaceDaBancada = enlace;
  try {
    await comPrazo(enlace.ready, 15_000, "conectar");
  } catch (falha) {
    return { abriu: false, falha };
  }
  R.abriu = true;
  let caiu = null;
  enlace.closed.then((motivo) => (caiu = motivo));

  // ------------------------------------------------- a voz, o tempo todo
  const vozLeitor = enlace.canalDaVoz.readable.getReader();
  const vozEscritor = enlace.canalDaVoz.writable.getWriter();
  const idas = new Map();
  const medidas = [];
  let seq = 0;
  (async () => {
    for (;;) {
      const { value, done } = await vozLeitor.read();
      if (done) break;
      const s = new DataView(value.buffer, value.byteOffset).getUint32(1);
      const ida = idas.get(s);
      if (ida !== undefined) {
        medidas.push({ ida, rtt: performance.now() - ida });
        idas.delete(s);
      }
    }
  })().catch(() => {});
  const relogio = setInterval(() => {
    const msg = new Uint8Array(160);
    msg[0] = 1;
    new DataView(msg.buffer).setUint32(1, seq);
    idas.set(seq, performance.now());
    seq += 1;
    vozEscritor.write(msg).catch(() => {});
  }, 20);
  const fases = [];
  const marcar = (nome) => fases.push({ nome, t: performance.now() });

  // O conteúdo: o byte i é (i * 7 + semente) % 251, conferido sem guardar tudo.
  const pedacoDe = (inicio, tamanho, semente) => {
    const p = new Uint8Array(tamanho);
    for (let i = 0; i < tamanho; i++) p[i] = ((inicio + i) * 7 + semente) % 251;
    return p;
  };
  async function subirUmU(total, semente) {
    const fluxo = await comPrazo(enlace.createUnidirectionalStream(), 10_000, "abrir o u que sobe");
    const escritor = fluxo.getWriter();
    const PEDACO = 256 * 1024;
    for (let i = 0; i < total; i += PEDACO) {
      await comPrazo(escritor.write(pedacoDe(i, Math.min(PEDACO, total - i), semente)), 20_000, `o u que sobe, o bloco em ${i} (o crédito do anfitrião)`);
    }
    await comPrazo(escritor.close(), 30_000, "o u que sobe, o fim e a entrega");
  }
  /** Lê um fluxo conferindo o padrão; `parar` ms antes de começar. */
  async function lerConferindo(leitura, semente, { parar = 0 } = {}) {
    if (parar) await espera(parar);
    const leitor = leitura.getReader();
    let total = 0;
    let iguais = true;
    let erro = null;
    try {
      for (;;) {
        const { value, done } = await leitor.read();
        if (done) break;
        for (let i = 0; i < value.length; i++) {
          if (value[i] !== ((total + i) * 7 + semente) % 251) {
            iguais = false;
            break;
          }
        }
        total += value.length;
      }
    } catch (falha) {
      erro = falha;
    }
    return { total, iguais, erro };
  }
  const fluxosQueChegam = enlace.incomingUnidirectionalStreams.getReader();
  const proximoFluxo = (prazo) =>
    comPrazo(proximoFluxoSemPrazo(prazo), prazo + 1000, "esperar o u que desce (o crédito da página)");
  const proximoFluxoSemPrazo = (prazo) =>
    Promise.race([fluxosQueChegam.read().then(({ value }) => value ?? null, () => null), espera(prazo).then(() => null)]);
  const contas = () => ({ ...enlace.contas });
  let maiorEmAberto = 0;
  const vigia = setInterval(async () => {
    maiorEmAberto = Math.max(maiorEmAberto, (await enlace.getStats()).creditoEmAberto ?? 0);
  }, 25);

  await espera(800);
  // ------------------------------------- 1. 40 MiB nos dois sentidos
  {
    marcar("grande");
    const antes = contas();
    const TOTAL = 40 * MiB;
    const t0 = performance.now();
    const chegando = proximoFluxo(60_000);
    let subiu = null;
    try {
      await subirUmU(TOTAL, 11);
      subiu = true;
    } catch (falha) {
      subiu = { falha: String(falha?.corte !== undefined ? `corte ${falha.corte}` : falha) };
    }
    const tSubida = performance.now();
    const volta = await chegando;
    const desceu = volta ? await comPrazo(lerConferindo(volta, 11), 60_000, "ler o u que desce") : { total: 0, iguais: false, erro: "não voltou" };
    const depois = contas();
    R.casos.grande = {
      bytes: TOTAL,
      subiu,
      subida_ms: Math.round(tSubida - t0),
      voltou: desceu.total,
      iguais: desceu.iguais,
      erro: desceu.erro,
      descida_ms: Math.round(performance.now() - tSubida),
      cortesPorFila: depois.cortesPorFila - antes.cortesPorFila,
      esperasPorCredito: depois.esperasPorCredito - antes.esperasPorCredito,
      creditosAnunciados: depois.creditosAnunciados - antes.creditosAnunciados,
      creditosRecebidos: depois.creditosRecebidos - antes.creditosRecebidos,
    };
  }

  // ----------------------------------------- 2. a página que não lê
  {
    marcar("paginaParada");
    const antes = contas();
    const TOTAL = 30 * MiB;
    const chegando = proximoFluxo(60_000);
    let subiu = null;
    try {
      await subirUmU(TOTAL, 23);
      subiu = true;
    } catch (falha) {
      subiu = { falha: String(falha?.corte !== undefined ? `corte ${falha.corte}` : falha) };
    }
    const volta = await chegando;
    maiorEmAberto = 0;
    const desceu = volta
      ? await comPrazo(lerConferindo(volta, 23, { parar: 5000 }), 65_000, "a página parada 5 s, e depois ler o u que desce")
      : { total: 0, iguais: false, erro: "não voltou" };
    const depois = contas();
    R.casos.paginaParada = {
      bytes: TOTAL,
      subiu,
      parada_ms: 5000,
      voltou: desceu.total,
      iguais: desceu.iguais,
      erro: desceu.erro === null ? null : String(desceu.erro?.corte !== undefined ? `corte ${desceu.erro.corte}` : desceu.erro),
      cortesPorFila: depois.cortesPorFila - antes.cortesPorFila,
      maiorCreditoEmAberto: maiorEmAberto,
      tetoDaConexao: MAX_CREDITO_DA_CONEXAO,
    };
  }

  // -------------------------------------- 3. o anfitrião que não lê
  {
    marcar("anfitriaoParado");
    const antes = contas();
    const TOTAL = 8 * MiB;
    const { readable, writable } = enlace.controle();
    const escritor = writable.getWriter();
    let escrito = 0;
    let falhaDaEscrita = null;
    const escrevendo = (async () => {
      const PEDACO = 64 * 1024;
      try {
        for (let i = 0; i < TOTAL; i += PEDACO) {
          await comPrazo(escritor.write(pedacoDe(i, PEDACO, 37)), 20_000, `o controle, o bloco em ${i}`);
          escrito += PEDACO;
        }
      } catch (falha) {
        falhaDaEscrita = String(falha);
      }
    })();
    // Ninguém lê o eco por 3 s: o anfitrião para de ler, e a página espera.
    await espera(3000);
    const escritoNaParada = escrito;
    const esperasNaParada = contas().esperasPorCredito - antes.esperasPorCredito;
    const lendo = (async () => {
      const leitor = readable.getReader();
      let total = 0;
      let iguais = true;
      while (total < TOTAL) {
        let lido;
        try {
          lido = await Promise.race([leitor.read(), espera(20_000).then(() => ({ done: true }))]);
        } catch {
          break;
        }
        const { value, done } = lido;
        if (done) break;
        for (let i = 0; i < value.length; i++) {
          if (value[i] !== ((total + i) * 7 + 37) % 251) {
            iguais = false;
            break;
          }
        }
        total += value.length;
      }
      return { total, iguais };
    })();
    await comPrazo(escrevendo, 30_000, "o controle, a escrita inteira");
    const voltou = await comPrazo(lendo, 30_000, "o controle, a leitura do eco");
    const depois = contas();
    R.casos.anfitriaoParado = {
      bytes: TOTAL,
      escritoEmTresSegundos: escritoNaParada,
      esperasPorCreditoNaParada: esperasNaParada,
      falhaDaEscrita,
      voltou: voltou.total,
      iguais: voltou.iguais,
      cortesPorFila: depois.cortesPorFila - antes.cortesPorFila,
      caiu,
    };
  }

  marcar("depois");
  await espera(1500);
  clearInterval(relogio);
  clearInterval(vigia);
  await espera(500);
  // A voz por fase.
  const faseDe = (t) => {
    let nome = "antes";
    for (const f of fases) if (t >= f.t) nome = f.nome;
    return nome;
  };
  const voz = {};
  for (const nome of ["antes", ...fases.map((f) => f.nome)]) {
    const v = medidas.filter((m) => faseDe(m.ida) === nome).map((m) => m.rtt).sort((a, b) => a - b);
    const p = (k) => (v.length ? Math.round(v[Math.min(v.length - 1, Math.floor(k * v.length))] * 10) / 10 : null);
    const perdidas = [...idas.values()].filter((ida) => faseDe(ida) === nome).length;
    voz[nome] = { recebidas: v.length, perdidas, p50: p(0.5), p95: p(0.95), p99: p(0.99), max: v.length ? Math.round(v[v.length - 1] * 10) / 10 : null };
  }
  R.voz = voz;
  R.contas = contas();
  R.caiu = caiu;
  } catch (falha) {
    // Um prazo vencido (ou outra falha) sai dito, com o que se mediu até ali.
    R.parou = falha?.prazoVencido ? falha : { falha: String(falha?.message ?? falha) };
  } finally {
    enlace?.close();
  }
  return R;
}

// **A limpeza garantida**: o eco (e o navegador, no `finally` de baixo) sai
// em toda saída, inclusive na de erro. Um eco órfão segura o stderr herdado, e
// quem lê a saída da bancada esperaria por ele para sempre.
let ecoVivo = null;
process.on("exit", () => {
  try {
    ecoVivo?.kill("SIGKILL");
  } catch {
    // Já saiu.
  }
});

(async () => {
  const eco = await subirOEco();
  ecoVivo = eco.processo;
  const servidor = await servirAPagina();
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  let R;
  try {
    const pagina = await browser.newPage();
    pagina.on("console", (m) => {
      if (m.type() === "error") console.error(`[página] ${m.text()}`);
    });
    await pagina.goto(`http://127.0.0.1:${servidor.address().port}/`);
    // O prazo da rodada inteira (400 s), além dos de cada etapa.
    R = await Promise.race([
      pagina.evaluate(medirNaPagina, { ip: eco.ip, porta: eco.porta, fp: eco.fp }),
      new Promise((resolver) => setTimeout(resolver, 400_000)).then(async () => ({
        casos: {},
        pendurou: await pagina.evaluate(() => ({
          etapa: window.__etapaDaBancada ?? null,
          contas: window.__enlaceDaBancada ? { ...window.__enlaceDaBancada.contas } : null,
        })),
      })),
    ]);
    R.navegador = `${navegador} ${browser.version()}`;
    R.reversao = SEM_CREDITO;
  } finally {
    await browser.close();
    servidor.close();
    eco.processo.kill();
  }
  R.eco = eco.saida.filter((l) => l.startsWith("VOZ") || l.startsWith("FECHOU"));
  if (R.parou) console.error(`rtc-credito: a rodada parou: ${JSON.stringify(R.parou)}`);
  if (R.pendurou) console.error(`rtc-credito: a rodada passou do prazo na etapa «${R.pendurou.etapa}»`);
  const c = R.casos ?? {};
  const perdaDaVoz = Object.values(R.voz ?? {}).reduce((t, f) => t + f.perdidas, 0);
  R.veredito = {
    grande: c.grande?.subiu === true && c.grande.voltou === c.grande.bytes && c.grande.iguais && c.grande.cortesPorFila === 0,
    paginaParada:
      c.paginaParada?.subiu === true &&
      c.paginaParada.voltou === c.paginaParada.bytes &&
      c.paginaParada.iguais &&
      c.paginaParada.erro === null &&
      c.paginaParada.cortesPorFila === 0 &&
      c.paginaParada.maiorCreditoEmAberto <= c.paginaParada.tetoDaConexao,
    anfitriaoParado:
      c.anfitriaoParado?.falhaDaEscrita === null &&
      c.anfitriaoParado.esperasPorCreditoNaParada > 0 &&
      c.anfitriaoParado.escritoEmTresSegundos < c.anfitriaoParado.bytes &&
      c.anfitriaoParado.voltou === c.anfitriaoParado.bytes &&
      c.anfitriaoParado.iguais &&
      c.anfitriaoParado.caiu === null,
    // A voz continua em toda fase (chega eco dela em cada uma). A perda vai
    // contada ao lado, e não vira critério: «voz sem perda sob qualquer carga»
    // não é o que se afirma.
    vozContinua: ["grande", "paginaParada", "anfitriaoParado"].every((nome) => (R.voz?.[nome]?.recebidas ?? 0) > 0),
  };
  R.perdaDaVoz = perdaDaVoz;
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  const passou = Object.values(R.veredito).every(Boolean);
  if (SEM_CREDITO) {
    const exigidas = ["paginaParada", "anfitriaoParado"];
    const reprovaram = exigidas.filter((nome) => !R.veredito[nome]);
    console.error(`rtc-credito: reversão — reprovaram ${reprovaram.length} de ${exigidas.length} (${reprovaram.join(", ")})`);
    process.exit(reprovaram.length === exigidas.length ? 0 : 1);
  }
  process.exit(passou ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
