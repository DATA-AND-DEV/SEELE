// Os dois processadores de áudio do spike. Rodam na thread de áudio, fora da
// página: o que chega aqui não pode esperar a thread principal.

const QUADRO = 960; // 20 ms a 48 kHz, o quadro do Opus no produto (specs/03-audio.md)

// Junta os blocos de 128 amostras do navegador em quadros de 20 ms e os entrega
// à página. A página carimba a hora de chegada, que é a "hora da captura" da
// medida de eco: ela já inclui os 20 ms de acúmulo, e o README diz isso.
class Captura extends AudioWorkletProcessor {
  constructor() {
    super();
    this.quadro = new Float32Array(QUADRO);
    this.cheio = 0;
  }
  process(entradas) {
    const canal = entradas[0] && entradas[0][0];
    if (!canal) return true;
    let i = 0;
    while (i < canal.length) {
      const n = Math.min(canal.length - i, QUADRO - this.cheio);
      this.quadro.set(canal.subarray(i, i + n), this.cheio);
      this.cheio += n;
      i += n;
      if (this.cheio === QUADRO) {
        this.port.postMessage(this.quadro, [this.quadro.buffer]);
        this.quadro = new Float32Array(QUADRO);
        this.cheio = 0;
      }
    }
    return true;
  }
}

// Uma fila por quem fala, misturadas na saída. A fila espera dois quadros
// (40 ms) antes de tocar e descarta o mais velho acima de dez (200 ms): é o
// buffer de jitter mais simples que ainda separa "a rede atrasou" de "a rede
// perdeu". Contadores voltam à página a cada segundo.
class Reproducao extends AudioWorkletProcessor {
  constructor() {
    super();
    this.filas = new Map(); // id -> { pedacos: Float32Array[], pos, tocando }
    this.faltas = 0;
    this.descartes = 0;
    this.blocos = 0;
    // Blocos de 128 amostras que saíram com som. É o que diz, sem perguntar a
    // ninguém, se a voz tocou enquanto a tela estava bloqueada.
    this.comSom = 0;
    this.port.onmessage = (e) => {
      const { id, pcm } = e.data;
      let fila = this.filas.get(id);
      if (!fila) {
        fila = { pedacos: [], pos: 0, tocando: false };
        this.filas.set(id, fila);
      }
      fila.pedacos.push(pcm);
      while (fila.pedacos.length > 10) {
        fila.pedacos.shift();
        fila.pos = 0;
        this.descartes++;
      }
    };
  }
  process(_entradas, saidas) {
    const saida = saidas[0][0];
    saida.fill(0);
    for (const fila of this.filas.values()) {
      if (!fila.tocando) {
        if (fila.pedacos.length < 2) continue;
        fila.tocando = true;
      }
      let escrito = 0;
      while (escrito < saida.length) {
        const atual = fila.pedacos[0];
        if (!atual) {
          this.faltas++;
          fila.tocando = false;
          break;
        }
        const n = Math.min(saida.length - escrito, atual.length - fila.pos);
        for (let k = 0; k < n; k++) saida[escrito + k] += atual[fila.pos + k];
        escrito += n;
        fila.pos += n;
        if (fila.pos === atual.length) {
          fila.pedacos.shift();
          fila.pos = 0;
        }
      }
    }
    for (let c = 1; c < saidas[0].length; c++) saidas[0][c].set(saida);
    for (let k = 0; k < saida.length; k++) {
      if (saida[k] > 1e-4 || saida[k] < -1e-4) {
        this.comSom++;
        break;
      }
    }
    if (++this.blocos % 375 === 0) {
      const ms = {};
      for (const [id, fila] of this.filas) ms[id] = fila.pedacos.length * 20;
      this.port.postMessage({ faltas: this.faltas, descartes: this.descartes, bufferMs: ms, blocos: this.blocos, comSom: this.comSom });
    }
    return true;
  }
}

registerProcessor("captura", Captura);
registerProcessor("reproducao", Reproducao);
