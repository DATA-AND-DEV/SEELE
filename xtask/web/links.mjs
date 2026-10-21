// Os casos do link do navegador da F1, DENTRO do `.wasm` do `seele-web` e pela
// fronteira de verdade.
//
//     node xtask/web/links.mjs <seele_web.wasm> <crates/seele-web/tests/links-da-f1.json>
//
// A tabela é a mesma que `crates/seele-web/tests/links_da_f1.rs` passa no
// nativo: um caso por asserção do módulo `a_forma_do_navegador` do `uri.rs`.
// Os casos têm origem própria (`origem.invalid`), e passam por
// `analisar_link_em`, da feature `prova`. A função da página, `analisar_link`,
// não recebe origem: usa a `ORIGEM_DO_NAVEGADOR` do produto (a D-web-2), e por
// isso recusa o link certo de `origem.invalid`. Isso também é conferido aqui:
// a página não escolhe contra que origem um link é lido.
import { readFileSync } from "node:fs";

const [caminhoDoWasm, caminhoDaTabela] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(caminhoDoWasm), {});
const e = instance.exports;
const enc = new TextEncoder();
const dec = new TextDecoder();
const memoria = () => new Uint8Array(e.memory.buffer);
const saida = () => JSON.parse(dec.decode(memoria().slice(e.saida_ptr(), e.saida_ptr() + e.saida_len())));
function comTexto(texto, f) {
  const b = enc.encode(texto);
  const p = e.alocar(b.length);
  memoria().set(b, p);
  try { return f(p, b.length); } finally { e.liberar(p, b.length); }
}
const igual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const { casos } = JSON.parse(readFileSync(caminhoDaTabela, "utf8"));
const falhas = [];
for (const caso of casos) {
  const codigo = comTexto(caso.texto, (tp, tl) => comTexto(caso.origem, (op, ol) => e.analisar_link_em(tp, tl, op, ol)));
  const lido = saida();
  if ("erro" in caso.esperado) {
    if (codigo !== 1 || lido.erro !== caso.esperado.erro) falhas.push(`${caso.de}: esperava o erro ${caso.esperado.erro}, veio ${JSON.stringify(lido)}`);
    continue;
  }
  if (codigo !== 0) { falhas.push(`${caso.de}: esperava um link, veio ${JSON.stringify(lido)}`); continue; }
  for (const [campo, valor] of Object.entries(caso.esperado.link)) {
    if (!igual(lido.link[campo], valor)) falhas.push(`${caso.de}: o campo ${campo} veio ${JSON.stringify(lido.link[campo])}`);
  }
}

// A função da página, com o primeiro caso bom da tabela: ele é de
// `origem.invalid`, e a página o recusa.
const bom = casos.find((c) => "link" in c.esperado);
const codigo = comTexto(bom.texto, (p, n) => e.analisar_link(p, n));
if (codigo !== 1 || saida().erro !== "OutraOrigem") {
  falhas.push(`analisar_link aceitou um link de ${bom.origem}: a página escolheria a origem (${JSON.stringify(saida())})`);
}

// E o mesmo caso na origem do produto (a D-web-2), que a página aceita. A
// origem é lida do Rust, e não escrita aqui: o guarda
// `a_origem_do_web_app_e_escrita_num_lugar_so` reprova uma cópia.
const doRust = /pub const ORIGEM_DO_NAVEGADOR: &str = "([^"]+)";/.exec(readFileSync("crates/seele-proto/src/uri.rs", "utf8"));
if (!doRust) {
  falhas.push("não achei a ORIGEM_DO_NAVEGADOR no crates/seele-proto/src/uri.rs, ou ela está vazia");
} else {
  const naOrigem = bom.texto.replace(`https://${bom.origem}`, `https://${doRust[1]}`);
  const aceito = comTexto(naOrigem, (p, n) => e.analisar_link(p, n));
  const lido = saida();
  if (naOrigem === bom.texto || aceito !== 0 || !igual(lido.link?.alvo, bom.esperado.link.alvo)) {
    falhas.push(`analisar_link não leu o link da origem do produto (${doRust[1]}): ${JSON.stringify(lido)}`);
  }
}

if (casos.length < 16) falhas.push(`a tabela encolheu para ${casos.length} casos; este guarda ficou cego`);
if (falhas.length) {
  console.error(`links da F1 no .wasm: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log(`links da F1 no .wasm: os ${casos.length} casos pela fronteira; e a função da página lê só a origem do produto.`);
