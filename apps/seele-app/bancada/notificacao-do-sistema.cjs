// A notificação do sistema na página de navegador, com a ponte de verdade.
//
// A `ui/` inteira num Chromium, **sem duble**: quem atende é a ponte
// (`ui/ponte-web.js`), quem decide é a regra (`ui/notificacao-do-sistema.js`), e
// quem desenha é CONFIGURAÇÕES (`ui/tela-server.js`). De mentira aqui só o
// `Notification` do navegador, posto antes da página: ele anota o que foi
// mostrado e **em que momento a permissão foi pedida** — pelo
// `navigator.userActivation` do próprio navegador, que é quem sabe se há um
// gesto.
//
// O que ela prende, nas duas cascas (o PC, 1280 px; o celular, 390 px e toque):
//
//  1. Quem nunca escolheu: a opção está em MICROFONE E SOM, desligada, e nem a
//     carga nem abrir a tela pedem permissão ao navegador.
//  2. Ligar é um clique de verdade, e o navegador é perguntado **dentro** dele.
//     Concedida, a confirmação aparece e a tela diz «ligadas».
//  3. Com a página na frente, a menção não notifica. Fora da frente, notifica
//     uma vez, com o texto certo; a segunda, logo depois, não.
//  4. O toque da sala notifica pelo ouvinte de verdade (`camada-toque.js`).
//  5. O clique na notificação traz a página à frente e leva à sessão da menção.
//  6. Negada pelo navegador: a tela **diz**, com o caminho, e nada notifica.
//  7. Sem a API (o iPhone fora de página instalada): a opção para, e diz que
//     não há.
//  8. Nada estoura, nenhum dos três comandos volta recusado pela ponte, e a
//     frase do estado cabe na coluna.
//
// O que ela **não** alcança: a notificação de verdade do navegador (a de
// mentira não aparece em tela nenhuma), o Chrome do Android e o Safari do
// iPhone de verdade, e o app — lá quem emite é o plugin, e a bancada do Rust é
// `src/notificacao.rs`.
//
//     PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/notificacao-do-sistema.cjs [pasta-para-as-capturas]
//
// Sai com 1 dizendo cada caso que falhou.

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const raiz = path.resolve(__dirname, "..");
const capturas = process.argv.slice(2).find((argumento) => !argumento.startsWith("--")) ?? null;

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("notificacao-do-sistema: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

// **Uma cópia**, e não a `ui/`: um arquivo de bancada esquecido lá iria para o
// instalador.
const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "notificacao-do-sistema-"));
fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });

const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".webmanifest": "application/manifest+json",
};

function servir() {
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404);
      resposta.end();
      return;
    }
    resposta.writeHead(200, {
      ...cabecalhosDaOrigem(`/${nome}`),
      "content-type": TIPOS[path.extname(arquivo)] ?? "application/octet-stream",
    });
    fs.createReadStream(arquivo).pipe(resposta);
  });
}

const CASCAS = {
  larga: { viewport: { width: 1280, height: 860 } },
  celular: { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true },
};

/**
 * O `Notification` de mentira, posto antes de qualquer script da página.
 * `resposta` é o que o navegador responde à pergunta; `semApi` tira a API.
 */
function porNotificacaoDeMentira({ resposta, semApi }) {
  const mundo = { mostradas: [], pedidos: [], focos: 0, naFrente: true, vistas: [] };
  window.notificacaoDeMentira = mundo;
  // A página «na frente» é o que o caso diz, e não o que a janela sem cabeça
  // do Chromium acha: é a única coisa do documento que a bancada troca.
  Object.defineProperty(Document.prototype, "hasFocus", { configurable: true, value: () => mundo.naFrente });
  const focar = window.focus.bind(window);
  window.focus = () => {
    mundo.focos += 1;
    focar();
  };
  if (semApi) {
    delete window.Notification;
    return;
  }
  class Notificacao {
    constructor(titulo, opcoes = {}) {
      this.titulo = titulo;
      this.corpo = opcoes.body ?? "";
      this.marca = opcoes.tag ?? null;
      this.fechada = false;
      this.onclick = null;
      mundo.mostradas.push(this);
    }
    close() {
      this.fechada = true;
    }
    static requestPermission() {
      mundo.pedidos.push({ noGesto: navigator.userActivation?.isActive === true });
      Notificacao.permission = resposta;
      return Promise.resolve(resposta);
    }
  }
  Notificacao.permission = "default";
  window.Notification = Notificacao;
}

