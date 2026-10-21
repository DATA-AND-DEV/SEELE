// O que a tela de aceite diz de cada MOD exigido (ADR 0045; `tema.porta` do
// `api/v6.json`; D-api-8). Node puro, com a `linhaDeMod` de `camada-mods.js` e
// a `fraseDeErro` de `frases.js` de verdade, recortadas e rodadas aqui.
//
// O ADR 0045 manda mostrar, antes de qualquer byte ser baixado, quem é o MOD,
// de onde veio e o que alcança. Duas coisas não são mais um alcance na lista,
// e por isso têm linha própria: o MOD que **roda na máquina de quem hospeda**
// (rede de saída, relógio e registro) e o que **pode mudar a aparência do
// servidor para todos que entram** (a porta do tema, `"tema": true`). Quem
// aceita tem de saber das duas, na tela e na frase do erro.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const ui = (nome) => fs.readFileSync(path.join(__dirname, "../ui", nome), "utf8");

/** Do começo da função até o fim dela, como o arquivo a escreve. */
function recorte(fonte, assinatura) {
  const inicio = fonte.indexOf(assinatura);
  const fim = fonte.indexOf("\n}\n", inicio);
  if (inicio < 0 || fim < 0) throw new Error(`não achei ${assinatura}`);
  return fonte.slice(inicio, fim + 3);
}

/** Um nó de mentira: a etiqueta, a classe, o texto e os filhos. */
function no(tag, classe, texto) {
  return {
    tag, className: classe ?? "", textContent: texto ?? "", filhos: [], title: "",
    append(...outros) { this.filhos.push(...outros); },
  };
}
const textos = (raiz) => [raiz.textContent, ...raiz.filhos.flatMap(textos)].filter(Boolean);
const comClasse = (raiz, classe) => [
  ...(raiz.className.split(/\s+/).includes(classe) ? [raiz] : []),
  ...raiz.filhos.flatMap((f) => comClasse(f, classe)),
];

const contexto = vm.createContext({ console, elemento: no });
vm.runInContext(recorte(ui("camada-mods.js"), "function linhaDeMod(mod, indice) {"), contexto);
const daFrase = vm.createContext({ console });
vm.runInContext(ui("frases.js"), daFrase);

const MOD = {
  id: "fulano/tema", version: "4.0.0", hash: "ab".repeat(32),
  repo: "https://github.com/fulano/tema", reach: ["tema"], no_servidor: true,
};
const TEMA = /aparência deste servidor/;

// A linha da tela: o MOD com a porta do tema diz isso numa linha própria, e
// o que não a tem não diz.
const comTema = contexto.linhaDeMod({ ...MOD, tema: true }, 0);
const semTema = contexto.linhaDeMod({ ...MOD, tema: false }, 0);
const linhaDoTema = comClasse(comTema, "mods-tema");
assert.equal(linhaDoTema.length, 1, `a linha do MOD com "tema": true não tem a linha do tema: ${JSON.stringify(textos(comTema))}`);
assert.match(linhaDoTema[0].textContent, TEMA, `a linha do tema diz «${linhaDoTema[0].textContent}»`);
assert.equal(comClasse(semTema, "mods-tema").length, 0, "a linha de um MOD sem a porta do tema diz que ele muda a aparência");
// E a do servidor continua, à parte: as duas são coisas diferentes.
assert.equal(comClasse(comTema, "mods-servidor").length, 1, "a linha do servidor sumiu com a do tema");

// A frase do erro, quando a pessoa ainda não aceitou: a mesma linha.
const frase = (tema) => daFrase.fraseDeErro({ ModsNaoAceitos: { mods: [{ ...MOD, tema }], conjunto: "c" } }, "", "");
assert.match(frase(true), TEMA, `a frase dos MODs não aceitos não diz o tema: ${frase(true)}`);
assert.doesNotMatch(frase(false), TEMA, "a frase de um MOD sem a porta do tema diz que ele muda a aparência");

console.log("aceite-dos-mods: a tela e a frase dizem, numa linha própria, o MOD que pode mudar a aparência do servidor, e só ele.");
