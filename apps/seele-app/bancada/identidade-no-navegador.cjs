// A identidade do navegador, o link e o convite num Chromium de verdade, com
// um servidor de mentira no lugar da sessão (Tarefa 3.4 da F4).
//
//     node apps/seele-app/bancada/identidade-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm
//
// O `.wasm` vem do `cargo xtask web`, com a `prova`: o link é lido pelo
// `analisar_link_em`, contra a origem dos casos da F1 (`origem.invalid`),
// porque a do produto é vazia até o D-web-2 e todo link cairia em
// `outra-origem`. Isso também se confere, pelo `analisar_link` de verdade.
//
// O servidor de mentira anota cada entrada (o alvo, o apelido, o token e a
// chave pública) e responde o que o caso pede, na forma do `connect` do
// desktop. O que ela prova, os três casos do plano e o resto:
//
// 1. **o `mods-a-aceitar` e a volta com o mesmo token**: a primeira entrada
//    para nos MODs, o `aceitarOsMods` entra de novo com o mesmo token e a mesma
//    chave, e o desfecho é `{ ok: true }`;
// 2. **depois de entrar, o token não está em armazenamento nenhum da origem**
//    (`localStorage`, `sessionStorage`, IndexedDB, `caches`), procurado pelo
//    texto, e a tentativa não o guarda mais;
// 3. **o `agoraNao()` esquece**: depois dele, o `aceitarOsMods` recusa por não
//    haver tentativa, e nada mais chega ao servidor;
// 4. os estados, em ordem de precedência, e nada conecta ao abrir;
// 5. o `apelido-tomado` e o `sem-rede` não gastam a tentativa; o
//    `CredentialRejected` encerra, como `convite-gasto` ou `convite-vencido`
//    pela validade do link;
// 6. o `persist()` é pedido antes de a chave nascer;
// 7. o fragmento sai da barra na carga, e o link da carga é entregue uma vez;
// 8. quem já é conhecido entra pela chave, sem gastar o convite.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const [caminhoDoWasm] = process.argv.slice(2);
if (!caminhoDoWasm || !fs.existsSync(caminhoDoWasm)) {
  console.error("uso: node apps/seele-app/bancada/identidade-no-navegador.cjs <seele_web.wasm>  (o `cargo xtask web` o compila, com a `prova`)");
  process.exit(2);
}

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("identidade-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};

const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "identidade-no-navegador-"));
for (const nome of ["ponte-web.js", "identidade-web.js"]) {
  fs.copyFileSync(path.join(raiz, "ui", nome), path.join(pasta, nome));
}
fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
fs.writeFileSync(
  path.join(pasta, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>identidade</title>
<script src="ponte-web.js"></script>
<script src="identidade-web.js"></script>
<script src="bancada.js"></script>`,
);

// A bancada na página: o `.wasm`, o servidor de mentira e o que anota a ordem
// do `persist()` e do nascimento da chave.
fs.writeFileSync(
  path.join(pasta, "bancada.js"),
  `"use strict";
window.ordem = [];
const persistDoNavegador = navigator.storage.persist.bind(navigator.storage);
navigator.storage.persist = async () => { window.ordem.push("persist"); return persistDoNavegador(); };
const gerarDoNavegador = crypto.subtle.generateKey.bind(crypto.subtle);
crypto.subtle.generateKey = async (...args) => { window.ordem.push("chave"); return gerarDoNavegador(...args); };

window.carregado = fetch("seele_web.wasm").then((r) => r.arrayBuffer()).then((b) => WebAssembly.instantiate(b, {})).then(({ instance }) => instance.exports);

/** O link lido contra a origem dos casos da F1. */
function analisarEm(w, origem) {
  return (texto) => {
    const t = new TextEncoder().encode(texto);
    const o = new TextEncoder().encode(origem);
    const pt = w.alocar(t.length);
    const po = w.alocar(o.length);
    new Uint8Array(w.memory.buffer).set(t, pt);
    new Uint8Array(w.memory.buffer).set(o, po);
    w.analisar_link_em(pt, t.length, po, o.length);
    w.liberar(pt, t.length);
    w.liberar(po, o.length);
    return JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.saida_ptr(), w.saida_len()).slice()));
  };
}

