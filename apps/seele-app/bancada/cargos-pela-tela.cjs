// **Cargos pela TELA, contra o backend REAL** (E6 da 0.17, cargos-ui).
//
//   node apps/seele-app/bancada/cargos-pela-tela.cjs <seele_web.wasm> <servidor_da_bancada> [--webkit] [--json saída] [--reverter nome[,nome]] [--servidor-local]
//
// `--servidor-local` sobe os servidores no modo local (`--local` do
// `servidor_da_bancada`: só loopback, sem escada). Sem ele, o servidor anuncia o
// endereço da rede da máquina e sobe a escada (UPnP/PCP) — não use fora de uma
// máquina de bancada.
//
// A E2E da camada de cargos: a página de produto inteira (`ui/`, com a ponte web,
// a identidade e o `.wasm` real da sessão) num navegador de verdade, contra dois
// `servidor_da_bancada` reais com o backend de cargos do F. Nada de mentira no
// meio: a camada (`cargos.js`) fala com a ponte web, que fala com o `.wasm` da
// sessão, que fala com o servidor pelo enlace da 0.17.
//
// Quatro pessoas, quatro contextos do navegador, cada uma com a identidade
// fixada pela bancada (`identidade-da-bancada.cjs`): a DONA (declarada por
// `--proprietario` em S), a GI e o TEO (que vão gerir) e o RUI (abaixo deles).
// T é um segundo servidor, sem dono.
//
// A entrada é a do COLAR UM LINK: `identidade.abrir(link)`, `entrar(apelido)` e
// `depoisDeConectar` — o mesmo caminho do botão ENTRAR (celular-entrada.js). Daí
// em diante, TUDO pela tela: a camada aberta por `abrirCargos`, e cliques nos
// botões, abas, rádios e caixas dela.
//
// O que se prova, cada caso pelo servidor real:
//  1. entrar: as três pessoas na sessão de S, a dona é a dona;
//  2. criar e editar: a dona cria «Gestão» (GERIR CARGOS e EXPULSAR), «Guarda»
//     (concede BANIR) e «Quarentena» (restringe BANIR), pela tela, e o servidor
//     os tem;
//  3. ordenar: SUBIR a Quarentena muda a ordem no servidor;
//  4. atribuir: a dona dá a Gestão à Gi e ao Teo, e Guarda e Quarentena ao Rui,
//     com a prévia antes e o CONFIRMAR; a Gi passa a gerir cargos NA TELA DELA;
//  5. revogar: a dona tira um cargo (a Guarda da Gi, dada e tirada) pela tela;
//  6. a recusa por ganho efetivo: a Gi (sem BANIR) tenta tirar a restrição da
//     Quarentena — a prévia do servidor diz que isso daria BANIR ao Rui, e SALVAR
//     fica desligado; o mesmo ao tirar a Quarentena do Rui;
//  7. a revisão velha entre dois gestores: a Gi edita a Guarda (do Rui, abaixo
//     dos dois); o Teo grava antes;
//     o aviso do servidor deixa o rascunho da Gi desatualizado, com o que mudou
//     destacado; REFAZER e SALVAR gravam as duas mudanças;
//  8. o isolamento entre sessões: a dona entra também em T; os cargos de S somem
//     na vista de T, os de T são os de T (sem dono declarado, ela não gere), e de
//     volta a S a camada de S volta como estava.
//
// `--reverter`: aplica, ao `cargos.js` servido, reversões da `cargos.cjs`
// (o conserto desfeito em memória; a árvore não muda), e a bancada sai com 0 só
// se algum caso reprovar.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

"use strict";

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");
const { spawn } = require("node:child_process");
const { chromium, webkit } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const { fixarAIdentidade, impressaoDaPagina } = require("./identidade-da-bancada.cjs");
const { REVERSOES } = require("./cargos.cjs");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const posicionais = argumentos.filter((a, i) => !a.startsWith("--") && !["--json", "--reverter", "--experimento"].includes(argumentos[i - 1]));
const [caminhoDoWasm, caminhoDoServidor] = posicionais;
const WEBKIT = argumentos.includes("--webkit");
const saidaJson = opcao("--json");
const REVERTER = (opcao("--reverter") ?? "").split(",").filter(Boolean);
const SERVIDOR_LOCAL = argumentos.includes("--servidor-local");
// `--experimento nome`: uma mutação de MEDIDA no `cargos.js` servido (em
// memória, a árvore não muda), com a saída normal — não é guarda, é a prova de
// uma causa. `sem-releitura-pela-marca` tira as duas releituras que a marca dos
// cargos dispara (o `CargosChanged` e o `cargos_revision` do retrato): se o
// alerta de ritmo some sem elas, o laço «leitura → PedirCargos → resposta →
// marca anda → leitura» é a causa (medido em 25/10, LEIA.md).
const EXPERIMENTOS = {
  "sem-releitura-pela-marca": [
    ["      estado.marcaVista = marca;\n      cargosRelerSeMudou(estado);", "      estado.marcaVista = marca;"],
    ["  cargosRelerSeMudou(estado);\n}\n\n/** Fecha e devolve", "}\n\n/** Fecha e devolve"],
  ],
};
const EXPERIMENTO = opcao("--experimento");
if (EXPERIMENTO && !EXPERIMENTOS[EXPERIMENTO]) {
  console.error(`experimento desconhecido: ${EXPERIMENTO} (${Object.keys(EXPERIMENTOS).join(", ")})`);
  process.exit(2);
}
if (!caminhoDoWasm || !caminhoDoServidor || !fs.existsSync(caminhoDoWasm) || !fs.existsSync(caminhoDoServidor)) {
  console.error("uso: node cargos-pela-tela.cjs <seele_web.wasm> <servidor_da_bancada> [--webkit] [--json saída] [--reverter nome[,nome]]");
  process.exit(2);
}
for (const nome of REVERTER) {
  if (!REVERSOES[nome]) {
    console.error(`reversão desconhecida: ${nome} (as de cargos.cjs: ${Object.keys(REVERSOES).join(", ")})`);
    process.exit(2);
  }
}

