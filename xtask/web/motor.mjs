// O motor de MODs do celular, DENTRO do `.wasm` do `wasm32-wasip1`, com um
// anfitrião que faz o papel do Worker: a WASI mínima e as chamadas da
// fronteira (`crates/seele-motor-web/src/fronteira.rs`).
//
//     node xtask/web/motor.mjs <seele_motor_web.wasm> <mod-comum.js> \
//       <crates/seele-motor/tests/casos.json> <crates/seele-motor/src/lib.rs>
//
// Três cobranças:
// - **as importações são as da lista**, e nenhuma outra;
// - **a tabela da volta passa aqui como passa no nativo**
//   (`crates/seele-motor/tests/casos.rs`): a mesma tabela, o mesmo `Motor`,
//   embrulhado como o celular o embrulha. É a prova de que o desktop e o
//   celular rodam a mesma volta. Ela inclui o teto de memória, que só vale com
//   o `RustAllocator` no `wasm32-wasip1` (`cutils.h:601-612`), e a sonda das
//   globais, que tem de dar a mesma lista nos dois alvos;
// - o MOD comum chega ao `pronto` com a API do desktop, respondido como a
//   janela responde.
import { readFileSync } from "node:fs";

const [caminhoDoWasm, caminhoDoMod, caminhoDaTabela, caminhoDoMotor] = process.argv.slice(2);
const modulo = await WebAssembly.compile(readFileSync(caminhoDoWasm));

// **Tudo o que o código de um MOD alcança fora do motor**, mesmo com um defeito
// de memória no QuickJS: um `.wasm` só chama o que importa. A lista é fixa, e
// uma importação nova reprova aqui até alguém a revisar.
// - Os dois `fd_prestat_*` são do `std` procurando diretórios pré-abertos: o
//   Worker responde que não há nenhum, e sem eles nenhum `path_*` teria onde
//   abrir (e nenhum é importado).
// - O `sched_yield` é do `mpsc` do `std`, por onde o motor fala desde 09/10:
//   no Worker ele não faz nada, porque não há outra thread a quem ceder.
// - Não há mais o módulo `seele`: a saída do motor é o canal, lido pela
//   fronteira.
const PERMITIDAS = [
  "wasi_snapshot_preview1.random_get", "wasi_snapshot_preview1.environ_get",
  "wasi_snapshot_preview1.environ_sizes_get", "wasi_snapshot_preview1.clock_time_get",
  "wasi_snapshot_preview1.fd_close", "wasi_snapshot_preview1.fd_fdstat_get",
  "wasi_snapshot_preview1.fd_prestat_get", "wasi_snapshot_preview1.fd_prestat_dir_name",
  "wasi_snapshot_preview1.fd_seek", "wasi_snapshot_preview1.fd_write",
  "wasi_snapshot_preview1.proc_exit", "wasi_snapshot_preview1.sched_yield",
];
const importadas = WebAssembly.Module.imports(modulo).map((i) => `${i.module}.${i.name}`);
const novas = importadas.filter((i) => !PERMITIDAS.includes(i));

const dormir = (ms) => new Promise((ok) => setTimeout(ok, Math.max(0, ms)));
const bytesDoTexto = (texto) => new TextEncoder().encode(texto).length;

