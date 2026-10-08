// O motor de MODs do celular, DENTRO do `.wasm` do `wasm32-wasip1`, com um
// anfitrião que faz o papel do Worker: a WASI mínima e as três funções do
// módulo `seele`.
//
//     node xtask/web/motor.mjs <seele_motor_web.wasm> <crates/seele-motor-web/tests/mod-comum.js>
//
// Três cobranças:
// - o MOD comum chega ao `pronto` com a API do desktop;
// - um laço sem fim é cortado (`interrupted`);
// - **um MOD que aloca sem parar é parado pelo teto de 8 MiB** antes do quarto
//   bloco de 2 MiB. É o guarda do `RustAllocator`: no `wasm32-wasip1`, com o
//   alocador padrão do quickjs-ng, o teto não existe (`cutils.h:601-612`), e
//   este caso aloca os 64 blocos. Só se prova aqui: no nativo o alocador padrão
//   já responde o tamanho dos blocos.
import { readFileSync } from "node:fs";

const [caminhoDoWasm, caminhoDoMod] = process.argv.slice(2);
const modulo = await WebAssembly.compile(readFileSync(caminhoDoWasm));

// **Tudo o que o código de um MOD alcança fora do motor**, mesmo com um defeito
// de memória no QuickJS: um `.wasm` só chama o que importa. A lista é fixa, e
// uma importação nova reprova aqui até alguém a revisar. Os dois `fd_prestat_*`
// são do `std` procurando diretórios pré-abertos: o Worker responde que não há
// nenhum, e sem eles nenhum `path_*` teria onde abrir (e nenhum é importado).
const PERMITIDAS = [
  "seele.postar", "seele.console", "seele.agora_ms",
  "wasi_snapshot_preview1.random_get", "wasi_snapshot_preview1.environ_get",
  "wasi_snapshot_preview1.environ_sizes_get", "wasi_snapshot_preview1.clock_time_get",
  "wasi_snapshot_preview1.fd_close", "wasi_snapshot_preview1.fd_fdstat_get",
  "wasi_snapshot_preview1.fd_prestat_get", "wasi_snapshot_preview1.fd_prestat_dir_name",
  "wasi_snapshot_preview1.fd_seek", "wasi_snapshot_preview1.fd_write",
  "wasi_snapshot_preview1.proc_exit",
];
const importadas = WebAssembly.Module.imports(modulo).map((i) => `${i.module}.${i.name}`);
const novas = importadas.filter((i) => !PERMITIDAS.includes(i));
const CAPS = ["regiao", "tema", "cartoes", "arquivo", "superficies", "contribuicoes", "estilos", "classes", "volume"];

