// O enlace WebRTC da página (`ui/rtc-web.js`) contra o enlace de PRODUTO
// (`crates/seele-enlace/examples/eco.rs`), num navegador real.
//
//   node apps/seele-app/bancada/rtc-eco.cjs <binário do eco> <chromium|webkit> <caminho do playwright> [--json saída.json]
//
// O que mede, cada um com o número na saída:
//
// 1. **eco**: o `ready` abre com o certificado longo do seeled (1975–4096), a
//    apresentação 0x03 chega, e o controle, a voz e um «u» de 256 KiB voltam
//    inteiros;
// 2. **adulterado**: com o `fp` do convite trocado, o `ready` recusa com
//    `VerificacaoDaConexaoSegura`, e em quanto tempo;
// 3. **nome**: o alvo escrito como `localhost:<porta>` (o DDNS do convite)
//    entra pelo `addIceCandidate`;
// 4. **rajada** (R4 da revisão): a voz a 50 mensagens/s, 160 B, ecoada pelo
//    anfitrião, antes, durante e depois de 32 MiB subindo por um «u» com a fila
//    de volume do `rtc-web.js` (o limiar global de 64 KiB), e depois a volta dos
//    32 MiB descendo pelo «u» do anfitrião. Latência p50/p95/p99/máx e perda de
//    cada fase.
//
// O binário do eco e o Playwright vêm por argumento, e nada é baixado: o
// Playwright é o que a máquina já tem (o do spike C, por exemplo).

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");
const { spawn } = require("child_process");

const [, , binario, navegador = "chromium", playwright, ...resto] = process.argv;
if (!binario || !playwright) {
  console.error("uso: node rtc-eco.cjs <binário do eco> <chromium|webkit> <caminho do playwright> [--json saída.json]");
  process.exit(2);
}
const opcao = (nome) => (resto.includes(nome) ? resto[resto.indexOf(nome) + 1] : null);
const saidaJson = opcao("--json");
// `--limiar N`: o limiar global do volume, em bytes, para medir a voz com outro
// valor que o do produto (o padrão é o `LIMIAR_DO_VOLUME` do `rtc-web.js`).
const limiar = opcao("--limiar") ? Number(opcao("--limiar")) : null;
// `--ritmo M`: a subida a no máximo M Mbit/s (o que um enlace de verdade
// deixaria), para separar o que é do navegador do que é de saturar o loopback.
const ritmo = opcao("--ritmo") ? Number(opcao("--ritmo")) : null;
// `--escuta E`: onde o eco escuta (padrão 127.0.0.1:0). Com `[::]:0`, a pilha
// dupla do produto, o caso do nome (`localhost`) separa «o navegador não
// resolve nome» de «resolveu para ::1».
const escutaDoEco = opcao("--escuta") ?? "127.0.0.1:0";
// `--canais N`: quantos fluxos de volume sobem ao mesmo tempo na rajada
// (padrão 1; 4 é «3 anexos e uma tela»).
const canais = opcao("--canais") ? Number(opcao("--canais")) : 1;
const UI = path.join(__dirname, "..", "ui");

function subirOEco() {
  return new Promise((ok, falha) => {
    const processo = spawn(binario, [escutaDoEco], { stdio: ["ignore", "pipe", "inherit"] });
    let resto = "";
    processo.stdout.on("data", (dados) => {
      resto += dados.toString();
      let fim;
      while ((fim = resto.indexOf("\n")) >= 0) {
        const linha = resto.slice(0, fim);
        resto = resto.slice(fim + 1);
        const achado = linha.match(/^ESCUTA (\S+):(\d+) FP ([0-9a-f]{64})$/);
        if (achado) ok({ processo, ip: achado[1], porta: Number(achado[2]), fp: achado[3] });
      }
    });
    processo.on("exit", (codigo) => falha(new Error(`o eco saiu com ${codigo}`)));
  });
}

