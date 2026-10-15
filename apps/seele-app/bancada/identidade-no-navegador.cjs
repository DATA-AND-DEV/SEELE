// A identidade do navegador, o link e o convite num Chromium de verdade, com
// um servidor de mentira no lugar da sessão (Tarefa 3.4 da F4).
//
//     node apps/seele-app/bancada/identidade-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm
//
// O `.wasm` vem do `cargo xtask web`, com a `prova`: o link é lido pelo
// `analisar_link_em`, contra a origem dos casos da F1 (`origem.invalid`): a do
// produto (a D-web-2) não é a desta bancada, e todo link daqui cairia em
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
// 8. quem já é conhecido entra pela chave, sem gastar o convite;
// 9. **a casca de verdade**: a página inteira (`ui/`), sem casca injetada, com
//    e sem o `CELULAR` da casca do celular. O `numa-aba` só no celular. Os
//    casos de cima injetam a casca e não exercitam o caminho padrão: fixá-lo em
//    «larga» passava por eles (achado da orquestradora na décima segunda);
// 10. **os SERVIDORES DESTE APARELHO**: o `conhecidos()` do mais recente ao
//     mais antigo, com o nome que chega depois (`lembrarONome`), e o
//     `entrarNoConhecido` pela chave, sem convite, e com `link-vencido` quando
//     os hashes guardados venceram, sem bater em servidor nenhum;
// 11. **a identidade da página** (`identidadeDaPagina()`): uma só, que abre
//     sozinha o link da carga, e a casca não vê o token;
// 12. **outro link aberto no meio de uma entrada** (o E54 da F5, do lado do
//     navegador): a continuação da entrada velha não toca na tentativa nova —
//     nem na chave, nem no token, nem no aceite de MODs — e resolve
//     `sem-tentativa`; a que o servidor aceitou fica guardada com a chave dela
//     e é solta. E dois links abertos fora de ordem: o token do segundo não vai
//     ao servidor do primeiro; nem o servidor deste aparelho, procurado no
//     banco enquanto um link abre, entra no lugar dele.
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
for (const nome of ["ponte-web.js", "voz-web.js", "identidade-web.js"]) {
  fs.copyFileSync(path.join(raiz, "ui", nome), path.join(pasta, nome));
}
fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
// A página inteira, para o caso 9: uma cópia da `ui/`, servida em `/ui/`.
fs.cpSync(path.join(raiz, "ui"), path.join(pasta, "ui"), { recursive: true });
fs.writeFileSync(
  path.join(pasta, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>identidade</title>
<script src="ponte-web.js"></script>
<script src="voz-web.js"></script>
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
navigator.storage.persist = async () => {
  window.ordem.push("persist");
  // O que o caso 12 faz enquanto o navegador responde: outro link aberto no meio.
  const durante = window.duranteOPersist;
  window.duranteOPersist = null;
  if (durante) await durante();
  return persistDoNavegador();
};
const gerarDoNavegador = crypto.subtle.generateKey.bind(crypto.subtle);
crypto.subtle.generateKey = async (...args) => {
  window.ordem.push("chave");
  // O mesmo, enquanto a chave nasce; e o gancho que lança é a chave que não nasceu.
  const durante = window.duranteAChave;
  window.duranteAChave = null;
  if (durante) await durante();
  return gerarDoNavegador(...args);
};

// O banco que demora a responder (caso 12): o pedido seguinte de abri-lo só
// entrega o sucesso a quem o pediu quando a promessa da fila resolver.
window.atrasosDoBanco = [];
const abrirOBancoDoNavegador = indexedDB.open.bind(indexedDB);
indexedDB.open = (...args) => {
  const pedido = abrirOBancoDoNavegador(...args);
  const atraso = window.atrasosDoBanco.shift();
  if (atraso) {
    let aoAbrir = null;
    Object.defineProperty(pedido, "onsuccess", { get: () => aoAbrir, set: (f) => { aoAbrir = f; } });
    pedido.addEventListener("success", (evento) => atraso.then(() => aoAbrir && aoAbrir.call(pedido, evento)));
  }
  return pedido;
};

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

/**
 * O servidor de mentira: anota cada entrada e responde o roteiro, um por vez.
 * A entrada aceita devolve a geração dela, e o \`soltar\` anota a que foi solta.
 */
window.servidor = { entradas: [], roteiro: [], soltas: [] };
async function conectarNaMentira(alvo, apelido, credencial) {
  // Anotada mesmo sem chave: uma entrada que chega sem ela também chegou.
  const chave = alvo.chave ? hex(await crypto.subtle.exportKey("raw", alvo.chave.publicKey)) : null;
  window.servidor.entradas.push({ endereco: alvo.endereco, impressao: alvo.impressao, hashes: alvo.hashes, apelido, token: credencial, chave });
  const proxima = window.servidor.roteiro.shift() ?? "ok";
  // Uma resposta pode ser uma função: ela roda no meio da entrada (o relógio
  // que anda, ou outro link aberto, enquanto o servidor responde) e devolve a
  // resposta.
  const resposta = typeof proxima === "function" ? await proxima() : proxima;
  if (resposta !== "ok") throw resposta;
  return { geracao: window.servidor.entradas.length };
}

window.criar = async ({ casca = "larga", instalado = false, agora = Date.now() } = {}) => {
  const w = await window.carregado;
  return criarIdentidadeDoNavegador({
    analisar: analisarEm(w, "origem.invalid"),
    conectar: conectarNaMentira,
    soltar: (resposta) => {
      window.servidor.soltas.push(resposta);
    },
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
const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".wasm": "application/wasm",
};

function servir() {
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo)) {
      resposta.writeHead(404).end();
      return;
    }
    // A página inteira vai sem o cabeçalho da página mínima, como na
    // `ponte-sem-gesto.cjs`: ela tem a política dela.
    const cabecalhos = { "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" };
    if (!nome.startsWith("ui/")) cabecalhos["content-security-policy"] = CSP;
    resposta.writeHead(200, cabecalhos);
    resposta.end(fs.readFileSync(arquivo));
  });
}

