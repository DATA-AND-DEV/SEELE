// O fecho de um canal "b" do `ui/rtc-web.js`, sem navegador: o `criarEnlaceRtc`
// EXATO do arquivo, num `vm`, com um `RTCPeerConnection` falso. O caminho é o da
// página: `createBidirectionalStream()`, o pedido, o fim, a resposta lida.
//
//   node apps/seele-app/bancada/rtc-fecho-do-b.cjs
//
// O defeito que isto guarda (CI de 12/11, `sessao-no-navegador`, caso 14:
// `ler_imagem_mod` respondeu `{"Recusado":{"motivo":"caiu"}}` no lugar da recusa
// do MOD): passados os dois fins de um "b", a página E o anfitrião fechavam o
// canal ao mesmo tempo. O SCTP do anfitrião responde ao reset da página com um
// reset a mais, que chegava depois de o navegador soltar o número do canal — e
// matava o "b" seguinte, que tinha acabado de nascer com o mesmo número. Um
// `RTCPeerConnection` falso não tem SCTP: o que se prova aqui é a metade que é
// da página, isto é, que ELA não manda o fecho que se cruza.
//
// 1. **a página não fecha o "b"**: com os dois fins passados e a resposta lida,
//    `close()` não é chamado; o anfitrião fecha, e a página continua sem chamar;
// 2. **a resposta ganha do `close`**: os dados, o fim e o `close` do anfitrião
//    no mesmo tique, antes de alguém ler — quem lê recebe a resposta inteira, e
//    não `caiu` (a hipótese que a investigação descartou, guardada mesmo assim:
//    este caso já passava antes do conserto);
// 3. **o anfitrião que não fecha**: vencido o prazo, a página fecha, uma vez, e
//    conta (`contas.fechosPeloPrazo`);
// 4. **um fim só não fecha**: sem o fim da página, nem o prazo fecha.

const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const fonte = fs.readFileSync(path.join(__dirname, "..", "ui", "rtc-web.js"), "utf8");

/** O prazo do fecho nesta bancada: curto, para o caso 3 não esperar 5 s. */
const PRAZO_MS = 60;
const espera = (ms) => new Promise((r) => setTimeout(r, ms));

class Canal extends EventTarget {
  constructor(rotulo) {
    super();
    this.label = rotulo;
    this.readyState = "connecting";
    this.bufferedAmount = 0;
    this.enviados = [];
    this.fechosDaPagina = 0;
  }
  send(bytes) {
    this.enviados.push([...new Uint8Array(bytes)]);
  }
  /** O `close()` que a PÁGINA chama. */
  close() {
    this.fechosDaPagina += 1;
    this.readyState = "closed";
    this.dispatchEvent(new Event("close"));
  }
  abrir() {
    this.readyState = "open";
    this.dispatchEvent(new Event("open"));
  }
  /** Uma mensagem do anfitrião. */
  chegar(bytes) {
    const evento = new Event("message");
    evento.data = new Uint8Array(bytes).buffer;
    this.dispatchEvent(evento);
  }
  /** O fecho que vem do anfitrião (o reset dele). */
  fecharPeloAnfitriao() {
    this.readyState = "closed";
    this.dispatchEvent(new Event("close"));
  }
}

function enlace() {
  const canais = [];
  class PC extends EventTarget {
    constructor() {
      super();
      this.connectionState = "connecting";
      this.iceConnectionState = "checking";
      this.iceGatheringState = "gathering";
      this.sctp = { transport: { state: "new" } };
    }
    createDataChannel(rotulo) {
      const canal = new Canal(rotulo);
      canais.push(canal);
      return canal;
    }
    createOffer() {
      return new Promise(() => {});
    }
    close() {
      this.connectionState = this.iceConnectionState = "closed";
    }
  }
  // Os relógios do enlace (o prazo da tentativa, o keepalive) não seguram o
  // processo: quem espera aqui é a bancada, com os relógios dela.
  const solto = (marcar) => (...args) => {
    const relogio = marcar(...args);
    relogio.unref?.();
    return relogio;
  };
  const contexto = vm.createContext({
    RTCPeerConnection: PC, console, setTimeout: solto(setTimeout), clearTimeout, setInterval: solto(setInterval), clearInterval, performance,
    crypto: require("node:crypto").webcrypto, Uint8Array, ArrayBuffer, TextEncoder, TextDecoder,
    ReadableStream, WritableStream, TransformStream, ByteLengthQueuingStrategy, AbortController,
  });
  vm.runInContext(fonte, contexto);
  const um = contexto.criarEnlaceRtc({
    alvos: ["203.0.113.7:8383"],
    impressao: "ab".repeat(32),
    certificado: { certificado: {}, impressao: "ab".repeat(32) },
    prazo: 60_000,
    prazoDoFechoDoB: PRAZO_MS,
  });
  um.ready.catch(() => {});
  return { um, canais, contexto };
}

