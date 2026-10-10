// A voz na thread de áudio (Tarefa 3.3 da F4, lote C): os dois processadores
// do `AudioWorklet`, a captura e a reprodução.
//
// **Este arquivo não é carregado pela `index.html`.** Quem o carrega é a
// `voz-web.js`, com `audioWorklet.addModule`, e ele roda num escopo sem
// `window` nem `document`. O `tests/frontend.rs` sabe disso, e confere que é a
// `voz-web.js` quem o carrega.
//
// # O relógio é este
//
// O `process()` é chamado pelo relógio da placa de som, de 128 em 128
// amostras, com a página visível ou não. **Nada da voz depende de
// temporizador** (ADR 0057): com a página oculta, o `setInterval` cai para um
// por segundo. A captura junta 960 amostras e entrega um quadro de 20 ms à
// página; a reprodução pede um quadro novo à mistura cada vez que o anterior
// acaba de tocar.
//
// # A conta não é daqui
//
// O buffer de jitter e o mixer são os do desktop, no `.wasm` do `seele-web`
// (`voz::Reproducao`), compilados **aqui dentro** a partir dos bytes: o
// Chromium recusa um `WebAssembly.Module` mandado pela porta
// (`spikes/voz-em-wasm/`). Este arquivo move bytes e alças, e não sabe o que é
// um SSRC.
//
// # O que falha diz que falhou
//
// Uma mensagem que não atravessa a porta chega como `messageerror`, e sem um
// ouvinte nele ela é silenciosa: foi o que o spike viu, uma espera sem
// resposta. Os dois processadores ouvem e contam à página.

const QUADRO_DA_VOZ = 960;

/** Quantos quadros de 20 ms entre dois relatos à página: um segundo. */
const QUADROS_POR_RELATO = 50;

/**
 * Os bytes UTF-8 da saída do `.wasm` em texto. O escopo do worklet não tem
 * `TextDecoder` (ele é dos Workers e da janela, e um worklet não é nenhum
 * dos dois).
 */
function textoDeUtf8(bytes) {
  let texto = "";
  for (let i = 0; i < bytes.length; ) {
    const b = bytes[i];
    const n = b < 0x80 ? 1 : b < 0xe0 ? 2 : b < 0xf0 ? 3 : 4;
    let ponto = n === 1 ? b : b & (0xff >> (n + 1));
    for (let k = 1; k < n; k++) ponto = (ponto << 6) | (bytes[i + k] & 0x3f);
    texto += String.fromCodePoint(ponto);
    i += n;
  }
  return texto;
}

function falhaNaPorta(porta, onde) {
  porta.postMessage({
    tipo: "falha",
    onde,
    texto: "uma mensagem da página não atravessou a porta do worklet (messageerror)",
  });
}

/**
 * Junta os blocos do microfone em quadros de 20 ms e os entrega à página, que
 * passa cada um pelo portão e pelo `AudioEncoder`.
 */
class CapturaDaVoz extends AudioWorkletProcessor {
  constructor() {
    super();
    this.quadro = new Float32Array(QUADRO_DA_VOZ);
    this.cheio = 0;
    this.entregues = 0;
    /** Blocos em que o microfone não entregou canal nenhum. */
    this.semEntrada = 0;
    this.port.onmessageerror = () => falhaNaPorta(this.port, "captura");
  }

  process(entradas) {
    const canal = entradas[0] && entradas[0][0];
    if (!canal || canal.length === 0) {
      this.semEntrada++;
      return true;
    }
    let i = 0;
    while (i < canal.length) {
      const n = Math.min(canal.length - i, QUADRO_DA_VOZ - this.cheio);
      this.quadro.set(canal.subarray(i, i + n), this.cheio);
      this.cheio += n;
      i += n;
      if (this.cheio === QUADRO_DA_VOZ) {
        this.entregues++;
        this.port.postMessage(
          { tipo: "quadro", pcm: this.quadro, entregues: this.entregues, semEntrada: this.semEntrada },
          [this.quadro.buffer],
        );
        this.quadro = new Float32Array(QUADRO_DA_VOZ);
        this.cheio = 0;
      }
    }
    return true;
  }
}

/**
 * Toca a mistura: um buffer de jitter por alça e o mixer, no `.wasm`.
 *
 * Até o `.wasm` compilar, sai silêncio, e o que chegar fica na fila para ele.
 */
