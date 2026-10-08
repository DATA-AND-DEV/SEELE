// O lado wasm: instancia o .wasm sem cola nenhuma (sem wasm-bindgen) e imprime
// em hex o buffer que `rodar()` deixou. Uso: node prova.mjs <arquivo.wasm>
import { readFileSync } from "node:fs";

const bytes = readFileSync(process.argv[2]);
const { instance } = await WebAssembly.instantiate(bytes, {});
const e = instance.exports;
const codigo = e.rodar();
const vista = new Uint8Array(e.memory.buffer, e.saida_ptr(), e.saida_len());
if (codigo !== 0) {
  console.error("DEFEITO:", new TextDecoder().decode(vista));
  process.exit(1);
}
console.log(Array.from(vista, (x) => x.toString(16).padStart(2, "0")).join(""));