const hex = (bytes) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");

/** O servidor de mentira: anota cada entrada e responde o roteiro, um por vez. */
window.servidor = { entradas: [], roteiro: [] };
async function conectarNaMentira(alvo, apelido, credencial) {
  const chave = hex(await crypto.subtle.exportKey("raw", alvo.chave.publicKey));
  window.servidor.entradas.push({ endereco: alvo.endereco, impressao: alvo.impressao, hashes: alvo.hashes, apelido, token: credencial, chave });
  const proxima = window.servidor.roteiro.shift() ?? "ok";
  // Uma resposta pode ser uma função: ela roda no meio da entrada (o relógio
  // que anda enquanto o servidor responde) e devolve a resposta.
  const resposta = typeof proxima === "function" ? proxima() : proxima;
  if (resposta !== "ok") throw resposta;
  return {};
}

window.criar = async ({ casca = "larga", instalado = false, agora = Date.now() } = {}) => {
  const w = await window.carregado;
  return criarIdentidadeDoNavegador({
    analisar: analisarEm(w, "origem.invalid"),
    conectar: conectarNaMentira,
    agora: typeof agora === "function" ? agora : () => agora,
    instalado: () => instalado,
    casca: () => casca,
  });
};
window.criarComOProduto = async () => {
  const w = await window.carregado;
  return criarIdentidadeDoNavegador({ analisar: analisadorDoWasm(w), conectar: conectarNaMentira });
};

/** Todo texto que a origem guarda: os quatro armazenamentos, inteiros. */
window.tudoQueAOrigemGuarda = async () => {
  const partes = [];
  for (const area of [localStorage, sessionStorage]) {
    for (let i = 0; i < area.length; i++) partes.push(area.key(i), area.getItem(area.key(i)));
  }
  for (const { name, version } of await indexedDB.databases()) {
    const banco = await new Promise((resolve, reject) => {
      const pedido = indexedDB.open(name, version);
      pedido.onsuccess = () => resolve(pedido.result);
      pedido.onerror = () => reject(pedido.error);
    });
    for (const loja of banco.objectStoreNames) {
      const registros = await new Promise((resolve, reject) => {
        const pedido = banco.transaction(loja, "readonly").objectStore(loja).getAll();
        pedido.onsuccess = () => resolve(pedido.result);
        pedido.onerror = () => reject(pedido.error);
      });
      partes.push(JSON.stringify(registros));
    }
    banco.close();
  }
  for (const nome of await caches.keys()) {
    const cache = await caches.open(nome);
    for (const pedido of await cache.keys()) partes.push(pedido.url, await (await cache.match(pedido)).text());
  }
  return partes.join("\\n");
};
`,
);

const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; frame-ancestors 'none'";
const TIPOS = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".wasm": "application/wasm" };

function servir() {
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo)) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream", "content-security-policy": CSP });
    resposta.end(fs.readFileSync(arquivo));
  });
}

const IMPRESSAO = "3cbcfb02".repeat(8);
const HASH = "ab".repeat(32);
const TOKEN = "7K4MNPQRSTVWXYZ23456";
const FUTURO = Math.floor(Date.now() / 1000) + 7 * 86400;
const link = ({ origem = "origem.invalid", fp = IMPRESSAO, wh = HASH, ate = FUTURO, token = TOKEN } = {}) => {
  const partes = [];
  if (fp) partes.push(`fp=${fp}`);
  if (token) partes.push(`convite=${token}`);
  partes.push("v=1.0");
  if (wh) partes.push(`wh=${wh}`);
  if (ate) partes.push(`ate=${ate}`);
  return `https://${origem}/e#198.51.100.7:8383?${partes.join("&")}`;
};