const IMPRESSAO = "3cbcfb02".repeat(8);
const HASH = "ab".repeat(32);
const TOKEN = "7K4MNPQRSTVWXYZ23456";
const TOKEN_B = "23456XYZWVTSRQPNMK47";
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
    confere("estados", estados.produto === "outra-origem", `o analisador do produto leu ${estados.produto} num link de origem.invalid`);
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
    // A resposta do `persist()` chega com o `{ ok: true }`: com a tentativa
    // encerrada, o `ler()` já não a tem (achado da F8).
    confere("6", caso1.depois.persistencia === caso1.persistencia, `a entrada que deu certo não trouxe a persistência: ${JSON.stringify(caso1.depois)}`);
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

    // 10. Os SERVIDORES DESTE APARELHO.
    const OUTRO = "55".repeat(32);
    const aparelho = await rodar(async ([l, impressao, outro]) => {
      // Um segundo servidor, entrado depois: ele vem primeiro na lista.
      const depois = await window.criar({ agora: Date.now() + 60_000 });
      await depois.abrir(l);
      const entrou = await depois.entrar("ana");
      const identidade = await window.criar();
      const antesDoNome = await identidade.conhecidos();
      const lembrou = await identidade.lembrarONome(impressao, "Casa");
      const lista = await identidade.conhecidos();
      const conhecida = await identidade.entrarNoConhecido(impressao, "ana");
      const vencida = await (await window.criar({ agora: Date.now() + 400 * 86400_000 })).entrarNoConhecido(impressao, "ana");
      const desconhecida = await identidade.entrarNoConhecido("66".repeat(32), "ana");
      return { entrou, antesDoNome, lembrou, lista, conhecida, vencida, desconhecida, outro };
    }, [link({ fp: OUTRO }), IMPRESSAO, OUTRO]);
    const pelaChave = await rodar(() => window.servidor.entradas.splice(0));
    const [primeiro, segundo] = aparelho.lista;
    confere("10", aparelho.lista.length === 2 && primeiro.impressao === OUTRO && segundo.impressao === IMPRESSAO, `a lista não vem do mais recente ao mais antigo: ${JSON.stringify(aparelho.lista)}`);
    confere("10", aparelho.antesDoNome.every((s) => s.nome === null) && aparelho.lembrou === true && segundo.nome === "Casa", `o nome que chega depois: ${JSON.stringify(aparelho)}`);
    confere("10", aparelho.lista.every((s) => s.podeEntrar === true && typeof s.visto === "number"), `o podeEntrar e o visto: ${JSON.stringify(aparelho.lista)}`);
    confere("10", aparelho.conhecida.ok === true, `entrar no conhecido: ${JSON.stringify(aparelho.conhecida)}`);
    confere("10", aparelho.vencida.erro === "link-vencido" && aparelho.desconhecida.erro === "recusado", `o vencido e o desconhecido: ${JSON.stringify(aparelho)}`);
    confere("10", pelaChave.length === 2 && pelaChave[1].token === null && pelaChave[1].chave === hellos[0].chave && pelaChave[1].impressao === IMPRESSAO, `entrar no conhecido gastou convite, trocou de chave, ou o vencido bateu no servidor: ${JSON.stringify(pelaChave)}`);

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

    // 12. Outro link aberto no meio de uma entrada. Cada caso abre a tentativa
    // velha, e o link novo é aberto enquanto quem a entrada espera responde.
    const [VELHO, NOVO, MODS, PERSIST, FORA_A, FORA_B] = ["71", "72", "73", "74", "75", "76"].map((b) => b.repeat(32));
    const caso12 = await rodar(async (l) => {
      const novoNoMeio = (identidade, texto, resposta) => async () => {
        await identidade.abrir(texto);
        return resposta;
      };
      // a. O servidor aceita a entrada velha depois de o link novo abrir.
      const aceita = await window.criar();
      await aceita.abrir(l.velho);
      window.servidor.roteiro = [novoNoMeio(aceita, l.novo, "ok")];
      const velha = await aceita.entrar("ana");
      const lidaDepois = aceita.ler();
      const nova = await aceita.entrar("bia");
      const soltas = window.servidor.soltas.splice(0);
      const deVolta = await (await window.criar()).entrarNoConhecido(l.impressaoVelha, "ana");
      // b. O servidor recusa a velha: a recusa dela não encerra a nova.
      const recusada = await window.criar();
      await recusada.abrir(l.velho2);
      window.servidor.roteiro = [novoNoMeio(recusada, l.novo2, { Refused: { reason: "CredentialRejected" } })];
      const recusa = await recusada.entrar("ana");
      const lidaDepoisDaRecusa = recusada.ler();
      // c. A velha para nos MODs: o aceite não é da nova.
      const mods = await window.criar();
      await mods.abrir(l.velho3);
      window.servidor.roteiro = [novoNoMeio(mods, l.mods, { ModsNaoAceitos: { mods: [], conjunto: "c3" } })];
      const paradaNosMods = await mods.entrar("ana");
      const aceiteNaNova = await mods.aceitarOsMods("c3");
      // d. O link novo abre enquanto o navegador responde ao persist(): a
      // velha não conecta.
      const noPersist = await window.criar();
      await noPersist.abrir(l.velho4);
      window.duranteOPersist = () => noPersist.abrir(l.persist);
      window.ordem.splice(0);
      const antesDeConectar = await noPersist.entrar("ana");
      const lidaNoPersist = noPersist.ler();
      const ordemNoPersist = window.ordem.splice(0);
      // e. O mesmo enquanto a chave nasce, e com a chave que não nasce: a
      // falha da velha não encerra a nova.
      const naChave = await window.criar();
      await naChave.abrir(l.velho5);
      window.duranteAChave = () => naChave.abrir(l.chave);
      const enquantoNasce = await naChave.entrar("ana");
      const lidaNaChave = naChave.ler();
      const semChave = await window.criar();
      await semChave.abrir(l.velho6);
      window.duranteAChave = async () => {
        await semChave.abrir(l.semChave);
        throw new Error("a chave da bancada não nasceu");
      };
      const naoNasceu = await semChave.entrar("ana");
      const lidaSemChave = semChave.ler();
      return { velha, lidaDepois, nova, soltas, deVolta, recusa, lidaDepoisDaRecusa, paradaNosMods, aceiteNaNova, antesDeConectar, lidaNoPersist, ordemNoPersist, enquantoNasce, lidaNaChave, naoNasceu, lidaSemChave };
    }, {
      velho: link({ fp: VELHO }),
      impressaoVelha: VELHO,
      novo: link({ fp: NOVO, token: TOKEN_B }),
      velho2: link({ fp: "81".repeat(32) }),
      novo2: link({ fp: "82".repeat(32), token: TOKEN_B }),
      velho3: link({ fp: "83".repeat(32) }),
      mods: link({ fp: MODS, token: TOKEN_B }),
      velho4: link({ fp: "84".repeat(32) }),
      persist: link({ fp: PERSIST, token: TOKEN_B }),
      velho5: link({ fp: "85".repeat(32) }),
      chave: link({ fp: "86".repeat(32), token: TOKEN_B }),
      velho6: link({ fp: "87".repeat(32) }),
      semChave: link({ fp: "88".repeat(32), token: TOKEN_B }),
    });
    const caso12Hellos = await rodar(() => window.servidor.entradas.splice(0));
    const [daVelha, daNova, deVolta, daRecusada, daParada] = caso12Hellos;
    // Só cinco entradas chegam a servidor: a velha aceita, a nova, a volta pela
    // chave, a velha recusada e a parada nos MODs. Nenhuma entrada velha chega
    // ao servidor do link aberto no meio dela.
    const servidores12 = caso12Hellos.map((h) => h.impressao.slice(0, 2)).join(" ");
    confere("12", servidores12 === "71 72 71 81 83", `as entradas do caso chegaram a ${servidores12}, e não a 71 72 71 81 83`);
    confere("12", caso12.velha.erro === "sem-tentativa", `a entrada velha, aceita depois de o link novo abrir, resolveu ${JSON.stringify(caso12.velha)}`);
    confere("12", caso12.lidaDepois && caso12.lidaDepois.impressao === NOVO && caso12.lidaDepois.estado === "pronto", `a entrada velha encerrou a tentativa nova: ${JSON.stringify(caso12.lidaDepois)}`);
    confere("12", caso12.nova.ok === true && daNova && daNova.impressao === NOVO && daNova.token === TOKEN_B && daNova.chave !== daVelha.chave, `a tentativa nova não entrou com o token e a chave dela: ${JSON.stringify({ nova: caso12.nova, daVelha, daNova })}`);
    confere("12", caso12.soltas.length === 1 && caso12.soltas[0].geracao === 1, `a entrada velha não foi solta, ou foi solta a errada: ${JSON.stringify(caso12.soltas)}`);
    confere("12", caso12.deVolta.ok === true && deVolta && deVolta.impressao === VELHO && deVolta.chave === daVelha.chave, `o servidor que aceitou a velha não ficou guardado com a chave dela: ${JSON.stringify({ deVolta: caso12.deVolta, hello: deVolta })}`);
    confere("12", caso12.recusa.erro === "sem-tentativa" && caso12.lidaDepoisDaRecusa && caso12.lidaDepoisDaRecusa.impressao === "82".repeat(32), `a recusa da velha encerrou a nova: ${JSON.stringify({ recusa: caso12.recusa, lida: caso12.lidaDepoisDaRecusa })}`);
    confere("12", caso12.paradaNosMods.erro === "sem-tentativa" && caso12.aceiteNaNova.erro === "sem-tentativa" && caso12Hellos.length === 5, `o aceite de MODs da velha passou para a nova: ${JSON.stringify({ parada: caso12.paradaNosMods, aceite: caso12.aceiteNaNova, entradas: caso12Hellos.length })}`);
    confere("12", caso12.antesDeConectar.erro === "sem-tentativa" && caso12.lidaNoPersist && caso12.lidaNoPersist.impressao === PERSIST && caso12.lidaNoPersist.persistencia === null, `a entrada velha, com o link novo aberto durante o persist(), seguiu: ${JSON.stringify({ entrou: caso12.antesDeConectar, lida: caso12.lidaNoPersist, entradas: caso12Hellos.map((h) => h.impressao.slice(0, 2)) })}`);
    // A chave da tentativa nova nasce depois do persist() dela (o caso 6), e
    // não na continuação da velha.
    confere("12", JSON.stringify(caso12.ordemNoPersist) === '["persist"]', `a entrada velha, depois do persist() em que o link novo abriu, fez nascer uma chave: ${JSON.stringify(caso12.ordemNoPersist)}`);
    confere("12", caso12.enquantoNasce.erro === "sem-tentativa" && caso12.lidaNaChave && caso12.lidaNaChave.impressao === "86".repeat(32) && caso12.lidaNaChave.persistencia === null, `a entrada velha, com o link novo aberto enquanto a chave nascia, seguiu: ${JSON.stringify({ entrou: caso12.enquantoNasce, lida: caso12.lidaNaChave })}`);
    confere("12", caso12.naoNasceu.erro === "sem-tentativa" && caso12.lidaSemChave && caso12.lidaSemChave.impressao === "88".repeat(32), `a chave da velha que não nasceu encerrou a nova: ${JSON.stringify({ entrou: caso12.naoNasceu, lida: caso12.lidaSemChave })}`);
    confere("12", !daRecusada || daRecusada.token === TOKEN, `a velha recusada levou outro token: ${JSON.stringify(daRecusada)}`);
    confere("12", !daParada || daParada.token === TOKEN, `a velha parada nos MODs levou outro token: ${JSON.stringify(daParada)}`);
    // e. Dois links abertos fora de ordem: o banco responde ao primeiro depois
    // do segundo. A tentativa é a do segundo, e o token dele vai ao servidor
    // dele, e só.
    const foraDeOrdem = await rodar(async (l) => {
      let responder;
      window.atrasosDoBanco.push(new Promise((resolve) => (responder = resolve)));
      const identidade = await window.criar();
      const primeiro = identidade.abrir(l.a);
      const segundo = await identidade.abrir(l.b);
      responder();
      const doPrimeiro = await primeiro;
      const lida = identidade.ler();
      return { segundo, doPrimeiro, lida, entrou: await identidade.entrar("ana") };
    }, { a: link({ fp: FORA_A }), b: link({ fp: FORA_B, token: TOKEN_B }) });
    const foraHellos = await rodar(() => window.servidor.entradas.splice(0));
    confere("12", foraDeOrdem.lida && foraDeOrdem.lida.impressao === FORA_B, `com o banco fora de ordem, a tentativa ficou com o primeiro link: ${JSON.stringify(foraDeOrdem)}`);
    confere("12", foraHellos.length === 1 && foraHellos[0].impressao === FORA_B && foraHellos[0].token === TOKEN_B, `com o banco fora de ordem, o token foi a outro servidor: ${JSON.stringify(foraHellos.map((h) => ({ impressao: h.impressao.slice(0, 2), token: h.token })))}`);
    // f. Um link aberto enquanto o banco procura um servidor deste aparelho: a
    // vez é do link, e o conhecido não entra nem o encerra.
    const NO_MEIO = "77".repeat(32);
    const conhecidoNoMeio = await rodar(async (l) => {
      let responder;
      window.atrasosDoBanco.push(new Promise((resolve) => (responder = resolve)));
      const identidade = await window.criar();
      const pelaChave = identidade.entrarNoConhecido(l.conhecida, "ana");
      await identidade.abrir(l.link);
      responder();
      return { pelaChave: await pelaChave, depois: identidade.ler() };
    }, { conhecida: VELHO, link: link({ fp: NO_MEIO, token: TOKEN_B }) });
    const conhecidoHellos = await rodar(() => window.servidor.entradas.splice(0));
    confere("12", conhecidoNoMeio.pelaChave.erro === "sem-tentativa" && conhecidoNoMeio.depois && conhecidoNoMeio.depois.impressao === NO_MEIO && conhecidoHellos.length === 0, `o servidor conhecido entrou, ou encerrou o link aberto no meio: ${JSON.stringify({ ...conhecidoNoMeio, entradas: conhecidoHellos.length })}`);

    // 11. A identidade da página: abre o link da carga sozinha, e a casca não
    // vê o token. A página vem desta bancada, e não da origem do produto (a
    // D-web-2), então o link da carga cai em `outra-origem`, e é o que se espera.
    const daPagina = await contexto.newPage();
    daPagina.on("pageerror", (erro) => doConsole.push(`daPagina: pageerror: ${erro.message}`));
    await daPagina.goto(`${endereco}#198.51.100.7:8383?fp=${IMPRESSAO}&convite=${TOKEN}`);
    const pagina11 = await daPagina.evaluate(async () => {
      const identidade = await identidadeDaPagina();
      const outra = await identidadeDaPagina();
      return { lida: identidade.ler(), mesma: identidade === outra, sobrou: tomarOLinkDaCarga(), url: location.href };
    });
    confere("11", pagina11.mesma, "a página tem duas identidades");
    confere("11", pagina11.lida && pagina11.lida.estado === "outra-origem", `a identidade da página não abriu o link da carga: ${JSON.stringify(pagina11.lida)}`);
    confere("11", pagina11.sobrou === null, "o link da carga continua à mão depois de a identidade abri-lo");
    confere("11", !JSON.stringify(pagina11).includes(TOKEN), `o token apareceu para quem lê a identidade: ${JSON.stringify(pagina11)}`);
    await daPagina.close();

    // 9. A casca de verdade, sem nada injetado: a escolha guardada decide a
    // marca, a casca do celular sobe e põe o `CELULAR`, e a identidade pergunta
    // por ele pelo caminho padrão.
    const lido = {
      alvo: "198.51.100.7:8383",
      alternativos: [],
      impressao: "44".repeat(32),
      convite: TOKEN,
      sala: null,
      versao: "1.0",
      hashes: [HASH],
      ate: FUTURO,
    };
    for (const [casca, esperado, opcoes] of [
      ["celular", "numa-aba", { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }],
      ["larga", "pronto", { viewport: { width: 1280, height: 860 } }],
    ]) {
      const real = await navegador.newContext(opcoes);
      const pagina = await real.newPage();
      pagina.on("pageerror", (erro) => doConsole.push(`${casca}: pageerror: ${erro.message}`));
      await pagina.addInitScript((c) => localStorage.setItem("seele.casca", c), casca);
      await pagina.goto(`${endereco}ui/index.html`);
      const visto = await pagina.evaluate(async (l) => {
        const identidade = criarIdentidadeDoNavegador({ analisar: () => ({ link: l }), conectar: async () => ({}) });
        return {
          marca: document.documentElement.dataset.casca,
          celular: typeof CELULAR === "object" && CELULAR !== null,
          estado: (await identidade.abrir("o link da bancada")).estado,
        };
      }, lido);
      confere("9", visto.marca === casca, `a página da casca ${casca} saiu com a marca ${visto.marca}`);
      confere("9", visto.celular === (casca === "celular"), `o CELULAR da casca ${casca}: ${visto.celular}`);
      confere("9", visto.estado === esperado, `na casca ${casca}, sem nada injetado, o link abriu ${visto.estado}, e não ${esperado}`);
      await real.close();
    }

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