const MENCAO = (sessao) => ({
  sessao,
  geracao: 1,
  vista: false,
  evento: { NoticeRaised: { notice: { severity: "Info", reason: "Mentioned", operator_text: null } } },
});

async function abrir(navegador, url, casca, mentira) {
  const contexto = await navegador.newContext(CASCAS[casca]);
  const aba = await contexto.newPage();
  const estouros = [];
  aba.on("pageerror", (erro) => estouros.push(String(erro.message).slice(0, 240)));
  await aba.addInitScript(porNotificacaoDeMentira, mentira);
  await aba.goto(url);
  await aba.waitForFunction(() => typeof notificacaoDoSistema === "object" && typeof abrirServer === "function");
  return { contexto, aba, estouros };
}

/** Abre CONFIGURAÇÕES em MICROFONE E SOM e espera a opção ser desenhada. */
async function abrirAOpcao(aba) {
  await aba.evaluate(async () => {
    await abrirServer("tela-boot");
    abrirSecao("secao-audio");
  });
  await aba.locator("#notificacoes-bloco").scrollIntoViewIfNeeded();
  await aba.waitForFunction(() => document.getElementById("notificacoes-estado").textContent !== "");
}

/** Uma captura, depois de o interruptor acabar de deslizar. */
async function capturar(aba, nome) {
  await aba.waitForTimeout(400);
  await aba.screenshot({ path: path.join(capturas, nome) });
}

const ver = (aba) =>
  aba.evaluate(() => {
    const chave = document.getElementById("notificacoes-ligadas");
    const estado = document.getElementById("notificacoes-estado");
    const bloco = document.getElementById("notificacoes-bloco");
    const rotulo = chave.closest("label");
    const caixa = (no) => {
      const r = no.getBoundingClientRect();
      return { largura: Math.round(r.width), altura: Math.round(r.height), esquerda: Math.round(r.left), direita: Math.round(r.right) };
    };
    const m = window.notificacaoDeMentira;
    return {
      marcada: chave.checked,
      parada: chave.disabled,
      dito: estado.textContent,
      permissao: estado.dataset.permissao,
      noPainelDeAudio: document.getElementById("painel-audio").contains(bloco),
      aVista: bloco.getBoundingClientRect().height > 0 && getComputedStyle(bloco).display !== "none" && !bloco.closest("[hidden]"),
      bloco: caixa(bloco),
      rotulo: caixa(rotulo),
      estado: caixa(estado),
      larguraDaJanela: document.documentElement.clientWidth,
      rolaDeLado: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      guardado: localStorage.getItem("seele.notificacoes"),
      pedidos: m.pedidos,
      mostradas: m.mostradas.map((n) => ({ titulo: n.titulo, corpo: n.corpo, marca: n.marca, fechada: n.fechada })),
      focos: m.focos,
      contas: [...CONTAS_DA_PONTE.entries()].filter(([comando]) => /notific/.test(comando)),
      contasDaRegra: JSON.parse(JSON.stringify(notificacaoDoSistema.contas)),
    };
  });

/** Põe a página na frente ou atrás, e deixa a regra e a ponte lerem isso. */
const porNaFrente = (aba, sim) => aba.evaluate((valor) => { window.notificacaoDeMentira.naFrente = valor; }, sim);

