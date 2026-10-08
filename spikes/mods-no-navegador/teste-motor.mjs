// O motor fora do navegador: o MOD comum sobe, fala e responde? E o laço sem
// fim é cortado no prazo, e a memória no teto? Uso: node teste-motor.mjs
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { criarMotor } = require("./pagina/motor-anfitriao.js");
const modulo = await WebAssembly.compile(readFileSync(new URL("./motor/target/wasm32-wasip1/release/motor_de_mods_wasm.wasm", import.meta.url)));
const CAPS = ["regiao", "tema", "cartoes", "arquivo", "superficies", "contribuicoes", "estilos", "classes", "volume"];

function rodar(fonte, eventos = []) {
  return new Promise(async (fim) => {
    const visto = { falas: [], console: [], erros: [] };
    let motor;
    const responder = (texto) => {
      const m = JSON.parse(texto);
      visto.falas.push(m);
      if (m.n == null) return;
      let resposta;
      if (m.tipo === "snapshot") resposta = { n: m.n, ok: true, valor: { servidor: "bancada" } };
      else if (m.tipo === "pedido" && m.id === "eco") resposta = { n: m.n, ok: true, valor: m.valor };
      else if (m.tipo === "regiao") resposta = { n: m.n, ok: true, valor: null };
      else resposta = { n: m.n, ok: false, erro: "desconhecido" };
      setTimeout(() => motor.entregar(JSON.stringify(resposta)), 0);
    };
    motor = criarMotor(modulo, {
      aoPostar: (t) => { responder(t); return true; },
      aoConsole: (m, t) => visto.console.push(`${m}: ${t}`),
      aoErroNaVolta: (e) => visto.erros.push(e),
      agendar: (f, ms, repete) => (repete ? setInterval(f, ms) : setTimeout(f, ms)),
      cancelar: (h) => { clearTimeout(h); clearInterval(h); },
    });
    const t0 = performance.now();
    const r = await motor.subir(CAPS, fonte);
    visto.subiu_ms = Math.round(performance.now() - t0);
    if (r.erro) visto.erros.push(r.erro);
    for (const e of eventos) { await new Promise((ok) => setTimeout(ok, 200)); motor.entregar(JSON.stringify({ tipo: "evento", ...e })); }
    setTimeout(() => { visto.memoria = motor.memoria(); motor.encerrar(); fim(visto); }, 1500);
  });
}

const comum = await rodar(readFileSync(new URL("./pagina/mod-comum.js", import.meta.url), "utf8"), [{ nome: "ping", valor: 1 }, { nome: "cpu" }, { nome: "json" }]);
const pronto = comum.falas.find((m) => m.canal === "pronto");
console.log("comum · subiu em", comum.subiu_ms, "ms · pronto:", JSON.stringify(pronto && pronto.valor));
console.log("comum · cpu:", JSON.stringify(comum.falas.find((m) => m.canal === "cpu")?.valor), "json:", JSON.stringify(comum.falas.find((m) => m.canal === "json")?.valor));
console.log("comum · console:", comum.console, "erros:", comum.erros, "memória:", comum.memoria);

const laco = await rodar("SeeleUI.aoEvento(() => { const t = Date.now(); while (Date.now() - t < 3000) {} SeeleMods.request('eco', 'terminou', 1); });", [{ nome: "x" }]);
console.log("laço · erros:", laco.erros.map((e) => e.slice(0, 60)), "terminou:", laco.falas.some((m) => m.canal === "terminou"));

const bomba = await rodar("SeeleUI.aoEvento(() => { const a = []; let n = 0; try { for (; n < 64; n++) a.push(new Array(262144).fill(n)); SeeleMods.request('eco', 'mem', 'alocou ' + n); } catch (e) { a.length = 0; SeeleMods.request('eco', 'mem', 'parou em ' + n + ': ' + e); } });", [{ nome: "x" }]);
console.log("memória · ", JSON.stringify(bomba.falas.find((m) => m.canal === "mem")?.valor), "erros:", bomba.erros.map((e) => e.slice(0, 60)));