async function principal() {
  const navegador = await chromium().launch();
  const servidor = servir();
  await new Promise((resolve) => servidor.listen(0, "127.0.0.1", resolve));
  const endereco = `http://127.0.0.1:${servidor.address().port}/`;
  try {
    const contexto = await navegador.newContext();
    const aba = await contexto.newPage();
    const doConsole = [];
    aba.on("pageerror", (erro) => doConsole.push(`pageerror: ${erro.message}`));
    aba.on("console", (m) => m.type() === "error" && doConsole.push(m.text()));

    // 7. O fragmento sai da barra na carga.
    await aba.goto(`${endereco}#198.51.100.7:8383?fp=${IMPRESSAO}&convite=${TOKEN}`);
    const carga = await aba.evaluate(() => ({ url: location.href, hash: location.hash, primeira: tomarOLinkDaCarga(), segunda: tomarOLinkDaCarga() }));
    confere("fragmento", carga.hash === "" && !carga.url.includes("#") && !carga.url.includes(TOKEN), `o fragmento ficou na barra: ${carga.url}`);
    confere("fragmento", carga.primeira && carga.primeira.includes(TOKEN), "o link da carga não foi entregue");
    confere("fragmento", carga.segunda === null, "o link da carga foi entregue duas vezes");

    const rodar = (f, arg) => aba.evaluate(f, arg);
    const entradas = () => rodar(() => window.servidor.entradas.length);

    // 4. Os estados, e nada conecta ao abrir.
    const estados = await rodar(async ([l, passado]) => {
      const lerCom = async (texto, opcoes) => (await (await window.criar(opcoes)).abrir(texto)).estado;
      return {
        outra: await lerCom(l.outra),
        incompleto: await (await window.criar()).abrir(l.semFp),
        antigo: await lerCom(l.semWh),
        vencido: await lerCom(l.vencido),
        numaAba: await lerCom(l.bom, { casca: "celular", instalado: false }),
        instalado: await lerCom(l.bom, { casca: "celular", instalado: true }),
        pc: await lerCom(l.bom, { casca: "larga" }),
        produto: (await (await window.criarComOProduto()).abrir(l.bom)).estado,
        passado,
      };
    }, [
      {
        outra: link({ origem: "outra.invalid" }),
        semFp: link({ fp: null }),
        semWh: link({ wh: null }),
        vencido: link({ ate: Math.floor(Date.now() / 1000) - 60 }),
        bom: link(),
      },
      null,
    ]);
    confere("estados", estados.outra === "outra-origem", `outra origem leu ${estados.outra}`);
    confere("estados", estados.incompleto.estado === "incompleto" && estados.incompleto.motivo === "SemImpressaoDigital", `sem fp leu ${JSON.stringify(estados.incompleto)}`);
    confere("estados", estados.antigo === "servidor-0.15", `sem wh leu ${estados.antigo}`);
    confere("estados", estados.vencido === "vencido", `o link vencido leu ${estados.vencido}`);
    confere("estados", estados.numaAba === "numa-aba", `o celular numa aba leu ${estados.numaAba}`);
    confere("estados", estados.instalado === "pronto" && estados.pc === "pronto", `instalado ${estados.instalado}, no PC ${estados.pc}`);
    confere("estados", estados.produto === "outra-origem", `o analisador do produto, sem origem até o D-web-2, leu ${estados.produto}`);
    const leitura = await rodar(async (l) => (await window.criar()).abrir(l), link());
    confere("estados", leitura.endereco === "198.51.100.7:8383" && leitura.impressao === IMPRESSAO && leitura.valeAte === FUTURO && leitura.persistencia === null, `a leitura do pronto: ${JSON.stringify(leitura)}`);
    confere("estados", !JSON.stringify(leitura).includes(TOKEN), `a leitura mostra o token à tela: ${JSON.stringify(leitura)}`);
    confere("estados", (await entradas()) === 0, "abrir um link conectou");
    const fora = await rodar(async (l) => {
      const vencido = await window.criar();
      await vencido.abrir(l.vencido);
      const aba = await window.criar({ casca: "celular" });
      await aba.abrir(l.bom);
      return [await vencido.entrar("ana"), await aba.entrar("ana")];
    }, { vencido: link({ ate: Math.floor(Date.now() / 1000) - 60 }), bom: link() });
    confere("estados", fora[0].erro === "convite-vencido" && fora[1].erro === "recusado" && fora[1].razao === "numa-aba", `entrar fora do pronto: ${JSON.stringify(fora)}`);
    confere("estados", (await entradas()) === 0, "entrar num link vencido ou numa aba conectou");

    // 1. O mods-a-aceitar e a volta com o mesmo token.
    const caso1 = await rodar(async (l) => {
      const mods = { ModsNaoAceitos: { mods: [{ id: "estilo" }], conjunto: "c1" } };
      window.servidor.roteiro = [mods, "ok"];
      const identidade = await window.criar();
      await identidade.abrir(l);
      const primeira = await identidade.entrar("ana");
      const persistencia = identidade.ler().persistencia;
      const depois = await identidade.aceitarOsMods(primeira.conjunto);
      return { primeira, depois, persistencia, ler: identidade.ler(), copiou: await identidade.copiarLink() };
    }, link());
    const hellos = await rodar(() => window.servidor.entradas.splice(0));
    confere("1", caso1.primeira.erro === "mods-a-aceitar" && caso1.primeira.conjunto === "c1", `a primeira entrada: ${JSON.stringify(caso1.primeira)}`);
    confere("1", caso1.depois.ok === true, `a volta depois do aceite: ${JSON.stringify(caso1.depois)}`);
    confere("1", hellos.length === 2 && hellos.every((h) => h.token === TOKEN), `a volta não levou o mesmo token: ${JSON.stringify(hellos.map((h) => h.token))}`);
    confere("1", hellos.length === 2 && hellos[0].chave === hellos[1].chave && hellos[0].chave.length === 64, "a volta não levou a mesma chave");
    confere("1", hellos.length === 2 && hellos[0].hashes[0] === HASH, `os hashes do link não chegaram ao alvo: ${JSON.stringify(hellos[0])}`);
    confere("6", ["concedida", "negada", "sem-suporte"].includes(caso1.persistencia), `a persistência depois do entrar: ${caso1.persistencia}`);
    const ordem = await rodar(() => window.ordem.splice(0));
    confere("6", ordem.indexOf("persist") !== -1 && ordem.indexOf("persist") < ordem.indexOf("chave"), `o persist() não veio antes da chave: ${JSON.stringify(ordem)}`);

    // 2. Depois de entrar, o token não está em armazenamento nenhum.
    const guardado = await rodar(() => window.tudoQueAOrigemGuarda());
    confere("2", !guardado.includes(TOKEN), "o token ficou num armazenamento da origem");
    confere("2", guardado.includes(IMPRESSAO), "a busca não leu o IndexedDB da identidade, e ficou cega");
    confere("2", caso1.ler === null && caso1.copiou === false, `a tentativa ainda guarda o token: ${JSON.stringify(caso1)}`);

    // 8. Quem já é conhecido entra pela chave, sem gastar o convite.
    const conhecido = await rodar(async (l) => {
      const identidade = await window.criar();
      const lida = await identidade.abrir(l);
      return { estado: lida.estado, entrou: await identidade.entrar("ana") };
    }, link());
    const volta = await rodar(() => window.servidor.entradas.splice(0));
    confere("8", conhecido.estado === "ja-conhecido" && conhecido.entrou.ok === true, `o servidor já conhecido: ${JSON.stringify(conhecido)}`);
    confere("8", volta.length === 1 && volta[0].token === null && volta[0].chave === hellos[0].chave, `quem já é conhecido gastou o convite ou trocou de chave: ${JSON.stringify(volta)}`);

    // 3. O agoraNao() esquece.
    const outra = "11".repeat(32);
    const caso3 = await rodar(async (l) => {
      window.servidor.roteiro = [{ ModsNaoAceitos: { mods: [], conjunto: "c2" } }];
      const identidade = await window.criar();
      await identidade.abrir(l);
      const primeira = await identidade.entrar("bia");
      identidade.agoraNao();
      return { primeira, depois: await identidade.aceitarOsMods("c2"), ler: identidade.ler(), copiou: await identidade.copiarLink() };
    }, link({ fp: outra }));
    const caso3Hellos = await rodar(() => window.servidor.entradas.splice(0));
    confere("3", caso3.primeira.erro === "mods-a-aceitar", `a primeira entrada: ${JSON.stringify(caso3.primeira)}`);
    confere("3", caso3.depois.erro === "sem-tentativa" && caso3Hellos.length === 1, `depois do agoraNao(), o aceite ainda entrou: ${JSON.stringify(caso3)}`);
    confere("3", caso3.ler === null && caso3.copiou === false, "o agoraNao() deixou o link na tentativa");

    // 5. O que segue e o que encerra.
    const outra2 = "22".repeat(32);
    const caso5 = await rodar(async (l) => {
      window.servidor.roteiro = [{ Refused: { reason: "NicknameTaken" } }, "SemResposta", { Refused: { reason: "CredentialRejected" } }];
      const identidade = await window.criar();
      await identidade.abrir(l);
      return [await identidade.entrar("ana"), await identidade.entrar("ana2"), await identidade.entrar("ana3"), await identidade.entrar("ana4")];
    }, link({ fp: outra2 }));
    const caso5Hellos = await rodar(() => window.servidor.entradas.splice(0));
    confere("5", caso5[0].erro === "apelido-tomado" && caso5[1].erro === "sem-rede", `o que segue: ${JSON.stringify(caso5)}`);
    confere("5", caso5[2].erro === "convite-gasto" && caso5[3].erro === "sem-tentativa", `o que encerra: ${JSON.stringify(caso5)}`);
    confere("5", caso5Hellos.length === 3 && caso5Hellos.every((h) => h.token === TOKEN), `o apelido tomado e a rede gastaram a tentativa: ${JSON.stringify(caso5Hellos.map((h) => h.token))}`);
    // O link vale ao abrir e vence enquanto o servidor responde: a mesma
    // recusa, que o servidor dá igual a todo convite que não vale (spec 08),
    // vira `convite-vencido` pela validade do link.
    const ate = Math.floor(Date.now() / 1000) + 60;
    const vencida = await rodar(async ([l, ate]) => {
      let relogio = Date.now();
      window.servidor.roteiro = [() => {
        relogio = (ate + 1) * 1000;
        return { Refused: { reason: "CredentialRejected" } };
      }];
      const identidade = await window.criar({ agora: () => relogio });
      await identidade.abrir(l);
      return identidade.entrar("ana");
    }, [link({ fp: "33".repeat(32), ate }), ate]);
    confere("5", vencida.erro === "convite-vencido", `o convite que venceu na entrada: ${JSON.stringify(vencida)}`);
    await rodar(() => window.servidor.entradas.splice(0));

    confere("console", doConsole.length === 0, doConsole.join(" | "));
  } finally {
    await navegador.close();
    servidor.close();
  }
}

principal()
  .catch((erro) => falhas.push(`a bancada estourou: ${erro && erro.stack ? erro.stack : erro}`))
  .finally(() => {
    terminou = true;
    fs.rmSync(pasta, { recursive: true, force: true });
    if (falhas.length) {
      for (const f of falhas) console.error(`FALHOU — ${f}`);
      process.exitCode = 1;
    } else {
      console.log("identidade-no-navegador: ok");
    }
  });