/** Emite pela ponte, como a sessão e a sala emitem, e espera a regra terminar. */
async function emitir(aba, canal, carga) {
  await aba.evaluate(([nome, payload]) => emitirNaPonte(nome, payload), [canal, carga]);
  await aba.waitForTimeout(150);
}

async function oCaminhoInteiro(navegador, url, casca) {
  const falhas = [];
  const confere = (ok, frase) => { if (!ok) falhas.push(`${casca}: ${frase}`); };
  const { contexto, aba, estouros } = await abrir(navegador, url, casca, { resposta: "granted", semApi: false });
  try {
    // 1. Quem nunca escolheu.
    await aba.waitForTimeout(1500);
    let v = await ver(aba);
    confere(v.pedidos.length === 0, `a carga pediu a permissão ao navegador sem gesto (${v.pedidos.length})`);
    await abrirAOpcao(aba);
    v = await ver(aba);
    confere(v.noPainelDeAudio && v.aVista, "a opção não está à vista em MICROFONE E SOM");
    confere(v.marcada === false && v.parada === false, `quem nunca escolheu vê a opção ${v.marcada ? "ligada" : "desligada"} e ${v.parada ? "parada" : "solta"}`);
    confere(v.dito === "Desligadas: o SEELE só avisa dentro da janela.", `desligada, a tela diz: ${v.dito}`);
    confere(v.pedidos.length === 0, "abrir CONFIGURAÇÕES pediu a permissão ao navegador sem gesto");
    confere(v.mostradas.length === 0, "uma notificação saiu antes de alguém ligar");
    // A menção com a opção desligada e a página atrás: nada.
    await porNaFrente(aba, false);
    await emitir(aba, "seele://event", MENCAO(77));
    v = await ver(aba);
    confere(v.mostradas.length === 0 && v.contasDaRegra.porque.desligadas === 1, `desligada, a menção: ${JSON.stringify(v.mostradas)} ${JSON.stringify(v.contasDaRegra)}`);
    await porNaFrente(aba, true);
    if (capturas) await capturar(aba, `notificacoes-${casca}-desligadas.png`);

    // 2. Ligar, num clique de verdade.
    await aba.locator("#notificacoes-ligadas").click();
    await aba.waitForFunction(() => document.getElementById("notificacoes-estado").dataset.permissao === "Concedida");
    v = await ver(aba);
    confere(v.pedidos.length === 1, `a permissão foi pedida ${v.pedidos.length} vez(es)`);
    confere(v.pedidos[0]?.noGesto === true, "a permissão foi pedida FORA do gesto: o navegador não mostra a pergunta");
    confere(v.marcada === true && v.guardado === "sim", `ligada, a opção ficou ${v.marcada} e a origem guardou ${v.guardado}`);
    confere(v.dito === "Ligadas. O navegador deixa esta página notificar.", `ligada, a tela diz: ${v.dito}`);
    confere(v.mostradas.length === 1 && v.mostradas[0].titulo === "Notificações do SEELE ligadas", `a confirmação: ${JSON.stringify(v.mostradas)}`);
    confere(!v.rolaDeLado, "a página passou a rolar de lado com a opção ligada");
    confere(v.estado.direita <= v.larguraDaJanela && v.estado.esquerda >= 0, `a frase do estado sai da janela: ${JSON.stringify(v.estado)} em ${v.larguraDaJanela}`);
    confere(v.rotulo.altura >= 44, `o interruptor tem ${v.rotulo.altura}px de alvo, e não 44`);
    if (capturas) await capturar(aba, `notificacoes-${casca}-ligadas.png`);

    // 3. A menção: na frente não, atrás sim, repetida não.
    await emitir(aba, "seele://event", MENCAO(77));
    v = await ver(aba);
    confere(v.mostradas.length === 1, `com a página na frente a menção notificou: ${JSON.stringify(v.mostradas)}`);
    // O que a página avisou por dentro vale meio minuto: a menção seguinte vem de outro servidor.
    await porNaFrente(aba, false);
    await emitir(aba, "seele://event", MENCAO(78));
    v = await ver(aba);
    const mencao = v.mostradas[1];
    confere(v.mostradas.length === 2, `com a página atrás a menção não notificou: ${JSON.stringify(v.contasDaRegra)}`);
    confere(mencao?.titulo === "Você foi chamado" && mencao?.corpo === "numa conversa do SEELE", `o texto da menção: ${JSON.stringify(mencao)}`);
    await emitir(aba, "seele://event", MENCAO(78));
    v = await ver(aba);
    confere(v.mostradas.length === 2 && v.contasDaRegra.porque.repetida === 1, `a menção repetida notificou de novo: ${v.mostradas.length}, ${JSON.stringify(v.contasDaRegra)}`);
    // Mensagem comum de canal não notifica.
    await emitir(aba, "seele://event", { sessao: 78, geracao: 1, vista: false, evento: "MessagesChanged" });
    v = await ver(aba);
    confere(v.mostradas.length === 2, "uma mensagem comum de canal notificou");

    // 4. O toque da sala, pelo ouvinte de verdade.
    await emitir(aba, "seele://toque", { evento: "Tocou", quem: "ff00", codigo: "K9P2-CSM7-NBFR-AYQ3", apelido: "rafa", nome_na_agenda: null, com_som: true });
    v = await ver(aba);
    const toque = v.mostradas[2];
    confere(toque?.titulo === "Ligação chegando" && toque?.corpo === "«rafa» está ligando", `o toque: ${JSON.stringify(toque)}`);
    await emitir(aba, "seele://toque", { evento: "Tocou", quem: "ff00", codigo: "K9P2-CSM7-NBFR-AYQ3", apelido: "rafa", nome_na_agenda: null, com_som: true });
    v = await ver(aba);
    confere(v.mostradas.length === 3, "o mesmo toque notificou duas vezes");

    // 5. O clique: a página vem à frente, e a casca vai à sessão da menção.
    await aba.evaluate(() => {
      window.verSessao = async (sessao) => { window.notificacaoDeMentira.vistas.push(sessao); };
      window.notificacaoDeMentira.mostradas[1].onclick();
    });
    await aba.waitForTimeout(100);
    const clique = await aba.evaluate(() => ({ focos: window.notificacaoDeMentira.focos, vistas: window.notificacaoDeMentira.vistas, fechada: window.notificacaoDeMentira.mostradas[1].fechada }));
    confere(clique.focos === 1 && clique.fechada, `o clique não trouxe a página à frente: ${JSON.stringify(clique)}`);
    confere(clique.vistas.length === 1 && clique.vistas[0] === 78, `o clique não levou à sessão da menção: ${JSON.stringify(clique.vistas)}`);

    // Desligar: nada mais sai.
    await porNaFrente(aba, true);
    await aba.locator("#notificacoes-ligadas").click();
    await aba.waitForFunction(() => document.getElementById("notificacoes-estado").textContent.startsWith("Desligadas"));
    await porNaFrente(aba, false);
    await emitir(aba, "seele://event", MENCAO(90));
    v = await ver(aba);
    confere(v.guardado === "nao" && v.mostradas.length === 3, `desligada de novo: guardado ${v.guardado}, ${v.mostradas.length} mostradas`);

    // 8. Nada estourou, e a ponte não recusou nenhum dos três.
    confere(v.contas.length === 0, `a ponte recusou: ${JSON.stringify(v.contas)}`);
    for (const estouro of estouros) falhas.push(`${casca}: a página estourou: ${estouro}`);
  } finally {
    await contexto.close();
  }
  return falhas;
}