class ReproducaoDaVoz extends AudioWorkletProcessor {
  constructor(opcoes) {
    super();
    this.e = null;
    this.antesDoWasm = [];
    this.misturado = new Float32Array(QUADRO_DA_VOZ);
    this.bytesDoMisturado = new Uint8Array(this.misturado.buffer);
    this.posicao = QUADRO_DA_VOZ;
    this.blocos = 0;
    this.blocosComSom = 0;
    this.quadros = 0;
    this.falhasDaMistura = 0;
    this.port.onmessage = (evento) => this.receber(evento.data);
    this.port.onmessageerror = () => falhaNaPorta(this.port, "reprodução");

    const bytes = opcoes && opcoes.processorOptions && opcoes.processorOptions.wasm;
    if (!(bytes instanceof ArrayBuffer)) {
      this.port.postMessage({ tipo: "falha", onde: "reprodução", texto: "o worklet nasceu sem os bytes do .wasm" });
      return;
    }
    WebAssembly.instantiate(bytes, {}).then(
      ({ instance }) => {
        this.e = instance.exports;
        this.e.voz_tocar();
        for (const mensagem of this.antesDoWasm) this.receber(mensagem);
        this.antesDoWasm = [];
        this.port.postMessage({ tipo: "pronto" });
      },
      (erro) =>
        this.port.postMessage({
          tipo: "falha",
          onde: "reprodução",
          texto: `o .wasm não compilou no worklet: ${erro && erro.message ? erro.message : erro}`,
        }),
    );
  }

  /** O que a página manda: um quadro decodificado, um controle ou um pedido. */
  receber(m) {
    if (!this.e) {
      this.antesDoWasm.push(m);
      return;
    }
    const e = this.e;
    switch (m && m.tipo) {
      case "chegou": {
        const bytes = new Uint8Array(m.pcm.buffer, m.pcm.byteOffset, m.pcm.byteLength);
        const p = e.alocar(bytes.length);
        new Uint8Array(e.memory.buffer).set(bytes, p);
        const codigo = e.voz_tocar_chegou(m.alca, m.seq, m.carimbo, m.chegada, p, bytes.length);
        e.liberar(p, bytes.length);
        if (codigo !== 0) this.port.postMessage({ tipo: "falha", onde: "reprodução", texto: this.saidaEmTexto() });
        break;
      }
      case "ganho":
        e.voz_tocar_ganho(m.alca, m.ganho);
        break;
      case "isolado":
        e.voz_tocar_isolado(m.isolado ? 1 : 0);
        break;
      case "esquecer":
        e.voz_tocar_esquecer(m.alca);
        break;
      case "recomecar":
        // Uma recepção nova (a reconexão): as alças recomeçam do um, e as velhas
        // não podem tocar no lugar delas.
        e.voz_tocar();
        break;
      case "contas":
        if (e.voz_tocar_contas() === 0) {
          this.port.postMessage({ tipo: "contas", pedido: m.pedido, contas: JSON.parse(this.saidaEmTexto()), ...this.andamento() });
        }
        break;
      default:
        this.port.postMessage({ tipo: "falha", onde: "reprodução", texto: `uma mensagem sem tipo conhecido: ${m && m.tipo}` });
    }
  }

  saidaEmTexto() {
    const e = this.e;
    return textoDeUtf8(new Uint8Array(e.memory.buffer, e.saida_ptr(), e.saida_len()));
  }

  andamento() {
    return { blocos: this.blocos, blocosComSom: this.blocosComSom, quadros: this.quadros, falhasDaMistura: this.falhasDaMistura };
  }

  /** Os próximos 20 ms da mistura, ou silêncio enquanto o `.wasm` não chegou. */
  proximoQuadro() {
    this.posicao = 0;
    if (!this.e) {
      this.misturado.fill(0);
      return;
    }
    const e = this.e;
    if (e.voz_tocar_quadro() !== 0) {
      this.falhasDaMistura++;
      this.misturado.fill(0);
      return;
    }
    this.bytesDoMisturado.set(new Uint8Array(e.memory.buffer, e.saida_ptr(), e.saida_len()));
    this.quadros++;
    if (this.quadros % QUADROS_POR_RELATO === 0) {
      this.port.postMessage({ tipo: "andamento", ...this.andamento() });
    }
  }

  process(_entradas, saidas) {
    const canais = saidas[0];
    const saida = canais[0];
    let escrito = 0;
    while (escrito < saida.length) {
      if (this.posicao === QUADRO_DA_VOZ) this.proximoQuadro();
      const n = Math.min(saida.length - escrito, QUADRO_DA_VOZ - this.posicao);
      saida.set(this.misturado.subarray(this.posicao, this.posicao + n), escrito);
      escrito += n;
      this.posicao += n;
    }
    for (let c = 1; c < canais.length; c++) canais[c].set(saida);
    this.blocos++;
    for (let k = 0; k < saida.length; k++) {
      if (saida[k] > 1e-4 || saida[k] < -1e-4) {
        this.blocosComSom++;
        break;
      }
    }
    return true;
  }
}

registerProcessor("seele-captura-da-voz", CapturaDaVoz);
registerProcessor("seele-reproducao-da-voz", ReproducaoDaVoz);