/** Abre um "b" pela porta da página, com o crédito do anfitrião já anunciado. */
async function abrirUmB({ um, canais, contexto }) {
  const abrindo = um.createBidirectionalStream();
  const canal = canais.at(-1);
  assert.equal(canal.label, "b");
  canal.abrir();
  const fluxo = await abrindo;
  canal.chegar(contexto.envelopeDoCredito(1024 * 1024));
  return { canal, fluxo };
}

/** O pedido e o fim da página: o `transferirAImagem` da ponte faz isto. */
async function pedir(fluxo) {
  const escritor = fluxo.writable.getWriter();
  await escritor.write(new Uint8Array([1, 2, 3]));
  await escritor.close();
}

async function lerTudo(fluxo) {
  const leitor = fluxo.readable.getReader();
  const lidos = [];
  for (;;) {
    const { value, done } = await leitor.read();
    if (done) return lidos;
    lidos.push(...value);
  }
}

const DADOS = 0x00;
const FIM = 0x01;

test("a página não fecha o «b»: quem fecha, passados os dois fins, é o anfitrião", async () => {
  const bancada = enlace();
  const { canal, fluxo } = await abrirUmB(bancada);
  await pedir(fluxo);
  assert.deepEqual(canal.enviados.at(-1), [FIM], "o fim da página não saiu");
  canal.chegar([DADOS, 7, 8, 9]);
  canal.chegar([FIM]);
  assert.deepEqual(await lerTudo(fluxo), [7, 8, 9]);
  // Os dois fins passaram e a resposta foi lida: era aqui que a página fechava.
  await espera(PRAZO_MS / 3);
  assert.equal(canal.fechosDaPagina, 0, "a página fechou o «b» por conta própria: o fecho dela se cruza com o do anfitrião");
  canal.fecharPeloAnfitriao();
  await espera(PRAZO_MS * 2);
  assert.equal(canal.fechosDaPagina, 0, "o anfitrião já tinha fechado, e a página fechou mesmo assim");
  assert.equal(bancada.um.contas.fechosPeloPrazo, 0);
  bancada.um.close();
});

test("a resposta já recebida ganha do close: dados, fim e close no mesmo tique", async () => {
  const bancada = enlace();
  const { canal, fluxo } = await abrirUmB(bancada);
  await pedir(fluxo);
  canal.chegar([DADOS, 7, 8, 9]);
  canal.chegar([FIM]);
  canal.fecharPeloAnfitriao();
  assert.deepEqual(await lerTudo(fluxo), [7, 8, 9], "quem lê tinha de receber a resposta, e não «caiu»");
  bancada.um.close();
});

test("o anfitrião que não fecha: vencido o prazo a página fecha, uma vez, e conta", async () => {
  const bancada = enlace();
  const { canal, fluxo } = await abrirUmB(bancada);
  await pedir(fluxo);
  canal.chegar([DADOS, 7]);
  canal.chegar([FIM]);
  assert.deepEqual(await lerTudo(fluxo), [7]);
  await espera(PRAZO_MS * 3);
  assert.equal(canal.fechosDaPagina, 1, "sem o fecho do anfitrião, o canal ficou aberto para sempre (ou foi fechado duas vezes)");
  assert.equal(bancada.um.contas.fechosPeloPrazo, 1, "o fecho pelo prazo não foi contado");
  bancada.um.close();
});

test("um fim só não fecha: sem o fim da página, nem o prazo fecha o «b»", async () => {
  const bancada = enlace();
  const { canal, fluxo } = await abrirUmB(bancada);
  canal.chegar([DADOS, 7]);
  canal.chegar([FIM]);
  assert.deepEqual(await lerTudo(fluxo), [7]);
  await espera(PRAZO_MS * 3);
  assert.equal(canal.fechosDaPagina, 0, "a página fechou um «b» em que ela ainda escreve");
  assert.equal(bancada.um.contas.fechosPeloPrazo, 0);
  bancada.um.close();
});
