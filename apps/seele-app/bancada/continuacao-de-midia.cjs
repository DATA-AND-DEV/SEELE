// A continuação opaca de uma mídia do servidor, contra o `base.js` de verdade.
//
// O laço mora em `donoDaRegiao`, que é função de `base.js` e não sai de lá sem
// uma janela inteira em volta. Este arquivo extrai **o laço** pelo texto e o
// roda com um servidor de mentira: é menos que rodar `base.js` inteiro, e é
// mais que um guarda de texto — ele prova a ordem, o acúmulo e o teto.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const fonte = fs.readFileSync(
  path.resolve(__dirname, "../ui/base.js"),
  "utf8",
);
const corpo = fonte
  .split("carregarMidiaDoServidor: async (canal, pedido, campo) => {")[1]
  .split("\n    },")[0];

const falhas = [];
const confere = (caso, cond, detalhe) => { if (!cond) falhas.push(`${caso}: ${detalhe}`); };

function rodar({ respostas, campo = "bytes" }) {
  const pedidos = [];
  const contexto = vm.createContext({
    geracaoDaSessao: 1,
    daGeracaoDePe: () => true,
    meu: () => true,
    mod: { id: "a/b" },
    PEDACOS_DE_MIDIA: 128,
    pedirAoServidor: async (_id, _canal, pedido) => {
      pedidos.push(JSON.parse(JSON.stringify(pedido)));
      return respostas(pedidos.length - 1);
    },
    invoke: async (_cmd, args) => ({ uri: "data:", papel: "imagem", bytes: args.base64.length, base64: args.base64 }),
  });
  const fn = vm.runInContext(`(async (canal, pedido, campo) => {${corpo}})`, contexto);
  return { pedidos, resultado: fn(1, { op: "asset", slot: "avatar" }, campo) };
}

(async () => {
  // Três pedaços, com a continuação que o servidor manda.
  {
    const caso = "a continuação junta os pedaços na ordem";
    const { pedidos, resultado } = rodar({
      campo: "image",
      respostas: (i) => [
        { image: "AAA", proximo: { offset: 3, path: "p1" } },
        { image: "BBB", proximo: { offset: 6, path: "p1" } },
        { image: "CCC" },
      ][i],
    });
    const midia = await resultado;
    confere(caso, midia.base64 === "AAABBBCCC", `juntou «${midia.base64}»`);
    confere(caso, pedidos.length === 3, `foram ${pedidos.length} pedidos`);
    confere(
      caso,
      pedidos[1].offset === 3 && pedidos[1].path === "p1" && pedidos[1].op === "asset",
      `a continuação não foi juntada ao pedido original: ${JSON.stringify(pedidos[1])}`,
    );
    confere(
      caso,
      pedidos[2].offset === 6,
      `o segundo proximo não substituiu o primeiro: ${JSON.stringify(pedidos[2])}`,
    );
  }

  // Um servidor que nunca para é parado pelo teto.
  {
    const caso = "o teto de pedaços para um servidor sem fim";
    const { pedidos, resultado } = rodar({
      campo: "image",
      respostas: () => ({ image: "A", proximo: { offset: 1 } }),
    });
    await resultado;
    confere(caso, pedidos.length === 128, `pediu ${pedidos.length} vezes em vez de parar em 128`);
  }

  // Um campo que não veio é dito pelo nome.
  {
    const caso = "o campo ausente é dito pelo nome";
    const { resultado } = rodar({ campo: "image", respostas: () => ({ outra: "coisa" }) });
    let dito = "";
    await resultado.catch((e) => { dito = e.message; });
    confere(caso, dito.includes("image"), `a falha não nomeia o campo: «${dito}»`);
  }

  if (falhas.length === 0) {
    console.log("continuação de mídia: as 3 provas passam");
    process.exit(0);
  }
  for (const f of falhas) console.error(`FALHOU — ${f}`);
  process.exit(1);
})();
