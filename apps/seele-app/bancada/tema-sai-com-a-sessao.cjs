// O tema sai com a sessão.
//
// **O defeito.** O tema é escrito como variáveis em `#tela-sessao`, o mesmo
// elemento em toda sessão. A única limpeza dele morava em `limparARegiaoDoMod`,
// registrada só quando o MOD criava uma região — e o ESTILO 3.1.0, de API 4,
// não criava. O tema sobrevivia à saída, e o servidor seguinte, sem MOD nenhum,
// aparecia com as cores do anterior. Relato de 05/10/2026: um Mac entrou no
// servidor de um iPhone com a personalização de outro servidor. A
// personalização é da sessão (`specs/07-estetica.md`), e isto não pode
// acontecer em hipótese nenhuma.
//
// **Na 6**, o tema que pinta a sessão é o do servidor, que entra por
// `aplicarOTemaDoMod` com o id do MOD que o declarou (`aplicarOTemaDoServidor`),
// e a janela só faz prévia. O tema que a janela aplicava para um MOD das APIs 3
// a 5, com a limpeza por instância (`darDonoAoTema`), saiu com o corte; os
// casos dele saíram junto.
//
// **Os dois casos**, contra o código real de `ui/base.js` e `ui/mods-runtime.js`
// carregado num `vm`:
//
// 1. Sair do servidor apaga o tema na hora — o caso do relato.
// 2. E a prévia, que é desta sessão como o tema.
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
  // As contagens da aba DIAGNÓSTICO, que a saída da sessão zera (tarefa 5):
  // moram depois do bloco do tema, em `base.js`.
  "const regioesAusentesAvisadas = new Map();",
  "const identificadoresIgnorados = new Map();",
  trecho(base, "function encerrarOAmbienteDosMods("),
  // O que `case "tema"` faz, base.js: `aplicarOTemaDoMod(mod.id, m.valores)`.
  `globalThis.__api = { aplicarOTemaDoMod, encerrarOAmbienteDosMods, entrarNaGeracao,
     InstanciaDeMod, ESTADOS_DE_MOD, modsCarregados };`,
].join("\n");

const estilo = new Map();
const sessao = {
  style: {
    setProperty: (k, v) => estilo.set(k, v),
    removeProperty: (k) => estilo.delete(k),
  },
};
// O selo «prévia não publicada» (`pintarOSeloDaPrevia`): um nó só, no corpo.
let selo = null;
const raizCss = { "--seele-osso": "#e8e4dc", "--seele-negro-absoluto": "#000000" };
const ctx = {
  console,
  setTimeout, clearTimeout, Promise, Map, Set, Error, Object, Number, String, JSON, Array, Math,
  CustomEvent: class { constructor(t) { this.type = t; } },
  dispatchEvent: () => true,
  $: (id) => (id === "tela-sessao" ? sessao : null),
  document: {
    documentElement: {},
    body: { append: (no) => { selo = no; } },
    getElementById: (id) => (id === "selo-da-previa" ? selo : null),
  },
  elemento: (tag, classe, texto) => ({ tag, className: classe, textContent: texto, hidden: false, setAttribute() {} }),
  getComputedStyle: () => ({ getPropertyValue: (t) => raizCss[t] ?? "" }),
  contribuicoesDosMods: { escolherSubstituicao: () => ({}), revogarDoMod: () => {} },
  modsCarregados: new Map(),
  modsExigidos: new Map(),
  pedidosDeMod: new Map(),
  conjuntoJaBuscado: "",
  superficiesDosMods: new Map(),
  encerrarOSomDosMods: () => {},
};
ctx.globalThis = ctx;
vm.createContext(ctx);
// `let`/`const` de topo não viram propriedade do contexto: declare os que o
// código atribui como variáveis do próprio script.
vm.runInContext(
  "let modsCarregados = globalThis.modsCarregados; let modsExigidos = globalThis.modsExigidos;"
  + "let pedidosDeMod = globalThis.pedidosDeMod; let conjuntoJaBuscado = '';"
  + "const superficiesDosMods = globalThis.superficiesDosMods;\n" + codigo,
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

/** O tema do servidor, como `aplicarOTemaDoServidor` o aplica, em base.js. */
function pedirTema(valores) {
  api.aplicarOTemaDoMod("seele/estilo", valores);
}

const falhas = [];
function cobrar(ok, caso, visto) {
  console.log(`${ok ? "ok    " : "FALHOU"} ${caso}${ok ? "" : ` — visto: ${JSON.stringify(visto)}`}`);
  if (!ok) falhas.push(caso);
}

(async () => {
  // 1. Sair do servidor.
  {
    estilo.clear();
    api.entrarNaGeracao(7);
    const inst = instanciaAtiva(7);
    pedirTema(TEMA);
    const durante = estilo.size;
    api.encerrarOAmbienteDosMods();
    const logoDepois = Object.fromEntries(estilo);
    await inst.encerramento;
    api.entrarNaGeracao(8);
    cobrar(durante > 0 && Object.keys(logoDepois).length === 0 && estilo.size === 0,
      "sair do servidor apaga o tema na hora", logoDepois);
  }

  // 2. A prévia, por cima do tema, sai junto.
  {
    estilo.clear();
    api.entrarNaGeracao(9);
    const inst = instanciaAtiva(9);
    pedirTema(TEMA);
    api.aplicarOTemaDoMod("seele/estilo", OUTRO, { previa: { superficie: "ajustes" } });
    const comAPrevia = estilo.get("--seele-laranja-nerv");
    api.encerrarOAmbienteDosMods();
    const logoDepois = Object.fromEntries(estilo);
    await inst.encerramento;
    api.entrarNaGeracao(10);
    cobrar(comAPrevia === "#2f8fe0" && Object.keys(logoDepois).length === 0 && selo?.hidden === true,
      "sair do servidor apaga a prévia do tema na hora, e o selo dela", { comAPrevia, logoDepois, selo: selo?.hidden });
  }

  if (falhas.length) process.exit(1);
  console.log("\no tema e a prévia saem com a sessão.");
})();