async function aRecusa(navegador, url, casca) {
  const falhas = [];
  const confere = (ok, frase) => { if (!ok) falhas.push(`${casca}, negada: ${frase}`); };
  const { contexto, aba, estouros } = await abrir(navegador, url, casca, { resposta: "denied", semApi: false });
  try {
    await abrirAOpcao(aba);
    await aba.locator("#notificacoes-ligadas").click();
    await aba.waitForFunction(() => document.getElementById("notificacoes-estado").dataset.permissao === "Negada");
    let v = await ver(aba);
    confere(v.pedidos.length === 1 && v.pedidos[0].noGesto === true, `a pergunta: ${JSON.stringify(v.pedidos)}`);
    confere(v.dito.startsWith("O NAVEGADOR NÃO DEIXA ESTA PÁGINA NOTIFICAR") && v.dito.includes("permissões do navegador"), `a recusa não ficou dita com o caminho: ${v.dito}`);
    confere(v.aVista && v.estado.altura > 0, "a frase da recusa não está à vista");
    confere(v.mostradas.length === 0, "negada, a confirmação apareceu");
    confere(!v.rolaDeLado && v.estado.direita <= v.larguraDaJanela, `a frase da recusa sai da janela: ${JSON.stringify(v.estado)} em ${v.larguraDaJanela}`);
    if (capturas) await capturar(aba, `notificacoes-${casca}-negadas.png`);
    await porNaFrente(aba, false);
    await emitir(aba, "seele://event", MENCAO(5));
    v = await ver(aba);
    confere(v.mostradas.length === 0 && v.contasDaRegra.porque["sem-permissao"] === 1, `negada, a menção: ${v.mostradas.length} mostradas, ${JSON.stringify(v.contasDaRegra)}`);
    confere(v.pedidos.length === 1, "a menção pediu a permissão de novo, sem gesto");
    for (const estouro of estouros) falhas.push(`${casca}, negada: a página estourou: ${estouro}`);
  } finally {
    await contexto.close();
  }
  return falhas;
}

