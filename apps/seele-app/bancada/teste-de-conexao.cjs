// **O TESTAR A CONEXÃO e o «copiar diagnóstico».**
//
// A lacuna 8 da análise para a 1.0 (`docs/analise-para-a-1.0-2026-11-09.md`,
// seção 5): a página de diagnóstico de rede existia e nada na interface levava
// a ela. Quem não conectava por causa de rede lia uma frase e ficava sem
// caminho.
//
// A UI é a de verdade, servida como está. O que é dublê: no app, a ponte do
// Tauri (o `connect` que falha e o comando que abre o navegador); na página, só
// o transporte (o `conectar` da identidade, o enlace WebRTC do teste). A ponte
// do navegador, a identidade, as duas cascas e a página de diagnóstico rodam
// inteiras.
//
// O que ela prende:
//
//  A. **No app** (a casca larga com a ponte do Tauri):
//     1. uma falha de rede na entrada oferece TESTAR A CONEXÃO, à vista;
//     2. o aperto pede `abrir_o_teste_de_rede` com o NOME da falha e mais nada,
//        não navega esta janela, e a tela diz que abriu;
//     3. se o navegador não abre, a tela diz o endereço;
//     4. o que não é de rede (banimento, versão, senha) não oferece;
//     5. a oferta some quando a frase dela some;
//     6. uma ligação pelo código não oferece (não há convite para colar lá);
//     7. a tela de fim oferece para o enlace perdido, e não para uma expulsão.
//  B. **Na página, casca larga** (a ponte do navegador de verdade): a falha do
//     enlace tem a frase dela, e não «ALGO FALHOU»; o aperto abre uma aba da
//     mesma origem com a página de diagnóstico, que diz de que falha a pessoa
//     veio; a aba não fica com o `opener`.
//  C. **Na página, casca do celular**: o `sem-rede` da entrada oferece, o
//     aperto leva ao diagnóstico, e a aba que o navegador bloqueia é dita com o
//     endereço. Uma recusa que não é de rede não oferece.
//  D. **A página de diagnóstico**: um convite que não serve é dito e nada é
//     testado; depois da medida, o quadro mostra o texto que o botão copia; o
//     que chega à área de transferência é esse texto, com o resultado e a falha
//     vista, e sem endereço, porta, impressão, convite nem nome; a página diz o
//     que copiou; e a cópia que o navegador recusa é dita, com o texto
//     selecionado.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/teste-de-conexao.cjs
//
//   --reverter=<nome>   serve ao navegador o arquivo SEM um dos consertos
//                       (`REVERSOES`, abaixo): a bancada tem de reprovar. O
//                       arquivo em disco não é tocado.
//   --capturas=<pasta>  grava as telas em 1280 e 390 px.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { servir, respostas } = require("./telas.cjs");
const { origemDoProduto } = require("./origem-do-produto.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const chromium = require("./playwright.cjs").chromium();

const ui = path.resolve(__dirname, "../ui");
const argumento = (nome) => process.argv.find((a) => a.startsWith(`--${nome}=`))?.slice(nome.length + 3) ?? null;
const reverter = argumento("reverter");
const capturas = argumento("capturas");

/**
 * Cada conserto, e o que o navegador recebe no lugar dele quando revertido. O
 * `de` tem de existir no arquivo: uma reversão que não casa com nada passaria
 * por conserto provado.
 */
const REVERSOES = {
  // A entrada não oferece o teste.
  entrada: {
    arquivo: "tela-boot.js",
    de: "      oferecerOTesteNaEntrada(ligacao ? null : motivo);\n",
    para: "",
  },
  // A ligação pelo código oferece um teste que não tem o que medir.
  ligacao: {
    arquivo: "tela-boot.js",
    de: "oferecerOTesteNaEntrada(ligacao ? null : motivo);",
    para: "oferecerOTesteNaEntrada(motivo);",
  },
  // A oferta sobra debaixo de outra frase.
  "some-com-a-frase": {
    arquivo: "tela-boot.js",
    de: "if (linha.hidden || linha.textContent !== fraseDaOfertaNaEntrada) oferecerOTesteNaEntrada(null);",
    para: "",
  },
  // A tela de fim não oferece.
  fim: {
    arquivo: "tela-fim.js",
    de: '  oferecerOTesteDeConexao($("fim-teste-de-conexao"), motivo);\n',
    para: "",
  },
  // A entrada do celular não oferece.
  celular: {
    arquivo: "celular-entrada.js",
    de: 'return desfecho?.erro === "sem-rede" ? ofertaDoTesteDeConexao(desfecho.motivo) : null;',
    para: "return null;",
  },
  // Toda falha com nome oferece: a de versão, a de senha.
  lista: {
    arquivo: "frases.js",
    de: "return FALHAS_QUE_OFERECEM_O_TESTE_DE_CONEXAO.includes(nome) ? nome : null;",
    para: "return nome;",
  },
  // A falha do enlace da página volta a ser «ALGO FALHOU».
  "frase-do-enlace": {
    arquivo: "frases.js",
    de: 'if (typeof frase === "string") return frase;',
    para: "",
  },
  // O teste que não abre fica calado.
  "dizer-que-nao-abriu": {
    arquivo: "base.js",
    de: '        estado.dataset.tipo = "erro";\n        estado.textContent = typeof endereco === "string" && endereco !== ""\n          ? frases.naoAbriu[onde](endereco)\n          : frases.naoAbriu.semEndereco;\n',
    para: "",
  },
  // A ponte abre com `noopener`, e a aba bloqueada passa por aberta.
  ponte: {
    arquivo: "ponte-web.js",
    de: '    const aba = window.open(endereco, "_blank");\n    if (!aba) return Promise.reject({ NavegadorNaoAbriu: { endereco } });\n',
    para: '    const aba = window.open(endereco, "_blank", "noopener");\n',
  },
  // A página ignora a falha de quem chegou por uma tela de erro.
  "erro-na-pagina": {
    arquivo: "diagnostico-rede.js",
    de: "  const erroVisto = erroVistoNaConsulta(location.search);",
    para: "  const erroVisto = null;",
  },
  // O convite que não serve vira «a conexão não abriu», com um relatório de nada.
  "convite-dito": {
    arquivo: "diagnostico-rede.js",
    de: "      estado.textContent = `${recusa.message} Nada foi testado.`;\n      return;\n",
    para: "",
  },
  // A cópia volta a ser a de antes: o JSON, e uma frase que não diz que falhou.
  "copia-diz": {
    arquivo: "diagnostico-rede.js",
    de: "    const texto = resumo.textContent;\n    if (!texto) return;\n    copia.textContent = \"\"; delete copia.dataset.tipo;\n    try {\n      await navigator.clipboard.writeText(texto);\n      copia.textContent = \"Copiado: o texto do quadro acima, inteiro e só ele. Cole na conversa com quem hospeda.\";\n      // Inteira na tela: num celular o botão fica no fim da janela, e a frase nasceria abaixo dela.\n      copia.scrollIntoView({ block: \"nearest\" });\n    } catch {\n      copia.dataset.tipo = \"erro\";\n      copia.textContent = \"Não foi copiado: o navegador recusou a área de transferência. O texto do quadro acima ficou selecionado: copie com Ctrl+C, ou com o toque longo.\";\n",
    para: "    try { await navigator.clipboard.writeText(resultado.textContent); estado.textContent = \"Diagnóstico copiado.\"; return; }\n    catch { estado.textContent = \"Selecione e copie o diagnóstico abaixo.\";\n",
  },
  // O resumo copia o JSON inteiro em vez do texto curto.
  "texto-curto": {
    arquivo: "diagnostico-rede.js",
    de: "resumo.textContent = resumoDoDiagnostico(relatorio, erroVisto);",
    para: "resumo.textContent = JSON.stringify(relatorio, null, 2);",
  },
};

// O aperto leva o carimbo da sessão que caiu, e a ponte o recusa.
REVERSOES["sem-carimbo"] = {
  arquivo: "base.js",
  de: '  "abrir_o_teste_de_rede",\n',
  para: "",
};

// A página de diagnóstico volta a ter 720 px fixos, e rola de lado no celular.
REVERSOES["cabe-no-celular"] = {
  arquivo: "diagnostico-rede.css",
  tipo: "text/css; charset=utf-8",
  de: "  grid-template-columns: minmax(0, 720px);\n  justify-content: center;\n",
  para: "",
  mais: [["  width: 100%;\n  min-width: 0;\n  padding: 40px;", "  width: 720px;\n  max-width: 100%;\n  padding: 40px;"]],
};

if (reverter && !Object.hasOwn(REVERSOES, reverter)) {
  console.error(`reversão desconhecida: ${reverter}. Há: ${Object.keys(REVERSOES).join(", ")}`);
  process.exit(2);
}

/**
 * **A página de diagnóstico vai com os cabeçalhos que a origem dá a ela**
 * (`web/_headers`, a regra própria de `/diagnostico-rede.html`), e não com a
 * política do desktop que o `servir()` põe em tudo: é com a CSP dela que a
 * página tem de carregar, medir e copiar. Vale para as abas que o contexto
 * abre também.
 */
async function servirODiagnosticoComoAOrigem(contexto) {
  const daOrigem = cabecalhosDaOrigem("/diagnostico-rede.html");
  assert.match(daOrigem["content-security-policy"] ?? "", /connect-src 'self' https:\/\/rendzvous\./,
    "a regra de /diagnostico-rede.html saiu de web/_headers: a bancada serviria a página com a política errada");
  await contexto.route(/\/diagnostico-rede\.html(\?.*)?$/, async (rota) => {
    const resposta = await rota.fetch();
    await rota.fulfill({ response: resposta, headers: { ...resposta.headers(), ...daOrigem } });
  });
}

/** Serve o arquivo sem o conserto, a toda página do contexto (as abas que ele abre também). */
async function aplicarAReversao(contexto) {
  await servirODiagnosticoComoAOrigem(contexto);
  if (!reverter) return;
  const { arquivo, de, para, mais = [], tipo = "text/javascript; charset=utf-8" } = REVERSOES[reverter];
  let fonte = fs.readFileSync(path.join(ui, arquivo), "utf8");
  for (const [antes, depois] of [[de, para], ...mais]) {
    assert.ok(fonte.includes(antes), `a reversão «${reverter}» não casa com nada em ${arquivo}: ela não reverte o que diz`);
    fonte = fonte.replace(antes, depois);
  }
  await contexto.route(`**/${arquivo}`, (rota) => rota.fulfill({ contentType: tipo, body: fonte }));
}

let capturasFeitas = 0;
async function capturar(pagina, nome) {
  if (!capturas) return;
  fs.mkdirSync(capturas, { recursive: true });
  // A entrada abre com uma transição de opacidade: a captura espera ela acabar.
  await pagina.waitForTimeout(1200);
  await pagina.screenshot({ path: path.join(capturas, `${nome}.png`), fullPage: false });
  capturasFeitas += 1;
}

/**
 * À vista de verdade: desenhado (`checkVisibility`, com a opacidade), com
 * caixa, e dentro da janela. Espera um pouco por isso: a entrada abre com uma
 * transição de opacidade, e durante ela nada está «à vista».
 */
async function aVista(pagina, seletor) {
  return pagina.waitForFunction((s) => {
    const nodo = document.querySelector(s);
    if (!nodo || !nodo.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
    const caixa = nodo.getBoundingClientRect();
    // Meio pixel de folga: `scrollIntoView` encosta a caixa na borda, e a borda cai em fração.
    return caixa.width > 0 && caixa.height > 0 && caixa.top >= -0.5 && caixa.left >= -0.5 &&
      caixa.bottom <= innerHeight + 0.5 && caixa.right <= innerWidth + 0.5;
  }, seletor, { timeout: 3000 }).then(() => true, () => false);
}

/** A caixa de um nó e a janela, para a frase de uma reprovação. */
async function caixaDe(pagina, seletor) {
  return pagina.evaluate((s) => {
    const c = document.querySelector(s)?.getBoundingClientRect();
    return c ? `caixa ${Math.round(c.left)},${Math.round(c.top)} a ${Math.round(c.right)},${Math.round(c.bottom)} numa janela de ${innerWidth}×${innerHeight}` : "não está no documento";
  }, seletor);
}

/** Espera `condicao` na página por até 3 s, e responde se ela chegou: quem pergunta escreve a frase da reprovação. */
async function chegou(pagina, condicao, argumento) {
  return pagina.waitForFunction(condicao, argumento, { timeout: 3000 }).then(() => true, () => false);
}

/** Desenhado, mesmo que seja preciso rolar até ele. */
async function desenhado(pagina, seletor) {
  return pagina.evaluate((s) => {
    const nodo = document.querySelector(s);
    return Boolean(nodo && nodo.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }));
  }, seletor);
}

