// O anfitrião do motor da candidata A: instancia o `.wasm`, fornece as três
// funções do módulo `seele` e a WASI mínima, e faz a volta que o desktop faz
// (avaliar, recolher os pedidos de relógio, disparar os relógios).
//
// Roda dentro do Worker (candidato-quickjs-worker.js) e no Node (teste-motor.mjs).
// Descartável.
"use strict";

function criarMotor(modulo, { aoPostar, aoConsole, aoErroNaVolta, agendar, cancelar }) {
  let inst = null;
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  const mem = () => new Uint8Array(inst.exports.memory.buffer);
  const dv = () => new DataView(inst.exports.memory.buffer);
  const ler = (p, n) => dec.decode(mem().slice(p, p + n));

  // A WASI que o QuickJS pede (WebAssembly.Module.imports do motor): o relógio,
  // o aleatório, um ambiente vazio e a escrita no log. Nada de disco nem rede:
  // o `wasm32-wasip1` declara essas funções, e aqui elas não existem.
  const wasi = {
    random_get(p, n) { const b = mem(); for (let i = 0; i < n; i += 65536) crypto.getRandomValues(b.subarray(p + i, p + Math.min(n, i + 65536))); return 0; },
    environ_sizes_get(pc, ps) { dv().setUint32(pc, 0, true); dv().setUint32(ps, 0, true); return 0; },
    environ_get() { return 0; },
    clock_time_get(id, _precisao, p) {
      const ns = id === 0 ? BigInt(Date.now()) * 1000000n : BigInt(Math.round(performance.now() * 1e6));
      dv().setBigUint64(p, ns, true);
      return 0;
    },
    fd_close() { return 8; },
    fd_fdstat_get() { return 8; },
    fd_seek() { return 8; },
    fd_write(fd, iovs, n, escritos) {
      let total = 0;
      let texto = "";
      for (let i = 0; i < n; i++) {
        const p = dv().getUint32(iovs + i * 8, true);
        const l = dv().getUint32(iovs + i * 8 + 4, true);
        texto += ler(p, l);
        total += l;
      }
      aoConsole(fd === 2 ? "stderr" : "stdout", texto);
      dv().setUint32(escritos, total, true);
      return 0;
    },
    proc_exit(c) { throw new Error(`o motor pediu proc_exit(${c})`); },
  };
  const seele = {
    postar(p, n) { return aoPostar(ler(p, n)) ? 1 : 0; },
    console(mp, ml, tp, tl) { aoConsole(ler(mp, ml), ler(tp, tl)); },
    agora_ms() { return performance.now(); },
  };

  const comTexto = (texto, f) => {
    const b = enc.encode(texto);
    const p = inst.exports.alocar(b.length);
    mem().set(b, p);
    try { return f(p, b.length); } finally { inst.exports.liberar(p, b.length); }
  };
  const saida = () => ler(inst.exports.saida_ptr(), inst.exports.saida_len());

  // Os relógios do MOD, que o prelúdio pede por vetor (executor.rs:1733).
  const relogios = new Map();
  const recolher = () => {
    inst.exports.pedidos_de_relogio();
    let pedidos = [];
    try { pedidos = JSON.parse(saida()); } catch (_) {}
    for (const r of pedidos) {
      if (r.cancelar) { const h = relogios.get(r.id); if (h) cancelar(h); relogios.delete(r.id); continue; }
      // O mínimo de 4 ms do desktop (executor.rs:436).
      const ms = Math.max(r.repete ? 4 : 0, r.ms);
      const h = agendar(() => {
        if (!r.repete) relogios.delete(r.id);
        const c = inst.exports.relogio(r.id);
        if (c !== 0) aoErroNaVolta(saida());
        recolher();
      }, ms, r.repete);
      relogios.set(r.id, h);
    }
  };

  return {
    async subir(capacidades, fonte) {
      const r = await WebAssembly.instantiate(modulo, { seele, wasi_snapshot_preview1: wasi });
      inst = r.exports ? r : r.instance;
      const c = comTexto(capacidades.join(","), (cp, cl) => comTexto(fonte, (fp, fl) => inst.exports.iniciar(cp, cl, fp, fl)));
      if (c !== 0) return { erro: saida() };
      recolher();
      return { ok: true };
    },
    entregar(json) {
      const c = comTexto(json, (p, l) => inst.exports.entregar(p, l));
      if (c !== 0) aoErroNaVolta(saida());
      recolher();
    },
    encerrar() { for (const h of relogios.values()) cancelar(h); relogios.clear(); },
    memoria() {
      return { motor_malloc: inst ? inst.exports.memoria_do_motor() : 0, wasm_memoria: inst ? inst.exports.memory.buffer.byteLength : 0 };
    },
  };
}

if (typeof module !== "undefined") module.exports = { criarMotor };