// Um Worker de mentira: instancia, responde como a página, e dispara relógios.
async function rodar(fonte, eventos) {
  const falas = [];
  const linhas = [];
  const erros = [];
  let inst = null;
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  const mem = () => new Uint8Array(inst.exports.memory.buffer);
  const dv = () => new DataView(inst.exports.memory.buffer);
  const ler = (p, n) => dec.decode(mem().slice(p, p + n));
  const saida = () => ler(inst.exports.saida_ptr(), inst.exports.saida_len());
  const comTexto = (texto, f) => {
    const b = enc.encode(texto);
    const p = inst.exports.alocar(b.length);
    mem().set(b, p);
    try { return f(p, b.length); } finally { inst.exports.liberar(p, b.length); }
  };
  // A WASI que o QuickJS pede: relógio, aleatório, ambiente vazio, registro.
  const wasi = {
    random_get(p, n) { crypto.getRandomValues(mem().subarray(p, p + n)); return 0; },
    environ_sizes_get(pc, ps) { dv().setUint32(pc, 0, true); dv().setUint32(ps, 0, true); return 0; },
    environ_get() { return 0; },
    clock_time_get(id, _precisao, p) {
      dv().setBigUint64(p, id === 0 ? BigInt(Date.now()) * 1000000n : BigInt(Math.round(performance.now() * 1e6)), true);
      return 0;
    },
    fd_close() { return 8; },
    fd_fdstat_get() { return 8; },
    // Nenhum diretório pré-aberto: EBADF encerra a busca do `std`.
    fd_prestat_get() { return 8; },
    fd_prestat_dir_name() { return 8; },
    fd_seek() { return 8; },
    fd_write(_fd, iovs, n, escritos) {
      let total = 0;
      for (let i = 0; i < n; i++) total += dv().getUint32(iovs + i * 8 + 4, true);
      dv().setUint32(escritos, total, true);
      return 0;
    },
    proc_exit(c) { throw new Error(`proc_exit(${c})`); },
  };
  const pendentes = [];
  const seele = {
    postar(p, n) { const m = JSON.parse(ler(p, n)); falas.push(m); if (m.n != null) pendentes.push(m); return 1; },
    console(mp, ml, tp, tl) { linhas.push(`${ler(mp, ml)}: ${ler(tp, tl)}`); },
    agora_ms() { return performance.now(); },
  };
  inst = await WebAssembly.instantiate(modulo, { seele, wasi_snapshot_preview1: wasi });
  if (comTexto(CAPS.join(","), (cp, cl) => comTexto(fonte, (fp, fl) => inst.exports.iniciar(cp, cl, fp, fl))) !== 0) {
    return { erros: [`não subiu: ${saida()}`], falas, linhas };
  }
  const volta = (codigo) => { if (codigo !== 0) erros.push(saida()); };
  const intervalos = new Set();
  const aDisparar = [];
  for (let passo = 0; passo < 300; passo++) {
    while (pendentes.length) {
      const m = pendentes.shift();
      const r = m.tipo === "snapshot" ? { n: m.n, ok: true, valor: { servidor: "bancada" } }
        : m.tipo === "pedido" && m.id === "eco" ? { n: m.n, ok: true, valor: m.valor }
        : m.tipo === "regiao" ? { n: m.n, ok: true, valor: null }
        : { n: m.n, ok: false, erro: "desconhecido" };
      volta(comTexto(JSON.stringify(r), (p, n) => inst.exports.entregar(p, n)));
    }
    inst.exports.pedidos_de_relogio();
    for (const r of JSON.parse(saida())) {
      if (r.cancelar) intervalos.delete(r.id);
      else if (r.repete) intervalos.add(r.id);
      else aDisparar.push(r.id);
    }
    const agora = [...aDisparar.splice(0), ...intervalos];
    for (const id of agora) volta(inst.exports.relogio(id));
    if (passo === 5) for (const ev of eventos) volta(comTexto(JSON.stringify({ tipo: "evento", ...ev }), (p, n) => inst.exports.entregar(p, n)));
    if (!pendentes.length && !agora.length && passo > 5) break;
  }
  return { falas, linhas, erros };
}

const falhas = [];
if (novas.length) falhas.push(`o .wasm importa o que a lista não permite: ${novas.join(", ")}`);

const comum = await rodar(readFileSync(caminhoDoMod, "utf8"), []);
const pronto = comum.falas.find((m) => m.canal === "pronto");
const v = pronto ? pronto.valor : {};
if (!pronto) falhas.push(`MOD comum: não chegou ao pronto (${JSON.stringify(comum.falas).slice(0, 300)}; erros ${comum.erros})`);
else if (v.snap?.servidor !== "bancada" || JSON.stringify(v.capacidades) !== JSON.stringify(CAPS) || v.intervalos !== 3
  || v.erro !== "rejeitou: desconhecido" || !String(v.grande).startsWith("mensagem-grande: pedido tem 14062 bytes")
  || v.superficies !== "object" || v.console !== "function" || !comum.linhas.includes("log: mod comum de pé")) {
  falhas.push(`MOD comum: a API diverge do desktop: ${JSON.stringify(v)}`);
}

const laco = await rodar("SeeleUI.aoEvento(() => { while (true) {} });", [{ nome: "x" }]);
if (!laco.erros.some((e) => e.includes("interrupted"))) falhas.push(`laço sem fim: não foi cortado (${laco.erros})`);

const bomba = await rodar(
  "SeeleUI.aoEvento(() => { const a = []; let n = 0; try { for (; n < 64; n++) a.push(new Array(262144).fill(n)); a.length = 0; SeeleMods.request('eco', 'mem', n); } catch (e) { a.length = 0; SeeleMods.request('eco', 'mem', n); } });",
  [{ nome: "x" }],
);
const blocos = bomba.falas.find((m) => m.canal === "mem")?.valor;
if (!(blocos < 4)) falhas.push(`memória: o MOD alocou ${blocos} blocos de 2 MiB com o teto de 8 MiB; o RustAllocator saiu?`);

if (falhas.length) {
  console.error(`motor do celular no .wasm: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log(`motor do celular no .wasm: ${importadas.length} importações, todas da lista; o MOD comum fala a API do desktop, o laço sem fim é cortado, e a memória para no teto (${blocos} blocos de 2 MiB).`);
