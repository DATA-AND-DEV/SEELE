// O tema que um MOD pede sai com a sessão — e com o MOD.
//
// **O defeito.** O tema de um MOD (`case "tema"` → `aplicarOTemaDoMod`) é escrito
// como variáveis em `#tela-sessao`, o mesmo elemento em toda sessão. A única
// limpeza dele morava em `limparARegiaoDoMod`, registrada só quando o MOD cria
// uma região — e o ESTILO 3.1.0, de API 4, não cria. O tema sobrevivia à saída,
// e o servidor seguinte, sem MOD nenhum, aparecia com as cores do anterior.
// Relato de 05/10/2026: um Mac entrou no servidor de um iPhone com a
// personalização de outro servidor. A personalização é da sessão
// (`specs/07-estetica.md`), e isto não pode acontecer em hipótese nenhuma.
//
// **Os três casos**, contra o código real de `ui/base.js` e `ui/mods-runtime.js`
// carregado num `vm`:
//
// 1. Sair do servidor apaga o tema, com e sem região — o caso do relato.
// 2. Um MOD que se encerra no meio da sessão leva o tema dele junto, sem
//    esperar a sessão acabar (`darDonoAoTema`).
// 3. Uma instância velha que termina de encerrar **depois** de a nova pedir tema
//    não apaga o tema da nova.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual. Rodada por
// `cargo xtask check-runtime`.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repo = path.resolve(__dirname, "../../..");
const base = fs.readFileSync(`${repo}/apps/seele-app/ui/base.js`, "utf8");
const runtime = fs.readFileSync(`${repo}/apps/seele-app/ui/mods-runtime.js`, "utf8");

/** Do início do trecho até o primeiro "\n}\n" (ou "});\n") depois dele. */
function trecho(fonte, comeco, fim = "\n}\n") {
  const i = fonte.indexOf(comeco);
  if (i < 0) throw new Error(`não achei ${comeco}`);
  const j = fonte.indexOf(fim, i);
  return fonte.slice(i, j + fim.length);
}
/** De `comeco` até o fim da função `ultima`. */
function bloco(fonte, comeco, ultima) {
  const i = fonte.indexOf(comeco);
  const k = fonte.indexOf(ultima, i);
  const j = fonte.indexOf("\n}\n", k);
  return fonte.slice(i, j + 3);
}

const codigo = [
  trecho(runtime, "const ESTADOS_DE_MOD", "\n});\n"),
  trecho(runtime, "class InstanciaDeMod"),
  "let geracaoDaSessao = 0;",
  trecho(base, "function entrarNaGeracao("),
  trecho(base, "function daGeracaoDePe("),
  bloco(base, "const temaDosMods = new Map();", "function contrasteEntre("),
  trecho(base, "function limparARegiaoDoMod("),
  trecho(base, "function encerrarOAmbienteDosMods("),
  // O que `case "tema"` faz, base.js: `aplicarOTemaDoMod(mod.id, m.valores)`.
  // O que `regiaoDoMod` registra na criação, base.js: a linha do registrar.
  `globalThis.__api = { aplicarOTemaDoMod, darDonoAoTema, encerrarOAmbienteDosMods, entrarNaGeracao,
     limparARegiaoDoMod, InstanciaDeMod, ESTADOS_DE_MOD, modsCarregados };`,
].join("\n");