async function semAApi(navegador, url, casca) {
  const falhas = [];
  const confere = (ok, frase) => { if (!ok) falhas.push(`${casca}, sem a API: ${frase}`); };
  const { contexto, aba, estouros } = await abrir(navegador, url, casca, { resposta: "granted", semApi: true });
  try {
    await abrirAOpcao(aba);
    const v = await ver(aba);
    confere(v.parada === true && v.marcada === false, `a opção ficou ${v.marcada ? "ligada" : "desligada"} e ${v.parada ? "parada" : "clicável"}`);
    confere(v.dito.startsWith("Este navegador não tem notificações para esta página"), `a tela não diz que não há: ${v.dito}`);
    confere(v.permissao === "NaoHa", `a permissão no nó: ${v.permissao}`);
    confere(!v.rolaDeLado && v.estado.direita <= v.larguraDaJanela, `a frase sai da janela: ${JSON.stringify(v.estado)} em ${v.larguraDaJanela}`);
    if (capturas) await capturar(aba, `notificacoes-${casca}-nao-ha.png`);
    for (const estouro of estouros) falhas.push(`${casca}, sem a API: a página estourou: ${estouro}`);
  } finally {
    await contexto.close();
  }
  return falhas;
}

(async () => {
  if (capturas) fs.mkdirSync(capturas, { recursive: true });
  const servidor = servir();
  await new Promise((pronto) => servidor.listen(0, "127.0.0.1", pronto));
  const url = `http://127.0.0.1:${servidor.address().port}/index.html`;
  const navegador = await chromium().launch();
  const falhas = [];
  try {
    for (const casca of Object.keys(CASCAS)) {
      for (const caso of [oCaminhoInteiro, aRecusa, semAApi]) {
        try {
          falhas.push(...(await caso(navegador, url, casca)));
        } catch (erro) {
          falhas.push(`${casca}, ${caso.name}: lançou ${erro?.stack ?? erro}`);
        }
      }
    }
  } finally {
    await navegador.close();
    servidor.close();
    fs.rmSync(pasta, { recursive: true, force: true });
  }
  terminou = true;
  if (falhas.length === 0) {
    console.log("notificação do sistema: os três casos passam nas duas cascas");
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
