// O teto de pixels e a redução de imagem, DENTRO do `.wasm` do `seele-web` e
// pela fronteira de verdade.
//
//     node xtask/web/imagens.mjs <seele_web.wasm>
//
// # Por que isto não se prova no nativo
//
// No `wasm32` o endereço tem 32 bits. Um PNG que declara 30 000 × 30 000 não
// tem tamanho que caiba num `usize`, e o decodificador o recusa **por limite,
// antes de dizer as medidas**: medido em 11/11/2026, a mesma imagem que o
// desktop recusava com os números («30000 × 30000, e o teto é…») a página
// recusava como «formato desconhecido» e «não abriu». As medidas do PNG saem
// então do `IHDR`, lidas à mão (`reducao::medidas`) — um ramo que o teste
// nativo nunca atravessa, porque lá o decodificador responde. Sem esta prova,
// ele seria código que existe e que ninguém viu funcionar.
//
// # O que ela mede
//
// 1. **A prévia de um anexo** (`previa_julgar`, que a página usa também para a
//    imagem de um link): uma imagem em 4K chega em 1280 × 720, e a que declara
//    pixels demais é recusada como `TooManyPixels`, com as medidas e o teto.
// 2. **A mídia que um MOD recebe do servidor** (`mod_midia_do_servidor`): sem
//    uso, 1920 × 1080; como `fundo`, 1280 × 720; e a que declara pixels demais
//    é `imagem-grande-demais-decodificada`, com os números.
import { readFileSync } from "node:fs";
import zlib from "node:zlib";

const [caminhoDoWasm] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(caminhoDoWasm), {});
const e = instance.exports;
const enc = new TextEncoder();
const dec = new TextDecoder();
const memoria = () => new Uint8Array(e.memory.buffer);
const saida = () => JSON.parse(dec.decode(memoria().slice(e.saida_ptr(), e.saida_ptr() + e.saida_len())));
function comBlocos(blocos, f) {
  const postos = blocos.map((b) => {
    const p = e.alocar(b.length);
    memoria().set(b, p);
    return [p, b.length];
  });
  try {
    return f(...postos.flat());
  } finally {
    for (const [p, n] of postos) e.liberar(p, n);
  }
}

// O CRC do PNG, escrito aqui: o `zlib.crc32` do node só existe nas versões novas.
const TABELA = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = TABELA[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function pedaco(tipo, dados) {
  const nome = Buffer.from(tipo, "latin1");
  const tamanho = Buffer.alloc(4);
  tamanho.writeUInt32BE(dados.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([nome, dados])));
  return Buffer.concat([tamanho, nome, dados, crc]);
}
/**
 * Um PNG RGB liso. `inteiro` grava os pixels todos (comprimem a quase nada);
 * sem ele, só o cabeçalho diz as medidas, e o IDAT quase não tem dados — é a
 * imagem de poucos bytes que aberta pediria gigabytes.
 */
function png(largura, altura, inteiro = true) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(largura, 0);
  ihdr.writeUInt32BE(altura, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const linha = largura * 3 + 1;
  const cru = inteiro ? Buffer.alloc(linha * altura, 0x50) : Buffer.alloc(64);
  if (inteiro) for (let y = 0; y < altura; y += 1) cru[y * linha] = 0;
  return Buffer.concat([
    Buffer.from("\x89PNG\r\n\x1a\n", "latin1"),
    pedaco("IHDR", ihdr),
    pedaco("IDAT", zlib.deflateSync(cru)),
    pedaco("IEND", Buffer.alloc(0)),
  ]);
}
/** A largura e a altura da imagem que um `data:` de PNG carrega. */
function medidasDoUri(uri) {
  const bytes = Buffer.from(String(uri).slice(String(uri).indexOf(",") + 1), "base64");
  return `${bytes.readUInt32BE(16)}x${bytes.readUInt32BE(20)}`;
}

const falhas = [];
const confere = (ok, texto) => {
  if (!ok) falhas.push(texto);
};
const grande = png(3840, 2160);
const bomba = png(30000, 30000, false);
confere(bomba.length < 1024, `a imagem de pixels demais desta prova tem ${bomba.length} bytes: era para ser minúscula`);

// 1. A prévia de um anexo.
function previa(bytes) {
  const pedido = enc.encode(JSON.stringify({ attachment: 7, claimed: "image/png", chegada: "bytes" }));
  comBlocos([pedido, bytes], (pp, pn, bp, bn) => e.previa_julgar(pp, pn, bp, bn));
  return saida();
}
const reduzida = previa(grande);
confere(reduzida.refusal === null && medidasDoUri(reduzida.image) === "1280x720",
  `a prévia de uma imagem em 4K não chegou em 1280x720: ${JSON.stringify(reduzida).slice(0, 200)}`);
const recusada = previa(bomba);
confere(
  recusada.image === null && recusada.refusal?.kind === "TooManyPixels"
    && recusada.refusal.width === 30000 && recusada.refusal.height === 30000 && recusada.refusal.limit === 36000000,
  `a prévia que declara 30000x30000 não foi recusada por pixels, com as medidas: ${JSON.stringify(recusada).slice(0, 300)}`,
);

// 2. A mídia que um MOD recebe do servidor.
function midia(bytes, uso) {
  const codigo = comBlocos(
    [enc.encode(bytes.toString("base64")), enc.encode(uso)],
    (tp, tn, up, un) => e.mod_midia_do_servidor(tp, tn, up, un),
  );
  return { codigo, lida: saida() };
}
for (const [uso, esperado] of [["", "1920x1080"], ["fundo", "1280x720"], ["um-uso-que-nao-existe", "1920x1080"]]) {
  const { codigo, lida } = midia(grande, uso);
  confere(codigo === 0 && medidasDoUri(lida.uri) === esperado,
    `a mídia de MOD em 4K, com o uso «${uso}», não chegou em ${esperado}: ${JSON.stringify(lida).slice(0, 200)}`);
}
const daBomba = midia(bomba, "");
const motivo = String(daBomba.lida?.Recusado?.motivo);
confere(
  daBomba.codigo === 1 && motivo.startsWith("imagem-grande-demais-decodificada") && motivo.includes("30000 × 30000"),
  `a mídia de MOD que declara 30000x30000 não foi recusada pelo nome, com as medidas: ${JSON.stringify(daBomba.lida).slice(0, 300)}`,
);

if (falhas.length) {
  console.error(falhas.join("\n"));
  process.exit(1);
}
console.log("imagens no .wasm: a prévia e a mídia de MOD chegam reduzidas, e a que declara pixels demais é recusada com as medidas");