const UI = path.resolve(__dirname, "../ui");
const TIPOS = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm", ".svg": "image/svg+xml", ".png": "image/png", ".json": "application/json", ".woff2": "font/woff2",
};

/** A `ui/` inteira com o `.wasm`, com os cabeçalhos da origem, e as reversões pedidas no `cargos.js`. */
function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "cargos-pela-tela-"));
  fs.cpSync(UI, pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  if (EXPERIMENTO) {
    const arquivo = path.join(pasta, "cargos.js");
    let texto = fs.readFileSync(arquivo, "utf8");
    for (const [de, para] of EXPERIMENTOS[EXPERIMENTO]) {
      if (texto.split(de).length !== 2) throw new Error(`o experimento ${EXPERIMENTO} não casa com o cargos.js`);
      texto = texto.replace(de, para);
    }
    fs.writeFileSync(arquivo, texto);
  }
  if (REVERTER.length > 0) {
    const arquivo = path.join(pasta, "cargos.js");
    let texto = fs.readFileSync(arquivo, "utf8");
    for (const nome of REVERTER) {
      const r = REVERSOES[nome];
      for (const [de, para] of r.trocas ?? [[r.de, r.para]]) {
        if (texto.split(de).length !== 2) throw new Error(`a reversão ${nome} não casa com o cargos.js`);
        texto = texto.replace(de, para);
      }
    }
    fs.writeFileSync(arquivo, texto);
  }
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
}