const IMPRESSAO = "ab".repeat(32);
const ENLACE_FALHOU = (motivo) => ({ FalhaDoEnlace: { motivo, detalhe: { ice: "failed", conexao: "failed" } } });

// ------------------------------------------------------------------ A. no app

async function noApp(navegador, url) {
  const contexto = await navegador.newContext({ viewport: { width: 1280, height: 860 } });
  await aplicarAReversao(contexto);
  const pagina = await contexto.newPage();
  const erros = [];
  pagina.on("pageerror", (erro) => erros.push(String(erro)));
  await pagina.addInitScript((tabela) => {
    window.chamadas = [];
    window.falhaDoConnect = "NotConnected";
    window.falhaAoAbrirOTeste = null;
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          window.chamadas.push({ cmd, args });
          if (cmd === "connect") throw window.falhaDoConnect;
          if (cmd === "abrir_o_teste_de_rede" && window.falhaAoAbrirOTeste) throw window.falhaAoAbrirOTeste;
          return Object.hasOwn(tabela, cmd) ? tabela[cmd] : null;
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
  await pagina.goto(url);
  await pagina.waitForFunction(() => typeof conectar === "function" && typeof mostrarFim === "function");
  const inicio = pagina.url();

  const tentar = (falha, ligacao = null) => pagina.evaluate(async ([f, l]) => {
    window.falhaDoConnect = f;
    await conectar("203.0.113.7:8383", "ana", "7K4MSEGREDO", null, l);
  }, [falha, ligacao]);
  const pedidos = () => pagina.evaluate(() => window.chamadas.filter((c) => c.cmd === "abrir_o_teste_de_rede").map((c) => c.args));
  const botao = "#boot-teste-de-conexao .teste-de-conexao-abrir";
  const estado = "#boot-teste-de-conexao .teste-de-conexao-estado";

  // 1. A falha de rede oferece o caminho, à vista, debaixo da frase dela.
  await tentar({ error: "NatNaoAbriu", trail: [] });
  const frase = await pagina.evaluate(() => $("boot-erro").textContent);
  assert.match(frase, /^O CAMINHO DIRETO ATÉ QUEM HOSPEDA NÃO ABRIU/, `a entrada diz: ${frase}`);
  assert.ok(await aVista(pagina, botao), "A1: a falha de rede da entrada não oferece TESTAR A CONEXÃO à vista");
  assert.equal(await pagina.locator(botao).textContent(), "TESTAR A CONEXÃO");
  assert.match(await pagina.locator("#boot-teste-de-conexao .teste-de-conexao-nota").textContent(), /Abre no navegador/,
    "A1: a nota não diz o que o botão abre");
  await capturar(pagina, "1280-app-entrada-nat-nao-abriu");

  // 2. O aperto pede o comando com o nome da falha, e só ele; a janela fica.
  await pagina.locator(botao).click();
  assert.deepEqual(await pedidos(), [{ erro: "NatNaoAbriu" }],
    "A2: o aperto não pediu `abrir_o_teste_de_rede` só com o nome da falha");
  assert.doesNotMatch(JSON.stringify(await pedidos()), /203\.0\.113|8383|ana|7K4M/, "A2: o pedido leva o endereço, o apelido ou o convite");
  assert.equal(pagina.url(), inicio, "A2: o aperto navegou esta janela");
  await chegou(pagina, (s) => document.querySelector(s)?.textContent !== "", estado);
  assert.equal(await pagina.locator(estado).textContent(), "O teste abriu no navegador.", "A2: o teste abriu, e a tela não diz");

  // 3. Sem navegador, a tela diz o endereço, em vermelho.
  const endereco = `${origemDoProduto}/diagnostico-rede.html?erro=NatNaoAbriu`;
  await pagina.evaluate((e) => { window.falhaAoAbrirOTeste = { NavegadorNaoAbriu: { endereco: e } }; }, endereco);
  await pagina.locator(botao).click();
  await chegou(pagina, (s) => /NÃO DEU/.test(document.querySelector(s)?.textContent ?? ""), estado);
  const naoAbriu = await pagina.locator(estado).textContent();
  assert.match(naoAbriu, /^NÃO DEU PARA ABRIR O NAVEGADOR\./, `A3: o teste não abriu, e a tela diz «${naoAbriu}»`);
  assert.ok(naoAbriu.includes(endereco.replace("https://", "")), `A3: a tela não diz o endereço: ${naoAbriu}`);
  assert.equal(await pagina.locator(estado).getAttribute("data-tipo"), "erro", "A3: a falha não está marcada como falha");
  assert.ok(await aVista(pagina, estado), "A3: a frase da falha não está à vista");
  await capturar(pagina, "1280-app-entrada-teste-nao-abriu");
  await pagina.evaluate(() => { window.falhaAoAbrirOTeste = null; });

  // 4. O que não é de rede não oferece.
  for (const falha of [
    { Refused: { reason: "Banned" } },
    { Refused: { reason: "CredentialRejected" } },
    "LinhaLegada",
    "Incompatible",
    "HandshakeTimeout",
    "UnresolvableHost",
    ENLACE_FALHOU("VerificacaoDaConexaoSegura"),
    ENLACE_FALHOU("SemCaminho"),
  ]) {
    await tentar({ error: falha, trail: [] });
    assert.equal(await pagina.evaluate(() => $("boot-erro").hidden), false, `A4: ${JSON.stringify(falha)} não chegou à linha da entrada`);
    assert.equal(await desenhado(pagina, botao), false, `A4: ${JSON.stringify(falha)} não é de rede, e a entrada oferece o teste`);
    assert.equal(await pagina.evaluate(() => $("boot-teste-de-conexao").hidden), true);
  }
  // E as outras de rede oferecem, com o nome delas.
  for (const [falha, nome] of [["SemResposta", "SemResposta"], ["Unreachable", "Unreachable"], [ENLACE_FALHOU("SemRefletido"), "SemRefletido"]]) {
    await tentar({ error: falha, trail: [] });
    assert.equal(await pagina.locator("#boot-teste-de-conexao .teste-de-conexao").getAttribute("data-falha"), nome,
      `A4: ${nome} é de rede e não oferece o teste`);
  }

  // 5. A oferta some quando a frase dela some: outro escritor, e a tentativa nova.
  await tentar({ error: "NatNaoAbriu", trail: [] });
  assert.ok(await desenhado(pagina, botao));
  await pagina.evaluate(() => { $("boot-erro").textContent = "ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU."; });
  await pagina.waitForTimeout(50);
  assert.equal(await desenhado(pagina, botao), false, "A5: a frase da entrada mudou, e o TESTAR A CONEXÃO continuou debaixo dela");
  await tentar({ error: "NatNaoAbriu", trail: [] });
  assert.ok(await desenhado(pagina, botao));
  await pagina.evaluate(() => { $("boot-erro").hidden = true; });
  await pagina.waitForTimeout(50);
  assert.equal(await desenhado(pagina, botao), false, "A5: a frase da entrada sumiu, e o TESTAR A CONEXÃO ficou");

  // 6. Uma ligação pelo código não oferece: o teste não tem o que medir.
  await tentar({ error: "NatNaoAbriu", trail: [] }, { nome: "bia" });
  assert.equal(await pagina.evaluate(() => $("boot-erro").hidden), false);
  assert.equal(await desenhado(pagina, botao), false, "A6: uma ligação pelo código oferece um teste que pede o convite de um servidor");

  // 7. A tela de fim: o enlace perdido oferece, a expulsão não.
  const doFim = "#fim-teste-de-conexao .teste-de-conexao-abrir";
  await pagina.evaluate(() => mostrarFim("LinkLost"));
  assert.equal(await pagina.evaluate(() => $("fim-motivo").textContent), "ENLACE PERDIDO");
  assert.ok(await aVista(pagina, doFim), "A7: o enlace perdido não oferece TESTAR A CONEXÃO na tela de fim");
  const antes = (await pedidos()).length;
  await pagina.locator(doFim).click();
  assert.deepEqual((await pedidos()).slice(antes), [{ erro: "LinkLost" }], "A7: o aperto da tela de fim não pediu o teste com `LinkLost`");
  await chegou(pagina, () => document.querySelector("#fim-teste-de-conexao .teste-de-conexao-estado")?.textContent !== "");
  await capturar(pagina, "1280-app-fim-enlace-perdido");
  await pagina.evaluate(() => mostrarFim("Timeout"));
  assert.ok(await desenhado(pagina, doFim), "A7: o `Timeout` do fim não oferece");
  for (const motivo of ["Kicked", "Banned", "ServerShuttingDown", "ModsMudaram"]) {
    await pagina.evaluate((m) => mostrarFim(m), motivo);
    assert.equal(await desenhado(pagina, doFim), false, `A7: «${motivo}» não é de rede, e a tela de fim oferece o teste`);
  }

  assert.deepEqual(erros, [], `no app, a página lançou: ${erros.join(" | ")}`);
  await contexto.close();
}

// ------------------------------------------------- B. na página, casca larga

/** Uma identidade de verdade, com o transporte que falha com `falha`. */
const IDENTIDADE_QUE_FALHA = `async (falha) => {
  Object.defineProperty(navigator, "storage", { value: { persist: async () => false }, configurable: true });
  const identidade = criarIdentidadeDoNavegador({
    analisar: () => ({ link: { alvo: "192.0.2.1:8383", impressao: "${IMPRESSAO}", versao: "2.0" } }),
    conectar: async () => { throw falha; },
  });
  return identidade;
}`;

async function naPaginaLarga(navegador, url) {
  const contexto = await navegador.newContext({ viewport: { width: 1280, height: 860 } });
  await aplicarAReversao(contexto);
  const pagina = await contexto.newPage();
  const erros = [];
  pagina.on("pageerror", (erro) => erros.push(String(erro)));
  await pagina.addInitScript(() => localStorage.setItem("seele.casca", "larga"));
  await pagina.goto(url);
  await pagina.waitForFunction(() => typeof conectar === "function" && typeof criarIdentidadeDoNavegador === "function"
    && document.documentElement.dataset.casca === "larga");
  const inicio = pagina.url();
  // O `connect` da casca larga na página (`entrarPeloEndereco`, na ponte): a
  // identidade tem a tentativa aberta para este endereço, e o transporte falha.
  await pagina.evaluate(`(async () => {
    const identidade = await (${IDENTIDADE_QUE_FALHA})(${JSON.stringify(ENLACE_FALHOU("CaminhoNaoAbriu"))});
    await identidade.abrir("link da bancada");
    window.identidadeDaPagina = async () => identidade;
    await conectar("192.0.2.1:8383", "ana");
  })()`);
  const frase = await pagina.evaluate(() => ($("boot-erro").hidden ? null : $("boot-erro").textContent));
  assert.match(String(frase), /^AS DUAS REDES NÃO SE ALCANÇARAM\./,
    `B: a falha do enlace chegou à entrada da página como «${frase}»`);
  const botao = "#boot-teste-de-conexao .teste-de-conexao-abrir";
  assert.ok(await aVista(pagina, botao), "B: a falha do enlace não oferece TESTAR A CONEXÃO na casca larga da página");
  assert.match(await pagina.locator("#boot-teste-de-conexao .teste-de-conexao-nota").textContent(), /Abre numa aba nova/);
  await capturar(pagina, "1280-pagina-entrada-caminho-nao-abriu");

  // O aperto abre a página de diagnóstico, numa aba da mesma origem.
  const [aba] = await Promise.all([contexto.waitForEvent("page"), pagina.locator(botao).click()]);
  await aba.waitForLoadState("domcontentloaded");
  const destino = new URL(aba.url());
  assert.equal(destino.origin, new URL(url).origin, "B: o teste abriu noutra origem");
  assert.equal(destino.pathname, "/diagnostico-rede.html");
  assert.equal(destino.search, "?erro=CaminhoNaoAbriu", `B: a aba abriu em ${aba.url()}`);
  assert.equal(destino.hash, "", "B: a aba levou um fragmento");
  assert.ok(await chegou(aba, () => document.getElementById("visto") && !document.getElementById("visto").hidden),
    "B: a página de diagnóstico não diz de que falha a pessoa veio");
  assert.match(await aba.locator("#visto").textContent(), /CaminhoNaoAbriu/, "B: a página de diagnóstico não diz de que falha a pessoa veio");
  assert.equal(await aba.locator("h1").textContent(), "Testar a conexão com um servidor");
  assert.equal(await aba.evaluate(() => window.opener), null, "B: a aba do teste ficou com o `opener` da página do SEELE");
  assert.equal(pagina.url(), inicio, "B: o aperto navegou a página do SEELE");
  assert.equal(await pagina.locator("#boot-teste-de-conexao .teste-de-conexao-estado").textContent(), "O teste abriu numa aba nova.");
  await aba.close();

  assert.deepEqual(erros, [], `na casca larga, a página lançou: ${erros.join(" | ")}`);
  await contexto.close();
}

// ---------------------------------------------- C. na página, casca do celular

async function noCelular(navegador, url) {
  const contexto = await navegador.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  await aplicarAReversao(contexto);

  /** Abre a entrada do celular com uma identidade cujo transporte falha com `falha`, e toca ENTRAR. */
  async function entrarEFalhar(falha) {
    const pagina = await contexto.newPage();
    pagina.on("pageerror", (erro) => { throw new Error(`no celular, a página lançou: ${erro}`); });
    await pagina.addInitScript(() => localStorage.setItem("seele.casca", "celular"));
    await pagina.goto(url);
    await pagina.waitForFunction(() => typeof CELULAR !== "undefined" && CELULAR.ajudante("entrada"));
    await pagina.evaluate(`(async () => {
      const identidade = await (${IDENTIDADE_QUE_FALHA})(${JSON.stringify(falha)});
      await CELULAR.ajudante("entrada").usar(identidade, { link: "link da bancada" });
    })()`);
    await pagina.locator("#cel-entrada-apelido").fill("ana");
    await pagina.locator(".cel-entrada-pe button").filter({ hasText: /^ENTRAR$/ }).click();
    await pagina.locator('.cel-entrada-aviso[data-tipo="erro"]').waitFor();
    return pagina;
  }
  const botao = ".cel-entrada-aviso .teste-de-conexao-abrir";
  const estado = ".cel-entrada-aviso .teste-de-conexao-estado";

  // A falta de rede oferece, dentro do aviso, com alvo de toque.
  const pagina = await entrarEFalhar(ENLACE_FALHOU("SinalMudo"));
  assert.equal(await pagina.locator(".cel-entrada-aviso-titulo").first().textContent(), "O SERVIDOR NÃO ABRIU A CONEXÃO");
  assert.ok(await desenhado(pagina, botao), "C: o `sem-rede` da entrada do celular não oferece TESTAR A CONEXÃO");
  await pagina.locator(botao).scrollIntoViewIfNeeded();
  assert.ok(await aVista(pagina, botao), "C: o TESTAR A CONEXÃO do celular não fica à vista");
  const alvo = await pagina.locator(botao).boundingBox();
  assert.ok(alvo.height >= 44 && alvo.width >= 44, `C: o alvo de toque tem ${alvo.width}×${alvo.height}`);
  const lado = await pagina.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  assert.ok(lado <= 0, `C: a entrada do celular rola ${lado}px de lado com a oferta`);
  await capturar(pagina, "390-celular-entrada-sem-rede");

  // O aperto leva ao diagnóstico, com a falha.
  const [aba] = await Promise.all([contexto.waitForEvent("page"), pagina.locator(botao).click()]);
  await aba.waitForLoadState("domcontentloaded");
  const destino = new URL(aba.url());
  assert.equal(`${destino.pathname}${destino.search}`, "/diagnostico-rede.html?erro=SinalMudo", `C: a aba abriu em ${aba.url()}`);
  assert.ok(await chegou(aba, () => document.getElementById("visto") && !document.getElementById("visto").hidden),
    "C: a página de diagnóstico não diz de que falha a pessoa veio");
  assert.match(await aba.locator("#visto").textContent(), /SinalMudo/);
  await capturar(aba, "390-diagnostico-chegada");
  await aba.close();
  assert.equal(await pagina.locator(estado).textContent(), "O teste abriu numa aba nova.");

  // A aba que o navegador bloqueia é dita, com o endereço.
  await pagina.evaluate(() => { window.open = () => null; });
  await pagina.locator(botao).click();
  await chegou(pagina, (s) => /NÃO DEIXOU/.test(document.querySelector(s)?.textContent ?? ""), estado);
  const bloqueada = await pagina.locator(estado).textContent();
  assert.match(bloqueada, /^O NAVEGADOR NÃO DEIXOU ABRIR A ABA\./, `C: a aba bloqueada, e a tela diz «${bloqueada}»`);
  assert.ok(bloqueada.includes(`${new URL(url).host}/diagnostico-rede.html?erro=SinalMudo`), `C: a tela não diz o endereço: ${bloqueada}`);
  assert.equal(await pagina.locator(estado).getAttribute("data-tipo"), "erro");
  await pagina.locator(estado).scrollIntoViewIfNeeded();
  assert.ok(await aVista(pagina, estado), "C: a frase da aba bloqueada não fica à vista");
  await capturar(pagina, "390-celular-entrada-aba-bloqueada");
  await pagina.close();

  // O que não é de rede não oferece.
  for (const falha of [{ Refused: { reason: "AdmissionDenied" } }, { Refused: { reason: "CredentialRejected" } }, ENLACE_FALHOU("VerificacaoDaConexaoSegura"), "HandshakeTimeout"]) {
    const outra = await entrarEFalhar(falha);
    assert.equal(await desenhado(outra, botao), false, `C: ${JSON.stringify(falha)} não é falta de rede que o teste meça, e o celular o oferece`);
    await outra.close();
  }

  // A tela de fim, no celular: a mesma caixa, à vista em 390.
  const fim = await contexto.newPage();
  await fim.addInitScript(() => localStorage.setItem("seele.casca", "celular"));
  await fim.goto(url);
  await fim.waitForFunction(() => typeof mostrarFim === "function" && typeof CELULAR !== "undefined");
  await fim.evaluate(() => mostrarFim("LinkLost"));
  const doFim = "#fim-teste-de-conexao .teste-de-conexao-abrir";
  assert.ok(await desenhado(fim, doFim), "C: a tela de fim do celular não oferece o teste para o enlace perdido");
  await fim.locator(doFim).scrollIntoViewIfNeeded();
  assert.ok(await aVista(fim, doFim), "C: o TESTAR A CONEXÃO da tela de fim não fica à vista no celular");
  const ladoDoFim = await fim.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  assert.ok(ladoDoFim <= 0, `C: a tela de fim do celular rola ${ladoDoFim}px de lado com a oferta`);
  await capturar(fim, "390-celular-fim-enlace-perdido");
  // **Com a vista ainda numa sessão que a ponte já não tem** (é o estado de
  // quem caiu): o aperto não leva o carimbo dela, ou a ponte o recusaria como
  // `NotConnected` antes de abrir aba nenhuma, e o teste falharia justo aqui.
  await fim.evaluate(() => { vistaDaJanela.sessao = 4242; vistaDaJanela.geracao = 1; });
  const abriuDoFim = contexto.waitForEvent("page", { timeout: 3000 }).then((aba) => aba, () => null);
  await fim.locator(doFim).click();
  const abaDoFim = await abriuDoFim;
  const ditoNoFim = await fim.locator("#fim-teste-de-conexao .teste-de-conexao-estado").textContent();
  assert.ok(abaDoFim, `C: na tela de fim, com a sessão caída ainda na vista, o teste não abriu, e a tela diz «${ditoNoFim}»`);
  assert.equal(new URL(abaDoFim.url()).search, "?erro=LinkLost");
  await abaDoFim.close();
  await fim.close();

  await contexto.close();
}

// ------------------------------------------------ D. a página de diagnóstico

/** O que o enlace de mentira «mediu»: endereços e portas de verdade, que não podem sair. */
const TRANSPORTE_QUE_NAO_ABRE = () => {
  const estatisticas = new Map([
    ["local-secreto", { type: "local-candidate", candidateType: "host", address: "2001:db8::42", port: 54321 }],
    ["local-publico", { type: "local-candidate", candidateType: "srflx", address: "198.51.100.23", port: 40001 }],
    ["remoto-secreto", { type: "remote-candidate", candidateType: "srflx", address: "203.0.113.7", port: 8383 }],
    ["par-1", { type: "candidate-pair", localCandidateId: "local-publico", remoteCandidateId: "remoto-secreto", state: "failed", nominated: false, requestsSent: 7, responsesReceived: 0 }],
    ["par-2", { type: "candidate-pair", localCandidateId: "local-secreto", remoteCandidateId: "remoto-secreto", state: "in-progress", nominated: false, requestsSent: 21 }],
  ]);
  window.pedidosDaPagina = 0;
  window.fetch = async () => { window.pedidosDaPagina += 1; return { arrayBuffer: async () => new ArrayBuffer(0) }; };
  WebAssembly.instantiate = async () => ({ instance: { exports: {} } });
  window.prepararCertificadoDtls = async () => ({});
  window.fronteiraDoSinal = () => ({});
  window.criarEnlaceRtc = ({ aoMudar }) => {
    aoMudar({ Discando: { candidatos: 2 } });
    aoMudar({ ChamandoPeloPonto: { ponto: "ponto.exemplo:8384" } });
    let falhar;
    const ready = new Promise((_, rejeitar) => { falhar = rejeitar; });
    // Depois da primeira medida, para o relatório ter caminho.
    setTimeout(() => falhar({ FalhaDoEnlace: { motivo: "CaminhoNaoAbriu", detalhe: { ice: "failed", conexao: "failed" } } }), 120);
    return {
      ready,
      visto: { refletido: true, sinal: { aqui: {} }, dtls: null, chamadasAoPonto: 2, candidatosAnunciados: 3, candidatos: ["203.0.113.7:8383", "[2001:db8::7]:8383"], antesDeFechar: { ice: "failed", conexao: "failed" } },
      contas: { candidatosRecusados: 0 },
      close() {},
      // O resumo é o de verdade (`resumirCaminhoRtc`, do `rtc-web.js`): é ele que tira endereço e porta.
      medirCaminho: async () => ({ estado: { ice: "failed", conexao: "failed" }, ...resumirCaminhoRtc(estatisticas) }),
    };
  };
};

/** O que nenhum texto copiado pode levar, do convite e da medida desta bancada. */
const NAO_PODE_SAIR = /192\.0\.2|203\.0\.113|198\.51\.100|2001:db8|8383|8384|54321|40001|abab|7K4M|rendzvous|ponto\.exemplo|https?:\/\/|seele:\/\//;

async function aPaginaDeDiagnostico(navegador, url, largura, altura) {
  const contexto = await navegador.newContext({ viewport: { width: largura, height: altura }, isMobile: largura < 600, hasTouch: largura < 600 });
  await contexto.grantPermissions(["clipboard-read", "clipboard-write"]);
  await aplicarAReversao(contexto);
  const pagina = await contexto.newPage();
  const erros = [];
  pagina.on("pageerror", (erro) => erros.push(String(erro)));
  const pedidos = [];
  pagina.on("request", (pedido) => pedidos.push(pedido.url()));
  // O que a política da página recusar aparece no console, e reprova.
  pagina.on("console", (mensagem) => {
    if (/Content Security Policy|Refused to/i.test(mensagem.text())) erros.push(mensagem.text().slice(0, 240));
  });
  const resposta = await pagina.goto(`${url}diagnostico-rede.html?erro=NatNaoAbriu`);
  assert.equal(resposta.headers()["content-security-policy"], cabecalhosDaOrigem("/diagnostico-rede.html")["content-security-policy"],
    `D(${largura}): a página de diagnóstico não veio com a política da origem dela`);
  await pagina.waitForFunction(() => typeof resumoDoDiagnostico === "function");
  const origem = new URL(url).origin;
  const L = `D(${largura})`;

  // Cabe na largura: medido antes de tudo, porque numa página mais larga que
  // a tela nem o toque acerta o botão.
  const medidas = () => pagina.evaluate(() => ({ pagina: document.documentElement.scrollWidth, painel: Math.round(document.querySelector("main").getBoundingClientRect().width) }));
  const vazia = await medidas();
  assert.ok(vazia.pagina <= largura, `${L}: a página de diagnóstico tem ${vazia.pagina}px de largura numa tela de ${largura}px`);
  assert.ok(vazia.painel <= 720, `${L}: o painel tem ${vazia.painel}px, e o desenho dele é de 720`);

  // A falha de quem chegou é dita antes de qualquer teste.
  assert.ok(await aVista(pagina, "#visto"), `${L}: a página não diz de que falha a pessoa veio`);
  assert.match(await pagina.locator("#visto").textContent(), /NatNaoAbriu.*vai junto no diagnóstico copiado/);
  assert.equal(await pagina.locator("#copiar").isDisabled(), true, `${L}: copiar está aceso sem medida nenhuma`);

  // Um convite que não serve é dito, e nada é testado.
  await pagina.evaluate(TRANSPORTE_QUE_NAO_ABRE);
  await pagina.locator("#convite").fill("seele://192.0.2.1:8383?fp=" + IMPRESSAO);
  await pagina.locator("#testar").click();
  const recusa = await pagina.locator("#estado").textContent();
  assert.match(recusa, /não traz a impressão do servidor ou o ponto de encontro.* Nada foi testado\.$/, `${L}: o convite sem ponto, e a página diz «${recusa}»`);
  assert.equal(await pagina.locator("#resumo").textContent(), "", `${L}: há um resumo de um teste que não houve`);
  assert.equal(await pagina.locator("#copiar").isDisabled(), true, `${L}: a página oferece copiar o diagnóstico de um teste que não houve`);
  assert.equal(await pagina.locator("#convite").getAttribute("aria-invalid"), "true");
  await capturar(pagina, `${largura}-diagnostico-convite-que-nao-serve`);

  // A medida (com o convite do app, o `seele://`), e o quadro com o que o botão copia.
  await pagina.locator("#convite").fill(`seele://192.0.2.1:8383?alt=203.0.113.7:8383&enc=rendzvous.seele.sh:8384/203.0.113.1:8383&fp=${IMPRESSAO}&convite=7K4MSEGREDO`);
  await pagina.locator("#testar").click();
  await pagina.waitForFunction(() => !document.getElementById("copiar").disabled, null, { timeout: 5000 });
  assert.equal(await pagina.locator("#estado").textContent(), "A conexão não abriu. O diagnóstico está abaixo, pronto para copiar.");
  const naTela = await pagina.locator("#resumo").textContent();
  assert.ok(await desenhado(pagina, "#resumo"), `${L}: o texto que o botão copia não está na tela`);
  for (const parte of [
    "falha vista no SEELE: NatNaoAbriu",
    "resultado: não conectou (CaminhoNaoAbriu)",
    "etapas: Discando",
    "ponto de encontro: aqui · chamadas 2 · endereço refletido sim",
    "endereços: enviados 3 · do servidor 2 · recusados 0",
    "estado: ice failed · conexão failed · dtls não medido",
    "candidatos deste lado: host_ipv6 1, srflx_ipv4 1",
    "candidatos do servidor: srflx_ipv4 1",
    // O segundo par não deu respostas: a soma é a do que foi medido, e não um zero a mais.
    "pares: 2 (failed 1, in-progress 1) · pedidos enviados 28 · respostas recebidas 0",
  ]) {
    assert.ok(naTela.includes(parte), `${L}: falta «${parte}» no texto a copiar:\n${naTela}`);
  }
  assert.ok(naTela.split("\n").length <= 14, `${L}: o texto a copiar tem ${naTela.split("\n").length} linhas`);
  assert.doesNotMatch(naTela, NAO_PODE_SAIR, `${L}: o texto a copiar leva o que identifica:\n${naTela}`);
  // O relatório completo também não leva.
  const completo = await pagina.locator("#resultado").textContent();
  assert.doesNotMatch(completo, NAO_PODE_SAIR, `${L}: o resultado completo leva o que identifica`);
  assert.equal(JSON.parse(completo).erroVisto, "NatNaoAbriu");

  // Copiar: o que chega à área de transferência é o quadro, e a página diz.
  await pagina.locator("#copiar").scrollIntoViewIfNeeded();
  await pagina.locator("#copiar").click();
  await chegou(pagina, () => document.getElementById("copia").textContent !== "" || /copiado|copie/i.test(document.getElementById("estado").textContent));
  const copiado = await pagina.evaluate(() => navigator.clipboard.readText());
  assert.equal(copiado, naTela, `${L}: o que foi copiado não é o que a tela mostra`);
  assert.doesNotMatch(copiado, NAO_PODE_SAIR);
  const dito = await pagina.locator("#copia").textContent();
  assert.match(dito, /^Copiado: o texto do quadro acima, inteiro e só ele\./, `${L}: copiou, e a página diz «${dito}»`);
  assert.ok(await aVista(pagina, "#copia"), `${L}: a frase da cópia não está à vista`);
  await capturar(pagina, `${largura}-diagnostico-copiado`);

  // A cópia que o navegador recusa é dita, e o texto fica selecionado.
  await pagina.evaluate(() => { navigator.clipboard.writeText = async () => { throw new DOMException("recusado", "NotAllowedError"); }; });
  await pagina.locator("#copiar").click();
  await chegou(pagina, () => document.getElementById("copia").textContent.startsWith("Não foi copiado"));
  const falhou = await pagina.locator("#copia").textContent();
  assert.match(falhou, /^Não foi copiado: o navegador recusou a área de transferência\./, `${L}: a cópia falhou, e a página diz «${falhou}»`);
  assert.equal(await pagina.locator("#copia").getAttribute("data-tipo"), "erro");
  assert.equal(await pagina.evaluate(() => getSelection().toString()), naTela, `${L}: o texto não ficou selecionado para a cópia à mão`);
  assert.ok(await aVista(pagina, "#copia"), `${L}: a frase da cópia que falhou não está à vista: ${await caixaDe(pagina, "#copia")}`);
  await capturar(pagina, `${largura}-diagnostico-copia-recusada`);

  // Com o resultado na tela, continua cabendo; e a página não mandou o diagnóstico a lugar nenhum.
  const cheia = await medidas();
  assert.ok(cheia.pagina <= largura, `${L}: com o resultado, a página de diagnóstico tem ${cheia.pagina}px de largura numa tela de ${largura}px`);
  assert.ok(cheia.painel <= 720, `${L}: com o resultado, o painel tem ${cheia.painel}px, e o desenho dele é de 720`);
  const fora = pedidos.filter((p) => !p.startsWith(origem));
  assert.deepEqual(fora, [], `${L}: a página pediu algo fora da origem dela: ${fora.join(", ")}`);
  assert.deepEqual(erros, [], `${L}: a página lançou: ${erros.join(" | ")}`);
  await contexto.close();
}

// ------------------------------------------------------------------ a corrida

(async () => {
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;
  const navegador = await chromium.launch({ headless: true });
  try {
    const partes = [
      ["A. no app", () => noApp(navegador, url)],
      ["B. na página, casca larga", () => naPaginaLarga(navegador, url)],
      ["C. na página, casca do celular", () => noCelular(navegador, url)],
      ["D. a página de diagnóstico, 1280", () => aPaginaDeDiagnostico(navegador, url, 1280, 860)],
      ["D. a página de diagnóstico, 390", () => aPaginaDeDiagnostico(navegador, url, 390, 844)],
    ];
    for (const [nome, parte] of partes) {
      await parte();
      console.log(`  ${nome}: ok`);
    }
    console.log(`teste de conexão: ok${capturas ? ` (${capturasFeitas} capturas em ${capturas})` : ""}`);
  } finally {
    await navegador.close();
    servidor.close();
  }
})().catch((erro) => {
  console.error(reverter ? `REPROVOU com «${reverter}» revertido:` : "REPROVOU:", erro?.message ?? erro);
  process.exit(1);
});
