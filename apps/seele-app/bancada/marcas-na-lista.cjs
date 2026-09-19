// A marca de um MOD na lista de pessoas: limites, recusa e descarte, contra o
// código de verdade.
//
// Irmão de `regiao-do-mod.cjs`. A razão de existir é a mesma: um guarda de
// texto prova que a linha está escrita, e nunca que um texto de duzentos
// caracteres realmente não chega à lista.
//
// # Por que fatiar em vez de carregar `base.js` inteiro
//
// `base.js` abre janelas, fala com a ponte do Tauri e conhece a tela toda;
// carregá-lo aqui exigiria um produto de mentira do tamanho do produto. O que
// este arquivo faz é recortar **o texto exato** das duas funções e das três
// constantes que a marca usa — se alguém as renomear, o recorte falha, e falhar
// é a resposta certa: um instrumento que silenciosamente para de medir é pior
// que nenhum.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const raiz = path.resolve(__dirname, "..");
const base = fs.readFileSync(path.join(raiz, "ui/base.js"), "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

/** Recorta do começo de `abre` até o fim do bloco que ele abre. */
function recortar(marca) {
  const i = base.indexOf(marca);
  if (i < 0) throw new Error(`«${marca}» não está em base.js`);
  let fundura = 0;
  for (let j = i; j < base.length; j += 1) {
    if (base[j] === "{") fundura += 1;
    else if (base[j] === "}") {
      fundura -= 1;
      if (fundura === 0) return base.slice(i, j + 1);
    }
  }
  throw new Error(`«${marca}» não fecha`);
}

function constante(nome) {
  const linha = base.split("\n").find((l) => l.startsWith(`const ${nome} =`));
  if (!linha) throw new Error(`a constante ${nome} não está em base.js`);
  return linha;
}

// O produto de mentira: só o que as duas funções tocam. `redesenharAsPessoas`
// é o gancho que `tela-sessao.js` define — aqui ele conta quantas vezes foi
// chamado, que é exatamente o que interessa provar.
let repintou = 0;
const caixa = {
  redesenharAsPessoas: () => { repintou += 1; },
  console,
};
vm.createContext(caixa);
vm.runInContext(
  [
    constante("COR_DO_TEMA"),
    constante("marcasDosMods"),
    constante("MARCAS_POR_MOD"),
    constante("TEXTO_DA_MARCA"),
    recortar("function marcarPessoasDoMod("),
    recortar("function marcasDaPessoa("),
    // O que este arquivo precisa alcançar de fora.
    "globalThis.__marcar = marcarPessoasDoMod;",
    "globalThis.__ler = marcasDaPessoa;",
    "globalThis.__mapa = marcasDosMods;",
    "globalThis.__tetos = { pessoas: MARCAS_POR_MOD, texto: TEXTO_DA_MARCA };",
  ].join("\n"),
  caixa,
);

const marcar = caixa.__marcar;
const ler = caixa.__ler;
const mapa = caixa.__mapa;
const tetos = caixa.__tetos;

function recusa(caso, corpo, pedaco) {
  try {
    corpo();
    confere(caso, false, "aceitou o que devia recusar");
  } catch (erro) {
    confere(caso, String(erro.message).includes(pedaco), `recusou por outra razão: ${erro.message}`);
  }
}

// ------------------------------------------------------- o caso que funciona

marcar("seele/perfis", { 7: { texto: "ELFA", cor: "#88ccff" } });
confere(
  "a marca chega",
  ler(7).length === 1 && ler(7)[0].texto === "ELFA" && ler(7)[0].cor === "#88ccff",
  JSON.stringify(ler(7)),
);
confere("a lista repinta", repintou === 1, `repintou ${repintou} vezes`);

// O `id` do snapshot é um número e a chave do MOD é texto: as duas pontas têm
// de se encontrar, ou a marca some sem dizer nada.
confere("o número acha a marca do texto", ler("7").length === 1, "a chave não normaliza");
confere("quem não foi marcado não tem marca", ler(8).length === 0, "marca vazada");

// ---------------------------------------------------------------- os limites

marcar("seele/perfis", { 7: { texto: "X".repeat(tetos.texto + 40) } });
confere(
  "o texto é cortado no teto",
  ler(7)[0].texto.length === tetos.texto,
  `ficou com ${ler(7)[0].texto.length}`,
);

const demais = {};
for (let i = 0; i <= tetos.pessoas; i += 1) demais[i] = { texto: "a" };
recusa("pessoas demais", () => marcar("seele/perfis", demais), "marca até");

recusa("cor livre", () => marcar("seele/perfis", { 7: { texto: "a", cor: "red" } }), "#rrggbb");
recusa(
  "cor que não é texto",
  () => marcar("seele/perfis", { 7: { texto: "a", cor: { toString: () => "#000000" } } }),
  "#rrggbb",
);

// Uma marca sem texto não é marca: ela viraria um retângulo vazio na linha de
// alguém.
marcar("seele/perfis", { 7: { texto: "" }, 9: { texto: "ANÃO" } });
confere("marca vazia não entra", ler(7).length === 0, "um selo vazio entrou na lista");
confere("a outra entrou", ler(9)[0].texto === "ANÃO", JSON.stringify(ler(9)));

// A recusa não pode deixar metade gravada: o MOD pediu um conjunto, e um
// conjunto pela metade é pior que nenhum.
recusa("recusa é inteira", () => marcar("seele/perfis", { 9: { texto: "ORC" }, 10: { texto: "b", cor: "azul" } }), "#rrggbb");
confere("nada de meio gravado", ler(10).length === 0 && ler(9)[0].texto === "ANÃO", JSON.stringify([ler(9), ler(10)]));

// ---------------------------------------------------------------- o descarte

marcar("seele/estilo", { 9: { texto: "TEMA" } });
confere("dois MODs marcam a mesma pessoa", ler(9).length === 2, JSON.stringify(ler(9)));
mapa.delete("seele/estilo");
confere("some junto com o MOD", ler(9).length === 1 && ler(9)[0].texto === "ANÃO", JSON.stringify(ler(9)));

// Marcar de novo **substitui**: um MOD que marca a cada ciclo não pode acumular
// selos na mesma pessoa até a linha não caber mais.
marcar("seele/perfis", { 9: { texto: "MAGA" } });
confere("a marca nova substitui a velha", ler(9).length === 1 && ler(9)[0].texto === "MAGA", JSON.stringify(ler(9)));

// -------------------------------------------------------------------- o fim

if (falhas.length) {
  console.error("marcas na lista: " + falhas.length + " falha(s)");
  for (const f of falhas) console.error("  · " + f);
  process.exit(1);
}
console.log("marcas na lista: as 12 provas passam");
