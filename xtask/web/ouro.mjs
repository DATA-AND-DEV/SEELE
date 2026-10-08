// Os bytes-ouro da linha 1, conferidos DENTRO do `.wasm` do `seele-web` e pela
// fronteira de verdade, com o JSON passando pelo JavaScript.
//
//     node xtask/web/ouro.mjs <seele_web.wasm> <crates/seele-proto/tests/fio_1_0.txt>
//
// O `.wasm` é o do `wasm32-unknown-unknown`, com a feature `prova`, que acrescenta
// duas funções só de teste: o JSON de uma mensagem do cliente e o quadro de uma
// mensagem do servidor. As outras quatro são as que a casca vai usar.
//
// Cada mensagem do cliente: o JSON dela vai ao JavaScript (`JSON.parse` e
// `JSON.stringify`), volta por `codificar`, e tem de sair com os bytes do fio.
// Cada mensagem do servidor: sai por `decodificar`, passa pelo JavaScript, e tem
// de codificar de novo nos mesmos bytes. Um número que o `Number` do JavaScript
// não guarda (um `u64` acima de 2^53) muda os bytes e reprova aqui.
import { readFileSync } from "node:fs";

const [caminhoDoWasm, caminhoDoFio] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(caminhoDoWasm), {});
const e = instance.exports;
const enc = new TextEncoder();
const dec = new TextDecoder();
const memoria = () => new Uint8Array(e.memory.buffer);
const saida = () => memoria().slice(e.saida_ptr(), e.saida_ptr() + e.saida_len());

function comBytes(bytes, f) {
  const p = e.alocar(bytes.length);
  memoria().set(bytes, p);
  try {
    return f(p, bytes.length);
  } finally {
    e.liberar(p, bytes.length);
  }
}
const deHex = (h) => Uint8Array.from(h.match(/../g), (x) => parseInt(x, 16));
const iguais = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);

const falhas = [];
let cliente = 0;
let servidor = 0;
for (const linha of readFileSync(caminhoDoFio, "utf8").split("\n")) {
  if (!linha || linha.startsWith("#")) continue;
  const [nome, hex] = linha.split("\t");
  const ouro = deHex(hex);
  if (nome.startsWith("cliente:")) {
    cliente++;
    if (comBytes(ouro, (p, n) => e.json_do_cliente(p, n)) !== 0) { falhas.push(`${nome}: json_do_cliente: ${dec.decode(saida())}`); continue; }
    const json = JSON.stringify(JSON.parse(dec.decode(saida())));
    if (comBytes(enc.encode(json), (p, n) => e.codificar(ouro[0], p, n)) !== 0) { falhas.push(`${nome}: codificar: ${dec.decode(saida())}`); continue; }
    if (!iguais(saida(), ouro)) falhas.push(`${nome}: a fronteira codificou outros bytes`);
  } else if (nome.startsWith("servidor:")) {
    servidor++;
    if (comBytes(ouro, (p, n) => e.decodificar(p, n)) !== 0) { falhas.push(`${nome}: decodificar: ${dec.decode(saida())}`); continue; }
    const lido = JSON.parse(dec.decode(saida()));
    if (!("mensagem" in lido)) { falhas.push(`${nome}: chegou como ${JSON.stringify(lido)}`); continue; }
    const json = JSON.stringify(lido.mensagem);
    if (comBytes(enc.encode(json), (p, n) => e.quadro_do_servidor(ouro[0], p, n)) !== 0) { falhas.push(`${nome}: quadro_do_servidor: ${dec.decode(saida())}`); continue; }
    if (!iguais(saida(), ouro)) falhas.push(`${nome}: perdeu algo no caminho pelo JavaScript`);
  }
}

// Um ordinal de vocabulário mais novo (10 000, varint 0x90 0x4e) chega como desconhecido.
if (comBytes(Uint8Array.of(9, 0x90, 0x4e), (p, n) => e.decodificar(p, n)) !== 0) falhas.push("desconhecida: virou erro");
else if (JSON.parse(dec.decode(saida())).desconhecida !== 10000) falhas.push(`desconhecida: ${dec.decode(saida())}`);

// A recusa da 0.15.1, `[8, 11, 0]`: erro, com a versão que veio.
if (comBytes(Uint8Array.of(8, 11, 0), (p, n) => e.decodificar(p, n)) !== 1) falhas.push("carimbo 8: não foi recusado");
else if (JSON.parse(dec.decode(saida())).versao !== 8) falhas.push(`carimbo 8: ${dec.decode(saida())}`);

if (cliente === 0 || servidor === 0) falhas.push(`o arquivo trouxe ${cliente} do cliente e ${servidor} do servidor; este guarda ficou cego`);
if (falhas.length) {
  console.error(`seele-web no .wasm: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log(`seele-web no .wasm: ${cliente} mensagens do cliente e ${servidor} do servidor pela fronteira, com os bytes do fio; a desconhecida e a recusa da 0.15.1 também.`);
