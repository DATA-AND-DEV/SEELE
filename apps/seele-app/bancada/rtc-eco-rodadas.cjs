// N rodadas do R4 (`rtc-eco.cjs`) por configuração, e a DISTRIBUIÇÃO da voz.
//
//   node apps/seele-app/bancada/rtc-eco-rodadas.cjs <binário do eco> <playwright chromium> <playwright webkit> <pasta de saída> [--rodadas N]
//
// Para cada navegador (Chromium e WebKit) e cada número de canais de volume (1 e 4), roda N vezes o
// `rtc-eco.cjs` contra o eco do enlace de produto, com o eco em `[::]:0`. Guarda o JSON de cada rodada,
// e escreve `resumo.json` com, por configuração e por sentido (subida e descida com carga):
// - cada rodada: as recebidas, as perdidas, p50, p95 e p99 da voz;
// - a distribuição sobre as rodadas: o mínimo, a mediana e o máximo de cada número, e o total de
//   perdidas sobre o total mandado.
// Nenhuma rodada é escolhida: o que vale é a distribuição inteira.
//
// **Fora do CI, de propósito** (`SEM_LUGAR` em `xtask/src/check_runtime.rs`): é um
// coletor, não um gate. Uma rodada que falha entra na distribuição em vez de
// reprovar. O gate é a `rtc-eco.cjs`, obrigatória no job `bancadas-de-mod`. Roda só
// à mão, com a linha de cima, e o resultado não conta como validação executada.

"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const argumentos = process.argv.slice(2);
const opcao = (nome, padrao) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : padrao);
const [eco, pwChromium, pwWebkit, saida] = argumentos.filter((a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1].startsWith("--")));
const rodadas = Number(opcao("--rodadas", "10"));
if (!eco || !pwChromium || !pwWebkit || !saida) {
  console.error("uso: node rtc-eco-rodadas.cjs <eco> <playwright chromium> <playwright webkit> <pasta> [--rodadas N]");
  process.exit(2);
}
fs.mkdirSync(saida, { recursive: true });
const bancada = path.join(__dirname, "rtc-eco.cjs");

const mediana = (v) => {
  const o = [...v].sort((a, b) => a - b);
  return o.length ? o[Math.floor(o.length / 2)] : null;
};
const faixa = (v) => {
  const n = v.filter((x) => typeof x === "number");
  return n.length ? { min: Math.min(...n), mediana: mediana(n), max: Math.max(...n) } : null;
};

const resumo = { rodadas, configuracoes: {} };
for (const [navegador, playwright] of [
  ["chromium", pwChromium],
  ["webkit", pwWebkit],
]) {
  for (const canais of [1, 4]) {
    const chave = `${navegador}-${canais}canais`;
    const porRodada = [];
    for (let i = 1; i <= rodadas; i += 1) {
      const arquivo = path.join(saida, `${chave}-${String(i).padStart(2, "0")}.json`);
      try {
        execFileSync("node", [bancada, eco, navegador, playwright, "--canais", String(canais), "--escuta", "[::]:0", "--json", arquivo], {
          stdio: ["ignore", "ignore", "ignore"],
          timeout: 240_000,
        });
      } catch {
        // A bancada sai com 1 quando um dos casos de antes da rajada falha; a rajada ainda vale.
      }
      let lido = null;
      try {
        lido = JSON.parse(fs.readFileSync(arquivo, "utf8"));
      } catch {
        lido = null;
      }
      const r = lido?.rajada;
      const fase = (nome) =>
        r?.voz?.[nome]
          ? { recebidas: r.voz[nome].recebidas, perdidas: r.voz[nome].perdidas, p50: r.voz[nome].p50, p95: r.voz[nome].p95, p99: r.voz[nome].p99 }
          : null;
      porRodada.push({
        rodada: i,
        abriu: Boolean(r?.abriu),
        subida_mbit_s: r?.subida_mbit_s ?? null,
        voltaram: r?.voltaram ?? null,
        descida_ms: r?.descida_ms ?? null,
        subida: fase("subida"),
        descida: fase("descida"),
      });
      console.log(`${chave} ${i}/${rodadas}: ${JSON.stringify(porRodada.at(-1))}`);
    }
    const distribuicao = {};
    for (const sentido of ["subida", "descida"]) {
      const v = porRodada.map((x) => x[sentido]).filter(Boolean);
      const mandadas = v.reduce((t, x) => t + x.recebidas + x.perdidas, 0);
      const perdidas = v.reduce((t, x) => t + x.perdidas, 0);
      distribuicao[sentido] = {
        rodadasComDados: v.length,
        perdidasPorRodada: v.map((x) => x.perdidas),
        perdaTotal: { perdidas, mandadas, fracao: mandadas ? Math.round((perdidas / mandadas) * 10000) / 10000 : null },
        p50: faixa(v.map((x) => x.p50)),
        p95: faixa(v.map((x) => x.p95)),
        p99: faixa(v.map((x) => x.p99)),
      };
    }
    resumo.configuracoes[chave] = { porRodada, distribuicao, voltaramInteiras: porRodada.filter((x) => x.voltaram === 32 * 1024 * 1024).length };
    fs.writeFileSync(path.join(saida, "resumo.json"), JSON.stringify(resumo, null, 2) + "\n");
  }
}
console.log(JSON.stringify(Object.fromEntries(Object.entries(resumo.configuracoes).map(([k, v]) => [k, v.distribuicao])), null, 2));