/** Um Worker de mentira: a WASI mínima e as chamadas da fronteira. */
async function instancia() {
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
  const registro = [];
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
      for (let i = 0; i < n; i++) {
        const p = dv().getUint32(iovs + i * 8, true);
        const l = dv().getUint32(iovs + i * 8 + 4, true);
        registro.push(ler(p, l));
        total += l;
      }
      dv().setUint32(escritos, total, true);
      return 0;
    },
    proc_exit(c) { throw new Error(`proc_exit(${c}): ${registro.join("")}`); },
    sched_yield() { return 0; },
  };
  inst = await WebAssembly.instantiate(modulo, { wasi_snapshot_preview1: wasi });
  const falas = () => JSON.parse(saida());
  return {
    iniciar: (caps, fonte) => {
      comTexto(caps.join(","), (cp, cl) => comTexto(fonte, (fp, fl) => inst.exports.iniciar(cp, cl, fp, fl)));
      return falas();
    },
    entregar: (json) => { comTexto(json, (p, n) => inst.exports.entregar(p, n)); return falas(); },
    vencer: () => { inst.exports.vencer(); return falas(); },
    diagnostico: () => { inst.exports.diagnostico(); return falas(); },
    parar: () => { inst.exports.parar(); return falas(); },
    prazo: () => inst.exports.proximo_prazo_ms(),
    lido: (bytes) => inst.exports.lido(bytes),
    avisoLido: () => inst.exports.aviso_lido(),
  };
}

// ---------------------------------------------------------------- a tabela

/** Lê as falas, devolvendo o crédito como a janela devolve (e como `casos.rs`). */
function colher(motor, novas, semLer, falas) {
  for (const fala of novas) {
    if (!semLer) {
      if (fala.tipo === "mensagem") motor.lido(bytesDoTexto(fala.corpo));
      else if (fala.tipo === "falhou" || fala.tipo === "interrompido") motor.avisoLido();
    }
    falas.push(fala);
  }
}

/** Espera `ms` atendendo os prazos do motor, como o Worker com `setTimeout`. */
async function esperar(motor, semLer, falas, ms) {
  const fim = performance.now() + ms;
  for (;;) {
    const resta = fim - performance.now();
    const prazo = motor.prazo();
    if (prazo >= 0 && prazo <= resta) {
      await dormir(prazo);
      colher(motor, motor.vencer(), semLer, falas);
    } else {
      await dormir(resta);
      return;
    }
  }
}

