// As chaves do tema, na casca, contra o vetor que o servidor também lê
// (rascunho da API 6, §9; tarefa 17j).
//
// # Por que esta bancada existe
//
// O `mundo.tema` aceitava qualquer chave, e o MOD lia `ok` para um tema que a
// casca ia recusar. A 17j fecha as chaves no servidor pela mesma tabela — e
// «a mesma tabela» só é verdade se as duas respostas forem as mesmas, caso a
// caso. Um espelho que compara as duas listas pelo texto não veria o que esta
// bancada achou na primeira rodada: a casca conferia com `in`, que enxerga o
// protótipo. `fonte: "toString"` era aplicado, e `constructor` era recusado
// pelo caminho errado — como uma medida que «só aceita» uma lista vazia.
//
// # O que ela mede
//
// Cada caso de `crates/seele-proto/tests/vetores/chaves-do-tema.json` passa
// pelo `aplicarOTemaDoMod` real, carregado num `vm`: o que o vetor aceita a
// casca aplica, e o que ele recusa a casca recusa, dizendo a mesma chave e o
// mesmo motivo: `chave-desconhecida` é a frase «não conhece», e
// `valor-invalido` é qualquer outra. O
// contraste fica de fora — a raiz não tem cor medida aqui, e a medida é da 17e
// (`tons-do-tema.cjs`).
//
// Rodar:  node apps/seele-app/bancada/chaves-do-tema.cjs
// Rodada por `cargo xtask check-runtime`.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repo = path.resolve(__dirname, "../../..");
const base = fs.readFileSync(`${repo}/apps/seele-app/ui/base.js`, "utf8");
const vetor = JSON.parse(
  fs.readFileSync(`${repo}/crates/seele-proto/tests/vetores/chaves-do-tema.json`, "utf8"),
);

/** De `comeco` até o fim da função `ultima`. */
function bloco(fonte, comeco, ultima) {
  const i = fonte.indexOf(comeco);
  if (i < 0) throw new Error(`não achei ${comeco}`);
  const k = fonte.indexOf(ultima, i);
  if (k < 0) throw new Error(`não achei ${ultima}`);
  const j = fonte.indexOf("\n}\n", k);
  return fonte.slice(i, j + 3);
}

const ctx = {
  Map, Set, Error, Object, Number, String, JSON, Array, Math,
  $: () => ({ style: { setProperty() {}, removeProperty() {} } }),
  document: { documentElement: {} },
  // Sem cor medida na raiz, a medida de contraste não recusa nada.
  getComputedStyle: () => ({ getPropertyValue: () => "" }),
};
ctx.globalThis = ctx;
vm.createContext(ctx);
vm.runInContext(
  bloco(base, "const temaDosMods = new Map();", "function contrasteEntre(")
    + "\nglobalThis.__api = { aplicarOTemaDoMod, temaDosMods };",
  ctx,
);
const { aplicarOTemaDoMod, temaDosMods } = ctx.__api;

const falhas = [];
for (const caso of vetor.casos) {
  temaDosMods.clear();
  let erro = null;
  let lancado = null;
  try {
    aplicarOTemaDoMod("seele/estilo", caso.tema);
  } catch (e) {
    erro = e.message;
    lancado = e;
  }
  if (caso.recusa === null) {
    if (erro !== null) falhas.push(`${caso.nome}: o vetor aceita, e a casca recusou — ${erro}`);
  } else if (erro === null) {
    falhas.push(`${caso.nome}: o vetor recusa por «${caso.recusa.chave}», e a casca aplicou`);
  } else if (!erro.includes(`«${caso.recusa.chave}»`)) {
    falhas.push(`${caso.nome}: a casca recusou sem dizer «${caso.recusa.chave}» — ${erro}`);
  } else if ((caso.recusa.erro === "chave-desconhecida") !== erro.includes("não conhece")) {
    falhas.push(`${caso.nome}: o vetor recusa com ${caso.recusa.erro}, e a casca disse outra coisa — ${erro}`);
  } else if (caso.recusa.regiao !== undefined && !erro.includes(`«${caso.recusa.regiao}»`)) {
    falhas.push(`${caso.nome}: a casca recusou sem dizer a região «${caso.recusa.regiao}» — ${erro}`);
  } else {
    // **O nome e os campos que o MOD da 6 recebe, em todo caso**: os mesmos
    // do vetor, e não só a frase. Valia só para as regiões (17i, item 3), e a
    // prévia devolvia `falhou` para a chave fora da tabela, o valor fora da
    // forma e o `modo` que discorda (a auditoria do `v6.json`, B5).
    const campos = { erro: lancado?.recusa, ...lancado?.camposDaRecusa };
    const esperado = { ...caso.recusa };
    if (JSON.stringify(campos) !== JSON.stringify(esperado)) {
      falhas.push(`${caso.nome}: a casca recusa com ${JSON.stringify(campos)}, e o vetor com ${JSON.stringify(esperado)}`);
    }
  }
}

if (falhas.length) {
  console.error(`chaves-do-tema: ${falhas.length} falha(s)`);
  for (const f of falhas) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`chaves-do-tema: ok (${vetor.casos.length} casos)`);
