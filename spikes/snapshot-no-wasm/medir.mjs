// O custo do retrato no V8 do Node (o mesmo motor do Chromium), por tamanho de
// sala: o retrato montado e serializado no `wasm`, copiado e lido pelo
// `JSON.parse`, que é o que a ponte faria a cada `invoke("snapshot")`.
//
//     node medir.mjs target/wasm32-unknown-unknown/release/snapshot_no_wasm.wasm
import fs from "node:fs";

const bytes = fs.readFileSync(process.argv[2]);
const { instance } = await WebAssembly.instantiate(bytes, {});
const e = instance.exports;
const importacoes = WebAssembly.Module.imports(new WebAssembly.Module(bytes));
console.log("importações do .wasm:", importacoes.length ? importacoes.map((i) => `${i.module}.${i.name}`).join(", ") : "nenhuma");
const dec = new TextDecoder();
const VOLTAS = 300;
const resultados = [];
for (const [pessoas, salas, canais] of [[10, 3, 5], [50, 5, 10], [250, 10, 20], [500, 10, 40], [1000, 10, 40]]) {
  e.montar(pessoas, salas, canais);
  let retrato = null;
  const tempos = [];
  for (let i = 0; i < VOLTAS + 30; i++) {
    const t0 = performance.now();
    const n = e.retratar();
    const texto = dec.decode(new Uint8Array(e.memory.buffer, e.saida_ptr(), n));
    retrato = JSON.parse(texto);
    const t1 = performance.now();
    if (i >= 30) tempos.push(t1 - t0);
  }
  tempos.sort((a, b) => a - b);
  const n = e.saida_len();
  const r = {
    pessoas, salas, canais,
    bytes: n,
    mediana_ms: +tempos[Math.floor(tempos.length / 2)].toFixed(3),
    p95_ms: +tempos[Math.floor(tempos.length * 0.95)].toFixed(3),
    na_sala_1: retrato.voice_rooms[0].people.length,
    presentes: retrato.presentes.length,
  };
  resultados.push(r);
  console.log(JSON.stringify(r));
}
