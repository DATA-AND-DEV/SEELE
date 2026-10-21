// Os cabeçalhos da origem do web app, lidos de `web/_headers`, para as bancadas
// servirem a página como a origem a serve (D-web-2). Não é bancada: as bancadas
// o usam.
//
// O formato é o do `_headers` do Cloudflare Pages e do Netlify: uma linha com o
// caminho, e embaixo dela, recuadas, as linhas `Nome: valor`. As regras que
// casam com o mesmo caminho se somam; `/*` casa com todos.

const fs = require("node:fs");
const path = require("node:path");

const ARQUIVO = path.resolve(__dirname, "..", "web", "_headers");

function lerAsRegras() {
  const regras = [];
  let atual = null;
  for (const linha of fs.readFileSync(ARQUIVO, "utf8").split("\n")) {
    if (linha.trim() === "" || linha.trimStart().startsWith("#")) continue;
    if (!/^\s/.test(linha)) {
      atual = { caminho: linha.trim(), cabecalhos: [] };
      regras.push(atual);
      continue;
    }
    const dois = linha.indexOf(":");
    if (!atual || dois < 0) throw new Error(`web/_headers: linha sem caminho ou sem «:»: ${linha}`);
    atual.cabecalhos.push([linha.slice(0, dois).trim().toLowerCase(), linha.slice(dois + 1).trim()]);
  }
  if (regras.length === 0) throw new Error("web/_headers não tem regra nenhuma");
  return regras;
}

const REGRAS = lerAsRegras();

const casa = (caminho, pedido) =>
  caminho.endsWith("*") ? pedido.startsWith(caminho.slice(0, -1)) : caminho === pedido;

/** Os cabeçalhos da origem para o caminho pedido (`/`, `/index.html`, …), como um objeto. */
function cabecalhosDaOrigem(pedido) {
  const juntos = {};
  for (const { caminho, cabecalhos } of REGRAS) {
    if (!casa(caminho, pedido)) continue;
    for (const [nome, valor] of cabecalhos) juntos[nome] = nome in juntos ? `${juntos[nome]}, ${valor}` : valor;
  }
  return juntos;
}

module.exports = { cabecalhosDaOrigem, ARQUIVO_DOS_CABECALHOS: ARQUIVO };