const estilo = new Map();
const sessao = {
  style: {
    setProperty: (k, v) => estilo.set(k, v),
    removeProperty: (k) => estilo.delete(k),
  },
};
const raizCss = { "--seele-osso": "#e8e4dc", "--seele-negro-absoluto": "#000000" };
const ctx = {
  console,
  setTimeout, clearTimeout, Promise, Map, Set, Error, Object, Number, String, JSON, Array, Math,
  CustomEvent: class { constructor(t) { this.type = t; } },
  dispatchEvent: () => true,
  $: (id) => (id === "tela-sessao" ? sessao : id === "regioes-dos-mods" ? { childElementCount: 0 } : null),
  document: { documentElement: {} },
  getComputedStyle: () => ({ getPropertyValue: (t) => raizCss[t] ?? "" }),
  contribuicoesDosMods: { escolherSubstituicao: () => ({}), revogarDoMod: () => {} },
  modsCarregados: new Map(),
  modsExigidos: new Map(),
  pedidosDeMod: new Map(),
  conjuntoJaBuscado: "",
  regioesDosMods: new Map(),
  superficiesDosMods: new Map(),
  cartoesDosMods: new Map(),
  encerrarOSomDosMods: () => {},
};
ctx.globalThis = ctx;
vm.createContext(ctx);
// `let`/`const` de topo não viram propriedade do contexto: declare os que o
// código atribui como variáveis do próprio script.
vm.runInContext(
  "let modsCarregados = globalThis.modsCarregados; let modsExigidos = globalThis.modsExigidos;"
  + "let pedidosDeMod = globalThis.pedidosDeMod; let conjuntoJaBuscado = '';"
  + "const regioesDosMods = globalThis.regioesDosMods; const superficiesDosMods = globalThis.superficiesDosMods;"
  + "const cartoesDosMods = globalThis.cartoesDosMods;\n" + codigo,
  ctx,
);
const api = ctx.__api;

const executorQueConfirma = () => ({
  pedirEncerramento() {},
  async encerrou() { return true; },
});
const TEMA = { fundo: "#0b1d2a", texto: "#f5f1e6", acento: "#19c37d" };
const OUTRO = { acento: "#2f8fe0" };

function instanciaAtiva(geracao) {
  const inst = new api.InstanciaDeMod("seele/estilo", "h", geracao, executorQueConfirma());
  inst.estado = api.ESTADOS_DE_MOD.ativa;
  api.modsCarregados.set("seele/estilo", inst);
  return inst;
}

/** O que `case "tema"` faz, em base.js. */
function pedirTema(inst, valores) {
  api.aplicarOTemaDoMod("seele/estilo", valores);
  api.darDonoAoTema("seele/estilo", inst);
}

const falhas = [];
function cobrar(ok, caso, visto) {
  console.log(`${ok ? "ok    " : "FALHOU"} ${caso}${ok ? "" : ` — visto: ${JSON.stringify(visto)}`}`);
  if (!ok) falhas.push(caso);
}

(async () => {
  // 1. Sair do servidor, com e sem região.
  for (const desenhaRegiao of [true, false]) {
    estilo.clear();
    api.entrarNaGeracao(7);
    const inst = instanciaAtiva(7);
    if (desenhaRegiao) {
      inst.registrar("seele/estilo: a região", () => api.limparARegiaoDoMod("seele/estilo", inst));
    }
    pedirTema(inst, TEMA);
    const durante = estilo.size;
    api.encerrarOAmbienteDosMods();
    const logoDepois = Object.fromEntries(estilo);
    await inst.encerramento;
    api.entrarNaGeracao(8);
    cobrar(durante > 0 && Object.keys(logoDepois).length === 0 && estilo.size === 0,
      `sair do servidor apaga o tema na hora (${desenhaRegiao ? "com" : "sem"} região)`, logoDepois);
  }

  // 2. O MOD se encerra no meio da sessão: o tema vai com ele.
  estilo.clear();
  api.entrarNaGeracao(9);
  const sozinho = instanciaAtiva(9);
  pedirTema(sozinho, TEMA);
  await sozinho.encerrar();
  cobrar(estilo.size === 0, "um MOD que sai no meio da sessão leva o tema dele", Object.fromEntries(estilo));

  // 3. A instância velha termina tarde e não apaga o tema da nova.
  estilo.clear();
  api.entrarNaGeracao(10);
  const velha = instanciaAtiva(10);
  pedirTema(velha, TEMA);
  let soltar;
  velha.executor = { pedirEncerramento() {}, encerrou: () => new Promise((r) => { soltar = r; }) };
  const encerrandoVelha = velha.encerrar();
  const nova = instanciaAtiva(10);
  pedirTema(nova, OUTRO);
  soltar?.(true);
  await encerrandoVelha.catch(() => {});
  cobrar(estilo.get("--seele-laranja-nerv") === "#2f8fe0",
    "a instância velha que termina tarde não apaga o tema da nova", Object.fromEntries(estilo));

  if (falhas.length) process.exit(1);
  console.log("\no tema sai com a sessão e com o MOD.");
})();