function servirAPagina() {
  const servidor = http.createServer((pedido, resposta) => {
    if (pedido.url === "/rtc-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(fs.readFileSync(path.join(UI, "rtc-web.js")));
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script><p>rtc-eco</p>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Roda na página: as quatro medidas. */
async function medirNaPagina({ ip, porta, fp, limiar, ritmo, canais }) {
  const R = {};
  try {
  const espera = (ms) => new Promise((r) => setTimeout(r, ms));
  // O eco em `[::]` diz `[::]:porta`; a página o alcança pelo loopback IPv4.
  const alvo = `${ip.startsWith("[") ? "127.0.0.1" : ip}:${porta}`;

  // Para quem vigia de fora: a etapa de agora e o enlace dela (as contas do
  // crédito dizem se uma escrita espera 0x04 que não vem).
  const etapa = (nome) => (window.__etapaDaBancada = nome);
  /**
   * **O prazo de uma etapa**: vencido, a rodada para com a etapa e os
   * contadores do crédito do enlace dela (anunciados, recebidos, esperas, e o
   * concedido e não lido da conexão), em vez de esperar para sempre.
   */
  async function comPrazo(promessa, ms, nome) {
    let relogio;
    const vencido = new Promise((_, rejeitar) => {
      relogio = setTimeout(async () => {
        const enlace = window.__enlaceDaBancada;
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
  async function abrir(alvos, impressao) {
    const certificado = await prepararCertificadoDtls();
    const t0 = performance.now();
    const enlace = criarEnlaceRtc({ alvos, impressao, certificado, ...(limiar ? { limiarDoVolume: limiar } : {}) });
    window.__enlaceDaBancada = enlace;
    try {
      await comPrazo(enlace.ready, 15_000, "conectar");
      return { enlace, ms: Math.round(performance.now() - t0), certificado };
    } catch (falha) {
      return { falha, ms: Math.round(performance.now() - t0) };
    }
  }

  /** Lê um ReadableStream inteiro. */
  async function lerTudo(leitura) {
    const leitor = leitura.getReader();
    const pedacos = [];
    let total = 0;
    for (;;) {
      const { value, done } = await leitor.read();
      if (done) break;
      pedacos.push(value);
      total += value.length;
    }
    return { pedacos, total };
  }

  // ------------------------------------------------------------ 1. eco
  etapa("1. eco");
  {
    const aberto = await abrir([alvo], fp);
    R.eco = { abriu: Boolean(aberto.enlace), ms: aberto.ms, falha: aberto.falha ?? null };
    if (aberto.enlace) {
      const enlace = aberto.enlace;
      R.eco.apresentacao_bytes = enlace.apresentacao?.length ?? null;
      R.eco.atendeu_por = enlace.atendeuPor;
      R.eco.impressao_dtls = enlace.impressaoDtls;
      // O controle: 3 pedaços de 10 000 bytes, e o eco devolve os 30 000.
      const { readable, writable } = enlace.controle();
      const escritor = writable.getWriter();
      const leitor = readable.getReader();
      const enviado = new Uint8Array(30000).map((_, i) => i % 251);
      for (let i = 0; i < 3; i++) await escritor.write(enviado.subarray(i * 10000, (i + 1) * 10000));
      let voltou = 0;
      let iguais = true;
      while (voltou < enviado.length) {
        const { value, done } = await Promise.race([leitor.read(), espera(3000).then(() => ({ done: true }))]);
        if (done) break;
        for (let i = 0; i < value.length; i++) if (value[i] !== enviado[voltou + i]) iguais = false;
        voltou += value.length;
      }
      R.eco.controle = { enviados: enviado.length, voltaram: voltou, iguais };
      etapa("1. eco: a voz");
      // A voz: 100 datagramas.
      const voz = enlace.canalDaVoz.readable.getReader();
      const escritaDaVoz = enlace.canalDaVoz.writable.getWriter();
      let ecoDaVoz = 0;
      const lendoAVoz = (async () => {
        for (;;) {
          const { value, done } = await voz.read();
          if (done) break;
          if (value[0] === 7) ecoDaVoz += 1;
        }
      })();
      for (let i = 0; i < 100; i++) await escritaDaVoz.write(new Uint8Array(160).fill(7));
      await espera(500);
      R.eco.voz = { enviados: 100, voltaram: ecoDaVoz };
      voz.releaseLock();
      escritaDaVoz.releaseLock();
      // Um «u» de 256 KiB, e o eco devolve por um «u» dele.
      etapa("1. eco: o u de 256 KiB");
      const dados = new Uint8Array(256 * 1024).map((_, i) => (i * 7) % 256);
      const chegando = enlace.incomingUnidirectionalStreams.getReader().read();
      const fluxo = await comPrazo(enlace.createUnidirectionalStream(), 10_000, "1. eco: abrir o u");
      const escritorDoFluxo = fluxo.getWriter();
      await comPrazo(escritorDoFluxo.write(dados), 20_000, "1. eco: o u de 256 KiB, os dados (o crédito do anfitrião)");
      await comPrazo(escritorDoFluxo.close(), 20_000, "1. eco: o u de 256 KiB, o fim e a entrega");
      const { value: volta } = await Promise.race([chegando, espera(5000).then(() => ({ value: null }))]);
      if (volta) {
        const { pedacos, total } = await comPrazo(lerTudo(volta), 20_000, "1. eco: ler o u de volta");
        const junto = new Uint8Array(total);
        let p = 0;
        for (const pedaco of pedacos) {
          junto.set(pedaco, p);
          p += pedaco.length;
        }
        R.eco.u = { enviados: dados.length, voltaram: total, iguais: total === dados.length && junto.every((b, i) => b === dados[i]) };
      } else R.eco.u = { enviados: dados.length, voltaram: 0 };
      R.eco.contas_do_enlace = { ...enlace.contas };
      enlace.close();
      await lendoAVoz.catch(() => {});
    }
  }

  // ------------------------------------------------------- 2. adulterado
  etapa("2. adulterado");
  {
    const trocado = ((parseInt(fp.slice(0, 2), 16) ^ 0xff).toString(16).padStart(2, "0")) + fp.slice(2);
    const aberto = await abrir([alvo], trocado);
    R.adulterado = {
      abriu: Boolean(aberto.enlace),
      ms: aberto.ms,
      motivo: aberto.falha?.FalhaDoEnlace?.motivo ?? String(aberto.falha),
      detalhe: aberto.falha?.FalhaDoEnlace?.detalhe ?? null,
    };
    aberto.enlace?.close();
  }

  // ------------------------------------------------------------ 3. nome
  etapa("3. nome");
  {
    const aberto = await abrir([`localhost:${porta}`], fp);
    R.nome = { abriu: Boolean(aberto.enlace), ms: aberto.ms, falha: aberto.falha ?? null };
    aberto.enlace?.close();
  }

  // ---------------------------------------------------------- 4. rajada
  etapa("4. rajada");
  {
    const aberto = await abrir([alvo], fp);
    if (!aberto.enlace) {
      R.rajada = { abriu: false, falha: aberto.falha };
    } else {
      const enlace = aberto.enlace;
      const voz = enlace.canalDaVoz.readable.getReader();
      const escritaDaVoz = enlace.canalDaVoz.writable.getWriter();
      const enviados = new Map();
      const medidas = [];
      let seq = 0;
      (async () => {
        for (;;) {
          const { value, done } = await voz.read();
          if (done) break;
          const s = new DataView(value.buffer, value.byteOffset).getUint32(1);
          const ida = enviados.get(s);
          if (ida !== undefined) {
            medidas.push({ ida, rtt: performance.now() - ida });
            enviados.delete(s);
          }
        }
      })().catch(() => {});
      const relogio = setInterval(() => {
        const msg = new Uint8Array(160);
        msg[0] = 1;
        new DataView(msg.buffer).setUint32(1, seq);
        enviados.set(seq, performance.now());
        seq += 1;
        escritaDaVoz.write(msg).catch(() => {});
      }, 20);
      await espera(1000);
      const TOTAL = 32 * 1024 * 1024;
      // `canais` fluxos ao mesmo tempo (3 anexos e uma tela são 4), dividindo
      // os 32 MiB: é o que disputa a fila de volume de verdade.
      const porCanal = TOTAL / canais;
      // As contas do navegador para o canal da voz (o `getStats`), em cada
      // fronteira de fase: o que saiu e o que chegou, sem depender do eco.
      const contasDaVoz = async () => (await enlace.contasDosCanais()).voz ?? null;
      const janelas = { inicio: await contasDaVoz() };
      const subida = { inicio: performance.now() };
      const leitorDosFluxos = enlace.incomingUnidirectionalStreams.getReader();
      const voltas = Array.from({ length: canais }, () =>
        Promise.race([leitorDosFluxos.read(), espera(60000).then(() => ({ value: null }))]).then(async ({ value }) =>
          value ? (await lerTudo(value)).total : 0,
        ),
      );
      let maiorEspera = 0;
      const pedaco = new Uint8Array(256 * 1024).fill(3);
      await Promise.all(
        Array.from({ length: canais }, async (_, indice) => {
          const fluxo = await comPrazo(enlace.createUnidirectionalStream(), 10_000, `4. rajada: abrir o u ${indice}`);
          const escritor = fluxo.getWriter();
          for (let enviado = 0; enviado < porCanal; enviado += pedaco.length) {
            if (ritmo) {
              const devido = subida.inicio + (enviado * canais * 8) / (ritmo * 1000);
              const falta = devido - performance.now();
              if (falta > 0) await espera(falta);
            }
            await comPrazo(escritor.write(pedaco), 20_000, `4. rajada: o u ${indice}, o bloco em ${enviado}`);
            if (indice === 0) maiorEspera = Math.max(maiorEspera, (await enlace.getStats()).emEspera);
          }
          await comPrazo(escritor.close(), 30_000, `4. rajada: o u ${indice}, o fim e a entrega`);
        }),
      );
      subida.fim = performance.now();
      etapa("4. rajada: a descida");
      janelas.fimDaSubida = await contasDaVoz();
      const descida = { inicio: performance.now() };
      const recebido = (await Promise.all(voltas)).reduce((t, n) => t + n, 0);
      descida.fim = performance.now();
      janelas.fimDaDescida = await contasDaVoz();
      await espera(1500);
      clearInterval(relogio);
      await espera(500);
      const fase = (ida) =>
        ida < subida.inicio ? "antes" : ida <= subida.fim ? "subida" : ida <= descida.fim ? "descida" : "depois";
      const resumo = {};
      for (const nome of ["antes", "subida", "descida", "depois"]) {
        const v = medidas.filter((m) => fase(m.ida) === nome).map((m) => m.rtt).sort((a, b) => a - b);
        const p = (k) => (v.length ? Math.round(v[Math.min(v.length - 1, Math.floor(k * v.length))] * 10) / 10 : null);
        const perdidas = [...enviados.values()].filter((ida) => fase(ida) === nome).length;
        resumo[nome] = { recebidas: v.length, perdidas, p50: p(0.5), p95: p(0.95), p99: p(0.99), max: v.length ? Math.round(v[v.length - 1] * 10) / 10 : null };
      }
      R.rajada = {
        abriu: true,
        limiar: limiar ?? LIMIAR_DO_VOLUME,
        ritmo_mbit_s: ritmo,
        canais,
        bytes: TOTAL,
        subida_ms: Math.round(subida.fim - subida.inicio),
        subida_mbit_s: Math.round((TOTAL * 8) / (subida.fim - subida.inicio) / 1000),
        descida_ms: Math.round(descida.fim - descida.inicio),
        voltaram: recebido,
        maior_em_espera_no_volume: maiorEspera,
        // As contas do enlace da página: canais recusados, cortes por fila
        // estourada, voz descartada.
        contas_do_enlace: { ...enlace.contas },
        // Por janela, pelo `getStats` da página: as mensagens de voz que ela
        // mandou e as que recebeu (o eco). O que saiu e não voltou se perdeu
        // na ida ou na volta; a separação de onde fica com o anfitrião.
        voz_por_janela: (() => {
          const d = (a, b, campo) => (a && b ? b[campo] - a[campo] : null);
          return {
            subida: {
              enviadas: d(janelas.inicio, janelas.fimDaSubida, "mensagensEnviadas"),
              recebidas: d(janelas.inicio, janelas.fimDaSubida, "mensagensRecebidas"),
            },
            descida: {
              enviadas: d(janelas.fimDaSubida, janelas.fimDaDescida, "mensagensEnviadas"),
              recebidas: d(janelas.fimDaSubida, janelas.fimDaDescida, "mensagensRecebidas"),
            },
          };
        })(),
        voz: resumo,
      };
      enlace.close();
    }
  }
  } catch (falha) {
    // Um prazo vencido (ou outra falha) sai dito, com o que se mediu até ali.
    R.parou = falha?.prazoVencido ? falha : { falha: String(falha?.message ?? falha) };
  }
  return R;
}

(async () => {
  const eco = await subirOEco();
  const servidor = await servirAPagina();
  const { chromium, webkit } = require(playwright);
  const tipo = navegador === "webkit" ? webkit : chromium;
  const browser = await tipo.launch();
  let resultado;
  try {
    const pagina = await browser.newPage();
    pagina.on("console", (m) => {
      if (m.type() === "error" || m.type() === "warning") console.error(`[página] ${m.text()}`);
    });
    await pagina.goto(`http://127.0.0.1:${servidor.address().port}/`);
    // **Um prazo para a rodada inteira** (300 s): uma espera sem fim (um crédito
    // que não vem) sai dita, com a etapa e as contas do enlace, e não calada.
    const PRAZO_MS = 300_000;
    resultado = await Promise.race([
      pagina.evaluate(medirNaPagina, { ip: eco.ip, porta: eco.porta, fp: eco.fp, limiar, ritmo, canais }),
      new Promise((resolver) => setTimeout(resolver, PRAZO_MS)).then(async () => ({
        pendurou: await pagina.evaluate(() => ({
          etapa: window.__etapaDaBancada ?? null,
          contas: window.__enlaceDaBancada ? { ...window.__enlaceDaBancada.contas } : null,
        })),
      })),
    ]);
    resultado.navegador = `${navegador} ${browser.version()}`;
  } finally {
    await browser.close();
    servidor.close();
    eco.processo.kill();
  }
  const texto = JSON.stringify(resultado, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  if (resultado.pendurou) console.error(`rtc-eco: a rodada passou do prazo na etapa «${resultado.pendurou.etapa}»`);
  if (resultado.parou) console.error(`rtc-eco: a rodada parou: ${JSON.stringify(resultado.parou)}`);
  const ok =
    !resultado.pendurou &&
    !resultado.parou &&
    resultado.eco?.abriu &&
    resultado.eco.controle?.iguais &&
    resultado.eco.controle.voltaram === resultado.eco.controle.enviados &&
    resultado.eco.u?.iguais &&
    !resultado.adulterado?.abriu &&
    resultado.adulterado?.motivo === "VerificacaoDaConexaoSegura";
  process.exit(ok ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
