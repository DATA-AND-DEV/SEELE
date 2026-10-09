// Os eventos da tela e o aviso da metade de servidor, como chegam a um MOD da
// API 6 (rascunho da API 6, §7.2 e §7.3; tarefa 8 do plano da F3).
//
// # Por que esta bancada existe
//
// O MOD assina mudanças em vez de consultar o retrato em laço: o canal aberto,
// a sala, quem entrou e saiu, quem fala, a forma da casca. Cada evento leva os
// nomes do retrato (§6), e nasce **onde o estado muda** — a comparação entre o
// retrato de antes e o de agora —, e não num temporizador que repetiria o mesmo
// estado. A fala tem teto: no máximo 4 por segundo, e o conjunto mais novo
// sempre chega.
//
// # O que ela mede
//
// As três funções de `ui/base.js`, recortadas e rodadas num `vm`: a comparação
// (`mudancasDaTela`), o ritmo da fala (`criarRitmoDaFala`, com um relógio de
// mentira) e a entrega do aviso do servidor (`entregarOAvisoDoMod`).

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repo = path.resolve(__dirname, "../../..");
const base = fs.readFileSync(`${repo}/apps/seele-app/ui/base.js`, "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

/** Do início da função até o primeiro "\n}\n" depois dele. */
function trecho(comeco) {
  const i = base.indexOf(comeco);
  if (i < 0) throw new Error(`não achei ${comeco} em ui/base.js`);
  const j = base.indexOf("\n}\n", i);
  return base.slice(i, j + 3);
}

const registrados = [];
const ditos = [];
const contexto = {
  JSON, Object, Array, Number, String, Set, Map, Math,
  CAPACIDADES_POR_API: { 6: ["eventos-da-tela", "aviso-do-servidor"], 5: ["regiao"] },
  modsCarregados: new Map(),
  registrarNoAnfitriao: (...args) => registrados.push(args),
  donoDaRegiao: (mod, instancia) => ({ falar: (dados) => ditos.push({ mod: mod.id, api: mod.api, instancia, dados }) }),
};
vm.createContext(contexto);
vm.runInContext(
  [
    trecho("function mudancasDaTela("),
    trecho("function criarRitmoDaFala("),
    trecho("function entregarOAvisoDoMod("),
    "globalThis.__api = { mudancasDaTela, criarRitmoDaFala, entregarOAvisoDoMod };",
  ].join("\n"),
  contexto,
);
const api = contexto.__api;

/** Um retrato mínimo, com o que importa a cada caso. */
const retrato = (extra = {}) => ({
  eu: 1,
  servidor: { nome: "Casa" },
  pessoas: [{ id: 1, apelido: "ana", falando: false, mudo: false }],
  salas: [{ id: 3, nome: "Mesa", limite: 0, pessoas: [], minha: false }],
  canais: [{ id: 6, nome: "geral", aberto: true }, { id: 7, nome: "mesa", aberto: false }],
  onde: "computador",
  regioes: ["pagina"],
  ...extra,
});

// ------------------------------------------------- a comparação

{
  const nada = api.mudancasDaTela(retrato(), retrato());
  confere("nada mudou", nada.canal === undefined && nada.sala === undefined
    && nada.entraram.length === 0 && nada.sairam.length === 0 && !nada.lugar && nada.falando === undefined,
  `o mesmo retrato deu mudança: ${JSON.stringify(nada)}`);

  const canal = api.mudancasDaTela(retrato(), retrato({
    canais: [{ id: 6, nome: "geral", aberto: false }, { id: 7, nome: "mesa", aberto: true }],
  }));
  confere("canal", canal.canal === 7, `trocar o canal aberto não deu \`canal: 7\`: ${JSON.stringify(canal)}`);

  const entrou = api.mudancasDaTela(retrato(), retrato({
    salas: [{ id: 3, nome: "Mesa", limite: 0, pessoas: [1], minha: true }],
  }));
  confere("sala · entrou", entrou.sala === 3, `entrar na sala 3 não deu \`sala: 3\`: ${JSON.stringify(entrou)}`);
  const saiu = api.mudancasDaTela(
    retrato({ salas: [{ id: 3, nome: "Mesa", limite: 0, pessoas: [1], minha: true }] }),
    retrato(),
  );
  confere("sala · saiu", saiu.sala === null, `sair da sala não deu \`sala: null\`: ${JSON.stringify(saiu)}`);

  const pessoas = api.mudancasDaTela(retrato(), retrato({
    pessoas: [{ id: 2, apelido: "juli", falando: false, mudo: false }],
  }));
  confere("pessoas", JSON.stringify(pessoas.entraram) === "[2]" && JSON.stringify(pessoas.sairam) === "[1]",
    `quem entrou e quem saiu não vieram: ${JSON.stringify(pessoas)}`);

  const fala = api.mudancasDaTela(retrato(), retrato({
    pessoas: [{ id: 1, apelido: "ana", falando: true, mudo: false }],
  }));
  confere("fala", JSON.stringify(fala.falando) === "[1]", `começar a falar não mudou \`falando\`: ${JSON.stringify(fala)}`);

  const lugar = api.mudancasDaTela(retrato(), retrato({ onde: "celular" }));
  confere("lugar", lugar.lugar === true, `a casca mudar de forma não deu \`lugar\`: ${JSON.stringify(lugar)}`);
}

// ------------------------------------------------- o ritmo da fala

{
  let agora = 0;
  const agendados = [];
  const soltos = [];
  const ritmo = api.criarRitmoDaFala({
    agora: () => agora,
    agendar: (fn, ms) => agendados.push({ quando: agora + ms, fn }),
    soltar: (falando) => soltos.push({ agora, falando }),
  });
  // Dez trocas de quem fala dentro de um segundo.
  for (let n = 0; n < 10; n += 1) {
    agora = n * 100;
    ritmo.mudou([n]);
    // Roda o que venceu.
    for (const item of agendados.splice(0)) {
      if (item.quando <= agora) item.fn();
      else agendados.push(item);
    }
  }
  agora = 2000;
  for (const item of agendados.splice(0)) item.fn();
  const noPrimeiroSegundo = soltos.filter((s) => s.agora < 1000).length;
  confere("fala · teto", noPrimeiroSegundo <= 4,
    `${noPrimeiroSegundo} eventos de fala no primeiro segundo: ${JSON.stringify(soltos)}`);
  confere("fala · o mais novo chega", JSON.stringify(soltos.at(-1)?.falando) === "[9]",
    `o conjunto mais novo não chegou: ${JSON.stringify(soltos)}`);
}

// ------------------------------------------------- o aviso do servidor

{
  const seis = { api: 6 };
  const cinco = { api: 5 };
  contexto.modsCarregados.set("seele/mesa", seis);
  contexto.modsCarregados.set("seele/velho", cinco);

  api.entregarOAvisoDoMod({ mod_id: "seele/mesa", canal: 7, carga: '{"revisao":3}' });
  const dito = ditos.find((d) => d.mod === "seele/mesa");
  confere("aviso · a 6 recebe", dito?.dados?.nome === "servidor" && dito.dados.canal === 7
    && dito.dados.carga?.revisao === 3 && dito.instancia === seis,
  `o aviso não chegou ao MOD da 6 como \`servidor\`, com a carga já lida: ${JSON.stringify(dito)}`);

  api.entregarOAvisoDoMod({ mod_id: "seele/velho", canal: null, carga: "1" });
  confere("aviso · a 5 não recebe", !ditos.some((d) => d.mod === "seele/velho"),
    "um MOD da API 5 recebeu o aviso, que é da 6");

  api.entregarOAvisoDoMod({ mod_id: "seele/ausente", canal: null, carga: "1" });
  confere("aviso · sem instância", !ditos.some((d) => d.mod === "seele/ausente"),
    "o aviso de um MOD sem janela de pé foi entregue a alguém");

  const antes = registrados.length;
  api.entregarOAvisoDoMod({ mod_id: "seele/mesa", canal: null, carga: "isto não é json" });
  confere("aviso · carga que não é JSON", registrados.length === antes + 1
    && ditos.filter((d) => d.mod === "seele/mesa").length === 1,
  `uma carga que não é JSON foi entregue, ou calada: ${JSON.stringify(registrados.slice(antes))}`);
}

// ------------------------------------------------- a cola, de retrato em retrato

{
  const doBloco = (comeco, fim) => {
    const i = base.indexOf(comeco);
    const j = base.indexOf(fim, i);
    if (i < 0 || j < 0) throw new Error(`não achei o bloco ${comeco}`);
    return base.slice(i, j);
  };
  const falados = [];
  const cola = {
    JSON, Object, Array, Number, String, Set, Map, Math, Date, setTimeout,
    document: { documentElement: { dataset: {} } },
    CAPACIDADES_POR_API: { 6: ["eventos-da-tela"], 5: ["regiao"] },
    modsCarregados: new Map([["seele/mesa", { api: 6 }], ["seele/velho", { api: 5 }]]),
    donoDaRegiao: (mod) => ({ falar: (dados) => falados.push({ mod: mod.id, dados }) }),
  };
  vm.createContext(cola);
  vm.runInContext(
    [
      "var geracaoDaSessao = 1;",
      trecho("function projetarORetrato("),
      doBloco("const REGIOES_E_CAMADAS_DA_API", "/**\n * O que o retrato diz da casca"),
      trecho("function cascaDoRetrato("),
      trecho("function mudancasDaTela("),
      trecho("function criarRitmoDaFala("),
      doBloco("/** O retrato da última vez que os MODs souberam da tela", "/**\n * **A recusa como o MOD a recebe**"),
      "globalThis.__avisar = avisarOsModsDaTela;",
    ].join("\n"),
    cola,
  );
  const snapshot = (extra) => ({
    me: 1, server: "Casa",
    presentes: [{ id: 1, nickname: "ana", speaking: false, muted: false }],
    voice_rooms: [{ id: 3, name: "Mesa", limit: 0, occupied_by_us: false, people: [] }],
    channels: [{ id: 6, name: "geral", open: true }, { id: 7, name: "mesa", open: false }],
    ...extra,
  });
  cola.__avisar(snapshot({}));
  confere("cola · o primeiro retrato", falados.length === 0,
    `o primeiro retrato da sessão virou evento: ${JSON.stringify(falados)}`);
  cola.__avisar(snapshot({
    presentes: [
      { id: 1, nickname: "ana", speaking: true, muted: false },
      { id: 2, nickname: "juli", speaking: false, muted: false },
    ],
    voice_rooms: [{ id: 3, name: "Mesa", limit: 0, occupied_by_us: true, people: [{ id: 1 }] }],
    channels: [{ id: 6, name: "geral", open: false }, { id: 7, name: "mesa", open: true }],
  }));
  const nomes = falados.filter((f) => f.mod === "seele/mesa").map((f) => JSON.stringify(f.dados));
  for (const esperado of [
    '{"nome":"canal","canal":7}',
    '{"nome":"sala","sala":3}',
    '{"nome":"pessoas","entrou":2}',
    '{"nome":"fala","falando":[1]}',
  ]) {
    confere("cola · o segundo retrato", nomes.includes(esperado),
      `faltou ${esperado}: ${JSON.stringify(nomes)}`);
  }
  confere("cola · só a API 6", !falados.some((f) => f.mod === "seele/velho"),
    "um MOD da API 5 recebeu evento da tela");
}

if (falhas.length) {
  console.error(`eventos-da-tela: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log("eventos-da-tela: ok");
