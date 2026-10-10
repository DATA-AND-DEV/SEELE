// O AudioWorklet da voz, com o portão, o buffer de jitter e o mixer do
// desktop em wasm. A cada 960 amostras (20 ms), um quadro; na primeira volta
// com o motor pronto, um lote cronometrado.
class VozEmWasm extends AudioWorkletProcessor {
  constructor() {
    super();
    this.motor = null;
    this.amostras = 0;
    this.quadros = 0;
    this.blocos = 0;
    this.medido = false;
    // Uma mensagem que não se desserializa aqui chega como `messageerror`, e
    // sem este ouvinte ela some calada.
    this.port.onmessageerror = (evento) => {
      this.port.postMessage({ tipo: "erro", via: "messageerror", erro: String(evento?.data ?? evento?.type ?? "messageerror") });
    };
    this.port.onmessage = (mensagem) => {
      const dado = mensagem.data;
      try {
        if (dado.tipo === "modulo") this.subir(dado.modulo, "modulo-pela-porta");
        else if (dado.tipo === "bytes") this.subir(new WebAssembly.Module(dado.bytes), "bytes-compilados-no-worklet");
      } catch (falha) {
        this.port.postMessage({ tipo: "erro", via: dado.tipo, erro: String(falha) });
      }
    };
  }
  subir(modulo, via) {
    this.motor = new WebAssembly.Instance(modulo, {}).exports;
    this.motor.preparar(8);
    this.port.postMessage({ tipo: "pronto", via });
  }
  process() {
    if (!this.motor) return true;
    this.blocos += 1;
    if (!this.medido) {
      this.medido = true;
      const voltas = 2000;
      const inicio = Date.now();
      let soma = 0;
      for (let i = 0; i < voltas; i++) soma += this.motor.quadro(i * 20);
      const ms = Date.now() - inicio;
      this.port.postMessage({ tipo: "lote", voltas, ms, finito: Number.isFinite(soma) });
    }
    this.amostras += 128;
    while (this.amostras >= 960) {
      this.amostras -= 960;
      this.motor.quadro(currentTime * 1000);
      this.quadros += 1;
    }
    if (this.blocos % 375 === 0) this.port.postMessage({ tipo: "andamento", blocos: this.blocos, quadros: this.quadros });
    return true;
  }
}
registerProcessor("voz-em-wasm", VozEmWasm);