const SONDA = (() => {
  const achado = readFileSync(caminhoDoMotor, "utf8").match(/pub const SONDA: &str = r#"([\s\S]*?)"#;/);
  if (!achado) throw new Error(`a sonda não está em ${caminhoDoMotor}`);
  return achado[1];
})();

async function rodar(tabela, caso) {
  const motor = await instancia();
  const semLer = caso.sem_ler === true;
  const falas = [];
  const fonte = caso.sonda === true ? SONDA : caso.fonte;
  colher(motor, motor.iniciar(caso.capacidades ?? tabela.capacidades_padrao, fonte), semLer, falas);
  for (const passo of caso.passos ?? []) {
    if (typeof passo.responder === "string") colher(motor, motor.entregar(passo.responder), semLer, falas);
    else if (typeof passo.esperar_ms === "number") await esperar(motor, semLer, falas, passo.esperar_ms);
    else if (passo.diagnostico === true) colher(motor, motor.diagnostico(), semLer, falas);
    else throw new Error(`passo desconhecido: ${JSON.stringify(passo)}`);
  }
  colher(motor, motor.parar(), semLer, falas);
  // A diferença declarada da sonda: no `.wasm`, nenhuma delas pode existir
  // (o nativo cobra que existam, em `casos.rs`).
  for (const soDoNativo of caso.globais_so_do_nativo ?? []) {
    if (String(falas[0]?.corpo).includes(`,${soDoNativo},`)) {
      throw new Error(`o .wasm passou a ter ${soDoNativo}: a diferença declarada da sonda mudou`);
    }
  }
  return falas;
}

/** O que difere entre o esperado e o que veio, como `diferenca` em `casos.rs`. */
function diferenca(esperado, veio) {
  const lista = esperado.flatMap((item) => Array(item.vezes ?? 1).fill(item));
  if (lista.length !== veio.length) return `${lista.length} falas esperadas, vieram ${veio.length}`;
  for (let n = 0; n < lista.length; n++) {
    const e = lista[n];
    const v = veio[n];
    const ok = e.tipo === v.tipo
      && (e.corpo === undefined || e.corpo === v.corpo)
      && (e.corpo_comeca === undefined || String(v.corpo ?? "").startsWith(e.corpo_comeca))
      && (e.texto === undefined || e.texto === v.texto)
      && (e.texto_contem === undefined || String(v.texto ?? "").includes(e.texto_contem))
      && (e.nivel === undefined || e.nivel === v.nivel)
      && (e.suprimidas === undefined || e.suprimidas === v.suprimidas)
      && (e.relogios === undefined || e.relogios === v.relogios);
    if (!ok) return `a fala ${n}: esperava ${JSON.stringify(e)}, veio ${JSON.stringify(v)}`;
  }
  return null;
}

// ---------------------------------------------------------------- o MOD comum

/** O MOD comum, respondido como a janela responde, até o `pronto`. */
async function oModComum(tabela) {
  const motor = await instancia();
  const falas = [];
  const respostas = [];
  const atender = (novas) => {
    colher(motor, novas, false, falas);
    for (const fala of novas) {
      if (fala.tipo !== "mensagem") continue;
      const m = JSON.parse(fala.corpo);
      if (typeof m.n !== "number") continue;
      respostas.push(JSON.stringify(
        m.tipo === "snapshot" ? { tipo: "resposta", n: m.n, ok: true, valor: { servidor: "bancada" } }
          : m.tipo === "pedido" && m.id === "eco" ? { tipo: "resposta", n: m.n, ok: true, valor: m.valor }
          : m.tipo === "regiao" ? { tipo: "resposta", n: m.n, ok: true, valor: null }
          : { tipo: "resposta", n: m.n, ok: false, erro: "desconhecido" },
      ));
    }
  };
  atender(motor.iniciar(tabela.capacidades_padrao, readFileSync(caminhoDoMod, "utf8")));
  const fim = performance.now() + 5000;
  while (performance.now() < fim) {
    while (respostas.length) atender(motor.entregar(respostas.shift()));
    const pronto = falas.find((f) => f.tipo === "mensagem" && JSON.parse(f.corpo).canal === "pronto");
    if (pronto) return { valor: JSON.parse(pronto.corpo).valor, falas };
    const prazo = motor.prazo();
    await dormir(prazo >= 0 ? Math.min(prazo, 50) : 10);
    if (prazo >= 0) atender(motor.vencer());
  }
  return { valor: null, falas };
}

// ---------------------------------------------------------------- as cobranças

const falhas = [];
if (novas.length) falhas.push(`o .wasm importa o que a lista não permite: ${novas.join(", ")}`);

const tabela = JSON.parse(readFileSync(caminhoDaTabela, "utf8"));
if (!(tabela.casos?.length >= 13)) falhas.push(`a tabela encolheu para ${tabela.casos?.length} casos`);
for (const caso of tabela.casos ?? []) {
  const veio = await rodar(tabela, caso);
  const d = diferenca(caso.esperado, veio);
  if (d) falhas.push(`${caso.de}: ${d}\n    veio: ${JSON.stringify(veio).slice(0, 1500)}`);
}

const comum = await oModComum(tabela);
const v = comum.valor ?? {};
if (!comum.valor) falhas.push(`MOD comum: não chegou ao pronto (${JSON.stringify(comum.falas).slice(0, 400)})`);
else if (v.snap?.servidor !== "bancada" || JSON.stringify(v.capacidades) !== JSON.stringify(tabela.capacidades_padrao)
  || v.intervalos !== 3 || v.erro !== "rejeitou: desconhecido" || !String(v.grande).startsWith("mensagem-grande: pedido tem 14062 bytes")
  || v.superficies !== "object" || v.console !== "function"
  || !comum.falas.some((f) => f.tipo === "console" && f.texto === "mod comum de pé")) {
  falhas.push(`MOD comum: a API diverge do desktop: ${JSON.stringify(v)}`);
}

if (falhas.length) {
  console.error(`motor do celular no .wasm: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log(`motor do celular no .wasm: ${importadas.length} importações, todas da lista; os ${tabela.casos.length} casos da volta passam como no nativo, e o MOD comum fala a API do desktop.`);