/** Um servidor real, e a primeira linha que ele diz (o link, a impressão, o dono). */
function subirOServidor(proprietario) {
  // `--servidor-local`: o modo local do servidor (só loopback, sem escada — sem
  // UPnP nem PCP no roteador de quem roda), pedido ao E no `servidor_da_bancada`.
  const args = [...(SERVIDOR_LOCAL ? ["--local"] : []), ...(proprietario ? ["--proprietario", proprietario] : [])];
  const processo = spawn(caminhoDoServidor, args, { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

/** Uma pessoa: um contexto com a identidade fixa, a página carregada. */
async function umaPessoa(navegador, endereco, nome) {
  const contexto = await navegador.newContext({ viewport: { width: 1920, height: 1080 } });
  await fixarAIdentidade(contexto);
  const aba = await contexto.newPage();
  const erros = [];
  aba.on("pageerror", (e) => erros.push(e.message));
  aba.on("console", (m) => {
    if (m.type() === "error") erros.push(`console: ${m.text()}`);
  });
  aba.setDefaultTimeout(15000);
  await aba.goto(`${endereco}index.html`);
  return { nome, contexto, aba, erros };
}

/** Entra pelo link, como o ENTRAR: a identidade e o `depoisDeConectar` da tela. */
async function entrar(pessoa, link, apelido) {
  const r = await pessoa.aba.evaluate(
    async ({ link, apelido }) => {
      const identidade = await identidadeDaPagina();
      const lido = await identidade.abrir(link);
      const entrada = await identidade.entrar(apelido);
      if (!entrada.ok) return { erro: entrada };
      depoisDeConectar(entrada.resposta, lido.endereco);
      return { ok: true };
    },
    { link, apelido },
  );
  if (!r.ok) throw new Error(`${pessoa.nome} não entrou: ${JSON.stringify(r.erro)}`);
  pessoa.ultimaEntrada = r;
  // A tela da chegada (`tela-auth`): o primeiro contato conferido, e ENTRAR,
  // que é o botão que a pessoa aperta.
  await pessoa.aba.waitForFunction(() => !$("tela-auth").hidden || !$("tela-sessao").hidden);
  if (await pessoa.aba.evaluate(() => !$("tela-auth").hidden)) {
    try {
      await pessoa.aba.waitForFunction(() => !$("auth-botao").disabled && !$("auth-botao").hidden);
      await pessoa.aba.click("#auth-botao", { timeout: 8000 });
    } catch (falha) {
      const onde = await pessoa.aba.evaluate(() => ({
        telas: [...document.querySelectorAll("section.tela")].filter((t) => !t.hidden).map((t) => t.id),
        camadaDeCargos: !$("cargos").hidden,
        botao: { desligado: $("auth-botao").disabled, escondido: $("auth-botao").hidden, texto: $("auth-botao").textContent.trim() },
        porCima: (() => {
          const r = $("auth-botao").getBoundingClientRect();
          const el = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
          return el ? `${el.tagName}#${el.id}.${el.className}` : null;
        })(),
      }));
      throw new Error(`${pessoa.nome}: o ENTRAR da chegada não pôde ser apertado: ${JSON.stringify(onde)} (${String(falha.message).split("\n")[0]})`);
    }
  }
  try {
    await pessoa.aba.waitForFunction(() => !$("tela-sessao").hidden && vistaDaJanela.sessao !== null && desenhado !== null);
  } catch {
    const onde = await pessoa.aba.evaluate(() => ({
      telas: [...document.querySelectorAll("section.tela")].filter((t) => !t.hidden).map((t) => t.id),
      vista: vistaDaJanela.sessao,
      retrato: desenhado !== null,
      casca: document.documentElement.dataset.casca ?? null,
    }));
    throw new Error(`${pessoa.nome} entrou e a tela não chegou à sessão: ${JSON.stringify(onde)} — ${pessoa.erros.slice(-3).join(" | ")}`);
  }
  return pessoa.aba.evaluate(() => vistaDaJanela.sessao);
}

// ------------------------------------------------------------- pela tela

const radio = (nome, estado) => `#cargos-permissoes fieldset.cargos-permissao:has(> legend:text-is("${nome}")) input[value="${estado}"]`;
const botaoDoCargo = (nome) => `#cargos-lista .cargos-item-botao:has(.cargos-item-nome:text-is("${nome}"))`;
const nomesDaLista = (aba) => aba.$$eval("#cargos-lista .cargos-item-nome", (es) => es.map((e) => e.textContent));
const texto = (aba, seletor) => aba.$eval(seletor, (e) => e.textContent.trim());

async function abrirACamada(aba) {
  await aba.evaluate(() => abrirCargos({}));
  await aba.waitForFunction(() => !$("cargos").hidden && document.querySelectorAll("#cargos-lista .cargos-item-nome").length > 0);
}

/** Espera o rascunho gravado: o aviso de rascunho some e SALVAR desliga. */
async function esperarGravado(aba) {
  await aba.waitForFunction(() => $("cargos-rascunho").hidden && $("cargos-estado").textContent === "");
  const erro = await aba.$eval("#cargos-form-erro", (e) => (e.hidden ? null : e.textContent));
  if (erro) throw new Error(`o servidor recusou: ${erro}`);
}

/** Cria um cargo pela tela: NOVO CARGO, o nome, as permissões, SALVAR. */
async function criarCargo(aba, nome, permissoes) {
  await aba.click("#cargos-aba-cargos");
  await aba.click("#cargos-novo");
  try {
    await aba.waitForFunction(() => document.activeElement?.id === "cargos-nome", null, { timeout: 15000 });
  } catch (falha) {
    const onde = await aba.evaluate(() => ({
      foco: document.activeElement ? `${document.activeElement.tagName}#${document.activeElement.id}` : null,
      novoDesligado: $("cargos-novo").disabled,
      novoMotivo: $("cargos-novo").title || $("cargos-novo").getAttribute("aria-describedby"),
      formEscondido: $("cargos-form")?.hidden,
      camadaEscondida: $("cargos").hidden,
      sessao: cargosNaTela?.sessao, vista: vistaDaJanela.sessao,
      eu: cargosNaTela?.eu ?? null, gere: cargosNaTela?.gere ?? null, dono: cargosNaTela?.donoDeclarado ?? null,
      erro: $("cargos-form-erro")?.hidden ? null : $("cargos-form-erro")?.textContent,
      erroDaLista: $("cargos-erro")?.hidden ? null : $("cargos-erro")?.textContent,
      lista: [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].map((e) => e.textContent),
      cargoAberto: cargosNaTela?.cargo ?? null, detalhe: cargosNaTela?.detalhe ?? null, mandando: cargosNaTela?.mandando ?? null,
      revisao: cargosNaTela?.quadro?.revisao ?? null,
      estado: $("cargos-estado")?.textContent,
    }));
    throw new Error(`NOVO CARGO não abriu o formulário: ${JSON.stringify(onde)} (${String(falha.message).split("\n")[0]})`);
  }
  await aba.fill("#cargos-nome", nome);
  for (const [permissao, estado] of permissoes) await aba.check(radio(permissao, estado));
  await aba.waitForFunction(() => !$("cargos-salvar").disabled);
  await aba.click("#cargos-salvar");
  await esperarGravado(aba);
  await aba.waitForFunction((n) => [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].some((e) => e.textContent === n), nome);
}

/** Dá ou tira um cargo pela aba PESSOAS: a caixa, a prévia, CONFIRMAR. */
async function darOuTirar(aba, apelido, cargo, dar) {
  await aba.click("#cargos-aba-pessoas");
  await aba.click(`#cargos-pessoas .cargos-item-botao:has(.cargos-item-nome:text-is("${apelido}"))`);
  await aba.waitForSelector("#cargos-pessoa:not([hidden])");
  const caixa = `#cargos-da-pessoa label:has(.cargos-atribuicao-nome:text-matches(" ${cargo}$")) input`;
  if (dar) await aba.check(caixa);
  else await aba.uncheck(caixa);
  await aba.waitForFunction(() => !$("cargos-pendente").hidden && !/calculando/.test($("cargos-pendente-lista").textContent));
  const previa = await texto(aba, "#cargos-pendente-lista");
  const podeConfirmar = await aba.$eval("#cargos-pendente-confirmar", (b) => !b.disabled);
  return { previa, podeConfirmar, caixa };
}

async function confirmar(aba, caixa, dar) {
  await aba.click("#cargos-pendente-confirmar");
  await aba.waitForFunction(() => $("cargos-pendente").hidden && $("cargos-estado").textContent === "");
  const erro = await aba.$eval("#cargos-pessoa-erro", (e) => (e.hidden ? null : e.textContent));
  if (erro) throw new Error(`o servidor recusou: ${erro}`);
  const marcada = await aba.$eval(caixa, (c) => c.checked);
  if (marcada !== dar) throw new Error(`a caixa não ficou ${dar ? "marcada" : "desmarcada"} depois do CONFIRMAR`);
}

/**
 * A medida do ritmo: cada `invoke` da página (tempo, comando, sessão do
 * carimbo) e cada vez que o `#banner` aparece (tempo, texto). Só lê: o
 * envoltório repassa tudo ao `invoke` do base.js como veio.
 */
async function instrumentar(aba) {
  await aba.evaluate(() => {
    if (window.__ritmo) return;
    const ritmo = { inicio: performance.now(), pedidos: [], banners: [] };
    window.__ritmo = ritmo;
    const original = window.invoke;
    window.invoke = function (comando, args) {
      ritmo.pedidos.push([Math.round(performance.now() - ritmo.inicio), comando, args?.carimbo?.sessao ?? vistaDaJanela.sessao]);
      return original.apply(this, arguments);
    };
    new MutationObserver(() => {
      const b = $("banner");
      const visto = !b.hidden ? b.textContent.trim().replace(/\s+/g, " ").slice(0, 160) : null;
      const ultimo = ritmo.banners.at(-1);
      if ((ultimo?.[1] ?? null) !== visto) ritmo.banners.push([Math.round(performance.now() - ritmo.inicio), visto]);
    }).observe($("banner"), { attributes: true, childList: true, subtree: true, characterData: true });
  });
}

/** O resumo do ritmo: pedidos por comando, o pior segundo, e os banners. */
async function lerORitmo(aba) {
  return aba.evaluate(() => {
    const r = window.__ritmo;
    if (!r) return null;
    const porComando = {};
    const porSegundo = {};
    for (const [t, c, s] of r.pedidos) {
      porComando[`${c}@${s}`] = (porComando[`${c}@${s}`] ?? 0) + 1;
      const seg = Math.floor(t / 1000);
      porSegundo[seg] = porSegundo[seg] ?? {};
      porSegundo[seg][c] = (porSegundo[seg][c] ?? 0) + 1;
    }
    const segundos = Object.entries(porSegundo).map(([s, cs]) => [Number(s), Object.values(cs).reduce((a, b) => a + b, 0), cs]);
    segundos.sort((a, b) => b[1] - a[1]);
    return { total: r.pedidos.length, duracaoMs: Math.round(performance.now() - r.inicio), porComando, pioresSegundos: segundos.slice(0, 6), banners: r.banners };
  });
}

/** Clica, e se não der diz por quê: se o alvo existe, se está visível e o que está por cima. */
async function clicarOuDizer(aba, seletor) {
  try {
    await aba.click(seletor, { timeout: 8000 });
  } catch (falha) {
    const onde = await aba.evaluate((s) => {
      const el = document.querySelector(s);
      if (!el) {
        return { existe: false, placas: [...document.querySelectorAll("#trilha-sessoes button")].map((b) => `${b.dataset.sessao}:${b.getAttribute("aria-current")}`), trilhaEscondida: document.getElementById("trilha-sessoes")?.hidden };
      }
      const r = el.getBoundingClientRect();
      const cima = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
      let ancestralEscondido = null;
      for (let n = el; n; n = n.parentElement) if (n.hidden) { ancestralEscondido = n.id || n.className; break; }
      return { existe: true, caixa: [r.x, r.y, r.width, r.height], ancestralEscondido, porCima: cima ? `${cima.tagName}#${cima.id}.${cima.className}` : null, textoPorCima: cima?.closest("[id]")?.textContent?.trim().replace(/\s+/g, " ").slice(0, 300) ?? null, caixaPorCima: cima ? (({ x, y, width, height }) => [x, y, width, height])(cima.getBoundingClientRect()) : null, casca: document.documentElement.dataset.casca ?? null };
    }, seletor);
    throw new Error(`não deu para clicar em ${seletor}: ${JSON.stringify(onde)} (${String(falha.message).split("\n")[0]})`);
  }
}

/** O que a sessão desta página sabe do quadro, pelo `pedir_cargos` (para conferir o servidor). */
const quadroDaSessao = (aba) =>
  aba.evaluate(async () => {
    const r = await invoke("pedir_cargos", {});
    return r?.quadro ?? r;
  });

// ----------------------------------------------- prazo e limpeza garantida

/** O prazo de cada caso: passou disso, o caso reprova com o nome dele. */
const PRAZO_DO_CASO = 180000;

const esperar = (ms) => new Promise((ok) => setTimeout(ok, ms));

/**
 * Encerra um servidor pelo PID EXATO dele e espera o status de saída: fecha a
 * entrada (o contrato do `servidor_da_bancada`), espera o `exit`; se não sair,
 * SIGTERM e depois SIGKILL, sempre no mesmo processo. Nunca procura por nome.
 */
async function encerrarOServidor(s) {
  const p = s.processo;
  if (p.exitCode !== null || p.signalCode !== null) return { pid: p.pid, codigo: p.exitCode, sinal: p.signalCode, como: "já tinha saído" };
  const saiu = new Promise((ok) => p.once("exit", (codigo, sinal) => ok({ codigo, sinal })));
  p.stdin.end();
  let r = await Promise.race([saiu, esperar(10000).then(() => null)]);
  let como = "fechou a entrada";
  if (!r) {
    p.kill("SIGTERM");
    como = "SIGTERM";
    r = await Promise.race([saiu, esperar(5000).then(() => null)]);
  }
  if (!r) {
    p.kill("SIGKILL");
    como = "SIGKILL";
    r = await saiu;
  }
  return { pid: p.pid, ...r, como };
}

// ------------------------------------------------------------- o roteiro

(async () => {
  const servidorDaPagina = servirAPagina();
  await new Promise((ok) => servidorDaPagina.listen(0, "127.0.0.1", ok));
  const endereco = `http://127.0.0.1:${servidorDaPagina.address().port}/`;
  const navegador = WEBKIT ? await webkit().launch() : await chromium().launch();
  const servidores = [];
  const pessoas = [];
  const R = { navegador: `${WEBKIT ? "webkit" : "chromium"} ${navegador.version()}`, reversoes: REVERTER, experimento: EXPERIMENTO, casos: {} };
  let caso = "0";
  const passou = (nome, dados = {}) => {
    R.casos[nome] = { ok: true, ...dados };
    console.log(`PASSA ${nome}`);
  };
  // O vigia do prazo: cada caso tem PRAZO_DO_CASO desde que começou.
  let casoVisto = caso;
  let inicioDoCaso = Date.now();
  let vigiaDoPrazo = null;
  const prazo = new Promise((_, rejeitar) => {
    vigiaDoPrazo = setInterval(() => {
      if (caso !== casoVisto) {
        casoVisto = caso;
        inicioDoCaso = Date.now();
      } else if (Date.now() - inicioDoCaso > PRAZO_DO_CASO) {
        rejeitar(new Error(`prazo de ${PRAZO_DO_CASO / 1000} s esgotado no caso ${caso}`));
      }
    }, 1000);
  });
  try {
    await Promise.race([prazo, (async () => {
    // ------------------------------------------------------------ 1. entrar
    caso = "1-entrar";
    const dona = await umaPessoa(navegador, endereco, "dona");
    const gi = await umaPessoa(navegador, endereco, "gi");
    const teo = await umaPessoa(navegador, endereco, "teo");
    // O Rui fica ABAIXO das duas pessoas que gerem: é nele que um gestor pode
    // mexer (cargos-decisoes §3: a pessoa atingida está abaixo de quem pede).
    const rui = await umaPessoa(navegador, endereco, "rui");
    pessoas.push(dona, gi, teo, rui);
    const impressao = await impressaoDaPagina(dona.aba);
    const S = await subirOServidor(impressao);
    const T = await subirOServidor(null);
    servidores.push(S, T);
    if (S.proprietario !== impressao) throw new Error(`S não declarou a dona: ${S.proprietario} ≠ ${impressao}`);
    await instrumentar(dona.aba);
    await instrumentar(gi.aba);
    const sessaoDaDonaEmS = await entrar(dona, S.link, "dona");
    await entrar(gi, S.link, "gi");
    await entrar(teo, S.link, "teo");
    await entrar(rui, S.link, "rui");
    await abrirACamada(dona.aba);
    const eDona = await dona.aba.evaluate(() => cargosNaTela?.quadro?.eu?.proprietario === true);
    if (!eDona) throw new Error("a camada da dona não a vê como proprietária");
    passou(caso, { impressao, listaInicial: await nomesDaLista(dona.aba) });

    // -------------------------------------------------- 2. criar e editar
    caso = "2-criar-e-editar";
    await criarCargo(dona.aba, "Gestão", [["GERIR CARGOS", "concede"], ["EXPULSAR", "concede"]]);
    await criarCargo(dona.aba, "Guarda", [["BANIR", "concede"]]);
    await criarCargo(dona.aba, "Quarentena", [["BANIR", "restringe"]]);
    const doServidor = await quadroDaSessao(gi.aba);
    const gestao = doServidor.cargos.find((c) => c.nome === "Gestão");
    const quarentena = doServidor.cargos.find((c) => c.nome === "Quarentena");
    if (!gestao || !gestao.concede.some((p) => p.Produto === "ManageRoles")) throw new Error(`a Gestão não chegou ao servidor como foi feita: ${JSON.stringify(gestao)}`);
    if (!quarentena || !quarentena.restringe.some((p) => p.Produto === "Ban")) throw new Error(`a Quarentena não restringe BANIR no servidor: ${JSON.stringify(quarentena)}`);
    passou(caso, { lista: await nomesDaLista(dona.aba) });

    // ------------------------------------------------------------ 3. ordenar
    // A Gestão nasce abaixo do cargo de chegada (Person, que toda conta nova
    // tem): para quem a tiver ficar ACIMA de quem só chegou, a dona a sobe,
    // pela tela, até acima do Person — que é o que se faz no produto.
    caso = "3-ordenar";
    const antes = await nomesDaLista(dona.aba);
    const posDe = (nomes, n) => nomes.indexOf(n);
    let subidas = 0;
    while (posDe(await nomesDaLista(dona.aba), "Gestão") > posDe(await nomesDaLista(dona.aba), "Person")) {
      const agora = posDe(await nomesDaLista(dona.aba), "Gestão");
      await dona.aba.click(`[data-subir="${gestao.id}"]`);
      await dona.aba.waitForFunction(
        ([n, j]) => [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].findIndex((e) => e.textContent === n) === j,
        ["Gestão", agora - 1],
      );
      subidas += 1;
      if (subidas > 6) throw new Error("a Gestão não sobe acima do Person");
    }
    const ordemNoServidor = (await quadroDaSessao(gi.aba)).cargos.map((c) => c.nome);
    if (posDe(ordemNoServidor, "Gestão") > posDe(ordemNoServidor, "Person")) throw new Error(`o servidor não tem a ordem nova: ${ordemNoServidor.join(", ")}`);
    passou(caso, { antes, depois: ordemNoServidor, subidas });

    // ------------------------------------------------------------ 4. atribuir
    caso = "4-atribuir";
    const deGi = await darOuTirar(dona.aba, "gi", "Gestão", true);
    if (!/gi PASSA A PODER: .*GERIR CARGOS/.test(deGi.previa)) throw new Error(`a prévia não diz o que a Gi ganha: ${deGi.previa}`);
    await confirmar(dona.aba, deGi.caixa, true);
    const deTeo = await darOuTirar(dona.aba, "teo", "Gestão", true);
    await confirmar(dona.aba, deTeo.caixa, true);
    for (const cargo of ["Guarda", "Quarentena"]) {
      const d = await darOuTirar(dona.aba, "rui", cargo, true);
      await confirmar(dona.aba, d.caixa, true);
    }
    // A Gi passa a gerir cargos NA TELA DELA.
    await abrirACamada(gi.aba);
    await gi.aba.waitForFunction(() => !$("cargos-novo").disabled);
    passou(caso, { previaDaGi: deGi.previa });

    // ------------------------------------------------------------ 5. revogar
    caso = "5-revogar";
    const darGuarda = await darOuTirar(dona.aba, "gi", "Guarda", true);
    await confirmar(dona.aba, darGuarda.caixa, true);
    const tirarGuarda = await darOuTirar(dona.aba, "gi", "Guarda", false);
    if (!/gi DEIXA DE PODER: .*BANIR/.test(tirarGuarda.previa)) throw new Error(`a prévia de tirar não diz o que a Gi perde: ${tirarGuarda.previa}`);
    await confirmar(dona.aba, tirarGuarda.caixa, false);
    const idDaGi = await gi.aba.evaluate(() => desenhado.me);
    const idDaGuarda = doServidor.cargos.find((c) => c.nome === "Guarda").id;
    const daGi = (await quadroDaSessao(gi.aba)).atribuicoes.find((a) => a.pessoa === idDaGi);
    if (!daGi || !daGi.cargos.includes(gestao.id) || daGi.cargos.includes(idDaGuarda)) {
      throw new Error(`no servidor, a Gi não está com a Gestão e sem a Guarda: ${JSON.stringify(daGi)}`);
    }
    passou(caso, { previa: tirarGuarda.previa });

    // ---------------------------------------- 6. a recusa por ganho efetivo
    caso = "6-recusa-por-ganho-efetivo";
    await gi.aba.click("#cargos-aba-cargos");
    await gi.aba.click(botaoDoCargo("Quarentena"));
    await gi.aba.waitForSelector("#cargos-form:not([hidden])");
    await gi.aba.check(radio("BANIR", "neutro"));
    await gi.aba.waitForFunction(() => /vai recusar/.test($("cargos-previa-lista").textContent), null, { timeout: 8000 });
    const previaRecusada = await texto(gi.aba, "#cargos-previa-lista");
    if (!/isto daria a rui BANIR, que você não tem/.test(previaRecusada)) throw new Error(`a prévia não nomeia o ganho: ${previaRecusada}`);
    if (!(await gi.aba.$eval("#cargos-salvar", (b) => b.disabled))) throw new Error("SALVAR ficou apertável com a prévia do servidor recusando");
    await gi.aba.click("#cargos-descartar");
    const tirarQuarentena = await darOuTirar(gi.aba, "rui", "Quarentena", false);
    if (!/isto daria a rui BANIR, que você não tem/.test(tirarQuarentena.previa) || tirarQuarentena.podeConfirmar) {
      throw new Error(`tirar a Quarentena do Rui não foi antecipado como recusa: ${tirarQuarentena.previa}`);
    }
    await gi.aba.click("#cargos-pendente-cancelar");
    passou(caso, { previa: previaRecusada, pendente: tirarQuarentena.previa });

    // ------------------------- 7. a revisão velha entre dois gestores
    caso = "7-revisao-velha-entre-gestores";
    await abrirACamada(teo.aba);
    await gi.aba.click("#cargos-aba-cargos");
    await gi.aba.click(botaoDoCargo("Guarda"));
    await gi.aba.waitForSelector("#cargos-form:not([hidden])");
    await gi.aba.check(radio("ANEXAR ARQUIVOS", "restringe"));
    await gi.aba.waitForFunction(() => !/calculando/.test($("cargos-previa-lista").textContent));
    await teo.aba.click(botaoDoCargo("Guarda"));
    await teo.aba.waitForSelector("#cargos-form:not([hidden])");
    await teo.aba.check(radio("FALAR", "restringe"));
    await teo.aba.waitForFunction(() => !$("cargos-salvar").disabled);
    await teo.aba.click("#cargos-salvar");
    await esperarGravado(teo.aba);
    // Dois caminhos, os dois do produto: com o aviso do servidor na página
    // (`CargosChanged`), o rascunho da Gi fica desatualizado sem clique; sem
    // ele, a Gi aperta SALVAR e o servidor recusa por revisão velha. Nos dois,
    // a camada destaca o que mudou e não reaplica nada sozinha.
    const comoAGiEstava = () =>
      gi.aba.evaluate(() => ({
        rascunho: $("cargos-rascunho").hidden ? null : $("cargos-rascunho").textContent,
        salvar: $("cargos-salvar").disabled ? "desligado" : "ligado",
        titulo: $("cargos-editor-titulo").textContent,
        anexar: [...document.querySelectorAll('#cargos-permissoes fieldset.cargos-permissao')].find((f) => f.querySelector("legend").textContent === "ANEXAR ARQUIVOS")?.querySelector("input:checked")?.value ?? null,
        revisao: cargosNaTela?.quadro?.revisao ?? null,
        base: cargosNaTela?.rascunho?.base?.revisaoDoQuadro ?? null,
      }));
    R.casos.giLogoDepoisDoTeo = await comoAGiEstava();
    const pelo = await gi.aba
      .waitForSelector(".cargos-permissao-mudou", { timeout: 2000 })
      .then(() => "aviso")
      .catch(() => "salvar");
    if (pelo === "salvar") {
      await gi.aba.click("#cargos-salvar");
      await gi.aba.waitForSelector("#cargos-form-erro:not([hidden])");
      const recusa = await texto(gi.aba, "#cargos-form-erro");
      if (!/outro gestor mudou os cargos antes de você salvar/.test(recusa)) throw new Error(`a recusa da revisão velha não veio: ${recusa}`);
      await gi.aba.waitForSelector(".cargos-permissao-mudou");
    }
    R.casos.caminhoDaRevisao = pelo;
    const mudou = await gi.aba.$$eval(".cargos-permissao-mudou", (fs) => fs.map((f) => f.querySelector("legend").textContent));
    if (!mudou.includes("FALAR")) throw new Error(`o destaque não aponta o que o Teo mudou: ${mudou}`);
    if (!(await gi.aba.$eval("#cargos-salvar", (b) => b.disabled))) throw new Error("dá para salvar por cima sem refazer");
    await gi.aba.click("#cargos-refazer");
    await gi.aba.waitForFunction(() => !$("cargos-salvar").disabled);
    await gi.aba.click("#cargos-salvar");
    await esperarGravado(gi.aba);
    const guarda = (await quadroDaSessao(dona.aba)).cargos.find((c) => c.nome === "Guarda");
    const restringe = guarda.restringe.map((p) => p.Produto);
    if (!restringe.includes("Speak") || !restringe.includes("AttachFile")) throw new Error(`a Guarda não tem as duas mudanças: ${JSON.stringify(guarda)}`);
    passou(caso, { pelo, destacado: mudou, restringe });

    // -------------------------------------- 7b. dentro do ritmo do servidor
    // O servidor avisa quem passa de 20 quadros por segundo (`taxa.rs`), e o
    // aviso cobre a tela inteira. Uma camada que relê em laço não falha em
    // nenhum caso de cima: ela aparece aqui, com os números.
    caso = "7b-ritmo-dentro-do-limite";
    for (const p of [dona, gi]) {
      const ritmo = await lerORitmo(p.aba);
      const alertas = (ritmo?.banners ?? []).filter(([, t]) => t && /RÁPIDO DEMAIS/.test(t));
      if (alertas.length > 0) {
        const pedidos = Object.entries(ritmo.porComando).filter(([c]) => c.startsWith("pedir_cargos")).map(([c, n]) => `${c}=${n}`).join(", ");
        throw new Error(
          `o servidor avisou ${p.nome} de ritmo ${alertas.length} vez(es) (a primeira aos ${alertas[0][0]} ms); ` +
            `${ritmo.total} pedidos em ${ritmo.duracaoMs} ms, ${pedidos}; o pior segundo: ${JSON.stringify(ritmo.pioresSegundos[0])}`,
        );
      }
    }
    passou(caso);

    // ------------------------------------------ 8. o isolamento entre sessões
    caso = "8-isolamento-entre-sessoes";
    // A camada da dona em S, aberta; ela entra também em T.
    await abrirACamada(dona.aba);
    await dona.aba.waitForFunction(() => [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].some((e) => e.textContent === "Gestão"));
    // Entrar em T pelo fluxo normal (o link e o ENTRAR NO SERVIDOR), e afirmar
    // DIRETO que a vista da janela é T. Sem contorno: enquanto a entrada pela
    // web não trocar a vista (defeito pedido ao ui-ux e ao browser-signaling),
    // este caso REPROVA aqui — vermelho esperado, registrado no LEIA.
    await entrar(dona, T.link, "dona");
    const depoisDeT = await dona.aba.evaluate(async () => ({
      vista: vistaDaJanela.sessao,
      sessoes: (await invoke("sessoes", {})).map((s) => ({ sessao: s.carimbo?.sessao, endereco: s.endereco, na_vista: s.na_vista })),
    }));
    R.casos.vistaNaEntradaEmT = depoisDeT;
    const sessaoDaDonaEmT = depoisDeT.sessoes.find((s) => s.sessao !== sessaoDaDonaEmS)?.sessao ?? null;
    if (sessaoDaDonaEmT === null) throw new Error(`a entrada em T não abriu uma segunda sessão: ${JSON.stringify(depoisDeT)}`);
    if (depoisDeT.vista !== sessaoDaDonaEmT) {
      throw new Error(
        `DEFEITO DA CASCA WEB (vermelho esperado): depois de entrar em T, a vista da janela continua ${depoisDeT.vista}, ` +
          `e a ponte diz que a vista é ${sessaoDaDonaEmT} — ${JSON.stringify(depoisDeT.sessoes)}`,
      );
    }
    // A camada de S não aparece na vista de T.
    await dona.aba.waitForFunction(() => $("cargos").hidden || ![...document.querySelectorAll("#cargos-lista .cargos-item-nome")].some((e) => e.textContent === "Gestão"));
    await abrirACamada(dona.aba);
    const emT = await nomesDaLista(dona.aba);
    if (emT.includes("Gestão")) throw new Error(`os cargos de S aparecem em T: ${emT}`);
    const semDono = await texto(dona.aba, "#cargos-regra-proprietario");
    if (!/NÃO TEM PROPRIETÁRIO DECLARADO/.test(semDono)) throw new Error(`T não diz que não tem dono: ${semDono}`);
    if (!(await dona.aba.$eval("#cargos-novo", (b) => b.disabled))) throw new Error("em T, sem gerir cargos, NOVO CARGO ficou apertável");
    // De volta a S POR CLIQUE na placa de S da trilha (a camada de T cobre a
    // trilha: fecha-se a camada antes, como a pessoa faria). A camada de S volta
    // como estava, e é a de S — pela sessão do estado e pela lista dela.
    await dona.aba.click("#cargos-fechar");
    await clicarOuDizer(dona.aba, `#trilha-sessoes button[data-sessao="${sessaoDaDonaEmS}"]`);
    await dona.aba.waitForFunction((s) => vistaDaJanela.sessao === s && !$("cargos").hidden && cargosNaTela?.sessao === s, sessaoDaDonaEmS);
    const abaQueVoltou = await dona.aba.evaluate(() => cargosNaTela.aba);
    await dona.aba.click("#cargos-aba-cargos");
    await dona.aba.waitForFunction(() => [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].some((e) => e.textContent === "Gestão"));
    // E de novo para T, por clique: a camada de T estava fechada, e fica fechada;
    // nada de S aparece.
    await dona.aba.click("#cargos-fechar");
    await clicarOuDizer(dona.aba, `#trilha-sessoes button[data-sessao="${sessaoDaDonaEmT}"]`);
    await dona.aba.waitForFunction((t) => vistaDaJanela.sessao === t, sessaoDaDonaEmT);
    await dona.aba.waitForTimeout(500);
    if (await dona.aba.evaluate(() => !$("cargos").hidden)) throw new Error("a camada reapareceu em T, que estava fechada");
    passou(caso, { sessoes: [sessaoDaDonaEmS, sessaoDaDonaEmT], listaEmT: emT, abaQueVoltou });

    for (const p of pessoas) {
      const deCargos = p.erros.filter((e) => /cargo/i.test(e));
      if (deCargos.length > 0) throw new Error(`${p.nome}: erros de página da camada: ${deCargos.join(" | ")}`);
    }
    })()]);
    R.veredito = "todos os casos passam";
    R.ritmo = {};
    for (const p of pessoas.slice(0, 2)) R.ritmo[p.nome] = await lerORitmo(p.aba).catch((e) => String(e));
  } catch (falha) {
    const linha = String(falha.stack ?? "").split("\n").find((l) => l.includes("cargos-pela-tela.cjs"))?.trim() ?? "";
    R.falhou = { caso, motivo: `${String(falha.message ?? falha).split("\n")[0]} ${linha}` };
    console.log(`REPROVA ${caso}: ${R.falhou.motivo}`);
    R.errosDePagina = Object.fromEntries(pessoas.map((p) => [p.nome, p.erros.slice(-5)]));
    R.ritmo = {};
    for (const p of pessoas.slice(0, 2)) R.ritmo[p.nome] = await lerORitmo(p.aba).catch((e) => String(e));
    // O estado da camada de cada pessoa no instante da falha: onde a tela parou.
    R.camadas = {};
    for (const p of pessoas) {
      R.camadas[p.nome] = await p.aba
        .evaluate(() => ({
          aberta: !$("cargos").hidden,
          estado: $("cargos-estado").textContent,
          erro: $("cargos-erro").hidden ? null : $("cargos-erro").textContent,
          erroDoForm: $("cargos-form-erro").hidden ? null : $("cargos-form-erro").textContent,
          rascunho: $("cargos-rascunho").hidden ? null : $("cargos-rascunho").textContent,
          salvar: $("cargos-salvar").disabled ? "desligado" : "ligado",
          previa: $("cargos-previa").hidden ? null : $("cargos-previa-lista").textContent,
          revisaoLida: cargosNaTela?.quadro?.revisao ?? null,
          recusa: cargosNaTela?.quadro?.ultimaRecusa ?? null,
        }))
        .catch((e) => String(e));
    }
  } finally {
    clearInterval(vigiaDoPrazo);
    await Promise.race([navegador.close(), esperar(15000)]);
    servidorDaPagina.close();
    // Cada servidor pelo PID exato, com o status de saída no JSON.
    R.limpeza = [];
    for (const s of servidores) R.limpeza.push(await encerrarOServidor(s));
  }
  if (saidaJson) fs.writeFileSync(saidaJson, `${JSON.stringify(R, null, 2)}\n`);
  if (REVERTER.length > 0 && R.falhou?.caso === "1-entrar") {
    // Reprovar na entrada não prova a reversão: é a rede ou o servidor, antes
    // de a camada fazer qualquer coisa. Inconclusiva, e diz isso.
    console.log(`reversão ${REVERTER.join("+")}: INCONCLUSIVA — reprovou na entrada, antes da camada (${R.falhou.motivo})`);
    process.exit(2);
  }
  if (REVERTER.length > 0) {
    // Com o conserto desfeito, a E2E tem de reprovar.
    console.log(R.falhou ? `reversão ${REVERTER.join("+")}: REPROVA no caso ${R.falhou.caso}, como deve` : `reversão ${REVERTER.join("+")}: continua VERDE — o guarda não guarda`);
    process.exit(R.falhou ? 0 : 1);
  }
  process.exit(R.falhou ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
